/**
 * 与 Rust 后端 serde 结构体一一对应的类型定义。
 *
 * 分两部分：
 *   1. 配置树（`src-tauri/src/config.rs`）
 *   2. 运行时事件与快照（`src-tauri/src/events.rs`）
 *
 * 约定：**所有时间参数单位都是分钟（float，0.5 = 30 秒）**，
 * 唯一例外是 `install.popup.scan_interval_ms`（毫秒）。
 */

// ===========================================================================
// 一、配置树
// ===========================================================================

/** `[general]` */
export interface GeneralConfig {
  /** 界面语言: `zh-CN` / `en-US` */
  language: string;
  /** 界面风格: `endfield` */
  theme: string;
}

/** `[download]` */
export interface DownloadConfig {
  /** 压缩包地址（HTTP/HTTPS）；分片模式下可留空 */
  url: string;
  /** 分片下载：每个分片一个 URL（换行分隔）。非空时进入分片模式 */
  multipart_urls: string[];
  /**
   * 分片模式是否把各片按顺序拼接成单文件。
   *
   * 默认 `false`：直接交给 7z 处理标准分卷（`.001/.002/...`）。
   * 只有分片是"被任意切开的单个文件"时才需要 `true`。
   */
  multipart_concat: boolean;
  /** **同时下载的文件数**（分片模式），范围 1~32 */
  concurrent_files: number;
  /**
   * 下载前是否用 HEAD 探测真实文件名与尺寸。
   *
   * 网盘直链务必保持 `true`：文件名常藏在查询参数里并挂着 `.aspx` 假后缀，
   * 不探测会存成 `xxx.001.aspx`，7z 就找不到整套分卷。
   */
  probe_filenames: boolean;
  /** **每个文件**的下载线程数，范围 1~255 */
  threads: number;
  /** SHA-256 校验值，空字符串表示跳过校验（分片模式下不生效） */
  checksum_sha256: string;
  /** 失败重试次数 */
  retry_count: number;
  /** 重试间隔基数（指数退避），单位分钟 */
  retry_backoff_minutes: number;
  /** 连接超时，单位分钟 */
  connect_timeout_minutes: number;
  /** 读取超时，单位分钟 */
  read_timeout_minutes: number;
}

/** `[remote_config]` */
export interface RemoteConfigConfig {
  /** 远程配置默认 URL */
  url: string;
  /** 启动时自动拉取 */
  auto_fetch_on_start: boolean;
  /** 拉取超时，单位分钟 */
  fetch_timeout_minutes: number;
}

/** `[install.polling]` */
export interface PollingConfig {
  /** 轮询间隔，单位分钟 */
  poll_interval_minutes: number;
  /** 最长等待，单位分钟 */
  timeout_minutes: number;
  /** 检查安装日志中的成功标记 */
  log_check_enabled: boolean;
  /** 检查安装进程是否退出 */
  process_check_enabled: boolean;
  /** 检查 SLDWORKS.exe 是否生成（注意不是 SOLIDWORKS.exe） */
  file_check_enabled: boolean;
}

/** `[install.popup]` */
export interface PopupConfig {
  /** 窗口扫描间隔，单位毫秒 */
  scan_interval_ms: number;
  /** 自动点击「确定」 */
  auto_click_confirm: boolean;
  /** 自动点击「是」 */
  auto_click_yes: boolean;
  /** 自动点击「否」 */
  auto_click_no: boolean;
  /** 单个弹窗等待超时，单位分钟 */
  popup_timeout_minutes: number;
}

