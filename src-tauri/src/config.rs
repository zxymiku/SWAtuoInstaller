//! 配置系统。
//!
//! 三层来源，按优先级从低到高：
//!   1. 编译期嵌入的默认配置（`include_str!("../resources/default-config.toml")`）
//!   2. 用户配置 `{app_data_dir}/config.toml`
//!   3. 前端显式传入的 `Config`（`start_install` / `save_config`）
//!
//! 合并使用 `toml_edit` 做**结构保留合并**：用户已有的值与注释原样保留，
//! 缺失字段从默认配置补齐，且补齐时把默认配置里的行内注释一并带过去。
//!
//! 所有时间参数单位为**分钟**（float，`0.5` = 30 秒）。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, Value};

/// 嵌入二进制的默认配置。
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../resources/default-config.toml");

/// 最小的可支持线程数 / 最大线程数。
pub const MIN_DOWNLOAD_THREADS: u32 = 1;
pub const MAX_DOWNLOAD_THREADS: u32 = 255;

// ---------------------------------------------------------------------------
// 结构体定义
// ---------------------------------------------------------------------------

/// 顶层配置：所有字段都有 `Default`，因此整体可直接 derive。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub download: DownloadConfig,
    pub remote_config: RemoteConfigConfig,
    pub install: InstallConfig,
    pub network: NetworkConfig,
    pub process: ProcessConfig,
    pub flexnet: FlexnetConfig,
    pub workdir: WorkdirConfig,
    pub sevenzip: SevenZipConfig,
    pub antivirus: AntivirusConfig,
}

