//! 下载取包：单包模式与分片模式。
//!
//! 分片模式的关键约束：7z 的 `a -v` 分卷（`.001/.002/...`）必须**保留原始文件名**
//! 并放在同一目录，只把 `.001` 交给 7z —— 拼接会破坏它的分卷识别。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::collections::BTreeMap;

use crate::downloader::{DownloadEngine, DownloadError, DownloadSettings};

use super::sniff::{cache_file_name, is_volume_part_name, sniff_archive_kind};
use super::super::context::{InstallContext, ReporterSink};
use super::super::paths::humanize_duration;

fn find_cached_archive(cache_dir: &Path, provisional: &Path) -> Option<PathBuf> {
    if provisional.is_file() {
        return Some(provisional.to_path_buf());
    }
    if !cache_dir.is_dir() {
        return None;
    }

    let base = provisional
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let mut candidates: Vec<PathBuf> = std::fs::read_dir(cache_dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            // 排除断点续传的分片与状态文件。
            !name.ends_with(".part") && !name.ends_with(".state") && !name.ends_with(".tmp")
        })
        .collect();

    // 优先同基名（`download.php` 与 `download.php.zip` 视为同一份）。
    candidates.sort();
    if let Some(same_base) = candidates.iter().find(|path| {
        path.file_name()
            .map(|n| {
                let name = n.to_string_lossy().to_ascii_lowercase();
                name == base || name.starts_with(&format!("{base}."))
            })
            .unwrap_or(false)
    }) {
        return Some(same_base.clone());
    }

    None
}