/** `[install]` */
export interface InstallConfig {
  /** 目标盘符: C / D / E ... */
  install_drive: string;
  /** 自定义安装路径，留空则用 `{drive}:\SW` */
  install_path: string;
  /**
   * 组件白名单，空数组 = 全部组件。
   *
   * **仅 msiexec 路径生效**：`StartSWInstall.exe` 读的是管理员映像里生成的
   * `.sldIM`，命令行不接受组件列表。设置本项但走 StartSWInstall 时程序会告警。
   */
  components_whitelist: string[];
  /** 是否强制走 msiexec 路径（要用 components_whitelist 时才打开） */
  force_msiexec: boolean;
  /**
   * 追加给 `StartSWInstall.exe` 的额外命令行开关。
   *
   * 程序固定传 `/install /now`，这里只补额外项，例如 `["/l","C:\\sw.log"]`。
   */
  install_switches: string[];
  /**
   * 序列号 → MSI 属性 **`SOLIDWORKSSERIALNUMBER`**（官方属性名，不是 SERIALNUMBER）。
   *
   * 留空则不传该属性（由补丁包里的 `.reg` 授权）。
   */
  serial_number: string;
  /**
   * Toolbox 数据目录 → 官方属性 `TOOLBOXFOLDER`。
   *
   * **尽量避免含空格**：msiexec 会重新解析命令行，含空格的值若引号处理不当
   * 会报 `1639 ERROR_INVALID_COMMAND_LINE` 并弹出用法对话框。
   */
  toolbox_folder: string;
  /**
   * 语言包名称，对应介质 `swwi\lang\<名称>\<名称>.msi`。
   *
   * **语言包是独立的 MSI，官方要求单独安装。** 留空则跳过。
   * 可用值即介质 `swwi\lang\` 下的目录名（`chinese-simplified` 为简体中文）。
   */
  language_pack: string;
  /**
   * 兼容旧配置字段。语言包现在等待主程序进程退出后立即启动。
   */
  language_pack_delay_minutes: number;
  /**
   * 是否检测并在缺失时安装 Windows 前置组件。
   *
   * 官方手册明确：**命令行安装不会自动装前置组件**
   *（.NET 4.8 / Visual C++ 可再发行 / WebView2 / VBA）。
   * 缺失的后果是「装上了却跑不起来」。
   */
  install_prerequisites: boolean;
  /** 单个前置组件安装超时（分钟）。.NET 4.8 安装包有 112 MB。 */
  prerequisite_timeout_minutes: number;
  polling: PollingConfig;
  popup: PopupConfig;
}

/** `[network]` */
export interface NetworkConfig {
  /** 安装期间禁用网络 */
  disable_network: boolean;
  /** 安装后恢复网络 */
  restore_network_after: boolean;
  /** `netsh` 或 `powershell` */
  adapter_disable_method: string;
}

/** `[process]` */
export interface ProcessConfig {
  kill_sw_processes: boolean;
  /** 进程匹配关键词（正则，不区分大小写；`|` 分隔多个） */
  kill_pattern: string;
  /** 是否把匹配范围扩展到进程完整路径与命令行（默认 true，较慢） */
  match_command_line: boolean;
  /** 优雅退出等待时间，单位分钟 */
  kill_timeout_minutes: number;
  /** 超时后强制终止 */
  force_kill_after_timeout: boolean;
}

/** `[flexnet]` */
export interface FlexnetConfig {
  /** 等待 server_install.bat 完成的最长时间，单位分钟 */
  server_install_timeout_minutes: number;
  /** 服务状态轮询间隔，单位分钟 */
  server_check_interval_minutes: number;
  /** 是否先运行 server_remove.bat */
  remove_old_server_first: boolean;
}

/** `[workdir]` */
export interface WorkdirConfig {
  /** 安装完成后清理临时文件 */
  cleanup_temp_after: boolean;
  /** 临时目录，留空则用系统默认 */
  temp_dir: string;
}

/** `[sevenzip]` */
export interface SevenZipConfig {
  /** 7z.exe 路径，留空则自动查找（配置 → 打包资源 → 下载缓存 → 已安装 → PATH） */
  exe_path: string;
  /** 单次解压超时，单位分钟 */
  extract_timeout_minutes: number;
  /** 全部查找失败时，是否自动从 download_url 获取 7z */
  auto_download: boolean;
  /** 自动下载地址 */
  download_url: string;
  /** 自动下载超时，单位分钟 */
  download_timeout_minutes: number;
}

/**
 * `[antivirus]` —— 安装前的安全软件处置。
 *
 * Defender 与多数第三方杀软会把 `_SolidSQUAD_` 里的补丁判为威胁并删除，
 * 所以编排在**解压之前**先处置杀软。
 */
export interface AntivirusConfig {
  /** 是否启用本阶段 */
  enabled: boolean;
  /** 是否先用 SecurityCenter2 检测并告知用户 */
  detect: boolean;
  /** 是否移除 Windows Defender */
  remove_defender: boolean;
  /** windows-defender-remover 的 script 目录；留空自动查找 */
  defender_tool_path: string;
  /** 删除 Defender 后是否提示「必须重启」 */
  warn_reboot_after_removal: boolean;
  /**
   * 第三方杀软处置方式：
   *
   * - `prompt` 只检测并提示用户手动从托盘退出
   * - `terminate` 额外终止其进程 / 停止其服务
   * - `uninstall` 再额外尝试按其 UninstallString 静默卸载
   */
  third_party_mode: string;
  /** 处置后等待生效的时间，单位分钟 */
  settle_minutes: number;
  /** 单条处置命令超时，单位分钟 */
  command_timeout_minutes: number;
}

