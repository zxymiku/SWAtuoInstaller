//! 下载文件名解析。
//!
//! # 为什么需要这一层
//!
//! 网盘（OneDrive / SharePoint / 各种「下载页」）的直链**不把文件名放在路径里**，
//! 而是塞进查询参数，并且常常在末尾挂一个 `.aspx` 之类的假后缀。两个真实例子：
//!
//! ```text
//! .../download.aspx?UniqueId=6c0c5656%2D9228%2D4c35%2D8e43%2D9641906d9492
//! .../download.aspx?SourceUrl=%2Fpersonal%2F...%2FDownloads%2E7z%2E002
//! ```
//!
//! 前者完全看不出文件名，后者能看出 `Downloads.7z.002`。
//! 于是**下载下来的文件叫 `Downloads.7z.001.aspx`** —— 7z 虽然能按文件头识别内容，
//! 但 `x.001.aspx` 这种名字会让它**找不到同目录的 `.002`**，整套分卷直接解不开。
//!
//! # 解析顺序
//!
//! 1. **查询参数**（`SourceUrl` / `UniqueId` / `file` / `filename` / …）—— 最可靠，
//!    OneDrive 的 `SourceUrl` 里带的就是真实文件名；
//! 2. **URL 路径末段**；
//! 3. **`Content-Disposition` 响应头**（需要一次探测请求，可选）。
//!
//! 得到候选名后统一做**假后缀剥离**：`Downloads.7z.001.aspx` → `Downloads.7z.001`。
//! 剥离采用白名单策略，只去掉 `.aspx` / `.ashx` / `.axd` / `.php` / `.jsp` 这类
//! **已知的下载端点后缀**，避免把 `setup.php` 这种真实文件名误伤。

use std::path::{Path, PathBuf};

/// 查询参数里可能承载文件名的键（小写比较）。
///
/// 顺序即优先级：`SourceUrl` 最具体，`UniqueId` 最泛（OneDrive 的 GUID，不带扩展名）。
const NAME_PARAM_KEYS: &[&str] = &[
    "sourceurl",
    "file",
    "filename",
    "fn",
    "name",
    "download",
    "uniqueid",
];

/// 已知的「下载端点」假后缀。只有这些才会被剥掉。
const FAKE_SUFFIXES: &[&str] = &[
    ".aspx", ".ashx", ".axd", ".asmx", ".php", ".jsp", ".do", ".cgi", ".action", ".crdownload",
];

/// 从 URL 解析文件名的结果。
#[derive(Debug, Clone)]
pub struct ResolvedName {
    /// 最终应当使用的文件名（已剥离假后缀）。
    pub name: String,
    /// 名字是从哪来的——写进日志，便于排查。
    pub origin: NameOrigin,
}

/// 文件名来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameOrigin {
    /// 来自 URL 查询参数（例如 OneDrive 的 `SourceUrl`）
    QueryParam,
    /// 来自 URL 路径末段
    UrlPath,
    /// 来自 `Content-Disposition` 响应头
    Header,
    /// 完全推不出来，调用方使用兜底名
    Fallback,
}

impl NameOrigin {
    pub fn label(self) -> &'static str {
        match self {
            NameOrigin::QueryParam => "URL 查询参数",
            NameOrigin::UrlPath => "URL 路径",
            NameOrigin::Header => "Content-Disposition 响应头",
            NameOrigin::Fallback => "兜底命名",
        }
    }
}

/// 解析 URL 得到候选文件名。推不出来时返回 `None`，由调用方兜底。
///
/// **不做网络请求**：只看 URL 本身。需要看响应头时用
/// [`parse_content_disposition`] 单独处理。
pub fn resolve_from_url(raw_url: &str) -> Option<ResolvedName> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }

    // ---- 1. 查询参数优先 ----
    let query = trimmed
        .split_once('?')
        .map(|(_, rest)| rest.split('#').next().unwrap_or(rest))
        .unwrap_or("");

    for key in NAME_PARAM_KEYS {
        let Some(value) = query_param(query, key) else {
            continue;
        };
        let decoded = percent_decode(&value);
        // SourceUrl 可能是一个完整路径 `/personal/.../Downloads.7z.002`，
        // 也可能带目录分隔符，统一只取最后一段。
        if let Some(candidate) = last_path_segment(&decoded) {
            if let Some(name) = normalize_name(&candidate) {
                return Some(ResolvedName {
                    name,
                    origin: NameOrigin::QueryParam,
                });
            }
        }
    }

    // ---- 2. URL 路径末段 ----
    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let path_part = without_scheme
        .split(['?', '#'])
        .next()
        .unwrap_or(without_scheme);
    if let Some(candidate) = last_path_segment(path_part) {
        if let Some(name) = normalize_name(&candidate) {
            return Some(ResolvedName {
                name,
                origin: NameOrigin::UrlPath,
            });
        }
    }

    None
}

