//! 应用状态、进度上报器与通用工具。
//!
//! `Reporter` 是后端向前端推送 `InstallEvent` 的唯一出口：
//! 生产环境包裹 `tauri::ipc::Channel<InstallEvent>`；模块结构不依赖 Tauri 的
//! 具体运行时，便于在安装编排中自由传递（`Reporter: Clone + Send + Sync`）。

pub mod commands;
pub mod config;
pub mod downloader;
pub mod events;
pub mod installer;
pub mod antivirus;
pub mod process_utils;
pub mod win32_utils;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Local;

use crate::config::Config;
use crate::events::{ErrorKind, InstallEvent, InstallStatus, LogLevel, StepId};
use crate::installer::InstallStatusCell;

/// 从应用启动开始持续写入的会话日志。
#[derive(Clone)]
pub struct AppLogger {
    file: Arc<Mutex<File>>,
    pub path: PathBuf,
}

impl AppLogger {
    pub fn open(app_data_dir: &Path) -> std::io::Result<Self> {
        let dir = app_data_dir.join("logs");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("session-{}.log", Local::now().format("%Y%m%d-%H%M%S")));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { file: Arc::new(Mutex::new(file)), path })
    }

    pub fn write(&self, level: LogLevel, message: &str) {
        let timestamp = now_timestamp();
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "[{timestamp}] [{}] {message}", level.as_str().to_uppercase());
            let _ = file.flush();
        }
    }
}

/// 前端通过 `Channel<InstallEvent>` 接收的消息发送器。
#[derive(Clone)]
pub struct Reporter {
    send: Arc<dyn Fn(InstallEvent) + Send + Sync>,
    logger: Option<AppLogger>,
    current_step: Arc<Mutex<Option<StepId>>>,
}

impl Reporter {
    /// 从任意可调用对象构造（生产环境传 Channel，测试无需注入）。
    pub fn new<F>(send: F) -> Self
    where
        F: Fn(InstallEvent) + Send + Sync + 'static,
    {
        Self {
            send: Arc::new(send),
            logger: None,
            current_step: Arc::new(Mutex::new(None)),
        }
    }

    /// 创建同时写入前端事件和磁盘文件的安装日志出口。
    pub fn with_logger<F>(send: F, logger: AppLogger) -> Self
    where
        F: Fn(InstallEvent) + Send + Sync + 'static,
    {
        Self {
            send: Arc::new(send),
            logger: Some(logger),
            current_step: Arc::new(Mutex::new(None)),
        }
    }

    /// 直接投递一个事件。
    pub fn emit(&self, event: InstallEvent) {
        (self.send)(event);
    }

