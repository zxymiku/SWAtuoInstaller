//! 7z 解压程序的定位与变体检测（只读，不下载）。
//!
//! 查找顺序：
//!
//! 1. `[sevenzip].exe_path` 显式指定
//! 2. 随程序打包的资源目录
//! 3. 应用数据目录下的下载缓存
//! 4. 已安装的官方 7-Zip（注册表 + 常见安装目录）
//! 5. **第三方兼容实现**：NanaZip / 7-Zip ZS / PeaZip / Bandizip
//! 6. exe 同级目录
//! 7. `PATH`
//!
//! 关于 NanaZip：它通过 **AppExecutionAlias** 暴露 `NanaZipC.exe`，
//! 那是一个 **0 字节的重解析点**（reparse point）但可执行 ——
//! 所以可用性判定必须同时接受「非空文件」和「符号链接/重解析点」，
//! 只看文件长度会把 NanaZip 误判为不可用。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::Config;
use crate::win32_utils;

/// 7z 可执行文件的来源。
///
/// `Copy` 是必需的：`label()` 按值取 `self`，若没有 `Copy`，
/// 调用处会把 `SevenZipResolution` 部分移动，后续再用就编译不过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SevenZipSource {
    /// `[sevenzip].exe_path` 显式指定
    Configured,
    /// 随程序打包的资源目录
    Bundled,
    /// 自动下载后缓存在应用数据目录
    Downloaded,
    /// 已安装的 7-Zip（注册表 / 官方安装目录）
    Installed,
    /// 第三方兼容实现（NanaZip 等）
    Variant,
    /// 系统 `PATH`
    Path,
}

impl SevenZipSource {
    pub fn label(self) -> &'static str {
        match self {
            SevenZipSource::Configured => "配置指定",
            SevenZipSource::Bundled => "随程序打包",
            SevenZipSource::Downloaded => "自动下载缓存",
            SevenZipSource::Installed => "已安装的 7-Zip",
            SevenZipSource::Variant => "第三方兼容实现",
            SevenZipSource::Path => "系统 PATH",
        }
    }
}

/// 7z 解析结果。
#[derive(Debug, Clone)]
pub struct SevenZipResolution {
    pub program: PathBuf,
    pub source: SevenZipSource,
    /// 人类可读的实现名，例如 `7-Zip` / `NanaZip`。
    pub impl_name: String,
}

/// 自动下载缓存中 7z 的文件名。
pub const CACHED_SEVEN_ZIP_NAME: &str = "7zr.exe";

/// 所有已知的 7z 命令行可执行文件名。
///
/// 第三方实现普遍沿用 7-Zip 的 CLI 约定（`x` / `t` / `l` / `-o` / `-aoa` / `-bb1`），
/// 因此只要有其中之一就能直接当 7z 用：
///
/// | 名称 | 出处 |
/// |---|---|
/// | `7z.exe` | 7-Zip 官方安装版 |
/// | `7zr.exe` | 7-Zip 官方独立控制台版（自动下载用的就是它）|
/// | `7za.exe` | 7-Zip 独立版 / p7zip |
/// | `7zz.exe` | 7-Zip 官方 Linux 版与部分第三方 Windows 构建 |
/// | `NanaZipC.exe` | NanaZip 控制台版（AppExecutionAlias）|
/// | `NanaZip.exe` | NanaZip 单文件版 |
/// | `nanazipc.exe` | 大小写变体（大小写不敏感匹配，这里列出来是为了可读性）|
/// | `7zG.exe` / `7zFM.exe` | 官方 GUI，**仅供识别提示**，不作为命令行候选 |
pub const SEVEN_ZIP_EXE_NAMES: &[&str] = &[
    "7z.exe",
    "7zr.exe",
    "7za.exe",
    "7zz.exe",
    "NanaZipC.exe",
    "NanaZip.exe",
];

/// 第三方实现的安装目录（相对 Program Files 的常见落点）。
///
/// 只作为**候选目录**，目录里必须真的存在可执行文件才会被采用。
pub const VARIANT_INSTALL_DIRS: &[(&str, &str)] = &[
    // (相对路径, 实现名)
    (r"7-Zip", "7-Zip"),
    (r"7-Zip-Zstandard", "7-Zip ZS"),
    (r"NanaZip", "NanaZip"),
    (r"NanaZip Preview", "NanaZip"),
    (r"PeaZip", "PeaZip"),
    (r"Bandizip", "Bandizip"),
];

