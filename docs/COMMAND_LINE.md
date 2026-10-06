# 官方命令行属性参考（SolidWorks 2024）

本文档记录本项目**实际使用**的 SolidWorks 安装命令行属性，以及每个值的来源。
目的是让后续维护者不必再去翻官方帮助站 —— 那里从部分网络无法访问。

---

## 来源

### 主要来源：官方安装手册（最权威）

[`html/install_guide.pdf`](../html/install_guide.pdf) ——
**SOLIDWORKS 2024《安装与管理》官方手册，共 123 页。**

本文档的所有结论都以它为准。相关章节：

| PDF 页 | 章节 | 提供了什么 |
|---|---|---|
| 31–34 | 准备客户端以便通过命令行从管理映像进行安装 | **Windows 前置组件清单**（命令行不会自动装！） |
| 35–36 | 通过命令行从管理映像安装 | `msiexec` 调用模板、语言包需单独安装 |
| 37 | 命令行特征属性 | **官方 `ADDLOCAL` 属性清单**（15 项） |
| 37–40 | 命令行全局属性 | 全部全局属性与产品序列号属性 |
| 41–44 | eDrawings / Flow Simulation / Inspection / Visualize 属性 | 各产品特定属性 |
| **45–46** | **管理映像的 MSI 文件位置** | **每个组件的 MSI 路径**（含 13 种语言包） |

> 手册第 45 页的原文（照录）：
>
> ```text
> SOLIDWORKS（核心产品，英文）
>     swwi\data\SOLIDWORKS.msi
> SOLIDWORKS（非英文组件）
>     简体中文  swwi\lang\chinese-simplified\chinese-simplified.msi
>     繁体中文  swwi\lang\chinese\chinese.msi
>     法文      swwi\lang\french\french.msi
>     …（共 13 种）
> ```

### 次要来源：离线保存的帮助页

[`html/`](../html) 下另有 7 个帮助页 HTML。**但它们的正文完全相同**
（都是「通过命令行从管理映像安装」那一篇）——
官方帮助站用客户端路由，逐个"另存为"会把同一页存 7 份。
因此其余 6 个主题的内容**不在仓库里**，本文档中凡涉及那些主题的结论，
要么来自官方 PDF，要么标注「来自 MSI 实测」。

## 一、官方调用模板

官方文档给出的 SOLIDWORKS 静默安装模板（**原文照录**）：

```text
msiexec /i "administrative_image_directory\64bit\SOLIDWORKS\SOLIDWORKS.Msi"
 INSTALLDIR="C:\Program Files\your_folder"
 SOLIDWORKSSERIALNUMBER="xxxx xxxx xxxx xxxx xxxx xxxx"
 ENABLEPERFORMANCE=1 OFFICEOPTION=3 ADDLOCAL=SolidWorks,SolidWorksToolbox
 TOOLBOXFOLDER="toolbox path for example C:\SolidWorks Data\"
 /qb
```

语言包（官方明确「必须单独安装」，且「请勿指定命令行参数」）：

```text
msiexec /i "administrative_image_directory\64bit\SOLIDWORKS French\french.msi" /qb
```

---

## 二、全局属性

### `SOLIDWORKSSERIALNUMBER`

序列号。

```text
SOLIDWORKSSERIALNUMBER="xxxx xxxx xxxx xxxx xxxx xxxx"
```

> **不是** `SERIALNUMBER`。项目早期版本用错了这个属性名，会导致序列号被静默忽略。

对应配置：`[install].serial_number`

### `ENABLEPERFORMANCE`

是否把性能数据发送给 SOLIDWORKS Corporation。

| 值 | 含义 |
|---|---|
| `1` | 发送性能数据（官方模板用的值） |
| `0` | **不发送** |

> 本项目默认传 `0` —— 不向厂商上报使用数据是更保守的选择。
> 官方模板是 `1`，这是刻意的差异。

### `OFFICEOPTION`

产品级别。

| 值 | 产品 |
|---|---|
| `0` | SOLIDWORKS Standard |
| `1` | SOLIDWORKS Office |
| `2` | SOLIDWORKS Professional |
| `3` | SOLIDWORKS Premium |

> 本项目**不传**该属性：用什么级别应当由序列号决定，
> 擅自指定可能与实际授权不符。

### `TOOLBOXFOLDER`

Toolbox 的数据目录，只在组件白名单含 `SolidWorksToolbox` 时才传。

```toml
[install]
toolbox_folder = "C:\\SOLIDWORKS_Data"
```

