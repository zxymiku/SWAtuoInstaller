//! 7z 可用性判定、状态查询与自动下载。
//!
//! 全部查找失败时，可按 `[sevenzip].auto_download` 从 `download_url` 拉取官方
//! `7zr.exe` 并缓存到应用数据目录。下载后会检查 `MZ` 文件头，
//! 避免把一个 HTML 错误页当成可执行文件缓存下来。

use std::path::{Path, PathBuf};
use std::io::{Read, Write};

use crate::config::{Config, SevenZipConfig};
use crate::events::{LogLevel, SevenZipStatus};

use super::lookup::{
    executable_in_dir, implementation_label, resolve_seven_zip, usable_executable,
    which_on_path, SevenZipResolution, SevenZipSource, CACHED_SEVEN_ZIP_NAME,
};

pub fn seven_zip_available(resolution: &SevenZipResolution) -> bool {
    match resolution.source {
        SevenZipSource::Path => {
            if resolution.program.components().count() > 1 {
                usable_executable(&resolution.program)
            } else {
                which_on_path(&resolution.program).is_some()
            }
        }
        _ => usable_executable(&resolution.program),
    }
}

/// 汇总 7z 的可用性状态（只解析位置，不触发下载）。
///
/// 设置页的「运行环境实测」与第 1 步的环境快照共用这一份逻辑。
pub fn seven_zip_status(
    config: &Config,
    resource_dir: Option<&Path>,
    app_data_dir: &Path,
) -> SevenZipStatus {
    let resolution = resolve_seven_zip(config, resource_dir, Some(app_data_dir));
    SevenZipStatus {
        program: resolution.program.display().to_string(),
        source: resolution.source.label().to_string(),
        available: seven_zip_available(&resolution),
        auto_download: config.sevenzip.auto_download,
        download_url: config.sevenzip.download_url.clone(),
    }
}

/// 自动下载 7z 到应用数据目录。
///
/// 使用 reqwest 的**阻塞**客户端：本函数由安装编排在工作线程中调用，
/// 不持有 Tokio 运行时句柄，起一个临时运行时反而更简单可靠。
///
/// 返回下载后的完整路径。
pub fn download_seven_zip(
    config: &Config,
    app_data_dir: &Path,
    emit: &dyn Fn(LogLevel, String),
) -> Result<PathBuf, String> {
    let url = config.sevenzip.download_url.trim();
    if url.is_empty() {
        return Err("未配置 7z 下载地址（[sevenzip].download_url）".to_string());
    }
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("7z 下载 URL 无效: {e}"))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err("7z 自动下载只允许使用带主机名的 HTTPS URL".to_string());
    }

    let target = app_data_dir.join(CACHED_SEVEN_ZIP_NAME);
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("创建目录失败 {}: {e}", app_data_dir.display()))?;

    let timeout = config.seven_zip_download_timeout();
    emit(LogLevel::Info, format!("自动获取 7z：{url}"));

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(config.connect_timeout())
        .timeout(timeout)
        .user_agent(concat!(
            "SolidWorksInstaller/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("请求 7z 下载地址失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "7z 下载地址返回 HTTP {}（{url}）",
            response.status()
        ));
    }

    const MAX_SEVEN_ZIP_BYTES: u64 = 32 * 1024 * 1024;
    if let Some(length) = response.content_length() {
        if length == 0 || length > MAX_SEVEN_ZIP_BYTES {
            return Err(format!("7z 下载大小异常（{length} 字节，允许范围 1..={MAX_SEVEN_ZIP_BYTES}）"));
        }
    }

    // 先写临时文件再改名，避免下载中断留下半个可执行文件被误用。
    let tmp = app_data_dir.join(format!("{CACHED_SEVEN_ZIP_NAME}.part"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("创建临时文件失败 {}: {e}", tmp.display()))?;

    let mut limited = response.take(MAX_SEVEN_ZIP_BYTES + 1);
    let written = std::io::copy(&mut limited, &mut file)
        .map_err(|e| format!("写入 7z 失败: {e}"))?;

    if written == 0 || written > MAX_SEVEN_ZIP_BYTES {
        let _ = std::fs::remove_file(&tmp);
        return Err("下载到的 7z 为空文件，已放弃".to_string());
    }

    // 可执行文件必须以 MZ 开头，否则说明拿到的是 HTML 错误页之类。
    use std::io::{Seek, SeekFrom};
    file.flush()
        .map_err(|e| format!("刷新 7z 临时文件失败 {}: {e}", tmp.display()))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("定位 7z 临时文件失败 {}: {e}", tmp.display()))?;
    let mut head = [0u8; 2];
    let head_len = file
        .read(&mut head)
        .map_err(|e| format!("读取 7z 文件头失败 {}: {e}", tmp.display()))?;
    drop(file);
    if head_len < 2 || &head != b"MZ" {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "下载内容不是可执行文件（前 2 字节非法，共 {written} 字节）。\
             请检查 download_url 是否为直链；GitHub 的 releases/latest/download 是直链，\
             而 releases/latest 页面不是。"
        ));
    }

    if target.exists() {
        let _ = std::fs::remove_file(&target);
    }
    std::fs::rename(&tmp, &target)
        .map_err(|e| format!("重命名 7z 失败: {e}"))?;

    emit(
        LogLevel::Success,
        format!(
            "7z 已就绪：{}（{:.2} MB，来源：自动下载）",
            target.display(),
            written as f64 / 1024.0 / 1024.0
        ),
    );
    Ok(target)
}

