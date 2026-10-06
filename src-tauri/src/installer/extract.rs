//! 第 4、5 步：两阶段解压。
//!
//! 主包解压 → 匹配文件名含 `solidsquad` 的压缩包 → 二次解压得到 `_SolidSQUAD_`。
//! **不传 `-t<格式>`**：7z 按文件头识别，扩展名无关。
//!
use std::path::{Path, PathBuf};
use std::time::Duration;
use crate::process_utils;

use super::archive::sniff_archive_kind;
use super::context::InstallContext;
use super::paths;
use crate::process_utils::CommandOutcome;
use super::paths::humanize_duration;

pub fn extract_archive(
    ctx: &InstallContext,
    archive: &Path,
    destination: &Path,
    pass: usize,
) -> Result<(), String> {
    ctx.info(format!(
        "第 {pass} 阶段解压 {} → {}",
        archive.display(),
        destination.display()
    ));
    if let Some(kind) = sniff_archive_kind(archive) {
        ctx.debug(format!("7z 将按文件头以 {kind} 格式解压（扩展名不参与判定）"));

        // 内置的 `7zr.exe` 是 **7-Zip Extra 精简版**，只支持
        // `7z` / `Split(.001)` / `lzma` / `xz`（实测 `7zr i` 输出）。
        // 它**不能**解 zip / rar / gz / bz2 / cab / iso / chm / zstd。
        //
        // 自己的 SolidWorks 介质是 7z 分卷，所以这个限制通常不触发；
        // 但用户换成 zip/rar/iso 时，若不给提示就会看到一句很难懂的
        // "Cannot open the file as archive"，白白排查半天。
        if is_limited_seven_zip(&ctx.seven_zip) && !SEVEN_ZR_FORMATS.contains(&kind) {
            ctx.warn(format!(
                "检测到压缩包是 {kind} 格式，而当前使用的 7z 程序是精简版 7zr.exe，\
                 **它只支持 7z / 分卷(.001) / lzma / xz**，无法解 {kind}。\n\
                 两条出路：\n\
                 · 把完整版 7z.exe 放到程序同级目录或 src-tauri\\resources\\ 下\
                   （会自动优先使用）；\n\
                 · 或在 [sevenzip].exe_path 里指定完整版 7z.exe 的绝对路径。\n\
                 另外可把 [sevenzip].download_url 换成完整版 7z.exe 的地址，\
                 让自动下载拿到完整版。"
            ));
        }
    }

    let mut command = process_utils::hidden_command(&ctx.seven_zip);
    command.args([
        "x",
        &archive.to_string_lossy(),
        &format!("-o{}", destination.display()),
        "-aoa",
        "-bb1",
        "-y",
    ]);

    let timeout = ctx.config.extract_timeout();
    let reporter = ctx.reporter.clone();
    let archive_name = archive
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let outcome = process_utils::run_command_streaming(command, timeout, Some(&ctx.cancel), move |line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }
        let percent = trimmed
            .split_whitespace()
            .find_map(|part| part.strip_suffix('%')?.parse::<f64>().ok());
        let current = trimmed
            .strip_prefix("-")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&archive_name);
        reporter.extract_progress(percent.unwrap_or(0.0), current, pass);
    })
        .map_err(|e| format!("调用 7z 失败: {e}"))?;

    emit_extract_progress(ctx, &outcome, pass, archive);

    if ctx.is_cancelled() {
        return Err("解压被用户取消".to_string());
    }
    if outcome.timed_out {
        return Err(format!(
            "解压超时（超过 {} 分钟）: {}",
            ctx.config.sevenzip.extract_timeout_minutes,
            archive.display()
        ));
    }
    if !outcome.is_success() {
        return Err(format!(
            "7z 解压失败（code={:?}）: {}",
            outcome.code,
            outcome.tail(6)
        ));
    }

    ctx.success(format!(
        "第 {pass} 阶段解压完成，耗时 {}",
        humanize_duration(Duration::from_secs_f64(outcome.elapsed_seconds))
    ));
    Ok(())
}

