# src-tauri/resources

本目录存放随程序**打包**的运行时资源。

| 文件 | 必需 | 说明 |
|---|---|---|
| `default-config.toml` | 是 | 嵌入二进制的默认配置（`include_str!`），同时作为用户配置的合并基线 |
| `7zr.exe` | **是** | 解压引擎，**已随仓库提供**（588 KB，见下方「关于许可」） |
| `7z.exe` / `7za.exe` / `7zz.exe` | 否 | 可选的替代实现；放进来会**优先于** `7zr.exe` 被选中（按文件名顺序） |

---

## 为什么内置 `7zr.exe`

本项目最初让程序在找不到 7z 时从 **GitHub 官方发布**自动下载。实际部署时发现：

> **部分设备连接 GitHub 很慢**，导致每次首次运行都要等一次长时间的超时。

因此把 7-Zip 官方的独立控制台程序 `7zr.exe` **直接打包进程序**：
单文件、免安装、588 KB，覆盖本项目两阶段解压所需的全部格式。

**这样做的效果**：

| 场景 | 结果 |
|---|---|
| 慢网 / 内网 / 完全离线 | **不再访问网络**，直接用打包的 7zr.exe |
| 想换更强的 7z（如完整版 `7z.exe`） | 把文件放进本目录即可，按文件名顺序优先命中 |
| 已装 7-Zip / NanaZip | 打包版本优先级更高；想让系统版本生效就删掉本目录的 7zr.exe |

`7zr.exe` 是 **32 位**程序，在 32/64 位 Windows 上都能运行。

---

## 7z 解压程序的获取顺序

程序在开始解压前（第 4 步之前）按**从高到低**的优先级查找：

| # | 位置 | 来源标签 | 说明 |
|---|---|---|---|
| 1 | `[sevenzip].exe_path` | 配置指定 | 显式路径，最高优先级 |
| 2 | **本目录**（打包资源） | 随程序打包 | **已内置 7zr.exe，因此这一级通常直接命中** |
| 3 | `{app_data_dir}` 下的下载缓存 | 自动下载缓存 | 自动下载后落地，下次启动直接复用 |
| 4 | 注册表 `HKLM\SOFTWARE\7-Zip` | 已安装的 7-Zip | 读 `Path64` / `Path` 后拼 `7z.exe` |
| 5 | 第三方实现（NanaZip / 7-Zip ZS / PeaZip / Bandizip） | 第三方兼容实现 | 含 `NanaZipC.exe` 这类 AppExecutionAlias |
| 6 | 程序 exe 同级目录 | 随程序打包 | 绿色版部署 |
| 7 | 系统 `PATH` | 系统 PATH | 最后兜底 |

**全部失败**时才进入自动下载：若 `[sevenzip].auto_download = true`（默认），
从 `[sevenzip].download_url` 下载到应用数据目录；
否则在第 4 步抛出明确错误并终止，**不会静默跳过解压**。

> 因为第 2 级已经内置了 7zr.exe，**正常情况下永远不会走到自动下载**。
> 这正是打包的目的 —— 慢网机器不会再有那次网络等待。

---

## 方式一：使用打包的 7zr.exe（默认，无需操作）

仓库已经带了 `7zr.exe`，`tauri.conf.json` 也已把它列入 `bundle.resources`：

```json
"resources": [
  "resources/default-config.toml",
  "resources/7zr.exe"
]
```

**开箱即用。** 不需要任何手工步骤。

### 换成自己准备的 7z

```powershell
# 用完整版 7z.exe 覆盖（支持更多格式，体积更大）
Copy-Item "C:\Program Files\7-Zip\7z.exe" "src-tauri\resources\7z.exe"
```

放进去之后**不用改 `bundle.resources`**（`7z.exe` 已在通配查找范围内），
但要把它加进 `bundle.resources` 才能真的打包：

```json
"resources": [
  "resources/default-config.toml",
  "resources/7zr.exe",
  "resources/7z.exe"
]
```

> `src-tauri/resources/7z.exe` 在 `.gitignore` 里 —— 那是**可选替代品**，
> 不入库；而 `7zr.exe` 是必需资源，**故意不忽略**。

