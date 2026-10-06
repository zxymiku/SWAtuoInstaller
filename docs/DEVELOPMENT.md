# 开发指南

面向需要修改、扩展或调试本项目的开发者。假定你已阅读
[`README.md`](./README.md) 的架构概览。

---

## 目录

- [环境搭建](#环境搭建)
- [项目结构约定](#项目结构约定)
- [前端开发](#前端开发)
- [后端开发](#后端开发)
- [前后端契约](#前后端契约)
- [构建与打包](#构建与打包)
- [代码规范](#代码规范)
- [调试技巧](#调试技巧)

---

## 环境搭建

### 1. Rust 工具链

```powershell
# 安装 rustup（若尚未安装）
winget install Rustlang.Rustup

# 确保使用 MSVC 工具链
rustup default stable-x86_64-pc-windows-msvc
rustc --version   # 需要 1.77+
```

### 2. MSVC 构建工具

Rust 在 Windows 上需要 MSVC 链接器：

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools
```

安装时勾选 **「使用 C++ 的桌面开发」** 工作负载（含 MSVC v143 与 Windows SDK）。

> **注意**：本项目刻意避免需要编译 C 代码的依赖（详见
> [为什么不用 rustls](#为什么不用-rustls)），因此不需要额外的 C 编译器配置。

### 3. Node.js

```powershell
winget install OpenJS.NodeJS.LTS
node --version    # 需要 20+
npm --version
```

### 4. 安装依赖

```powershell
cd solidworks-installer
npm install
```

### 5. 补齐 7z.exe（可选）

解压功能需要 `7z.exe`：

```powershell
Copy-Item "C:\Program Files\7-Zip\7z.exe" "src-tauri\resources\7z.exe"
```

未提供时程序仍可编译运行，只在第 4 步解压时报出明确错误。

### 6. 验证环境

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
npm run build
npx svelte-check --tsconfig ./tsconfig.json
```

三条命令都应无错误输出。

### Windows 代理 / 镜像配置

若 `cargo fetch` 缓慢，可在 `%USERPROFILE%\.cargo\config.toml` 配置镜像源：

```toml
[source.crates-io]
replace-with = 'mirror'

[source.mirror]
registry = "sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"
```

npm 侧可设置：

```powershell
npm config set registry https://registry.npmmirror.com
```

---

## 项目结构约定

### 目录职责

| 目录 | 职责 | 不应包含 |
|---|---|---|
| `src/routes/` | 页面级组件，直接对应一个路由 | 可复用的通用逻辑 |
| `src/lib/components/` | 可复用 UI 组件 | 页面级布局与路由逻辑 |
| `src/lib/stores/` | 共享状态（runes） | 组件、DOM 操作 |
| `src/lib/api/` | 与后端通信的唯一入口 | UI 逻辑 |
| `src/styles/` | 设计令牌与共享组件样式 | 组件私有样式 |
| `src-tauri/src/` | 后端全部逻辑 | 前端相关代码 |
| `src-tauri/src/installer/` | 安装编排，**一个文件一个阶段** | 跨阶段的通用工具（放 `process_utils` / `paths`） |

**文件规模约定**：单个源码文件控制在 **450 行以内**。
`installer.rs` 曾是 3000+ 行的单文件，改动一处需要在几千行里跳转，
现已按阶段拆分（见下方「模块地图」）。新增功能时如果某个文件要突破这个规模，
优先考虑按职责再拆一层，而不是继续追加。

### 三条硬性约定

1. **组件不直接 import `@tauri-apps/api`**
   一律通过 `$lib/api/commands.ts` 或 `$lib/api/channels.ts`。
   这样在浏览器（无 Tauri 宿主）中也能优雅降级并给出明确提示。

2. **前后端类型必须成对修改**
   修改 `src-tauri/src/config.rs` 或 `events.rs` 时，
   必须同步修改 `src/lib/api/types.ts`。反之亦然。

3. **时间参数一律为分钟**
   新增任何时间参数都用 float 分钟，命名以 `_minutes` 结尾。
   唯一的毫秒参数是 `scan_interval_ms`，命名以 `_ms` 结尾。

---

## 前端开发

### 两个必须记住的陷阱

这两个问题都会导致**全白窗口**，而且都不报编译错误，极难定位。已经踩过一次。

#### 陷阱一：runes 必须写在 `.svelte.ts` 里，并且导入时要带扩展名

Svelte 5 的 `$state` / `$derived` / `$effect` / `$props` **只有在经过 Svelte 编译的
模块里才有定义**。普通 `.ts` 文件不会被 Svelte 编译，运行时直接抛
`ReferenceError: $state is not defined`，整个应用在挂载前就崩掉。

```ts
// ✅ 文件名以 .svelte.ts 结尾 —— 会被 Svelte 编译
// src/lib/stores/install.svelte.ts
export const installState = $state({ running: false });

// ✅ 导入时必须写出完整扩展名 .svelte.ts
import { installState } from '$lib/stores/install.svelte.ts';

// ❌ 普通 .ts 文件里写 runes —— 运行时 $state undefined
// src/lib/stores/install.ts

// ❌ 省略扩展名：vite-plugin-svelte 不会把它交给 Svelte 编译
import { installState } from '$lib/stores/install.svelte';

// ❌ 写成 .ts（去掉 .svelte）—— 同样不会被编译
```

配套的 `tsconfig.json` 必须开启：

```json
"allowImportingTsExtensions": true
```

（与 `"noEmit": true` 搭配使用即可，不会产出多余文件。）

**怎么快速自检**：构建产物里不应再出现 runes 标识符。

```powershell
$js = Get-ChildItem dist\assets\*.js | Select-Object -First 1
$c = [IO.File]::ReadAllText($js.FullName)
foreach ($p in @('$state(','$derived(','$effect(','$props(')) { "$p -> $($c.Contains($p))" }
```

四项全为 `False` 才算正常。若为 `True`，说明 runes 泄漏到了未编译的模块里。

#### 陷阱二：Vite 必须设置 `base: './'`

Tauri 把前端资源嵌入二进制后，用自定义协议提供，请求路径会被
`trim_start_matches('/')` **去掉前导斜杠**；而嵌入时用的 `AssetKey` 是
**带**前导斜杠的（`/assets/index-xxx.js`）。两者对不上 → 脚本 404 → 白屏。

```ts
// vite.config.ts
export default defineConfig({
  base: './',   // ← 必须。产物变成 ./assets/xxx.js，浏览器相对文档 URL 解析后可命中
  // ...
});
```

```html
<!-- ✅ base: './' 的产物 -->
<script type="module" src="./assets/index-xxx.js"></script>

<!-- ❌ 默认 base: '/' 的产物 —— Tauri 里永远 404 -->
<script type="module" src="/assets/index-xxx.js"></script>
```

**迷惑点**：CSS 通常也会一起 404，但页面背景色却是对的——
因为 `:root` 的 `color-scheme` / 浏览器默认底色让白屏看起来"还算正常"。
不要因为"颜色对"就排除资源加载问题。

#### 排错手段：把白屏变成报错

`src/main.ts` 第一件事就是注册全局 `error` 与 `unhandledrejection` 处理器，
把启动异常直接画到页面上（深色面板 + 信号黄描边）。
桌面 WebView 默认看不到控制台，这个兜底面板是唯一能拿到信息的途径。

若连面板都没出现，说明 JS 根本没执行，按以下顺序查：

1. 用 Edge/Chrome 无头模式打开 `dist/index.html`（**必须经 HTTP 服务，
   `file://` 会被 CORS 拦住 ES module**），看是否白屏；
2. 用 CDP 抓 `Runtime.exceptionThrown` 与控制台（见下）；
3. 确认产物里的资源路径是 `./assets/...`。

一次性的 CDP 排错脚本形态（连上 DevTools 协议后开启
`Runtime.enable` + `Log.enable`，导航一次，再读
`document.getElementById('app').innerHTML.length`）：

```js
// 关键三步（完整脚本需要 WebSocket 客户端）
await send('Runtime.enable');
await send('Log.enable');
await send('Page.navigate', { url });
// 之后收集 Runtime.exceptionThrown / Runtime.consoleAPICalled / Log.entryAdded
```

`#app innerHTML length` 为 `0` 就是没挂载成功；正常应为上万字符。

---

### 技术栈

| 项 | 版本 | 说明 |
|---|---|---|
| Svelte | 5 | 使用 **runes**（`$state` / `$derived` / `$effect` / `$props`） |
| Vite | 6 | 构建工具 |
| TypeScript | 5 | 严格模式 |

### Svelte 5 runes 约定

本项目**不使用** Svelte 4 的 `export let`、`$:`、store 订阅语法。

```svelte
<script lang="ts">
  // ✅ 组件 props
  interface Props {
    label: string;
    value?: number;
    onSelect?: (id: string) => void;
  }
  let { label, value = 0, onSelect }: Props = $props();

  // ✅ 本地状态
  let query = $state('');

  // ✅ 派生值
  const filtered = $derived(items.filter((i) => i.name.includes(query)));

  // ✅ 复杂派生（多语句）
  const stats = $derived.by(() => {
    let total = 0;
    for (const item of items) total += item.size;
    return { total, count: items.length };
  });

  // ✅ 副作用（返回清理函数）
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
</script>
```

**跨组件共享状态**放在 `$lib/stores/*.ts`。模块级 `$state` 可用于导出对象与数组，
但**不要导出会被重新赋值的原始值**：

```ts
// ✅ 导出对象，直接改属性
export const installState = $state({ running: false, percent: 0 });
export function setRunning(v: boolean) { installState.running = v; }

// ✅ 导出数组，push/splice 均可
export const logs = $state<LogEntry[]>([]);

// ❌ 不要导出会被重新赋值的原始值
// export let counter = $state(0);
// export function bump() { counter += 1; }   // 订阅者收不到更新
```

### 设计令牌

令牌定义在 `src/styles/tokens.css`，分两个正交的轴：

| 轴 | 根属性 | 控制内容 |
|---|---|---|
| **家族（family）** | `data-ark-theme="endfield"` | 墨色、纸色、信号色、字型、几何性格 |
| **深度（depth）** | `data-ark-depth="maximal"` | 层数与不透明度、装饰显隐、动效幅度 |

两个属性都写在 `index.html` 的 `<html>` 上。

**组件样式只允许引用语义变量**，不写死色值：

```css
/* ✅ 正确 */
.panel {
  background: var(--ark-paper);
  border: var(--ark-rule) solid var(--ark-border);
  border-left: 3px solid var(--ark-ink);
}

/* ❌ 错误：绕过了令牌层 */
.panel {
  background: #f2f2f0;
  border-left: 3px solid #191919;
}
```

常用令牌速查：

| 令牌 | 用途 |
|---|---|
| `--ark-ink` / `--ark-paper` | 主文字 / 主背景 |
| `--ark-accent` | 信号黄，**只用于**选择、进度、行动、聚焦 |
| `--ark-border` / `--ark-line-guide` | 边框 / 长引导线 |
| `--ark-dock` / `--ark-dock-text` | 炭黑器械坞站 |
| `--ark-rule` | 统一 1px 线宽 |
| `--ark-space-*` | 4 / 8 / 16 / 32 / 64 / 96 px |
| `--ark-font-size-*` | micro(10) label(11) body(14) heading(20) display(48) |
| `--ark-ease` / `--ark-motion-*` | 缓动曲线与时长 |
| `--ark-*-wash` | 提示条底色（成功/警告/错误） |
| `--ark-orchestration` | 深度驱动的编排系数（0.15 ~ 1） |

### 共享组件类

`global.css` 提供了跨页面共用的类，可直接在任意组件中使用
（Svelte 的作用域样式不会剔除全局类）：

| 类 | 用途 |
|---|---|
| `.ark-btn` 及 `--primary` / `--danger` / `--dock` / `--compact` | 行动按钮 |
| `.ark-chip` 及 `[data-state]` | 状态胶囊（标签 + 值） |
| `.ark-field` / `.ark-input` / `.ark-select` / `.ark-textarea` | 表单控件 |
| `.ark-switch` | 方形开关 |
| `.ark-panel` 及 `--dock` | 器械面板 |
| `.ark-stage__*` | 舞台层（网格/引导线/楔形/刻度/编号/扫描） |
| `.ark-log` / `.ark-log__row` | 日志行（按 `data-level` 着色） |
| `.ark-scroll` 及 `--fade` | 滚动容器与渐隐遮罩 |
| `.ark-reveal` / `.ark-wipe` | 揭示动效 / 黄色擦入 |
| `.ark-visually-hidden` / `.ark-skip-link` | 无障碍辅助 |

### 页面路由

使用**哈希路由**（`#/` `#/settings` `#/logs`），实现在 `App.svelte`：

```ts
function parseHash(hash: string): RouteId {
  const cleaned = hash.replace(/^#\/?/, '').split('?')[0].toLowerCase();
  if (cleaned === 'settings') return 'settings';
  if (cleaned === 'logs') return 'logs';
  return 'install';
}
```

选哈希而不是 History API 的原因：Tauri 生产环境用 `tauri://localhost`
或 `file://` 提供资源，History 路由在直接刷新时会 404。

**新增页面**的步骤：

1. 在 `src/routes/` 新建 `XxxPage.svelte`
2. 在 `App.svelte` 扩展 `RouteId` 联合类型
3. 更新 `parseHash`、`RAIL_ITEMS`、`ROUTE_LABELS`、`ROUTE_IDENTIFIERS`
4. 在 `<main>` 的条件分支中渲染

### 响应式约定

断点使用两套条件（同时覆盖窄屏与竖屏）：

```css
@media (max-width: 860px), (orientation: portrait) and (max-width: 1024px) {
  /* 侧栏转底部行动条、多列落为单列、舞台层收敛 */
}

@media (max-height: 780px) and (min-width: 861px) {
  /* 短宽视口压缩纵向留白 */
}
```

**竖屏必须重新编排，不是等比缩小**：保留全部标签与取值，
只改变排列方式。禁止为了塞进窄屏而删除文字或缩小到不可读。

### 无障碍清单

新增组件时逐项确认：

- [ ] 用语义化元素（`button` / `nav` / `header` / `main` / `ol` / `dl` / `table`）
- [ ] 状态不只靠颜色（同时用填充、字重、`aria-current`、文字标签）
- [ ] 交互目标 ≥ 40×40 px（竖屏按钮 ≥ 48 px）
- [ ] 图标按钮有 `aria-label` 或可见文字
- [ ] 焦点环保留（`global.css` 已定义 `:focus-visible`）
- [ ] 装饰元素 `aria-hidden="true"`
- [ ] 动态区域用 `role="status"` 或 `role="alert"`
- [ ] 动效在 `prefers-reduced-motion: reduce` 下静态可读

### 校验前端

```powershell
npx svelte-check --tsconfig ./tsconfig.json
npm run build
node .skills\ark-ui-skill-main\scripts\audit-ark-ui.mjs dist\assets\*.css
```

第三条是 ark-ui 自带的启发式审计，应输出 `"errors": []` 与 `"warnings": []`。

---

## 后端开发

### 模块地图

后端**按职责拆分**，单个文件不超过约 450 行。`installer` 曾是 3000+ 行的单文件，
现已按安装阶段拆成 15 个子模块 —— 改某一步时只需要打开对应的那一个文件。

#### 顶层模块

| 文件 | 职责 | 关键类型/函数 |
|---|---|---|
| `lib.rs` | 应用入口、`AppState`、`Reporter`、工具 | `run()`, `Reporter`, `AppState`, `decode_console_bytes()` |
| `config.rs` | 配置结构与结构保留合并 | `Config`, `merge_toml()`, `merge_toml_preserving_empty()` |
| `commands.rs` | Tauri 命令层 | 13 个 `#[tauri::command]` |
| `downloader.rs` | 多线程分块下载 | `DownloadEngine`, `ProgressSink` |
| `win32_utils.rs` | Win32 API 封装 | 窗口/网卡/进程/文件对话框/注册表 |
| `process_utils.rs` | 子进程与服务控制 | `run_command()`, `kill_matching_processes()` |
| `events.rs` | 事件与状态类型 | `InstallEvent`, `StepId`, `InstallStatus` |
| `antivirus.rs` | 安全软件检测与处置 | `detect_products()`, `handle_all()` |

#### `installer/` —— 按安装阶段拆分

| 子模块 | 负责阶段 | 关键函数 |
|---|---|---|
| `pipeline.rs` | 13 阶段主编排 | `run()` |
| `context.rs` | 运行上下文与状态单元 | `InstallContext`, `InstallStatusCell`, `ClosureSink` |
| `steps.rs` | 第 1 步环境检测、第 2.5 步杀软处置 | `environment_check()`, `handle_antivirus()` |
| `archive/` | 第 2 步取包 | `sniff.rs` 纯函数嗅探/命名；`fetch.rs` 单包与分片下载 |
| `sevenzip/` | 7z 定位与获取 | `lookup.rs` 只读查找/变体检测；`fetch.rs` 状态与自动下载 |
| `extract.rs` | 第 4、5 步两阶段解压 | `extract_archive()`, `extract_solidsquad()` |
| `registry.rs` | 第 6 步导入注册表 | `import_registry()` |
| `iso.rs` | 第 7 步挂载 ISO 与定位入口 | `locate_iso()`, `locate_setup()` |
| `install_ops.rs` | 第 8 步启动安装器 + 弹窗守护 | `launch_installer()`, `spawn_popup_daemon()` |
| `verify.rs` | 第 9 步轮询检测完成 | `poll_for_completion()` |
| `postinstall.rs` | 第 10、11 步进程清理 + 文件替换 | `kill_solidworks_processes()`, `replace_files()` |
| `flexnet.rs` | 第 12 步 FlexNet 服务 | `install_flexnet_service()` |
| `guards.rs` | `Drop` 兜底守卫 | `IsoMountGuard`, `TempDirGuard` |
| `paths.rs` | 大小写不敏感查找 | `find_first_dir()`, `wildcard_ci()`, `walk_entries()` |

**依赖方向是单向的**：

```text
pipeline ──► 各阶段模块 ──► context / paths / guards
```

阶段模块之间**不互相调用**（`extract` 只借用 `archive::sniff` 的纯函数
`sniff_archive_kind`），因此没有循环依赖。新增一个阶段时：

1. 在 `installer/` 下新建 `my_stage.rs`
2. 在 `installer/mod.rs` 里 `pub mod my_stage;`
3. 在 `pipeline.rs` 的 `run()` 里按顺序调用

`installer/mod.rs` 用 `#[doc(hidden)] pub use` 把原先 `installer.rs` 的公开项
原样导出，所以 `commands.rs` / `lib.rs` 里的调用点无需改动。

### 已验证的纯函数

下面这些函数与 Tauri 运行时无关，可以单独调用做真实执行验证。**推荐做法**：
临时建一个 `src-tauri/src/bin/_probe.rs`（crate 内的 bin），跑完就删。

> **为什么不能建独立 crate**：Tauri 的 build script 会往 `OUT_DIR` 写入一个
> **格式非法的 `msvcrt.lib` 占位文件**，它在链接搜索路径里会遮蔽真库。
> 因此 `cargo check` 能过，但**任何外部 crate 链接这个 lib 都会失败**
> （`LNK1120` 一堆 `_tls_index` / `strlen` 未解析）。放进 crate 内就没这个问题。

| 函数 | 验证内容 |
|---|---|
| `paths::wildcard_ci` | `*` / `?` 匹配、大小写折叠、空模式、回溯正确性 |
| `archive::sniff::sniff_archive_kind` | 7z/zip/rar/gzip/bzip2/xz/zstd/cab/iso 签名、反例为 `None` |
| `archive::sniff::is_volume_part_name` | `.001`~`.9999` 识别；`.7z` / `.01` / 无扩展名为 `false` |
| `archive::sniff::cache_file_name` | 分卷名原样保留（**曾经的回归点**）、非分卷按嗅探补后缀 |
| `paths::find_*` 系列 | 大小写不一致的目录/文件、深层嵌套、深度上限截断 |
| `sevenzip::lookup::usable_executable` | 非空文件与 **AppExecutionAlias 重解析点**都要认可 |
| `antivirus::detect_products` | SecurityCenter2 查询与 `productState` 位解码 |
| `config::merge_toml*` | 保留用户值、**留空即保留**、补齐缺失字段、带回默认注释 |

**两个已确认的坑**：

- `sniff_archive_kind` 的读取窗口是 **64 KiB**，不是 512 字节——
  ISO9660 的 `CD001` 卷描述符在偏移 `0x8001`，窗口小了探测不到 ISO。
- `usable_executable` **不能**用 `file_type().is_symlink()` 判断 0 字节文件。
  `%LOCALAPPDATA%\Microsoft\WindowsApps\NanaZipC.exe` 是 **AppExecutionAlias**：
  属性位含 `FILE_ATTRIBUTE_REPARSE_POINT`，但**不是 symlink**，
  `is_symlink()` 返回 `false`，会导致装有 NanaZip 的机器上 7z 查找全部失败。
  正确做法是读文件属性位（见 `is_reparse_point()`）。

### 添加一个新命令

**第一步：在 `commands.rs` 实现**

```rust
/// 一句话说明这个命令做什么，以及失败时的语义。
#[tauri::command]
pub async fn my_command(
    state: State<'_, AppState>,
    // 参数名会作为前端 invoke 的 key；使用 snake_case
    some_arg: String,
) -> Result<MyResult, String> {
    // 返回 Err(String) 会在前端抛出，被 commands.ts 包装为 BackendError
    Ok(MyResult { /* ... */ })
}
```

**要点**

| 项 | 要求 |
|---|---|
| 返回类型 | `Result<T, String>`，`T` 实现 `Serialize` |
| 错误类型 | `String`（人类可读，会直接显示给用户） |
| 参数名 | snake_case，与前端 `invoke('my_command', { some_arg })` 对应 |
| 状态访问 | `State<'_, AppState>`；**不要把它移进线程闭包**（带生命周期），先克隆字段 |
| 阻塞操作 | 用 `tauri::async_runtime::spawn_blocking` 或独立 `std::thread` |

**第二步：注册到 `lib.rs` 的 `generate_handler!`**

```rust
.invoke_handler(tauri::generate_handler![
    commands::get_config,
    // ...
    commands::my_command,   // ← 新增
])
```

> 忘记注册是最常见的错误：命令在 Rust 侧编译通过，
> 但前端调用时报 `command my_command not found`。

**第三步：在前端 `commands.ts` 暴露**

```ts
/** 一句话说明（与 Rust 侧注释保持一致）。 */
export function myCommand(someArg: string): Promise<MyResult> {
  return call<MyResult>('my_command', { some_arg: someArg });
}
```

**第四步：在 `types.ts` 声明返回类型**

```ts
export interface MyResult {
  field: string;
  count: number;
}
```

### 添加一个新的 Channel 事件类型

**第一步：在 `events.rs` 扩展枚举**

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum InstallEvent {
    // ... 已有变体
    MyEvent {
        detail: String,
        progress: f64,
    },
}
```

**关键点**

- `#[serde(tag = "type", content = "data")]` 让消息序列化为 `{ "type": "...", "data": {...} }`
- `rename_all = "snake_case"` 把 `MyEvent` 变成 `"my_event"`
- 变体必须 `Clone`（`Channel<T>: IpcResponse` 要求）

**第二步：在 `Reporter` 添加便捷方法（`lib.rs`）**

```rust
impl Reporter {
    pub fn my_event(&self, detail: &str, progress: f64) {
        self.emit(InstallEvent::MyEvent {
            detail: detail.to_string(),
            progress,
        });
    }
}
```

**第三步：在编排中调用**

```rust
ctx.reporter().my_event("阶段说明", 42.0);
// 或通过 InstallContext 的便捷方法
ctx.info("阶段说明");
```

**第四步：前端声明并在 `applyEvent` 中折叠**

`types.ts`：

```ts
export type InstallEvent =
  | { /* ... 已有变体 */ }
  | { type: 'my_event'; data: { detail: string; progress: number } };
```

`stores/install.ts` 的 `applyEvent`（**必须用 `if` 逐个判断，
不要用 `switch`** —— `switch` 无法对本联合类型做完整的判别收窄）：

```ts
export function applyEvent(event: InstallEvent): void {
  if (event.type === 'my_event') {
    installState.message = event.data.detail;
    installState.percent = event.data.progress;
    return;
  }
  // ... 其他类型
}
```

### 访问配置

配置在编排中通过 `ctx.config` 访问。**所有超时都必须走便捷方法**，
不要在业务代码里写 `Duration::from_secs(30)`：

```rust
// ✅ 从配置读取
let timeout = ctx.config.extract_timeout();
let interval = ctx.config.poll_interval();
let backoff = ctx.config.retry_backoff(attempt);

// ❌ 硬编码
let timeout = Duration::from_secs(1800);
```

`Config` 上已有的时间便捷方法：

| 方法 | 对应配置 |
|---|---|
| `connect_timeout()` | `download.connect_timeout_minutes` |
| `read_timeout()` | `download.read_timeout_minutes` |
| `retry_backoff(attempt)` | `download.retry_backoff_minutes` |
| `fetch_timeout()` | `remote_config.fetch_timeout_minutes` |
| `poll_interval()` | `install.polling.poll_interval_minutes` |
| `poll_timeout()` | `install.polling.timeout_minutes` |
| `popup_timeout()` | `install.popup.popup_timeout_minutes` |
| `kill_timeout()` | `process.kill_timeout_minutes` |
| `server_install_timeout()` | `flexnet.server_install_timeout_minutes` |
| `server_check_interval()` | `flexnet.server_check_interval_minutes` |
| `extract_timeout()` | `sevenzip.extract_timeout_minutes` |

### 添加新的配置项

**第一步：Rust 结构体**（`config.rs`）

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadConfig {
    // ...
    pub my_new_option_minutes: f64,   // 时间参数用 f64 + _minutes 后缀
    pub my_new_flag: bool,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            // ...
            my_new_option_minutes: 2.0,
            my_new_flag: true,
        }
    }
}
```

**第二步：归一化**（`Config::normalize()`）

```rust
self.download.my_new_option_minutes = self.download.my_new_option_minutes.max(0.1);
```

**第三步：默认配置文件**（`resources/default-config.toml`）

```toml
[download]
my_new_option_minutes = 2            # 一句话说明（这个注释会随合并带给用户）
my_new_flag = true                   # 说明
```

> **注释很重要**：合并机制会把这里的行内注释带给已有用户，
> 所以新字段一定要带注释。

**第四步：前端类型**（`types.ts` 的 `DownloadConfig`）与
**默认值**（`emptyConfig()`）。

**第五步：设置页表单**（`ConfigPanel.svelte`）

在对应分组的 `{#if activeGroup === 'download'}` 块内添加字段，并写明
**类型 / 默认值 / 取值范围 / 单位**：

```svelte
<div class="ark-field">
  <label class="ark-field__label" for="cfg-my-option">
    <span>我的新选项</span>
    <span class="ark-field__path">download.my_new_option_minutes</span>
  </label>
  <input
    id="cfg-my-option"
    class="ark-input"
    type="number"
    min="0.1"
    step="0.5"
    bind:value={config.download.my_new_option_minutes}
    {disabled}
    oninput={markDirty}
  />
  <p class="ark-field__hint">类型 float · 单位分钟 · 默认 2 · 最小值 0.1</p>
</div>
```

**第六步：文档**（`docs/CONFIG.md`）在对应分组补充条目，
并更新「完整空白默认配置文件」一节。

### 文件系统查找规范

**不要用 `glob` crate 查找补丁包内容**。`glob` 的匹配是逐字符比较的，
在 Windows 上一样**大小写敏感**，而补丁包的目录名与文档经常不一致
（`_SolidSQUAD_` / `_SolidSquad_`、`SolidWorks Corp` / `SOLIDWORKS corp`）。

`installer/paths.rs` 提供了一组不区分大小写的查找函数，所有查找都应走它们：

| 函数 | 用途 |
|---|---|
| `find_first_dir(root, name)` | 找第一个名字匹配的目录（找不到时回退判断 `root` 自身） |
| `find_dirs_named(root, name)` | 找所有同名目录 |
| `find_file_named(root, name)` | 找第一个同名文件 |
| `find_files_named(root, name)` | 找所有同名文件 |
| `find_files_matching(root, pattern)` | 按通配符匹配文件名（`*` / `?`） |
| `walk_entries(root, predicate)` | 底层遍历，自定义判定 |
| `wildcard_ci(pattern, candidate)` | 通配符匹配（不区分大小写） |

```rust
// ✅ 大小写不敏感，且扩展名不写死
let patches = find_files_matching(folder1, "*solid*squad*");
let regs = find_files_matching(&squad, "*.reg");
let isos = find_files_matching(folder1, "*.iso");

// ❌ 大小写敏感，且把扩展名写死
let pattern = folder1.join("*[Ss]olid...*").join("*.7z");
```

**安全护栏**：遍历深度上限 `WALK_MAX_DEPTH = 24`，单次结果上限
`WALK_MAX_HITS = 512`。新增查找时不要绕过这两个限制——
在 `C:\` 这类目录上无上限遍历会失控，符号链接还可能导致环。

### 压缩包格式判定规范

**永远不要用扩展名判断压缩包格式。**

| 做法 | 原因 |
|---|---|
| 解压时不传 `-t<格式>` | 7z 按文件头自动识别。强行指定 `-t` 会让"名字像 zip、实际是 7z"的文件失败 |
| 准入判定用 `7z t` 实测 | 唯一可靠的判据；超时可放行，交由正式解压判定 |
| 用 `sniff_archive_kind()` 只做日志与缓存命名 | 它读 64 KiB 文件头（ISO 的 `CD001` 在 0x8001，窗口必须够大） |

```rust
// ✅ 让 7z 自己判定
command.args(["x", archive, &format!("-o{}", dest.display()), "-aoa", "-bb1", "-y"]);

// ✅ 嗅探结果只用于展示
if let Some(kind) = sniff_archive_kind(archive) {
    ctx.debug(format!("7z 将按文件头以 {kind} 格式解压"));
}

// ❌ 按扩展名拒绝文件
if !looks_like_archive(&path) { return Err("不是压缩包".into()); }
```

`looks_like_archive()` 仍然保留，但**只用于展示与缓存命名**，
不得作为准入判定。

### 子进程执行规范

所有外部程序都必须经过 `process_utils`：

```rust
let mut command = process_utils::hidden_command(Path::new("sc"));
command.args(["query", "SolidWorks Flexnet Server"]);

let outcome = process_utils::run_command(
    command,
    ctx.config.some_timeout(),        // 超时来自配置
    Some(&ctx.cancel),                // 支持取消
)?;

if outcome.is_success() {
    // ...
} else {
    // outcome.tail(6) 给出输出末尾若干行，便于定位
}
```

`hidden_command()` 会：

- 设置 `stdin/stdout/stderr` 为 `piped`（或 `null`）
- 加上 `CREATE_NO_WINDOW`（0x08000000），**避免弹出控制台黑框**

`run_command()` 会：

- 用独立线程读取输出（防止管道写满导致子进程死锁）
- 每 120 ms 检查超时与取消标记
- 超时/取消时 `kill` 子进程
- 输出截断到 256 KiB

**运行 `.bat` 脚本**要经过 `cmd /C` 并设置工作目录：

```rust
let mut command = process_utils::hidden_command(Path::new("cmd"));
command.args(["/C", &bat.to_string_lossy()]);
if let Some(parent) = bat.parent() {
    command.current_dir(parent);
}
```

### 为什么不用 rustls

`reqwest` 的 `rustls-tls` 特性会引入 `ring`，而 `ring` 包含需要 C 编译器
处理的汇编与 C 源码。本项目改用 `native-tls`：

```toml
reqwest = { version = "0.12", default-features = false, features = [
    "stream", "native-tls", "http2", "charset",
] }
```

| 方面 | `rustls-tls` | `native-tls` |
|---|---|---|
| Windows 后端 | ring（自带实现） | SChannel（系统） |
| 需要 C 编译器 | **是** | 否 |
| 依赖树 | +rustls +ring +webpki-roots | +native-tls +schannel |
| 证书信任 | 内置 webpki 根证书 | **跟随系统证书存储** |

最后一项对内网部署是额外好处：企业自签 CA 装进系统存储后即可被信任。

### `windows` crate 用法

版本与 Tauri 保持一致（`0.61`），避免依赖树里出现两套 `windows`。

```rust
use windows::core::{Interface, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW};
```

**注意事项**

| 项 | 说明 |
|---|---|
| 特性门控 | 每个 API 都需要在 `Cargo.toml` 的 `features` 中启用对应模块 |
| 返回类型 | 多数返回 `windows_core::Result<T>`，需 `.is_err()` / `.ok()` 处理 |
| `BOOL` | 在 `windows::core`（不是 `Win32::Foundation`） |
| 字符串 | 用 `PWSTR::to_string()`（返回 `Result<String, FromUtf16Error>`） |
| 资源释放 | 句柄用 `CloseHandle`，`PWSTR` 由 API 分配的用 `CoTaskMemFree` |
| 平台桩 | 非 Windows 目标需在 `#[cfg(not(windows))] mod imp` 提供同名桩实现，保证 `cargo check` 跨平台可过 |

### 异常安全：Drop 守卫模式

任何「申请了必须归还的资源」都应写成守卫：

```rust
pub struct MyGuard {
    resource: Option<Resource>,
    restore: bool,
}

impl MyGuard {
    pub fn acquire(...) -> Result<Self, String> { /* ... */ }

    /// 收尾步骤主动调用；调用后 Drop 不再重复执行。
    pub fn release_now(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if let Some(resource) = self.resource.take() {
            /* 释放并记录事件 */
        }
        events
    }
}

impl Drop for MyGuard {
    fn drop(&mut self) {
        // Drop 中绝不 panic：所有失败只记录
        if let Some(resource) = self.resource.take() {
            let _ = release(resource);
        }
    }
}
```

编排主体包在 `catch_unwind` 中，因此 panic 展开也会触发全部 `Drop`：

```rust
let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    // ... 13 个阶段
}));
```

---

## 前后端契约

### 命名约定

| 层 | 约定 | 示例 |
|---|---|---|
| Rust 结构体字段 | `snake_case` | `install_drive` |
| TOML key | `snake_case` | `install_drive = "C"` |
| JSON（IPC） | `snake_case`（Rust 默认，**不要加 rename_all**） | `{ "install_drive": "C" }` |
| TS 接口字段 | `snake_case`（与 JSON 一致） | `install_drive: string` |
| TS 局部变量/函数 | `camelCase` | `const targetDirectory` |
| Rust 命令名 | `snake_case` | `fetch_remote_config` |
| 事件变体 | `snake_case`（`rename_all`） | `step_changed` |

> **重要**：**不要**给配置结构体加 `#[serde(rename_all = "camelCase")]`。
> 加了之后 TOML 解析会失败（因为 TOML 里是 `snake_case`），
> 除非同时给每个字段加 `#[serde(alias)]`。

### 命令签名 ↔ 前端调用对照

| Rust | 前端 |
|---|---|
| `async fn get_config(state: State<'_, AppState>) -> Result<Config, String>` | `call<Config>('get_config')` |
| `async fn save_config(state: State<'_, AppState>, config: Config) -> Result<(), String>` | `call<void>('save_config', { config })` |
| `async fn fetch_remote_config(state: State<'_, AppState>, url: String) -> Result<Config, String>` | `call<Config>('fetch_remote_config', { url })` |
| `async fn start_install(app: AppHandle, state: State<'_, AppState>, channel: Channel<InstallEvent>, config: Config) -> Result<(), String>` | `tauriInvoke('start_install', { channel, config })` |
| `async fn export_logs(state: State<'_, AppState>, contents: String, label: String) -> Result<String, String>` | `call<string>('export_logs', { contents, label })` |

### Channel 生命周期

```rust
#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: Channel<InstallEvent>,   // ← 作为命令参数传入
    config: Config,
) -> Result<(), String> {
    let reporter = Reporter::new({
        let channel = channel.clone();
        move |event| { let _ = channel.send(event); }
    });

    // 安装跑在独立 OS 线程
    let handle = std::thread::spawn(move || { /* installer::run(&mut ctx) */ });

    // 关键：在这里等待完成，命令不返回 → Channel 保持打开
    let wait = tauri::async_runtime::spawn_blocking(move || {
        let received = done_rx.recv_timeout(Duration::from_secs(24 * 60 * 60));
        let _ = handle.join();
        received
    }).await?;

    wait
}
```

**为什么能工作**：`Channel<T>` 的生命周期与命令调用绑定。只要 `start_install`
的 `Future` 还没 resolve，Tauri 就不会释放该 channel 的 JS 侧回调，
后端可以持续 `send()`。

**前端对应写法**：

```ts
const channel = new Channel<InstallEvent>();
channel.onmessage = (event) => applyEvent(event);
await invoke('start_install', { channel, config });  // 直到安装结束才 resolve
```

### 使用 `$state.snapshot` 断开代理

把 runes 代理对象传给 `invoke` 前，先取快照：

```ts
// ✅ 纯对象，无 Proxy，序列化安全
const error = await startInstall($state.snapshot(config));

// ⚠️ 直接传 Proxy 通常也能工作，但引用会与组件状态共享
await startInstall(config);
```

本项目在 `InstallPage.handleStart` 使用快照；
`SettingsPage` 与 `store.persistConfig` 使用 `cloneConfig()`（JSON 深拷贝）。

---

## 构建与打包

### 开发

```powershell
npm run tauri:dev
```

该命令会：

1. 启动 Vite 开发服务器（`http://localhost:1420`，`strictPort: true`）
2. 编译 Rust 并启动应用窗口
3. 监听 `src/` 变化（HMR）与 `src-tauri/src/` 变化（自动重编译）

> **不要用 `cargo run` 代替**。`tauri.conf.json` 配了 `build.devUrl`，
> 构建期的 `dev` cfg 分支会让 WebView 去连 `http://localhost:1420`；
> 没有 Vite 在跑就会白屏。`cargo run` 只在你想验证"嵌入产物"时才用，
> 而且那时必须先 `npm run build`。
>
> **改了前端后必须重新构建二进制才能看到效果**（如果走 `cargo run`）：
> `tauri::generate_context!()` 在**编译期**把 `dist/` 嵌进二进制，
> 改完 `dist/` 需要触发 build.rs 重跑（`cargo build` 会因
> `cargo:rerun-if-changed` 未变而跳过）。直接改文件时间戳或 `cargo clean -p`
> 都可以，但更省事的做法始终是 `npm run tauri:dev`。

### 前端单独构建

```powershell
npm run build
```

产物在 `dist/`：`index.html` + `assets/*.css` + `assets/*.js`。

### 打包 Windows 安装包

**第一步：启用打包**

编辑 `src-tauri/tauri.conf.json`：

```json
"bundle": {
  "active": true,
  ...
}
```

> 默认 `active: false` 是为了让 `cargo check` / 日常开发不触发耗时的打包流程。

**第二步：执行打包**

```powershell
npm run tauri:build
```

产物位于 `src-tauri/target/release/bundle/nsis/*.exe`（或 `msi/`）。

### 打包资源

`bundle.resources` 当前只包含默认配置：

```json
"resources": ["resources/default-config.toml"]
```

补齐 `7z.exe` 后可追加：

```json
"resources": ["resources/default-config.toml", "resources/7z.exe"]
```

运行时通过 `app.path().resource_dir()` 定位；`installer::resolve_seven_zip`
会依次尝试资源目录、资源目录下的 `resources/`、可执行文件同级目录。

### 图标

`src-tauri/icons/` 下的图标由 `generate_icons.py` 程序化生成（Pillow），
图形是原创的 endfield 几何标记，不含任何第三方素材。重新生成：

```powershell
python src-tauri/icons/generate_icons.py
```

### 发布构建配置

`Cargo.toml` 的 release profile 已针对体积优化：

```toml
[profile.release]
codegen-units = 1     # 更激进的优化
lto = true            # 链接时优化
opt-level = "s"       # 优化体积
strip = true          # 剥离符号
```

> **不要启用 `panic = "abort"`**。安装编排依赖 `catch_unwind` + `Drop` 守卫
> 来保证「异常退出也恢复网络、卸载镜像」。`panic = "abort"` 会让 panic
> 直接终止进程，跳过全部 `Drop`，用户可能因此留下被禁用的网卡。

---

## 代码规范

### Rust

**格式与静态检查**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo check --manifest-path src-tauri/Cargo.toml
```

**风格约定**

| 项 | 要求 |
|---|---|
| 注释语言 | 中文；解释**为什么**而不是**做了什么** |
| 文档注释 | 公开类型与函数用 `///`；模块顶部用 `//!` |
| 错误处理 | 业务层返回 `Result<T, String>`（人类可读）；底层用具体错误类型再转换 |
| `unwrap` / `expect` | 仅限「逻辑上不可能失败」的场景，并附注释说明为什么不可能 |
| 字符串格式化 | 优先 `format!` 内联变量（`format!("{name}")` 而非 `format!("{}", name)`） |
| 硬编码时间 | **禁止**。一律走 `Config` 的便捷方法 |
| 平台代码 | Windows 专属逻辑放 `#[cfg(windows)]` 模块，并提供同名桩实现 |

**禁止项**

- ❌ 在业务代码里写 `Duration::from_secs(...)` 作为超时
- ❌ 直接 `Command::new(...)` 而不经过 `process_utils::hidden_command`
- ❌ 在 `Drop` 实现中 panic 或 `unwrap`
- ❌ 新增需要 C 编译器的依赖（会破坏构建可移植性）

### TypeScript / Svelte

**检查**

```powershell
npx svelte-check --tsconfig ./tsconfig.json
```

`tsconfig.json` 已开启：

```json
"strict": true,
"noUnusedLocals": true,
"noUnusedParameters": true,
"noFallthroughCasesInSwitch": true,
"noImplicitAny": true
```

**风格约定**

| 项 | 要求 |
|---|---|
| 注释语言 | 中文；模块顶部说明职责，复杂逻辑说明意图 |
| 类型 | 显式标注 Props 接口；避免 `any` |
| 联合类型收窄 | 用 `if (x.type === '...')` 逐项判断，**不要用 `switch`** |
| 函数 | 优先 `function` 声明；`$derived` 内是表达式 |
| 空函数体 | 用 `void expr;` 显式标记「故意忽略」 |
| 术语大小写 | `snake_case` 保留给后端字段；本地变量用 `camelCase` |

**CSS 约定**

| 项 | 要求 |
|---|---|
| 色值 | **只用** `var(--ark-*)`，不写字面量 |
| 圆角 | `var(--ark-radius)`（0px）；功能控件最多 `var(--ark-radius-functional)` |
| 线宽 | `var(--ark-rule)`（1px） |
| 类命名 | 组件内用块前缀（`.install__grid`），语义化 |
| 装饰元素 | `aria-hidden="true"` |
| 动画 | 提供 `prefers-reduced-motion` 下的静态替代 |

### 文档

修改以下内容时必须同步更新文档：

| 代码变更 | 需更新的文档 |
|---|---|
| 配置项增删改 | `docs/CONFIG.md`（含空白默认配置文件一节） |
| 安装阶段逻辑变化 | `docs/INSTALL_FLOW.md` |
| 新增 Tauri 命令 | `docs/DEVELOPMENT.md` 契约表 + `docs/README.md` 命令列表 |
| 新增 Channel 事件 | `docs/DEVELOPMENT.md` |
| 依赖/构建流程变化 | `docs/DEVELOPMENT.md` + `docs/README.md` |
| 新增已知问题 | `docs/TROUBLESHOOTING.md` |

---

## 调试技巧

### 后端日志

Rust 侧的 `println!` / `eprintln!` 输出到启动 Tauri 应用的终端。
`npm run tauri:dev` 的终端会同时显示 Vite 与 Rust 输出。

编排中的日志通过两条路径输出：

| 路径 | 去处 |
|---|---|
| `ctx.info()` / `ctx.debug()` / … | 推送到前端日志页（经 Channel） |
| `println!` / `eprintln!` | 终端（仅启动/致命错误） |

### 前端日志

浏览器控制台在 Tauri 开发模式下可通过 **右键 → 检查** 打开
（`tauri.conf.json` 未禁用 devtools）。

### 独立验证配置合并

配置合并是纯函数，可以直接验证：

```rust
let merged = config::merge_toml(
    config::DEFAULT_CONFIG_TOML,
    "[download]\nthreads = 32\n",
)?;
println!("{merged}");
```

预期：`threads = 32` 保留，其余字段与注释从默认配置补齐。

### 验证下载引擎

下载引擎不依赖 Tauri 运行时，可指向任意 HTTP 服务验证：

```rust
let settings = DownloadSettings {
    url: "http://127.0.0.1:8000/testfile.bin".into(),
    threads: 8,
    checksum: String::new(),
    retries: 3,
    backoffs: vec![Duration::from_secs(1)],
    connect_timeout: Duration::from_secs(30),
    read_timeout: Duration::from_secs(60),
};
```

用一个支持 `Range` 的静态服务器（`python -m http.server` 支持 `Range`）
即可观察分块、续传（中途 kill 后重启）与 `.state` 位图的变化。

### 快速模拟完整流程

想在不装 SolidWorks 的情况下走完流程，可以构造一个「假压缩包」目录：

1. 建 `fake/solidsquad.7z`（含 `_SolidSQUAD_/SolidWorks Corp/` 与一个 `.reg`）
2. 用 7z 打成 `fake.7z`
3. 主页选择 `fake.7z` 作为本地压缩包
4. 把 `install_path` 指向一个空目录
5. 把 `install.polling` 三项全部设为 `false` 之外的组合，观察判定行为

> 第 9 步的判定会一直等到超时 —— **因为主程序叫 `SLDWORKS.exe`，
> 早期版本却在找 `SOLIDWORKS.exe`，那个文件根本不存在。**
> 已于 2026-10 在一台真实安装完成的机器上核实并修复。
> 调试时把 `timeout_minutes` 设为 `1` 可以快速看到超时错误路径。

### 关于测试代码

按项目要求，本仓库**不包含单元测试**。上文「已验证的纯函数」列出了
曾经用一次性脚手架真实执行验证过的逻辑（验证脚手架已删除，不进入交付物）。
若将来需要补充正式测试，建议优先覆盖：

| 目标 | 类型 |
|---|---|
| `config::merge_toml` | 纯函数单元测试（保留注释、补齐字段、覆盖值） |
| `Config::normalize` | 边界值（threads 0/256、空盘符、非法正则） |
| `wildcard_ci` / `sniff_archive_kind` | 纯函数单元测试（含反例，确保不是恒真） |
| `find_*` 大小写不敏感查找 | 需要临时目录，属集成测试 |
| `StepId::logical_index` | 映射完整性（9 个步骤索引密集无空洞） |
| `decode_console_bytes` | UTF-8 / GBK / 混合字节 |
| 下载引擎 | 需要本地 HTTP 服务，属集成测试 |

> **重要**：本环境缺少 MSVC C++ 库，`cargo build` / `cargo test` **无法完成链接**
> （WebView2 的 C++ 对象需要 `operator new`、`_CxxThrowException` 等符号）。
> 因此验证策略是：`cargo check` + `cargo clippy` 保证类型与静态正确性，
> 纯逻辑另行用无 Tauri 依赖的最小脚手架真实执行。CI 上应确保完整的
> MSVC 环境可用后再跑 `cargo test`。

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CONFIG_GUIDE.md`](./CONFIG_GUIDE.md) | **我需要填什么** —— 每个键是否必填、填什么、最小可用示例、自检清单 |
| [`COMMAND_LINE.md`](./COMMAND_LINE.md) | 官方命令行属性参考：属性名、语言代码表、ADDLOCAL 层级、介质路径 |
| [`CONFIG.md`](./CONFIG.md) | 每个配置项的类型 / 默认值 / 取值范围 / 内部语义 |
| [`CUSTOMIZE.md`](./CUSTOMIZE.md) | 部署前必须改什么、已知局限、完整自检清单 |
| [`INSTALL_FLOW.md`](./INSTALL_FLOW.md) | 13 阶段流程详解、弹窗规则、轮询逻辑 |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | 按症状排查 |
