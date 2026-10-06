# 部署前必须自行修改的内容

本程序是**通用编排框架**，不附带任何安装介质、下载地址或序列号。
下面列出**必须**或**强烈建议**在部署前改掉的内容，以及每项的修改位置。

> 原则：所有可调项都在 `config.toml`，**不需要改代码**。
> 只有「打包 7z」「商标/标识符」这两类才需要动源码或构建配置。

---

## 一、必须修改（不改就没有意义）

### 1. 下载地址

默认值是占位域名 `example.com`，**一定下载不到东西**。

| 位置 | 键 | 说明 |
|---|---|---|
| 默认配置 | `[download].url` | 单包下载地址 |
| 默认配置 | `[download].multipart_urls` | 分片下载地址（每片一个，留空则用 `url`） |

**改哪里**：运行后打开设置页 → `02 下载` 分组 → 修改 → 保存。
或直接编辑 `{app_data_dir}\config.toml`。

```toml
# 单包
[download]
url = "https://你的服务器/solidworks2024sp5.7z"

# 或分片（7z / NanaZip 用 a -v 切出来的分卷）
[download]
multipart_urls = [
  "https://你的服务器/solidworks2024sp5.7z.001",
  "https://你的服务器/solidworks2024sp5.7z.002",
  "https://你的服务器/solidworks2024sp5.7z.003",
]
multipart_concat = false
```

**如果想改「内置默认值」**（让程序首次启动就是这个地址），编辑
`src-tauri/resources/default-config.toml` 里 `[download]` 的 `url`，
它通过 `include_str!` 编译进二进制。

---

### 2. 远程配置地址

`[remote_config].url` 默认同样是 `example.com`。

**改哪里**：设置页 → `03 远程配置` → 修改 → 保存。
这是「拉取远程配置」按钮所使用的地址。

---

### 3. 安装目标

| 键 | 默认 | 说明 |
|---|---|---|
| `[install].install_drive` | `"C"` | 目标盘符 |
| `[install].install_path` | `""` | 留空则解析为 `{install_drive}:\SW` |

**为什么不建议留在 C 盘**：完整安装约占 20 GB 以上，加上临时解压产物
（见 `[workdir]`）可能再占 60 GB。建议 `install_drive = "D"` 并把
`[workdir].temp_dir` 指到另一个空间充足的磁盘。

---

## 二、强烈建议检查

### 4. 7z 解压程序

程序会自动查找（配置 → 打包资源 → 下载缓存 → 已安装的 7-Zip → 第三方实现 → PATH），
都找不到时默认从**官方源**自动下载：

```toml
[sevenzip]
auto_download = true
download_url = "https://github.com/ip7z/7zip/releases/latest/download/7zr.exe"
```

| 你的环境 | 要做什么 |
|---|---|
| 能访问 GitHub | **不用改**，首次运行自动下载并缓存 |
| 内网 / 离线 | 把 `download_url` 换成本地文件服务器地址；或把 `auto_download` 设为 `false` 并改用下面任一方式 |
| 机器上已装 7-Zip / NanaZip | **不用改**，程序会自动检测到（含 `NanaZipC.exe`） |
| 想随程序分发 | 把 `7z.exe`（或 `7zr.exe`）放到 `src-tauri/resources/`，并在 `tauri.conf.json` 的 `bundle.resources` 里加上它 |

```toml
# 内网镜像示例
[sevenzip]
auto_download = true
download_url = "http://intranet.mirror.local/tools/7zr.exe"
```

> **完全离线**请务必设 `auto_download = false`，
> 否则每次启动都会先等一次注定失败的网络请求。

---

### 5. 进程匹配关键词

```toml
[process]
kill_pattern = ".*solidwork.*|.*sldworks.*|.*sw_dn.*|.*sidworks.*|.*flexnet.*|.*lmgrd.*"
match_command_line = true
```

| 键 | 说明 |
|---|---|
| `kill_pattern` | **不区分大小写**，首尾空白自动忽略。写纯关键词即可（`solidworks` 等价于 `.*solidworks.*`），`\|` 分隔多个。**空值会被拒绝**，避免误杀全部进程 |
| `match_command_line` | 为 `true` 时同时匹配进程的**完整路径与命令行**，能按关键词找到"名字无关"的进程（如 `java.exe -jar ...\solidworks-helper.jar`）；代价是每轮要查询进程元数据，**明显更慢** |

**按需调整**：

