//! 大小写不敏感的文件系统查找。
//!
//! **为什么不用 `glob` crate**：`glob` 的匹配是逐字符比较的，在 Windows 上同样是
//! **大小写敏感**的。补丁包的目录名常常与文档不一致（`_SolidSQUAD_` / `_SolidSquad_`、
//! `SolidWorks Corp` / `SOLIDWORKS corp`），用 `glob("**/_solidsquad_")` 会直接漏掉。
//!
//! 因此这里改为**自行遍历目录树 + 逐段不区分大小写比较**，并让通配符（`*` / `?`）
//! 也参与不区分大小写的匹配。遍历深度与结果数都有上限，避免在 `C:\` 这类目录上失控。
//!
use std::path::{Path, PathBuf};
use std::time::Duration;

const WALK_MAX_DEPTH: usize = 24;
/// 单次查找返回的结果上限，防止在超大目录树上耗尽内存。
const WALK_MAX_HITS: usize = 512;

/// 遍历 `root` 下的目录树，返回所有满足 `predicate` 的条目。
///
/// `predicate` 接收 `(文件类型, 文件名)`，只对名字判定，不关心父路径。
pub fn walk_entries<F>(root: &Path, predicate: F) -> Vec<PathBuf>
where
    F: Fn(&std::fs::FileType, &str) -> bool,
{
    let mut hits = Vec::new();
    walk_inner(root, &predicate, 0, &mut hits);
    hits.sort();
    hits
}

fn walk_inner<F>(dir: &Path, predicate: &F, depth: usize, hits: &mut Vec<PathBuf>)
where
    F: Fn(&std::fs::FileType, &str) -> bool,
{
    if depth > WALK_MAX_DEPTH || hits.len() >= WALK_MAX_HITS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        // 无权限或已被删除的子目录：跳过，不影响整体查找。
        return;
    };

    for entry in entries.flatten() {
        if hits.len() >= WALK_MAX_HITS {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();

        if predicate(&file_type, &name) {
            hits.push(entry.path());
        }
        // 目录自身已被匹配时仍然下探，以便一次遍历覆盖多层结果。
        if file_type.is_dir() {
            walk_inner(&entry.path(), predicate, depth + 1, hits);
        }
    }
}

/// 通配符匹配（`*` 匹配任意字符序列，`?` 匹配单个字符），**不区分大小写**。
///
/// 只用于文件名与单层路径段，不是完整的 glob 实现。
pub fn wildcard_ci(pattern: &str, candidate: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let candidate: Vec<char> = candidate.to_lowercase().chars().collect();
    match_wildcard(&pattern, &candidate)
}

pub fn match_wildcard(pattern: &[char], text: &[char]) -> bool {
    // 朴素回溯：`star_at` / `star_match` 记录最近一个 `*` 的位置与已吞掉的字符数。
    let mut p = 0usize;
    let mut t = 0usize;
    let mut star_at: Option<usize> = None;
    let mut star_match = 0usize;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star_at = Some(p);
            star_match = t;
            p += 1;
        } else if let Some(star) = star_at {
            // 让上一个 `*` 多吞一个字符后重试。
            p = star + 1;
            star_match += 1;
            t = star_match;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// 精确名字匹配（不区分大小写）。
pub fn name_matches(pattern: &str, candidate: &str) -> bool {
    candidate.eq_ignore_ascii_case(pattern)
}

/// 在 `root` 的目录树中查找第一个名字等于 `wanted` 的**目录**。
///
/// 找不到时若 `root` 自身的名字就是 `wanted`，返回 `root`。
pub fn find_first_dir(root: &Path, wanted: &str) -> Result<Option<PathBuf>, String> {
    if root.is_dir() {
        let hits = walk_entries(root, |file_type, name| {
            file_type.is_dir() && name_matches(wanted, name)
        });
        if let Some(found) = hits.into_iter().next() {
            return Ok(Some(found));
        }
    }

    // 顶层目录本身可能就是目标。
    let own = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if name_matches(wanted, &own) {
        return Ok(Some(root.to_path_buf()));
    }

    Ok(None)
}

/// 在 `root` 的目录树中查找所有名字等于 `wanted` 的**目录**。
pub fn find_dirs_named(root: &Path, wanted: &str) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    walk_entries(root, |file_type, name| {
        file_type.is_dir() && name_matches(wanted, name)
    })
}

/// 在 `root` 的目录树中查找第一个名字等于 `wanted` 的**文件**。
pub fn find_file_named(root: &Path, wanted: &str) -> Option<PathBuf> {
    find_files_named(root, wanted).into_iter().next()
}

/// 在 `root` 的目录树中查找所有名字等于 `wanted` 的**文件**。
pub fn find_files_named(root: &Path, wanted: &str) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    walk_entries(root, |file_type, name| {
        file_type.is_file() && name_matches(wanted, name)
    })
}

/// 在 `root` 的目录树中查找所有**文件**名匹配通配符 `pattern` 的条目。
pub fn find_files_matching(root: &Path, pattern: &str) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    walk_entries(root, |file_type, name| {
        file_type.is_file() && wildcard_ci(pattern, name)
    })
}

/// 把耗时格式化成「1 分 20 秒」。
pub fn humanize_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    if total < 60 {
        return format!("{total} 秒");
    }
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours} 小时 {minutes} 分 {seconds} 秒")
    } else {
        format!("{minutes} 分 {seconds} 秒")
    }
}