/// 从 `Content-Disposition` 头解析文件名。
///
/// 支持两种写法：
///
/// ```text
/// attachment; filename="Downloads.7z.001"
/// attachment; filename*=UTF-8''Downloads%2E7z%2E001
/// ```
pub fn parse_content_disposition(header: &str) -> Option<String> {
    let lower = header.to_ascii_lowercase();

    // `filename*=UTF-8''<pct-encoded>` 优先，它能正确表达非 ASCII 名。
    if let Some(index) = lower.find("filename*=") {
        let rest = &header[index + "filename*=".len()..];
        let value = rest.split([';', '\r', '\n']).next().unwrap_or(rest).trim();
        // 形如 UTF-8''name 或 utf-8'en'name
        let payload = value.rsplit("''").next().unwrap_or(value);
        let decoded = percent_decode(payload.trim_matches('"'));
        if let Some(name) = normalize_name(&decoded) {
            return Some(name);
        }
    }

    if let Some(index) = lower.find("filename=") {
        let rest = &header[index + "filename=".len()..];
        let value = rest.split([';', '\r', '\n']).next().unwrap_or(rest).trim();
        let unquoted = value.trim_matches('"').trim();
        if let Some(name) = normalize_name(&percent_decode(unquoted)) {
            return Some(name);
        }
    }

    None
}

/// 从一次探测结果里挑出最可信的文件名。
///
/// 顺序：`Content-Disposition`（服务器明确给出的）→ URL 解析。
/// 之所以把响应头放在前面：它是服务器对**这一次请求**的直接回答，
/// 而 URL 参数可能只是复制粘贴留下的痕迹。
pub fn resolve_name(url: &str, content_disposition: Option<&str>) -> ResolvedName {
    if let Some(header) = content_disposition {
        if let Some(name) = parse_content_disposition(header) {
            return ResolvedName {
                name,
                origin: NameOrigin::Header,
            };
        }
    }
    resolve_from_url(url).unwrap_or_else(|| ResolvedName {
        name: String::new(),
        origin: NameOrigin::Fallback,
    })
}

/// 归一化候选文件名：剥离假后缀、去掉非法字符、拒绝明显不是文件名的值。
fn normalize_name(raw: &str) -> Option<String> {
    let mut name = raw.trim().trim_matches('"').to_string();
    if name.is_empty() {
        return None;
    }
    // 路径分隔符只取最后一段
    if let Some(last) = last_path_segment(&name) {
        name = last;
    }
    // URL 里 `+` 有时当空格用，但文件名里的 `+` 是合法的，这里不动它。
    name = strip_fake_suffix(&name);
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    // 去掉 Windows 不允许的字符
    name = sanitize(&name);
    if name.is_empty() {
        return None;
    }
    Some(name)
}

/// 剥离已知的下载端点假后缀。
///
/// `Downloads.7z.001.aspx` → `Downloads.7z.001`
///
/// **只在剥掉之后仍留有扩展名时才剥**：
/// `report.aspx` 剥成 `report` 会丢掉它原本的形态，
/// 而 `Downloads.7z.001.aspx` 剥掉后还剩 `.001`，说明 `.aspx` 是外加的。
fn strip_fake_suffix(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for suffix in FAKE_SUFFIXES {
        if !lower.ends_with(suffix) {
            continue;
        }
        let stem = &name[..name.len() - suffix.len()];
        if stem.is_empty() {
            continue;
        }
        // 剥掉后还有扩展名 → 说明假后缀是外加的，可以剥。
        if Path::new(stem).extension().is_some() {
            return stem.to_string();
        }
        // 剥掉后没有扩展名 → 保留原样，避免把 report.aspx 变成 report。
        return name.to_string();
    }
    name.to_string()
}

/// 取路径的最后一段（同时兼容 `/` 与 `\`）。
fn last_path_segment(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches(['/', '\\']);
    let segment = trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed)
        .trim();
    if segment.is_empty() {
        None
    } else {
        Some(segment.to_string())
    }
}

/// 替换 Windows 文件名非法字符。
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect::<String>()
        .trim_matches(['.', ' '])
        .to_string()
}

/// 取查询参数值（大小写不敏感，值不解码）。
fn query_param(query: &str, wanted: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => continue,
        };
        if key.eq_ignore_ascii_case(wanted) {
            if value.is_empty() {
                continue;
            }
            return Some(value.to_string());
        }
    }
    None
}