/// 判断一个候选路径是否真的可用。
///
/// **注意**：不能只检查 `len() > 0`。Windows 的 `AppExecutionAlias`
/// （例如 `%LOCALAPPDATA%\Microsoft\WindowsApps\NanaZipC.exe`）是
/// **0 字节的重解析点**，但 `CreateProcess` 能正常执行它。
/// 因此这里接受「非空文件」或「重解析点」。
pub fn usable_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    if metadata.len() > 0 {
        return true;
    }
    // 0 字节：只有当它是**重解析点**时才认可。
    is_reparse_point(path)
}

/// `FILE_ATTRIBUTE_REPARSE_POINT`：重解析点（符号链接 / 挂载点 / AppExecutionAlias）。
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// 判断路径是否为重解析点。
///
/// **为什么不能只用 `file_type().is_symlink()`**：
/// Windows 的 `AppExecutionAlias`（例如 `%LOCALAPPDATA%\Microsoft\WindowsApps\NanaZipC.exe`）
/// 属性位里有 `FILE_ATTRIBUTE_REPARSE_POINT`，**但不是 `IO_REPARSE_TAG_SYMLINK`**，
/// 所以 `is_symlink()` 返回 `false`。Rust 的 `std` 没有暴露通用的重解析点判定，
/// 因此这里直接读文件属性位。
#[cfg(windows)]
pub fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(path)
        // `file_attributes()` 来自 MetadataExt，取出 WIN32_FILE_ATTRIBUTE_DATA 的属性位。
        .map(|meta| meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn is_reparse_point(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

/// 目录里是否存在某个名字的可执行文件（大小写不敏感）。
pub fn find_exe_in_dir_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if usable_executable(&direct) {
        return Some(direct);
    }
    // 直接拼不出来时，列出目录做一次大小写不敏感比较
    // （Windows 通常不区分大小写，但 AppExecutionAlias 与跨盘复制会有例外）。
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if let Some(found) = entry.path().file_name() {
            if found.to_string_lossy().eq_ignore_ascii_case(name) {
                let candidate = entry.path();
                if usable_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// 按实现名猜测目录里可执行文件的“品牌名”（仅用于日志展示）。
pub fn implementation_label(program: &Path) -> String {
    let name = program
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name.contains("nanazip") {
        "NanaZip".to_string()
    } else if name.starts_with("7z") {
        "7-Zip".to_string()
    } else {
        program
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "未知".to_string())
    }
}

/// 收集“已安装的实现”候选目录。
pub fn installed_candidate_dirs() -> Vec<(PathBuf, SevenZipSource, String)> {
    let mut dirs: Vec<(PathBuf, SevenZipSource, String)> = Vec::new();

    // 1) 注册表：官方 7-Zip 安装版会写 Path64 / Path
    for key in [r"SOFTWARE\7-Zip", r"SOFTWARE\WOW6432Node\7-Zip"] {
        for value in ["Path64", "Path"] {
            if let Some(dir) =
                win32_utils::read_registry_string(win32_utils::HKEY_LOCAL_MACHINE, key, value)
            {
                let dir = dir.trim().trim_matches('"').to_string();
                if !dir.is_empty() {
                    dirs.push((PathBuf::from(dir), SevenZipSource::Installed, "7-Zip".into()));
                }
            }
        }
    }

    // 2) 注册表：第三方实现的卸载信息里常带 InstallLocation
    for (sub, location) in registry_uninstall_locations() {
        let lower = sub.to_ascii_lowercase();
        let label = if lower.contains("nanazip") {
            "NanaZip"
        } else if lower.contains("7-zip") || lower.contains("7zip") {
            "7-Zip"
        } else if lower.contains("peazip") {
            "PeaZip"
        } else if lower.contains("bandizip") {
            "Bandizip"
        } else {
            continue;
        };
        if !location.trim().is_empty() {
            dirs.push((
                PathBuf::from(location.trim().trim_matches('"')),
                if label == "7-Zip" {
                    SevenZipSource::Installed
                } else {
                    SevenZipSource::Variant
                },
                label.to_string(),
            ));
        }
    }

    // 3) 常见安装目录（覆盖没有写注册表 / 便携版的情况）
    for base in [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|p| format!("{p}\\Programs")),
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(|p| format!("{p}\\Microsoft\\WindowsApps")),
    ]
    .into_iter()
    .flatten()
    {
        for (relative, label) in VARIANT_INSTALL_DIRS {
            dirs.push((
                PathBuf::from(&base).join(relative),
                if *label == "7-Zip" {
                    SevenZipSource::Installed
                } else {
                    SevenZipSource::Variant
                },
                (*label).to_string(),
            ));
        }
    }

    dirs
}

/// 从注册表的卸载项里抽取 `(子键名, 安装目录)`，用于按产品名找安装位置。
pub fn registry_uninstall_locations() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for base in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        found.extend(win32_utils::registry_subkeys_with_value(
            win32_utils::HKEY_LOCAL_MACHINE,
            base,
            "InstallLocation",
        ));
    }
    found
}