/// 确保 7z 可用：解析失败且在允许时自动下载。
///
/// 这是安装编排第 4 步之前调用的入口。
pub fn ensure_seven_zip(
    config: &Config,
    app_data_dir: &Path,
    resource_dir: Option<&Path>,
    emit: &dyn Fn(LogLevel, String),
) -> Result<SevenZipResolution, String> {
    // 先看缓存里是否已经有上次下载的 7z。
    if let Some(found) = executable_in_dir(app_data_dir) {
        let resolution = SevenZipResolution {
            impl_name: implementation_label(&found),
            program: found,
            source: SevenZipSource::Downloaded,
        };
        emit(
            LogLevel::Debug,
            format!("使用缓存中的 7z：{}", resolution.program.display()),
        );
        return Ok(resolution);
    }

    let resolution = resolve_seven_zip(config, resource_dir, Some(app_data_dir));
    if seven_zip_available(&resolution) {
        emit(
            LogLevel::Info,
            format!(
                "解压程序就绪：{}（来源：{}）",
                resolution.program.display(),
                resolution.source.label()
            ),
        );
        return Ok(resolution);
    }

    if !config.sevenzip.auto_download {
        return Err(format!(
            "未找到可用的 7z 解压程序（已查找：配置路径、随程序资源、应用数据目录、\
             已安装的 7-Zip、系统 PATH），且 [sevenzip].auto_download 为 false。\n\
             请任选其一：\n\
              · 把 7z.exe 放到 {} 目录并重新打包；\n\
              · 安装 7-Zip（winget install 7zip.7zip）；\n\
              · 在 [sevenzip].exe_path 中填写完整路径；\n\
              · 把 auto_download 设为 true 让程序自动获取。",
            resource_dir
                .map(|dir| dir.display().to_string())
                .unwrap_or_else(|| "<资源目录>".to_string())
        ));
    }

    let downloaded = download_seven_zip(config, app_data_dir, emit)?;
    Ok(SevenZipResolution {
        impl_name: implementation_label(&downloaded),
        program: downloaded,
        source: SevenZipSource::Downloaded,
    })
}

/// 依据配置生成 7z 配置摘要，供日志与文档对照。
pub fn describe_sevenzip(config: &SevenZipConfig) -> String {
    format!(
        "7z: exe_path={} extract_timeout={} 分钟",
        if config.exe_path.trim().is_empty() {
            "<内置>"
        } else {
            config.exe_path.trim()
        },
        config.extract_timeout_minutes
    )
}
