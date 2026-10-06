/**
 * 安装状态存储（Svelte 5 runes）。
 *
 * 唯一真相来源是 Rust 后端的 `InstallStatusCell`；本模块把 `Channel<InstallEvent>`
 * 推送的事件折叠成 UI 直接可用的结构，并保留日志环形缓冲。
 */

import {
  cancelInstall,
  getInstallStatus,
  isTauriHost,
  logUiEvent,
  recheckAntivirus as recheckAntivirusCommand,
} from '$lib/api/commands';
import { startInstall as startInstallCommand } from '$lib/api/channels';
import { configRuntime } from '$lib/stores/config.svelte.ts';
import type {
  AvOutcome,
  Config,
  ErrorKind,
  InstallEvent,
  LogLevel,
  SecurityProduct,
  StepId,
} from '$lib/api/types';

/** 步骤追踪器使用的 9 个逻辑步骤定义。 */
export interface StepDefinition {
  id: StepId;
  /** 展示编号 01~09 */
  index: number;
  name: string;
  enName: string;
  note: string;
}

export const STEPS: StepDefinition[] = [
  {
    id: 'environment',
    index: 1,
    name: '环境检测',
    enName: 'ENVIRONMENT PROBE',
    note: '校验用户名/计算机名 ASCII 与管理员权限',
  },
  {
    id: 'acquire',
    index: 2,
    name: '获取压缩包',
    enName: 'ARCHIVE ACQUISITION',
    note: '本地选择或分块多线程下载',
  },
  {
    id: 'network',
    index: 3,
    name: '禁用网络',
    enName: 'NETWORK ISOLATION',
    note: '禁用非虚拟网卡，Drop 兜底恢复',
  },
  {
    id: 'extract',
    index: 4,
    name: '两阶段解压',
    enName: 'TWO-STAGE EXTRACTION',
    note: '主包解压 + _SolidSQUAD_ 二次解压',
  },
  {
    id: 'registry',
    index: 5,
    name: '导入注册表',
    enName: 'REGISTRY INJECTION',
    note: '导入 _SolidSQUAD_ 下全部 .reg',
  },
  {
    id: 'mount_iso',
    index: 6,
    name: '挂载镜像',
    enName: 'IMAGE MOUNT',
    note: 'Mount-DiskImage 挂载并定位 setup.exe',
  },
  {
    id: 'install',
    index: 7,
    name: '静默安装',
    enName: 'SILENT DEPLOYMENT',
    note: 'StartSWInstall /install /now 或 msiexec 回退',
  },
  {
    id: 'verify',
    index: 8,
    name: '轮询检测',
    enName: 'COMPLETION POLLING',
    note: '日志 / 进程 / 可执行文件三重轮询',
  },
  {
    id: 'finalize',
    index: 9,
    name: '收尾恢复',
    enName: 'TEARDOWN & RESTORE',
    note: '恢复网络、卸载镜像、清理临时目录',
  },
];

/** 一行日志。 */
export interface LogEntry {
  id: number;
  timestamp: string;
  level: LogLevel;
  message: string;
}

/** 弹窗处理记录。 */
export interface PopupRecord {
  id: number;
  timestamp: string;
  title: string;
  action: string;
}

/** 下载通道状态。 */
export interface DownloadRuntime {
  bytesDone: number;
  bytesTotal: number;
  speedBps: number;
  chunksDone: number;
  chunksTotal: number;
}

/** 生成器式环形缓冲：只保留最近 N 条日志，避免长时间安装吃满内存。 */
const LOG_CAPACITY = 2000;