/** 完整配置树。 */
export interface Config {
  general: GeneralConfig;
  download: DownloadConfig;
  remote_config: RemoteConfigConfig;
  install: InstallConfig;
  network: NetworkConfig;
  process: ProcessConfig;
  flexnet: FlexnetConfig;
  workdir: WorkdirConfig;
  sevenzip: SevenZipConfig;
  antivirus: AntivirusConfig;
}

/** 深拷贝一份配置（避免 `$state` 代理被后端或基线共享）。 */
export function cloneConfig(config: Config): Config {
  return JSON.parse(JSON.stringify(config)) as Config;
}

/** 未连接后端时使用的空配置（所有字段取零值，等待 `get_config` 填充）。 */
export function emptyConfig(): Config {
  return {
    general: { language: 'zh-CN', theme: 'endfield' },
    download: {
      url: '',
      multipart_urls: [],
      multipart_concat: false,
      concurrent_files: 4,
      probe_filenames: true,
      threads: 32,
      checksum_sha256: '',
      retry_count: 3,
      retry_backoff_minutes: 0.5,
      connect_timeout_minutes: 1,
      read_timeout_minutes: 5,
    },
    remote_config: {
      url: '',
      auto_fetch_on_start: false,
      fetch_timeout_minutes: 1,
    },
    install: {
      install_drive: 'C',
      install_path: '',
      components_whitelist: ['SolidWorks', 'AddIns', 'SolidWorksToolbox'],
      force_msiexec: true,
      install_switches: [],
      serial_number: '',
      toolbox_folder: 'C:\\SOLIDWORKS_Data',
      language_pack: 'chinese-simplified',
      language_pack_delay_minutes: 0,
      install_prerequisites: true,
      prerequisite_timeout_minutes: 20,
      polling: {
        poll_interval_minutes: 0.5,
        timeout_minutes: 240,
        log_check_enabled: true,
        process_check_enabled: true,
        file_check_enabled: true,
      },
      popup: {
        scan_interval_ms: 500,
        auto_click_confirm: true,
        auto_click_yes: true,
        auto_click_no: false,
        popup_timeout_minutes: 5,
      },
    },
    network: {
      disable_network: true,
      restore_network_after: true,
      adapter_disable_method: 'netsh',
    },
    process: {
      kill_sw_processes: true,
      kill_pattern:
        '.*solidwork.*|.*sldworks.*|.*sw_dn.*|.*sidworks.*|.*flexnet.*|.*lmgrd.*',
      match_command_line: true,
      kill_timeout_minutes: 2,
      force_kill_after_timeout: true,
    },
    flexnet: {
      server_install_timeout_minutes: 5,
      server_check_interval_minutes: 0.5,
      remove_old_server_first: true,
    },
    workdir: { cleanup_temp_after: true, temp_dir: '' },
    sevenzip: {
      exe_path: '',
      extract_timeout_minutes: 30,
      auto_download: true,
      download_url: 'https://github.com/ip7z/7zip/releases/latest/download/7zr.exe',
      download_timeout_minutes: 3,
    },
    antivirus: {
      enabled: true,
      detect: true,
      remove_defender: true,
      defender_tool_path: '',
      warn_reboot_after_removal: true,
      third_party_mode: 'terminate',
      settle_minutes: 0.5,
      command_timeout_minutes: 5,
    },
  };
}

// ===========================================================================
// 二、运行时事件与快照
// ===========================================================================

export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'success';

export type ErrorKind =
  | 'environment'
  | 'download'
  | 'extract'
  | 'install'
  | 'post_install'
  | 'cancelled'
  | 'internal';

/** 前端向导展示的 9 个逻辑步骤。 */
export type StepId =
  | 'environment'
  | 'acquire'
  | 'network'
  | 'extract'
  | 'registry'
  | 'mount_iso'
  | 'install'
  | 'verify'
  | 'finalize';