/// `[antivirus]` —— 安装前的安全软件处置。
///
/// 背景：Windows Defender 与多数第三方杀软会把 `_SolidSQUAD_` 里的补丁文件
/// 判为威胁并直接删除，导致安装到一半失败。因此编排在解压**之前**先处置杀软。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AntivirusConfig {
    /// 是否启用本阶段。设为 false 则整步跳过（只记日志）。
    pub enabled: bool,
    /// 是否先用 SecurityCenter2 检测已注册的安全软件并告知用户。
    pub detect: bool,
    /// 是否移除 Windows Defender。
    pub remove_defender: bool,
    /// 随仓库提供的 windows-defender-remover 的 `script` 目录。
    ///
    /// 留空时按顺序自动查找：
    ///   1. 本字段
    ///   2. exe 同级的 `othertools\windows-defender-remover-main\script`
    ///   3. 源码目录 `othertools\windows-defender-remover-main\script`（开发期）
    ///   4. Tauri 资源目录下的同名相对路径
    ///
    /// 必须指向含 `PowerRun.exe` 与 `RemoveSecHealthApp.ps1` 的那个 `script` 目录。
    pub defender_tool_path: String,
    /// 删除 Defender 后是否提示"必须重启"。
    ///
    /// 删除动作本身会完成，但部分驱动/服务注册要重启才彻底生效。
    /// 本程序**不会**代替用户重启，也不因此中断流程——只如实提示。
    pub warn_reboot_after_removal: bool,
    /// 第三方杀软处置方式：
    ///
    /// | 取值 | 行为 |
    /// |---|---|
    /// | `"prompt"` | 只检测并在界面上提示用户手动从托盘退出 |
    /// | `"terminate"` | 额外尝试终止其进程 / 停止其服务 |
    /// | `"uninstall"` | 再额外尝试按其 `UninstallString` 静默卸载 |
    pub third_party_mode: String,
    /// 处置后等待杀软真正停下的时间，单位分钟。
    pub settle_minutes: f64,
    /// 单条处置命令的超时，单位分钟。
    pub command_timeout_minutes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub language: String,
    pub theme: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadConfig {
    pub url: String,
    /// 分片下载：每个分片一个 URL（换行分隔）。
    ///
    /// 留空时退回到 `url` 单包模式。非空时逐片下载到同一目录，
    /// **保留各片原始文件名**，让 7z 直接把整套分卷当压缩包读。
    pub multipart_urls: Vec<String>,
    /// 分片模式：是否把各片按顺序拼接成单个文件。
    ///
    /// 默认 `false` —— 直接交给 7z 处理标准分卷（`.001/.002/...`）。
    /// 只有当分片是"被任意切开的单个文件"而不是 7z 分卷时，才需要设为 `true`。
    pub multipart_concat: bool,
    /// **同时下载的文件数**（分片模式）。
    ///
    /// `threads` 是"每个文件内部切几块"，本项是"同时下几个文件"，
    /// 两者相乘 = 峰值连接数。默认 4，上限 32。
    pub concurrent_files: u32,
    /// 下载前是否用 HEAD 探测真实文件名与尺寸。
    ///
    /// **网盘直链务必保持 `true`**：文件名往往不在 URL 路径里（藏在查询参数中，
    /// 还挂着 `.aspx` 假后缀），不探测就会存成 `xxx.001.aspx`，
    /// 而 7z 需要 `.001/.002` 这样成套的名字才能读整套分卷。
    pub probe_filenames: bool,
    pub threads: u32,
    pub checksum_sha256: String,
    pub retry_count: u32,
    pub retry_backoff_minutes: f64,
    pub connect_timeout_minutes: f64,
    pub read_timeout_minutes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteConfigConfig {
    pub url: String,
    pub auto_fetch_on_start: bool,
    pub fetch_timeout_minutes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallConfig {
    pub install_drive: String,
    pub install_path: String,
    /// 组件白名单 —— **只对 msiexec 路径生效**。
    ///
    /// `StartSWInstall.exe` 读的是管理员映像里生成的 `.sldIM`，
    /// 命令行**不接受**组件列表；设置本项但走 StartSWInstall 时程序会明确告警。
    pub components_whitelist: Vec<String>,
    /// 是否强制走 `msiexec` 路径（跳过 `StartSWInstall.exe`）。
    ///
    /// 只在确实需要 `components_whitelist` 时才打开：msiexec 直接装 MSI，
    /// 会跳过安装管理器负责的若干步骤（前置检查、部分组件的串接安装）。
    pub force_msiexec: bool,
    /// 追加给 `StartSWInstall.exe` 的额外命令行开关。
    ///
    /// 程序固定会传 `/install /now`；这里只补你需要的额外项，例如：
    ///
    /// ```toml
    /// install_switches = ["/l", "C:\\sw-install.log"]
    /// ```
    ///
    /// 注意：不在列表里的项不会被自动添加，写错开关安装器会直接忽略它。
    pub install_switches: Vec<String>,
    /// SOLIDWORKS 序列号，映射到 MSI 属性 **`SOLIDWORKSSERIALNUMBER`**。
    ///
    /// 官方文档《通过命令行从管理映像安装》用的就是这个属性名：
    ///
    /// ```text
    /// SOLIDWORKSSERIALNUMBER="xxxx xxxx xxxx xxxx xxxx xxxx"
    /// ```
    ///
    /// 留空则不传该属性（由补丁包里的 `.reg` 授权）。
    pub serial_number: String,
    /// **Toolbox 数据目录**，映射到官方属性 `TOOLBOXFOLDER`。
    ///
    /// 官方文档给的例子是 `TOOLBOXFOLDER="C:\SolidWorks Data\"`。
    /// 留空则用默认值 `C:\SOLIDWORKS Data`（需要装 Toolbox 时）。
    pub toolbox_folder: String,
    /// 语言包名称，对应介质里的 `swwi\lang\<名称>\<名称>.msi`。
    ///
    /// 例如 `"chinese-simplified"` → `swwi\lang\chinese-simplified\chinese-simplified.msi`。
    ///
    /// **语言包是独立的 MSI**，官方文档明确要求单独安装：
    ///
    /// > SOLIDWORKS 法语安装组件必须单独安装：
    /// > `msiexec /i "...\64bit\SOLIDWORKS French\french.msi" /qb`
    ///
    /// 留空则不装语言包。可用的值就是介质 `swwi\lang\` 下的目录名：
    /// `chinese`（繁体 1028）、`chinese-simplified`（简体 2052）、
    /// `czech`、`french`、`german`、`italian`、`japanese`、`korean`、
    /// `polish`、`portuguese-brazilian`、`russian`、`spanish`、`turkish`。
    pub language_pack: String,
    /// 兼容旧配置的字段。语言包现在等待主程序 PID 退出后立即启动，
    /// 不再使用固定延迟；读取旧配置时该值会被归零。
    pub language_pack_delay_minutes: f64,
    /// 是否检测并在缺失时安装 Windows 前置组件。
    ///
    /// 官方手册明确：**命令行安装不会自动装前置组件**
    /// （.NET 4.8 / Visual C++ 可再发行 / WebView2 / VBA）。
    /// 平时双击 `setup.exe` 是安装管理器代劳，改走 `msiexec` 就必须自己补。
    ///
    /// 缺失的后果不是"装不上"而是"装上了跑不起来"：
    /// `solidworks.msi` 没有 `LaunchCondition` 表，安装会报成功。
    pub install_prerequisites: bool,
    /// 单个前置组件安装的超时，单位分钟。
    ///
    /// .NET 4.8 的安装包有 112 MB，在慢盘上可能要几分钟。
    pub prerequisite_timeout_minutes: f64,
    pub polling: PollingConfig,
    pub popup: PopupConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PollingConfig {
    pub poll_interval_minutes: f64,
    pub timeout_minutes: f64,
    pub log_check_enabled: bool,
    pub process_check_enabled: bool,
    pub file_check_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PopupConfig {
    pub scan_interval_ms: u64,
    pub auto_click_confirm: bool,
    pub auto_click_yes: bool,
    pub auto_click_no: bool,
    pub popup_timeout_minutes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub disable_network: bool,
    pub restore_network_after: bool,
    pub adapter_disable_method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessConfig {
    pub kill_sw_processes: bool,
    /// 进程匹配关键词（正则，**不区分大小写**）。
    ///
    /// 默认值是一组用 `|` 连接的关键词。匹配时会先去掉首尾空白，
    /// 并把匹配目标统一折叠大小写后比较，因此 `SolidWorks` / `solidworks`
    /// / ` SOLIDWORKS ` 都能命中。
    pub kill_pattern: String,
    /// 是否把匹配范围扩展到进程的**完整路径与命令行**。
    ///
    /// 默认 `true`：按关键词能在命令行里找到进程
    /// （例如 `java.exe -jar ...\solidworks-helper.jar`）。
    /// 代价是每轮要查询全部进程的元数据，会明显变慢；
    /// 只关心进程名时设为 `false` 可显著提速。
    pub match_command_line: bool,
    pub kill_timeout_minutes: f64,
    pub force_kill_after_timeout: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlexnetConfig {
    pub server_install_timeout_minutes: f64,
    pub server_check_interval_minutes: f64,
    pub remove_old_server_first: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkdirConfig {
    pub cleanup_temp_after: bool,
    pub temp_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SevenZipConfig {
    pub exe_path: String,
    pub extract_timeout_minutes: f64,
    /// 三级查找全部失败时，是否自动从 `download_url` 获取 7z 并缓存到应用数据目录。
    pub auto_download: bool,
    /// 自动下载地址，默认指向 7-Zip 官方发布的独立控制台程序 7zr.exe。
    pub download_url: String,
    /// 自动下载超时，单位分钟。
    pub download_timeout_minutes: f64,
}

// ---------------------------------------------------------------------------
// 默认实现
// ---------------------------------------------------------------------------

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: "zh-CN".to_string(),
            theme: "endfield".to_string(),
        }
    }
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            url: "https://example.com/solidworks2024sp5.7z".to_string(),
            multipart_urls: Vec::new(),
            multipart_concat: false,
            concurrent_files: 4,
            probe_filenames: true,
            threads: 32,
            checksum_sha256: String::new(),
            retry_count: 3,
            retry_backoff_minutes: 0.5,
            connect_timeout_minutes: 1.0,
            read_timeout_minutes: 5.0,
        }
    }
}

impl Default for RemoteConfigConfig {
    fn default() -> Self {
        Self {
            url: "https://example.com/config.toml".to_string(),
            auto_fetch_on_start: false,
            fetch_timeout_minutes: 1.0,
        }
    }
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            install_drive: "C".to_string(),
            install_path: String::new(),
            // 默认装「主程序 + Toolbox」。组件名是 MSI Feature 表里的**精确名字**，
            // 已在 SolidWorks 2024 SP5 Premium.DVD 的 swwi\data\solidworks.msi 上核对
            //（该 MSI 共 221 个 Feature，Feature 名区分大小写）：
            //   SolidWorks（SOLIDWORKS 2024 SP05）    ← 主程序
            //     └── AddIns（SOLIDWORKS Add-Ins）    ← Toolbox 的父级，必须一起列出
            //           └── SolidWorksToolbox          ← Toolbox
            // 只写 SolidWorksToolbox 会因缺父级被 msiexec **静默忽略**；
            // 只写 SolidWorks 会把全部 AddIns（Simulation、FeatureWorks…）都装上。
            // 官方手册《命令行特征属性》给出的就是这两项（原文：
            // `ADDLOCAL=SolidWorks, SolidWorksToolbox`）。
            // 实测 MSI 里 `SolidWorksToolbox` 的父级是 `AddIns`，但 `AddIns`
            // 的属性位是 10（ADVERTISE|UIDISALLOWED）—— 界面上不可选，
            // 也正因此不在官方 ADDLOCAL 清单里；Windows Installer 会按需
            // 自动补装父级。以官方写法为准。
            components_whitelist: vec!["SolidWorks".to_string(), "SolidWorksToolbox".to_string()],
            // 必须为 true —— 走 startswinstall 时命令行不接受组件列表，
            // components_whitelist 会被静默忽略（程序会在日志里告警）。
            force_msiexec: true,
            install_switches: Vec::new(),
            serial_number: String::new(),
            // 官方属性 TOOLBOXFOLDER 的默认值。只在装 Toolbox 时才会传。
            //
            // **默认值刻意不带空格**：msiexec 会自己重新解析命令行，
            // 含空格的属性值若引号处理不当会直接报
            // `1639 ERROR_INVALID_COMMAND_LINE` 并弹出用法对话框，
            // 安装完全不执行（连 `/l*v` 日志都不生成）—— 这是实机踩到的坑。
            // 用下划线代替空格可以从根上避开这个问题。
            toolbox_folder: r"C:\SOLIDWORKS_Data".to_string(),
            // 介质 swwi\lang\ 下的目录名，简体中文就是它（ProductLanguage=2052）。
            language_pack: "chinese-simplified".to_string(),
            // 兼容旧配置字段；实际启动由主程序 PID 退出事件驱动。
            language_pack_delay_minutes: 0.0,
            install_prerequisites: true,
            prerequisite_timeout_minutes: 20.0,
            polling: PollingConfig::default(),
            popup: PopupConfig::default(),
        }
    }
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            poll_interval_minutes: 0.5,
            timeout_minutes: 240.0,
            log_check_enabled: true,
            process_check_enabled: true,
            file_check_enabled: true,
        }
    }
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            scan_interval_ms: 500,
            auto_click_confirm: true,
            auto_click_yes: true,
            auto_click_no: false,
            popup_timeout_minutes: 5.0,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            disable_network: true,
            restore_network_after: true,
            adapter_disable_method: "netsh".to_string(),
        }
    }
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            kill_sw_processes: true,
            kill_pattern: ".*solidwork.*|.*sldworks.*|.*sw_dn.*|.*sidworks.*|.*flexnet.*|.*lmgrd.*"
                .to_string(),
            match_command_line: true,
            kill_timeout_minutes: 2.0,
            force_kill_after_timeout: true,
        }
    }
}

