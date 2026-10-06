/**
 * 配置状态存储（Svelte 5 runes）。
 *
 * 负责：加载、编辑、保存、重置、拉取远程配置，以及本地区压缩包选择。
 * 未保存的修改会以 `dirty` 标记暴露给设置页。
 */

import {
  BackendError,
  envCheck,
  exportConfig as exportConfigCommand,
  fetchRemoteConfig,
  getConfig,
  isTauriHost,
  logUiEvent,
  pickArchive,
  pickArchiveFolder,
  resetConfig,
  saveConfig,
  setLocalArchive as setLocalArchiveCommand,
} from '$lib/api/commands';
import type { Config, EnvSnapshot } from '$lib/api/types';
import { cloneConfig, emptyConfig } from '$lib/api/types';

export const config = $state<Config>(emptyConfig());
export const environments = $state<{
  snapshot: EnvSnapshot | null;
  loading: boolean;
  error: string | null;
}>({ snapshot: null, loading: false, error: null });

export const configRuntime = $state<{
  loading: boolean;
  saving: boolean;
  fetching: boolean;
  resetting: boolean;
  /** 是否有未保存的修改 */
  dirty: boolean;
  /** 最近一次操作的结果提示 */
  notice: { kind: 'ok' | 'error' | 'info'; text: string } | null;
  /** 最近一次保存/加载的时间戳 */
  lastSyncedAt: string | null;
  /** 正在等待文件/文件夹对话框或 7z 验证。 */
  selectingArchive: boolean;
  /** 本地模式选中的压缩包路径（null = 下载模式） */
  localArchive: string | null;
  /** 默认配置文本（“查看默认值”抽屉） */
  defaultToml: string | null;
  hostAvailable: boolean;
}>({
  loading: false,
  saving: false,
  fetching: false,
  resetting: false,
  dirty: false,
  notice: null,
  lastSyncedAt: null,
  selectingArchive: false,
  localArchive: null,
  defaultToml: null,
  hostAvailable: isTauriHost(),
});

/** 保存一份用于脏检查的基线。 */
let baseline = JSON.stringify(emptyConfig());

export function markClean(): void {
  baseline = JSON.stringify(config);
  configRuntime.dirty = false;
}

/** 与基线比对，更新 `dirty`（在输入变更后调用）。 */
export function markDirty(): void {
  configRuntime.dirty = JSON.stringify(config) !== baseline;
}

export function setNotice(kind: 'ok' | 'error' | 'info', text: string): void {
  configRuntime.notice = { kind, text };
}

/** 记录原生对话框或 7z 验证长时间没有返回，帮助定位“点击后无反应”。 */
function watchArchiveSelection(kind: string): () => void {
  const timer = window.setTimeout(() => {
    void logUiEvent(`状态：${kind}超过 30 秒仍无响应，可能正在等待对话框或 7z 验证`);
  }, 30_000);
  return () => window.clearTimeout(timer);
}

/** 用后端返回值整体替换本地配置。 */
function replaceConfig(next: Config): void {
  Object.assign(config, cloneConfig(next));
  markClean();
}

/** 加载当前生效配置。 */
export async function loadConfig(): Promise<void> {
  void logUiEvent('请求：加载配置');
  configRuntime.loading = true;
  configRuntime.notice = null;
  try {
    const loaded = await getConfig();
    replaceConfig(loaded);
    configRuntime.lastSyncedAt = new Date().toISOString();
    configRuntime.hostAvailable = true;
    void logUiEvent('响应：配置加载成功');
  } catch (error) {
    configRuntime.hostAvailable = isTauriHost();
    setNotice(
      'error',
      error instanceof BackendError
        ? error.message
        : `加载配置失败: ${error instanceof Error ? error.message : String(error)}`,
    );
    void logUiEvent(`响应：配置加载失败；${error instanceof Error ? error.message : String(error)}`);
  } finally {
    configRuntime.loading = false;
  }
}