function createInitialState() {
  return {
    running: false,
    finished: false,
    success: false,
    cancelled: false,
    /** 当前步骤的 1 基编号（1~9）；0 表示尚未开始 */
    currentStep: 0,
    totalSteps: STEPS.length,
    currentStepId: null as StepId | null,
    percent: 0,
    message: '待命',
    etaSeconds: null as number | null,
    startedAt: null as string | null,
    finishedAt: null as string | null,
    popupsHandled: 0,
    networkDisabled: false,
    isoMounted: false,
    completedSteps: [] as number[],
    /** 当前活动的进度通道：download / extract / install */
    activeChannel: 'idle' as 'idle' | 'download' | 'extract' | 'install',
    download: {
      bytesDone: 0,
      bytesTotal: 0,
      speedBps: 0,
      chunksDone: 0,
      chunksTotal: 0,
    } satisfies DownloadRuntime,
    extract: { percent: 0, currentFile: '', pass: 0 },
    antivirusAcknowledged: false,
    lastError: null as { kind: ErrorKind; message: string; recoverable: boolean } | null,
    /** 需要用户注意的环境问题（中文用户名/计算机名/缺少管理员权限） */
    environmentAlert: null as string | null,
    /** 安全软件检测与处置报告 */
    antivirus: null as {
      detected: SecurityProduct[];
      outcomes: AvOutcome[];
      /** 需要用户手动从系统托盘退出的软件名 */
      manualRequired: string[];
      defenderRemoved: boolean;
      rebootRequired: boolean;
      summary: string;
    } | null,
    /**
     * 逐文件下载状态清单（分片 / 多文件下载）。
     *
     * 按下标保存，`DownloadFileStatus` 事件按 `index` 更新对应槽位。
     */
    downloadFiles: [] as DownloadFileState[],
  };
}

/** 单个文件的下载状态。 */
export interface DownloadFileState {
  index: number;
  total: number;
  name: string;
  bytesDone: number;
  bytesTotal: number;
  speedBps: number;
  phase: string;
  detail: string;
}

/** 该文件是否处于「进行中」阶段。 */
export function isFileActive(phase: string): boolean {
  return phase === 'resolving' || phase === 'downloading';
}

export type InstallState = ReturnType<typeof createInitialState>;

export const installState = $state<InstallState>(createInitialState());

export const logs = $state<LogEntry[]>([]);
export const popups = $state<PopupRecord[]>([]);

let logSeq = 0;
let popupSeq = 0;

function pushLog(level: LogLevel, message: string, timestamp: string): void {
  logSeq += 1;
  logs.push({ id: logSeq, timestamp, level, message });
  if (logs.length > LOG_CAPACITY) {
    logs.splice(0, logs.length - LOG_CAPACITY);
  }
}

function pushPopup(title: string, action: string): void {
  popupSeq += 1;
  popups.push({
    id: popupSeq,
    timestamp: new Date().toISOString(),
    title,
    action,
  });
  if (popups.length > 200) {
    popups.splice(0, popups.length - 200);
  }
}

function resetState(): void {
  Object.assign(installState, createInitialState());
}