impl Default for FlexnetConfig {
    fn default() -> Self {
        Self {
            server_install_timeout_minutes: 5.0,
            server_check_interval_minutes: 0.5,
            remove_old_server_first: true,
        }
    }
}

impl Default for WorkdirConfig {
    fn default() -> Self {
        Self {
            cleanup_temp_after: true,
            temp_dir: String::new(),
        }
    }
}

impl Default for SevenZipConfig {
    fn default() -> Self {
        Self {
            exe_path: String::new(),
            extract_timeout_minutes: 30.0,
            auto_download: true,
            // 官方发布的"独立控制台版本"，单文件、体积小、恰好够用。
            download_url: "https://github.com/ip7z/7zip/releases/latest/download/7zr.exe"
                .to_string(),
            download_timeout_minutes: 3.0,
        }
    }
}

impl Default for AntivirusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            detect: true,
            remove_defender: true,
            defender_tool_path: String::new(),
            warn_reboot_after_removal: true,
            // 默认只终止进程 / 停服务，不擅自卸载别人的安全软件。
            third_party_mode: "terminate".to_string(),
            settle_minutes: 0.5,
            command_timeout_minutes: 5.0,
        }
    }
}

// ---------------------------------------------------------------------------
// 配置实现
// ---------------------------------------------------------------------------

