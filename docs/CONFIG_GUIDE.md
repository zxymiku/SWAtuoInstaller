# TOML 配置填写指南

本文档只回答一个问题：**`config.toml` 里哪些内容需要你自己填，填什么。**

每项的完整类型/取值范围/内部语义见 [`CONFIG.md`](./CONFIG.md)；
部署前的完整自检清单见 [`CUSTOMIZE.md`](./CUSTOMIZE.md)。

---

## 配置文件在哪

程序启动时按「内置默认值 ← 用户配置」的顺序合并，用户配置落在：

```text
%APPDATA%\<应用标识符>\config.toml
```

默认标识符是 `com.solidworks.autoinstaller`，即：

```text
%APPDATA%\com.solidworks.autoinstaller\config.toml
```

**怎么打开它**：

| 方式 | 操作 |
|---|---|
| 界面（推荐） | 设置页改完点「保存配置」 |
| 生成一份带注释的模板 | 设置页点「**导出当前配置**」 |
| 直接用编辑器 | 资源管理器地址栏输入 `%APPDATA%\com.solidworks.autoinstaller` |

> 没填的项会自动沿用内置默认值；程序升级新增的字段也会自动补齐。
> 所以**你只需要写你想改的那几项**，不必抄整份文件。

---

## 一、必须填的（不填跑不起来）

### 1. 下载地址 —— `[download]`

默认值是占位域名 `example.com`，**必须替换**。两种模式二选一。

#### 模式 A：单个整包

```toml
[download]
url = "https://你的服务器/SolidWorks.2024.SP5.0.Premium.DVD.iso"
```

#### 模式 B：分卷 / 分片（你的场景）

每个分片一行，**顺序必须与实际分卷号一致**：

```toml
[download]
multipart_urls = [
  "https://.../Downloads.7z.001",
  "https://.../Downloads.7z.002",
  "https://.../Downloads.7z.003",
  "https://.../Downloads.7z.004",
  "https://.../Downloads.7z.005",
  "https://.../Downloads.7z.006",
  "https://.../Downloads.7z.007",
  "https://.../Downloads.7z.008",
]
multipart_concat = false
```

**关于文件名后缀**：不需要你操心。程序会在下载前先把每个地址解析成**真实文件名**，
自动剥掉 `.aspx` / `.ashx` / `.php` 这类下载端点的假后缀：

| 你填的地址形态 | 程序实际存成 |
|---|---|
| `...download.aspx?SourceUrl=%2F...%2FDownloads%2E7z%2E002` | `Downloads.7z.002` |
| `...download.aspx?UniqueId=6c0c5656%2D...` | 用 `Content-Disposition` 或兜底名 |
| `.../x/SolidWorks.2024.SP5.0.Premium.DVD.iso` | 原样 |
| `.../download.php`（内容其实是 zip） | `download.php.zip` |

**这一步为什么关键**：7z 拿到 `.001` 会自动读同目录的整套分卷，
但前提是它们的名字必须是 `xxx.001`、`xxx.002`…
如果存成 `xxx.001.aspx`，7z 就找不到 `.002`，整个包解不开。
所以程序**绝不给分卷追加后缀**，并且在多片解析出同一个名字时
（网盘常见）会按你填写的顺序补上 `.001/.002/…`。

**分享短链（`1drv.ms` 之类）**：这类地址的路径末段是分享令牌、不是文件名，而且
私有分享通常**需要登录**，程序直接请求会拿到一个 HTML 登录页。
程序会识别这种情况并明确报错（而不是把登录页当压缩包存下来）。
**请用浏览器登录后手动点击下载，复制最终落盘的直链**填进来。

```toml
# ✅ 可以：带 ?SourceUrl= 的 download.aspx 直链
multipart_urls = ["https://onedrive.live.com/personal/<id>/_layouts/15/download.aspx?SourceUrl=%2F...%2FDownloads%2E7z%2E001"]

# ❌ 不行：1drv.ms 分享短链（需要登录，且路径末段是令牌）
multipart_urls = ["https://1drv.ms/u/c/<id>/<token>?e=xxxxxx"]
```