    /// 一行日志。
    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        let message = self.contextual_message(message.into());
        let timestamp = now_timestamp();
        self.write_log_line(&timestamp, level, &message);
        self.emit(InstallEvent::LogMessage {
            level,
            message,
            timestamp,
        });
    }

    /// 进入某个逻辑步骤（自动推导下一个步骤的英文名与说明）。
    pub fn step_changed(&self, step: StepId) {
        if let Ok(mut current) = self.current_step.lock() {
            *current = Some(step);
        }
        let timestamp = now_timestamp();
        self.write_log_line(
            &timestamp,
            LogLevel::Info,
            &format!("进入步骤 {:02}/{}：{}", step.display_index(), StepId::ALL.len(), step.zh_label()),
        );
        self.emit(InstallEvent::StepChanged {
            step: step.display_index(),
            total: StepId::ALL.len(),
            id: step,
            name: step.zh_label().to_string(),
            en_name: step.en_label().to_string(),
            note: step.note().to_string(),
        });
    }

    /// 通用进度。
    pub fn progress(&self, percent: f64, message: impl Into<String>) {
        self.emit(InstallEvent::InstallProgress {
            percent: percent.clamp(0.0, 100.0),
            message: message.into(),
        });
    }

    /// 下载进度。
    pub fn download_progress(
        &self,
        bytes_done: u64,
        bytes_total: u64,
        speed_bps: u64,
        chunks_done: usize,
        chunks_total: usize,
    ) {
        self.emit(InstallEvent::DownloadProgress {
            bytes_done,
            bytes_total,
            speed_bps,
            chunks_done,
            chunks_total,
        });
    }

    /// 单个文件的下载状态（多文件 / 分片下载时逐文件上报）。
    #[allow(clippy::too_many_arguments)]
    pub fn download_file_status(
        &self,
        index: usize,
        total: usize,
        name: &str,
        bytes_done: u64,
        bytes_total: u64,
        speed_bps: u64,
        phase: &str,
        detail: &str,
    ) {
        self.emit(InstallEvent::DownloadFileStatus {
            index,
            total,
            name: name.to_string(),
            bytes_done,
            bytes_total,
            speed_bps,
            phase: phase.to_string(),
            detail: detail.to_string(),
        });
    }
    /// 解压进度。
    pub fn extract_progress(&self, percent: f64, current_file: &str, pass: usize) {        self.emit(InstallEvent::ExtractProgress {
            percent,
            current_file: current_file.to_string(),
            pass,
        });
    }

    /// 环境检测结果。
    pub fn environment_check(
        &self,
        username: &str,
        computer_name: &str,
        username_ascii: bool,
        computer_ascii: bool,
        elevated: bool,
        free_space_gb: f64,
    ) {
        self.emit(InstallEvent::EnvironmentCheck {
            username: username.to_string(),
            computer_name: computer_name.to_string(),
            username_ascii,
            computer_ascii,
            elevated,
            free_space_gb,
        });
    }

    /// 弹窗处理记录。
    pub fn popup(&self, title: &str, action: &str) {
        self.emit(InstallEvent::PopupDetected {
            title: title.to_string(),
            action: action.to_string(),
        });
    }

    /// 安全软件处置报告。
    pub fn antivirus_report(&self, outcome: &crate::antivirus::AntivirusOutcome) {
        self.emit(InstallEvent::AntivirusReport {
            detected: outcome.detected.clone(),
            outcomes: outcome.outcomes.clone(),
            manual_required: outcome.manual_required.clone(),
            defender_removed: outcome.defender_removed,
            reboot_required: outcome.reboot_recommended,
            summary: outcome.summary.clone(),
        });
    }

    /// 错误事件。
    pub fn error(&self, kind: ErrorKind, message: &str, recoverable: bool) {
        let message = self.contextual_message(message.to_string());
        self.write_log_line(&now_timestamp(), LogLevel::Error, &format!("[{kind:?}] {message}"));
        self.emit(InstallEvent::Error {
            kind,
            message,
            recoverable,
        });
    }

    /// 剩余时间估算。
    pub fn estimate(&self, seconds: u64) {
        self.emit(InstallEvent::EstimatedTimeRemaining { seconds });
    }

    /// 流程结束。
    pub fn completed(&self, success: bool, message: &str) {
        self.write_log_line(
            &now_timestamp(),
            if success { LogLevel::Success } else { LogLevel::Error },
            message,
        );
        self.emit(InstallEvent::Completed {
            success,
            message: message.to_string(),
        });
    }

    fn contextual_message(&self, message: String) -> String {
        let step = self.current_step.lock().ok().and_then(|guard| *guard);
        match step {
            Some(step) => format!("[步骤 {:02}/{} {}] {message}", step.display_index(), StepId::ALL.len(), step.zh_label()),
            None => message,
        }
    }

    fn write_log_line(&self, timestamp: &str, level: LogLevel, message: &str) {
        if let Some(logger) = &self.logger {
            // Reporter 已经生成时间戳，避免一条日志出现两个不同时间。
            if let Ok(mut file) = logger.file.lock() {
                let _ = writeln!(file, "[{timestamp}] [{}] {message}", level.as_str().to_uppercase());
                let _ = file.flush();
            }
        }
    }
}

/// 应用级共享状态（由 Tauri 的 `manage` 注册）。
pub struct AppState {
    /// `{app_data_dir}`
    pub app_data_dir: PathBuf,
    /// 用户配置文件路径 `{app_data_dir}/config.toml`
    pub config_path: PathBuf,
    /// 最近一次加载/保存的配置。
    pub config: Mutex<Config>,
    /// 安装状态快照。
    pub status: Arc<InstallStatusCell>,
    /// 取消标记。
    pub cancel: Arc<AtomicBool>,
    /// 本地模式下选定的压缩包。
    pub local_archive: Mutex<Option<PathBuf>>,
    /// 是否有安装任务在运行。
    pub install_running: Arc<AtomicBool>,
    pub logger: AppLogger,
}