impl Config {
    /// 解析一段 TOML 文本，缺失字段用 `Default` 补齐。
    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str::<Config>(text).map_err(|e| format!("配置解析失败: {e}"))
    }

    /// 序列化为 TOML 文本（保存用户配置时使用）。
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| format!("配置序列化失败: {e}"))
    }

    /// 规范化来自前端的配置：钳制线程数、清理盘符、去空白。
    ///
    /// 在保存或执行之前调用，避免无效值进入安装编排。
    pub fn normalize(&mut self) {
        self.download.threads = self
            .download
            .threads
            .clamp(MIN_DOWNLOAD_THREADS, MAX_DOWNLOAD_THREADS);

        self.download.retry_backoff_minutes = self.download.retry_backoff_minutes.max(0.0);
        self.download.connect_timeout_minutes = self.download.connect_timeout_minutes.max(0.1);
        self.download.read_timeout_minutes = self.download.read_timeout_minutes.max(0.1);
        self.remote_config.fetch_timeout_minutes =
            self.remote_config.fetch_timeout_minutes.max(0.1);
        self.install.polling.poll_interval_minutes =
            self.install.polling.poll_interval_minutes.max(0.01);
        self.install.polling.timeout_minutes = self.install.polling.timeout_minutes.max(0.1);
        self.install.popup.popup_timeout_minutes =
            self.install.popup.popup_timeout_minutes.max(0.1);
        self.install.popup.scan_interval_ms = self.install.popup.scan_interval_ms.clamp(50, 60_000);
        self.process.kill_timeout_minutes = self.process.kill_timeout_minutes.max(0.0);
        self.flexnet.server_install_timeout_minutes =
            self.flexnet.server_install_timeout_minutes.max(0.1);
        self.flexnet.server_check_interval_minutes =
            self.flexnet.server_check_interval_minutes.max(0.01);
        self.sevenzip.extract_timeout_minutes = self.sevenzip.extract_timeout_minutes.max(0.1);
        self.sevenzip.download_timeout_minutes = self.sevenzip.download_timeout_minutes.max(0.1);
        self.sevenzip.download_url = self.sevenzip.download_url.trim().to_string();
        if self.sevenzip.auto_download && self.sevenzip.download_url.is_empty() {
            self.sevenzip.download_url = SevenZipConfig::default().download_url;
        }

        let drive = self
            .install
            .install_drive
            .trim()
            .trim_end_matches(':')
            .trim_end_matches('\\')
            .to_ascii_uppercase();
        self.install.install_drive = if drive.is_empty() {
            "C".to_string()
        } else {
            drive
        };

        self.install.install_path = self.install.install_path.trim().to_string();
        self.install.serial_number = self.install.serial_number.trim().to_string();
        self.download.url = self.download.url.trim().to_string();
        self.download.checksum_sha256 = self
            .download
            .checksum_sha256
            .trim()
            .to_ascii_lowercase()
            .replace(' ', "");
        self.remote_config.url = self.remote_config.url.trim().to_string();
        self.workdir.temp_dir = self.workdir.temp_dir.trim().to_string();
        self.sevenzip.exe_path = self.sevenzip.exe_path.trim().to_string();
        self.process.kill_pattern = self.process.kill_pattern.trim().to_string();
        // 分片 URL：去掉空白与包裹的引号，丢弃空行（用户从文档里粘贴时常见）。
        self.download.multipart_urls = self
            .download
            .multipart_urls
            .iter()
            .map(|item| item.trim().trim_matches('"').trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();

        // 下载并发：每文件线程数与同时下载文件数都要有界。
        self.download.threads = self.download.threads.clamp(1, 255);
        self.download.concurrent_files = self.download.concurrent_files.clamp(1, 32);

        // 安装开关：去掉空白项，避免把空参数传给安装器。
        self.install.install_switches = self
            .install
            .install_switches
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();

        // 语言包名要与介质 `swwi\lang\` 下的**目录名**比对，
        // 因此首尾空白必须去掉 —— 否则 `" chinese-simplified"` 会匹配不到。
        self.install.language_pack = self.install.language_pack.trim().to_string();
        // Toolbox 数据目录会作为 MSI 属性值传给安装器，同样去空白。
        self.install.toolbox_folder = self.install.toolbox_folder.trim().to_string();
        // 组件名区分大小写，但首尾空白一定是手误；去掉以免被 msiexec 静默忽略。
        self.install.components_whitelist = self
            .install
            .components_whitelist
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();

        // 兼容旧配置，但不再允许固定延迟阻塞语言包启动。
        self.install.language_pack_delay_minutes = 0.0;
        // .NET 4.8 安装包有 112 MB，超时至少要给一点时间。
        self.install.prerequisite_timeout_minutes =
            self.install.prerequisite_timeout_minutes.max(1.0);

        // 杀软阶段
        self.antivirus.defender_tool_path = self.antivirus.defender_tool_path.trim().to_string();
        self.antivirus.settle_minutes = self.antivirus.settle_minutes.max(0.0);
        self.antivirus.command_timeout_minutes = self.antivirus.command_timeout_minutes.max(0.1);
        let mode = self.antivirus.third_party_mode.trim().to_ascii_lowercase();
        self.antivirus.third_party_mode = match mode.as_str() {
            "prompt" | "terminate" | "uninstall" => mode,
            _ => "terminate".to_string(),
        };
        self.install.components_whitelist = self
            .install
            .components_whitelist
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();

        let method = self.network.adapter_disable_method.to_ascii_lowercase();
        self.network.adapter_disable_method = if method == "powershell" {
            "powershell".to_string()
        } else {
            "netsh".to_string()
        };

        if self.general.language.trim().is_empty() {
            self.general.language = "zh-CN".to_string();
        }
        if self.general.theme.trim().is_empty() {
            self.general.theme = "endfield".to_string();
        }
    }

    /// 安装目标目录：`install_path` 非空则直接使用，否则 `{drive}:\SW`。
    pub fn resolved_install_path(&self) -> PathBuf {
        let custom = self.install.install_path.trim();
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
        PathBuf::from(format!("{}:\\SW", self.install.install_drive))
    }

    /// 传给 `msiexec` 的 `INSTALLDIR` 值 —— 就是 [`Config::resolved_install_path`]。
    ///
    /// 官方手册的模板是 `INSTALLDIR="C:\Program Files\your_folder"`，
    /// 而实测（一台真实安装完成的机器）：
    ///
    /// ```text
    /// INSTALLDIR = C:\solidworks\2024corp
    ///   → 程序落在 C:\solidworks\2024corp\SOLIDWORKS\SLDWORKS.exe
    ///   → 语言资源 C:\solidworks\2024corp\SOLIDWORKS\lang\chinese-simplified\
    ///   → Toolbox  C:\solidworks\2024corp\SOLIDWORKS\Toolbox\
    /// ```
    ///
    /// 也就是说 **`INSTALLDIR` 就是安装根目录**，各组件会在下面建立
    /// 自己的目录（例如 `SOLIDWORKS\`、`eDrawings\`）。因此这里**原样传入**，
    /// 不要额外追加任何层级 —— 追加会让文件落到用户没预期的地方。
    pub fn msi_install_dir(&self) -> PathBuf {
        self.resolved_install_path()
    }

    /// `SLDWORKS.exe` 的候选位置（第 9 步文件检测用）。
    ///
    /// **注意文件名是 `SLDWORKS.exe`，不是 `SOLIDWORKS.exe`。**
    /// 项目早期版本一直在找后者，而它根本不存在 ——
    /// 结果是**安装明明成功了，轮询却永远判定不了完成**，白等到超时。
    /// 这是在一台真实安装完成的机器上实测踩到的坑。
    ///
    /// `INSTALLDIR` 是安装根目录，MSI 会在下面建立各组件目录；
    /// 但用户也可能直接把 `install_path` 指到程序目录，
    /// 所以这里给出候选，由调用方取第一个存在的。
    pub fn solidworks_exe_candidates(&self) -> Vec<PathBuf> {
        let root = self.resolved_install_path();

        vec![
            // 最常见：INSTALLDIR 是产品系列根（已实测确认）
            root.join("SOLIDWORKS").join("SLDWORKS.exe"),
            // 用户把 install_path 直接指到了程序目录
            root.join("SLDWORKS.exe"),
            // 少数布局会多一层 SOLIDWORKS Corp
            root.join("SOLIDWORKS Corp")
                .join("SOLIDWORKS")
                .join("SLDWORKS.exe"),
            root.join("SOLIDWORKS Corp").join("SLDWORKS.exe"),
        ]
    }

    /// 兼容旧接口：返回第一个候选（不做存在性检查）。
    ///
    /// 新代码应当用 [`Config::solidworks_exe_candidates`]。
    pub fn solidworks_exe_path(&self) -> PathBuf {
        self.solidworks_exe_candidates()
            .into_iter()
            .next()
            .unwrap_or_else(|| self.resolved_install_path().join("SLDWORKS.exe"))
    }

    // --- 时间换算：TOML 中一律是分钟，运行时转成 Duration -------------------

    pub fn connect_timeout(&self) -> Duration {
        minutes(self.download.connect_timeout_minutes)
    }

    pub fn read_timeout(&self) -> Duration {
        minutes(self.download.read_timeout_minutes)
    }

    pub fn retry_backoff(&self, attempt: u32) -> Duration {
        // 指数退避：base * 2^(attempt-1)，上限 30 分钟，避免极端配置导致长时间挂起。
        let factor = 2u32.saturating_pow(attempt.saturating_sub(1).min(6));
        let secs = self.download.retry_backoff_minutes.max(0.0) * 60.0 * factor as f64;
        Duration::from_secs_f64(secs.min(30.0 * 60.0))
    }

    pub fn fetch_timeout(&self) -> Duration {
        minutes(self.remote_config.fetch_timeout_minutes)
    }

    pub fn poll_interval(&self) -> Duration {
        minutes(self.install.polling.poll_interval_minutes)
    }

    pub fn poll_timeout(&self) -> Duration {
        minutes(self.install.polling.timeout_minutes)
    }

    pub fn popup_timeout(&self) -> Duration {
        minutes(self.install.popup.popup_timeout_minutes)
    }

    /// 进程优雅退出的等待时长。
    pub fn kill_timeout(&self) -> Duration {
        minutes(self.process.kill_timeout_minutes)
    }

    pub fn server_install_timeout(&self) -> Duration {
        minutes(self.flexnet.server_install_timeout_minutes)
    }

    pub fn server_check_interval(&self) -> Duration {
        minutes(self.flexnet.server_check_interval_minutes)
    }

    pub fn extract_timeout(&self) -> Duration {
        minutes(self.sevenzip.extract_timeout_minutes)
    }

    /// 7z 自动下载的超时。
    pub fn seven_zip_download_timeout(&self) -> Duration {
        minutes(self.sevenzip.download_timeout_minutes)
    }

    /// 杀软处置后等待其停下的时长。
    pub fn antivirus_settle(&self) -> Duration {
        minutes(self.antivirus.settle_minutes)
    }

    /// 单条杀软处置命令的超时。
    pub fn antivirus_command_timeout(&self) -> Duration {
        minutes(self.antivirus.command_timeout_minutes)
    }
}