/** 保存配置。 */
export async function persistConfig(): Promise<boolean> {
  void logUiEvent('点击：保存配置');
  configRuntime.saving = true;
  configRuntime.notice = null;
  try {
    await saveConfig(cloneConfig(config));
    markClean();
    configRuntime.lastSyncedAt = new Date().toISOString();
    setNotice('ok', '配置已写入 {app_data_dir}/config.toml');
    void logUiEvent('响应：配置保存成功');
    return true;
  } catch (error) {
    setNotice(
      'error',
      error instanceof BackendError
        ? error.message
        : `保存失败: ${error instanceof Error ? error.message : String(error)}`,
    );
    void logUiEvent(`响应：配置保存失败；${error instanceof Error ? error.message : String(error)}`);
    return false;
  } finally {
    configRuntime.saving = false;
  }
}

/** 重置为嵌入二进制的默认配置。 */
export async function resetToDefault(): Promise<void> {
  void logUiEvent('点击：重置默认配置');
  configRuntime.resetting = true;
  configRuntime.notice = null;
  try {
    replaceConfig(await resetConfig());
    configRuntime.lastSyncedAt = new Date().toISOString();
    setNotice('ok', '已恢复嵌入式默认配置');
    void logUiEvent('响应：已恢复默认配置');
  } catch (error) {
    setNotice(
      'error',
      error instanceof BackendError
        ? error.message
        : `重置失败: ${error instanceof Error ? error.message : String(error)}`,
    );
    void logUiEvent(`响应：恢复默认配置失败；${error instanceof Error ? error.message : String(error)}`);
  } finally {
    configRuntime.resetting = false;
  }
}

/**
 * 拉取远程配置。
 *
 * URL 取自 `config[remote_config].url`；只更新表单，不自动落盘——
 * 用户确认后点「保存配置」才写入。
 */
export async function pullRemoteConfig(urlOverride?: string): Promise<void> {
  const url = (urlOverride ?? config.remote_config.url).trim();
  void logUiEvent(`点击：拉取远程配置；URL=${url || '空'}`);
  if (!url) {
    setNotice('error', '远程配置 URL 为空，请先在 [remote_config] 中填写');
    void logUiEvent('响应：远程配置未执行，URL 为空');
    return;
  }

  configRuntime.fetching = true;
  configRuntime.notice = null;
  try {
    const remote = await fetchRemoteConfig(url);
    replaceConfig(remote);
    configRuntime.dirty = true;
    setNotice('info', '远程配置已载入表单，确认后点击「保存配置」写入本地');
    void logUiEvent('响应：远程配置载入成功，等待用户保存');
  } catch (error) {
    setNotice(
      'error',
      error instanceof BackendError
        ? error.message
        : `拉取失败: ${error instanceof Error ? error.message : String(error)}`,
    );
    void logUiEvent(`响应：拉取远程配置失败；${error instanceof Error ? error.message : String(error)}`);
  } finally {
    configRuntime.fetching = false;
  }
}

/** 打开原生对话框选择本地压缩包。 */
export async function chooseLocalArchive(): Promise<void> {  configRuntime.notice = null;
  void logUiEvent('点击：选择本地压缩包');
  configRuntime.selectingArchive = true;
  setNotice('info', '正在打开文件选择并准备 7z 验证，请稍候…');
  const stopWatchdog = watchArchiveSelection('本地压缩包选择');
  try {
    const selected = await pickArchive();
    if (selected) {
      await setLocalArchiveCommand(selected);
      configRuntime.localArchive = selected;
      setNotice('info', `已选择本地压缩包：${selected}`);
      void logUiEvent(`响应：已选择本地压缩包 ${selected}`);
    } else {
      setNotice('info', '已取消选择本地压缩包');
      void logUiEvent('响应：取消选择本地压缩包');
    }
  } catch (error) {
    setNotice(
      'error',
      error instanceof BackendError
        ? error.message
        : `选择文件失败: ${error instanceof Error ? error.message : String(error)}`,
    );
    void logUiEvent(`响应：选择本地压缩包失败；${error instanceof Error ? error.message : String(error)}`);
  } finally {
    stopWatchdog();
    configRuntime.selectingArchive = false;
  }
}

