//! 分片（分卷）下载。
//!
//! # 分片语义（关键，别搞反）
//!
//! 7z / NanaZip 的 `a -v100m` 产出的是**分卷**：`x.7z.001`、`x.7z.002`…
//! 7z 只要拿到 `.001` 就会**自动读取同目录的整套分卷**。因此程序必须
//! **原样保留每个分片的名字**、让它们落在同一目录，最后把 `.001` 交给 7z ——
//! **绝不能拼接**，拼成一个大文件 7z 反而不认识。
//!
//! 若各分片只是「把一个文件任意切成几段」（不是 7z 分卷），7z 无法识别，
//! 此时需要把 `multipart_concat` 设为 `true`。
//!
//! # 为什么必须先解析文件名
//!
//! 网盘直链的路径里常常没有文件名，而是塞在查询参数里，还挂着 `.aspx` 假后缀。
//! 不解析就会存成 `Downloads.7z.001.aspx`；7z 虽然能按文件头识别内容，
//! 但**找不到同目录的 `.002`**，整套分卷直接解不开。
//!
//! # 并发
//!
//! 各文件同时下载，并发数由 `[download].concurrent_files` 控制（默认 4）。
//! 每个文件内部再按 `[download].threads` 切块（默认 32），
//! 两者相乘就是峰值连接数。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::downloader::{DownloadEngine, DownloadError, DownloadSettings};

use super::sniff::sniff_archive_kind;
use super::super::context::{InstallContext, ReporterSink};

struct PartPlan {
    /// 原始 URL
    url: String,
    /// 解析出的真实文件名
    name: String,
    /// 目标路径
    target: PathBuf,
    /// 命名来源（写日志用）
    origin: String,
    /// 尺寸未知时为 0
    size: u64,
}

