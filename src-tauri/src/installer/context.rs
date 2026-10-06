//! 安装编排的运行时上下文与状态。
//!
//! `InstallContext` 是各阶段函数唯一需要携带的东西：配置、路径、取消标记、
//! 事件出口、以及供 `get_install_status` 读取的状态单元。
//!
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::events::{InstallEvent, LogLevel, StepId};
use crate::Reporter;
use crate::downloader::ProgressSink;
use crate::events::ErrorKind;


/// 安装期间的状态快照，供 `get_install_status` 与 UI 使用。
#[derive(Debug, Default)]
pub struct InstallStatusCell {
    pub running: AtomicBool,
    pub cancelled: AtomicBool,
    pub finished: AtomicBool,
    pub success: AtomicBool,
    pub current_step: AtomicU64,
    pub percent: Mutex<f64>,
    pub message: Mutex<String>,
    pub started_at: Mutex<Option<String>>,
    pub finished_at: Mutex<Option<String>>,
    pub popups_handled: AtomicU64,
    pub network_disabled: AtomicBool,
    pub iso_mounted: AtomicBool,
    pub eta_seconds: AtomicU64,
    /// 安全软件处置报告（供状态查询与 UI 恢复）。
    pub antivirus: Mutex<Option<crate::antivirus::AntivirusOutcome>>,
    /// 需要用户手动退出的安全软件数量。
    pub antivirus_manual_required: AtomicU64,
    /// 是否移除了 Windows Defender。
    pub antivirus_defender_removed: AtomicBool,
    /// 是否提示需要重启。
    pub reboot_required: AtomicBool,
    /// **语言包安装尚未结束**（已排期或正在运行）。
    ///
    /// 语言包是独立 MSI，在主程序具体 PID 退出后立即启动。
    /// 在语言包启动与结束前保持此标记，避免轮询在两个 MSI 的切换瞬间
    /// 误判安装完成，带着"没装中文"的状态继续往下走。
    ///
    /// 因此用这个标记把语言包纳入完成判定。
    pub language_pending: AtomicBool,
    /// 未满足的**强制** Windows 前置组件数量（0 表示都齐了）。
    pub prereq_missing: AtomicU64,
    /// 主程序 `msiexec` 是否已经退出。
    ///
    /// 轮询据此判断「安装器还在跑」还是「已经停了」——
    /// 停了却没有主程序就是失败，早点报错比空等 4 小时有用。
    pub installer_exited: AtomicBool,
}

impl InstallStatusCell {
    pub fn set_message(&self, message: impl Into<String>) {
        if let Ok(mut guard) = self.message.lock() {
            *guard = message.into();
        }
    }

    pub fn set_percent(&self, percent: f64) {
        if let Ok(mut guard) = self.percent.lock() {
            *guard = percent;
        }
    }

    pub fn message(&self) -> String {
        self.message
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn percent(&self) -> f64 {
        self.percent.lock().map(|guard| *guard).unwrap_or(0.0)
    }

    pub fn reset(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.cancelled.store(false, Ordering::SeqCst);
        self.finished.store(false, Ordering::SeqCst);
        self.success.store(false, Ordering::SeqCst);
        self.current_step.store(0, Ordering::SeqCst);
        self.popups_handled.store(0, Ordering::SeqCst);
        self.network_disabled.store(false, Ordering::SeqCst);
        self.iso_mounted.store(false, Ordering::SeqCst);
        self.eta_seconds.store(0, Ordering::SeqCst);
        self.antivirus_manual_required.store(0, Ordering::SeqCst);
        self.antivirus_defender_removed.store(false, Ordering::SeqCst);
        self.reboot_required.store(false, Ordering::SeqCst);
        self.language_pending.store(false, Ordering::SeqCst);
        self.prereq_missing.store(0, Ordering::SeqCst);
        self.installer_exited.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.antivirus.lock() {
            *guard = None;
        }
        self.set_percent(0.0);
        self.set_message("待命");
        if let Ok(mut guard) = self.started_at.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.finished_at.lock() {
            *guard = None;
        }
    }
}

/// 运行期闭包形式的进度接收器。
///
/// `Reporter` 是 `Fn`，因此这里用 `Mutex` 持有它以便在 `&self` 方法中使用。
pub struct ClosureSink<F: Fn(InstallEvent) + Send + Sync + 'static> {
    emit: Mutex<F>,
    cancel: Arc<AtomicBool>,
}

/// 下载器使用的 Reporter 适配器，确保下载线程的日志也落入同一份安装日志。
pub struct ReporterSink {
    reporter: Reporter,
    cancel: Arc<AtomicBool>,
}

impl ReporterSink {
    pub fn new(reporter: Reporter, cancel: Arc<AtomicBool>) -> Self {
        Self { reporter, cancel }
    }
}

impl ProgressSink for ReporterSink {
    fn log(&self, level: LogLevel, message: &str) {
        self.reporter.log(level, message);
    }