/// 百分号解码。只处理 `%XX`，非法序列原样保留。
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = (bytes[index + 1] as char).to_digit(16);
            let lo = (bytes[index + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 判断名字是否像**分卷**（`.001` … `.9999`）。
///
/// 与 [`super::sniff::is_volume_part_name`] 同一约定，这里复制一份是为了让
/// 本模块可以独立测试，不依赖嗅探模块。
pub fn volume_number(name: &str) -> Option<u32> {
    let extension = Path::new(name).extension()?.to_string_lossy().to_string();
    if !(3..=4).contains(&extension.len()) || !extension.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    extension.parse::<u32>().ok()
}

/// 把分卷编号写进文件名：`Downloads.7z` + 2 → `Downloads.7z.002`。
///
/// 若名字里已经带分卷号，则**替换**它。
pub fn set_volume_number(name: &str, number: u32) -> String {
    let base = match volume_number(name) {
        Some(_) => {
            let stem = &name[..name.len() - Path::new(name).extension().unwrap().len() - 1];
            stem.to_string()
        }
        None => name.to_string(),
    };
    format!("{base}.{number:03}")
}

/// 从一组已解析的名字里推断分卷基底，用于补齐缺失的卷号。
///
/// 能识别两种形态：
///
/// | 输入（全部同名） | 基底 |
/// |---|---|
/// | `Downloads.7z.001` × N | `Downloads.7z` |
/// | `Downloads.7z` × N | `Downloads.7z`（靠压缩包扩展名判断） |
///
/// 第二种是网盘常见情况：`SourceUrl` 里只有基底名，没有卷号。
/// 只在扩展名看起来是压缩包时才认，避免把 `readme.txt` 也叫成基底。
pub fn volume_base(names: &[String]) -> Option<String> {
    let mut base: Option<String> = None;
    let mut saw_volume = false;

    for name in names {
        match volume_number(name) {
            Some(_) => {
                saw_volume = true;
                let extension = Path::new(name).extension()?;
                let stem = &name[..name.len() - extension.len() - 1];
                match &base {
                    None => base = Some(stem.to_string()),
                    // 基底不一致 → 不是同一套分卷，不能统一编号
                    Some(existing) if existing != stem => return None,
                    Some(_) => {}
                }
            }
            None => {
                // 没有卷号：只有当它看起来是压缩包基底时，才当作基底。
                if !looks_like_archive_base(name) {
                    return None;
                }
                match &base {
                    None => base = Some(name.clone()),
                    Some(existing) if existing != name => return None,
                    Some(_) => {}
                }
            }
        }
    }

    if base.is_some() && (saw_volume || names.len() > 1) {
        base
    } else {
        None
    }
}

/// 名字是否像「压缩包的基底」（带压缩包扩展名、且没有卷号）。
fn looks_like_archive_base(name: &str) -> bool {
    const ARCHIVE_EXTS: &[&str] = &[
        "7z", "zip", "rar", "tar", "gz", "bz2", "xz", "zst", "iso", "cab", "wim", "esd",
    ];
    match Path::new(name).extension() {
        Some(ext) => {
            let lower = ext.to_string_lossy().to_ascii_lowercase();
            ARCHIVE_EXTS.contains(&lower.as_str())
        }
        None => false,
    }
}

/// 判断 URL 是否是**需要重定向解析**的短链（OneDrive `1drv.ms` 等）。
///
/// 这类链接的路径末段是一个分享令牌，不是文件名。直接拿它当文件名会得到
/// `IQBWVgxsKJI...` 这样的结果，所以调用方应当优先做 HEAD 探测，
/// 探不到再用一个中性的兜底名。
pub fn is_share_short_link(url: &str) -> bool {
    const SHORT_HOSTS: &[&str] = &[
        "1drv.ms",
        "aka.ms",
        "db.tt",
        "goo.gl",
        "bit.ly",
        "t.cn",
        "sourl.cn",
    ];
    let Some(host) = host_of(url) else {
        return false;
    };
    SHORT_HOSTS.iter().any(|short| host.eq_ignore_ascii_case(short))
}

/// 从 URL 里取出主机名（小写、不含端口）。
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, rest)| rest)?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// 拼接最终落盘路径。
pub fn join(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_onedrive_unique_id() {
        let url = "https://onedrive.live.com/personal/585b0a8a02824759/_layouts/15/download.aspx?UniqueId=6c0c5656%2D9228%2D4c35%2D8e43%2D9641906d9492";
        let resolved = resolve_from_url(url).unwrap();
        // UniqueId 是不带扩展名的 GUID，剥掉假后缀的规则会保留 .aspx 前的部分
        assert_eq!(resolved.origin, NameOrigin::QueryParam);
        assert!(resolved.name.starts_with("6c0c5656-9228-4c35-8e43-9641906d9492"));
    }

    #[test]
    fn parses_onedrive_source_url() {
        let url = "https://onedrive.live.com/personal/82875957a91204af/_layouts/15/download.aspx?SourceUrl=%2Fpersonal%2F82875957a91204af%2FDocuments%2FDownloads%2E7z%2E003";
        let resolved = resolve_from_url(url).unwrap();
        assert_eq!(resolved.name, "Downloads.7z.003");
        assert_eq!(resolved.origin, NameOrigin::QueryParam);
    }

    #[test]
    fn strips_aspx_suffix() {
        let url = "https://host/download.aspx?SourceUrl=/x/Downloads.7z.001";
        let resolved = resolve_from_url(url).unwrap();
        assert_eq!(resolved.name, "Downloads.7z.001");
    }

    #[test]
    fn keeps_real_aspx_extension() {
        // 剥掉就只剩 report，说明 .aspx 是它真实的扩展名，不能剥
        let url = "https://host/files/report.aspx";
        let resolved = resolve_from_url(url).unwrap();
        assert_eq!(resolved.name, "report.aspx");
    }
}