/// 分钟 → Duration，负数与 NaN 归零。
fn minutes(value: f64) -> Duration {
    if !value.is_finite() || value <= 0.0 {
        return Duration::from_secs(0);
    }
    Duration::from_secs_f64(value * 60.0)
}

// ---------------------------------------------------------------------------
// 文件读写与合并
// ---------------------------------------------------------------------------

/// 用户配置文件的完整路径。
pub fn user_config_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("config.toml")
}

/// 用户配置的可读文本形式——不存在时把默认配置落盘。
pub fn read_user_config_text(app_data_dir: &Path) -> Result<String, String> {
    let path = user_config_path(app_data_dir);
    if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("读取配置失败 {}: {e}", path.display()))
    } else {
        write_text(&path, DEFAULT_CONFIG_TOML)?;
        Ok(DEFAULT_CONFIG_TOML.to_string())
    }
}

/// 把默认配置写入用户目录（“重置默认”与首次启动）。
pub fn reset_user_config(app_data_dir: &Path) -> Result<Config, String> {
    let path = user_config_path(app_data_dir);
    write_text(&path, DEFAULT_CONFIG_TOML)?;
    let mut config = Config::from_toml(DEFAULT_CONFIG_TOML)?;
    config.normalize();
    Ok(config)
}

/// 保存用户配置：结构保留合并后再落盘。
///
/// 先与当前磁盘上的文档合并，使新字段带上默认注释、已有注释不被破坏。
pub fn save_user_config(app_data_dir: &Path, config: &Config) -> Result<(), String> {
    let path = user_config_path(app_data_dir);
    let current = if path.exists() {
        fs::read_to_string(&path).unwrap_or_else(|_| DEFAULT_CONFIG_TOML.to_string())
    } else {
        DEFAULT_CONFIG_TOML.to_string()
    };

    let mut normalized = config.clone();
    normalized.normalize();
    let incoming = normalized.to_toml()?;

    let merged = merge_toml(&current, &incoming)?;
    write_text(&path, &merged)
}