#### 网盘直链务必保留的开关

```toml
[download]
probe_filenames = true    # 保持 true，否则拿不到真实文件名
```

### 2. 安装目标 —— `[install]`

```toml
[install]
install_drive = "D"    # 建议不要留在 C 盘
install_path = ""      # 留空则解析为 D:\SW
```

完整安装约 20 GB 以上，加上临时解压产物可能再占 60 GB，
所以**目标盘要留足空间**。

---

## 二、建议检查的（默认值可能不适合你）

### 3. 下载并发 —— `[download]`

```toml
[download]
threads = 32            # 每个文件切几块（1~255）
concurrent_files = 4    # 同时下几个文件（1~32）
```

- **峰值连接数 = `threads × concurrent_files`**。默认 `32 × 4 = 128`，
  对家宽和多数网盘都偏激进。
- 被网盘限流 / 出现大量重试 → 先把 `concurrent_files` 降到 `2`，
  再把 `threads` 降到 `16`。
- 带宽充裕、分片很多 → 可以试 `concurrent_files = 6`。

### 4. 7z 解压程序 —— `[sevenzip]`

程序会自动查找（配置 → 打包资源 → 下载缓存 → 已装的 7-Zip → NanaZip 等变体 → PATH）。
找不到时默认从**官方源**自动下载。

| 你的环境 | 要改的 |
|---|---|
| 能访问 GitHub | 不用改 |
| **内网 / 离线** | `auto_download = false`，并自备 7z（或改 `download_url` 指向内网镜像） |
| 机器上已装 7-Zip / NanaZip | 不用改，会自动检测到 |

```toml
[sevenzip]
auto_download = false                                  # 离线环境必须关
exe_path = "D:\\tools\\7z.exe"                         # 或显式指定
download_url = "http://intranet.mirror.local/7zr.exe"  # 或换内网镜像
```

> 离线环境**务必**设 `auto_download = false`，
> 否则每次都会等一次注定失败的网络请求。

### 5. 安装组件 —— `[install]`

> **先读这一段**：SolidWorks 的组件选择**不是**由命令行开关决定的，
> 而是由安装管理器（`sldIM.exe`）生成的一份 `.sldIM` 配置文件决定的。
> 因此 `components_whitelist` **只在本程序走 `msiexec` 回退路径时生效**。

#### 情况 A：装全套（想装 Simulation / Flow 等时改成这样）

```toml
[install]
components_whitelist = []      # 空数组 = 全部组件
force_msiexec = false          # 走 StartSWInstall（官方推荐路径）
```

这是**最稳的路径**：安装管理器会做前置检查，并按官方顺序串接安装各组件。

程序执行的命令是：

```text
<介质>\sldim\startswinstall.exe /install /now
```

| 开关 | 作用 |
|---|---|
| `/install` | 开始安装（不加只会打开安装界面） |
| `/now` | **跳过「5 分钟后开始安装」的警告对话框**，否则会白等 5 分钟 |

要追加别的开关，写 `install_switches`（程序仍会固定带上 `/install /now`）：

```toml
[install]
install_switches = ["/l", "C:\\sw-install.log"]   # 例：写安装日志
```

#### 情况 B：只装主程序 + Toolbox（本项目当前默认）

**默认配置已经按这个目标设好并核对过**：

```toml
[install]
force_msiexec = true
components_whitelist = ["SolidWorks", "AddIns", "SolidWorksToolbox"]
```

程序会执行：

```text
msiexec /i "<介质>\swwi\data\solidworks.msi"
        INSTALLDIR=<目标>
        ADDLOCAL=SolidWorks,AddIns,SolidWorksToolbox
        TOOLBOXFOLDER=C:\SOLIDWORKS_Data      # 不要含空格
        ENABLEPERFORMANCE=0
        /qb /norestart
```

随后再单独启动简体中文语言包（见下方「关于语言与序列号」）。

##### 这三个名字是怎么来的（实测核对，不是猜的）