/** 把一条 Channel 事件折叠进状态。 */
export function applyEvent(event: InstallEvent): void {
  if (event.type === 'step_changed') {
    const step = event.data.step;
    installState.currentStep = step;
    installState.totalSteps = event.data.total;
    installState.currentStepId = event.data.id;
    installState.message = event.data.name;
    // 之前的步骤全部记为已完成。
    installState.completedSteps = Array.from({ length: Math.max(0, step - 1) }, (_, i) => i);
    return;
  }

  if (event.type === 'download_progress') {
    installState.activeChannel = 'download';
    installState.download = {
      bytesDone: event.data.bytes_done,
      bytesTotal: event.data.bytes_total,
      speedBps: event.data.speed_bps,
      chunksDone: event.data.chunks_done,
      chunksTotal: event.data.chunks_total,
    };
    if (event.data.bytes_total > 0) {
      installState.percent =
        (event.data.bytes_done / event.data.bytes_total) * 100;
    }
    return;
  }

  if (event.type === 'extract_progress') {
    installState.activeChannel = 'extract';
    installState.extract = {
      percent: event.data.percent,
      currentFile: event.data.current_file,
      pass: event.data.pass,
    };
    installState.message = `第 ${event.data.pass} 阶段解压`;
    return;
  }

  if (event.type === 'install_progress') {
    installState.activeChannel = 'install';
    installState.percent = event.data.percent;
    installState.message = event.data.message;
    return;
  }

  if (event.type === 'log_message') {
    pushLog(event.data.level, event.data.message, event.data.timestamp);
    return;
  }

  if (event.type === 'popup_detected') {
    installState.popupsHandled += 1;
    pushPopup(event.data.title, event.data.action);
    pushLog(
      'success',
      `弹窗「${event.data.title}」→ ${event.data.action}`,
      new Date().toISOString(),
    );
    return;
  }

  if (event.type === 'error') {
    installState.lastError = {
      kind: event.data.kind,
      message: event.data.message,
      recoverable: event.data.recoverable,
    };
    pushLog('error', event.data.message, new Date().toISOString());
    if (event.data.kind === 'environment') {
      installState.environmentAlert = event.data.message;
    }
    return;
  }

  if (event.type === 'environment_check') {
    if (!event.data.computer_ascii) {
      installState.environmentAlert = `计算机名「${event.data.computer_name}」包含非 ASCII 字符，FlexNet 许可服务将无法启动。`;
    } else if (!event.data.username_ascii) {
      installState.environmentAlert = `用户名「${event.data.username}」包含非 ASCII 字符，建议改用英文账户。`;
    } else if (!event.data.elevated) {
      installState.environmentAlert = '当前未以管理员身份运行，注册表与服务操作会失败。';
    }
    return;
  }

  if (event.type === 'download_file_status') {
    const { index, total, name, bytes_done, bytes_total, speed_bps, phase, detail } =
      event.data;

    // 首次收到某个 index 时按 total 展开清单，保证面板能显示全部槽位
    while (installState.downloadFiles.length < total) {
      const slot = installState.downloadFiles.length + 1;
      installState.downloadFiles.push({
        index: slot,
        total,
        name: '',
        bytesDone: 0,
        bytesTotal: 0,
        speedBps: 0,
        phase: 'pending',
        detail: '',
      });
    }

    const slot = installState.downloadFiles[index - 1];
    if (slot) {
      const previousName = slot.name;
      slot.index = index;
      slot.total = total;

      // 解析阶段可能先用 URL 占位；解析出真实名后要覆盖，并记一条日志，
      // 方便确认 `.aspx` 这类假后缀确实被剥掉了。
      if (name && name !== previousName) {
        if (previousName && !previousName.startsWith('http')) {
          pushLog(
            'info',
            `分片 ${index} 文件名解析：${previousName} → ${name}`,
            new Date().toISOString(),
          );
        }
        slot.name = name;
      }

      slot.bytesDone = bytes_done;
      slot.bytesTotal = bytes_total;
      slot.speedBps = speed_bps;
      slot.phase = phase;
      slot.detail = detail;
    }

    if (phase === 'failed') {
      pushLog(
        'error',
        `下载失败 ${name || `#${index}`}：${detail}`,
        new Date().toISOString(),
      );
    }
    return;
  }

  if (event.type === 'estimated_time_remaining') {
    installState.etaSeconds = event.data.seconds;
    return;
  }

  if (event.type === 'antivirus_report') {
    installState.antivirusAcknowledged = false;
    installState.antivirus = {
      detected: event.data.detected,
      outcomes: event.data.outcomes,
      manualRequired: event.data.manual_required,
      defenderRemoved: event.data.defender_removed,
      rebootRequired: event.data.reboot_required,
      summary: event.data.summary,
    };

    // 把关键结论也写进日志流，便于事后导出排查。
    pushLog(
      'info',
      `安全软件处置：${event.data.summary}`,
      new Date().toISOString(),
    );
    for (const item of event.data.outcomes) {
      pushLog(
        item.resolved ? 'success' : 'warn',
        `${item.display_name}：${item.action}`,
        new Date().toISOString(),
      );
    }
    if (event.data.manual_required.length > 0) {
      pushLog(
        'warn',
        `需要手动退出（系统托盘）后重试：${event.data.manual_required.join('、')}`,
        new Date().toISOString(),
      );
    }
    if (event.data.defender_removed && event.data.reboot_required) {
      pushLog(
        'warn',
        'Windows Defender 已移除，但需要**重启**才彻底生效；本程序不会自动重启。',
        new Date().toISOString(),
      );
    }
    return;
  }

  if (event.type === 'completed') {
    installState.finished = true;
    installState.success = event.data.success;
    installState.running = false;
    installState.activeChannel = 'idle';
    installState.etaSeconds = null;
    if (event.data.success) {
      installState.completedSteps = STEPS.map((_, i) => i);
      installState.percent = 100;
    }
    installState.message = event.data.message;
    return;
  }
}

