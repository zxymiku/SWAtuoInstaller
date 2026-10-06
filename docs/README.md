# SolidWorks 2024 SP5 自动安装程序

基于 **Tauri 2 + Svelte 5** 的 Windows 桌面部署终端：把 SolidWorks 2024 SP5
从「下载压缩包」到「许可服务 RUNNING」的全过程压缩成一次点击，所有可调参数
集中在 `config.toml`，界面按 **ark-ui · endfield 风格族 + maximal 深度** 编排。

---

## 目录

- [功能列表](#功能列表)
- [环境要求](#环境要求)
- [快速开始](#快速开始)
- [目录结构](#目录结构)
- [架构概览](#架构概览)
- [13 个安装阶段](#13-个安装阶段)
- [相关文档](#相关文档)
- [免责声明](#免责声明)

---

## 功能列表

### 部署能力

| 能力 | 说明 |
|---|---|
| 双模式取包 | **下载模式**（HTTP Range 多线程分块 + 断点续传 + SHA-256）／**本地模式**（原生对话框选择，扩展名不限） |
| **安全软件处置** | 解压前用 SecurityCenter2 检测；移除 Defender（复用仓库工具，**不强制重启**）；第三方杀软可终止/卸载，**停不掉就明确告知你手动从系统托盘退出**并提供重检按钮 |
| 分片下载 | 每片一个 URL，可任意数量；**保留分卷原始命名**，直接交给 7z 整套识别（不做无谓拼接） |
| 7z 变体兼容 | 自动检测 7-Zip / **NanaZip** / 7-Zip ZS / PeaZip / Bandizip 等兼容实现；找不到时可按配置自动下载官方 `7zr.exe` |
| 扩展名无关 | 7z 按**文件头**识别格式；`.php` / `.dat` / 无扩展名的压缩包同样可解压；缓存文件自动补真实后缀 |
| 大小写不敏感查找 | 所有补丁包查找均为不区分大小写（`_SolidSquad_`、`license.REG`、`sw2024.ISO` 都能命中） |
| 按关键词找进程 | 关键词正则**不区分大小写**、忽略首尾空白，可选同时匹配**进程路径与命令行** |
| 配置导出 | 一键导出当前配置为 TOML：**留空项保持磁盘现有值**，注释保留、新字段补齐 |
| 两阶段解压 | 主包解压 → 匹配文件名含 `solidsquad` 的压缩包（扩展名不限）→ 二次解压得到 `_SolidSQUAD_` |
| 网络隔离 | 安装期间禁用物理网卡；仅跳过虚拟/回环/VPN 适配器；**Drop 守卫保证异常退出也恢复** |
| 注册表注入 | 递归收集 `_SolidSQUAD_/**/*.reg` 并逐个 `reg import` |
| 镜像挂载 | PowerShell `Mount-DiskImage` 挂载 ISO，自动解析盘符并定位 `setup.exe` |
| 静默安装 | 首选 `sldim\startswinstall.exe /install /now`；不可用或配置了组件白名单时回退 `msiexec /qb` |
| 弹窗守护 | Win32 `EnumWindows` + `SendMessage` 自动处理三类安装对话框 |
| 完成检测 | 日志 / 进程退出 / `SLDWORKS.exe` 三项中满足两项即判定完成 |
| 进程清理 | 正则匹配 → `taskkill /T` 优雅关闭 → 超时 `taskkill /F /T` → Win32 `TerminateProcess` 兜底 |
| 文件替换 | `_SolidSQUAD_/SolidWorks Corp/` 递归覆盖安装目录 |
| FlexNet 服务 | `server_remove.bat` → `server_install.bat` → 轮询 `sc query` 至 `RUNNING` |

### 配置能力

- 默认配置通过 `include_str!` **嵌入二进制**
- 用户配置存于 `{app_data_dir}/config.toml`
- 启动时用 **`toml_edit` 结构保留合并**：保留用户已有值与注释，自动补齐新字段
- 前端可 **查看 / 修改 / 保存 / 重置 / 拉取远程配置**
- 所有耗时操作的超时与间隔 **全部来自 TOML，无硬编码**
- 时间参数单位统一为 **分钟（float，`0.5` = 30 秒）**

### 界面能力

- **endfield 风格族**：米白纸面 + 炭黑器械坞站 + 信号黄强调，零圆角、1px 细线
- **maximal 深度**：逐屏编排、最高 6 层协同舞台层、状态驱动仪表、完整动效系统
- 三页结构：**部署向导**（9 步追踪器）／**参数配置**（11 分组表单）／**日志档案**（过滤·搜索·导出）
- 进度经 **`tauri::ipc::Channel<InstallEvent>`** 流式推送，端到端类型安全
- 完整无障碍：语义化地标、`aria-current` 状态、2px 信号色焦点环、`prefers-reduced-motion` 静态编排

---

## 环境要求

### 运行环境

| 项 | 要求 |
|---|---|
| 操作系统 | Windows 10 1809+ / Windows 11（64 位） |
| 权限 | **必须以管理员身份运行** —— 注册表导入、网卡禁用、服务安装都要求提升权限 |
| 用户名 | 必须是**纯 ASCII**（不含中文/特殊符号） |
| 计算机名 | 必须是**纯 ASCII**，否则 FlexNet 许可服务无法启动；改名后需**重启** |
| WebView2 | Windows 11 内置；Windows 10 需安装 Evergreen Runtime |
| 磁盘空间 | 系统盘 ≥ 2 GB（应用与缓存）；**安装目标盘 ≥ 20 GB**，建议 30 GB |
| 内存 | ≥ 4 GB |

### 构建环境

| 工具 | 版本 | 说明 |
|---|---|---|
| Rust | 1.77+（stable-x86_64-pc-windows-msvc） | 后端 |
| Node.js | 20+ | 前端构建 |
| MSVC 构建工具 | Visual Studio 2022 Build Tools（含 C++ 工具集） | Rust 链接所需 |
| WebView2 SDK | 随 `webview2-com` crate 自动获取 | —— |

### 可选资源

**7z 解压程序**按以下优先级自动查找，**通常无需任何手工操作**：

| # | 位置 | 说明 |
|---|---|---|
| 1 | `[sevenzip].exe_path` | 配置中显式指定 |
| 2 | 随程序打包的 `resources/` | 离线部署首选 |
| 3 | `{app_data_dir}` 的下载缓存 | 自动下载后落地，后续启动直接复用 |
| 4 | 注册表 `HKLM\SOFTWARE\7-Zip` | 已安装 7-Zip 的情况 |
| 5 | 系统 `PATH` | 最后兜底 |

全部落空时，默认会从 **7-Zip 官方发布的独立控制台程序**
[`7zr.exe`](https://github.com/ip7z/7zip/releases/latest)（LGPL，单文件约 600 KB）
自动下载并缓存：

```toml
[sevenzip]
auto_download = true
download_url = "https://github.com/ip7z/7zip/releases/latest/download/7zr.exe"
```

想要**完全离线**，任选其一：

```powershell
# A. 打包进程序（同时要在 tauri.conf.json 的 bundle.resources 里加上它）
Copy-Item "C:\Program Files\7-Zip\7z.exe" "src-tauri\resources\7z.exe"

# B. 依赖机器上已装的 7-Zip
winget install 7zip.7zip

# C. 关掉自动下载，改用 A 或 B
#    [sevenzip] auto_download = false
```

详见 [`src-tauri/resources/README.md`](../src-tauri/resources/README.md)（含许可说明）。

---

## 快速开始

### 1. 安装依赖

```powershell
npm install
```

### 2. 补齐 7z.exe（通常不需要）

程序会自动查找 7z（配置 → 打包资源 → 下载缓存 → 已安装的 7-Zip → PATH），
都找不到时默认从官方源自动下载 `7zr.exe`。
**完全离线**的环境才需要手工打包：

```powershell
Copy-Item "C:\Program Files\7-Zip\7z.exe" "src-tauri\resources\7z.exe"
# 然后在 tauri.conf.json 的 bundle.resources 里追加 "resources/7z.exe"
```

### 3. 准备应用图标

图标已随仓库生成（`src-tauri/icons/`）。如需重新生成：

```powershell
python src-tauri/icons/generate_icons.py
```

### 4. 开发运行

```powershell
npm run tauri:dev
```

### 5. 静态检查

```powershell
cargo check --manifest-path src-tauri/Cargo.toml   # 后端
npm run build                                       # 前端（vite build）
npx svelte-check --tsconfig ./tsconfig.json         # 前端类型检查（可选）
```

### 6. 打包

```powershell
# tauri.conf.json 的 bundle.active 需先改为 true
npm run tauri:build
```

产物位于 `src-tauri/target/release/bundle/`。

### 常用脚本

| 命令 | 作用 |
|---|---|
| `npm run dev` | 仅启动 Vite 开发服务器（浏览器预览，无后端） |
| `npm run build` | 构建前端到 `dist/` |
| `npm run tauri:dev` | 启动完整桌面应用（含热重载） |
| `npm run tauri:build` | 打包 Windows 安装包 |
| `npm run check` | `svelte-check` 类型与模板检查 |

---

## 目录结构

```
solidworks-installer/
├── .skills/
│   └── ark-ui-skill-main/            # 设计规范来源（SKILL.md + references/）
├── src/                              # 前端（Svelte 5 + Vite + TypeScript）
│   ├── App.svelte                    # 外壳：导轨 + 坞站 + 舞台 + 哈希路由
│   ├── main.ts                        # 挂载入口
│   ├── index.html                     # (仓库根) HTML 入口与 root 属性
│   ├── lib/
│   │   ├── api/
│   │   │   ├── types.ts               # 与 Rust serde 结构体对应的类型
│   │   │   ├── commands.ts            # invoke() 封装
│   │   │   └── channels.ts            # Channel<InstallEvent> 封装
│   │   ├── components/
│   │   │   ├── ConfigPanel.svelte     # 11 分组配置表单
│   │   │   ├── EnvironmentPanel.svelte# 环境实测读数
│   │   │   ├── InstallProgress.svelte # 状态驱动仪表
│   │   │   ├── LogViewer.svelte       # 日志查看/过滤/搜索/导出
│   │   │   ├── RailNav.svelte         # 竖向导轨（竖屏转底部行动条）
│   │   │   ├── StageField.svelte      # 6 层舞台层
│   │   │   ├── StationHeader.svelte   # 炭黑器械坞站
│   │   │   ├── StatusChip.svelte      # 标签+值读数
│   │   │   ├── StepTracker.svelte     # 9 步追踪器
│   │   │   ├── SectionTitle.svelte    # 分区标题（大编号 + 引导线）
│   │   │   └── ActionButton.svelte    # 方形行动按钮
│   │   ├── stores/
│   │   │   ├── config.ts              # 配置状态（runes）
│   │   │   └── install.ts             # 安装状态（runes）+ 事件折叠
│   │   └── styles/
│   │       ├── tokens.css             # endfield 家族 + maximal 深度令牌
│   │       └── global.css             # 共享组件与响应式编排
│   └── routes/
│       ├── InstallPage.svelte         # 主页（安装向导）
│       ├── SettingsPage.svelte        # 设置页
│       └── LogsPage.svelte            # 日志页
├── src-tauri/                         # 后端（Rust）
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/                         # 程序化生成的原创图标
│   ├── resources/
│   │   ├── default-config.toml        # 嵌入二进制的默认配置
│   │   └── README.md                  # 7z.exe 补齐说明
│   └── src/
│       ├── main.rs                    # 薄壳
│       ├── lib.rs                     # 应用入口 / Reporter / AppState / 工具
│       ├── config.rs                  # serde + toml + toml_edit 结构保留合并
│       ├── commands.rs                # Tauri 命令层（13 个命令）
│       ├── downloader.rs              # 多线程分块下载引擎
│       ├── antivirus.rs               # 安全软件检测与处置
│       ├── win32_utils.rs             # Win32 API（窗口/网卡/进程/注册表/对话框）
│       ├── process_utils.rs           # 子进程、网卡守卫、服务控制
│       ├── events.rs                  # InstallEvent / InstallStatus / StepId
│       └── installer/                 # 13 阶段编排（按阶段拆分，见下）
│           ├── pipeline.rs            #   主编排 run()
│           ├── context.rs             #   InstallContext / 状态单元
│           ├── steps.rs               #   环境检测 + 杀软处置
│           ├── archive/               #   取包：sniff(纯函数) + fetch(下载)
│           ├── sevenzip/              #   7z：lookup(查找) + fetch(下载)
│           ├── extract.rs             #   两阶段解压
│           ├── registry.rs            #   导入注册表
│           ├── iso.rs                 #   挂载 ISO
│           ├── install_ops.rs         #   启动安装器 + 弹窗守护
│           ├── verify.rs              #   轮询检测完成
│           ├── postinstall.rs         #   进程清理 + 文件替换
│           ├── flexnet.rs             #   FlexNet 服务
│           ├── guards.rs              #   Drop 兜底守卫
│           └── paths.rs               #   大小写不敏感查找
├── docs/                              # 项目文档
├── package.json
├── vite.config.ts
├── tsconfig.json
├── svelte.config.js
└── index.html
```

---

## 架构概览

```
┌──────────────────────────────────────────────────────────────┐
│                      Tauri 2 应用进程                         │
│                                                              │
│  前端 (Svelte 5 / Vite / TS)        后端 (Rust)              │
│  ┌────────────────────────┐        ┌──────────────────────┐  │
│  │ App.svelte  外壳+路由   │        │ commands.rs  命令层   │  │
│  │  ├ InstallPage         │◄──IPC─►│ config.rs    配置系统 │  │
│  │  ├ SettingsPage        │        │ downloader.rs 下载引擎│  │
│  │  └ LogsPage            │        │ installer.rs 安装编排 │  │
│  │                        │        │ win32_utils.rs  Win32 │  │
│  │ stores/  runes 状态     │        │ process_utils.rs 进程 │  │
│  │  ├ config.ts           │        │ events.rs    事件类型 │  │
│  │  └ install.ts          │        └──────────────────────┘  │
│  └────────────────────────┘                                  │
│         ▲                 ▲                                  │
│         │ invoke()        │ Channel<InstallEvent>（流式）      │
│         └─────────────────┘                                  │
│                                                              │
│  内置资源：resources/default-config.toml（include_str!）       │
└──────────────────────────────────────────────────────────────┘
```

### 通信机制

**命令（前端 → 后端，请求/响应）**

`get_config` · `save_config` · `reset_config` · `fetch_remote_config` ·
`get_default_config` · `env_check` · `pick_archive` · `start_install` ·
`cancel_install` · `get_install_status` · `export_logs`

**流式进度（后端 → 前端，单次调用内的有序推送）**

`start_install` 额外接收 `tauri::ipc::Channel<InstallEvent>`。与 `app.emit()` 相比：

| 特性 | `Channel<T>` | `app.emit()` |
|---|---|---|
| 类型安全 | **端到端**（消息类型是命令签名的一部分） | 运行时字符串事件名 |
| 生命周期 | **与命令绑定**，调用结束自动失效 | 需手动 `unlisten` |
| 消息顺序 | **保序** | 跨事件可能重排 |
| 适用场景 | 高频进度、日志流 | 全局广播 |

安装线程通过 `Reporter`（`Arc<dyn Fn(InstallEvent)>`）推送事件；
命令在收到完成信号前保持挂起，因此 Channel 在整个安装期间保持打开。

**取消机制**：`cancel_install` 置位 `Arc<AtomicBool>`，
编排的每个阶段都在检查点读取该标记（子进程运行、轮询循环、文件复制均覆盖）。

**异常安全**：网络恢复、ISO 卸载、临时目录清理均由 `Drop` 实现；
编排主体包在 `catch_unwind` 中，panic 展开也会触发全部守卫。

### 关键设计决策

| 决策 | 原因 |
|---|---|
| `reqwest` 使用 `native-tls` 而非 `rustls` | Windows 走 SChannel，无需编译 `ring` 的 C 代码，构建更快且无额外工具链依赖 |
| 下载采用「预生成 worker 池 + 有界通道」 | 连接池保持预热，避免每分块生成任务的开销；写盘由单一写线程完成，无需文件锁 |
| 文件选择对话框用 Win32 `IFileOpenDialog` 自实现 | 少一个插件依赖，且能精确控制过滤器与返回值 |
| `.part` + `.state` 双文件断点续传 | `.state` 位图按块记录完成状态，中断后只补缺失分块 |
| `Reporter` 而非直接传 `Channel` | 编排层不依赖 Tauri 运行时，模块边界清晰 |

---

## 13 个安装阶段

| # | 阶段 | 逻辑步 | 关键配置 |
|---|---|---|---|
| 1 | 环境检测 | 01 | —— （ASCII / 管理员 / 磁盘） |
| 2 | 获取压缩包 | 02 | `[download]` |
| 2.5 | 安全软件处置 | 02 | `[antivirus]` |
| 3 | 禁用网络 | 03 | `[network]` |
| 4 | 第一次解压 | 04 | `[sevenzip].extract_timeout_minutes` |
| 5 | 第二次解压（`*solid*squad*`，扩展名不限） | 04 | 同上 |
| 6 | 导入注册表 | 05 | —— |
| 7 | 挂载 ISO 并安装 | 06 | `[install]` |
| 8 | 弹窗处理守护 | 07 | `[install.popup]` |
| 9 | 轮询检测完成 | 08 | `[install.polling]` |
| 10 | 关闭 SW 相关进程 | 09 | `[process]` |
| 11 | 文件替换 | 09 | `[install]` |
| 12 | 安装 FlexNet 服务 | 09 | `[flexnet]` |
| 13 | 收尾 | 09 | `[network]` `[workdir]` |

详细说明（每步的输入、输出、超时来源、失败策略）见
[`INSTALL_FLOW.md`](./INSTALL_FLOW.md)。

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CUSTOMIZE.md`](./CUSTOMIZE.md) | **部署前必须自行修改的内容**（下载地址、7z 来源、进程关键词、identifier 等）、快速自检清单、已知局限 |
| [`CONFIG_GUIDE.md`](./CONFIG_GUIDE.md) | **我需要填什么** —— 每个键是否必填、填什么、最小可用示例、自检清单 |
| [`COMMAND_LINE.md`](./COMMAND_LINE.md) | 官方命令行属性参考：属性名、语言代码表、ADDLOCAL 层级、介质路径 |
| [`CONFIG.md`](./CONFIG.md) | 配置文件位置、每个配置项的名称/类型/默认值/取值/作用/示例、远程拉取与重置机制、空白默认配置文件 |
| [`INSTALL_FLOW.md`](./INSTALL_FLOW.md) | 13 步详解、弹窗规则表、轮询逻辑、错误处理与恢复策略 |
| [`DEVELOPMENT.md`](./DEVELOPMENT.md) | 环境搭建、前端组件与令牌、后端新增命令/事件、构建打包、代码规范 |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | 中文用户名、网络未禁用、权限不足、服务启动失败、下载排查等 |

---

## 免责声明

本项目是**部署流程自动化工具**，不包含、不分发任何 SolidWorks 安装介质、
序列号或许可绕过组件。使用者需要自行提供合法授权的安装包与许可，
并自行承担使用风险与合规责任。请遵守所在地法律与软件许可协议。
