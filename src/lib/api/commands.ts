/**
 * `invoke()` 封装。
 *
 * 前端只通过本模块与 Rust 后端通信；组件不直接 import `@tauri-apps/api`，
 * 这样在有/无 Tauri 宿主两种运行环境下都能保持同一套调用签名。
 * `Channel<T>` 流式进度见 `./channels.ts`，类型定义见 `./types.ts`。
 */

import { invoke as tauriInvoke } from '@tauri-apps/api/core';

import type {
  Config,
  EnvSnapshot,
  ExportOutcome,
  InstallStatus,
  SecurityProduct,
} from './types';

// ---------------------------------------------------------------------------
// 运行时探测
// ---------------------------------------------------------------------------

/** 是否运行在 Tauri 宿主中（浏览器里 `__TAURI_INTERNALS__` 不存在）。 */
export function isTauriHost(): boolean {
  if (typeof window === 'undefined') return false;
  return '__TAURI_INTERNALS__' in (window as unknown as Record<string, unknown>);
}

/** 统一错误类型：把后端的字符串错误包装成可判断的对象。 */
export class BackendError extends Error {
  constructor(
    message: string,
    readonly command: string,
  ) {
    super(message);
    this.name = 'BackendError';
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriHost()) {
    throw new BackendError(
      `未检测到 Tauri 宿主，无法调用命令 ${command}。请通过 \`npm run tauri:dev\` 启动桌面应用。`,
      command,
    );
  }
  try {
    return await tauriInvoke<T>(command, args);
  } catch (error) {
    const message =
      typeof error === 'string'
        ? error
        : error instanceof Error
          ? error.message
          : JSON.stringify(error);
    throw new BackendError(message, command);
  }
}

/** 把前端按钮/交互写入启动会话日志。 */
export async function logUiEvent(message: string): Promise<void> {
  try {
    await call<void>('log_ui_event', { message });
  } catch {
    // 日志记录不能阻塞或打断用户正在执行的操作。
  }
}

/** 同步设置当前部署使用的本地压缩包路径。 */
export function setLocalArchive(path: string | null): Promise<void> {
  return call<void>('set_local_archive', { path });
}

// ---------------------------------------------------------------------------
// 命令封装（与 src-tauri/src/commands.rs 一一对应）
// ---------------------------------------------------------------------------

/** 读取当前生效配置（默认配置 ← 用户配置，结构保留合并）。 */
export function getConfig(): Promise<Config> {
  return call<Config>('get_config');
}

/** 保存配置到 `{app_data_dir}/config.toml`。 */
export function saveConfig(config: Config): Promise<void> {
  return call<void>('save_config', { config });
}

/** 重置为嵌入二进制的默认配置。 */
export function resetConfig(): Promise<Config> {
  return call<Config>('reset_config');
}

/** 从远程 URL 拉取配置并与默认配置合并（不自动落盘）。 */
export function fetchRemoteConfig(url: string): Promise<Config> {
  return call<Config>('fetch_remote_config', { url });
}

/** 读取嵌入二进制的默认配置文本（设置页「查看默认值」）。 */
export function getDefaultConfig(): Promise<string> {
  return call<string>('get_default_config');
}

/** 环境快照：用户名、主机名、ASCII 判定、管理员权限、目录、可用空间。 */
export function envCheck(): Promise<EnvSnapshot> {
  return call<EnvSnapshot>('env_check');
}

/** 打开原生文件对话框选择本地压缩包；取消时返回 `null`。 */
export function pickArchive(): Promise<string | null> {
  return call<string | null>('pick_archive');
}

/** 选择包含 `.001/.002/...` 分卷的文件夹；取消时返回 `null`。 */
export function pickArchiveFolder(): Promise<string | null> {
  return call<string | null>('pick_archive_folder');
}

/** 请求取消安装。 */
export function cancelInstall(): Promise<void> {
  return call<void>('cancel_install');
}

/** 读取安装状态快照。 */
export function getInstallStatus(): Promise<InstallStatus> {
  return call<InstallStatus>('get_install_status');
}

/**
 * 把日志文本导出到 `{app_data_dir}/logs/`，返回写入的完整路径。
 *
 * 由 Rust 落盘而非浏览器下载：WebView2 对 `blob:` 下载支持有限，
 * 且写入应用数据目录更便于事后查找。
 */
export function exportLogs(contents: string, label = 'install'): Promise<string> {
  return call<string>('export_logs', { contents, label });
}

/**
 * 导出当前配置为 TOML 文件。
 *
 * **留空即保留**：后端会先把传入配置与磁盘上的用户配置做结构保留合并，
 * 再与默认配置合并。因此未涉及的项保持现有值，不会被清空；
 * 已有注释保留，缺失的新字段自动补齐并带上默认注释。
 *
 * 导出目标由原生保存对话框决定；取消对话框时自动落到
 * `{app_data_dir}/exports/`，此时返回值的 `used_fallback` 为 `true`。
 */
export function exportConfig(
  config: Config,
  suggestedName?: string,
): Promise<ExportOutcome> {
  return call<ExportOutcome>('export_config', {
    config,
    suggested_name: suggestedName ?? null,
  });
}

/**
 * 重新检测安全软件（只读，不执行处置）。
 *
 * 用户在系统托盘手动退出杀软后调用，确认是否还有软件注册为「实时防护开启」。
 * 返回空数组表示已全部退出。
 */
export function recheckAntivirus(): Promise<SecurityProduct[]> {
  return call<SecurityProduct[]>('recheck_antivirus');
}