---

## 方式二：自动下载（已不是首选）

默认配置仍保留这一条作为兜底：

```toml
[sevenzip]
auto_download = true
download_url = "https://github.com/ip7z/7zip/releases/latest/download/7zr.exe"
download_timeout_minutes = 3
```

下载地址指向 7-Zip **官方发布**的独立控制台版本
[`7zr.exe`](https://github.com/ip7z/7zip/releases/latest)。

下载完成后会**校验文件头是否为 `MZ`**（可执行文件标识），
避免把 HTML 错误页之类的响应存成"程序"反复失败。

**内网部署**可以把 `download_url` 换成本地文件服务器地址：

```toml
[sevenzip]
download_url = "http://intranet.mirror.local/tools/7zr.exe"
```

**完全离线**可以直接关掉（有打包版本时本来也用不到）：

```toml
[sevenzip]
auto_download = false
```

---

## 方式三：依赖已安装的 7-Zip

```powershell
winget install 7zip.7zip
```

程序会自动从注册表读出安装位置。这条路径**不需要**改 `bundle.resources`，
但要求目标机器上确实装过 7-Zip，且**优先级低于打包版本**。

---

## 关于许可

`7zr.exe` 来自 7-Zip 官方发布 <https://www.7-zip.org/download.html>
（`7-Zip Extra` 中的独立控制台版本）。

实测该二进制自报的版权信息：

```
7-Zip (r) 26.03 (x86) : Igor Pavlov : Public domain : 2026-09-03
```

> **注意区分**：`7zr.exe` 自报 **Public domain**，
> 而**完整版** `7z.exe` 采用 **GNU LGPL**（其 unRAR 部分另有 restriction）。
> 混用这两个说法容易出错，所以这里按二进制自报为准。

**本仓库分发 `7zr.exe` 这一个二进制文件**，因此：

- 保留原始版权声明（7-Zip 官方发布页与 `7zr.exe` 自报信息）；
- **不修改**该二进制；
- 本项目只把它当**外部命令行工具**调用（`7z x` / `7z t`），
  不链接其代码、不修改其二进制。

---

## ⚠️ 内置 `7zr.exe` 的格式限制（重要）

`7zr.exe` 是 **7-Zip Extra 的精简版**。实测 `7zr i` 输出的支持范围：

```
Formats:
  7z       7z          7 z BC AF ' 1C
  Split    001
  lzma     lzma
  lzma86   lzma86
  xz       xz txz (.tar)
  Hash     sha256 sha512 … md5 crc32 …
```

**只支持 `7z` / 分卷(`.001`) / `lzma` / `xz` / 哈希校验。**

它**不能**解：`zip`、`rar`、`gz`、`bz2`、`cab`、`iso`、`chm`、`zstd`。

### 对本项目有没有影响？

**没有。** 本项目实际要解的两样东西都是 7z：

| 阶段 | 文件 | 格式 |
|---|---|---|
| 第 1 阶段 | `Downloads.7z.001`（+ `.002`…，共 11 卷） | **7z 分卷** |
| 第 2 阶段 | `_SolidSQUAD_.7z` | **7z** |

已实测验证分卷场景：用 7zr 打一个 **11 卷**的 7z，
**只把 `.001` 交给它**，解出的文件 SHA-256 与原始文件**完全一致** ——
说明它能正确自动读取同目录的其余分卷。

### 但如果你换成 zip / rar / iso

程序会在解压前**明确告警**并给出出路，而不是让你看一句
`Cannot open the file as archive` 白排查。告警内容：

- 把完整版 `7z.exe` 放到程序同级目录或 `src-tauri\resources\`（会自动优先使用）；
- 或在 `[sevenzip].exe_path` 里指定完整版路径；
- 或把 `[sevenzip].download_url` 换成完整版 `7z.exe` 的地址。

完整版 `7z.exe` 从 <https://www.7-zip.org/download.html> 的
`7-Zip Extra` 包中获取，支持 zip / rar / iso / cab / chm 等全部格式。