impl AppState {
    pub fn new(app_data_dir: PathBuf, logger: AppLogger) -> Self {
        let config_path = config::user_config_path(&app_data_dir);
        let config = config::load_effective_config(&app_data_dir).unwrap_or_default();
        Self {
            app_data_dir,
            config_path,
            config: Mutex::new(config),
            status: Arc::new(InstallStatusCell::default()),
            cancel: Arc::new(AtomicBool::new(false)),
            local_archive: Mutex::new(None),
            install_running: Arc::new(AtomicBool::new(false)),
            logger,
        }
    }

    /// 读取当前生效的配置副本。
    pub fn snapshot(&self) -> Config {
        self.config
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// 写回内存中的配置。
    pub fn store(&self, config: Config) {
        if let Ok(mut guard) = self.config.lock() {
            *guard = config;
        }
    }

    /// 生成 `get_install_status` 的返回值。
    pub fn install_status(&self) -> InstallStatus {
        let current_step = self.status.current_step.load(Ordering::SeqCst) as usize;
        let finished = self.status.finished.load(Ordering::SeqCst);
        let success = self.status.success.load(Ordering::SeqCst);

        // 步骤条着色：已完成的逻辑步骤下标集合。
        let completed_steps: Vec<usize> = if finished && success {
            (0..StepId::ALL.len()).collect()
        } else {
            (0..current_step).collect()
        };

        let eta = self.status.eta_seconds.load(Ordering::SeqCst);

        InstallStatus {
            running: self.status.running.load(Ordering::SeqCst),
            cancelled: self.status.cancelled.load(Ordering::SeqCst),
            finished,
            success,
            current_step: current_step + 1,
            total_steps: StepId::ALL.len(),
            step_id: StepId::ALL.get(current_step).copied(),
            percent: self.status.percent(),
            message: self.status.message(),
            started_at: self.status.started_at.lock().ok().and_then(|g| g.clone()),
            finished_at: self.status.finished_at.lock().ok().and_then(|g| g.clone()),
            popups_handled: self.status.popups_handled.load(Ordering::SeqCst),
            network_disabled: self.status.network_disabled.load(Ordering::SeqCst),
            iso_mounted: self.status.iso_mounted.load(Ordering::SeqCst),
            completed_steps,
            eta_seconds: if eta == 0 { None } else { Some(eta) },
            antivirus_manual_required: self
                .status
                .antivirus_manual_required
                .load(Ordering::SeqCst),
            antivirus_defender_removed: self
                .status
                .antivirus_defender_removed
                .load(Ordering::SeqCst),
            reboot_required: self.status.reboot_required.load(Ordering::SeqCst),
        }
    }
}

/// 本地时间戳（`YYYY-MM-DD HH:MM:SS.mmm`）。
pub fn now_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// 在 Tauri 的 Tokio 运行时上阻塞等待一个 future。
///
/// 安装编排运行在独立 OS 线程中（大量同步 Win32 调用），
/// 这条路径让下载引擎复用 Tauri 的 async runtime，而不额外创建运行时。
pub fn async_runtime_block_on<F>(future: F) -> F::Output
where
    F: std::future::Future,
{
    tauri::async_runtime::block_on(future)
}

/// 把 Windows 控制台字节流（可能是 GBK / ANSI）解码成 `String`。
///
/// install 日志与 7z / sc / reg 的输出都可能是本地代码页，
/// 直接用 `from_utf8_lossy` 会把中文变成替换字符。
pub fn decode_console_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    // 纯 ASCII 直接走快路径。
    if bytes.is_ascii() {
        return String::from_utf8_lossy(bytes).to_string();
    }
    // 已是合法 UTF-8 时优先按 UTF-8 解码。
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    #[cfg(windows)]
    {
        if let Some(text) = decode_with_code_page(bytes, 936) {
            return text;
        }
        // 0 = 系统 ANSI 代码页（非中文系统上的兜底）。
        if let Some(text) = decode_with_code_page(bytes, 0) {
            return text;
        }
    }