/** `Channel<InstallEvent>` 的消息体：`{ type, data }`。 */
export type InstallEvent =
  | {
      type: 'step_changed';
      data: {
        step: number;
        total: number;
        id: StepId;
        name: string;
        en_name: string;
        note: string;
      };
    }
  | {
      type: 'download_progress';
      data: {
        bytes_done: number;
        bytes_total: number;
        speed_bps: number;
        chunks_done: number;
        chunks_total: number;
      };
    }
  | {
      type: 'extract_progress';
      data: { percent: number; current_file: string; pass: number };
    }
  | {
      type: 'download_file_status';
      data: {
        index: number;
        total: number;
        name: string;
        bytes_done: number;
        bytes_total: number;
        speed_bps: number;
        phase: 'resolving' | 'downloading' | 'done' | 'failed' | 'skipped' | string;
        detail: string;
      };
    }
  | {
      type: 'install_progress';
      data: { percent: number; message: string };
    }
  | {
      type: 'log_message';
      data: { level: LogLevel; message: string; timestamp: string };
    }
  | {
      type: 'popup_detected';
      data: { title: string; action: string };
    }
  | {
      type: 'error';
      data: { kind: ErrorKind; message: string; recoverable: boolean };
    }
  | {
      type: 'environment_check';
      data: {
        username: string;
        computer_name: string;
        username_ascii: boolean;
        computer_ascii: boolean;
        elevated: boolean;
        free_space_gb: number;
      };
    }
  | {
      type: 'antivirus_report';
      data: {
        detected: SecurityProduct[];
        outcomes: AvOutcome[];
        /** 需要用户手动从系统托盘退出的软件名 */
        manual_required: string[];
        defender_removed: boolean;
        reboot_required: boolean;
        summary: string;
      };
    }
  | {
      type: 'estimated_time_remaining';
      data: { seconds: number };
    }
  | {
      type: 'completed';
      data: { success: boolean; message: string };
    };

/** `get_install_status` 的返回结构。 */
export interface InstallStatus {
  running: boolean;
  cancelled: boolean;
  finished: boolean;
  success: boolean;
  /** 1 基的当前步骤序号（1~9） */
  current_step: number;
  total_steps: number;
  step_id: StepId | null;
  percent: number;
  message: string;
  started_at: string | null;
  finished_at: string | null;
  popups_handled: number;
  network_disabled: boolean;
  iso_mounted: boolean;
  /** 已完成的逻辑步骤下标（0 基） */
  completed_steps: number[];
  eta_seconds: number | null;
  /** 需要手动退出的安全软件数量 */
  antivirus_manual_required: number;
  /** 是否移除了 Windows Defender */
  antivirus_defender_removed: boolean;
  /** 是否提示需要重启 */
  reboot_required: boolean;
}

/** `env_check` 的返回结构。 */
export interface EnvSnapshot {
  username: string;
  computer_name: string;
  username_ascii: boolean;
  computer_ascii: boolean;
  elevated: boolean;
  temp_dir: string;
  app_data_dir: string;
  config_path: string;
  config_exists: boolean;
  free_space_gb: number;
  /** 7z 解压程序的检测结果 */
  seven_zip: SevenZipStatus;
}

/** 7z 解压程序的可用性快照。 */
export interface SevenZipStatus {
  /** 程序路径（`PATH` 回退时为裸名 `7z`） */
  program: string;
  /** 来源标签：配置指定 / 随程序打包 / 自动下载缓存 / 已安装的 7-Zip / 第三方兼容实现 / 系统 PATH */
  source: string;
  /** 是否真的可用 */
  available: boolean;
  /** 自动下载开关 */
  auto_download: boolean;
  /** 自动下载地址 */
  download_url: string;
}

/** `export_config` 的返回结构。 */
export interface ExportOutcome {
  /** 实际写出的完整路径 */
  path: string;
  /** 字节数 */
  bytes: number;
  /** 行数 */
  lines: number;
  /** 是否因为用户取消对话框而落到默认目录 */
  used_fallback: boolean;
}

/** 检测到的一个安全软件。 */
export interface SecurityProduct {
  /** 显示名，例如 `Windows Defender`、`Kaspersky` */
  display_name: string;
  /** `productState` 原始值 */
  product_state: number;
  /** 是否处于「开启且实时防护生效」状态 */
  enabled: boolean;
  /** 定义是否最新 */
  up_to_date: boolean;
  /** 是否为 Windows Defender（内置） */
  is_defender: boolean;
  /** 厂商注册的路径 */
  exe_path: string;
}

/** 一个安全软件的处置结论。 */
export interface AvOutcome {
  display_name: string;
  is_defender: boolean;
  /** 处置动作简述，例如「已终止 2 个进程」 */
  action: string;
  /** 是否已确认不再生效 */
  resolved: boolean;
  /** 需要用户手动处理时给出的具体指引 */
  manual_hint: string;
}