/** 打开原生文件夹对话框，选择完整的本地分卷目录。 */
export async function chooseLocalArchiveFolder(): Promise<void> {
  configRuntime.notice = null;
  void logUiEvent('点击：选择本地分卷文件夹');
  configRuntime.selectingArchive = true;
  setNotice('info', '正在打开文件夹选择并准备 7z 验证，请稍候…');
  const stopWatchdog = watchArchiveSelection('本地分卷文件夹选择');
  try {
    const selected = await pickArchiveFolder();
    if (selected) {
      await setLocalArchiveCommand(selected);
      configRuntime.localArchive = selected;
      setNotice('info', `已选择本地分卷文件夹：${selected}`);
      void logUiEvent(`响应：已选择本地分卷文件夹 ${selected}`);
    } else {
      setNotice('info', '已取消选择本地分卷文件夹');
      void logUiEvent('响应：取消选择本地分卷文件夹');
    }
  } catch (error) {
    setNotice(
      'error',
      error instanceof BackendError
        ? error.message
        : `选择分卷文件夹失败: ${error instanceof Error ? error.message : String(error)}`,
    );
    void logUiEvent(`响应：选择分卷文件夹失败；${error instanceof Error ? error.message : String(error)}`);
  } finally {
    stopWatchdog();
    configRuntime.selectingArchive = false;
  }
}

/** 清除本地压缩包选择，回到下载模式。 */
export function clearLocalArchive(): void {
  configRuntime.localArchive = null;
  void setLocalArchiveCommand(null);
  void logUiEvent('点击：切回下载模式；已清除本地路径');
  void logUiEvent('响应：已切回下载模式');
  setNotice('info', '已切回下载模式');
}

/**
 * 导出当前配置为 TOML 文件。
 *
 * 后端先做「磁盘现有配置 ← 表单配置」的结构保留合并，再与默认配置合并：
 * 表单里留空的项保持磁盘上的现有值，不会被清空；注释保留、新字段补齐。
 *
 * 导出位置：原生保存对话框；取消则落到 `{app_data_dir}/exports/`。
 */
export async function exportCurrentConfig(): Promise<void> {
  void logUiEvent('点击：导出当前配置');
  configRuntime.notice = null;
  try {
    const outcome = await exportConfigCommand(cloneConfig(config));
    setNotice(
      'ok',
      outcome.used_fallback
        ? `已取消选择位置，配置导出到 ${outcome.path}（${outcome.lines} 行）`
        : `配置已导出到 ${outcome.path}（${outcome.lines} 行 / ${outcome.bytes} 字节）`,
    );
    void logUiEvent(`响应：配置导出完成；${outcome.path}`);
  } catch (error) {
    setNotice(
      'error',
      error instanceof BackendError
        ? error.message
        : `导出失败: ${error instanceof Error ? error.message : String(error)}`,
    );
    void logUiEvent(`响应：配置导出失败；${error instanceof Error ? error.message : String(error)}`);
  }
}

/** 加载环境快照。 */
export async function loadEnvironment(): Promise<void> {  environments.loading = true;
  void logUiEvent('点击：重新检测运行环境');
  environments.error = null;
  try {
    environments.snapshot = await envCheck();
    void logUiEvent('响应：运行环境检测完成');
  } catch (error) {
    environments.error = error instanceof Error ? error.message : String(error);
    void logUiEvent(`响应：运行环境检测失败；${environments.error}`);
  } finally {
    environments.loading = false;
  }
}

/** 当前是否存在需要注意的环境问题。 */
export function environmentWarning(snapshot: EnvSnapshot | null): string | null {
  if (!snapshot) return null;
  if (!snapshot.computer_ascii) {
    return `计算机名「${snapshot.computer_name}」含非 ASCII 字符：FlexNet 许可服务无法启动，请改名并重启。`;
  }
  if (!snapshot.username_ascii) {
    return `用户名「${snapshot.username}」含非 ASCII 字符：建议改用英文账户。`;
  }
  if (!snapshot.elevated) {
    return '当前未以管理员身份运行：注册表导入、网卡禁用与服务安装都会失败。';
  }
  if (snapshot.free_space_gb >= 0 && snapshot.free_space_gb < 20) {
    return `目标盘可用空间仅 ${snapshot.free_space_gb.toFixed(1)} GB，完整安装通常需要 20 GB 以上。`;
  }
  return null;
}
