//! 事件与状态类型定义。
//!
//! 这些类型既作为 `tauri::ipc::Channel<InstallEvent>` 的消息体（后端 → 前端流式推送），
//! 也作为 `get_install_status` 命令的返回值。前端 `src/lib/api/types.ts`
//! 必须与本文件保持字段一致。

use serde::{Deserialize, Serialize};

/// 日志级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Success,
}

impl LogLevel {
    /// 供前端 CSS 钩子使用的稳定键。
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
            LogLevel::Success => "success",
        }
    }
}

/// 前端安装向导展示的 9 个逻辑步骤。
///
/// 后端内部执行的是 13 个物理阶段，通过 `StepId::logical_index()` 映射到这 9 步，
/// 因此步骤追踪器始终显示 01~09 且不会出现空洞。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepId {
    Environment,
    Acquire,
    Network,
    Extract,
    Registry,
    MountIso,
    Install,
    Verify,
    Finalize,
}

impl StepId {
    pub const ALL: [StepId; 9] = [
        StepId::Environment,
        StepId::Acquire,
        StepId::Network,
        StepId::Extract,
        StepId::Registry,
        StepId::MountIso,
        StepId::Install,
        StepId::Verify,
        StepId::Finalize,
    ];

    /// 0 基的逻辑步序号（0~8）。
    pub fn logical_index(self) -> usize {
        match self {
            StepId::Environment => 0,
            StepId::Acquire => 1,
            StepId::Network => 2,
            StepId::Extract => 3,
            StepId::Registry => 4,
            StepId::MountIso => 5,
            StepId::Install => 6,
            StepId::Verify => 7,
            StepId::Finalize => 8,
        }
    }

    /// 1 基的展示编号 01~09。
    pub fn display_index(self) -> usize {
        self.logical_index() + 1
    }

    pub fn zh_label(self) -> &'static str {
        match self {
            StepId::Environment => "环境检测",
            StepId::Acquire => "获取压缩包",
            StepId::Network => "禁用网络",
            StepId::Extract => "两阶段解压",
            StepId::Registry => "导入注册表",
            StepId::MountIso => "挂载镜像",
            StepId::Install => "静默安装",
            StepId::Verify => "轮询检测",
            StepId::Finalize => "收尾恢复",
        }
    }

    pub fn en_label(self) -> &'static str {
        match self {
            StepId::Environment => "ENVIRONMENT PROBE",
            StepId::Acquire => "ARCHIVE ACQUISITION",
            StepId::Network => "NETWORK ISOLATION",
            StepId::Extract => "TWO-STAGE EXTRACTION",
            StepId::Registry => "REGISTRY INJECTION",
            StepId::MountIso => "IMAGE MOUNT",
            StepId::Install => "SILENT DEPLOYMENT",
            StepId::Verify => "COMPLETION POLLING",
            StepId::Finalize => "TEARDOWN & RESTORE",
        }
    }

    /// 安装编排中的从属说明行，用于当前步骤面板的副标题。
    pub fn note(self) -> &'static str {
        match self {
            StepId::Environment => "校验用户名/计算机名 ASCII 与管理员权限",
            StepId::Acquire => "本地选择或分块多线程下载",
            StepId::Network => "禁用非虚拟网卡，注册 Drop 兜底恢复",
            StepId::Extract => "主包解压 + _SolidSQUAD_ 二次解压",
            StepId::Registry => "导入 _SolidSQUAD_ 下全部 .reg",
            StepId::MountIso => "Mount-DiskImage 挂载并定位 setup.exe",
            StepId::Install => "StartSWInstall /install /now 或 msiexec 回退",
            StepId::Verify => "日志 / 进程 / 可执行文件三重轮询",
            StepId::Finalize => "恢复网络、卸载镜像、清理临时目录",
        }
    }
}