/// 解析本地文件或分卷文件夹，返回交给 7z 的入口文件。
///
/// 文件夹模式只收集直接位于该目录中的数字分卷（`.001/.002/...`），
/// 并拒绝目录内存在多套不同基名的分卷，避免误把其它压缩包拼到一起。
pub fn resolve_local_archive(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if !path.is_dir() {
        return Err(format!("本地路径不存在或不是文件夹：{}", path.display()));
    }

    let mut groups: BTreeMap<String, Vec<(u32, PathBuf)>> = BTreeMap::new();
    for entry in std::fs::read_dir(path)
        .map_err(|error| format!("读取分卷文件夹失败 {}: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| format!("读取分卷目录项失败: {error}"))?;
        let candidate = entry.path();
        if !candidate.is_file() {
            continue;
        }
        let name = candidate
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        let Some(number) = super::filename::volume_number(&name) else {
            continue;
        };
        let base = name
            .rsplit_once('.')
            .map(|(value, _)| value.to_ascii_lowercase())
            .unwrap_or_else(|| name.to_ascii_lowercase());
        groups.entry(base).or_default().push((number, candidate));
    }

    if groups.is_empty() {
        return Err(format!(
            "文件夹中没有找到 .001/.002 等数字分卷：{}",
            path.display()
        ));
    }
    if groups.len() > 1 {
        let names = groups.keys().cloned().collect::<Vec<_>>().join("、");
        return Err(format!(
            "文件夹中发现多套分卷（{names}），请只选择一套分卷所在的文件夹"
        ));
    }

    let mut parts = groups.into_values().next().unwrap_or_default();
    parts.sort_by_key(|(number, _)| *number);
    if let Some((number, entry)) = parts.iter().find(|(number, _)| *number == 1) {
        let _ = number;
        return Ok(entry.clone());
    }
    let (number, entry) = parts
        .first()
        .cloned()
        .ok_or_else(|| "分卷文件夹为空".to_string())?;
    Err(format!(
        "找到分卷但缺少 .001 首卷（最小卷号为 .{number:03}）：{}",
        entry.display()
    ))
}

/// 一个分片的下载计划。
use super::multipart::acquire_multipart_archive;

/// **探测失败不致命**：网盘不支持 HEAD 时仍然能靠 URL 参数拿到名字。
fn resolve_cache_name(ctx: &InstallContext, url: &str, _cache_dir: &Path) -> String {
    // 分卷名是不透明的：绝不能被后续步骤改写。
    if let Some(resolved) = super::filename::resolve_from_url(url) {
        if super::filename::volume_number(&resolved.name).is_some() {
            ctx.debug(format!(
                "URL 已含分卷名 {}（来源：{}），不再探测",
                resolved.name,
                resolved.origin.label()
            ));
            return resolved.name;
        }
    }

    if ctx.config.download.probe_filenames {
        ctx.reporter
            .download_file_status(1, 1, url, 0, 0, 0, "resolving", "探测真实文件名");

        match crate::async_runtime_block_on(crate::downloader::probe_remote(
            url,
            ctx.config.connect_timeout(),
            ctx.config.read_timeout(),
        )) {
            Ok(probe) => {
                if let Some(header) = probe.content_disposition.as_deref() {
                    if let Some(name) = super::filename::parse_content_disposition(header) {
                        ctx.info(format!(
                            "从响应头解析到文件名 {name}（最终地址 {}）",
                            probe.final_url
                        ));
                        return name;
                    }
                }
                if let Some(resolved) = super::filename::resolve_from_url(&probe.final_url) {
                    ctx.info(format!(
                        "从最终地址解析到文件名 {}（来源：{}）",
                        resolved.name,
                        resolved.origin.label()
                    ));
                    return resolved.name;
                }
            }
            Err(error) => {
                // 需要登录是硬错误：继续下载只会存下一个 HTML 登录页。
                if let DownloadError::AuthRequired(detail) = &error {
                    ctx.error(format!(
                        "下载地址需要登录，无法直接获取文件：{detail}\n\
                         请用浏览器登录网盘后重新复制直链，或改用本地模式。"
                    ));
                } else {
                    ctx.debug(format!("文件名探测未成功（不影响下载）：{error}"));
                }
            }
        }
    }

    cache_file_name(url, None)
}

pub fn acquire_archive(ctx: &InstallContext) -> Result<PathBuf, String> {
    if let Some(local) = &ctx.local_archive {
        if local.is_file() || local.is_dir() {
            let archive = resolve_local_archive(local)?;
            if local.is_dir() {
                let volume_count = std::fs::read_dir(local)
                    .ok()
                    .map(|entries| {
                        entries
                            .flatten()
                            .filter(|entry| entry.path().is_file())
                            .count()
                    })
                    .unwrap_or(0);
                ctx.info(format!(
                    "本地分卷模式：使用文件夹 {}，发现约 {} 个文件；7z 入口 {}",
                    local.display(),
                    volume_count,
                    archive.display()
                ));
            } else {
                ctx.info(format!("本地模式：使用 {}", archive.display()));
            }
            let size = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
            // 扩展名与实际格式无关——7z 按文件头识别；这里只是为了日志可读。
            match sniff_archive_kind(&archive) {
                Some(kind) => ctx.info(format!(
                    "文件大小 {size} 字节，按文件头识别为 {kind} 格式，扩展名不参与判定"
                )),
                None => ctx.warn(format!(
                    "文件大小 {size} 字节，文件头未匹配已知压缩格式；7z 将自行判定，若失败会明确报错"
                )),
            }
            ctx.reporter.progress(0.0, format!("本地压缩包 {size} 字节"));
            return Ok(archive);
        }
        return Err(format!(
            "已选择本地压缩包，但路径不存在或不可访问：{}；为避免误下载网络安装包，已停止部署",
            local.display()
        ));
    }

    // ---- 分片模式优先 ---------------------------------------------------
    // 分片 URL 列表非空时走分片流程；此时不参与单包的"缓存复用"逻辑，
    // 因为分片各自判断复用更准确（见 acquire_multipart_archive）。
    let multipart = &ctx.config.download.multipart_urls;
    if !multipart.is_empty() {
        ctx.info(format!(
            "已配置 {} 个分片 URL，进入分片模式（multipart_concat = {}）",
            multipart.len(),
            ctx.config.download.multipart_concat
        ));
        return acquire_multipart_archive(ctx, multipart);
    }

    let url = ctx.config.download.url.trim();
    if url.is_empty() {
        return Err(
            "既未选择本地压缩包，也没有可用的下载地址：\
             [download].url 与 [download].multipart_urls 都是空的"
                .to_string(),
        );
    }

    // 缓存目录先按 URL 原样推导一个候选名。
    // 网盘直链的 URL 里往往没有真文件名，所以先尝试探测，
    // 探到了就用真名（例如 `Downloads.7z.001`），避免存成 `.001.aspx`。
    let cache_dir = ctx.app_data_dir.join("cache");
    let resolved = resolve_cache_name(ctx, url, &cache_dir);
    let provisional = cache_dir.join(&resolved);
    let _ = resolved;

    // 已下载且未配置校验值 → 直接复用，避免重复下载几十 GB。
    // 同时兼容"上次用嗅探后缀命名"与"这次 URL 后缀不同"两种历史缓存。
    if ctx.config.download.checksum_sha256.trim().is_empty() {
        if let Some(existing) = find_cached_archive(&cache_dir, &provisional) {
            let size = std::fs::metadata(&existing).map(|m| m.len()).unwrap_or(0);
            ctx.success(format!(
                "复用已下载的压缩包 {}（{size} 字节）",
                existing.display()
            ));
            return Ok(existing);
        }
    }

    let settings = DownloadSettings {
        url: url.to_string(),
        threads: ctx.config.download.threads.clamp(1, 255) as usize,
        checksum: ctx.config.download.checksum_sha256.clone(),
        retries: ctx.config.download.retry_count,
        backoffs: (1..=ctx.config.download.retry_count.max(1))
            .map(|attempt| ctx.config.retry_backoff(attempt))
            .collect(),
        connect_timeout: ctx.config.connect_timeout(),
        read_timeout: ctx.config.read_timeout(),
    };

    let sink = Arc::new(ReporterSink::new(
        ctx.reporter.clone(),
        Arc::clone(&ctx.cancel),
    ));

    let engine = DownloadEngine::new(
        settings,
        provisional.clone(),
        sink,
        Arc::clone(&ctx.cancel),
    )
    .map_err(|e| e.to_string())?;

    ctx.info(format!("下载模式：{url}"));
    let outcome = crate::async_runtime_block_on(engine.run()).map_err(|error| match error {
        DownloadError::Cancelled => "下载被用户取消".to_string(),
        DownloadError::AuthRequired(detail) => format!(
            "下载地址返回的是登录页而不是文件（这类分享链接需要登录网盘账号）。\n\
             详情：{detail}\n\
             请改用浏览器登录后获取的直链，或把文件先下载到本地再用「本地模式」选择。"
        ),
        other => other.to_string(),
    })?;

    // 下载完成后按文件头识别真实格式，并给缓存文件补上像样的后缀。
    // 7z 本身按文件头识别，不依赖扩展名；这一步是为了日志与后续人工排查。
    //
    // 分卷（`.001`）不参与重命名：改了名字 7z 就找不到同目录的其余分卷。
    let final_path = if is_volume_part_name(
        &outcome
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
    ) {
        ctx.debug("检测到分卷命名，保持原文件名不变（7z 需要据此查找其余分卷）");
        outcome.path.clone()
    } else {
        match sniff_archive_kind(&outcome.path) {
            Some(kind) => {
                ctx.info(format!(
                    "按文件头识别为 {kind} 格式（URL 末段 {}）",
                    outcome
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default()
                ));
                let renamed = cache_dir.join(cache_file_name(url, Some(kind)));
                if renamed != outcome.path && !renamed.exists() {
                    match std::fs::rename(&outcome.path, &renamed) {
                        Ok(()) => {
                            ctx.debug(format!("缓存文件已重命名为 {}", renamed.display()));
                            renamed
                        }
                        Err(error) => {
                            ctx.warn(format!(
                                "重命名缓存文件失败（不影响解压）: {error}；继续使用 {}",
                                outcome.path.display()
                            ));
                            outcome.path.clone()
                        }
                    }
                } else {
                    outcome.path.clone()
                }
            }
            None => {
                ctx.warn(
                    "文件头未匹配已知压缩格式；7z 将自行判定，若无法识别会在第 4 步明确报错",
                );
                outcome.path.clone()
            }
        }
    };

    ctx.success(format!(
        "下载完成 {:.2} GB，耗时 {}，{}",
        outcome.bytes as f64 / 1024.0 / 1024.0 / 1024.0,
        humanize_duration(outcome.elapsed),
        if outcome.verified {
            "SHA-256 校验通过"
        } else {
            "未配置校验值"
        }
    ));

    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::resolve_local_archive;
    use std::fs;
    use std::path::PathBuf;

    fn temp_folder(label: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
            "solidworks-installer-{label}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn resolves_first_volume_from_folder() {
        let dir = temp_folder("volume");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("setup.7z.002"), b"part2").unwrap();
        fs::write(dir.join("setup.7z.001"), b"part1").unwrap();
        assert_eq!(resolve_local_archive(&dir).unwrap(), dir.join("setup.7z.001"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_folder_without_first_volume() {
        let dir = temp_folder("missing-first");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("setup.7z.002"), b"part2").unwrap();
        let error = resolve_local_archive(&dir).unwrap_err();
        assert!(error.contains("缺少 .001"));
        fs::remove_dir_all(dir).unwrap();
    }
}