    String::from_utf8_lossy(bytes).to_string()
}

/// 用 `MultiByteToWideChar` 按指定代码页解码。
#[cfg(windows)]
fn decode_with_code_page(
    bytes: &[u8],
    code_page: u32,
) -> Option<String> {
    use windows::Win32::Globalization::{
        MultiByteToWideChar, MB_ERR_INVALID_CHARS, MULTI_BYTE_TO_WIDE_CHAR_FLAGS,
    };

    unsafe {
        let strict = MULTI_BYTE_TO_WIDE_CHAR_FLAGS(MB_ERR_INVALID_CHARS.0);
        let needed = MultiByteToWideChar(code_page, strict, bytes, None);
        if needed <= 0 {
            // 严格模式失败（存在非法字节）→ 用宽松模式解一次。
            let len = MultiByteToWideChar(code_page, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
            if len <= 0 {
                return None;
            }
            let mut buffer = vec![0u16; len as usize];
            let written =
                MultiByteToWideChar(code_page, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut buffer));
            if written <= 0 {
                return None;
            }
            return Some(String::from_utf16_lossy(&buffer[..written as usize]));
        }

        let mut buffer = vec![0u16; needed as usize];
        let written = MultiByteToWideChar(code_page, strict, bytes, Some(&mut buffer));
        if written <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..written as usize]))
    }
}

/// 默认配置文件文本（供前端“查看默认值”使用）。
pub fn default_config_text() -> &'static str {
    config::DEFAULT_CONFIG_TOML
}

/// 判断路径是否带有「已知压缩包」的扩展名。
///
/// **仅用于展示与缓存命名，不用于准入判定**：真正能否解压由 `7z t` 与
/// `7z x` 按文件头决定（见 `installer::sniff_archive_kind` 的说明）。
pub fn looks_like_archive(path: &Path) -> bool {
    const KNOWN: &[&str] = &[
        "7z", "zip", "rar", "iso", "gz", "tgz", "bz2", "xz", "zst", "cab", "001", "exe",
    ];
    path.extension()
        .map(|ext| {
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            KNOWN.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tauri 应用入口
// ---------------------------------------------------------------------------

use tauri::Manager;

/// 标准 Tauri 入口：由 `main.rs` 调用。
///
/// 逻辑集中在 `lib` 中，`main.rs` 只负责调用本函数——这样
/// `cargo check` 可以不链接 Tauri 的运行时初始化代码。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));

            // 日志必须先于配置读取建立，这样启动阶段的配置错误也能进入同一会话文件。
            if let Err(error) = std::fs::create_dir_all(&app_data_dir) {
                eprintln!("创建应用数据目录失败: {error}");
            }
            let logger = AppLogger::open(&app_data_dir)
                .map_err(|error| std::io::Error::other(format!("创建启动日志失败: {error}")))?;
            logger.write(LogLevel::Info, &format!("应用启动；日志文件：{}", logger.path.display()));

            // 首次启动时把默认配置落盘，并记录用户配置路径。
            match config::load_effective_config(&app_data_dir) {
                Ok(_) => {
                    let path = config::user_config_path(&app_data_dir);
                    println!("配置已加载 · {}", path.display());
                    logger.write(LogLevel::Info, &format!("配置已加载：{}", path.display()));
                }
                Err(error) => {
                    eprintln!("配置加载失败，使用内置默认值: {error}");
                    logger.write(LogLevel::Error, &format!("配置加载失败，将使用内置默认值：{error}"));
                }
            }
            logger.write(LogLevel::Info, "已加载应用配置，等待用户操作");
            app.manage(AppState::new(app_data_dir, logger));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::log_ui_event,
            commands::set_local_archive,
            commands::save_config,
            commands::reset_config,
            commands::fetch_remote_config,
            commands::get_default_config,
            commands::env_check,
            commands::pick_archive,
            commands::pick_archive_folder,
            commands::start_install,
            commands::cancel_install,
            commands::get_install_status,
            commands::export_logs,
            commands::export_config,
            commands::recheck_antivirus,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