/// 安装过程中可重试或可终止的错误分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// 环境不满足（非 ASCII 用户名、缺少管理员权限、磁盘空间不足）。
    Environment,
    /// 网络与下载失败。
    Download,
    /// 解压失败。
    Extract,
    /// 安装器返回失败。
    Install,
    /// 文件替换 / 服务安装失败。
    PostInstall,
    /// 用户主动取消。
    Cancelled,
    /// 内部错误。
    Internal,
}
/// 通过 `Channel<InstallEvent>` 推送的事件。
///
/// 序列化为 `{ "type": "...", "data": { ... } }`。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum InstallEvent {
    /// 进入某个逻辑步骤。
    StepChanged {
        step: usize,
        total: usize,
        id: StepId,
        name: String,
        en_name: String,
        note: String,
    },
    /// 下载进度。
    DownloadProgress {
        bytes_done: u64,
        bytes_total: u64,
        speed_bps: u64,
        chunks_done: usize,
        chunks_total: usize,
    },
    /// 解压进度。
    ExtractProgress {
        percent: f64,
        current_file: String,
        pass: usize,
    },
    /// **单个文件**的下载状态。
    ///
    /// 多文件/分片下载时，`DownloadProgress` 只给总进度，
    /// 看不出"哪一片下到哪了"。这个事件按文件逐个上报，前端渲染成一张清单。
    DownloadFileStatus {
        /// 第几个文件（1 基，便于显示）
        index: usize,
        /// 共几个文件
        total: usize,
        /// 文件名（**解析后的真实名**，不是 URL 里 `.aspx` 那种假名）
        name: String,
        /// 已下载字节
        bytes_done: u64,
        /// 总字节（未知时为 0）
        bytes_total: u64,
        /// 瞬时速度（字节/秒）
        speed_bps: u64,
        /// 阶段：`resolving` / `downloading` / `done` / `failed` / `skipped`
        phase: String,
        /// 补充说明（命名来源、错误原因等）
        detail: String,
    },
    /// 通用安装进度。
    InstallProgress { percent: f64, message: String },    /// 一行日志。
    LogMessage {
        level: LogLevel,
        message: String,
        timestamp: String,
    },
    /// 检测到并处理（或放弃处理）的弹窗。
    PopupDetected { title: String, action: String },
    /// 错误。
    Error {
        kind: ErrorKind,
        message: String,
        recoverable: bool,
    },
    /// 环境检测结果。
    EnvironmentCheck {
        username: String,
        computer_name: String,
        username_ascii: bool,
        computer_ascii: bool,
        elevated: bool,
        free_space_gb: f64,
    },
    /// 安全软件检测与处置报告。
    ///
    /// 前端据此提示用户「已移除哪些」「哪些需要你手动从系统托盘退出」。
    AntivirusReport {
        /// 检测到的全部安全软件。
        detected: Vec<crate::antivirus::SecurityProduct>,
        /// 逐个的处置结论。
        outcomes: Vec<crate::antivirus::AvOutcome>,
        /// 需要用户手动从系统托盘退出的软件名。
        manual_required: Vec<String>,
        /// 是否移除了 Windows Defender。
        defender_removed: bool,
        /// 是否需要重启才彻底生效。
        reboot_required: bool,
        /// 一句话总结。
        summary: String,
    },
    /// 剩余时间估算（秒）。
    EstimatedTimeRemaining { seconds: u64 },
    /// 流程结束。
    Completed { success: bool, message: String },
}

/// `get_install_status` 的返回值——无需 Channel 也能渲染整个界面。
#[derive(Debug, Clone, Serialize)]
pub struct InstallStatus {
    pub running: bool,
    pub cancelled: bool,
    pub finished: bool,
    pub success: bool,
    pub current_step: usize,
    pub total_steps: usize,
    pub step_id: Option<StepId>,
    pub percent: f64,
    pub message: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub popups_handled: u64,
    pub network_disabled: bool,
    pub iso_mounted: bool,
    /// 完成度之和 = 已完成步骤数（用于前端步骤条着色）。
    pub completed_steps: Vec<usize>,
    /// 随流程增长的估算剩余秒数，null 表示未知。
    pub eta_seconds: Option<u64>,
    /// 需要用户手动退出的安全软件数量。
    pub antivirus_manual_required: u64,
    /// 是否移除了 Windows Defender。
    pub antivirus_defender_removed: bool,
    /// 是否提示需要重启才彻底生效。
    pub reboot_required: bool,
}

impl Default for InstallStatus {
    fn default() -> Self {
        Self {
            running: false,
            cancelled: false,
            finished: false,
            success: false,
            current_step: 0,
            total_steps: StepId::ALL.len(),
            step_id: None,
            percent: 0.0,
            message: "待命".to_string(),
            started_at: None,
            finished_at: None,
            popups_handled: 0,
            network_disabled: false,
            iso_mounted: false,
            completed_steps: Vec::new(),
            eta_seconds: None,
            antivirus_manual_required: 0,
            antivirus_defender_removed: false,
            reboot_required: false,
        }
    }
}

/// 前端通过 `env_check` 主动查询的环境快照。
#[derive(Debug, Clone, Serialize)]
pub struct EnvSnapshot {
    pub username: String,
    pub computer_name: String,
    pub username_ascii: bool,
    pub computer_ascii: bool,
    pub elevated: bool,
    pub temp_dir: String,
    pub app_data_dir: String,
    pub config_path: String,
    pub config_exists: bool,
    pub free_space_gb: f64,
    /// 7z 解压程序的可用性快照。
    pub seven_zip: SevenZipStatus,
}

/// 7z 解压程序的检测结果（设置页与部署页都会展示）。
#[derive(Debug, Clone, Serialize)]
pub struct SevenZipStatus {
    /// 程序路径（`PATH` 回退时为裸名 `7z`）。
    pub program: String,
    /// 来源标签：配置指定 / 随程序打包 / 自动下载缓存 / 已安装的 7-Zip / 系统 PATH
    pub source: String,
    /// 是否真的可用。
    pub available: bool,
    /// 自动下载开关的当前取值。
    pub auto_download: bool,
    /// 自动下载地址。
    pub download_url: String,
}
