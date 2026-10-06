/**
 * `Channel<T>` 流式进度封装。
 *
 * Tauri 2 的 `tauri::ipc::Channel<T>` 作为命令参数传入，在整次 IPC 调用期间
 * 保持打开：后端一边执行安装一边推送 `InstallEvent`，前端在 `await` 命令完成的
 * 同时按序收到全部消息。相比 `app.emit()`：
 *   · 端到端类型安全 —— 消息类型是命令签名的一部分；
 *   · 生命周期与命令绑定 —— 无需手动注册/清理监听器；
 *   · 消息保序 —— 高频进度不会被重排。
 */

import { Channel, invoke as tauriInvoke } from '@tauri-apps/api/core';

import { BackendError, isTauriHost } from './commands';
import type { Config, InstallEvent } from './types';

export interface StartInstallOptions {
  config: Config;
  localArchive: string | null;
  /** 每收到一条事件即回调一次（回调按后端发送顺序执行）。 */
  onEvent: (event: InstallEvent) => void;
}

/**
 * 启动安装并订阅流式进度。
 *
 * Promise 在安装流程结束后 resolve；期间所有事件通过 `onEvent` 送达。
 * 后端返回Err时抛出 `BackendError`。
 */
export async function startInstall({ config, localArchive, onEvent }: StartInstallOptions): Promise<void> {
  if (!isTauriHost()) {
    throw new BackendError(
      '未检测到 Tauri 宿主，无法启动安装。请通过 `npm run tauri:dev` 启动桌面应用。',
      'start_install',
    );
  }

  const channel = new Channel<InstallEvent>();
  channel.onmessage = (event) => {
    onEvent(event);
  };

  try {
    // 注意：channel 必须与 config 一起作为命名参数传入，
    // 参数名要与 Rust 命令签名 `start_install(channel, config)` 一致。
    await tauriInvoke<void>('start_install', { channel, config, localArchive });
  } catch (error) {
    const message =
      typeof error === 'string'
        ? error
        : error instanceof Error
          ? error.message
          : JSON.stringify(error);
    throw new BackendError(message, 'start_install');
  }
}