/// 加载最终生效的配置：默认配置 ← 用户配置（结构保留合并）→ 反序列化。
pub fn load_effective_config(app_data_dir: &Path) -> Result<Config, String> {
    let path = user_config_path(app_data_dir);
    let user_text = if path.exists() {
        fs::read_to_string(&path).map_err(|e| format!("读取配置失败 {}: {e}", path.display()))?
    } else {
        write_text(&path, DEFAULT_CONFIG_TOML)?;
        DEFAULT_CONFIG_TOML.to_string()
    };

    let merged = merge_toml(DEFAULT_CONFIG_TOML, &user_text)?;
    let mut config = Config::from_toml(&merged)?;
    config.normalize();

    // 用户文件若缺少新字段，顺手把补齐后的内容写回，下次启动即为完整文件。
    if merged != user_text {
        let _ = write_text(&path, &merged);
    }

    Ok(config)
}

/// 结构保留合并：`base` 为基线（默认配置），`overlay` 覆盖其值。
///
/// - 标量/数组：overlay 的值胜出。
/// - 表：递归合并。
/// - overlay 中缺失的表/键：保留 base 的内容**及其注释**。
pub fn merge_toml(base: &str, overlay: &str) -> Result<String, String> {
    merge_toml_with(base, overlay, false)
}