> ⚠️ **值里不要有空格 —— msiexec 无法接受含空格的属性值。**
>
> 本机用真实 `msiexec`（`/qn`，只做参数解析）实测：
>
> | 属性值 | 结果 |
> |---|---|
> | `C:\SOLIDWORKS_Data`（无空格） | ✅ 接受 |
> | `C:\SOLIDWORKS Data`（裸空格） | ❌ 弹用法对话框 |
> | `"C:\SOLIDWORKS Data"`（加引号） | ❌ **一样弹** |
>
> 加引号**救不回来**，所以本项目默认值改用下划线：
> `toolbox_folder = "C:\SOLIDWORKS_Data"`。
>
> **另有一个更隐蔽的坑**：`/l*v` 与日志路径**必须是两个独立的命令行参数**。
> 合成一个（`"/l*v C:\path\x.log"`）会让 msiexec 认不出这个开关，
> 同样弹出用法对话框且**永不返回**（实测卡满 15 秒被强杀）。
> 这个坑比空格问题更危险，因为它连日志都不生成，排查时看不到任何线索。

---

## 三、语言代码表（官方）

官方手册给出了语言与 LCID 的对照：

| 语言 | LCID | 介质目录名 |
|---|---|---|
| 中文（繁体） | 1028 | `chinese` |
| **简体中文** | **2052** | **`chinese-simplified`** |
| 捷克文 | 1029 | `czech` |
| 英文 | 1033 | `english` |
| 法文 | 1036 | `french` |
| 德文 | 1031 | `german` |
| 意大利文 | 1040 | `italian` |
| 日文 | 1041 | `japanese` |
| 韩文 | 1042 | `korean` |
| 波兰文 | 1045 | `polish` |
| 巴西葡萄牙文 | 1046 | `portuguese-brazilian` |
| 俄文 | 1049 | `russian` |
| 西班牙文 | 1034 | `spanish` |
| 土耳其文 | 1055 | `turkish` |

> 表中的「介质目录名」是**实测**结果：SolidWorks 2024 SP5 Premium.DVD 的
> `swwi\lang\` 下正好是这 13 个目录，每目录内一个同名 MSI。
>
> 简体中文那个 MSI 的 Property 表实测为：
> `ProductName=SOLIDWORKS 2024 Chinese Simplified Resources`、
> `ProductLanguage=2052`、`ProductVersion=32.150.0048` —— 确认无误。

---

## 四、`ADDLOCAL` 与 Feature 名

### 本项目使用的两项（与官方手册一致）

```text
ADDLOCAL=SolidWorks,SolidWorksToolbox
```

手册第 36 页的官方模板原文：

```text
msiexec /i "administrative_image_directory\64bit\SOLIDWORKS\SOLIDWORKS.Msi"
 INSTALLDIR="C:\Program Files\your_folder"
 SOLIDWORKSSERIALNUMBER="xxxx xxxx xxxx xxxx xxxx xxxx"
 ENABLEPERFORMANCE=1 OFFICEOPTION=3
 ADDLOCAL=SolidWorks, SolidWorksToolbox
 TOOLBOXFOLDER="toolbox path for example C:\SolidWorks Data\"
 /qb
```

### 官方支持的 `ADDLOCAL` 取值（手册第 37 页，照录）

手册明确列出了 15 个可用于命令行部署的 Feature：

```text
CircuitWorks            Motion                  SolidWorksRoutedsystems
CoreSolidWorksTaskScheduler   ScanTo3D          SolidWorksToolbox
ExampleFiles            Simulation              SolidWorksUtilities
FeatureWorks            SolidWorksCosting       TolAnalyst
HelpFiles               SolidWorksDesignChecker
Manuals
```

并给出三条硬规则（手册原文）：

> - SOLIDWORKS **不支持**使用 `ADDSOURCE` 选项进行产品安装。
> - 属性**区分大小写**，**不能包含空格或破折号**，并且必须由**逗号**隔开。

> ⚠️ 注意：官方清单里**没有 `AddIns`**。
> 但实测 MSI 的 Feature 表显示 `SolidWorksToolbox` 的父级确实是 `AddIns`：

```text
SolidWorks                     ← SOLIDWORKS 2024 SP05（顶层）
  └── AddIns                   ← SOLIDWORKS Add-Ins
        ├── Motion
        ├── ScanTo3D
        ├── SolidWorksCosting
        ├── CircuitWorks
        ├── FeatureWorks
        ├── SolidWorksRoutedsystems
        ├── Simulation
        ├── SolidWorksToolbox  ← Toolbox
        ├── SolidWorksUtilities
        └── TolAnalyst
