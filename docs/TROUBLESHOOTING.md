# 常见问题排查

按「症状」组织。先看[快速诊断表](#快速诊断表)，再跳到对应章节。

---

## 目录

- [快速诊断表](#快速诊断表)
- [日志查看方法](#日志查看方法)
- [中文用户名 / 计算机名](#中文用户名--计算机名)
- [权限不足](#权限不足)
- [网络未禁用 / 未恢复](#网络未禁用--未恢复)
- [下载失败](#下载失败)
- [解压失败](#解压失败)
- [下载后的文件后缀不是压缩包后缀](#下载后的文件后缀不是压缩包后缀)
- [安装卡住 / 轮询超时](#安装卡住--轮询超时)
- [弹窗未被处理](#弹窗未被处理)
- [文件替换失败](#文件替换失败)
- [许可服务启动失败](#许可服务启动失败)
- [构建与环境问题](#构建与环境问题)
- [界面问题](#界面问题)

---

## 快速诊断表

| 症状 | 最可能的原因 | 跳到 |
|---|---|---|
| 第 1 步就失败，提示计算机名/用户名 | 含中文或特殊符号 | [中文用户名 / 计算机名](#中文用户名--计算机名) |
| 第 1 步失败，提示「不是以管理员身份运行」 | 未提权 | [权限不足](#权限不足) |
| 第 4 步失败，提示找不到 7z | 未提供 `7z.exe` | [解压失败](#解压失败) |
| 下载进度长时间不动 | 服务端不支持 `Range` / 超时过小 | [下载失败](#下载失败) |
| 下载报 SHA-256 校验失败 | 镜像文件损坏或被篡改 | [下载失败](#下载失败) |
| 下载下来的文件后缀不是压缩包 | **不影响解压**（7z 按文件头识别） | [后缀不是压缩包后缀](#下载后的文件后缀不是压缩包后缀) |
| 提示找不到 `*solidsquad*` / `_SolidSQUAD_` / `.reg` / `.iso` | 名字大小写与预期不一致 | [解压失败](#解压失败) |
| 第 9 步超时，三项状态全 false | 安装器没启动（权限/介质/参数） | [安装卡住](#安装卡住--轮询超时) |
| 第 9 步超时，仅「文件 true」 | 卡在无人处理的弹窗 | [弹窗未被处理](#弹窗未被处理) |
| 第 11 步大量「文件被占用」 | SolidWorks 进程未关闭 | [文件替换失败](#文件替换失败) |
| 第 12 步服务未进入 RUNNING | 计算机名非 ASCII / 端口 25734 被占用 | [许可服务启动失败](#许可服务启动失败) |
| 安装结束后网卡仍是禁用状态 | 手工设置了 `restore_network_after = false` | [网络未禁用 / 未恢复](#网络未禁用--未恢复) |
| `npm run build` 报 esbuild 相关错误 | postinstall 未执行 | [构建与环境问题](#构建与环境问题) |
| `cargo check` 报 ring / cl.exe 错误 | 引入了需要 C 编译器的依赖 | [构建与环境问题](#构建与环境问题) |

---

## 日志查看方法

### 方法一：界面内查看（首选）

**日志页**（侧栏 `03 日志`）提供：

| 功能 | 用法 |
|---|---|
| 级别过滤 | 顶部按钮组：全部 / 信息 / 成功 / 警告 / 错误 / 调试，按钮上的数字是各等级条目数 |
| 全文搜索 | 右上搜索框，匹配消息内容（不区分大小写） |
| 自动滚动 | 开关；手动向上翻阅时会自动关闭，回到底部时恢复 |
| 导出 | 「导出 N 条」按钮，按钮上写明将导出多少条（即当前过滤结果） |
| 清空 | 清空本次会话的缓冲（不影响已导出的文件） |
| 弹窗记录 | 页面下方表格，列出每个已处理弹窗的时间、标题、执行动作 |

**主页**的日志区显示最近 20 条，适合边跑边看。

### 方法二：导出文件

点「导出」后文件写入：

```
%APPDATA%\com.solidworks.autoinstaller\logs\solidworks-install-YYYYMMDD-HHMMSS.log
```

实际路径以界面显示为准（应用标识符见 `src-tauri/tauri.conf.json`
的 `identifier`，导出成功后的提示条里会给出完整路径）。列出已有导出文件：

```powershell
Get-ChildItem "$env:APPDATA\com.solidworks.autoinstaller\logs" | Sort-Object LastWriteTime -Descending
```

用记事本或 `Get-Content` 打开：

```powershell
Get-ChildItem "$env:APPDATA\com.solidworks.autoinstaller\logs" | Sort-Object LastWriteTime -Descending | Select-Object -First 1 | Get-Content -Tail 80
```

导出文件包含上下文表头（导出时间、条目数、当前步骤、进度、弹窗数），
便于附在问题报告里。

### 方法三：保留现场

失败排查**最重要的一步**：把 `workdir.cleanup_temp_after` 改为 `false`，
重新运行。这样工作目录不会被删除，可以手动检查：

```toml
[workdir]
cleanup_temp_after = false
temp_dir = "E:\\SWTemp"    # 指到空间充足的盘
```

工作目录内容：

| 路径 | 内容 |
|---|---|
| `{workdir}\folder1\` | 主压缩包解压结果 |
| `{workdir}\folder1\_SolidSQUAD_\` | 补丁目录（含 `.reg` 与 `server_install.bat`） |
| `{workdir}\folder1\*.iso` | 安装镜像 |

然后检查：

```powershell
$wd = "E:\SWTemp\solidworks-install"
Get-ChildItem "$wd\folder1" | Select-Object Name, Length
Get-ChildItem "$wd\folder1" -Filter "*.iso"
Get-ChildItem "$wd\folder1" -Recurse -Filter "*.reg" | Select-Object -First 10 FullName
Test-Path "$wd\folder1\_SolidSQUAD_\server_install.bat"
```

### 方法四：实时监控关键位置

安装过程中另开一个 PowerShell 窗口：

```powershell
# 每 5 秒列出仍在运行的安装相关进程
while ($true) {
  $p = Get-Process -ErrorAction SilentlyContinue |
       Where-Object { $_.ProcessName -match 'setup|sldworks|swinstall|msiexec' }
  "{0}  {1}" -f (Get-Date -Format 'HH:mm:ss'), (($p.ProcessName -join ', ') + " ($($p.Count))")
  Start-Sleep 5
}
```

```powershell
# 盯着服务状态
while ($true) {
  $s = sc.exe query "SolidWorks Flexnet Server" 2>&1 | Select-String 'STATE'
  "{0}  {1}" -f (Get-Date -Format 'HH:mm:ss'), $s
  Start-Sleep 5
}
```

```powershell
# 盯着目标文件是否生成
while ($true) {
  $exe = "C:\SW\SOLIDWORKS\SLDWORKS.exe"   # 注意是 SLDWORKS，不是 SOLIDWORKS
  "{0}  exists={1}" -f (Get-Date -Format 'HH:mm:ss'), (Test-Path $exe)
  Start-Sleep 5
}
```

---

## 中文用户名 / 计算机名

### 症状

第 1 步（环境检测）立即失败，错误信息形如：

```
计算机名「我的电脑」包含非 ASCII 字符。SolidWorks FlexNet 许可服务无法在该名称下启动，
请改为纯英文并**重启**后再运行本程序。
```

或

```
用户名「张三」包含非 ASCII 字符。请改用纯英文用户名，或新建一个英文账户后再安装。
```

### 原因

SolidWorks 的 FlexNet 许可服务在注册与启动时无法正确处理非 ASCII 主机名。
这不是本程序的限制，而是许可服务的固有行为。用户名同理会影响
安装路径与临时目录中的非 ASCII 字符处理。

### 解决方案

#### 计算机名（必须修改）

```powershell
# 1. 查看当前名称
hostname
GetComputerNameEx   # 或在 PowerShell 中：$env:COMPUTERNAME

# 2. 修改（需管理员权限，15 字符以内，只能用 A-Z a-z 0-9 与连字符）
Rename-Computer -NewName "DEVWS01" -Force

# 3. 重启（必须）
Restart-Computer
```

或者用图形界面：**设置 → 系统 → 关于 → 重命名这台电脑**。

> **必须重启**。名称修改后 FlexNet 仍会读到旧名称，直到重启才生效。

#### 用户名（推荐新建账户）

改用户名比改计算机名麻烦，推荐新建一个纯英文的本地管理员账户：

```powershell
# 创建英文账户
net user swinstall P@ssw0rd /add
net localgroup Administrators swinstall /add

# 注销当前用户，用 swinstall 登录后运行本程序
```

> 直接重命名已有账户（`net user <旧名> /add` 之类）会改变用户配置文件目录
> 的归属，可能导致其他已装软件异常，不建议。

#### 如果名称已经正确却仍报错

检查是否用了**同形异义字符**（例如全角字母、带变音符号的拉丁字母）：

```powershell
# 逐个字符检查码点
"$env:COMPUTERNAME".ToCharArray() | ForEach-Object { "{0} U+{1:X4}" -f $_, [int]$_ }
```

正常应当只出现 `U+0041`–`U+005A`、`U+0061`–`U+007A`、`U+0030`–`U+0039`、`U+002D`。

---

## 权限不足

### 症状

- 第 1 步报「当前不是以管理员身份运行」
- `reg import` 报「拒绝访问」
- 网卡禁用无效果
- `sc create` / `sc start` 报「拒绝访问」

### 原因

本程序需要管理员权限的功能：

| 功能 | 需要的权限 |
|---|---|
| `reg import` 写 `HKLM` | 管理员 |
| `netsh interface set` / `Set-NetAdapter` | 管理员 |
| `Mount-DiskImage` / `Dismount-DiskImage` | 管理员（Windows 10 起通常允许普通用户，但不保证） |
| `sc create` / `sc delete` / `sc start` | 管理员 |
| 写入 `C:\Program Files\*` 或 `C:\SW` | 管理员 |
| `taskkill` 终止其他用户的进程 | 管理员 |

### 解决方案

**方式一：让程序自己提权**

第 1 步检测到非管理员时会尝试 `ShellExecuteW("runas")`，
弹出 UAC 对话框。点击「是」即可，程序会以管理员身份重新启动。

> 若 UAC 对话框未出现，可能是被组策略禁用或被安全软件拦截。

**方式二：手动以管理员身份运行**

```powershell
# 在已提权的 PowerShell 中启动
Start-Process powershell -Verb RunAs -ArgumentList "-NoExit","-Command","cd 'C:\path\to\project'; npm run tauri:dev"
```

或右键 `solidworks-installer.exe` → **以管理员身份运行**。

**方式三：永久设置兼容性标志**

右键 exe → 属性 → 兼容性 → 勾选「以管理员身份运行此程序」。
**不推荐**：会让每次启动都弹 UAC，且难以撤销。

### 验证权限

```powershell
# 是否已提权
([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
```

返回 `True` 表示已提权。

### 权限正确却仍失败

| 情况 | 检查方法 |
|---|---|
| UAC 被禁用但账户属标准用户 | `whoami /groups \| Select-String 'S-1-16'` 查看完整性级别 |
| 注册表项被组策略保护 | `gpedit.msc` → 计算机配置 → 管理模板 → 系统 → 组策略 |
| 安全软件拦截注册表写入 | 查看安全软件日志，临时加入白名单 |
| 域账户被限制本地管理员 | 联系域管理员 |

---

## 网络未禁用 / 未恢复

### 症状 A：安装期间网络仍可用

**检查配置**

```toml
[network]
disable_network = true      # ← 必须为 true
adapter_disable_method = "netsh"
```

**检查适配器是否被识别为虚拟**

程序刻意跳过含以下关键字的适配器（`virtual` / `vmware` / `hyper-v` /
`vethernet` / `loopback` / `tap-` / `wintun` / `wireguard` / `openvpn` /
`zerotier` / `tailscale` / `npcap` / `bluetooth` / `wi-fi direct` /
`docker` / `wsl`）以及回环类型（`IF_TYPE_SOFTWARE_LOOPBACK = 24`）。

查看程序识别到的适配器：

```powershell
Get-NetAdapter | Select-Object Name, InterfaceDescription, ifType, Status
```

`ifType` 为 `24` 表示回环；`InterfaceDescription` 中含有上述关键字会被跳过。

**可能是名称匹配问题**：日志中会列出每个适配器的处理结果，形如：

```
已禁用网卡 以太网
禁用网卡 Wi-Fi 失败: netsh 返回失败（code=Some(1)）: ...
```

如果日志显示适配器已被禁用但网络仍可用，说明**存在未识别的物理适配器**
（例如 USB 网卡或某些虚拟化带出的网卡）。

### 症状 B：安装结束后网卡仍是禁用状态

**根本原因**：手动把 `restore_network_after` 设为了 `false`：

```toml
[network]
restore_network_after = false    # ← 程序不会自动恢复
```

**立即恢复（无需重启）**

```powershell
# 方式一：重新启用所有已禁用的适配器
Get-NetAdapter | Where-Object { $_.Status -eq 'Disabled' } | Enable-NetAdapter -Confirm:$false

# 方式二：按名称启用
netsh interface set interface name="以太网" admin=enable
netsh interface set interface name="Wi-Fi" admin=enable

# 方式三：图形界面
ncpa.cpl       # 右键对应适配器 → 启用
```

**如果 `netsh` 也失败**：

```powershell
# 检查适配器状态
Get-NetAdapter | Format-Table Name, Status, AdminStatus, MediaConnectionState
```

若 `Status` 为 `Disabled` 但 `Enable-NetAdapter` 报错，
尝试重启网络相关服务：

```powershell
Restart-Service -Name netprofm, NlaSvc, Dhcp -Force
```

或用 `devcon`/设备管理器重新启用网卡设备。

### 为什么程序还会「漏掉」恢复

程序的恢复由两处保证：

| 机制 | 覆盖场景 |
|---|---|
| 第 13 步 `restore_now()` | 正常走到收尾 |
| `NetworkGuard::drop()` | 任何 `Err` 返回、panic 展开 |

**唯一不覆盖的场景**：进程被强杀（任务管理器结束进程、`taskkill /F`、
系统断电）。此时 `Drop` 不会执行。

> 因此建议 `restore_network_after = true`（默认值），
> 只在确知会手动接管网络时才改为 `false`。

---

## 下载失败

### 症状分类

| 错误信息片段 | 含义 |
|---|---|
| `网络错误: ...` | TCP/TLS 层失败（DNS、连接被拒、证书、超时） |
| `服务器拒绝请求: 探测请求返回 404` | URL 错误或文件已下架 |
| `SHA-256 校验失败：期望 ...，实际 ...` | 下载内容与预期不符 |
| `服务端忽略了 Range 请求` | 服务端不支持分块下载 |
| `分块 N 字节数 X 与期望 Y 不符` | 连接中途断开 |
| `下载已完成但 .part 大小不符` | 写入过程出错（磁盘满、权限） |
| 进度长时间不动 | 见下方「进度卡住」 |

### 检查 URL 与可访问性

```powershell
$url = "https://example.com/solidworks2024sp5.7z"

# 1. 是否能连通
Test-NetConnection (([uri]$url).Host) -Port 443

# 2. HEAD 请求，看长度与 Accept-Ranges
$r = Invoke-WebRequest -Uri $url -Method Head -UseBasicParsing
$r.StatusCode
$r.Headers['Content-Length']
$r.Headers['Accept-Ranges']

# 3. 是否支持 Range（关键）
$r2 = Invoke-WebRequest -Uri $url -Headers @{ Range = 'bytes=0-0' } -UseBasicParsing
$r2.StatusCode              # 206 = 支持；200 = 不支持
$r2.Headers['Content-Range']  # bytes 0-0/总长度
```

| 观察 | 结论 |
|---|---|
| `Accept-Ranges` 缺失且 `Range` 请求返回 200 | 服务端不支持 Range，**无法多线程与续传**，程序会自动降级为单连接 |
| `Content-Range` 的总长度与实际文件大小不符 | 镜像站返回的是错误页面（HTML），而不是压缩包 |

### 进度卡住不动

**原因一：`read_timeout_minutes` 过小**

大分块（最大 8 MiB）在慢速链路上会被整体超时掐断，然后重试，看起来像是卡住。

```toml
[download]
read_timeout_minutes = 10       # 5 太小时调大
connect_timeout_minutes = 1
```

**原因二：`threads` 过高，服务端限流**

服务端对同一 IP 的并发连接有限制时，过多 worker 会让所有连接都变慢。

```toml
[download]
threads = 8                     # 从 16 降到 8 试试
```

**原因三：磁盘写入成为瓶颈**

工作目录在慢速磁盘（机械盘、网络盘）上时，写线程会成为瓶颈。
把 `[workdir].temp_dir` 指到 SSD。

**原因四：杀毒软件实时扫描**

下载目录加入杀毒软件白名单可显著提速：

```powershell
# 把缓存目录加入 Defender 排除项（需管理员）
Add-MpPreference -ExclusionPath "$env:APPDATA\com.solidworks.autoinstaller\cache"
```

### SHA-256 校验失败

校验失败时程序会**主动删除 `.part` 与 `.state`**，所以下次会从头下载。

**排查顺序**

1. **确认期望值正确**

```powershell
# 本地计算实际值
Get-FileHash "D:\path\to\solidworks2024sp5.7z" -Algorithm SHA256 | Select-Object -ExpandProperty Hash
```

2. **确认镜像文件本身完整**

对比镜像站公布的哈希值。若镜像站未公布，从另一个来源重新获取。

3. **确认配置里没有多余空格**

```toml
# ✅ 程序会自动去掉空白并转小写
checksum_sha256 = "9f2c8b1e..."
```

4. **临时跳过校验以确认其余流程**

```toml
checksum_sha256 = ""
```

> 留空后，若缓存文件已存在会**直接复用**，不再下载。
> 想强制重新下载请手动删除 `%APPDATA%\com.solidworks.autoinstaller\cache\` 下的文件。

### 断点续传不生效

**前置条件**：服务端支持 `Range`。不支持时程序会明确记录：

```
服务端不支持 Range，丢弃已有分片重新开始
```

**检查状态文件**

```powershell
$cache = "$env:APPDATA\com.solidworks.autoinstaller\cache"
Get-ChildItem $cache | Select-Object Name, Length

# .state 是位图：SWDL1 前缀 + 每块一字节（1 完成 / 0 未完成）
$state = Get-ChildItem $cache -Filter '*.state' | Select-Object -First 1
if ($state) {
  $text = Get-Content $state.FullName -Raw
  $bits = $text.Substring(6)
  "已完成 {0} / {1} 分块" -f ($bits.ToCharArray() | Where-Object { $_ -eq '1' }).Count, $bits.Length
}
```

**若续传后仍有分块失败**：程序会自动进入「补齐轮次」串行重试。
如果补齐也失败，说明网络确实不稳定，建议降低 `threads` 并提高 `retry_count`。

### 代理环境

程序使用 reqwest，会读取系统代理设置。若需显式指定：

```powershell
$env:HTTP_PROXY = "http://proxy.corp.local:8080"
$env:HTTPS_PROXY = "http://proxy.corp.local:8080"
$env:NO_PROXY = "localhost,127.0.0.1,.corp.local"
npm run tauri:dev
```

> **企业自签证书**：程序使用 `native-tls`（Windows SChannel），
> **跟随系统证书存储**。把企业 CA 装进「受信任的根证书颁发机构」即可，
> 无需修改代码。

---

## 解压失败

### 先确认：扩展名根本不重要

程序**不依赖扩展名**来解压。7z 是按文件头（magic bytes）识别格式的，
所以下面这些都能正常解压：

| 文件 | 结果 |
|---|---|
| `solidworks2024sp5.7z` | ✅ |
| `download.php`（实际是 7z） | ✅ |
| `archive`（无扩展名） | ✅ |
| `PACKAGE.7Z`（大写扩展名） | ✅ |
| `patch.RAR`（实际是 7z） | ✅ 按实际格式解压 |
| `file.bin` | ✅ 只要能识别出格式 |

**唯一会拒绝的情况**：在本地模式下，程序会先用 `7z t` 实测一次。
只有当 7z **明确报错**（不是超时）时才会拒绝，并把你看到的 7z 原文一并显示。
超时一律放行，交由第 4 步解压时判定。

如果你看到的错误是「所选文件无法作为压缩包读取」，说明该文件确实不是压缩包
（或者已损坏）——用下一节的 `7z t` 手动确认。

### 症状

```
使用解压程序 ... 失败
7z 解压失败（code=Some(2)）
```

或

```
运行 7z 失败: 系统找不到指定的文件。(os error 2)
```

### 原因与解决

**原因一：未提供 `7z.exe`**

程序的四级查找顺序：

1. `[sevenzip].exe_path`
2. Tauri 资源目录（`resources\7z.exe`）
3. 源码目录 `src-tauri\resources\7z.exe`（开发期）
4. 可执行文件同级目录，最后回退到 `PATH` 中的 `7z`

**解决**：

```powershell
# 安装 7-Zip
winget install 7zip.7zip

# 复制到资源目录
Copy-Item "C:\Program Files\7-Zip\7z.exe" "src-tauri\resources\7z.exe"
```

或显式指定路径：

```toml
[sevenzip]
exe_path = "C:\\Program Files\\7-Zip\\7z.exe"
```

**原因二：`extract_timeout_minutes` 过小**

大型镜像解压可能超过 30 分钟，尤其在工作目录位于机械盘时：

```toml
[sevenzip]
extract_timeout_minutes = 60
```

判断方法：日志里会出现

```
解压超时（超过 30 分钟）: E:\SWTemp\...\solidworks2024sp5.7z
```

**原因三：磁盘空间不足**

解压 ISO 与 `_SolidSQUAD_` 需要 **≥ 60 GB**。检查：

```powershell
Get-PSDrive -PSProvider FileSystem | Select-Object Name, @{n='FreeGB';e={[math]::Round($_.Free/1GB,1)}}
```

**原因四：压缩包损坏**

手动验证：

```powershell
& "C:\Program Files\7-Zip\7z.exe" t "D:\path\to\solidworks2024sp5.7z"
```

`t` 是测试模式，会校验所有文件 CRC。

**原因五：杀毒软件锁定了文件**

解压过程中若杀毒软件正在扫描刚释放的 exe/dll，7z 会报「拒绝访问」。
把工作目录加入排除项。

**原因六：未找到含 `solidsquad` 的压缩包（第 5 步）**

```
在 E:\SWTemp\...\folder1 中未找到文件名含 "solidsquad" 的压缩包，无法完成第二阶段解压
```

说明主压缩包结构不符合预期。匹配规则是「文件名含 `solidsquad`，扩展名不限，
不区分大小写」，所以 `SolidSQUAD_Patch.RAR`、`solidsquad.7z`、`_SolidSquad_.zip`
都能命中。用「保留现场」方式检查 `folder1` 里到底有什么：

```powershell
Get-ChildItem "E:\SWTemp\solidworks-install\folder1" -Recurse -File |
  Where-Object { $_.Name -match 'solid|squad' } |
  Select-Object FullName, Length
```

若补丁包的名字里**完全没有** `solidsquad` 字样（例如改名为 `patch.7z`），
需要手动把它解压到 `folder1` 得到 `_SolidSQUAD_` 目录，再重跑程序。

**原因七：文件名大小写不一致**

程序在**所有**查找环节都做了不区分大小写比较，因此下面这些都能正确命中：

| 实际文件名 | 程序期望 | 是否命中 |
|---|---|---|
| `_SolidSquad_` | `_SolidSQUAD_` | ✅ |
| `SOLIDWORKS corp` | `SolidWorks Corp` | ✅ |
| `license.REG` | `*.reg` | ✅ |
| `sw2024.ISO` | `*.iso` | ✅ |
| `server_install.BAT` | `server_install.bat` | ✅ |

如果**确实**出现"文件明明在，程序却说找不到"，先手工确认路径：

```powershell
$wd = "E:\SWTemp\solidworks-install\folder1"
# 列出所有目录，人工核对名字
Get-ChildItem $wd -Recurse -Directory | Select-Object -ExpandProperty FullName
# 列出 .reg / .iso（不区分大小写）
Get-ChildItem $wd -Recurse -File -Include *.reg, *.iso, *.7z, *.rar
```

**已知边界**：目录遍历深度上限 24 层。若补丁包被埋在更深的层级里，
程序会找不到——把内容往上挪一层即可。

---

## 下载后的文件后缀不是压缩包后缀

**结论：不影响解压。** 详见「[解压失败](#解压失败)」开头的说明——7z 按文件头
识别格式，扩展名对它没有意义。

程序已经为这种情况做了三件事：

### 1. 解压命令不指定格式

`7z x` 刻意**不传 `-t<格式>`**。如果按扩展名强行指定，
"名字像 zip、实际是 7z"的文件反而会解压失败。

### 2. 缓存文件会补上真实后缀

下载完成后按文件头识别格式并重命名缓存文件：

```
按文件头识别为 rar 格式（URL 末段 download.php）
缓存文件已重命名为 C:\...\cache\download.php.rar
```

| URL 末段 | 嗅探结果 | 缓存文件名 |
|---|---|---|
| `solidworks2024sp5.7z` | `7z` | `solidworks2024sp5.7z` |
| `download.php` | `zip` | `download.php.zip` |
| `archive`（无扩展名） | `rar` | `archive.rar` |
| `/`（无文件名） | `iso` | `solidworks2024sp5.iso` |

重命名失败只记 `Warn`，**不影响解压**。

### 3. 复用查找接受旧名字

如果你改过 URL 或者换过版本，程序会同时接受「精确名」与
「同基名 + 任意已知后缀」（`download.php` 与 `download.php.zip` 视为同一份），
不会因为文件名变化就白下几十 GB。断点续传的 `.part` / `.state`
永远不会被误认为完整文件。

### 相关日志

正常时你会看到这些行（可用日志页的「全部」或「调试」级别过滤）：

```
目标 12458987520 字节 · 分块 16 个（每块 8388608 字节）· 线程上限 16
下载完成 11.60 GB，耗时 8 分 12 秒，SHA-256 校验通过
按文件头识别为 7z 格式（URL 末段 solidworks2024sp5.7z）
第 1 阶段解压 C:\...\cache\solidworks2024sp5.7z → C:\...\work\folder1
7z 将按文件头以 7z 格式解压（扩展名不参与判定）
```

若出现：

```
文件头未匹配已知压缩格式；7z 将自行判定，若无法识别会在第 4 步明确报错
```

说明文件头不是已知的压缩格式签名。这不一定是问题——少数自解压包或
带外壳的安装包可能如此。若第 4 步随后报错，用下面的命令人工确认：

```powershell
& "C:\Program Files\7-Zip\7z.exe" l "C:\...\cache\你的文件"
```

`l` 是列表模式：能列出内容说明 7z 能识别，不能则说明该文件确实不是压缩包。

---

## 安装卡住 / 轮询超时

### 症状

第 9 步（轮询检测）长时间无进展，最后报：

```
轮询超时（240 分钟）：日志=false 进程退出=true 可执行文件=false
```

### 按三项状态定位

| 日志 | 进程 | 文件 | 诊断 | 措施 |
|---|---|---|---|---|
| ✗ | ✗ | ✗ | **安装器根本没启动** | 见下方「A. 安装器未启动」 |
| ✗ | ✗ | ✓ | 安装完成但进程未退出（或残留旧文件） | 见下方「D. 残留进程」 |
| ✗ | ✓ | ✗ | **安装器启动后立即失败** | 见下方「B. 安装器早期失败」 |
| ✓ | ✗ | ✗ | 日志是**旧文件** | 手动清理目标目录后重装 |
| ✗ | ✓ | ✓ | 判定为完成（2 项） | 不需要处理 |
| ✓ | ✗ | ✓ | 判定为完成（2 项） | 不需要处理 |

> 「进程退出 = true」在有意义的结果里往往**不代表成功** ——
> 安装器可能启动后 0.1 秒就崩了。所以必须配合日志或文件检查。

### A. 安装器未启动（三项全 false）

**检查 1：`startswinstall.exe` 是否存在**

```powershell
# 先让程序保留现场，然后：
Get-ChildItem "E:\SWTemp\solidworks-install\folder1" -Filter "*.iso"
# 手动挂载镜像后检查
Mount-DiskImage -ImagePath "E:\SWTemp\solidworks-install\folder1\xxx.iso" -PassThru |
  Get-Volume | Select-Object DriveLetter
```

镜像根目录应包含 `sldim\startswinstall.exe`（命令行入口）
与 `swwi\data\solidworks.msi`（主程序包）。

> 根目录**只有** `setup.exe` 是正常的 —— 那是 GUI 引导程序。
> `startswinstall.exe` 在 `sldim\` 子目录里，早期版本才放在根目录。

**检查 2：是否缺少管理员权限**

`startswinstall.exe` 需要管理员权限，静默失败时不会有任何提示。
参考[权限不足](#权限不足)。

**检查 3：静默参数是否被接受**

手动试运行（**仅在测试机上**）：

```powershell
# 挂载镜像后
cd "<盘符>:\"
.\sldim\startswinstall.exe /install /now
```

观察是否有对话框或立即退出。

**检查 4：日志中是否有启动记录**

成功启动时日志会打印：

```
使用 <介质>\sldim\startswinstall.exe /install /now
静默安装器已启动（pid 12345）
```

没有这两行说明走的是 `msiexec` 回退路径；若回退也失败，日志会显示：

```
既没有 startswinstall.exe 也没有 solidworks.msi。
已查找：<root>\sldim\startswinstall.exe、<root>\startswinstall.exe、
        <root>\swwi\data\solidworks.msi。
```

### B. 安装器早期失败（进程退出 true、文件 false）

**看安装器自己的日志**。SolidWorks 安装日志常见位置：

```powershell
# 系统临时目录
Get-ChildItem $env:TEMP -Filter "*.log" | Sort-Object LastWriteTime -Descending | Select-Object -First 10 Name, LastWriteTime

# SolidWorks 自己的日志目录
Get-ChildItem "$env:LOCALAPPDATA\SOLIDWORKS" -Recurse -Filter "*.log" -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 10 FullName
```

**常见原因**

| 原因 | 特征 |
|---|---|
| 磁盘空间不足 | 日志中出现 `Not enough disk space` |
| `INSTALLDIR` 路径非法 | 路径含中文、特殊字符，或指向不存在的盘 |
| 组件 ID 错误 | `components_whitelist` 中的 ID 不存在（走 msiexec 时） |
| 已安装同版本 | 日志中出现 `already installed` |
| 缺少 .NET / VC++ 运行库 | 安装器前置检查失败 |

**解决「已安装同版本」**：先卸载旧版本，或在测试机上使用全新系统快照。

### C. 卡在某个界面（文件 true、日志 false、进程 false）

这通常意味着安装器在等待用户输入 —— 但弹窗守护可能没有匹配到该窗口。

参考[弹窗未被处理](#弹窗未被处理)，
用 Spy++ 或下面的脚本列出所有可见窗口标题：

```powershell
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
}
"@
$titles = New-Object System.Collections.ArrayList
[W]::EnumWindows({ param($h,$l)
  if ([W]::IsWindowVisible($h)) {
    $sb = New-Object System.Text.StringBuilder 512
    [void][W]::GetWindowText($h, $sb, 512)
    if ($sb.Length -gt 0) { [void]$titles.Add($sb.ToString()) }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
$titles
```

如果在列表里看到等待输入的对话框，把其标题关键词反馈到
`win32_utils.rs` 的 `DEFAULT_POPUP_RULES`（需要改代码重新编译）。

### D. 残留进程 / 残留文件导致误判

**清空目标目录后重装**：

```powershell
# 先确认没有 SolidWorks 进程
Get-Process | Where-Object { $_.ProcessName -match 'solidworks|sldworks|sw_dn|flexnet|lmgrd' }

# 清空目标目录
Remove-Item "C:\SW" -Recurse -Force -ErrorAction SilentlyContinue
```

**放宽超时**（慢速磁盘或大型组件集）：

```toml
[install.polling]
poll_interval_minutes = 1
timeout_minutes = 480        # 8 小时
```

**临时放宽判定**（仅用于确认流程能否走完）：

```toml
[install.polling]
log_check_enabled = false
process_check_enabled = false
file_check_enabled = true    # 只看文件生成
```

> 这样判定只依赖一项，容易误判成功。确认完流程记得改回默认。

---

## 弹窗未被处理

### 症状

- 日志的弹窗记录表为空，但安装明显卡住
- 弹出对话框长时间不消失
- 弹窗处理了但点错了按钮

### 内置规则回顾

| 标题关键词 | 动作 | 受控开关 |
|---|---|---|
| `重启` / `重新启动` / `restart` | 点「确定」 | `auto_click_confirm` |
| `不能核实此服务器存在` / `无法核实此服务器存在` / `cannot verify` | 点「是」 | `auto_click_yes` |
| `端口@服务器` / `端口 @ 服务器` / `port@server` | 写入 `25734@localhost` 后点「确定」 | `auto_click_confirm` |

### 检查 1：对应开关是否被关闭

```toml
[install.popup]
scan_interval_ms = 500
auto_click_confirm = true    # ← 关闭后「端口@服务器」不会被处理
auto_click_yes = true        # ← 关闭后「不能核实此服务器存在」不会被处理
auto_click_no = false
popup_timeout_minutes = 5
```

被禁用的动作**既不点击也不产生事件**，因此在记录表里看不到任何痕迹 ——
这容易被误认为守护线程没工作。

### 检查 2：守护线程是否启动

日志中应出现：

```
弹窗守护已启动（扫描间隔 500 毫秒，确定=true 是=true 否=false）
```

没有这行说明第 8 步没有执行到（安装器启动失败）。

### 检查 3：标题是否匹配

守护线程按**子串**匹配标题（大小写不敏感）。若实际弹窗标题是
「SolidWorks 无法核实服务器」，关键词 `不能核实此服务器存在` 不匹配、
但 `cannot verify` 也不匹配 —— 需要用方法 C 的脚本列出真实标题。

### 检查 4：去重窗口是否太长

同一标题在 `popup_timeout_minutes` 内只会处理**一次**。
若同一类弹窗在一分钟内连续出现两次，第二次会被跳过。

调小该值（同时保留足够长以避免与用户手动操作打架）：

```toml
[install.popup]
popup_timeout_minutes = 1
```

### 检查 5：是否需要人工介入的弹窗

某些弹窗（例如「安装程序检测到未满足的先决条件」）**不应**被自动点击 ——
自动确认可能导致安装不完整。此时应：

1. 用方法 C 找到弹窗标题
2. 手动处理该弹窗
3. 在代码中把该标题加入规则表（若确实需要自动化）

### 弹窗点击失败

按钮点击依赖文本匹配（`EnumChildWindows` 找 `Button` 类子窗口）。
若弹窗用的是**自绘按钮**（不是标准 `Button` 类），程序找不到按钮，
日志只会显示「弹窗「X」→ 点击「确定」」但实际没有效果。

**验证方法**：用 Spy++（Visual Studio 附带）查看该按钮的窗口类名。
若不是 `Button`，需要在 `win32_utils.rs` 中扩展匹配逻辑。

---

## 文件替换失败

### 症状

```
复制 C:\SW\SOLIDWORKS\sldworks.exe 仍然失败: 另一个程序正在使用此文件，进程无法访问。
文件替换存在 12 个失败项，请查看日志
```

### 原因一：进程未关闭（最常见）

第 10 步负责关闭进程。检查：

```toml
[process]
kill_sw_processes = true       # ← 若为 false 则整步跳过
kill_pattern = ".*solidwork.*|.*sldworks.*|.*sw_dn.*|.*sidworks.*|.*flexnet.*|.*lmgrd.*"
kill_timeout_minutes = 2
force_kill_after_timeout = true
```

**手动确认并关闭**：

```powershell
Get-Process | Where-Object { $_.ProcessName -match 'solidworks|sldworks|sw_dn|sidworks|flexnet|lmgrd' } |
  Select-Object Id, ProcessName, Path

# 强制关闭
Get-Process | Where-Object { $_.ProcessName -match 'solidworks|sldworks|sw_dn|sidworks|flexnet|lmgrd' } |
  Stop-Process -Force
```

**关闭许可服务**（它会锁定部分 DLL）：

```powershell
Stop-Service "SolidWorks Flexnet Server" -Force -ErrorAction SilentlyContinue
sc.exe stop "SolidWorks Flexnet Server"
```

**检查是否有 explorer.exe 预览锁**：如果文件资源管理器打开了目标目录且
处于「大图标」视图，缩略图预览会锁定 `sldworks.exe`。切到「详细信息」视图或关闭窗口。

### 原因二：只读属性

程序会在首次复制失败后尝试清除目标文件的只读位并重试一次。
若目标文件**在只读介质上**或**ACL 拒绝写入**，重试也会失败：

```powershell
# 递归清除只读属性
Get-ChildItem "C:\SW" -Recurse -File | ForEach-Object { $_.IsReadOnly = $false }

# 检查 ACL
icacls "C:\SW"
```

### 原因三：源目录选错

日志中的源目录应与目标结构对应：

```
文件替换 E:\SWTemp\solidworks-install\folder1\_SolidSQUAD_\SolidWorks Corp 的组件内容 → C:\SW
```

若源显示为 `_SolidSQUAD_` 本身（而不是其下的 `SolidWorks Corp`），
说明程序没找到 `SolidWorks Corp` 子目录，走了兜底逻辑：

```powershell
Get-ChildItem "E:\SWTemp\solidworks-install\folder1\_SolidSQUAD_" -Recurse -Directory |
  Select-Object -First 20 FullName
```

若补丁包结构与预期不同，可手动复制：

```powershell
robocopy "E:\SWTemp\solidworks-install\folder1\_SolidSQUAD_\SolidWorks Corp" "C:\SW" /E /IS /IT
```

`robocopy` 的 `/E` 复制子目录（含空目录），`/IS` 覆盖同名文件，`/IT` 保留属性。

### 原因四：路径过长

Windows 默认路径上限 260 字符。SolidWorks 目录层级较深时可能超限：

```toml
[install]
install_path = "D:\\SW"        # 缩短安装路径
```

或启用长路径支持（需管理员 + 重启）：

```powershell
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" `
  -Name LongPathsEnabled -Value 1 -PropertyType DWORD -Force
```

---

## 许可服务启动失败

### 症状

```
许可服务在 5 分钟内未进入 RUNNING，请检查 server_install.bat 输出与端口 25734
```

### 检查 1：服务是否存在

```powershell
sc.exe query "SolidWorks Flexnet Server"
Get-Service -Name "*Flexnet*" -ErrorAction SilentlyContinue
```

| 输出 | 含义 |
|---|---|
| `指定的服务未安装` | `server_install.bat` 没成功创建服务 |
| `STATE: 1 STOPPED` | 服务存在但没启动 → 尝试手动启动 |
| `STATE: 4 RUNNING` | 已运行（程序可能因 `STOP_PENDING` 误判，见下） |

### 检查 2：手动运行安装脚本

先保留现场（`cleanup_temp_after = false`），然后：

```powershell
$squad = "E:\SWTemp\solidworks-install\folder1\_SolidSQUAD_"
Get-ChildItem $squad -Recurse -Filter "server_*.bat" | Select-Object FullName

# 以管理员身份在脚本目录运行，观察输出
cd (Split-Path $bat)
.\server_remove.bat
.\server_install.bat
```

脚本输出的错误信息通常直接指向原因。

### 检查 3：计算机名非 ASCII（**最高频原因**）

```powershell
hostname
"$env:COMPUTERNAME".ToCharArray() | ForEach-Object { "{0} U+{1:X4}" -f $_, [int]$_ }
```

若含非 ASCII，FlexNet 无法绑定主机名，必须改名 + **重启**。
参考[中文用户名 / 计算机名](#中文用户名--计算机名)。

### 检查 4：端口 25734 被占用

```powershell
# 谁占用了 25734
Get-NetTCPConnection -LocalPort 25734 -ErrorAction SilentlyContinue |
  Select-Object LocalAddress, LocalPort, State, OwningProcess

# 反查进程
$pid25734 = (Get-NetTCPConnection -LocalPort 25734 -ErrorAction SilentlyContinue).OwningProcess
if ($pid25734) { Get-Process -Id $pid25734 | Select-Object Id, ProcessName, Path }
```

**常见占用者**：上一次残留的 `lmgrd.exe` / `SolidWorks Flexnet Server`，
或另一个许可服务（如 ANSYS、MATLAB 的 FlexNet）。

**解决**：

```powershell
# 停掉所有 FlexNet 相关服务与进程
Get-Service | Where-Object { $_.Name -match 'flexnet|lmgrd' } | Stop-Service -Force
Get-Process | Where-Object { $_.ProcessName -match 'lmgrd|flexnet' } | Stop-Process -Force

# 确认端口已释放
Get-NetTCPConnection -LocalPort 25734 -ErrorAction SilentlyContinue
```

若被**其他软件的许可服务**占用，需要在那个软件里改端口，
或卸载冲突软件。

### 检查 5：服务启动后立即停止

```powershell
# 启动并立即查看事件日志
Start-Service "SolidWorks Flexnet Server" -ErrorAction SilentlyContinue
Get-WinEvent -LogName Application -MaxEvents 30 |
  Where-Object { $_.ProviderName -match 'FlexNet|Service Control Manager' } |
  Select-Object TimeCreated, ProviderName, Message | Format-List
```

**常见原因**

| 原因 | 特征 |
|---|---|
| 计算机名非 ASCII | 事件日志中出现编码错误 |
| 许可文件（`.lic`）路径含中文或不存在 | 服务找不到许可文件 |
| 服务账户权限不足 | 事件日志出现 `Access denied` |
| 缺少 VC++ 运行库 | 依赖 DLL 加载失败 |
| 防火墙阻止 | 事件日志出现网络绑定失败 |

### 检查 6：`server_install.bat` 内容不符合预期

```powershell
Get-Content "E:\SWTemp\solidworks-install\folder1\_SolidSQUAD_\server_install.bat"
```

脚本通常执行：复制 `SolidWorks_Flexnet_Server` 目录到目标位置、
`sc create` 注册服务、`sc start` 启动。

若脚本引用了 GUI 工具（如 `server_install.exe`）并等待用户操作，
需要在弹窗守护中添加对应规则，或改为手动执行一次。

### 检查 7：轮询间隔 / 超时过紧

服务首次启动可能需要配置许可文件，耗时较长：

```toml
[flexnet]
server_install_timeout_minutes = 10     # 从 5 增大
server_check_interval_minutes = 1       # 从 0.5 增大，降低 sc 调用频率
```

### 检查 8：手动启动服务验证

```powershell
sc.exe start "SolidWorks Flexnet Server"
sc.exe query "SolidWorks Flexnet Server"

# 查看服务配置（可执行路径、启动类型、账户）
sc.exe qc "SolidWorks Flexnet Server"
```

若 `sc start` 报 1053（服务未及时响应），说明服务程序本身有问题 ——
通常是计算机名或许可文件问题。

---

## 构建与环境问题

### `cargo check` 报 `ring` / `cl.exe` 错误

**症状**

```
error: failed to run custom build command for `ring v0.17.x`
cl : Command line error D8050 : cannot execute '...c1.dll'
```

**原因**：引入了需要 C 编译器的依赖。本项目刻意使用 `native-tls`
而非 `rustls-tls` 正是为了避免这类问题。

**检查是否有人改回了 rustls**：

```powershell
Select-String -Path "src-tauri\Cargo.toml" -Pattern "rustls|native-tls"
```

应只出现 `native-tls`：

```toml
reqwest = { version = "0.12", default-features = false, features = [
    "stream", "native-tls", "http2", "charset",
] }
```

**检查是否有间接引入**：

```powershell
cargo tree --manifest-path src-tauri/Cargo.toml -i ring
```

若输出显示来自本项目直接依赖，改回 `native-tls`；
若来自某个新增依赖的默认特性，用 `default-features = false` 关掉它。

### `npm run build` 报 esbuild 错误

**症状**

```
Error: Cannot find module '.../esbuild.exe'
```

或

```
You installed esbuild for another platform than the one you're currently using
```

**原因**：npm 的 `allowScripts` 策略拦截了 esbuild 的 postinstall。

**解决**：

```powershell
# 检查平台包是否安装
Get-ChildItem node_modules\@esbuild

# 应看到 win32-x64 目录；若无：
npm install --force esbuild

# 验证
node node_modules/esbuild/bin/esbuild --version
```

**若仍失败**：在 `package.json` 中添加 allowScripts 白名单：

```json
"allowScripts": { "esbuild": true }
```

然后重新 `npm install`。

### `npm install` 报 peer dependency 冲突

**症状**

```
npm error ERESOLVE unable to resolve dependency tree
npm error Could not resolve dependency:
npm error peer vite@"^5.0.0" from @sveltejs/vite-plugin-svelte@4.0.4
```

**原因**：`@sveltejs/vite-plugin-svelte` 与 `vite` 大版本不匹配。

| 插件版本 | 需要的 Vite |
|---|---|
| `^4` | `^5` |
| `^5` | `^6` |
| `^6` | `^6.3` 或 `^7` |

本项目使用 `@sveltejs/vite-plugin-svelte@^5` + `vite@^6`。检查：

```powershell
Select-String -Path package.json -Pattern "vite-plugin-svelte|\"vite\""
```

### `tauri-build` 报找不到 `icons/icon.ico`

**症状**

```
`C:\...\src-tauri/icons/icon.ico` not found; required for generating a Windows Resource file during tauri-build
```

**解决**：重新生成图标（需要 Python + Pillow）：

```powershell
python src-tauri/icons/generate_icons.py
Get-ChildItem src-tauri\icons
```

应看到 `icon.ico` 与若干 PNG。

### `cargo check` 卡在 `Compiling tauri` 很久

首次编译 Tauri 需要 5–15 分钟，属正常。后续增量编译为秒级。

**加速手段**

- 保持 `src-tauri/target/` 不被清理
- 使用 `sccache` 或 `mold`（后者仅 Linux）
- 只在需要时用 `--release`

### 端口 1420 被占用

**症状**

```
error when starting dev server:
Error: Port 1420 is already in use
```

**解决**：

```powershell
# 找出占用进程
Get-NetTCPConnection -LocalPort 1420 | Select-Object OwningProcess
Get-Process -Id (Get-NetTCPConnection -LocalPort 1420).OwningProcess
```

关闭残留的 `node.exe` 或 `solidworks-installer.exe`。

> `vite.config.ts` 设置了 `strictPort: true`，这是刻意的 ——
> 静默换端口会让 Tauri 连到错误的地址。

### WebView2 缺失（Windows 10）

**症状**：应用启动后窗口空白，或提示无法加载。

```powershell
# 检查是否已安装
Get-ItemProperty "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" -ErrorAction SilentlyContinue
```

**解决**：安装 Evergreen Runtime：

```powershell
winget install Microsoft.EdgeWebView2Runtime
```

---

## 界面问题

### 界面显示为空白

**检查 1：是否在浏览器中直接打开**

程序前端需要 Tauri 宿主注入 `window.__TAURI_INTERNALS__`。
在浏览器里打开 `http://localhost:1420` 会看到界面，但所有后端调用都会失败，
页面会在设置页显示「未检测到 Tauri 宿主」。

**检查 2：`dist/` 是否存在（cargo check 场景）**

`tauri::generate_context!()` 在编译期需要 `frontendDist` 指向的目录存在：

```powershell
npm run build
Test-Path dist\index.html
```

**检查 3：浏览器控制台**

开发模式右键 → 检查 → Console，查看是否有资源 404 或 CSP 报错。

### 中文字体显示为方块

界面使用系统字体栈（`Inter` / `Space Grotesk` / `Noto Sans SC` /
`Source Han Sans SC` / `PingFang SC` / `system-ui`）。
Windows 上回退到「微软雅黑」通常正常。

若出现方块，说明系统缺少东亚字体：

```powershell
# 检查已安装字体
Get-ChildItem "C:\Windows\Fonts" -Filter "msyh*"
```

缺失时通过「设置 → 应用 → 可选功能 → 添加功能 → 中文（简体）补充字体」安装。

### 高对比模式 / 深色模式下文字不可读

程序当前只提供 light 配色（`index.html` 中 `color-scheme: light`），
信号黄在深色背景下不满足对比度要求。

**若系统强制深色**，可在 WebView2 中禁用强制深色模式：

```powershell
# 让 WebView2 忽略系统深色偏好
[Environment]::SetEnvironmentVariable('WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS', '--force-dark-mode=false', 'User')
```

（需要重启应用生效。）

### 界面缩放异常 / 元素重叠

程序在以下断点重新编排：`≤860px`、竖屏（`portrait and ≤1024px`）、
短宽（`max-height:780px and ≥861px`）。

**Windows 缩放设置**会改变 CSS 像素与物理像素的比例。
若在 150% 缩放下出现重叠，请记录窗口的逻辑尺寸（浏览器控制台执行）：

```js
`${window.innerWidth}x${window.innerHeight} dpr=${devicePixelRatio}`
```

并附在问题报告中 —— 该信息能直接定位到命中的断点。

### 动效看起来「没有」

若系统开启了「减少动画效果」：
**设置 → 辅助功能 → 视觉效果 → 动画效果**，程序会遵循
`prefers-reduced-motion: reduce` 并提供静态编排（这是刻意设计，不是故障）。
装饰性扫描带会被隐藏，揭示动效会直接呈现最终状态。

---

## 仍然无法解决

收集以下信息后提交问题：

1. **环境快照** —— 设置页的「运行环境实测」面板（用户名、计算机名、
   管理员权限、可用空间、配置路径、临时目录）
2. **导出的日志文件** —— 日志页「导出全部」
3. **生效的配置** —— 设置页「查看默认值」旁边点「重新读取」后截图，
   或用 `notepad "$env:APPDATA\com.solidworks.autoinstaller\config.toml"` 打开
4. **失败步骤 + 错误信息全文** —— 日志页按「错误」过滤后的完整文本
5. **软件版本** —— `npm run tauri:dev` 启动日志首行的版本信息

**切勿在问题报告中包含序列号、许可文件内容或任何个人信息。**

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CONFIG_GUIDE.md`](./CONFIG_GUIDE.md) | **我需要填什么** —— 每个键是否必填、填什么、最小可用示例、自检清单 |
| [`COMMAND_LINE.md`](./COMMAND_LINE.md) | 官方命令行属性参考：属性名、语言代码表、ADDLOCAL 层级、介质路径 |
| [`CONFIG.md`](./CONFIG.md) | 每个配置项的类型 / 默认值 / 取值范围 / 内部语义 |
| [`CUSTOMIZE.md`](./CUSTOMIZE.md) | 部署前必须改什么、已知局限、完整自检清单 |
| [`INSTALL_FLOW.md`](./INSTALL_FLOW.md) | 13 阶段流程详解、弹窗规则、轮询逻辑 |
| [`DEVELOPMENT.md`](./DEVELOPMENT.md) | 二次开发与构建 |