从 SolidWorks 2024 SP5 `Premium.DVD` 的 `swwi\data\solidworks.msi` 里
读 **Feature 表**（该 MSI 共 **221 个 Feature**），层级如下：

```text
SolidWorks                     ← SOLIDWORKS 2024 SP05（主程序）
  ├── AddIns                   ← SOLIDWORKS Add-Ins
  │     ├── Motion
  │     ├── ScanTo3D
  │     ├── SolidWorksCosting
  │     ├── CircuitWorks
  │     ├── FeatureWorks
  │     ├── SolidWorksRoutedsystems
  │     ├── Simulation
  │     ├── SolidWorksToolbox  ← ← ← 我们要的 Toolbox
  │     ├── SolidWorksUtilities
  │     └── TolAnalyst
  ├── Manuals / HelpFiles / ExampleFiles / Learn …
  └── ProgramFiles / SupportedLanguages / Vsta3 …
```

> ⚠️ **必须写全三项，缺一不可**：
>
> | 只写 | 后果 |
> |---|---|
> | `SolidWorksToolbox` | **缺父级，msiexec 会忽略它** —— Toolbox 装不上 |
> | `SolidWorks` | 会连带把 **全部 AddIns** 装上（Simulation、FeatureWorks、CircuitWorks…），不是你要的最小集 |
> | `SolidWorks,AddIns,SolidWorksToolbox` | ✅ 正好是主程序 + Toolbox |
>
> `ADDLOCAL` 的语义是「安装列出的这些 Feature」，
> 因此**列父级 ≠ 装全部子级**，但要装某个子级就**必须**把它的父级一并列出。

##### 大小写必须一致

真实名字是 `SolidWorks`（小写 w、大写 W），**不是** `SOLIDWORKS`。
`ADDLOCAL` 的 Feature 名匹配是**区分大小写**的，写错会被静默忽略。

##### 自己核对的三个办法

1. **直接读 Feature 表**（不用装任何工具）
   在 PowerShell 里跑：

   ```powershell
   $msi = "<介质>\swwi\data\solidworks.msi"
   $db = New-Object -ComObject WindowsInstaller.Installer
   $v = $db.OpenDatabase($msi, 0).OpenView("SELECT Feature, Feature_Parent, Title, Level FROM Feature")
   $v.Execute(); while ($r = $v.Fetch()) { "{0}`t{1}`t{2}" -f $r.StringData(1), $r.StringData(2), $r.StringData(3) }
   ```

2. **从安装日志里搜**（最贴近实际）
   先手动装一次并勾选你要的组件，然后到 `%TEMP%` 找
   `SOLIDWORKS Installation Manager*.log`，搜 `ADDLOCAL=` ——
   照抄日志里出现的列表。

3. **Orca**（图形化，Microsoft 官方工具）
   用 7-Zip 从介质里解出 `swwi\data\solidworks.msi`，
   用 Orca 打开看 **Feature** 表。

##### 想改成别的组件组合

从上表里挑，**连同父级一起写**：

| 想要 | components_whitelist |
|---|---|
| 主程序 only | `["SolidWorks"]` |
| 主程序 + Toolbox（默认） | `["SolidWorks", "AddIns", "SolidWorksToolbox"]` |
| 主程序 + Toolbox + Simulation | `["SolidWorks", "AddIns", "SolidWorksToolbox", "Simulation"]` |
| 主程序 + 简体中文语言 | 语言包是**另一个 MSI**（`swwi\lang\chinese-simplified\chinese-simplified.msi`），不在 `components_whitelist` 里 |

##### 代价：会跳过安装管理器

`force_msiexec = true` 跳过 `startswinstall.exe`，
也就跳过安装管理器负责的**前置检查**（磁盘/依赖/冲突）与
**多 MSI 的串接安装**。所以：

- 只要主程序（+ Toolbox）→ 当前默认配置没问题；
- 想装全套（Simulation / Flow Simulation / Electrical / PDM / 语言包）→
  把 `force_msiexec` 改回 `false`、`components_whitelist` 清空，走官方路径更稳。

