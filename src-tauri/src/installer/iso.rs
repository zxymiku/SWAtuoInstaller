//! 第 7 步：定位并挂载 ISO，定位安装入口。
//!
//! 挂载本身由 [`super::guards::IsoMountGuard`] 负责（含 Drop 兜底卸载）。
//!
use std::path::{Path, PathBuf};


use super::context::InstallContext;
use super::guards::IsoMountGuard;
use super::paths::find_files_matching;

pub fn locate_iso(folder1: &Path) -> Result<PathBuf, String> {
    // 扩展名不区分大小写（.iso / .ISO / .Iso 都接受），且不限层级。
    let candidates = find_files_matching(folder1, "*.iso");

    candidates.into_iter().next().ok_or_else(|| {
        format!(
            "在 {} 中未找到 .iso 安装镜像（查找不区分大小写）",
            folder1.display()
        )
    })
}

/// 在已挂载的镜像里找到安装介质根目录。
///
/// 返回 `(介质根目录, 盘符字母)`。
///
/// # 介质根目录长什么样
///
/// **SolidWorks 2024 SP5 Premium.DVD 的真实布局**（已核对）：
///
/// ```text
/// <根>\
///   setup.exe                  ← 引导程序（GUI）
///   sldim\startswinstall.exe   ← 命令行安装入口
///   swwi\data\solidworks.msi   ← 主程序 MSI
///   swwi\lang\<语言>\<语言>.msi
/// ```
///
/// 因此识别特征用 `sldim\` 与 `swwi\`，而不是早期版本才有的
/// `StartSWInstall.exe` / `64bit\`。
pub fn locate_setup(
    ctx: &InstallContext,
    iso_guard: Option<&mut IsoMountGuard>,
) -> Result<(PathBuf, String), String> {
    /// 判断某个盘符根目录是否是 SolidWorks 安装介质。
    ///
    /// 覆盖三种布局：DVD/ISO（`sldim` + `swwi`）、
    /// 管理映像（`64bit`）、早期版本（根目录 `StartSWInstall.exe`）。
    fn looks_like_media(root: &Path) -> bool {
        root.join("setup.exe").is_file()
            || root.join("sldim").join("startswinstall.exe").is_file()
            || root.join("swwi").is_dir()
            || root.join("64bit").is_dir()
            || root.join("StartSWInstall.exe").is_file()
    }

    let mut roots: Vec<(PathBuf, String)> = Vec::new();

    if let Some(guard) = iso_guard.as_ref() {
        if let Some(letter) = guard.drive() {
            roots.push((PathBuf::from(format!("{letter}:\\")), letter.to_string()));
        }
    }

    // 挂载盘符解析失败时，枚举所有可移动盘查找特征文件。
    if roots.is_empty() {
        for letter in b'D'..=b'Z' {
            let letter = (letter as char).to_string();
            let root = PathBuf::from(format!("{letter}:\\"));
            if looks_like_media(&root) {
                roots.push((root, letter));
            }
        }
    }

    if roots.is_empty() {
        return Err("未能定位已挂载的安装介质盘符".to_string());
    }

    for (root, letter) in &roots {
        // 介质**根目录**本身就是安装根（`sldim\`、`swwi\` 都在它下面），
        // 所以这里优先返回根目录，而不是入口 exe 所在的子目录。
        // 例：`sldim\startswinstall.exe` 的父目录是 `sldim\`，
        // 但安装器要在同级找 `swwi\`，因此必须回到根。
        //
        // 用 `startswinstall.exe`（命令行入口）优先做识别标记，
        // `setup.exe` 只是图形化引导程序，放最后仅作"确认这是安装介质"之用 ——
        // **它不会被用来执行安装**（见 `install_ops::launch_installer`）。
        for candidate in [
            root.join("sldim").join("startswinstall.exe"),
            root.join("StartSWInstall.exe"),
            root.join("setup.exe"),
            root.join("64bit").join("setup.exe"),
        ] {
            if candidate.is_file() {
                let dir = if candidate == root.join("64bit").join("setup.exe") {
                    // 管理映像布局：`64bit\` 才是安装根
                    candidate
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| root.clone())
                } else {
                    root.clone()
                };
                ctx.info(format!(
                    "安装介质根目录已确认：{}（识别依据 {}）",
                    dir.display(),
                    candidate.display()
                ));
                return Ok((dir, letter.clone()));
            }
        }
    }

    // 最后的兜底：只要 64bit 目录存在就交给调用方。
    for (root, letter) in roots {
        if root.join("64bit").is_dir() {
            ctx.warn("未找到 setup.exe / StartSWInstall.exe，回退到 64bit 目录");
            return Ok((root.join("64bit"), letter));
        }
    }

    Err("安装介质中未找到 StartSWInstall.exe / setup.exe / 64bit 目录".to_string())
}