/// 分片下载：**先解析真实文件名，再并发下载**，返回「应当交给 7z 的那个文件」。
///
/// # 分片语义（关键，别搞反）
///
/// 7z / NanaZip 的 `a -v100m` 产出的是**分卷**：`x.7z.001`、`x.7z.002`…
/// 7z 只要拿到 `.001` 就会**自动读取同目录的整套分卷**。因此程序必须
/// **原样保留每个分片的名字**、让它们落在同一目录，最后把 `.001` 交给 7z ——
/// **绝不能拼接**，拼成一个大文件 7z 反而不认识。
///
/// 若各分片只是"把一个文件任意切成几段"（不是 7z 分卷），7z 无法识别，
/// 此时需要把 `multipart_concat` 设为 `true`。
///
/// # 为什么必须先解析文件名
///
/// 网盘直链的路径里常常没有文件名，而是塞在查询参数里，还挂着 `.aspx` 假后缀：
///
/// ```text
/// .../download.aspx?SourceUrl=%2Fpersonal%2F...%2FDownloads%2E7z%2E002
/// ```
///
/// 不解析就会存成 `Downloads.7z.001.aspx`。**7z 虽然能按文件头识别内容，
/// 但 `x.001.aspx` 这种名字会让它找不到同目录的 `.002`**，整套分卷直接解不开。
/// 所以这里先解析出 `Downloads.7z.002`，再按这个名字落盘。
///
/// # 并发
///
/// 各文件同时下载，并发数由 `[download].concurrent_files` 控制（默认 4）。
/// 每个文件内部再按 `[download].threads` 切块（默认 32），
/// 两个维度相乘就是峰值连接数，注意别把出口带宽或网盘限流打爆。
pub fn acquire_multipart_archive(ctx: &InstallContext, urls: &[String]) -> Result<PathBuf, String> {
    let cache_dir = ctx.app_data_dir.join("cache");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("创建缓存目录失败 {}: {e}", cache_dir.display()))?;

    let total = urls.len();
    let concurrency = ctx.config.download.concurrent_files.clamp(1, 32) as usize;
    ctx.info(format!(
        "分片模式：共 {total} 个分片，并发 {concurrency} 个文件 × 每文件 {} 线程",
        ctx.config.download.threads
    ));

    // ---------- 1. 解析阶段：为每个 URL 定出真实文件名 -------------------
    let mut plans: Vec<PartPlan> = Vec::with_capacity(total);
    for (index, url) in urls.iter().enumerate() {
        if ctx.is_cancelled() {
            return Err("分片下载被用户取消".to_string());
        }
        ctx.reporter
            .download_file_status(index + 1, total, url, 0, 0, 0, "resolving", "解析下载地址");

        let plan = plan_part(ctx, url, index, total, &cache_dir);
        ctx.info(format!(
            "[{}/{total}] {} ← {} 字节（命名来源：{}）",
            index + 1,
            plan.name,
            if plan.size > 0 {
                plan.size.to_string()
            } else {
                "未知".to_string()
            },
            plan.origin
        ));
        plans.push(plan);
    }

    // ---------- 2. 分卷编号补齐 -----------------------------------------
    // 网盘的 SourceUrl 对每个分片都可能是同一个泛化名（例如都叫 `Downloads.7z`），
    // 这时按输入顺序补上 `.001/.002/...`，否则后下载的会把先下载的覆盖掉。
    normalize_volume_names(ctx, &mut plans);

    // ---------- 3. 下载阶段：并发执行 -----------------------------------
    let ordered = download_parts(ctx, plans, concurrency)?;

    // ---------- 4. 拼接模式（非标准切分才用） ---------------------------
    if ctx.config.download.multipart_concat {
        return concat_parts(ctx, &cache_dir, &ordered);
    }

    // ---------- 5. 分卷模式（默认）：挑出交给 7z 的入口 ------------------
    let entry = pick_volume_entry(&ordered);
    match &entry {
        Some(path) => ctx.success(format!(
            "分片下载完成；7z 将从 {} 读取整套分卷（同目录共 {} 个文件）",
            path.display(),
            ordered.len()
        )),
        None => ctx.warn(
            "分片文件名不符合 .001/.002 分卷约定，将把第一个分片交给 7z；\
             若解压失败请把 multipart_concat 设为 true",
        ),
    }

    Ok(entry.unwrap_or_else(|| {
        ordered
            .first()
            .cloned()
            .unwrap_or_else(|| cache_dir.join("part001"))
    }))
}

/// 为一个 URL 定出文件名与落盘路径。
///
/// 顺序：HEAD 探测（`Content-Disposition` + 最终地址）→ URL 参数 → URL 路径 → 兜底。
/// 探测失败不致命：网盘不支持 HEAD 时仍然可以靠 URL 参数拿到名字。
fn plan_part(
    ctx: &InstallContext,
    url: &str,
    index: usize,
    total: usize,
    cache_dir: &Path,
) -> PartPlan {
    // a) HEAD 探测，拿服务器声明的文件名与尺寸
    let probe = crate::async_runtime_block_on(crate::downloader::probe_remote(
        url,
        ctx.config.connect_timeout(),
        ctx.config.read_timeout(),
    ));

    let (header_name, size) = match &probe {
        Ok(probe) => (
            probe
                .content_disposition
                .as_deref()
                .and_then(super::filename::parse_content_disposition),
            probe.length,
        ),
        Err(error) => {
            // 需要登录是"确定失败"，直接上报，让用户看到真实原因；
            // 其他探测错误（例如服务端不支持 HEAD）只是拿不到尺寸，不影响下载。
            let detail = error.to_string();
            ctx.reporter.download_file_status(
                index + 1,
                total,
                url,
                0,
                0,
                0,
                "resolving",
                &detail,
            );
            ctx.debug(format!("[{}/{}] 探测未成功：{detail}", index + 1, total));
            (None, 0)
        }
    };

    // b) 决定最终名字
    let (name, origin) = if let Some(name) = header_name {
        (name, "Content-Disposition".to_string())
    } else if super::filename::is_share_short_link(url) {
        // 短链（1drv.ms 等）的路径末段是分享令牌，当文件名毫无意义。
        // 这里用中性兜底名，随后 normalize_volume_names 会补上分卷号。
        (format!("part{index:03}"), "短链兜底命名".to_string())
    } else if let Some(resolved) = super::filename::resolve_from_url(url) {
        let label = resolved.origin.label().to_string();
        (resolved.name, label)
    } else {
        // c) 兜底：连 URL 都推不出来时，用顺序号命名。
        //    这样至少不会互相覆盖；分卷编号会在下一步补齐。
        (format!("part{index:03}"), "兜底命名".to_string())
    };

    let target = cache_dir.join(&name);
    PartPlan {
        url: url.to_string(),
        name,
        target,
        origin,
        size,
    }
}