/// 与 [`merge_toml`] 相同，但 `preserve_empty = true` 时
/// **overlay 中的空值不会覆盖 base 的非空值**。
///
/// 「空值」的定义：
///
/// | 形态 | 是否视为"留空" |
/// |---|---|
/// | 空字符串 `""` | 是 |
/// | 空数组 `[]` | 是 |
/// | 只有空白的字符串 | 是 |
/// | 数字 `0` | **否**（0 是合法取值，不能当作留空） |
/// | 布尔 `false` | **否**（false 是合法取值） |
///
/// 导出配置时用这个模式：表单里没填的项保持磁盘上的现有值，
/// 而不是被清成默认值。
pub fn merge_toml_preserving_empty(base: &str, overlay: &str) -> Result<String, String> {
    merge_toml_with(base, overlay, true)
}

fn merge_toml_with(base: &str, overlay: &str, preserve_empty: bool) -> Result<String, String> {
    let mut base_doc = base
        .parse::<DocumentMut>()
        .map_err(|e| format!("默认配置解析失败: {e}"))?;
    let overlay_doc = overlay
        .parse::<DocumentMut>()
        .map_err(|e| format!("用户配置解析失败: {e}"))?;

    merge_table(
        base_doc.as_table_mut(),
        overlay_doc.as_table(),
        "",
        preserve_empty,
    );
    Ok(base_doc.to_string())
}