- 只关心进程名 → `match_command_line = false`（快很多）
- 你的补丁程序有别的名字 → 往 `kill_pattern` 里再加关键词
- 担心误杀同名进程 → 把关键词写具体些，例如
  `sldworks\.exe$|sw_dn.*\.exe$`

日志会写明每个进程是「按进程名」还是「按路径/命令行」命中，便于核对。

---

### 6. 网络隔离

```toml
[network]
disable_network = true
restore_network_after = true
```

- `disable_network = false`：如果安装过程需要联网（在线许可校验、下载组件），
  必须关掉，否则安装会卡住。
- **不要**把 `restore_network_after` 设为 `false`，除非你打算手动恢复网络 ——
  那样程序退出后网卡会一直处于禁用状态。

---

### 7. 安全软件（**很容易被忽略，但会直接导致安装失败**）

Windows Defender 与多数第三方杀软会把 `_SolidSQUAD_` 里的补丁文件判为威胁
并**直接删除**，安装要到第 11 步「文件替换」才发现源文件没了。
所以编排在**解压之前**先处置杀软。

```toml
[antivirus]
enabled = true
detect = true
remove_defender = true
defender_tool_path = ""              # 留空自动查找
warn_reboot_after_removal = true
third_party_mode = "terminate"
settle_minutes = 0.5
command_timeout_minutes = 5
```

#### 检测方式

用 **`root\SecurityCenter2`** 的 `AntiVirusProduct` —— 这是 Windows 安全中心
自己用的注册表，任何在系统里"登记过"的杀软都会出现在这里，
比枚举服务或猜进程名可靠得多。界面会列出**每个**检测到的软件及其状态
（实时防护开/关、内置还是第三方）。查不到时会如实提示"未能读出"，
而不是假装机器上干净。

#### Defender 的移除

复用随仓库提供的 [`othertools/windows-defender-remover-main`](../othertools/windows-defender-remover-main)
（LGPL，保留其 `LICENSE`）。执行的是与该项目 `Script_Run.ps1` 的 `y` 分支
**相同的动作**：

1. `PowerRun.exe powershell ... RemoveSecHealthApp.ps1` —— 移除 Windows 安全中心应用
2. `PowerRun.exe regedit /s <每个 .reg>` —— 导入 `Remove_Defender/` 与 `Remove_SecurityComp/` 的全部策略项
3. 删除 SmartScreen 相关文件

> **刻意不执行它最后的 `shutdown /r /f /t 10`。**
> 那一步会强制重启机器并直接打断安装流程。本程序**只提示**需要重启，
> 时机由你决定 —— 见下方「关于重启」。

**工具目录要求**：必须同时含 `PowerRun.exe` 与 `RemoveSecHealthApp.ps1`，
否则视为指错位置。留空时按顺序查找：

| # | 位置 |
|---|---|
| 1 | `antivirus.defender_tool_path` |
| 2 | exe 同级的 `othertools\windows-defender-remover-main\script` |
| 3 | Tauri 资源目录下的同名路径 |
| 4 | 源码目录（开发期） |

打包分发时，要么把整个 `othertools\windows-defender-remover-main` 放进
`bundle.resources`，要么在 `defender_tool_path` 里写绝对路径。
**找不到工具不会让安装失败**，只会在日志与界面上明确报出"无法移除 Defender"。

#### 第三方杀软

按 `third_party_mode` 逐级加强：

| 取值 | 行为 | 适用 |
|---|---|---|
| `prompt` | 只检测 + 界面提示你手动从托盘退出 | 不想让程序碰别人的软件 |
| **`terminate`**（默认） | 额外终止其进程 / 停止其服务 | 大多数情况 |
| `uninstall` | 再额外按其 `UninstallString` 静默卸载 | 确认要卸载时 |

**默认是 `terminate` 而非 `uninstall`** —— 静默卸载别人的安全软件风险太高，
不该是默认行为。即使选了 `uninstall`，个人版杀软普遍会弹交互界面或有自保护，
**失败是预期结果**，程序会如实告诉你。

**判定标准是"是否仍注册在 SecurityCenter2"**，不是"进程是否被杀掉" ——
进程可能被自保护立刻拉起来，所以以系统注册状态为准。只有它确实不再注册为
活动防护，才认为处置成功。

#### 无法自动停用时会发生什么

这时**不会假装成功**。部署页会出现高亮的
**「需要你手动退出安全软件」** 面板：