    fn download_progress(
        &self,
        bytes_done: u64,
        bytes_total: u64,
        speed_bps: u64,
        chunks_done: usize,
        chunks_total: usize,
    ) {
        self.reporter
            .download_progress(bytes_done, bytes_total, speed_bps, chunks_done, chunks_total);
    }

    fn estimate(&self, seconds: u64) {
        self.reporter.estimate(seconds);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

impl<F: Fn(InstallEvent) + Send + Sync + 'static> ClosureSink<F> {
    pub fn new(emit: F, cancel: Arc<AtomicBool>) -> Self {
        Self {
            emit: Mutex::new(emit),
            cancel,
        }
    }

    pub fn send(&self, event: InstallEvent) {
        if let Ok(guard) = self.emit.lock() {
            guard(event);
        }
    }
}

impl<F: Fn(InstallEvent) + Send + Sync + 'static> ProgressSink for ClosureSink<F> {
    fn log(&self, level: LogLevel, message: &str) {
        self.send(InstallEvent::LogMessage {
            level,
            message: message.to_string(),
            timestamp: crate::now_timestamp(),
        });
    }

    fn download_progress(
        &self,
        bytes_done: u64,
        bytes_total: u64,
        speed_bps: u64,
        chunks_done: usize,
        chunks_total: usize,
    ) {
        self.send(InstallEvent::DownloadProgress {
            bytes_done,
            bytes_total,
            speed_bps,
            chunks_done,
            chunks_total,
        });
    }

    fn estimate(&self, seconds: u64) {
        self.send(InstallEvent::EstimatedTimeRemaining { seconds });
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// 安装编排上下文。
pub struct InstallContext {
    pub config: Config,
    /// `{app_data_dir}`——配置与工作目录的根。
    pub app_data_dir: PathBuf,
    /// 本次运行的工作目录（解压与临时文件的落点）。
    pub workdir: PathBuf,
    /// 运行时解析出的 7z.exe 路径。
    pub seven_zip: PathBuf,
    /// Tauri 资源目录（打包后 7z.exe / default-config.toml 的落点）。
    pub resource_dir: Option<PathBuf>,
    /// 0 基的逻辑步骤，会随阶段推进写入 `InstallStatusCell`。
    pub status: Arc<InstallStatusCell>,
    pub cancel: Arc<AtomicBool>,
    /// 本地模式下已选定的压缩包；为空表示走下载模式。
    pub local_archive: Option<PathBuf>,
    pub reporter: Reporter,
}

impl InstallContext {
    pub fn new(
        config: Config,
        app_data_dir: PathBuf,
        status: Arc<InstallStatusCell>,
        cancel: Arc<AtomicBool>,
        reporter: Reporter,
        local_archive: Option<PathBuf>,
    ) -> Self {
        let workdir = resolve_workdir(&config, &app_data_dir);
        let seven_zip = PathBuf::new();
        Self {
            config,
            app_data_dir,
            workdir,
            seven_zip,
            resource_dir: None,
            status,
            cancel,
            local_archive,
            reporter,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn log(&self, level: LogLevel, message: impl Into<String>) {
        self.reporter.log(level, message.into());
    }

    pub fn info(&self, message: impl Into<String>) {
        self.log(LogLevel::Info, message);
    }

    pub fn debug(&self, message: impl Into<String>) {
        self.log(LogLevel::Debug, message);
    }

    pub fn warn(&self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message);
    }

    pub fn success(&self, message: impl Into<String>) {
        self.log(LogLevel::Success, message);
    }

    pub fn error(&self, message: impl Into<String>) {
        self.log(LogLevel::Error, message);
    }

    pub fn reporter(&self) -> &Reporter {
        &self.reporter
    }

    /// 进入某个逻辑步骤：推送 `StepChanged` 并同步状态单元。
    pub fn enter_step(&self, step: StepId) {
        let index = step.logical_index();
        self.status
            .current_step
            .store(index as u64, Ordering::SeqCst);
        self.status.set_message(step.zh_label());
        self.status
            .set_percent((index as f64 / StepId::ALL.len() as f64) * 100.0);
        self.reporter.step_changed(step);
    }

    /// 通用进度：同时更新状态单元与事件。
    pub fn progress(&self, percent: f64, message: impl Into<String>) {
        let message = message.into();
        self.status.set_percent(percent);
        self.status.set_message(message.clone());
        self.reporter.progress(percent, message);
    }

    pub fn fail(&self, kind: ErrorKind, message: impl Into<String>, recoverable: bool) {
        let message = message.into();
        self.error(message.clone());
        self.reporter.error(kind, &message, recoverable);
    }
}

/// 依据配置决定工作目录。
pub fn resolve_workdir(config: &Config, app_data_dir: &Path) -> PathBuf {
    let custom = config.workdir.temp_dir.trim();
    if !custom.is_empty() {
        return PathBuf::from(custom).join("solidworks-install");
    }
    app_data_dir.join("work")
}