##### 安装介质里的真实路径（排查时用得上）

```text
<介质根>\
  setup.exe                          ← 引导程序（GUI，双击用）
  sldim\
    startswinstall.exe               ← 命令行安装入口
    sldIM.exe                        ← 图形化安装管理器
  swwi\
    data\solidworks.msi             ← 主程序 MSI（58.8 MB）
    data\swc.msi
    lang\chinese-simplified\chinese-simplified.msi
  PreReqs\
```

> 早期版本把 `StartSWInstall.exe` 放在根目录，所以程序**两种位置都会查**
> （`sldim\startswinstall.exe` → 根目录 → `sldIM\`）。
> 程序启动时的日志会写明最终用了哪个文件。

#### ⚠️ `toolbox_folder` 不要含空格

默认值 `C:\SOLIDWORKS_Data` 用**下划线**而非空格，这不是笔误。

实测（真实 msiexec，`/qn` 只做参数解析）：含空格的属性值**会让 msiexec
弹出用法对话框并卡住**，安装完全不执行、日志也不生成。
**加引号也没用** —— `"C:\SOLIDWORKS Data"` 同样被拒。

所以请使用**不含空格**的目录，例如 `C:\SOLIDWORKS_Data`、`D:\SWData`。

#### 关于语言与序列号

```toml
[install]
serial_number = ""                       # 留空则依赖补丁包里的 .reg 授权
toolbox_folder = "C:\\SOLIDWORKS Data"   # Toolbox 数据目录
language_pack = "chinese-simplified"     # 介质 swwi\lang\ 下的目录名；留空不装
language_pack_delay_minutes = 0          # 兼容旧配置；主程序退出后立即启动语言包
```

##### 语言包是**独立的 MSI**，必须单独安装

官方文档原文：

> SOLIDWORKS 法语安装组件必须单独安装：
> `msiexec /i "...\64bit\SOLIDWORKS French\french.msi" /qb`
>
> 指定 SOLIDWORKS 语言组件安装命令时，请勿指定命令行参数。

所以程序的做法是**先后启动两个 `msiexec`**：

```text
1) msiexec /i "...\swwi\data\solidworks.msi" INSTALLDIR=... ADDLOCAL=... /qb
2) （主程序 MSI 进程退出后立即启动）
   msiexec /i "...\swwi\lang\chinese-simplified\chinese-simplified.msi" /qb