/// 让分卷编号连续且唯一。
///
/// 两种要修的情况：
///
/// 1. **多片同名**（网盘对每片返回同一个泛化名）→ 按输入顺序补 `.001/.002/…`；
/// 2. **名字里已带分卷号但重复/缺口**（如两片都是 `.001`）→ 按序号重排。
///
/// 只在**确实像分卷**时才动名字：普通的多个独立文件不应该被改掉。
fn normalize_volume_names(ctx: &InstallContext, plans: &mut [PartPlan]) {
    if plans.len() < 2 {
        return;
    }

    let names: Vec<String> = plans.iter().map(|plan| plan.name.clone()).collect();
    let all_volumes = names
        .iter()
        .all(|name| super::filename::volume_number(name).is_some());
    let duplicates = {
        let mut sorted = names.clone();
        sorted.sort();
        sorted.windows(2).any(|pair| pair[0] == pair[1])
    };

    // 情况 1：所有名字都是分卷，且有重复 → 按顺序重排编号，基底取第一个。
    if all_volumes && duplicates {
        if let Some(base) = super::filename::volume_base(&names) {
            ctx.warn(format!(
                "检测到分片文件名重复（网盘常对每片返回同一泛化名），\
                 将按配置顺序重编为 {base}.001…{base}.{:03}",
                plans.len()
            ));
            for (index, plan) in plans.iter_mut().enumerate() {
                let new_name = super::filename::set_volume_number(&base, index as u32 + 1);
                plan.target = plan.target.with_file_name(&new_name);
                plan.name = new_name;
                plan.origin = "按顺序补齐分卷号".to_string();
            }
            return;
        }
    }

    // 情况 2：一片都没有分卷号，但数量 > 1 —— 很可能是"分卷但名字被网盘抹平了"。
    // 只在整组名字完全一致时才这样处理，避免误伤多个独立文件。
    if !all_volumes {
        let same = names.windows(2).all(|pair| pair[0] == pair[1]);
        // 允许 `Downloads.7z`（压缩包基底）或 `part001` 这类兜底名。
        let replaceable = same
            && (super::filename::volume_base(&names).is_some()
                || names[0].starts_with("part"));
        if replaceable {
            let base = if let Some(base) = super::filename::volume_base(&names) {
                base
            } else {
                // 兜底名：把 `part001` 归一成 `part` 再编号。
                "part".to_string()
            };
            ctx.warn(format!(
                "所有分片解析出同一个名字，按配置顺序补上分卷号 → {base}.001…{base}.{:03}",
                plans.len()
            ));
            for (index, plan) in plans.iter_mut().enumerate() {
                let new_name = super::filename::set_volume_number(&base, index as u32 + 1);
                plan.target = plan.target.with_file_name(&new_name);
                plan.name = new_name;
                plan.origin = "按顺序补齐分卷号".to_string();
            }
        }
    }
}

