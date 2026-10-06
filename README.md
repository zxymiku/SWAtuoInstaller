# SolidWorks 2024 SP5 自动安装器

一个面向 Windows 的 Tauri 2 + Svelte 5 桌面安装工具。它把压缩包获取、断点续传、两阶段解压、ISO 挂载、静默安装、完成检测、文件替换和 FlexNet 服务处理编排成一个可观察的流程。

> 本项目只处理安装编排，不包含 SolidWorks 安装介质、序列号或授权文件。请只使用你有权使用的安装包，并遵守 Dassault Systèmes 的许可条款。

## 能力

- 本地压缩包或 HTTP(S) 下载；支持 Range 分块、断点续传和可选 SHA-256 校验。
- 本地模式支持选择单个压缩包，或选择包含完整 `.001/.002/...` 分卷的文件夹。
- 支持标准 7z 分卷（保留 `.001/.002` 文件名）以及需要顺序拼接的非标准分片。
- 通过文件头识别压缩格式，不依赖扩展名。
- 两阶段解压：主安装包 -> `*solidsquad*` 补丁包 -> `_SolidSQUAD_` 目录。
- ISO 挂载、安装入口定位、`StartSWInstall.exe` / `msiexec` 回退。
- 日志、进度事件、弹窗处理、安装完成轮询和资源清理。
- 可选的 Defender/第三方安全软件处置、网络隔离和 FlexNet 服务处理。

## 日志与手动测试

应用启动后立即在应用数据目录的 `logs/` 下创建一份会话日志，例如
`session-20261006-025242.log`。日志会记录启动、配置加载、每次按钮点击、对话框取消、
操作成功或失败、当前逻辑步骤、命令失败原因、重试结果、取消/异常退出和最终状态；
下载线程与安装线程共用同一日志出口。部署页可实时查看和筛选日志，安装结束后优先保留
这份磁盘日志用于排查。若某个动作没有得到后端响应，前端也会记录请求失败或超时上下文。

## 安全边界

这是一个需要管理员权限的系统工具，默认配置会执行网卡开关、注册表导入、进程终止、服务操作和安全软件处置。运行前请审阅 `src-tauri/resources/default-config.toml`，尤其是：

- `network.disable_network`、`antivirus.*`、`process.*` 和 `flexnet.*`。
- 下载地址尽量使用 HTTPS，并为大型安装包配置 `download.checksum_sha256`。
- 远程配置只接受 HTTPS，远程内容会限制大小并按 TOML 解析；不要把不可信地址写入配置。
- 自动下载的 `7zr.exe` 只接受 HTTPS，限制为 32 MiB 以内，并检查 PE 文件头；生产环境仍建议把校验后的 7z 放入资源目录。
- 临时目录只会清理程序创建的 `solidworks-install` 子目录。安装失败排查时可将 `workdir.cleanup_temp_after` 设为 `false`。
- 不要把未知来源的脚本、批处理文件或注册表文件放入安装包。程序会执行介质中的 `.reg`、`server_*.bat` 和安装器。

详细审查记录见 [`docs/SECURITY_AUDIT.md`](docs/SECURITY_AUDIT.md)。

## 使用

1. 在 Windows 10 1809+ 或 Windows 11 64 位系统上，以管理员身份运行。
2. 确保用户名和计算机名为 ASCII；FlexNet 对非 ASCII 计算机名可能无法启动。
3. 在设置页填写下载地址或选择本地压缩包，检查工作目录、目标盘和安全软件策略。
4. 首次运行建议保留 SHA-256 校验，并关闭不需要的网络/安全软件自动处置选项。
5. 点击部署，按日志处理需要人工确认的安全软件或 UAC 提示。

完整安装阶段说明见 [`docs/INSTALL_FLOW.md`](docs/INSTALL_FLOW.md)，配置说明见 [`docs/CONFIG_GUIDE.md`](docs/CONFIG_GUIDE.md)，故障排查见 [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)。

## 开发

环境：

- Windows 10/11 64 位
- Rust stable 1.77+
- Node.js 20+
- Visual Studio 2022 Build Tools（MSVC）

常用命令：

```powershell
npm install
npm run check
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri:dev
npm run tauri:build
```

打包产物位于 `src-tauri/target/release/bundle/`。7z 可从已安装的 7-Zip、资源目录、应用缓存或配置的 HTTPS 地址解析；离线部署建议把经过验证的 `7z.exe` 放入 `src-tauri/resources/` 并关闭自动下载。

当前 Windows x64 NSIS 安装包：
`src-tauri/target/release/bundle/nsis/SolidWorks Installer_1.0.0_x64-setup.exe`。

## 结构

- `src/`：Svelte 页面、配置表单、安装进度和日志界面。
- `src-tauri/src/commands.rs`：前端命令入口。
- `src-tauri/src/installer/`：安装流程、归档处理、ISO、FlexNet 和收尾守卫。
- `src-tauri/src/downloader/`：HTTP 下载引擎及断点状态。
- `src-tauri/src/process_utils.rs`、`win32_utils.rs`：外部进程和 Windows API 封装。
- `src-tauri/resources/default-config.toml`：嵌入程序的默认配置。

## 许可证

项目代码使用 MIT 许可证（见 `src-tauri/Cargo.toml`）。第三方工具、SolidWorks 安装介质和补丁文件分别受其自身许可证约束。