1. 按名字列出每一个没能停用的软件
2. 给出 4 步托盘操作指引（含"图标可能藏在 `^` 里"这种实际细节）
3. 给出该软件**自己的** `manual_hint`，而不是笼统的"请关闭杀软"
4. 提供「**重新检测**」按钮 —— 你退出后点一下确认，列表实时更新

日志里同样留痕：

```
安全软件处置：需要你手动退出：某杀毒软件
某杀毒软件：未能终止其进程或服务（仍在 SecurityCenter2 中注册为活动防护）
```

#### 关于重启

移除 Defender 后，策略注册表与安全中心应用已经处理完，但**驱动与服务注册
要重启后才会消失**。程序的处理是：

- 在部署页显示明确的「**需要重启才彻底生效**」提示
- 说明**建议先让本次安装继续**（Defender 已经不会再删文件），安装结束后再重启
- **不会替你重启**，也不因此中断流程

安装结束后可运行工具自带的 `script\verify.bat` 确认移除是否彻底。

#### 想完全不碰杀软

```toml
[antivirus]
enabled = false
```

此时请**手动**把工作目录与安装目录加进杀软的排除项，否则解压出的补丁文件
仍可能被删除。

---

### 8. 磁盘与超时（按机器性能调整）

| 场景 | 要改的项 |
|---|---|
| 慢速机械盘 / 大镜像 | 调大 `[sevenzip].extract_timeout_minutes`（默认 30）与 `[install.polling].timeout_minutes`（默认 240） |
| 想保留现场排查 | `[workdir].cleanup_temp_after = false` |
| 网络不稳定 | 调小 `[download].threads`（如 8）、调大 `retry_count` 与 `read_timeout_minutes` |
| 有官方哈希值 | 填 `[download].checksum_sha256`（**分片模式下不生效**） |

---

## 三、仅打包分发时需要修改

这些是**构建配置**，不改也能本地开发运行，但要发布就得改。

### 9. 应用标识符（identifier）

`src-tauri/tauri.conf.json`：

```json
"identifier": "com.solidworks.autoinstaller"
```

**必须改成你自己的反向域名**（例如 `com.yourcompany.swinstaller`）。
它同时决定用户配置的落盘位置：

```
%APPDATA%\<identifier>\config.toml
```

> **改 identifier 会换掉配置目录**：旧配置不会自动迁移，
> 用户需要把旧 `config.toml` 复制到新目录。

### 10. 应用名称与图标

| 项 | 位置 |
|---|---|
| 窗口标题 | `tauri.conf.json` → `app.windows[0].title` |
| 产品名 | `tauri.conf.json` → `productName` |
| 图标 | `src-tauri/icons/`，重新生成：`python src-tauri/icons/generate_icons.py` |

当前图标是**程序化生成的原创几何标记**（不是 SolidWorks 商标），
可直接使用；若要换成自己的品牌标识，替换 `icons/` 下的文件即可
（`icon.ico` 必须存在，否则 `tauri build` 会报错）。

### 11. 打包开关

`tauri.conf.json` → `bundle.active` 默认是 `false`：

```json
"bundle": { "active": true }
```

改成 `true` 后 `npm run tauri:build` 才会产出安装包。

### 12. 打包 7z（如果要随程序分发）

两处一起改，缺一个就会打包失败：

1. 把 `7z.exe` / `7zr.exe` 放进 `src-tauri/resources/`
2. `tauri.conf.json` → `bundle.resources` 加上它：

```json
"resources": ["resources/default-config.toml", "resources/7zr.exe"]
```

许可注意事项见 [`../src-tauri/resources/README.md`](../src-tauri/resources/README.md)。

---

## 四、不需要修改（说明）

| 内容 | 为什么不用改 |
|---|---|
| 界面设计令牌 | `src/styles/tokens.css` 已按 ark-ui endfield 规范定稿 |
| 13 步流程编排 | 由配置驱动，无需改代码 |
| 弹窗处理规则的关键词 | 内置三类（重启 / 不能核实服务器 / 端口@服务器）；**只有出现新的对话框类型**才需要在 `win32_utils.rs` 的 `DEFAULT_POPUP_RULES` 里加关键词并重新编译 |
| 解压命令 | 不传 `-t` 是刻意的：7z 按文件头识别，扩展名无关 |

---

## 五、已知局限

诚实列出当前实现的边界，避免踩坑。

