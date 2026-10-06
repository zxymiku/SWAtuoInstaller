/**
 * 前端入口。
 *
 * 样式在此集中引入；`index.html` 里不再放 `<link>`，避免开发期路径与
 * 打包后的 hash 产物对不上而白白产生 404。
 */

import { mount } from 'svelte';

import './styles/tokens.css';
import './styles/global.css';

import App from './App.svelte';

/**
 * 兜底错误面板。
 *
 * 桌面 WebView 默认看不到控制台，一旦启动阶段抛异常就只剩全白窗口。
 * 这里在**任何其它代码之前**注册处理器，把启动错误直接画到页面上，
 * 让"白屏"变成可诊断的信息（开发期尤其重要）。
 */
function installCrashOverlay(): void {
  const render = (title: string, detail: string): void => {
    let panel = document.getElementById('sw-crash-overlay');
    if (!panel) {
      panel = document.createElement('div');
      panel.id = 'sw-crash-overlay';
      // 内联样式：此刻样式表可能还没加载成功，不能依赖外部 CSS。
      panel.setAttribute(
        'style',
        [
          'position:fixed',
          'inset:0',
          'z-index:99999',
          'overflow:auto',
          'padding:24px',
          'background:#191919',
          'color:#f2f2f0',
          'font:12px/1.7 Consolas,monospace',
          'white-space:pre-wrap',
          'border-top:4px solid #fff500',
        ].join(';'),
      );
      document.body.appendChild(panel);
    }
    panel.textContent = `【${title}】\n\n${detail}`;
  };

  window.addEventListener('error', (event) => {
    const error = event.error as Error | undefined;
    render(
      '前端启动失败',
      [
        error?.message ?? event.message ?? String(event),
        error?.stack ?? '',
        `资源: ${event.filename ?? '-'}:${event.lineno ?? '-'}:${event.colno ?? '-'}`,
        `URL: ${location.href}`,
        `UA: ${navigator.userAgent}`,
      ]
        .filter(Boolean)
        .join('\n'),
    );
  });

  window.addEventListener('unhandledrejection', (event) => {
    const reason = event.reason as Error | string | undefined;
    render(
      '未处理的 Promise 拒绝',
      [
        reason instanceof Error ? `${reason.message}\n${reason.stack ?? ''}` : String(reason),
        `URL: ${location.href}`,
      ].join('\n\n'),
    );
  });
}

installCrashOverlay();

const target = document.getElementById('app');

if (!target) {
  throw new Error('缺少挂载点 #app，请检查 index.html');
}

try {
  mount(App, { target });
} catch (error) {
  const detail = error instanceof Error ? `${error.message}\n\n${error.stack ?? ''}` : String(error);
  window.dispatchEvent(
    Object.assign(new Event('error'), { message: detail, error: error as Error }),
  );
  throw error;
}