/// 并发下载所有分片，返回按输入顺序排列的路径。
///
/// 用一个 `block_on` 驱动 `FuturesUnordered`：编排线程本身没有 async 上下文，
/// 但这样可以让所有文件的下载任务在同一段运行时里并发推进。
fn download_parts(
    ctx: &InstallContext,
    plans: Vec<PartPlan>,
    concurrency: usize,
) -> Result<Vec<PathBuf>, String> {
    use futures_util::stream::{FuturesUnordered, StreamExt};

    let total = plans.len();
    let skip_checksum = ctx.config.download.checksum_sha256.trim().is_empty();

    // 已存在且无需校验的分片直接复用，不占并发额度。
    let mut ordered: Vec<Option<PathBuf>> = vec![None; total];
    let mut pending: Vec<(usize, PartPlan)> = Vec::new();

    for (index, plan) in plans.into_iter().enumerate() {
        if skip_checksum && plan.target.is_file() {
            let size = std::fs::metadata(&plan.target).map(|m| m.len()).unwrap_or(0);
            ctx.info(format!(
                "[{}/{total}] 复用已下载分片 {}（{size} 字节）",
                index + 1,
                plan.name
            ));
            ctx.reporter.download_file_status(
                index + 1,
                total,
                &plan.name,
                size,
                size,
                0,
                "skipped",
                "已存在，跳过下载",
            );
            ordered[index] = Some(plan.target);
        } else {
            pending.push((index, plan));
        }
    }

    if pending.is_empty() {
        return Ok(ordered.into_iter().flatten().collect());
    }

    ctx.info(format!(
        "开始下载 {} 个分片（并发 {concurrency}）",
        pending.len()
    ));

    let failures: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let results: std::sync::Mutex<Vec<(usize, PathBuf)>> = std::sync::Mutex::new(Vec::new());

    crate::async_runtime_block_on(async {
        let mut queue = pending.into_iter();
        let mut running = FuturesUnordered::new();

        // 先填满并发额度
        for _ in 0..concurrency {
            match queue.next() {
                Some(item) => running.push(download_one(ctx, item, total)),
                None => break,
            }
        }

        while let Some(finished) = running.next().await {
            match finished {
                Ok((index, path)) => {
                    results.lock().map(|mut guard| guard.push((index, path))).ok();
                }
                Err(message) => {
                    failures.lock().map(|mut guard| guard.push(message)).ok();
                }
            }
            // 一个完成就补一个，保持并发度
            if let Some(item) = queue.next() {
                running.push(download_one(ctx, item, total));
            }
        }
    });

    let failed = failures.into_inner().unwrap_or_default();
    if !failed.is_empty() {
        return Err(format!(
            "有 {} 个分片下载失败：\n{}",
            failed.len(),
            failed.join("\n")
        ));
    }

    for (index, path) in results.into_inner().unwrap_or_default() {
        if let Some(slot) = ordered.get_mut(index) {
            *slot = Some(path);
        }
    }

    Ok(ordered.into_iter().flatten().collect())
}

/// 下载单个分片（供并发调度使用）。
async fn download_one(
    ctx: &InstallContext,
    item: (usize, PartPlan),
    total: usize,
) -> Result<(usize, PathBuf), String> {
    let (index, plan) = item;
    let display_index = index + 1;

    // 并发下载时，逐文件进度由 downloader 的分块回调驱动；
    // 这里先标记进入下载阶段。
    ctx.reporter.download_file_status(
        display_index,
        total,
        &plan.name,
        0,
        plan.size,
        0,
        "downloading",
        &plan.url,
    );

    let settings = DownloadSettings {
        url: plan.url.clone(),
        threads: ctx.config.download.threads.clamp(1, 255) as usize,
        // 分片各自校验没有意义（校验值针对合并后的整体），因此清空。
        checksum: String::new(),
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
        plan.target.clone(),
        sink,
        Arc::clone(&ctx.cancel),
    )
    .map_err(|e| format!("[{display_index}/{total}] {} 初始化失败: {e}", plan.name))?;

    let outcome = crate::async_runtime_block_on(engine.run()).map_err(|error| {
        let reason = match error {
            DownloadError::Cancelled => "被用户取消".to_string(),
            other => other.to_string(),
        };
        format!("[{display_index}/{total}] {} 下载失败: {reason}", plan.name)
    });

    match outcome {
        Ok(outcome) => {
            ctx.success(format!(
                "[{display_index}/{total}] 完成 {}（{:.1} MB）",
                plan.name,
                outcome.bytes as f64 / 1024.0 / 1024.0
            ));
            ctx.reporter.download_file_status(
                display_index,
                total,
                &plan.name,
                outcome.bytes,
                outcome.bytes,
                0,
                "done",
                "",
            );
            Ok((index, plan.target))
        }
        Err(message) => {
            ctx.reporter.download_file_status(
                display_index,
                total,
                &plan.name,
                0,
                plan.size,
                0,
                "failed",
                &message,
            );
            Err(message)
        }
    }
}

