//! 压缩包格式嗅探与缓存文件命名。
//!
//! 这里全是**纯函数**：只根据文件头字节和 URL 推导结果。
//! 嗅探结果**只用于日志与缓存命名** —— 解压时不传 `-t<格式>`，
//! 由 7z 自己按文件头识别，因此扩展名不影响能否解压。

use std::path::Path;

pub fn sniff_archive_kind(path: &Path) -> Option<&'static str> {
    use std::io::Read;

    // 读 64 KiB：ISO9660 的 "CD001" 主卷描述符在偏移 0x8001（32769），
    // 只用 512 字节的头部是探测不到 ISO 的。
    const SNIFF_WINDOW: usize = 64 * 1024;

    let mut file = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; SNIFF_WINDOW];
    let read = file.read(&mut head).ok()?;
    let head = &head[..read];

    let starts = |signature: &[u8]| {
        head.len() >= signature.len() && &head[..signature.len()] == signature
    };

    if starts(b"7z\xBC\xAF\x27\x1C") {
        return Some("7z");
    }
    if starts(b"Rar!\x1A\x07") {
        return Some("rar");
    }
    if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") || starts(b"PK\x07\x08") {
        return Some("zip");
    }
    if starts(&[0x1F, 0x8B]) {
        return Some("gzip");
    }
    if starts(b"BZh") {
        return Some("bzip2");
    }
    if starts(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return Some("xz");
    }
    if starts(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return Some("zstd");
    }
    if starts(b"MSCF") {
        return Some("cab");
    }
    if starts(b"ITSF") {
        return Some("chm");
    }
    // ISO9660：卷描述符标识 "CD001" 位于偏移 0x8001，用窗口搜索覆盖它。
    if head.len() > 0x8001 && head.windows(5).any(|window| window == b"CD001") {
        return Some("iso");
    }
    if starts(b"MZ") {
        return Some("exe");
    }

    None
}

/// 给出一个与实际格式匹配的缓存文件名。
///
/// URL 末段通常已经带对了扩展名，此时原样使用；只有在扩展名缺失或明显不是
/// 压缩包后缀时，才用嗅探结果补一个后缀。
///
/// **分卷（`.001` / `.002` …）必须原样保留**：7z 靠 `xxx.7z.001` 这个命名去找
/// 同目录的 `xxx.7z.002`。一旦被重命名成 `xxx.7z.001.7z`，整套分卷就断了。
pub fn cache_file_name(url: &str, sniffed: Option<&str>) -> String {
    const KNOWN: &[&str] = &[
        "7z", "zip", "rar", "iso", "gz", "tgz", "bz2", "xz", "zst", "cab", "001", "exe",
    ];

    let from_url = url
        .rsplit('/')
        .next()
        .map(|segment| segment.split(['?', '#']).next().unwrap_or(segment))
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string());

    if let Some(name) = from_url {
        // 分卷名是不透明的：原样使用，绝不追加后缀。
        if is_volume_part_name(&name) {
            return name;
        }

        let extension = Path::new(&name)
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if KNOWN.contains(&extension.as_str()) {
            return name;
        }
        // 有扩展名但不在已知列表里：保留原名，但仍然按嗅探结果补后缀，
        // 例如 `.../download.php` 实际是 zip → `download.php.zip`。
        if !extension.is_empty() {
            if let Some(kind) = sniffed {
                return format!("{name}.{kind}");
            }
            return name;
        }
        // 完全没有扩展名。
        return match sniffed {
            Some(kind) => format!("{name}.{kind}"),
            None => name,
        };
    }

    match sniffed {
        Some(kind) => format!("solidworks2024sp5.{kind}"),
        None => "solidworks2024sp5.7z".to_string(),
    }
}

/// 判断文件名是否是**分卷**的其中一片（`.001` / `.002` / …）。
///
/// 这个判断很关键：分卷名是不透明的，**绝不能追加后缀**。
/// 例如 `solidworks.7z.001` 若被改成 `solidworks.7z.001.7z`，
/// 7z 就找不到同目录的 `.002`，整个包直接解不开。
///
/// 约定：扩展名是 3~4 位纯数字即视为分卷（`7z a -v` 的默认命名）。
pub fn is_volume_part_name(name: &str) -> bool {
    let Some(extension) = Path::new(name).extension() else {
        return false;
    };
    let ext = extension.to_string_lossy();
    (3..=4).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_digit())
}