/// 判断一个 item 是否代表"留空"（空字符串 / 纯空白 / 空数组）。
fn is_blank_item(item: &Item) -> bool {
    match item {
        Item::Value(Value::String(text)) => text.value().trim().is_empty(),
        Item::Value(Value::Array(array)) => array.is_empty(),
        Item::None => true,
        _ => false,
    }
}

fn merge_table(target: &mut Table, source: &Table, path: &str, preserve_empty: bool) {
    for (key, source_item) in source.iter() {
        let full_key = if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        };

        // 留空保留模式：跳过空值，让 base 的现有值活下来。
        if preserve_empty && is_blank_item(source_item) {
            continue;
        }

        match target.get_mut(key) {
            Some(target_item) => match (&mut *target_item, source_item) {
                // 表 + 表 → 递归合并，保留 base 的键顺序与注释。
                (Item::Table(target_table), Item::Table(source_table)) => {
                    merge_table(target_table, source_table, &full_key, preserve_empty);
                }
                // 值 + 值 → 覆盖，但保留 base 的行内注释。
                (target_value, source_value) => {
                    let comment = target_value
                        .as_value()
                        .and_then(|v| v.decor().suffix())
                        .and_then(|raw| raw.as_str())
                        .map(|s| s.to_string());
                    let mut new_value = source_value.clone();
                    if let (Some(text), Some(value)) = (comment, new_value.as_value_mut()) {
                        // 形如 "   # 注释" —— 与 toml_edit 的装饰格式保持一致。
                        if let Ok(decor) = raw_decor(&text) {
                            *value.decor_mut() = decor;
                        }
                    }
                    *target_item = new_value;
                }
            },
            // base 中没有的键：整体从 overlay 追加（新字段，带 overlay 的注释）。
            None => {
                target.insert(key, source_item.clone());
            }
        }
    }
}

/// 把一段原始后缀装饰文本解析回 `Decor`。
///
/// `toml_edit` 没有公开构造 `Decor` 的 API，这里用一次最小解析把
/// `"   # 注释"` 还原成等价的装饰对象。
fn raw_decor(text: &str) -> Result<toml_edit::Decor, ()> {
    let snippet = format!("key = 0{text}\n");
    let doc = snippet.parse::<DocumentMut>().map_err(|_| ())?;
    doc.as_table()
        .get("key")
        .and_then(|item| item.as_value())
        .map(|value| value.decor().clone())
        .ok_or(())
}

/// 原子写文本：先写临时文件再替换，避免中途失败留下半个配置。
fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败 {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("写入临时配置失败 {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("替换配置失败 {}: {e}", path.display()))?;
    Ok(())
}

/// 供“拉取远程配置”使用：把远端文本与默认配置合并后返回可编辑的 `Config`。
///
/// 远端文本不必包含全部字段——缺失部分由默认配置补齐。
pub fn merge_remote_into_default(remote_text: &str) -> Result<(Config, String), String> {
    let merged = merge_toml(DEFAULT_CONFIG_TOML, remote_text)?;
    let mut config = Config::from_toml(&merged)?;
    config.normalize();
    Ok((config, merged))
}