/** 用后端快照覆盖本地状态（页面重新挂载时调用）。 */
export function hydrateFromStatus(
  status: Awaited<ReturnType<typeof getInstallStatus>>,
): void {
  installState.running = status.running;
  installState.finished = status.finished;
  installState.success = status.success;
  installState.cancelled = status.cancelled;
  installState.currentStep = status.current_step;
  installState.totalSteps = status.total_steps;
  installState.currentStepId = status.step_id;
  installState.percent = status.percent;
  installState.message = status.message;
  installState.startedAt = status.started_at;
  installState.finishedAt = status.finished_at;
  installState.popupsHandled = status.popups_handled;
  installState.networkDisabled = status.network_disabled;
  installState.isoMounted = status.iso_mounted;
  installState.completedSteps = status.completed_steps;
  installState.etaSeconds = status.eta_seconds;
}

/** 从后端同步一次状态快照（忽略后端不可用的情形）。 */
export async function refreshStatus(): Promise<void> {
  if (!isTauriHost()) return;
  try {
    hydrateFromStatus(await getInstallStatus());
  } catch {
    // 状态查询失败不阻塞界面：Channel 事件仍会持续更新。
  }
}

/**
 * 启动安装。
 *
 * 返回 `null` 表示成功，否则返回人类可读的错误信息。
 */
export async function startInstall(config: Config): Promise<string | null> {
  const selectedLocalArchive = configRuntime.localArchive;
  void logUiEvent(
    selectedLocalArchive
      ? `请求：开始自动部署；来源=本地路径 ${selectedLocalArchive}；跳过网络下载`
      : '请求：开始自动部署；来源=网络下载',
  );
  resetState();
  logs.splice(0, logs.length);
  popups.splice(0, popups.length);

  installState.running = true;
  installState.message = '正在启动部署流程…';
  installState.startedAt = new Date().toISOString();

  try {
    await startInstallCommand({
      config,
      localArchive: selectedLocalArchive,
      onEvent: applyEvent,
    });
    installState.running = false;
    if (!installState.finished) {
      installState.finished = true;
      installState.success = installState.lastError === null;
    }
    void logUiEvent(
      installState.success ? '响应：自动部署成功' : `响应：自动部署失败；${installState.lastError?.message ?? '未知错误'}`,
    );
    return installState.lastError?.message ?? null;
  } catch (error) {
    installState.running = false;
    installState.finished = true;
    installState.success = false;
    const message = error instanceof Error ? error.message : String(error);
    installState.message = message;
    installState.lastError = { kind: 'internal', message, recoverable: false };
    pushLog('error', message, new Date().toISOString());
    void logUiEvent(`响应：自动部署调用异常；${message}`);
    return message;
  }
}

/** 请求取消安装。 */
export async function requestCancel(): Promise<void> {
  void logUiEvent('点击：取消部署');
  if (!isTauriHost()) {
    void logUiEvent('响应：取消部署无效，当前不是 Tauri 宿主');
    return;
  }
  installState.cancelled = true;
  installState.message = '正在取消…';
  try {
    await cancelInstall();
    void logUiEvent('响应：取消请求已发送');
  } catch (error) {
    pushLog(
      'warn',
      `取消请求发送失败: ${error instanceof Error ? error.message : String(error)}`,
      new Date().toISOString(),
    );
    void logUiEvent(`响应：取消请求失败；${error instanceof Error ? error.message : String(error)}`);
  }
}