```

### 为什么 `AddIns` 不在官方清单里

实测 `AddIns` 的 **Attributes = 10（`0xA`）**：

| 位 | 值 | 含义 |
|---|---|---|
| `msidbFeatureAttributesUIDisallowAbsent` | `0x2` | 安装界面上**不允许取消**（不可选） |
| `msidbFeatureAttributesAdvertise` | `0x8` | 通告型 Feature |

**`UIDISALLOWED` 正是它不在官方 `ADDLOCAL` 清单里的原因** ——
那份清单只列用户能在安装界面里勾选的组件，而 `AddIns` 是纯父级容器。

Windows Installer 在安装子 Feature 时会**自动补装父级**，
所以 `ADDLOCAL=SolidWorks,SolidWorksToolbox` 会把 `AddIns` 一并装上。

**本项目采用官方两项写法**（经确认的决定），不额外列 `AddIns`。

### Feature 名区分大小写

真实名字是 `SolidWorks`（小写 w），不是 `SOLIDWORKS`。
写错会被 msiexec **静默忽略**，不报错。

### 自己核对的命令

```powershell
$msi = "<介质>\swwi\data\solidworks.msi"
$db  = New-Object -ComObject WindowsInstaller.Installer
$v   = $db.OpenDatabase($msi, 0).OpenView("SELECT Feature, Feature_Parent, Title FROM Feature")
$v.Execute()
while ($r = $v.Fetch()) { "{0}`t{1}`t{2}" -f $r.StringData(1), $r.StringData(2), $r.StringData(3) }
```

---

## 五、Windows 前置组件（**命令行安装不会自动装**）

这是最容易踩的坑，官方手册第 31–34 页「准备客户端以便通过命令行从管理映像进行安装」
专门用一整节讲它。原文照录：

> 创建管理映像后，在通知客户端之前，您**必须安装**无法通过使用命令行或
> Microsoft Active Directory 创建的管理映像来安装的 Microsoft Windows 组件。
>
> **所有 SOLIDWORKS 产品（并非只是核心 SOLIDWORKS 产品）均要求有
> Visual C++ 可重新分发软件包和 .NET Framework 4.8。**

平时双击 `setup.exe` 时，是安装管理器替我们装了这些；
**一旦改走纯 `msiexec` 命令行路线，这一环就必须自己补上。**

### 缺了会怎样

**不是"装不上"，而是"装上了跑不起来"。**

实测确认：`swwi\data\solidworks.msi` **没有 `LaunchCondition` 表**，
所以缺前置组件时 `msiexec` 依然会报安装成功 ——
直到用户启动 SOLIDWORKS、系统找不到 DLL 才暴露问题。

### 该装哪些（本套介质的真实清单）

以**介质自带的安装管理器清单** `sldim\sldim.xml` 为准。
实测该文件中 `PreReqs\` 下出现的目录即为本套介质的真实前置集合：

| 组件 | 介质路径 | 强制 |
|---|---|---|
| Visual C++ 2015-2022 可再发行 (x64) | `PreReqs\VCRedist17\VC_redist.x64.exe` | 是 |
| Visual C++ 2015-2022 可再发行 (x86) | `PreReqs\VCRedist17\VC_redist.x86.exe` | 是 |
| .NET Framework 4.8 | `PreReqs\dotNetFx\ndp48-x86-x64-allos-enu.exe` | 是 |
| Microsoft Edge WebView2 Runtime | `sldim\MicrosoftEdgeWebView2RuntimeInstallerX64.exe` | 是 |
| Visual Basic for Applications 7.1 | `PreReqs\VBA\vba71.msi` + `vba71_<LCID>.msi` | 可选 |
| SWCEF（Chromium Embedded Framework） | `swcef\CEF for SOLIDWORKS Applications.msi` | 是 |

### 静默安装参数

| 组件 | 参数 |
|---|---|
| `VC_redist.x64.exe` / `.x86.exe` | `/install /quiet /norestart` |
| `ndp48-x86-x64-allos-enu.exe` | `/q /norestart` |
| `MicrosoftEdgeWebView2RuntimeInstallerX64.exe` | `/silent /install` |
| `vba71.msi` / `CEF ...msi` | 走 `msiexec /i "<路径>" /qb /norestart` |

> `msiexec` 返回 **3010** 表示「成功但需重启」—— 这是可接受结果，不算失败。

### 关于 Visual C++ 2010（一处文档与介质不符）

手册第 32 页的通用前置表里写着需要「Visual C++ 2010 和 2022」，
但**这套介质里根本没有 2010 的安装包**（`PreReqs\` 下只有 `VCRedist17`，
已实测）。

本项目的处理：**不检查也不安装 VC++ 2010** —— 以介质实际提供的内容为准，
不去要求一个介质里根本不存在的东西。

### 本项目的实现

配置项：

```toml
[install]
install_prerequisites = true         # 检测缺失项并从介质 PreReqs\ 安装
prerequisite_timeout_minutes = 20    # 单个组件超时（.NET 包有 112 MB）
```

流程：**先检测（只读注册表），只有真的缺了才安装**，装完再复检一次。
检测位置：

| 组件 | 检测方式 |
|---|---|
| VC++ 2015-2022 | `Uninstall` 表里 `DisplayName` 含 `Visual C++` 且含 `x64`/`x86` |
| .NET Framework 4.8 | `HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full` 的 `Release` ≥ **528040** |
| WebView2 | `EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` 的 `pv` 存在 |
| VBA 7.1 | `Uninstall` 表里 `DisplayName` 含 `Visual Basic for Applications` 且含 `7.1` |

**本步骤不终止流程**：最坏情况是告警 + 让用户自己补，
强行中断会连"部分装好"的机会都丢掉。

---

## 六、介质路径（实测核对）

### DVD / ISO 布局（本项目实际使用）

```text
<介质根>\
  setup.exe                              ← GUI 引导程序
  swdata1.id / swdata2.id / swdata3.id
  sldim\
    startswinstall.exe                   ← 命令行安装入口
    sldIM.exe                            ← 图形化安装管理器
    sldadminoptioneditor.exe
  swwi\
    data\solidworks.msi                  ← 主程序（58.8 MB）
    data\swc.msi
    lang\chinese-simplified\chinese-simplified.msi   ← 简体中文语言包（11.3 MB）
    lang\<其它 12 种语言>\<同名>.msi
  PreReqs\
  apisdk\ cam\ eDrawings\ Flow Simulation\ Toolbox\ …   ← 组件目录
