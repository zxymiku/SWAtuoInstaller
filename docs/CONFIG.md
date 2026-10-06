# 配置文件详解

本程序的所有可调参数集中在单个 TOML 文件中。本文件说明加载优先级、
每个配置项的类型与取值范围、远程拉取与重置机制，并附完整的空白默认配置。

> **时间单位约定**：除 `[install.popup].scan_interval_ms` 外，**所有时间参数单位均为分钟**，
> 类型为 float。`0.5` 表示 30 秒，`240` 表示 4 小时。

---

## 目录

- [文件位置与加载优先级](#文件位置与加载优先级)
- [结构保留合并机制](#结构保留合并机制)
- [配置项详解](#配置项详解)
- [远程配置拉取机制](#远程配置拉取机制)
- [重置为默认配置](#重置为默认配置)
- [完整空白默认配置文件](#完整空白默认配置文件)
- [常见配置场景](#常见配置场景)

---

## 文件位置与加载优先级

### 三层来源

| 优先级 | 来源 | 路径 | 说明 |
|---|---|---|---|
| 低 | **嵌入式默认配置** | 编译进二进制（`include_str!("../resources/default-config.toml")`） | 合并基线，也是「重置默认」的写入源 |
| 中 | **用户配置文件** | `{app_data_dir}/config.toml` | 用户可见可编辑的持久配置 |
| 高 | **前端传入的配置** | `start_install` / `save_config` 的参数 | 主页点「开始自动部署」时以表单值为准 |

### `{app_data_dir}` 的实际位置

```
C:\Users\<用户名>\AppData\Roaming\com.solidworks.autoinstaller\config.toml
```

Windows 上 `app_data_dir` 由 Tauri 解析为 `%APPDATA%\<identifier>`，
其中 `identifier` 来自 `src-tauri/tauri.conf.json` 的 `com.solidworks.autoinstaller`。

同目录下还会生成：

| 路径 | 内容 |
|---|---|
| `logs\` | 从日志页导出的日志文件（`solidworks-install-YYYYMMDD-HHMMSS.log`） |
| `cache\` | 下载模式缓存的压缩包（及其 `.part` / `.state` 断点文件） |
| `work\` | `[workdir].temp_dir` 留空时的默认工作目录 |

### 加载流程

```
启动
 └─ 用户配置不存在？
     ├─ 是 → 把嵌入式默认配置写入 {app_data_dir}/config.toml
     └─ 否 → 读取用户配置文本
 └─ toml_edit 结构保留合并（默认配置 ← 用户配置）
     ├─ 用户已有的值 / 注释 → 原样保留
     └─ 用户缺失的字段     → 从默认配置补齐（含默认配置里的行内注释）
 └─ serde 反序列化为 Config 结构体
 └─ normalize() 归一化（钳制线程数、清理盘符、去空白、修正非法枚举值）
 └─ 若合并结果与磁盘内容不同 → 回写磁盘（下次启动即为完整文件）
```

前端「开始自动部署」时，会把当前表单配置再次 `normalize()` 并落盘，
保证**本次运行**与**下次启动**读到的配置完全一致。

---

## 结构保留合并机制

合并由 `src-tauri/src/config.rs` 的 `merge_toml()` 实现，规则如下：

| 情形 | 行为 |
|---|---|
| 两边都是表 | **递归合并**，保留基线（默认配置）的键顺序 |
| 两边都是值/数组 | 用户值胜出；**保留基线的行内注释** |
| 用户缺失该键 | 从基线补齐，连同行内注释一起带入 |
| 用户多出该键 | 保留（不报错，多余字段在反序列化时被忽略） |

**实际效果**

假设用户文件只有两行：

```toml
[download]
threads = 32
```

程序升级后默认配置新增了 `[sevenzip]` 分组。保存后用户文件变成：

```toml
[download]
url = "https://example.com/solidworks2024sp5.7z"
threads = 32                         # 下载线程数，范围 1~255
...
[sevenzip]
exe_path = ""                        # 7z.exe 路径，留空则使用内置
extract_timeout_minutes = 30         # 单次解压超时
```

`threads = 32` 被保留，注释被带回，缺失字段被补齐。

### 归一化（normalize）规则

无论配置来自何处，执行前都会经过归一化：

| 字段 | 归一化行为 |
|---|---|
| `download.threads` | 钳制到 `1~255` |
| 所有 float 时间字段 | 负数 / NaN → 归零或最小值（各字段下限不同） |
| `install.popup.scan_interval_ms` | 钳制到 `50~60000` |
| `install.install_drive` | 去 `:` 与 `\`，转大写；空则回退 `"C"` |
| `download.checksum_sha256` | 去空白 + 转小写 |
| `install.components_whitelist` | 逐项去空白，丢弃空字符串 |
| `network.adapter_disable_method` | 非 `powershell` 一律归一为 `netsh` |
| `general.language` / `theme` | 空则回填默认值 |

---

## 配置项详解

### `[general]`

界面相关的顶层设置。

#### `general.language`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"zh-CN"` |
| 取值范围 | `"zh-CN"` / `"en-US"` |
| 作用 | 界面语言。当前所有界面文案均为中文，该字段保留用于后续 i18n 接入 |
| 示例 | `language = "zh-CN"` |

#### `general.theme`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"endfield"` |
| 取值范围 | `"endfield"` |
| 作用 | 界面风格族。当前实现锁定 endfield（米白 / 炭黑 / 信号黄）；其他值会被回填为 `endfield` |
| 示例 | `theme = "endfield"` |

---

### `[download]`

下载模式的全部参数。本地模式不使用本分组。

#### `download.url`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"https://example.com/solidworks2024sp5.7z"` |
| 取值范围 | HTTP/HTTPS 直链；留空表示不使用下载模式 |
| 作用 | 压缩包地址。文件名从 URL 末段推导（自动剥离 `?query`），缓存到 `{app_data_dir}/cache/` |
| 示例 | `url = "https://mirror.example.com/sw2024sp5/solidworks2024sp5.7z"` |

**注意**：URL 末段必须包含扩展名，否则缓存文件名回退为 `solidworks2024sp5.7z`。
服务端需支持 `Range` 请求才能启用多线程与断点续传；不支持时会自动降级为单连接流式下载并给出警告。

**扩展名无关性**：URL 末段只是缓存文件的**初始**命名依据。下载完成后程序按
**文件头**识别真实格式并补上正确后缀，因此 `.../download.php`、`.../archive`（无扩展名）
这类链接都能正常工作：

| URL 末段 | 嗅探结果 | 最终缓存文件名 |
|---|---|---|
| `solidworks2024sp5.7z` | `7z` | `solidworks2024sp5.7z` |
| `download.php` | `zip` | `download.php.zip` |
| `archive` | `rar` | `archive.rar` |
| `get?id=123` | `7z` | `get.7z` |

解压由 7z 按文件头判定，**与扩展名完全无关**；嗅探结果只用于日志与缓存命名。
复用已下载文件时会同时接受「精确名」与「同基名 + 任意已知后缀」。

#### `download.multipart_urls`

| 项 | 值 |
|---|---|
| 类型 | string[] |
| 默认值 | `[]` |
| 取值范围 | 每项一个 HTTP/HTTPS 直链，数量不限 |
| 作用 | **分片下载**。非空时进入分片模式，`download.url` 与 `checksum_sha256` 都不参与 |
| 示例 | 见下方 |

```toml
[download]
multipart_urls = [
  "https://host/sw2024.7z.001",
  "https://host/sw2024.7z.002",
  "https://host/sw2024.7z.003",
]
multipart_concat = false
```

**分片是怎么处理的（关键）**：

7z / NanaZip 用 `a -v100m` 产出的是**分卷**：`x.7z.001`、`x.7z.002`…
7z 只要拿到 `.001` 就会**自动读取同目录的整套分卷**。因此程序：

1. 逐个下载每个 URL 到 `{app_data_dir}\cache\`；
2. **原样保留每个分片的文件名**（`.001` 这类命名不会被追加后缀 —— 改了名字
   7z 就找不到其余分卷）；
3. 把 `.001` 交给 7z，由它读完整套。

> **不要拼接**。拼成一个大文件后 7z 反而不认识它。

**归一化**：下载前会去掉每项的首尾空白与包裹的引号、丢弃空行，
因此从文档里直接粘贴（常带缩进或引号）也能用。

#### `download.multipart_concat`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `false` |
| 作用 | `true` 时把各分片**按顺序拼接**成单个文件后再解压 |

**只在一种情况下需要设为 `true`**：分片不是 7z 分卷，而是"把一个完整文件
任意切成几段"（例如某些网盘的分段下载）。这类文件 7z 无法直接识别，
会报「无法作为压缩包打开」。

判断方法：如果分片名是 `.001/.002/...` 且在 7z/NanaZip 里用 `a -v` 生成，
保持 `false`；否则试 `true`。

#### `download.threads`

| 项 | 值 |
|---|---|
| 类型 | integer |
| 默认值 | `16` |
| 取值范围 | `1 ~ 255`（超出被钳制） |
| 作用 | 分块下载的 worker 数量。分块大小 = 总大小 ÷ 线程数，且钳制在 **1~8 MiB** 之间 |
| 示例 | `threads = 32` |

**调优建议**：家用宽带 8~16；千兆 / 镜像站 32~64；超过 64 通常不再提升速度，
反而增加服务端压力与重试概率。

#### `download.checksum_sha256`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `""`（空 → 跳过校验） |
| 取值范围 | 64 位十六进制字符 |
| 作用 | 下载完成后读取整个文件计算 SHA-256 并比对。**校验失败会删除 `.part` 与 `.state` 文件**，避免损坏数据进入续传 |
| 示例 | `checksum_sha256 = "9f2c8b1e...（64 位）"` |

**附带行为**：填写校验值后，已存在的缓存文件不会直接复用，每次都会重新校验；
留空时若缓存文件存在则直接复用（跳过下载）。

#### `download.retry_count`

| 项 | 值 |
|---|---|
| 类型 | integer |
| 默认值 | `3` |
| 取值范围 | `0 ~ 20`（更大值不报错，但退避上限固定 30 分钟） |
| 作用 | 单个分块的最大重试次数。超过后该分块记为失败，进入「补齐轮次」串行重试 |
| 示例 | `retry_count = 5` |

#### `download.retry_backoff_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `0.5`（30 秒） |
| 取值范围 | `≥ 0`；实际等待 = `base × 2^(n-1)`，**上限 30 分钟** |
| 作用 | 指数退避的基数。第 1 次失败等 `base`，第 2 次等 `2×base`，第 3 次等 `4×base`… |
| 示例 | `retry_backoff_minutes = 1.0` → 1 分、2 分、4 分… |

#### `download.connect_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `1` |
| 取值范围 | `≥ 0.1` |
| 作用 | TCP 建连 + TLS 握手超时，同时作为远程配置拉取与下载客户端的基础超时 |
| 示例 | `connect_timeout_minutes = 0.5` |

#### `download.read_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `5` |
| 取值范围 | `≥ 0.1` |
| 作用 | reqwest 客户端的整体请求超时。分块下载每个分块独立计时 |
| 示例 | `read_timeout_minutes = 10` |

**注意**：该值过小会让大分块在慢速链路上被反复掐断。若使用 `threads = 1`
且分块达 8 MiB，建议至少 `5`。

---

### `[remote_config]`

「拉取远程配置」按钮与启动自动拉取的行为。

#### `remote_config.url`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"https://example.com/config.toml"` |
| 取值范围 | HTTP/HTTPS 直链，指向 UTF-8 编码的 TOML |
| 作用 | 设置页「拉取远程配置」按钮直接使用该地址，无需每次手输 |
| 示例 | `url = "https://gist.example.com/raw/sw-config.toml"` |

#### `remote_config.auto_fetch_on_start`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `false` |
| 作用 | 启动时自动拉取该 URL。开启后界面在首次 `get_config` 完成后立即请求一次远程配置 |
| 行为 | 与手动点击「拉取远程配置」完全一致：**只填表、不落盘**；失败仅提示，不阻塞界面加载 |

> **注意**：该开关配合 `url` 非空才会生效。若 `url` 为空则不发起任何请求。
> 离线环境下请保持 `false`，否则每次启动都会等待一个必然失败的请求
> （超时上限由 `fetch_timeout_minutes` 控制）。

**示例**

```toml
[remote_config]
url = "https://intranet.example.com/sw-config.toml"
auto_fetch_on_start = true
fetch_timeout_minutes = 0.5     # 离线环境也建议设小，避免启动等待
```

#### `remote_config.fetch_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `1` |
| 取值范围 | `≥ 0.1` |
| 作用 | 单次远程配置请求的总超时 |
| 示例 | `fetch_timeout_minutes = 0.5` |

---

### `[install]`

安装目标与组件选择。

#### `install.install_drive`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"C"` |
| 取值范围 | 单个盘符字母（`C` / `D` / `E`…），可带或不带 `:` |
| 作用 | `install_path` 留空时的目标盘 |
| 示例 | `install_drive = "D"` |

#### `install.install_path`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `""` |
| 取值范围 | 绝对路径；留空则解析为 `{install_drive}:\SW` |
| 作用 | 安装根目录。传给 `msiexec` 的 `INSTALLDIR`；文件替换把补丁 `SOLIDWORKS Corp` 下的各组件目录合并到此目录 |
| 示例 | `install_path = "D:\\Apps\\SOLIDWORKS"` |

**解析后的实际使用位置**

| 用途 | 路径 |
|---|---|
| `INSTALLDIR` | `resolved_install_path()` |
| 第 11 步替换目标 | `{resolved_install_path}`（按组件目录名逐项合并） |
| 第 9 步文件检测 | `<install_path>\SOLIDWORKS\SLDWORKS.exe`（注意是 SLDWORKS） |

在 TOML 中写 Windows 路径请使用**转义反斜杠**（`"D:\\SW"`）或**单引号字面量**（`'D:\SW'`）。

#### `install.components_whitelist`

| 项 | 值 |
|---|---|
| 类型 | string[] |
| 默认值 | `[]`（空数组 = 安装全部组件） |
| 取值范围 | 任意组件 ID 列表 |
| 作用 | 非空时映射为 `msiexec` 的 `ADDLOCAL=组件1,组件2,...` |
| 示例 | `components_whitelist = ["SOLIDWORKS", "Simulation"]` |

**注意**：该参数只作用于 `msiexec` 回退路径。首选路径
`sldim\startswinstall.exe /install /now` 由 SolidWorks 安装管理器自行决定组件集合。

#### `install.serial_number`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `""`（依赖 `_SolidSQUAD_` 中的 `.reg` 文件） |
| 取值范围 | 合法序列号格式 |
| 作用 | 非空时作为 `msiexec` 的 **`SOLIDWORKSSERIALNUMBER`** 参数（官方属性名，**不是** `SERIALNUMBER`） |
| 示例 | `serial_number = "0000 0000 0000 0000 0000 0000 0000"` |

#### `install.language_pack`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"chinese-simplified"` |
| 取值范围 | **介质 `swwi\lang\` 下的目录名**：`chinese`（繁体 1028）/ `chinese-simplified`（简体 2052）/ `czech` / `french` / `german` / `italian` / `japanese` / `korean` / `polish` / `portuguese-brazilian` / `russian` / `spanish` / `turkish`；留空则不装语言包 |
| 作用 | 语言包标识，随安装参数一并传递 |
| 示例 | `language_pack = "chinese-simplified"` |

---

### `[install.polling]`

第 9 步「轮询检测安装完成」的判定参数。

#### `install.polling.poll_interval_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `0.5`（30 秒） |
| 取值范围 | `≥ 0.01`；实际睡眠被限制在 **0.2~5 秒**之间 |
| 作用 | 检测循环的间隔。三项检查（日志/进程/文件）每次循环都执行 |
| 示例 | `poll_interval_minutes = 1` |

**为什么实际睡眠有下限**：三项检查会枚举进程与 `glob` 日志文件，过于频繁
会显著占用 CPU。上限 5 秒则保证取消操作能及时响应。

#### `install.polling.timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `240`（4 小时） |
| 取值范围 | `≥ 0.1` |
| 作用 | 轮询总预算。超时即判定失败，并在错误信息中列出三项检查的实际状态 |
| 示例 | `timeout_minutes = 360` |

#### `install.polling.log_check_enabled`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 扫描安装日志尾部 256 KiB，查找 `安装成功` / `Installation succeeded` 等成功标记。日志按 GBK(936) 优先解码，避免中文乱码 |
| 搜索位置 | `{workdir}` / `{install_path}` / `{install_path}\SOLIDWORKS` / `%TEMP%` / `%LOCALAPPDATA%\Temp` / `%LOCALAPPDATA%\SOLIDWORKS` |

#### `install.polling.process_check_enabled`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 检查 `startswinstall.exe` / `setup.exe` / `msiexec.exe` / `sldworkssetup.exe` / `swinstallmanager` 是否全部退出 |

#### `install.polling.file_check_enabled`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 检查 `<install_path>\SOLIDWORKS\SLDWORKS.exe` 是否已生成（按候选列表逐个试） |

#### 判定规则

**满足的检查项数量 ≥ 2** 即判定完成；若只启用了一项，该一项即代表全部结论。

```
日志 ✓ + 进程 ✓ + 文件 ✗  →  2/3  →  完成
进程 ✓ + 文件 ✗ + 日志 ✗  →  1/3  →  继续等待
文件 ✓ + 日志 ✓ + 进程 ✗  →  2/3  →  完成
```

---

### `[install.popup]`

第 8 步弹窗守护线程的参数。

#### `install.popup.scan_interval_ms`

| 项 | 值 |
|---|---|
| 类型 | integer |
| 单位 | **毫秒**（本文件唯一的毫秒参数） |
| 默认值 | `500` |
| 取值范围 | `50 ~ 60000`（超出被钳制） |
| 作用 | `EnumWindows` 扫描顶层窗口的间隔。每次命中只处理一个弹窗 |
| 示例 | `scan_interval_ms = 250` |

#### `install.popup.auto_click_confirm`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 是否自动点击「确定」。**同时控制「端口@服务器」弹窗的确认**（该弹窗会先写入 `25734@localhost` 再点确定） |

#### `install.popup.auto_click_yes`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 是否自动点击「是」。覆盖「不能核实此服务器存在」弹窗 |

#### `install.popup.auto_click_no`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `false` |
| 作用 | 是否自动点击「否」。默认关闭，避免误拒必要操作 |

#### `install.popup.popup_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `5` |
| 取值范围 | `≥ 0.1` |
| 作用 | 同标题弹窗的去重窗口。同一标题在该时间窗内只处理一次，避免与用户手动操作互相打架 |

**禁用语义**：被设为 `false` 的动作**既不点击也不产生事件**——该弹窗会被静默跳过。

---

### `[network]`

第 3 步网络隔离与第 13 步恢复。

#### `network.disable_network`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 安装期间禁用**物理网卡** |

**被跳过的适配器**（不会被禁用）：

- `IF_TYPE_SOFTWARE_LOOPBACK`（24，回环）
- 名称或描述含 `virtual` / `vmware` / `hyper-v` / `vethernet` / `tap-` / `wintun` /
  `wireguard` / `openvpn` / `zerotier` / `tailscale` / `npcap` / `bluetooth` /
  `wi-fi direct` / `docker` / `wsl`

#### `network.restore_network_after`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 收尾时恢复适配器。**即使该值为 true，恢复也不只依赖收尾步骤**——`NetworkGuard` 的 `Drop` 实现会在任何退出路径（含 panic 展开）重新启用适配器 |

> 若把该值设为 `false`，程序**不会**在退出时恢复网络，需要手动重新启用。

#### `network.adapter_disable_method`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"netsh"` |
| 取值范围 | `"netsh"` / `"powershell"`（其他值归一为 `netsh`） |
| 作用 | 禁用/启用适配器的实现方式 |

| 取值 | 实际命令 |
|---|---|
| `netsh` | `netsh interface set interface name="<适配器名>" admin=disable\|enable` |
| `powershell` | `Set-NetAdapter -Name '<适配器名>' -AdminStatus Down\|Up -Confirm:$false` |

**建议**：优先 `netsh`（启动更快、无执行策略限制）。适配器名含特殊字符时
`powershell` 更稳妥（内部会转义单引号）。

---

### `[process]`

第 10 步进程清理。

#### `process.kill_sw_processes`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 是否执行进程清理。设为 `false` 会跳过进程终止**与许可服务删除** |

#### `process.kill_pattern`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `".*solidwork.*\|.*sldworks.*\|.*sw_dn.*\|.*sidworks.*\|.*flexnet.*\|.*lmgrd.*"` |
| 取值范围 | 关键词或用 `\|` 连接的关键词串；也接受完整正则 |
| 作用 | 匹配待终止进程。当前进程自身永远不会被匹配 |

**匹配规则（都是刻意设计的）**：

| 规则 | 说明 |
|---|---|
| **不区分大小写** | 编译时强制 `(?i:...)`，所以 `SolidWorks` / `SOLIDWORKS` / `solidworks` 一律命中 |
| **忽略首尾空白** | 从文档粘贴时带的缩进/空格不会导致失配 |
| **纯关键词即可** | `solidworks` 等价于 `.*solidworks.*`（正则未锚定，本身就是子串查找） |
| **空值被拒绝** | 空串或纯空白会**直接报错**，避免"空正则匹配一切"把系统进程杀光 |
| 正则非法即报错 | 语法错误会终止该步骤，不会静默跳过 |
| 显式 `(?-i)` 会生效 | 若你确实需要大小写敏感，写 `(?-i)solidworks`（遵循标准正则语义） |

`sw_dn` 与 `sidworks` 是 `_SolidSQUAD_` 相关辅助程序常见的映像名片段，
如不需要可自行删减。

#### `process.match_command_line`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 把匹配范围从"仅进程名"扩展到**进程完整路径 + 命令行** |

按关键词能命中"名字与关键词无关"的进程，例如：

| 进程名 | 命令行 | 是否命中 `solidworks` |
|---|---|---|
| `sldworks.exe` | 任意 | ✅ 按进程名 |
| `helper.exe` | （路径含 `\SOLIDWORKS Corp\`） | ✅ 按路径 |
| `java.exe` | `java -jar C:\tools\solidworks-helper.jar` | ✅ 按命令行 |
| `java.exe` | `java -jar unrelated.jar` | ❌ 不命中 |

**代价**：开启后每轮要查询进程元数据（通过一次 `Get-CimInstance Win32_Process`），
**明显更慢**。只关心进程名时设为 `false` 可显著提速。

**日志会写明命中原因**，便于核对是否误伤：

```
发现进程 helper.exe (pid [1234, 5678])
  · pid 1234 路径 C:\Program Files\SOLIDWORKS Corp\helper.exe
```

> **注意**：`taskkill /IM` 是按**映像名**批量操作的，
> 因此"靠命令行命中"的进程会连带终止同名进程。若这不可接受，
> 请把关键词写得足够具体（例如 `sldworks\.exe$`）。

#### `process.kill_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `2` |
| 取值范围 | `≥ 0` |
| 作用 | 发出 `taskkill /T`（优雅关闭）后，等待进程自行退出的时长 |

#### `process.force_kill_after_timeout`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 等待超时后是否强制终止 |

**终止链**：`taskkill /IM <名> /T` → 等待 `kill_timeout_minutes` →
`taskkill /IM <名> /F /T` → 仍失败则用 Win32 `OpenProcess` + `TerminateProcess` 兜底。

---

### `[flexnet]`

第 12 步许可服务安装。

#### `flexnet.server_install_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `5` |
| 取值范围 | `≥ 0.1` |
| 作用 | 双重用途：① `server_install.bat` 的执行超时；② 服务状态轮询的总预算 |

#### `flexnet.server_check_interval_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `0.5`（30 秒） |
| 取值范围 | `≥ 0.01` |
| 作用 | `sc query "SolidWorks Flexnet Server"` 的轮询间隔 |

#### `flexnet.remove_old_server_first`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 先执行 `server_remove.bat`。找不到该脚本时回退为 `sc stop` + `sc delete` |

---

### `[workdir]`

工作目录与清理策略。

#### `workdir.temp_dir`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `""` |
| 取值范围 | 绝对路径；留空则用 `{app_data_dir}\work` |
| 作用 | 解压与临时文件根目录。非空时实际工作目录为 `{temp_dir}\solidworks-install` |
| 示例 | `temp_dir = "D:\\SWTemp"` |

**空间需求**：解压 ISO 与 `_SolidSQUAD_` 需要 **≥ 60 GB** 可用空间，
建议指向空间充足的独立磁盘（而非系统盘）。

#### `workdir.cleanup_temp_after`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 第 13 步是否递归删除工作目录 |

> **排查失败原因时请设为 `false`** —— 保留解压产物与安装日志是定位问题的关键。
> `TempDirGuard` 的 `Drop` 实现同样遵循该开关。

---

### `[sevenzip]`

解压引擎配置。

#### `sevenzip.exe_path`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `""` |
| 取值范围 | 绝对路径；留空则走三级查找 |
| 作用 | `7z.exe` 的显式路径（最高优先级） |
| 示例 | `exe_path = "C:\\Program Files\\7-Zip\\7z.exe"` |

**四级查找顺序**：

1. `sevenzip.exe_path`（本字段）
2. Tauri 资源目录 / 资源目录下的 `resources\7z.exe`
3. 源码目录 `src-tauri\resources\7z.exe`（开发期）
4. 可执行文件同级目录，最后回退到 `PATH` 中的 `7z`

全部失败会在第 4 步抛出明确错误并终止，**不会静默跳过解压**。

#### `sevenzip.extract_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `30` |
| 取值范围 | `≥ 0.1` |
| 作用 | **单次** 7z 解压的超时。两阶段解压各自独立计时 |

该值还被复用为几个同量级系统级操作的等待预算，避免为一次性操作增加配置项：

| 复用点 | 说明 |
|---|---|
| 网卡禁用/启用命令 | 与挂载镜像同量级的系统操作 |
| `reg import` 单个文件 | 同上 |
| ISO 挂载 | 直接使用 |
| ISO 卸载 | 直接使用 |
| 设置页「选择本地压缩包」的 `7z t` 实测 | 取其 **1/3**，并钳制在 1~10 分钟 |

> **体积大的压缩包请把该值放大**（例如 `60`）。它同时决定了两阶段解压各自
> 允许的最长耗时。

---

### `[antivirus]`

安装前的安全软件处置。放在**解压之前**执行，因为 Defender 与多数第三方杀软
会把 `_SolidSQUAD_` 里的补丁文件判为威胁并直接删除，等解压完再处理就已经晚了。

#### `antivirus.enabled`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 是否启用本阶段。设为 `false` 则整步跳过，只记一条日志 |

关闭后请**手动**把工作目录与安装目录加进杀软的排除项。

#### `antivirus.detect`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 是否用 `root\SecurityCenter2` 的 `AntiVirusProduct` 检测并列出已装的安全软件 |

这是 Windows 安全中心自己用的注册表，比枚举服务或猜进程名可靠。
查询失败（被策略禁用）时会明确提示"未能读出"，**不会假装机器上干净**。

#### `antivirus.remove_defender`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 是否移除 Windows Defender |

执行的是随仓库提供的
[`othertools/windows-defender-remover-main`](../othertools/windows-defender-remover-main)
中 `Script_Run.ps1` 的 `y` 分支**相同**的动作：

1. `PowerRun.exe powershell ... RemoveSecHealthApp.ps1` —— 移除安全中心应用
2. `PowerRun.exe regedit /s <每个 .reg>` —— 导入全部策略注册表项
3. 删除 SmartScreen 相关文件

> **不含重启**。原脚本最后会 `shutdown /r /f /t 10`，本程序刻意不执行它，
> 否则会直接打断安装流程。

#### `antivirus.defender_tool_path`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `""`（自动查找） |
| 取值范围 | 指向含 `PowerRun.exe` 与 `RemoveSecHealthApp.ps1` 的 `script` 目录 |
| 作用 | 显式指定 windows-defender-remover 的位置 |

**两个文件缺一**即视为指错位置。留空时的查找顺序：

| # | 位置 |
|---|---|
| 1 | 本字段 |
| 2 | exe 同级的 `othertools\windows-defender-remover-main\script` |
| 3 | Tauri 资源目录下的同名路径 |
| 4 | 源码目录（开发期） |

找不到工具**不会让安装失败**，只在日志与界面报出"无法移除 Defender"。

#### `antivirus.warn_reboot_after_removal`

| 项 | 值 |
|---|---|
| 类型 | boolean |
| 默认值 | `true` |
| 作用 | 移除 Defender 后是否提示「必须重启才彻底生效」 |

删除动作本身已完成，但**驱动与服务注册要重启后才消失**。
本程序**不会替你重启，也不因此中断流程**——建议先让安装继续
（Defender 已经不会再删文件），安装结束后再重启系统。

#### `antivirus.third_party_mode`

| 项 | 值 |
|---|---|
| 类型 | string |
| 默认值 | `"terminate"` |
| 取值范围 | `"prompt"` / `"terminate"` / `"uninstall"`（其它值归一为 `terminate`） |

| 取值 | 行为 |
|---|---|
| `prompt` | 只检测并在界面提示用户手动从系统托盘退出 |
| `terminate` | 额外**终止其进程 / 停止其服务** |
| `uninstall` | 再额外按其注册表 `UninstallString` 尝试静默卸载 |

**默认不是 `uninstall`**：静默卸载别人的安全软件风险过高，不该是默认行为。
即使选了 `uninstall`，个人版杀软普遍会弹交互界面或有自保护，**失败是预期结果**，
程序会如实回报。

**判定标准**：以「是否仍注册在 SecurityCenter2 且实时防护开启」为准，
而不是"进程是否被杀掉"——进程可能被自保护立刻拉起来。

**命中关键词的来源**：产品显示名（含去空格的整体形式）、显示名里的词、
厂商 `pathToSignedProductExe` 的文件名；会过滤 `windows` / `security` / `antivirus`
等过于通用的词，避免误杀。

#### `antivirus.settle_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `0.5`（30 秒） |
| 取值范围 | `≥ 0` |
| 作用 | 处置后等待其真正停下的时长 |

#### `antivirus.command_timeout_minutes`

| 项 | 值 |
|---|---|
| 类型 | float |
| 单位 | **分钟** |
| 默认值 | `5` |
| 取值范围 | `≥ 0.1` |
| 作用 | 单条处置命令（PowerRun 调用 / regedit 导入 / taskkill / sc stop）的超时 |

---

## 远程配置拉取机制

### 操作步骤

1. 打开设置页，进入 `03 远程配置` 分组
2. 确认 `remote_config.url` 指向可访问的 TOML 文件
3. 点击工具条的 **「拉取远程配置」**
4. 拉取结果会**填入表单**（不会自动写盘）
5. 确认无误后点击 **「保存配置」** 写入 `{app_data_dir}/config.toml`

### 服务端要求

| 项 | 要求 |
|---|---|
| 协议 | HTTP 或 HTTPS |
| 内容类型 | `text/plain` / `application/toml` / 任意（不做校验） |
| 编码 | UTF-8 优先；非 UTF-8 时按系统 ANSI(936) 解码 |
| 内容 | **不必完整** —— 缺失字段由嵌入式默认配置补齐 |

### 合并语义

远程配置与默认配置做结构保留合并：

- 远程**有的字段** → 覆盖默认值
- 远程**缺少的字段** → 用默认值补齐（带注释）
- 远程**多出的字段** → 保留在文本中，反序列化时忽略

因此远程配置可以只写需要分发的差异项，例如：

```toml
# 只覆盖下载与轮询参数
[download]
url = "https://intranet.example.com/sw2024sp5.7z"
threads = 32

[install.polling]
timeout_minutes = 360
```

### 失败处理

| 情况 | 表现 |
|---|---|
| URL 为空 | 提示「远程配置 URL 为空，请先在 [remote_config] 中填写」 |
| 连接失败 / 超时 | 提示具体网络错误；表单保持原值不变 |
| HTTP 非 2xx | 提示 `远程配置返回 HTTP <状态码>` |
| TOML 语法错误 | 提示 `用户配置解析失败: <toml_edit 错误详情>`；表单保持原值 |

任何失败都**不会破坏本地已有配置**。

---

## 重置为默认配置

### 通过界面

设置页工具条 → **「重置默认」**。该操作用嵌入式 `default-config.toml`
覆盖用户文件并刷新表单，**不可撤销**。

### 通过文件系统

删除用户配置文件，下次启动会自动用嵌入式默认配置重建：

```powershell
Remove-Item "$env:APPDATA\com.solidworks.autoinstaller\config.toml"
```

### 通过命令行直接编辑

用任意文本编辑器修改，下次启动或点「重新读取」即生效：

```powershell
notepad "$env:APPDATA\com.solidworks.autoinstaller\config.toml"
```

> 手工编辑时保留注释不会影响解析；保存配置时注释也会被继续保留。

---

---

## 完整空白默认配置文件

以下内容与 `src-tauri/resources/default-config.toml` 完全一致，
所有项为默认值，**可直接复制使用**。

```toml
# ============================================================
# SolidWorks 2024 SP5 自动安装程序 — 默认配置文件
# 所有时间参数单位为分钟（float 类型，0.5 = 30 秒）
# 该文件通过 include_str! 嵌入二进制，作为合并基线。
# ============================================================

[general]
language = "zh-CN"                   # 界面语言: zh-CN / en-US
theme = "endfield"                   # 界面风格: endfield

# ---------- 下载配置 ----------
[download]
url = "https://example.com/solidworks2024sp5.7z"   # ← 改成你自己的下载地址
threads = 16                         # 下载线程数，范围 1~255
checksum_sha256 = ""                 # SHA-256 校验值，留空则跳过（分片模式下不生效）
retry_count = 3                      # 下载失败重试次数
retry_backoff_minutes = 0.5          # 重试间隔基数（指数退避）
connect_timeout_minutes = 1          # 连接超时
read_timeout_minutes = 5             # 读取超时

# ---------- 分片下载（可选） ----------
# 每个分片一个 URL，每行一个，数量不限。非空时进入分片模式：
# 各片下载到同一目录并**保留原始文件名**，最后把 .001 交给 7z ——
# 7z / NanaZip 的 "a -v" 分卷会被整套自动识别，不要拼接。
# 留空则使用上面的 url 单包模式。
multipart_urls = []
multipart_concat = false             # 仅"非标准切分"时才设为 true

# ---------- 远程配置 ----------
[remote_config]
url = "https://example.com/config.toml"  # ← 改成你的远程配置地址
auto_fetch_on_start = false          # 启动时自动拉取
fetch_timeout_minutes = 1            # 拉取超时

# ---------- 安装配置 ----------
[install]
install_drive = "C"                  # 目标盘符: C / D / E ...
install_path = ""                    # 自定义安装路径，留空则用 {drive}:\SW
components_whitelist = []            # 组件白名单，空数组=全部组件
serial_number = ""                   # 序列号，留空则依赖 .reg 文件
language_pack = "chinese-simplified"  # 语言包（介质 swwi\lang\ 下的目录名）

# ---------- 轮询检测 ----------
[install.polling]
poll_interval_minutes = 0.5          # 轮询间隔（30 秒）
timeout_minutes = 240                # 最长等待（4 小时）
log_check_enabled = true             # 检查安装日志
process_check_enabled = true         # 检查安装进程是否退出
file_check_enabled = true            # 检查 SLDWORKS.exe 是否生成

# ---------- 弹窗处理 ----------
[install.popup]
scan_interval_ms = 500               # 窗口扫描间隔（毫秒）
auto_click_confirm = true            # 自动点击"确定"
auto_click_yes = true                # 自动点击"是"
auto_click_no = false                # 不自动点击"否"
popup_timeout_minutes = 5            # 单个弹窗等待超时

# ---------- 网络管理 ----------
[network]
disable_network = true               # 安装期间禁用网络
restore_network_after = true         # 安装后恢复网络
adapter_disable_method = "netsh"     # "netsh" 或 "powershell"

# ---------- 进程管理 ----------
[process]
kill_sw_processes = true
# 关键词（正则，不区分大小写，首尾空白自动忽略）。写纯关键词即可。
# ← 按你的环境增删关键词；空值会被拒绝以免误杀全部进程。
kill_pattern = ".*solidwork.*|.*sldworks.*|.*sw_dn.*|.*sidworks.*|.*flexnet.*|.*lmgrd.*"
match_command_line = true            # 是否同时匹配进程路径与命令行（更慢但更全）
kill_timeout_minutes = 2             # 优雅退出等待时间
force_kill_after_timeout = true      # 超时后强制终止

# ---------- FlexNet 服务 ----------
[flexnet]
server_install_timeout_minutes = 5   # 等待 server_install.bat 完成的最长时间
server_check_interval_minutes = 0.5  # 服务状态轮询间隔
remove_old_server_first = true       # 是否先运行 server_remove.bat

# ---------- 工作目录 ----------
[workdir]
cleanup_temp_after = true            # 安装完成后清理临时文件
temp_dir = ""                        # 临时目录，留空则用系统默认

# ---------- 7z 解压 ----------
[sevenzip]
exe_path = ""                        # 7z.exe 路径，留空则自动查找
extract_timeout_minutes = 30         # 单次解压超时
auto_download = true                 # 找不到 7z 时自动从 download_url 获取
download_url = "https://github.com/ip7z/7zip/releases/latest/download/7zr.exe"
download_timeout_minutes = 3         # 自动下载超时

# ---------- 安全软件处置 ----------
# Windows Defender 与多数第三方杀软会把 _SolidSQUAD_ 里的补丁判为威胁并删除，
# 导致安装走到「文件替换」时缺少源文件。因此本阶段在**解压之前**执行。
[antivirus]
enabled = true                       # 是否启用本阶段
detect = true                        # 是否用 SecurityCenter2 检测并告知用户
remove_defender = true               # 是否移除 Windows Defender
# windows-defender-remover 的 script 目录（含 PowerRun.exe 与 RemoveSecHealthApp.ps1）。
# 留空则自动查找：配置 → exe 同级的 othertools\... → 资源目录 → 源码目录
defender_tool_path = ""
# 删除 Defender 后提示「必须重启」。本程序不会自动重启，也不会因此中断安装。
warn_reboot_after_removal = true
# 第三方杀软处置方式：
#   prompt    只检测并提示用户手动从托盘退出
#   terminate 额外终止其进程 / 停止其服务
#   uninstall 再额外尝试按其 UninstallString 静默卸载
third_party_mode = "terminate"
settle_minutes = 0.5                 # 处置后等待生效的时间
command_timeout_minutes = 5          # 单条处置命令超时
```

---

## 常见配置场景

### 场景一：内网镜像 + 校验 + 高并发

```toml
[download]
url = "https://intranet.mirror.local/sw/2024sp5/solidworks2024sp5.7z"
threads = 64
checksum_sha256 = "在你本地用 certutil -hashfile <文件> SHA256 获取"
retry_count = 5
retry_backoff_minutes = 0.25
connect_timeout_minutes = 0.5
read_timeout_minutes = 10
```

### 场景二：安装到 D 盘独立目录，保留现场以便排错

```toml
[install]
install_drive = "D"
install_path = "D:\\Apps\\SOLIDWORKS"

[workdir]
temp_dir = "E:\\SWTemp"      # 独立磁盘，≥ 60 GB
cleanup_temp_after = false   # 保留解压产物与日志

[install.polling]
timeout_minutes = 360        # 慢速磁盘放宽等待
```

### 场景三：不隔离网络（远程部署 / 需要联网许可校验）

```toml
[network]
disable_network = false
restore_network_after = true
```

### 场景四：半自动安装，关键弹窗由人工确认

```toml
[install.popup]
scan_interval_ms = 1000
auto_click_confirm = false   # 不自动点确定，留给人判断
auto_click_yes = true        # 仍自动处理"不能核实此服务器存在"
auto_click_no = false
popup_timeout_minutes = 10
```

### 场景五：只装核心模块（走 msiexec 回退路径）

```toml
[install]
components_whitelist = ["SOLIDWORKS", "Simulation"]
# 注意：startswinstall.exe 存在且 force_msiexec=false 时优先使用它；组件白名单只作用于 msiexec
```

### 场景六：本地模式（不使用下载）

```toml
[download]
url = ""                     # 留空后主页只能用"选择本地压缩包"
threads = 16
```

主页点击「选择本地压缩包」后，本次运行使用所选文件；无下载 URL 时
「开始自动部署」按钮才会启用（二者至少需要其一）。

---

## 相关文档

| 文档 | 内容 |
|---|---|
| [`CONFIG_GUIDE.md`](./CONFIG_GUIDE.md) | **我需要填什么** —— 每个键是否必填、填什么、最小可用示例、自检清单 |
| [`COMMAND_LINE.md`](./COMMAND_LINE.md) | 官方命令行属性参考：属性名、语言代码表、ADDLOCAL 层级、介质路径 |
| [`CUSTOMIZE.md`](./CUSTOMIZE.md) | 部署前必须改什么、已知局限、完整自检清单 |
| [`INSTALL_FLOW.md`](./INSTALL_FLOW.md) | 13 阶段流程详解、弹窗规则、轮询逻辑 |
| [`TROUBLESHOOTING.md`](./TROUBLESHOOTING.md) | 按症状排查 |
| [`DEVELOPMENT.md`](./DEVELOPMENT.md) | 二次开发与构建 |