/// 在目录里找一个可用的 7z 可执行文件（含第三方实现）。
pub fn executable_in_dir(dir: &Path) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    SEVEN_ZIP_EXE_NAMES
        .iter()
        .find_map(|name| find_exe_in_dir_ci(dir, name))
}

/// 解析 7z 可执行文件的**位置**（不触发下载）。
///
/// 查找顺序：
///   1. `[sevenzip].exe_path`
///   2. 随程序打包的资源目录（`resources\`）
///   3. 应用数据目录下的自动下载缓存
///   4. 已安装的 7-Zip（注册表 / 官方安装目录）
///   5. 第三方兼容实现（NanaZip / 7-Zip ZS / PeaZip / Bandizip 等）
///   6. 可执行文件同级目录
///   7. 系统 `PATH`
///
/// 全部失败时回退为 `PathBuf::from("7z")`，是否真的可用由
/// [`seven_zip_available`] 判定。
pub fn resolve_seven_zip(
    config: &Config,
    resource_dir: Option<&Path>,
    app_data_dir: Option<&Path>,
) -> SevenZipResolution {
    // 1. 配置显式指定
    let configured = config.sevenzip.exe_path.trim();
    if !configured.is_empty() {
        let candidate = PathBuf::from(configured);
        if usable_executable(&candidate) {
            return SevenZipResolution {
                impl_name: implementation_label(&candidate),
                program: candidate,
                source: SevenZipSource::Configured,
            };
        }
    }

    // 2. 打包资源
    let mut bundled: Vec<PathBuf> = Vec::new();
    if let Some(dir) = resource_dir {
        bundled.push(dir.to_path_buf());
        bundled.push(dir.join("resources"));
        bundled.push(dir.join("_up_").join("resources"));
    }
    // 开发期直接命中源码目录
    bundled.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            bundled.push(dir.to_path_buf());
            bundled.push(dir.join("resources"));
        }
    }
    for dir in &bundled {
        if let Some(found) = executable_in_dir(dir) {
            return SevenZipResolution {
                impl_name: implementation_label(&found),
                program: found,
                source: SevenZipSource::Bundled,
            };
        }
    }

    // 3. 自动下载缓存
    if let Some(dir) = app_data_dir {
        if let Some(found) = executable_in_dir(dir) {
            return SevenZipResolution {
                impl_name: implementation_label(&found),
                program: found,
                source: SevenZipSource::Downloaded,
            };
        }
    }

    // 4 & 5. 已安装的实现（官方 7-Zip 优先，其次第三方）
    let dirs = installed_candidate_dirs();
    for source in [SevenZipSource::Installed, SevenZipSource::Variant] {
        for (dir, dir_source, label) in &dirs {
            if *dir_source != source {
                continue;
            }
            if let Some(found) = executable_in_dir(dir) {
                return SevenZipResolution {
                    impl_name: if label.is_empty() {
                        implementation_label(&found)
                    } else {
                        label.clone()
                    },
                    program: found,
                    source,
                };
            }
        }
    }

    // 6. 系统 PATH
    if let Some(found) = which_on_path_any(SEVEN_ZIP_EXE_NAMES) {
        return SevenZipResolution {
            impl_name: implementation_label(&found),
            program: found,
            source: SevenZipSource::Path,
        };
    }

    // 7. 交给 CreateProcess 自己解析裸名
    SevenZipResolution {
        impl_name: "7-Zip（未确认）".to_string(),
        program: PathBuf::from("7z"),
        source: SevenZipSource::Path,
    }
}

/// 在 `PATH` 中按顺序查找一组可执行文件名。
pub fn which_on_path_any(names: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            if let Some(found) = find_exe_in_dir_ci(&dir, name) {
                return Some(found);
            }
        }
    }
    None
}

/// 在 `PATH` 中查找可执行文件。
pub fn which_on_path(program: &Path) -> Option<PathBuf> {
    let name = program.to_string_lossy();
    // 带目录分隔符时按路径直接判断。
    if name.contains('\\') || name.contains('/') {
        return usable_executable(program).then(|| program.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    let candidate_names: Vec<String> = if Path::new(&*name).extension().is_some() {
        vec![name.to_string()]
    } else {
        // 裸名 `7z` 要补上 Windows 的可执行后缀。
        vec![format!("{name}.exe"), name.to_string()]
    };

    for dir in std::env::split_paths(&path_var) {
        for candidate_name in &candidate_names {
            if let Some(found) = find_exe_in_dir_ci(&dir, candidate_name) {
                return Some(found);
            }
        }
    }
    None
}
