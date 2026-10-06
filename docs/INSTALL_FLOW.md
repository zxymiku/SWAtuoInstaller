# 安装流程详解

本文件逐阶段说明 13 个物理步骤的**输入、输出、超时来源、失败策略**，
并给出弹窗处理规则表、轮询检测逻辑与整体错误恢复策略。

> **步骤编号**：13 是后端的**物理阶段**编号；界面上的 9 步追踪器是
> **逻辑步骤**。两者的映射关系在下表列出。

---

## 目录

- [阶段总览](#阶段总览)
- [物理阶段 → 逻辑步骤映射](#物理阶段--逻辑步骤映射)
- [执行时序图](#执行时序图)
- [第 1 步：环境检测](#第-1-步环境检测)
- [第 2 步：获取压缩包](#第-2-步获取压缩包)
- [第 2.5 步：安全软件处置](#第-25-步安全软件处置)
- [第 3 步：禁用网络](#第-3-步禁用网络)
- [第 4 步：第一次解压](#第-4-步第一次解压)
- [第 5 步：第二次解压](#第-5-步第二次解压)
- [第 6 步：导入注册表](#第-6-步导入注册表)
- [第 7 步：挂载 ISO 并安装](#第-7-步挂载-iso-并安装)
- [第 8 步：弹窗自动处理](#第-8-步弹窗自动处理)
- [第 9 步：轮询检测安装完成](#第-9-步轮询检测安装完成)
- [第 10 步：关闭 SW 相关进程](#第-10-步关闭-sw-相关进程)
- [第 11 步：文件替换](#第-11-步文件替换)
- [第 12 步：安装 FlexNet 服务](#第-12-步安装-flexnet-服务)
- [第 13 步：收尾](#第-13-步收尾)
- [弹窗处理规则表](#弹窗处理规则表)
- [轮询检测逻辑](#轮询检测逻辑)
- [错误处理与恢复策略](#错误处理与恢复策略)

---

## 阶段总览

| # | 阶段 | 输入 | 输出 | 超时来源 | 失败即终止 |
|---|---|---|---|---|---|
| 1 | 环境检测 | 系统 API | 环境快照 | —— | 是 |
| 2 | 获取压缩包 | `[download]` 或本地选择 | 压缩包路径 | `[download]` 各项 + `[sevenzip]` | 是 |
| **2.5** | **安全软件处置** | `[antivirus]` | 已移除/已停用的杀软清单 | `[antivirus]` 各项 | **否**（只提示） |
| 3 | 禁用网络 | `[network]` | 已禁用的适配器列表 | netstat 命令固定 60 秒 | 否（记录后继续） |
| 4 | 第一次解压 | 压缩包 | `workdir\folder1\` | `[sevenzip].extract_timeout_minutes` | 是 |
| 5 | 补丁目录识别 | `*solidsquad*` 压缩包或已展开目录 | 补丁根目录（`_SolidSQUAD_`） | `[sevenzip].extract_timeout_minutes` | 是 |
| 6 | 导入注册表 | `_SolidSQUAD_\**\*.reg` | 注册表项 | `reg import` 固定 120 秒/个 | 是（汇总失败） |
| 7 | 挂载 ISO 并安装 | `folder1\*.iso` | 安装介质根目录 | `[sevenzip].extract_timeout_minutes` | 是 |
| 8 | 弹窗守护 | `[install.popup]` | 已处理弹窗记录 | `[install.popup].popup_timeout_minutes` | 否 |
| 9 | 轮询检测 | `[install.polling]` | 完成判定 | `[install.polling].timeout_minutes` | 是 |
| 10 | 关闭进程 | `[process]` | 进程终止记录 | `[process].kill_timeout_minutes` | 否（记录后继续） |
| 11 | 文件替换 | 补丁根目录中的 `SOLIDWORKS Corp` 等目录 | 已覆盖的安装目录 | —— （逐文件重试） | 是（汇总失败） |
| 12 | FlexNet 服务 | `server_*.bat` | RUNNING 的服务 | `[flexnet]` 两项 | 是 |
| 13 | 收尾 | `[network]` `[workdir]` | 已恢复的环境 | PowerShell 固定 180 秒 | 否 |

---

## 物理阶段 → 逻辑步骤映射

| 物理阶段 | 逻辑步骤 | 界面编号 | 逻辑步骤 ID |
|---|---|---|---|
| 1 | 环境检测 | 01 | `environment` |
| 2 | 获取压缩包 | 02 | `acquire` |
| 2.5 | 安全软件处置 | 02 | `acquire` |
| 3 | 禁用网络 | 03 | `network` |
| 4 + 5 | 两阶段解压 | 04 | `extract` |
| 6 | 导入注册表 | 05 | `registry` |
| 7 | 挂载镜像 | 06 | `mount_iso` |
| 8 | 静默安装 | 07 | `install` |
| 9 | 轮询检测 | 08 | `verify` |
| 10 + 11 + 12 + 13 | 收尾恢复 | 09 | `finalize` |

映射由 `StepId::logical_index()` 实现，界面因此始终显示 01~09，不会出现空洞。

---

## 执行时序图

```
 用户点击「开始自动部署」
      │
      ▼
 ① 环境检测 ──失败──► 推送 Error(environment) ──► 结束
      │ 通过
      ▼
 ② 获取压缩包 ──本地选择──┐
      │                   │
      └──下载模式─────────┘  ──失败──► 推送 Error(download) ──► 结束
      │ 得到 archive
      ▼
 ②.5 安全软件处置（必须在解压前，否则补丁文件会被杀软删掉）
      ├─ SecurityCenter2 检测 ──► 告知用户检测到哪些杀软
      ├─ 移除 Defender（复用 windows-defender-remover，但不重启）
      ├─ 第三方杀软：按 third_party_mode 终止进程 / 停服务 / 尝试卸载
      └─ 仍有活动防护 ──► 推送 AntivirusReport ──► 界面提示手动从托盘退出
      │ （本步永不失败，只影响提示）
      ▼
 ③ 禁用网络 ──► NetworkGuard 就绪（Drop 兜底）
      │
      ▼
 ④ 第一次解压 7z x archive -ofolder1 -aoa
      │
      ▼
 ⑤ 识别 *solidsquad* 补丁包；若内容已展开为补丁目录则直接复用，否则第二次解压
      │
      ▼
 ⑥ 在补丁根目录递归查找 .reg → 逐个 reg import
      │
      ▼
 ⑦ 定位 *.iso → Mount-DiskImage → 解析盘符 → 定位 StartSWInstall.exe / setup.exe
      │
      ▼
 ⑧ 启动弹窗守护线程 ────────────────┐
      │                             │ 每 scan_interval_ms 扫描
      ├─ StartSWInstall /install /now│ 命中规则 → 点击 → 推送 PopupDetected
      │  （或 msiexec /qb 回退）      │
      ▼                             │
 ⑨ 轮询检测（日志 / 进程 / 文件）◄────┘
      │ 满足两项
      ▼
      守护线程置停 ──► join
      │
      ▼
 ⑩ taskkill 优雅 → 等待 → 强制 → Win32 兜底；删除许可服务
      │
      ▼
 ⑪ _SolidSQUAD_/SolidWorks Corp/ 下的组件目录 → 安装目录下的同名组件目录（递归覆盖）
      │
      ▼
 ⑫ server_remove.bat → server_install.bat → sc query 至 RUNNING
      │
      ▼
 ⑬ 恢复网络 ──► 卸载 ISO ──► 清理临时目录
      │
      ▼
 推送 Completed(success = true)
```

---

## 第 1 步：环境检测

**输入**：无（直接调用系统 API）

| 检测项 | API | 说明 |
|---|---|---|
| 用户名 | `GetUserNameW` | 判定是否纯 ASCII |
| 计算机名 | `GetComputerNameExW(ComputerNamePhysicalDnsHostname)` | 判定是否纯 ASCII |
| 管理员权限 | `OpenProcessToken` + `GetTokenInformation(TokenElevation)` | 是否已提升 |
| 目标盘可用空间 | `GetDiskFreeSpaceExW` | GB |
| 进程位数 | `size_of::<usize>()` | 64 位提示 |

**输出**：`InstallEvent::EnvironmentCheck` 事件（推送到前端）+ 内部快照

**失败策略**（按顺序短路）：

| 情况 | 行为 |
|---|---|
| 计算机名含非 ASCII | 推送 `Error(environment, recoverable=true)` 并**终止**。提示改名后重启 |
| 用户名含非 ASCII | 同上 |
| 非管理员 | 推送 Error 并终止；同时尝试 `ShellExecuteW("runas")` 主动提权，由用户确认 UAC |
| 可用空间 < 20 GB | 推送 `Warn` 日志，**不终止** |

**为什么必须终止**：中文计算机名会导致 `SolidWorks Flexnet Server`
无法启动；非管理员无法写注册表、禁网卡、装服务。这两类问题不解决，
后续 12 个步骤都不可能成功，继续执行只会浪费时间与磁盘 IO。

---

## 第 2 步：获取压缩包

**输入**：`[download]` 分组，或用户在主页选择的本地文件

### 本地模式

| 项 | 值 |
|---|---|
| 触发 | 主页点击「选择本地压缩包」 |
| 对话框 | Win32 `IFileOpenDialog`，提供 `*.7z;*.zip;*.rar;*.iso` 快捷过滤器但**允许选择任意文件** |
| 准入判定 | **不按扩展名**。选中后按文件头嗅探格式，再用 `7z t` 实际测试一次；7z 明确报错才拒绝，并把 7z 的原话带回界面 |
| 测试超时 | `[sevenzip].extract_timeout_minutes` 的 1/3，钳制在 1~10 分钟。**超时视为放行**（大包完整测试可能很久，交由第 4 步判定） |
| 行为 | 直接使用所选文件，不复制、不校验哈希 |

> **为什么不用扩展名筛选**：下载或手里的安装包经常挂着 `.php` / `.dat` / 甚至没有
> 扩展名，但它们本身仍是合法压缩包。7z 是按**文件头（magic bytes）**识别格式的，
> 扩展名对它没有意义。

### 扩展名无关性（重要）

程序**全程不依赖扩展名**来判定格式：

| 环节 | 做法 |
|---|---|
| 解压命令 | `7z x` **不传 `-t<格式>`**，让 7z 自己按文件头识别 |
| 本地文件准入 | 按文件头嗅探 + `7z t` 实测 |
| 补丁包定位 | 匹配「文件名含 `solidsquad`」，**扩展名不限** |
| `.reg` / `.iso` 收集 | 通配符匹配，**不区分大小写**（`.REG` / `.ISO` 都能命中） |

反过来说：**显式传 `-t` 是有害的**——一个"名字像 zip、实际是 7z"的文件反而会
解压失败。让 7z 自己判定是唯一正确的做法。

程序会按文件头嗅探格式用于**日志与缓存命名**（支持 7z / rar / zip / gzip /
bzip2 / xz / zstd / cab / chm / iso / exe）：

```
文件大小 12458987520 字节，按文件头识别为 rar 格式，扩展名不参与判定
```

嗅探失败也不影响解压，只会在日志里提示「文件头未匹配已知压缩格式；
7z 将自行判定」。

### 下载模式

```
① probe（探测）
   ├─ HEAD 请求取 Content-Length 与 Accept-Ranges
   └─ 失败则 GET + Range: bytes=0-0 → 解析 Content-Range 总长
       │
② 计算分块：chunk = total / threads，钳制在 1~8 MiB
   │        chunks = ceil(total / chunk)
   │
③ 加载 .state 位图（"SWDL1\n" + 每块 1 字节）
   ├─ 服务端不支持 Range → 丢弃 .part 与位图，重新开始
   └─ 分片长度 ≠ 远端声明 → 保留位图但重新校验每块
   │
④ 预分配 .part 到目标长度（set_len）
   │
⑤ 生成 N 个 worker + 1 个写线程
   │   worker：领号 → GET Range → 收集字节 → 投递通道
   │   写线程：seek(offset) → write_all → flush
   │
⑥ 进度上报线程每 200ms 采样一次
   │   推送 DownloadProgress{ bytes_done, bytes_total, speed_bps,
   │                          chunks_done, chunks_total }
   │   并推送 EstimatedTimeRemaining{ seconds }（滑动平均速度）
   │
⑦ 首轮结束仍有缺失块 → 「补齐轮次」串行重试
   │
⑧ 尺寸校验（== 远端声明）→ SHA-256 校验（若配置）
   │
⑨ .part 改名 → 目标文件；删除 .state
```

**断点续传文件**

| 文件 | 位置 | 内容 |
|---|---|---|
| `<文件名>.part` | `{app_data_dir}\cache\` | 实际数据，按偏移写入 |
| `<文件名>.state` | `{app_data_dir}\cache\` | `SWDL1\n` + 每块一字节（`1` 完成 / `0` 未完成） |

**缓存复用规则**

| 条件 | 行为 |
|---|---|
| 目标文件存在 且 `checksum_sha256` 为空 | 直接复用，跳过下载 |
| 目标文件存在 且 配置了校验值 | 重新下载（校验只能发生在下载之后） |
| 用户取消 | 保留 `.part` 与 `.state`，下次续传 |

**缓存文件命名**（URL 后缀不可靠时的处理）

| URL 末段 | 嗅探结果 | 缓存文件名 |
|---|---|---|
| `solidworks2024sp5.7z` | `7z` | `solidworks2024sp5.7z`（原样） |
| `download.php` | `zip` | `download.php.zip` |
| `archive`（无扩展名） | `rar` | `archive.rar` |
| `get?id=123` | `7z` | `get.7z` |
| `/`（无文件名） | `iso` | `solidworks2024sp5.iso` |

下载完成后程序按文件头识别真实格式，并把缓存文件重命名成带正确后缀的名字；
重命名失败只记 `Warn`，**不影响解压**。

复用查找会同时接受「精确名」与「同基名 + 任意已知后缀」
（`download.php` 与 `download.php.zip` 视为同一份），
因此改过 URL 或换过版本也不会白下几十 GB。断点续传的 `.part` / `.state`
永远不会被误认为完整文件。

**超时来源**

| 参数 | 来源 | 作用 |
|---|---|---|
| 建连超时 | `[download].connect_timeout_minutes` | reqwest `connect_timeout`（同时用于远程配置拉取） |
| 请求超时 | `[download].read_timeout_minutes` | reqwest `timeout`（每分块独立计时） |
| 重试等待 | `[download].retry_backoff_minutes` | `base × 2^(n-1)`，上限 30 分钟 |
| 重试次数 | `[download].retry_count` | 超过后进入补齐轮次 |

**失败策略**：推送 `Error(download)` 并终止。SHA-256 失败时**主动删除
`.part` 与 `.state`**，避免损坏数据被后续续传沿用。

---

## 第 2.5 步：安全软件处置

**为什么必须在解压之前**

Windows Defender 与多数第三方杀软会把 `_SolidSQUAD_` 里的补丁文件判为威胁并
**直接删除**，或者锁住被替换的 `sldworks.exe` / `rld.dll`。如果等解压完再处理，
文件已经没了——安装要到第 11 步「文件替换」才暴露问题，且现场很难理解。

**输入**：`[antivirus]` 分组

### 执行流程

```
① SecurityCenter2 检测（detect = true）
   Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct
   └─ 输出：displayName 	 productState 	 pathToSignedProductExe
   └─ 解码 productState 得到「实时防护是否开启」「定义是否最新」
   └─ 空结果 → 明确提示"未能读出"，不假装机器干净

② Windows Defender（remove_defender = true）
   ├─ 定位 windows-defender-remover 的 script 目录
   │   （配置 → exe 同级 othertools\... → 资源目录 → 源码目录）
   ├─ PowerRun.exe powershell ... RemoveSecHealthApp.ps1
   ├─ 对 Remove_Defender\ 与 Remove_SecurityComp\ 下每个 .reg：
   │      PowerRun.exe regedit /s <文件>
   ├─ takeown + icacls + 删除 SmartScreen 相关文件
   └─ **不执行原脚本最后的 shutdown /r /f /t 10**

③ 第三方杀软（third_party_mode）
   ├─ terminate / uninstall：taskkill /PID <pid> /T /F
   ├─ terminate / uninstall：sc stop <服务名>
   ├─ uninstall：按其注册表 UninstallString 尝试 /S、/silent、/quiet
   └─ 复核：重新查询 SecurityCenter2

④ 等待 settle_minutes 让处置生效

⑤ 汇总 → 推送 AntivirusReport 事件
```

**输出**：`InstallEvent::AntivirusReport`，包含

| 字段 | 含义 |
|---|---|
| `detected` | 检测到的全部安全软件（名称、是否内置、实时防护开/关） |
| `outcomes` | 逐个的处置结论（动作、是否已停用、手动处理指引） |
| `manual_required` | **需要用户手动从系统托盘退出的软件名** |
| `defender_removed` | 是否移除了 Defender |
| `reboot_required` | 是否需要重启才彻底生效 |
| `summary` | 一句话总结 |

**超时来源**

| 参数 | 用途 |
|---|---|
| `[antivirus].command_timeout_minutes` | 单条处置命令（PowerRun / regedit / taskkill / sc） |
| `[antivirus].settle_minutes` | 处置后等待其真正停下 |

**失败策略：本步永不返回 Err**

杀软处置失败**不应该**让整个安装失败——最坏情况只是提醒用户手动退出，
而安装本身仍可能成功（例如用户已经把工作目录加进排除项）。所以：

| 情况 | 行为 |
|---|---|
| 找不到 windows-defender-remover | 日志 + 界面明确报错，继续安装 |
| 某个 `.reg` 导入失败 | 记 `Warn`，继续其余文件；最后汇总失败数 |
| 第三方杀软进程杀不掉 | 记 `Warn`，进 `manual_required` |
| SecurityCenter2 查询失败 | 记 `Warn`，提示"未能读出"，继续 |

### 界面如何告知用户

推送 `AntivirusReport` 后，部署页出现 **安全软件面板**，三种状态：

| 状态 | 呈现 |
|---|---|
| **已处置** | 表格列出每个软件 + 各自的处置结果；绿色左边框 |
| **需要手动退出** | 黄色高亮；按名字列出未停用的软件；给出 4 步托盘操作指引；给出该软件自己的 `manual_hint`；提供「重新检测」按钮 |
| **需要重启** | 明确说明"策略与应用已处理，但驱动/服务注册要重启才消失"，并建议先让安装继续 |

**「重新检测」** 调用只读命令 `recheck_antivirus`，重新查询 SecurityCenter2，
列表实时更新。用户退出杀软后点一下即可确认，不需要重跑整个安装。

### 判定标准为什么是 SecurityCenter2

不是"进程是否被杀掉"。第三方杀软普遍有自保护，进程被 `taskkill` 后会立刻拉起，
所以看进程会得到"成功"的假象。**以它是否仍注册为活动防护为准**才算可靠。

### 关于重启

移除 Defender 后，策略注册表与安全中心应用已经处理完，但驱动与服务注册要
重启后才消失。程序因此：

- 不自动重启（原工具会 `shutdown /r /f /t 10`，我们刻意去掉）
- 在界面与日志明确提示需要重启
- 建议**先让安装继续**，安装结束后再重启
- 重启后可运行工具自带的 `script\verify.bat` 确认移除是否彻底

---

## 第 3 步：禁用网络

**输入**：`[network]` 分组

| 参数 | 作用 |
|---|---|
| `disable_network` | 为 false 时整步跳过 |
| `restore_network_after` | 是否在 Drop 中恢复 |
| `adapter_disable_method` | `netsh` 或 `powershell` |

**执行流程**

```
GetAdaptersAddresses(AF_UNSPEC) → 适配器列表
    │
过滤：跳过虚拟/回环/VPN（见 CONFIG.md 的关键字表）
    │
对每个处于 Up 的物理适配器执行禁用命令
    │
记录已禁用的适配器名到 NetworkGuard
```

**输出**：`NetworkGuard` 实例（持有适配器名列表）+ 事件日志

**Drop 兜底（关键设计）**

```rust
impl Drop for NetworkGuard {
    fn drop(&mut self) {
        if !self.disabled || !self.restore { return; }
        for name in &self.adapters {
            let _ = set_adapter_enabled(name, true, &self.method);
        }
        self.disabled = false;
    }
}
```

只要栈正常展开**或被 `catch_unwind` 捕获**，`Drop` 就会重新启用适配器。
收尾步骤（第 13 步）也会主动调用 `restore_now()`，并把 `disabled` 置为 false
以避免重复执行。

**失败策略**：单个适配器禁用失败只记录 `Warn` 日志并继续——
部分适配器（如已被其他程序占用）失败不应阻断整个安装。

> **风险提示**：若把 `restore_network_after` 设为 `false`，
> 程序退出时不会恢复网络，需要手动在「网络连接」中重新启用。

---

## 第 4 步：第一次解压

**输入**：第 2 步得到的压缩包

**命令**

```
7z x "<archive>" -o"<workdir>\folder1" -aoa -bb1 -y
```

| 参数 | 含义 |
|---|---|
| `x` | 按完整路径解压，保留目录结构 |
| `-o<dir>` | 输出目录 |
| `-aoa` | 覆盖所有已存在文件（不询问） |
| `-bb1` | 输出被处理的文件名（用于进度显示） |
| `-y` | 对所有询问回答「是」 |

> **没有 `-t<格式>`**：7z 按文件头自动识别格式。压缩包叫什么名字、
> 有没有扩展名都不影响解压；反过来，按扩展名强行指定 `-t` 会让
> "名字像 zip、实际是 7z"的文件解压失败。

**输出**：`{workdir}\folder1\` 目录树

**超时来源**：`[sevenzip].extract_timeout_minutes`（默认 30 分钟）

**进度上报**：从 `-bb1` 输出中解析最后处理的文件名，
推送 `ExtractProgress{ percent, current_file, pass: 1 }`。
`percent` 按阶段权重给定（第一遍 50%，第二遍 100%）。

**子进程管理**

- 使用 `CREATE_NO_WINDOW` 隐藏控制台窗口
- stdout / stderr 由独立线程读取，避免管道写满导致子进程阻塞
- 输出上限 256 KiB（防止列出百万文件时吃满内存）
- 取消标记在每个检查点（120ms 轮询）被读取，命中即 `kill`

**失败策略**

| 情况 | 行为 |
|---|---|
| 超时 | `kill` 子进程，返回「解压超时（超过 N 分钟）」并终止 |
| 7z 不存在 | 在启动前由 `resolve_seven_zip` 报错并终止 |
| 非 0 退出码 | 附上输出末尾 6 行并终止 |
| 用户取消 | 返回「解压被用户取消」并终止 |

---

## 第 5 步：补丁目录识别与第二次解压

**输入**：`{workdir}\folder1\` 中文件名含 `solidsquad` 的压缩包，或已经展开的补丁目录。

如果目录顶层同时存在 `.reg` 文件，并包含 `SOLIDWORKS Corp` 或
`SolidWorks_Flexnet_Server`，程序判定它已经是最终补丁根目录，直接复用，
不再执行第二次解压。

**匹配规则**：大小写不敏感的子串匹配，**扩展名不限**

| 会被匹配的文件名 |
|---|
| `SolidSQUAD_Patch.7z` |
| `solidsquad.rar` |
| `_SolidSquad_.zip` |
| `SolidSQUAD_Patch`（无扩展名） |

实现是 `find_files_matching(folder1, "*solid*squad*")`——
自实现的通配符匹配（`*` / `?`），**不区分大小写**。

**输出**：补丁根目录。传统压缩包输出 `_SolidSQUAD_` 目录；已展开结构直接输出
`folder1`（或 `folder2`）。

**失败策略**

| 情况 | 行为 |
|---|---|
| 未匹配到文件名含 `solidsquad` 的压缩包且目录不是已展开结构 | 报错并终止（列出搜索目录） |
| 解压后未找到 `_SolidSQUAD_` 或已展开补丁结构 | 报错并终止 |

> 搜索发生在二次解压**之前**，因此匹配时不会排除 `_SolidSQUAD_/` 目录里的路径——
> 补丁包本身就常常放在这样的目录名下。

---

## 文件名大小写处理

Windows 的 NTFS 默认**不区分**文件名大小写，但**字符串比较是区分的**。
补丁包的目录名与文档往往不一致，因此程序在**所有**查找环节都显式做不区分大小写比较。

| 查找目标 | 匹配方式 | 能命中的实际写法 |
|---|---|---|
| `_SolidSQUAD_` 目录 | 名字精确比较（忽略大小写） | `_SolidSQUAD_` / `_SolidSquad_` / `_solidsquad_` |
| 补丁压缩包 | 通配符 `*solid*squad*`（忽略大小写） | `SolidSQUAD_Patch.RAR` / `solidsquad.7z` |
| 注册表补丁 | 通配符 `*.reg`（忽略大小写） | `license.REG` / `Settings.reg` / `X.Reg` |
| ISO 镜像 | 通配符 `*.iso`（忽略大小写） | `sw2024.ISO` / `SW2024.iso` |
| `SolidWorks Corp` 目录 | 名字精确比较（忽略大小写） | `SolidWorks Corp` / `SOLIDWORKS corp` |
| `server_install.bat` | 名字精确比较（忽略大小写） | `server_install.BAT` / `SERVER_INSTALL.bat` |
| 安装日志 | 通配符 `*.log` / `*.txt` / `*.html`（忽略大小写） | `Setup.LOG` / `install.TXT` |

### 为什么不直接用 `glob` crate

`glob` crate 的匹配是**逐字符比较**的，在 Windows 上一样是大小写敏感的。
`glob("**/_solidsquad_")` 会直接漏掉 `_SolidSQUAD_`。

程序改为**自行遍历目录树 + 逐段不区分大小写比较**，并让通配符参与
不区分大小写的匹配（`wildcard_ci`）。这样同时解决了两个问题：
大小写不一致，以及"文件名里有 solidsquad 但扩展名不是 `.7z`"。

**安全护栏**：遍历深度上限 24 层、单次查找结果上限 512 条，
避免在 `C:\` 这类目录上失控或遇到符号链接循环。

---

## 第 6 步：导入注册表

**输入**：`_SolidSQUAD_\**\*.reg`（扩展名不区分大小写）

**命令**：`reg import "<文件绝对路径>"`（每个文件一次）

**输出**：注册表项已写入

**超时**：每个文件固定 120 秒（注册表导入是本地操作，时间与配置无关）

**进度上报**：`InstallProgress{ percent, "注册表 n/m" }`

**排序**：文件按路径排序后依次导入，保证结果可复现。

**失败策略**：单个文件失败只记录 `Warn` 并继续处理剩余文件；
**全部处理完后若有任何失败**，汇总错误信息并终止（附失败文件清单与 tail）。

> **权限要求**：`reg import` 需要管理员权限。第 1 步已做过权限校验，
> 若在此处仍失败，通常是注册表策略限制（可查看 `TROUBLESHOOTING.md`）。

---

## 第 7 步：挂载 ISO 并安装

### 7.1 定位 ISO

```
在不区分大小写的递归查找中匹配 "*.iso"
   → 取第一个（.iso / .ISO / .Iso 都能命中，不限层级）
```

### 7.2 挂载镜像

```powershell
Mount-DiskImage -ImagePath "<iso>" -PassThru |
  Get-Volume | Select-Object -ExpandProperty DriveLetter
```

**盘符解析**：从输出中找第一个长度为 1 且为字母的行。

**兜底**：解析失败时，遍历 `D:` ~ `Z:`，查找含
`StartSWInstall.exe` / `setup.exe` / `64bit` 目录的盘符。

**超时**：`[sevenzip].extract_timeout_minutes`

**Drop 兜底**：`IsoMountGuard::drop()` 会在任何退出路径执行
`Dismount-DiskImage`，第 13 步也会主动卸载。

### 7.3 定位安装入口

按以下顺序查找 `startswinstall.exe`（Windows 路径不区分大小写）：

1. `<root>\sldim\startswinstall.exe`
2. `<root>\startswinstall.exe`
3. `<root>\sldIM\StartSWInstall.exe`
4. `<root>\setup\startswinstall.exe`

> **真实布局**（在 SolidWorks 2024 SP5 `Premium.DVD` 上核对）：
> 它在 **`sldim\` 子目录**里，根目录只有 `setup.exe`（GUI 引导程序）。
> 早期版本把它放在根目录，所以两种位置都查。
> 全部找不到时才回退到 `msiexec`。

### 7.4 静默安装

**首选方案**

```
<sldim>\startswinstall.exe /install /now [install_switches...]
```

工作目录设为**介质根**（不是 `sldim\`）—— 安装管理器要在同级找
`swwi\`、`PreReqs\` 等目录。

> `/now` 参数会跳过 5 分钟警告对话框，使安装管理器对用户不可见。
> 「前一个安装中的 Windows 重启操作正在等待处理」弹窗因此**不会出现**，
> 但「不能核实此服务器存在」与「端口@服务器」弹窗仍可能出现，
> 由第 8 步守护线程覆盖。

该进程**不等待**（`spawn_detached`），启动后立即进入第 9 步轮询。

**回退方案**（未找到 `StartSWInstall.exe`，或启动失败）

```
msiexec /i "<root>\swwi\data\solidworks.msi"
        INSTALLDIR="<install_path>"
        [ADDLOCAL=<components_whitelist 逗号拼接>]
        [SOLIDWORKSSERIALNUMBER=<serial_number>]
        [TOOLBOXFOLDER=<toolbox_folder>]
        ENABLEPERFORMANCE=0
        /qb /norestart
```

MSI 的查找顺序（实测核对）：

1. `<root>\swwi\data\solidworks.msi` ← 真实位置（58.8 MB）
2. `<root>\64bit\SOLIDWORKS\SOLIDWORKS.msi`
3. `<root>\SOLIDWORKS\SOLIDWORKS.msi`
4. `<root>\solidworks.msi`

`ADDLOCAL` 仅在 `components_whitelist` 非空时附加；
`SOLIDWORKSSERIALNUMBER` 仅在 `serial_number` 非空时附加；`TOOLBOXFOLDER` 仅在组件白名单含 `SolidWorksToolbox` 时附加。

> ⚠️ **该属性的值不要含空格**：实测含空格的属性值会让 msiexec
> 弹出用法对话框并卡住，安装不执行、日志不生成；**加引号也无法解决**。
> 默认值已改为不含空格的 `C:\SOLIDWORKS_Data`。

**语言包是独立的 MSI，必须单独安装**（官方文档明确要求）：

```
msiexec /i "<root>\swwi\lang\chinese-simplified\chinese-simplified.msi" /qb
```

语言包命令**不传** `INSTALLDIR` / `ADDLOCAL` —— 官方原文：
「指定 SOLIDWORKS 语言组件安装命令时，请勿指定命令行参数。」

程序先启动主程序 MSI，等待该 MSI 的具体进程退出后立即启动语言包；
Windows Installer 的全局互斥锁仍会防止两个 MSI 同时执行。

**失败策略**：既找不到 `startswinstall.exe` 也找不到 `solidworks.msi` → 报错并终止，错误信息里会列出全部已查找路径。

---

## 第 8 步：弹窗自动处理

**守护线程**：`sw-popup-daemon`，从第 7 步启动，第 9 步完成后停止。

**执行循环**

```
while !stop {
    EnumWindows(枚举可见顶层窗口)
        │
    取窗口标题，与规则表逐条匹配（大小写不敏感）
        │
    命中的第一个窗口：
        ├─ 该动作被配置禁用？ → 跳过（既不点击也不产生事件）
        └─ 已启用：
            ├─ 标题在 popup_timeout_minutes 内已处理过？ → 跳过（去重）
            └─ 执行动作
                ├─ 推送 PopupDetected{ title, action } 事件
                ├─ 累加 popups_handled 计数
                └─ 记录 Success 日志
        │
    sleep(scan_interval_ms)
}
```

**窗口标题获取**：`GetWindowTextLengthW` + `GetWindowTextW`，
同时跳过不可见窗口与空标题窗口。

**按钮点击**：`EnumChildWindows` 查找 `Button` 类子窗口，
按文本匹配（去除 `&` 助记符）后 `SendMessageW(hwnd, BM_CLICK)`。
「确定」优先尝试 `GetDlgItem(hwnd, IDOK)`。

**输入框写入**：`EnumChildWindows` 查找 `Edit` 类子窗口，
若内容不为 `25734@localhost` 则用 `SetWindowTextW` 写入，
然后点击「确定」。

**去重窗口**：`popup_timeout_minutes`。同一标题在该时间窗内只处理一次，
避免与用户手动操作互相打架。

**停止**：第 9 步成功后置位 `stop` 标记，主线程 `join` 守护线程。

**失败策略**：守护线程不产生致命错误——所有 Win32 调用失败都只是静默跳过，
不影响安装流程。

---

## 第 9 步：轮询检测安装完成

**输入**：`[install.polling]` 分组

**主循环**

```
started = now()
loop {
    if 用户取消        → 返回 Err("安装已被用户取消")
    
    signals = evaluate_completion()
        ├─ log_check_enabled     → scan_install_logs()
        ├─ process_check_enabled → live_installer_processes().is_empty()
        └─ file_check_enabled    → SLDWORKS.exe 存在？
    
    每 5 秒推送一次进度与调试日志
    
    if signals.is_complete()  → 返回 Ok(())
    if started.elapsed() >= timeout → 返回 Err("轮询超时...")
    
    sleep(clamp(poll_interval, 0.2s, 5s))
}
```

**判定规则**：满足的检查项数量 ≥ 2 即完成；只启用一项时该项即代表全部结论。

详细的判定矩阵与检查实现见 [轮询检测逻辑](#轮询检测逻辑)。

**超时来源**：`[install.polling].timeout_minutes`（默认 240 分钟）

**前端展示**：满足项数 / 3 映射为 `InstallProgress.percent`；
总预算剩余秒数在首轮写入 `EstimatedTimeRemaining`。

**失败策略**：超时后返回错误，错误信息中**列出三项检查的实际状态**，
便于用户判断究竟是哪一项没满足（例如「文件 ✓ 进程 ✓ 日志 ✗」同样判为完成，
而「日志 ✗ 进程 ✗ 文件 ✗」说明安装根本没有进展）。

---

## 第 10 步：关闭 SW 相关进程

**输入**：`[process]` 分组

**执行链**（对每个匹配的进程映像名）

```
① taskkill /IM <名> /T              ← 优雅关闭（含子进程树）
   └─ 成功 → 处理下一个
② 等待 kill_timeout_minutes
   （每 250ms 检查一次取消标记）
③ taskkill /IM <名> /F /T           ← 强制终止
   └─ 仍失败 → OpenProcess(PROCESS_TERMINATE) + TerminateProcess 兜底
```

**进程匹配**：`process.kill_pattern` 正则（不区分大小写）匹配进程映像名，
**当前进程自身永远排除**。

**许可服务处理**：若 `SolidWorks Flexnet Server` 正在运行，
执行 `sc stop` + `sc delete`（为第 12 步重装做准备）。

**失败策略**：`kill_sw_processes = false` 时整步跳过；
其他所有失败都只记录日志，**不终止流程**——
被占用的文件在第 11 步还有一次清除只读位重试的机会。

---

## 第 11 步：文件替换

**输入**：补丁根目录（`_SolidSQUAD_` 或已展开的 `folder1/folder2`）

**源目录选择**（优先级）

1. `_SolidSQUAD_\**\SolidWorks Corp\`（大小写不敏感匹配）
2. `_SolidSQUAD_\**\Program Files\`
3. `_SolidSQUAD_\` 自身

**目标目录**：`{resolved_install_path}\`

补丁中的 `SOLIDWORKS Corp` 只是组件目录的容器，不会作为额外层级复制到目标。其下的 `SOLIDWORKS`、`eDrawings`、`SOLIDWORKS Composer` 等目录分别合并到安装根目录下的同名目录。

**复制方式**：递归复制整个目录树（`copy_tree`）

```
对每个目录项：
    ├─ 目录 → create_dir_all → 递归
    └─ 文件 → fs::copy
        └─ 失败（被占用/只读）→ 清除目标只读位 → 重试一次
            └─ 仍失败 → 记录 Error 日志，计入失败数
```

**进度**：取消标记在每个目录项前检查。

**失败策略**：复制完成后若**失败数 > 0**，汇总错误并终止；
否则推送成功日志（复制 N 个文件）。

> **前置条件**：第 10 步会把 `SLDWORKS.exe` 等进程关掉，
> 否则替换必然失败。若第 10 步被配置禁用，这里很可能出现大量
> 「文件被占用」，此时应先手动关闭 SolidWorks。

---

## 第 12 步：安装 FlexNet 服务

**输入**：`[flexnet]` 分组

**执行流程**

```
① 定位 server_install.bat（在 _SolidSQUAD_ 下递归查找，大小写不敏感）
   └─ 未找到 → 报错并终止

② remove_old_server_first = true ?
   ├─ 找到 server_remove.bat → cmd /C "server_remove.bat"（180 秒超时）
   └─ 未找到 → sc stop + sc delete（直接清理）
   
③ cmd /C "server_install.bat"
   超时：server_install_timeout_minutes
   工作目录：脚本所在目录
   输出：前 40 行逐行写入 Debug 日志

④ 轮询服务状态
   loop {
     sc query "SolidWorks Flexnet Server"
     └─ 输出含 "RUNNING" 且不含 "STOP_PENDING" → 成功
     sleep(server_check_interval_minutes)
     if elapsed >= server_install_timeout_minutes → 失败
   }
```

**进度上报**：`InstallProgress{ percent, "许可服务 n/..." }`

**超时来源**

| 参数 | 用途 |
|---|---|
| `[flexnet].server_install_timeout_minutes` | ① `server_install.bat` 执行超时；② 服务轮询总预算 |
| `[flexnet].server_check_interval_minutes` | 轮询间隔 |

**失败策略**

| 情况 | 行为 |
|---|---|
| 找不到 `server_install.bat` | 报错并终止 |
| 脚本超时 | 报错并终止 |
| 脚本非 0 退出 | 记录 `Warn`，**继续**轮询（脚本退出码未必可靠） |
| 轮询超时 | 报错并终止，提示检查 `server_install.bat` 输出与端口 25734 |

---

## 第 13 步：收尾

**执行顺序（顺序重要）**

```
① 恢复网络
   NetworkGuard::restore_now()
   └─ 逐个 netsh/powershell 启用 → 等待 3 秒让适配器重新 Up
   └─ 更新状态：network_disabled = false

② 卸载 ISO
   IsoMountGuard::dismount_now()
   └─ powershell Dismount-DiskImage -ImagePath "<iso>"（180 秒超时）
   └─ 更新状态：iso_mounted = false

③ 清理临时目录
   TempDirGuard::cleanup_now()
   ├─ cleanup_temp_after = true  → 递归删除 {workdir}
   └─ cleanup_temp_after = false → 记录「按配置保留工作目录」

④ 推送 Completed{ success: true, message: "部署完成，总耗时 ..." }
```

**顺序为何重要**：网络恢复必须在最后一步之前完成（否则用户可能感觉断网）；
镜像卸载必须在文件替换之后（替换源可能仍在镜像上）。
清理放最后，保证出问题时现场还在。

**失败策略**：三步都只记录日志，**不终止流程** ——
流程已走到这里，任何清理失败都不应改变「安装成功」的结论，
但会在日志中留痕。

---

## 弹窗处理规则表

守护线程内置三条规则，标题匹配为**大小写不敏感的子串匹配**。

| 规则 ID | 标题关键词 | 动作 | 受哪个配置控制 |
|---|---|---|---|
| `restart-pending` | `重启`、`重新启动`、`restart` | 点击「确定」 | `auto_click_confirm` |
| `server-unverifiable` | `不能核实此服务器存在`、`无法核实此服务器存在`、`cannot verify` | 点击「是」 | `auto_click_yes` |
| `port-at-server` | `端口@服务器`、`端口 @ 服务器`、`port@server` | 确认输入框为 `25734@localhost` 后点击「确定」 | `auto_click_confirm` |

### 动作实现细节

| 动作 | 实现 |
|---|---|
| 点击「确定」 | ① `GetDlgItem(hwnd, IDOK=1)` → `SendMessageW(BM_CLICK)`；② 失败则 `EnumChildWindows` 查找 `Button` 类且文本匹配「确定」/`OK`/`&OK` 的子窗口 |
| 点击「是」 | `EnumChildWindows` 查找文本匹配 `是(&Y)` / `是(Y)` / `&Yes` / `Yes` / `是` 的按钮 |
| 点击「否」 | 同上，匹配 `否(&N)` / `否(N)` / `&No` / `No` / `否` |
| 确认服务器字段 | `EnumChildWindows` 查找 `Edit` 类子窗口 → 内容 ≠ `25734@localhost` 则 `SetWindowTextW` 写入 → 点击「确定」 |

### 事件

每处理一个弹窗推送一次：

```json
{
  "type": "popup_detected",
  "data": { "title": "<窗口标题>", "action": "点击「确定」" }
}
```

### 前端展示

主页右侧面板显示最近 4 条弹窗记录；日志页有完整的弹窗处理记录表格。

### 关于「前一个安装中的 Windows 重启操作正在等待处理」

该弹窗在使用 `StartSWInstall.exe /install /now` 时**不会出现**（`/now` 跳过
5 分钟等待）。若使用了 `msiexec` 回退路径且系统确实有待处理的重启操作，
可能遇到该弹窗——此时规则 `restart-pending` 会点击「确定」继续。

---

## 轮询检测逻辑

### 三项检查的实现

#### 日志检查（`log_check_enabled`）

**搜索位置**

| 根目录 | 说明 |
|---|---|
| `{workdir}` | 解压目录，某些安装器会把日志写在这里 |
| `{resolved_install_path}` | 安装目录 |
| `{resolved_install_path}\SOLIDWORKS` | SW 程序目录 |
| `%TEMP%` | 系统临时目录 |
| `%LOCALAPPDATA%\Temp` | 用户临时目录 |
| `%LOCALAPPDATA%\SOLIDWORKS` | SolidWorks 日志目录 |

**匹配方式**

- 递归 glob `*.log` / `*.txt` / `*.html` / `*.htm`
- 只检查大小 < 32 MiB 的文件，按路径排序取最近 12 个
- 只读取**文件尾部 256 KiB**（关键结论都在末尾）
- 编码：UTF-8 → GBK(936) → 系统 ANSI 三级回退解码

**成功标记**

```
安装成功
Installation succeeded / installation succeeded
Setup completed successfully
Installation completed successfully
成功完成安装
```

#### 进程检查（`process_check_enabled`）

检查以下映像名是否全部退出：

| 映像名 |
|---|
| `StartSWInstall.exe` |
| `setup.exe` |
| `msiexec.exe` |
| `sldworkssetup.exe` |
| `swinstallmanager` |

使用 `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS)` 枚举，**无进程存活即为满足**。

#### 文件检查（`file_check_enabled`）

检查 `<install_path>\SOLIDWORKS\SLDWORKS.exe`（按候选列表逐个试）
是否存在（`is_file()`）。

### 判定矩阵

| 日志 | 进程 | 文件 | 满足数 | 结论 |
|---|---|---|---|---|
| ✓ | ✓ | ✗ | 2 | **完成** |
| ✓ | ✗ | ✓ | 2 | **完成** |
| ✗ | ✓ | ✓ | 2 | **完成** |
| ✓ | ✗ | ✗ | 1 | 继续等待 |
| ✗ | ✓ | ✗ | 1 | 继续等待 |
| ✗ | ✗ | ✓ | 1 | 继续等待 |
| ✗ | ✗ | ✗ | 0 | 继续等待 |
| ✓ | ✓ | ✓ | 3 | **完成** |

> 「文件 ✓ + 进程 ✗」这一组合需要留意：文件已生成但安装进程仍在运行，
> 通常意味着安装器正在做收尾（注册组件、写配置）。判定为完成是安全的，
> 后续第 10 步会终止残留进程。

### 为什么默认要「两项」

单项判定太脆弱：

- 只看日志 → 日志可能写在别处、可能被清理、可能根本没有
- 只看进程 → 安装器可能在启动后立即失败退出（0 秒就"退出"了）
- 只看文件 → `SLDWORKS.exe` 可能是上一次安装残留的旧文件

两项组合能有效排除以上误判。

### 超时后的错误信息

```
轮询超时（240 分钟）：日志=false 进程退出=false 可执行文件=true
```

三项状态全部列出，让用户能直接判断问题方向：

| 观测到的组合 | 最可能的原因 |
|---|---|
| 三项全 false | 安装器没启动起来（权限、介质、`StartSWInstall` 参数） |
| 文件 true，其余 false | 安装卡在某个无人处理的弹窗上 |
| 进程 true，文件 false | 安装器异常退出，需看安装日志 |
| 日志 true，其余 false | 日志是旧文件；安装实际失败 |

---

## 错误处理与恢复策略

### 三类退出路径

| 路径 | 触发 | 恢复机制 |
|---|---|---|
| **正常完成** | 13 步全部成功 | 第 13 步主动恢复网络、卸载镜像、清理临时目录 |
| **步骤失败** | 某步返回 `Err` | `catch_unwind` 后的收尾代码 + 三个 `Drop` 守卫 |
| **Panic** | 内部 panic | `catch_unwind` 捕获 → 守卫 `Drop` → 推送 `Error(internal)` |

三个守卫分别是：

| 守卫 | `Drop` 行为 | 受哪个配置控制 |
|---|---|---|
| `NetworkGuard` | 重新启用所有被禁用的适配器 | `network.restore_network_after` |
| `IsoMountGuard` | `Dismount-DiskImage` | 无（始终执行） |
| `TempDirGuard` | 递归删除工作目录 | `workdir.cleanup_temp_after` |

### 事件流

**错误事件**

```json
{
  "type": "error",
  "data": {
    "kind": "environment | download | extract | install | post_install | cancelled | internal",
    "message": "<人类可读的失败原因>",
    "recoverable": true
  }
}
```

`recoverable = true` 表示用户修正后可以重试（如中文计算机名、缺管理员权限）；
`false` 表示需要人工介入（如解压失败、服务未启动）。

**结束事件**

```json
{ "type": "completed", "data": { "success": false, "message": "部署失败" } }
```

无论成功失败，`Completed` 事件总会推送一次，
`InstallStatus.finished` 与 `finished_at` 会同步更新。

### 取消语义

`cancel_install` 命令置位 `Arc<AtomicBool>`。检查点覆盖：

| 阶段 | 检查频率 |
|---|---|
| 下载 | 每个分块 + 进度采样 + 补齐轮次 |
| 子进程（7z / reg / bat / powershell） | 每 120 ms |
| 解压 | 子进程检查点 |
| 轮询检测 | 每次循环 |
| 进程终止 | 等待期间每 250 ms |
| 文件复制 | 每个目录项 |
| 服务轮询 | 每次循环 |

取消后：

| 阶段 | 保留的现场 |
|---|---|
| 下载中 | `.part` + `.state`（可续传） |
| 解压中 | 已解出的部分文件 |
| 轮询中 | 安装继续进行（程序停止等待，但不干预安装器） |

### 状态快照

`InstallStatusCell` 保存完整运行状态，`get_install_status` 命令可随时读取：

```json
{
  "running": true,
  "cancelled": false,
  "finished": false,
  "success": false,
  "current_step": 7,
  "total_steps": 9,
  "step_id": "install",
  "percent": 68.4,
  "message": "静默安装",
  "started_at": "2026-10-04 11:42:07.512",
  "finished_at": null,
  "popups_handled": 2,
  "network_disabled": true,
  "iso_mounted": true,
  "completed_steps": [0, 1, 2, 3, 4, 5],
  "eta_seconds": 1800
}
```

界面重新挂载（例如切换页面回来）时用该快照恢复 UI，
不需要重放 Channel 事件。

### 排查失败的建议顺序

1. **看日志页的 Error 行** —— 错误信息包含步骤名与具体原因
2. **把 `workdir.cleanup_temp_after` 改为 `false`** —— 保留解压产物与安装日志
3. **确认 `install.polling.timeout_minutes` 足够大** —— 慢速磁盘可能需要 6 小时
4. **检查弹窗记录表** —— 是否有该处理但被禁用策略跳过的弹窗
5. **确认权限与计算机名** —— 见 [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md)

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CONFIG_GUIDE.md`](./CONFIG_GUIDE.md) | **我需要填什么** —— 每个键是否必填、填什么、最小可用示例、自检清单 |
| [`COMMAND_LINE.md`](./COMMAND_LINE.md) | 官方命令行属性参考：属性名、语言代码表、ADDLOCAL 层级、介质路径 |
| [`CONFIG.md`](./CONFIG.md) | 每个配置项的类型 / 默认值 / 取值范围 / 内部语义 |
| [`CUSTOMIZE.md`](./CUSTOMIZE.md) | 部署前必须改什么、已知局限、完整自检清单 |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | 按症状排查 |
| [`DEVELOPMENT.md`](./DEVELOPMENT.md) | 二次开发与构建 |