/// 把各分片按顺序拼接成单个文件（`multipart_concat = true` 时使用）。
fn concat_parts(
    ctx: &InstallContext,
    cache_dir: &Path,
    parts: &[PathBuf],
) -> Result<PathBuf, String> {
    let merged = cache_dir.join("multipart-merged.bin");
    ctx.info(format!(
        "multipart_concat = true：按顺序拼接 {} 个分片 → {}",
        parts.len(),
        merged.display()
    ));

    let mut output =
        std::fs::File::create(&merged).map_err(|e| format!("创建合并文件失败: {e}"))?;
    let mut total = 0u64;

    for (index, part) in parts.iter().enumerate() {
        if ctx.is_cancelled() {
            let _ = std::fs::remove_file(&merged);
            return Err("分片拼接被用户取消".to_string());
        }
        let mut input = std::fs::File::open(part)
            .map_err(|e| format!("打开分片失败 {}: {e}", part.display()))?;
        let written =
            std::io::copy(&mut input, &mut output).map_err(|e| format!("写入合并文件失败: {e}"))?;
        total += written;
        ctx.debug(format!(
            "  · [{}/{}] {} → {written} 字节",
            index + 1,
            parts.len(),
            part.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
    }

    output
        .sync_all()
        .map_err(|e| format!("刷新合并文件失败: {e}"))?;
    drop(output);

    // 拼接完成后按文件头给合并文件一个像样的后缀（7z 按内容识别，后缀只影响可读性）。
    let final_path = match sniff_archive_kind(&merged) {
        Some(kind) => {
            let renamed = cache_dir.join(format!("multipart-merged.{kind}"));
            let _ = std::fs::remove_file(&renamed);
            std::fs::rename(&merged, &renamed)
                .map_err(|e| format!("重命名合并文件失败: {e}"))?;
            renamed
        }
        None => merged,
    };

    ctx.success(format!(
        "分片拼接完成，共 {:.2} GB（{}）",
        total as f64 / 1024.0 / 1024.0 / 1024.0,
        final_path.display()
    ));
    Ok(final_path)
}

/// 从已下载的分片里挑出 7z 应当读取的入口文件。
///
/// 优先 `.001`；否则取编号最小的纯数字后缀分片。
fn pick_volume_entry(parts: &[PathBuf]) -> Option<PathBuf> {
    if let Some(found) = parts.iter().find(|path| {
        path.extension()
            .map(|ext| ext.to_string_lossy() == "001")
            .unwrap_or(false)
    }) {
        return Some(found.clone());
    }

    let mut numbered: Vec<(u32, PathBuf)> = parts
        .iter()
        .filter_map(|path| {
            let ext = path.extension()?.to_string_lossy().to_string();
            if (3..=4).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_digit()) {
                ext.parse::<u32>().ok().map(|n| (n, path.clone()))
            } else {
                None
            }
        })
        .collect();
    numbered.sort_by_key(|(number, _)| *number);
    numbered.into_iter().next().map(|(_, path)| path)
}