```

### 管理映像布局（官方文档的路径）

```text
administrative_image_directory\
  64bit\
    SOLIDWORKS\SOLIDWORKS.Msi
    SOLIDWORKS French\french.msi         ← 注意目录名是 "SOLIDWORKS <语言>"
    eDrawings\eDrawings.msi
    SWFileUtilities\SOLIDWORKS  File Utilities.msi
```

> 两种布局程序都会查（`swwi\lang\<名>\<名>.msi` 优先，
> 再退回 `64bit\SOLIDWORKS <名>\<名>.msi`）。

---

## 七、程序里对应的配置项

| 官方属性 | 配置项 | 默认值 | 何时传 |
|---|---|---|---|
| `INSTALLDIR` | `[install].install_drive` + `install_path` | `C` / `{drive}:\SW` | 总是 |
| `ADDLOCAL` | `[install].components_whitelist` | `SolidWorks,AddIns,SolidWorksToolbox` | 非空时 |
| `SOLIDWORKSSERIALNUMBER` | `[install].serial_number` | `""` | 非空时 |
| `TOOLBOXFOLDER` | `[install].toolbox_folder` | `C:\SOLIDWORKS_Data`（**不含空格**） | 白名单含 Toolbox 时 |
| `ENABLEPERFORMANCE` | —— （固定） | `0` | 总是 |
| —— | `[install].language_pack` | `chinese-simplified` | 作为**独立 msiexec** 调用 |
| —— | `[install].language_pack_delay_minutes` | `0`（兼容字段） | 保留旧配置；现在改为主程序进程退出后立即启动语言包 |
| —— | `[install].install_switches` | `[]` | 追加到 `startswinstall.exe` |
| —— | `[install].install_prerequisites` | `true` | 检测并从介质装 Windows 前置组件 |
| —— | `[install].prerequisite_timeout_minutes` | `20` | 单个前置组件安装超时 |

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CONFIG_GUIDE.md`](./CONFIG_GUIDE.md) | **我需要填什么** —— 每个键是否必填、填什么、示例配置 |
| [`CONFIG.md`](./CONFIG.md) | 每个配置项的类型 / 默认值 / 取值范围 / 内部语义 |
| [`CUSTOMIZE.md`](./CUSTOMIZE.md) | 部署前必须改什么、已知局限 |
| [`INSTALL_FLOW.md`](./INSTALL_FLOW.md) | 13 阶段流程详解 |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | 按症状排查 |