/** 清空日志与弹窗记录。 */
export function clearLogs(): void {
  logs.splice(0, logs.length);
  popups.splice(0, popups.length);
}

/**
 * 用户在系统托盘手动退出杀软后重新检测。
 *
 * 只读取系统状态、不重复执行移除动作；结果直接更新面板。
 */
export async function recheckAntivirus(): Promise<void> {
  void logUiEvent('点击：重新检测安全软件');
  try {
    const products = await recheckAntivirusCommand();
    const remaining = products.filter((product) => product.enabled);

    if (installState.antivirus) {
      // 已不再出现在 SecurityCenter2 里的，视为已退出。
      const names = new Set(products.map((p) => p.display_name.toLowerCase()));
      installState.antivirus = {
        ...installState.antivirus,
        detected: products,
        manualRequired: installState.antivirus.manualRequired.filter((name) =>
          names.has(name.toLowerCase()),
        ),
      };
    }

    pushLog(
      remaining.length === 0 ? 'success' : 'warn',
      remaining.length === 0
        ? '重新检测：SecurityCenter2 已无处于开启状态的安全软件'
        : `重新检测：仍有 ${remaining.length} 个安全软件处于开启状态（${remaining
            .map((p) => p.display_name)
            .join('、')}）`,
      new Date().toISOString(),
    );
    void logUiEvent(
      remaining.length === 0
        ? '响应：安全软件重检完成，未发现开启的软件'
        : `响应：安全软件重检完成，仍有 ${remaining.length} 个开启的软件`,
    );
  } catch (error) {
    pushLog(
      'error',
      `重新检测失败：${error instanceof Error ? error.message : String(error)}`,
      new Date().toISOString(),
    );
    void logUiEvent(`响应：安全软件重检失败；${error instanceof Error ? error.message : String(error)}`);
  }
}

/** 用户确认已手动关闭/卸载杀毒软件，允许继续当前流程。 */
export function acknowledgeAntivirusResolved(): void {
  installState.antivirusAcknowledged = true;
  if (installState.antivirus) {
    installState.antivirus = {
      ...installState.antivirus,
      manualRequired: [],
      summary: `${installState.antivirus.summary}；用户已确认手动处理完成`,
    };
  }
  pushLog('success', '用户确认：已手动关闭或卸载杀毒软件，将继续安装', new Date().toISOString());
  void logUiEvent('点击：我已手动解决杀毒软件；继续安装');
}

// ---------------------------------------------------------------------------
// 派生格式化工具
// ---------------------------------------------------------------------------

/** 字节数 → 人类可读（1024 进制）。 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** exponent;
  return `${value.toFixed(exponent === 0 ? 0 : value >= 100 ? 0 : 1)} ${units[exponent]}`;
}

/** 速度 → `12.3 MB/s`。 */
export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '—';
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** 秒数 → `1 小时 04 分` / `03:20`。 */
export function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) return '—';
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  if (hours > 0) return `${hours} 小时 ${String(minutes).padStart(2, '0')} 分`;
  return `${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
}

/** 百分比 → 一位小数。 */
export function formatPercent(value: number): string {
  if (!Number.isFinite(value)) return '0.0';
  return value.toFixed(1);
}

/** `HH:MM:SS` 时间戳（用于日志列）。 */
export function shortTime(timestamp: string): string {
  const parsed = new Date(timestamp);
  if (Number.isNaN(parsed.getTime())) return timestamp.slice(11, 19) || '--:--:--';
  return parsed.toTimeString().slice(0, 8);
}

/** 已运行时长 → `mm:ss` / `h:mm:ss`。 */
export function elapsedLabel(startedAt: string | null, now = Date.now()): string {
  if (!startedAt) return '—';
  const started = new Date(startedAt).getTime();
  if (Number.isNaN(started)) return '—';
  const total = Math.max(0, Math.round((now - started) / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
  }
  return `${String(minutes).padStart(2, '0')}:${String(seconds).padStart(2, '0')}`;
}