| 局限 | 影响 | 规避方式 |
|---|---|---|
| **独立成行的注释不会被保留** | `toml::to_string` 会重排 section（按字母序），导致独立行的注释在合并时错位到别的 section，最终丢失。**行内注释（`key = value  # 注释`）正常保留**，默认配置的注释都是行内形式 | 把重要说明写成行内注释 |
| 保存/导出时"表单值优先" | 设置页表单包含全部字段，点「保存配置」会把表单值写入 —— 包括你没动过的项。**导出配置**用的是"留空保留"模式，留空项保持磁盘值 | 要保留磁盘值用「导出当前配置」；要落盘生效用「保存配置」 |
| `multipart_urls` 与 `checksum_sha256` 不共存 | 分片模式下单包 URL 与整包哈希都不参与（每片独立校验没有意义） | 分片模式下依赖 HTTPS + 源站完整性 |
| 非标准切分需要手动开启拼接 | 若分片不是 7z 的 `-v` 分卷，7z 会报"无法作为压缩包打开" | 把 `multipart_concat` 设为 `true` |
| 目录遍历深度上限 24 层 | 补丁包埋在更深的层级里会找不到 | 把内容往上挪一层 |
| 进程命令行匹配较慢 | 开启 `match_command_line` 后每轮要查询进程元数据 | 只关心进程名时设为 `false` |
| NanaZip 依赖其 CLI 别名 | 只用 `NanaZipC.exe`（AppExecutionAlias）。若某个 NanaZip 版本不提供该别名，程序会跳过并继续找其他实现 | 在 `[sevenzip].exe_path` 里显式指向可用的 7z 兼容程序 |
| **移除 Defender 必须重启** | 策略与安全中心应用已处理，但驱动/服务注册要重启才消失。程序不替你重启，所以**重启前 Defender 可能仍显示为存在** | 安装结束后重启，再运行工具的 `script\verify.bat` 确认 |
| **第三方杀软常常停不掉** | 自保护会立刻重启进程或拒绝访问。判定以 SecurityCenter2 注册状态为准，因此会如实报告"未能停用"而不是假装成功 | 按部署页指引手动从托盘退出，再点「重新检测」 |
| Defender 移除依赖外部工具 | 找不到 `PowerRun.exe` + `RemoveSecHealthApp.ps1` 时无法移除，只在界面与日志报错（不阻断安装） | 保留 `othertools/windows-defender-remover-main` 目录，或设置 `defender_tool_path` |
| 杀软检测依赖 WMI | 若 `root\SecurityCenter2` 被策略禁用，会查不到任何软件（程序会明说"未能读出"，不会假装干净） | 手动确认杀软已退出，或关掉 `[antivirus].detect` 自行把控 |

---

## 六、快速自检清单

部署到目标机器前，逐项确认：

- [ ] `[download].url` 或 `[download].multipart_urls` 填了**真实可用**的地址
- [ ] `[remote_config].url` 不再是 `example.com`（或 `auto_fetch_on_start` 保持 `false`）
- [ ] `[install].install_drive` / `install_path` 指向空间充足的磁盘
- [ ] 离线环境：`[sevenzip].auto_download = false`，且 7z 已就位
- [ ] `[process].kill_pattern` 覆盖你的补丁程序名字
- [ ] **`[antivirus]` 的配置符合预期**：默认会移除 Defender、并尝试终止第三方杀软的进程；
      确认你接受这个行为，否则设 `enabled = false` 并自行加排除项
- [ ] 机器上装了第三方杀软时，准备好手动从托盘退出（程序会提示，但停不掉就得你来）
- [ ] 若安装需要联网：`[network].disable_network = false`
- [ ] 目标机器**计算机名与用户名都是纯 ASCII**（否则第 1 步就会失败）
- [ ] 以**管理员身份**运行
- [ ] 首次部署先设 `[workdir].cleanup_temp_after = false`，确认流程走得通后再改回 `true`

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CONFIG_GUIDE.md`](./CONFIG_GUIDE.md) | **我需要填什么** —— 是否必填、填什么、示例配置 |
| [`COMMAND_LINE.md`](./COMMAND_LINE.md) | 官方命令行属性参考：属性名、语言代码表、ADDLOCAL 层级、介质路径 |
| [`CONFIG.md`](./CONFIG.md) | 每个配置项的类型 / 默认值 / 取值范围 / 作用 / 示例 |
| [`INSTALL_FLOW.md`](./INSTALL_FLOW.md) | 13 阶段详解、弹窗规则、轮询逻辑、恢复策略 |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | 按症状排查 |
| [`DEVELOPMENT.md`](./DEVELOPMENT.md) | 二次开发与构建 |