```

第二步**不传** `INSTALLDIR` / `ADDLOCAL` —— 遵守官方「请勿指定命令行参数」的要求。

> 语言包不再使用固定延迟，而是等待主程序这个具体 MSI 进程退出后立即启动，
> 这样不会在主程序完成后额外空等十分钟。

##### `language_pack` 的取值

就是介质 `swwi\lang\` 下的**目录名**（不是 LCID，也不是 `ChineseSimplified` 这种拼法）：

| 值 | 语言 | LCID |
|---|---|---|
| **`chinese-simplified`** | **简体中文（默认）** | 2052 |
| `chinese` | 繁体中文 | 1028 |
| `english` | 英语 | 1033 |
| `japanese` | 日语 | 1041 |
| `korean` | 韩语 | 1042 |
| `german` | 德语 | 1031 |
| `french` | 法语 | 1036 |
| `italian` | 意大利语 | 1040 |
| `spanish` | 西班牙语 | 1034 |
| `russian` | 俄语 | 1049 |
| `polish` | 波兰语 | 1045 |
| `czech` | 捷克语 | 1029 |
| `turkish` | 土耳其语 | 1055 |
| `portuguese-brazilian` | 巴西葡萄牙语 | 1046 |

留空 = 不装语言包。**值写错不会报错** —— 程序会在日志里告警并列出已查找的路径。

##### 序列号的官方属性名

```text
SOLIDWORKSSERIALNUMBER="xxxx xxxx xxxx xxxx xxxx xxxx"
```

**不是 `SERIALNUMBER`。** 项目早期版本用错了这个名字，会导致序列号被静默忽略。
现在传的是官方属性名。

> 完整的官方属性清单（`ENABLEPERFORMANCE` / `OFFICEOPTION` / `TOOLBOXFOLDER` /
> `NOTTOOLBOXSETUP` / `REINSTALLMODE` 等）见
> [`COMMAND_LINE.md`](./COMMAND_LINE.md)。

---

### 6. Windows 前置组件 —— `[install]`

**这是最容易踩的坑。** 官方手册（第 31~34 页）明确：

> 创建管理映像后，在通知客户端之前，您**必须安装**无法通过使用命令行或
> Microsoft Active Directory 创建的管理映像来安装的 Microsoft Windows 组件。
>
> **所有 SOLIDWORKS 产品均要求有 Visual C++ 可重新分发软件包和
> .NET Framework 4.8。**

**平时双击 `setup.exe` 时是安装管理器替我们装了这些**；
一旦改走纯 `msiexec` 命令行路线（也就是本项目 `force_msiexec = true` 的做法），
这一环就必须自己补上。

```toml
[install]
install_prerequisites = true         # 检测缺失项并从介质 PreReqs\ 安装
prerequisite_timeout_minutes = 20    # 单个组件超时（.NET 包有 112 MB）
```

### 缺了会怎样（重要）

**不是"装不上"，而是"装上了跑不起来"。**

`swwi\data\solidworks.msi` **没有 `LaunchCondition` 表**（实测确认），
所以缺前置组件时 `msiexec` 依然报**安装成功** ——
直到你启动 SOLIDWORKS、系统找不到 DLL 才暴露问题，那时很难想到是缺 VC++。

### 程序会检测这 5 项

| 组件 | 强制 | 介质里的位置 |
|---|---|---|
| Visual C++ 2015-2022 (x64) | 是 | `PreReqs\VCRedist17\VC_redist.x64.exe` |
| Visual C++ 2015-2022 (x86) | 是 | `PreReqs\VCRedist17\VC_redist.x86.exe` |
| .NET Framework 4.8 | 是 | `PreReqs\dotNetFx\ndp48-x86-x64-allos-enu.exe` |
| Microsoft Edge WebView2 Runtime | 是 | `sldim\MicrosoftEdgeWebView2RuntimeInstallerX64.exe` |
| Visual Basic for Applications 7.1 | 否 | `PreReqs\VBA\vba71.msi` |

流程是**先检测（只读注册表）→ 只装缺的 → 装完复检**。
已装的项会跳过，不会重复安装。

> **关于 Visual C++ 2010**：手册的通用前置表提到「VC++ 2010 和 2022」，
> 但**这套介质里没有 2010 的安装包**（`PreReqs\` 下只有 `VCRedist17`）。
> 因此程序不检查也不安装它 —— 不去要求一个介质里根本不存在的东西。

### 想自己先装好

把 `install_prerequisites` 设为 `false`，然后手动装介质 `PreReqs\` 下的组件。
程序仍会**检测并告警**，只是不替你装。

> 完整的官方属性与前置组件说明见
> [`COMMAND_LINE.md`](./COMMAND_LINE.md)。

---

## 三、按需调整的（默认值通常够用）

### 7. 网络隔离 —— `[network]`

```toml
[network]
disable_network = true          # 安装期间禁用网卡
restore_network_after = true    # 装完恢复
```

- 安装过程**需要联网**（在线许可校验、下载组件）→ 必须把 `disable_network` 设为 `false`。
- **不要**把 `restore_network_after` 设为 `false`，否则程序退出后网卡会一直是禁用状态。

### 8. 进程关键词 —— `[process]`

```toml
[process]
kill_pattern = ".*solidwork.*|.*sldworks.*|.*flexnet.*"
match_command_line = true    # 同时匹配路径与命令行（更全但更慢）
```

- **不区分大小写**、首尾空白自动忽略；写纯关键词即可。
- **空值会被拒绝**（避免误杀全部进程）。
- 你的补丁程序有别的名字 → 往 `kill_pattern` 里再加关键词。
- 只关心进程名、想更快 → `match_command_line = false`。

### 9. 安全软件 —— `[antivirus]`

Defender 与多数杀软会把 `_SolidSQUAD_` 里的补丁判为威胁并**直接删除**，
所以本阶段在**解压之前**执行。

```toml
[antivirus]
enabled = true
remove_defender = true
third_party_mode = "terminate"
```

| 取值 | 行为 |
|---|---|
| `prompt` | 只检测 + 提示你手动从托盘退出 |
| `terminate` | 额外终止其进程 / 停止其服务（默认） |
| `uninstall` | 再额外尝试静默卸载（**会卸载别人的软件，谨慎**） |

**不想让程序碰杀软** → `enabled = false`，然后自己把工作目录与安装目录加进排除项。

### 10. 超时与清理

| 场景 | 要改的项 |
|---|---|
| 慢速机械盘 / 超大镜像 | 调大 `[sevenzip].extract_timeout_minutes`（默认 30）、`[install.polling].timeout_minutes`（默认 240） |
| **首次部署想看现场** | `[workdir].cleanup_temp_after = false`（确认流程通了再改回 `true`） |
| 临时目录想放别处 | `[workdir].temp_dir = "D:\\sw-temp"` |
| 网络不稳定 | 调大 `[download].retry_count`、`read_timeout_minutes` |

### 11. 远程配置 —— `[remote_config]`

```toml
[remote_config]
url = "https://你的服务器/config.toml"   # ← 别留着 example.com
auto_fetch_on_start = false
```

不用远程配置就把它留空，并把 `auto_fetch_on_start` 保持 `false`。

---

## 四、各字段一览（速查）

| 段 | 键 | 需要你填？ | 说明 |
|---|---|---|---|
| `general` | `language` / `theme` | 否 | 界面语言与风格 |
| `download` | `url` | **是**（单包模式） | 整包下载地址 |
| | `multipart_urls` | **是**（分片模式） | 每片一个地址，按顺序 |
| | `threads` | 建议 | 每文件线程数，默认 32 |
| | `concurrent_files` | 建议 | 同时下几个文件，默认 4 |
| | `probe_filenames` | **保持 true** | 网盘直链靠它解析真名 |
| | `multipart_concat` | 按需 | 非标准切分才设 `true` |
| | `checksum_sha256` | 可选 | 分片模式下不生效 |
| `remote_config` | `url` | 按需 | 别留 `example.com` |
| `install` | `install_drive` | **是** | 目标盘符 |
| | `install_path` | 可选 | 留空 = `{drive}:\SW` |
| | `components_whitelist` | 已预设 | 默认 `["SOLIDWORKS", "Toolbox"]`，**仅 msiexec 路径生效** |
| | `force_msiexec` | 已预设 | 默认 `true`（配合白名单只装主程序 + Toolbox） |
| | `install_switches` | 按需 | 追加给 startswinstall 的开关 |
| | `install_prerequisites` | **建议确认** | 默认 `true`；检测并装 Windows 前置组件 |
| | `prerequisite_timeout_minutes` | 按需 | 默认 20；.NET 包有 112 MB |
| | `serial_number` | 可选 | 留空靠 `.reg` 授权 |
| | `language_pack` | 已预设 | 默认 `chinese-simplified`；取值即介质 `swwi\lang\` 下的目录名 |
| `install.polling` | 全部 | 按需 | 轮询间隔与超时 |
| `install.popup` | 全部 | 按需 | 弹窗自动处理 |
| `network` | `disable_network` | 按需 | 需要联网就必须关 |
| `process` | `kill_pattern` | 建议 | 不区分大小写；空值被拒 |
| | `match_command_line` | 按需 | 更全但更慢 |
| `flexnet` | 全部 | 否 | 服务安装超时 |
| `workdir` | `temp_dir` / `cleanup_temp_after` | 按需 | 临时目录与清理 |
| `sevenzip` | `auto_download` | **离线必改** | 离线设 `false` |
| | `exe_path` / `download_url` | 按需 | 自备或内网镜像 |
| `antivirus` | `enabled` / `remove_defender` / `third_party_mode` | 建议确认 | 默认会移除 Defender、终止第三方杀软 |

---

## 五、最小可用配置示例

### 例 1：分片 + 网盘直链（你的场景）

```toml
[download]
multipart_urls = [
  "https://onedrive.live.com/personal/585b0a8a02824759/_layouts/15/download.aspx?SourceUrl=%2Fpersonal%2F585b0a8a02824759%2FDocuments%2F%E6%96%87%E6%A1%A3%2FDownloads%2E7z%2E001",
  "https://onedrive.live.com/personal/585b0a8a02824759/_layouts/15/download.aspx?SourceUrl=%2Fpersonal%2F585b0a8a02824759%2FDocuments%2F%E6%96%87%E6%A1%A3%2FDownloads%2E7z%2E002",
  # …003 到 008，按顺序补全
]
multipart_concat = false
threads = 32
concurrent_files = 4
probe_filenames = true