/// 第 5 步：找出文件名含 `solidsquad` 的压缩包（扩展名不限）并二次解压。
///
/// 匹配规则见 `docs/INSTALL_FLOW.md`：
///
/// | 会被匹配 |
/// |---|
/// | `SolidSQUAD_Patch.7z` / `solidsquad.rar` / `_SolidSquad_.zip` |
/// | `SolidSQUAD_Patch`（无扩展名） |
///
/// 用 `find_files_matching(folder1, "*solid*squad*")` —— 自实现的通配符匹配，
/// **不区分大小写**。
///
/// > 搜索发生在二次解压**之前**，所以不会误排除 `_SolidSQUAD_/` 里的路径 ——
/// > 补丁包本身常常就放在这样的目录名下。
///
/// 返回解压得到的 `_SolidSQUAD_` 目录。
pub fn extract_solidsquad(ctx: &InstallContext, folder1: &Path) -> Result<PathBuf, String> {
    // 某些发行包的 `_SolidSQUAD_.7z` 解压结果已经直接展开为最终目录，
    // 顶层包含 .reg、SOLIDWORKS Corp 和 SolidWorks_Flexnet_Server。
    // 这种结构无需再次解压，直接作为补丁根目录使用。
    if is_ready_solid_squad_root(folder1) {
        ctx.info(format!(
            "发现已展开的补丁目录结构：{}；跳过第二次解压",
            folder1.display()
        ));
        return Ok(folder1.to_path_buf());
    }

    let candidates = paths::find_files_matching(folder1, "*solid*squad*");

    let patch = candidates.first().ok_or_else(|| {
        format!(
            "在 {} 中未找到文件名含 solidsquad 的压缩包（扩展名不限）。\
             请确认下载的压缩包内包含 _SolidSQUAD_ 补丁包。",
            folder1.display()
        )
    })?;

    if candidates.len() > 1 {
        ctx.warn(format!(
            "找到 {} 个候选补丁包，使用第一个：{}（其余：{}）",
            candidates.len(),
            patch.display(),
            candidates[1..]
                .iter()
                .map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("、")
        ));
    }

    let folder2 = ctx.workdir.join("folder2");
    std::fs::create_dir_all(&folder2)
        .map_err(|e| format!("创建二次解压目录 {} 失败: {e}", folder2.display()))?;

    extract_archive(ctx, patch, &folder2, 2)?;

    // 补丁包可能套了一层目录，递归找 _SolidSQUAD_（本身不区分大小写）。
    let squad = paths::find_first_dir(&folder2, "_solidsquad_")?.or_else(|| {
        if is_ready_solid_squad_root(&folder2) {
            Some(folder2.clone())
        } else {
            None
        }
    }).ok_or_else(|| {
        format!(
            "二次解压后在 {} 中未找到 _SolidSQUAD_ 目录或可识别的补丁目录结构",
            folder2.display()
        )
    })?;

    ctx.success(format!("补丁目录：{}", squad.display()));
    Ok(squad)
}

/// 内置精简版 `7zr.exe` 实际支持的格式（实测 `7zr i` 输出）。
///
/// 同时列出嗅探名与 7z 自己的格式名，避免两边叫法不同导致漏判。
const SEVEN_ZR_FORMATS: &[&str] = &["7z", "xz", "lzma", "lzma86"];

/// 判断给定的 7z 可执行文件是不是那个精简版 `7zr`。
fn is_limited_seven_zip(program: &Path) -> bool {
    program
        .file_name()
        .map(|name| name.to_string_lossy().eq_ignore_ascii_case("7zr.exe"))
        .unwrap_or(false)
}

/// 从 7z 的 `-bb1` 输出里解析出最近处理的文件名，推给前端。
fn emit_extract_progress(
    ctx: &InstallContext,
    outcome: &CommandOutcome,
    pass: usize,
    archive: &Path,
) {
    let current_file = outcome
        .output
        .lines()
        .rev()
        .find(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("- ") || trimmed.starts_with("Extracting")
        })
        .map(|line| line.trim_start_matches("- ").trim().to_string())
        .unwrap_or_else(|| archive.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());

    // 7z 只在结束时给出确定的总量，这里的百分比按阶段权重推进。
    let percent = if pass == 1 { 50.0 } else { 100.0 };
    ctx.reporter.extract_progress(percent, &current_file, pass);
    ctx.status.set_percent(percent * 0.5 + 25.0);
}

fn is_ready_solid_squad_root(root: &Path) -> bool {
    let reg_files = paths::find_files_matching(root, "*.reg");
    let has_sw = !paths::find_dirs_named(root, "SOLIDWORKS Corp").is_empty();
    let has_flexnet = !paths::find_dirs_named(root, "SolidWorks_Flexnet_Server").is_empty()
        || paths::find_file_named(root, "server_install.bat").is_some();
    !reg_files.is_empty() && (has_sw || has_flexnet)
}
