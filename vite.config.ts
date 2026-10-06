import { fileURLToPath, URL } from 'node:url';

import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

/**
 * Vite 配置 — 为 Tauri 2 桌面壳服务。
 *
 * 与 `src-tauri/tauri.conf.json` 的约定：
 *   - `build.devUrl`     = http://localhost:1420
 *   - `build.frontendDist` = ../dist
 */
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],

  // 关键：产物必须使用**相对**资源路径。
  //
  // Tauri 把前端资源嵌入二进制后，用自定义协议提供；请求路径会被
  // `trim_start_matches('/')` 去掉前导斜杠，而嵌入时的 AssetKey 是**带**前导斜杠的
  // （`/assets/index-xxx.js`）。因此绝对路径 `/assets/...` 永远查不到资源，
  // WebView 里就是一片白，只剩 CSS 因为同样被读从而背景色正常——极具迷惑性。
  //
  // `base: './'` 让 Vite 生成 `./assets/...`，浏览器相对文档 URL 解析后能被正确命中。
  base: './',

  resolve: {
    alias: {
      // 与 tsconfig 的 `paths` 保持一致，供 Svelte 组件里的 $lib/... 导入使用。
      $lib: fileURLToPath(new URL('./src/lib', import.meta.url)),
    },
  },

  // Tauri 期望一个固定端口，端口被占用时直接失败而不是静默换端口。
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // src-tauri 由 cargo 自己监听，避免重复触发。
      ignored: ['**/src-tauri/**'],
    },
  },

  build: {
    target: 'chrome110',
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 1200,
  },
});