[install]
install_drive = "D"
install_path = ""

[antivirus]
enabled = true
remove_defender = true
third_party_mode = "terminate"
```

### 例 2：内网镜像 + 离线

```toml
[download]
url = "http://intranet.mirror.local/sw/SolidWorks.2024.SP5.0.Premium.DVD.iso"
threads = 16
concurrent_files = 2
checksum_sha256 = "在此填官方 SHA-256"    # 内网建议启用校验

[sevenzip]
auto_download = false
exe_path = "C:\\Tools\\7z.exe"

[remote_config]
url = "http://intranet.mirror.local/sw/config.toml"
auto_fetch_on_start = true

[network]
disable_network = false    # 需要联网校验许可

[workdir]
cleanup_temp_after = false # 首次部署保留现场
```

### 例 3：只装部分组件

```toml
[install]
install_drive = "D"
force_msiexec = true          # 必须打开，否则 components_whitelist 不生效
components_whitelist = [
  "SOLIDWORKS",               # ← 核对过 MSI Feature 名后再填
  "Toolbox",
]
serial_number = "在此填序列号"
```

> 这两个名字是本项目的默认值，**但请用上面「怎么核对」里的办法确认一遍**
> —— 名字不符会被 msiexec 静默忽略。"

---

## 六、自检清单

- [ ] `[download].url` 或 `multipart_urls` 填了**真实可用**的地址
- [ ] 分片地址**按卷号顺序**排列（`.001` 在第一个）
- [ ] `probe_filenames` 保持 `true`
- [ ] `threads × concurrent_files` 不超过你的带宽与网盘限流承受度
- [ ] `[install].install_drive` 指向空间充足的盘
- [ ] 离线环境：`[sevenzip].auto_download = false` 且 7z 已就位
- [ ] `[remote_config].url` 不再是 `example.com`（或 `auto_fetch_on_start` 为 `false`）
- [ ] **`[install].install_prerequisites` 按预期配置**（默认 true：自动装 .NET 4.8 / VC++ / WebView2 / VBA；缺失会让 SOLIDWORKS 装上了却跑不起来）
- [ ] 需要联网安装 → `[network].disable_network = false`
- [ ] `[process].kill_pattern` 覆盖你的补丁程序名字
- [ ] 首次部署先设 `[workdir].cleanup_temp_after = false`
- [ ] 目标机器**计算机名与用户名都是纯 ASCII**
- [ ] 以**管理员身份**运行

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CONFIG.md`](./CONFIG.md) | 每个配置项的类型 / 默认值 / 取值范围 / 内部语义 |
| [`COMMAND_LINE.md`](./COMMAND_LINE.md) | 官方命令行属性参考：属性名、语言代码表、ADDLOCAL 层级、介质路径 |
| [`CUSTOMIZE.md`](./CUSTOMIZE.md) | 部署前必须改什么、已知局限、完整自检清单 |
| [`INSTALL_FLOW.md`](./INSTALL_FLOW.md) | 13 阶段流程详解 |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | 按症状排查 |
