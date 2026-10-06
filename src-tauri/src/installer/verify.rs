//! 第 9 步：轮询检测安装完成。
//!
//! 日志 / 进程 / 可执行文件三项中满足两项即判定完成；
//! 阈值与间隔全部来自 `[install.polling]`。
//!
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::win32_utils;

use super::context::InstallContext;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use super::paths::find_files_matching;

#[derive(Default)]
struct CompletionSignals {
    log_ok: bool,
    process_ok: bool,
    file_ok: bool,
}

impl CompletionSignals {
    /// 满足的检查项数量。
    fn satisfied(&self, config: &Config) -> usize {
        let mut count = 0usize;
        if config.install.polling.log_check_enabled && self.log_ok {
            count += 1;
        }
        if config.install.polling.process_check_enabled && self.process_ok {
            count += 1;
        }
        if config.install.polling.file_check_enabled && self.file_ok {
            count += 1;
        }
        count
    }

    /// 至少两项满足即判定完成（AGENTS.md 第 9 步）。
    fn is_complete(&self, config: &Config) -> bool {
        let enabled = [
            config.install.polling.log_check_enabled,
            config.install.polling.process_check_enabled,
            config.install.polling.file_check_enabled,
        ]
        .iter()
        .filter(|flag| **flag)
        .count();

        // 只启用了一项检查时，该项即代表全部结论。
        let required = enabled.clamp(1, 2);
        self.satisfied(config) >= required
    }
}

pub fn poll_for_completion(ctx: &InstallContext, install_started: bool) -> Result<(), String> {
    let timeout = ctx.config.poll_timeout();
    let started = Instant::now();
    let candidates = ctx.config.solidworks_exe_candidates();
    let poll = &ctx.config.install.polling;

    // 状态检查的节奏与"安装最长等待"是两件事，必须分开。
    //
    // 实测踩到的坑：`poll_interval_minutes` 被当成状态检查间隔用时是 0.5 分钟，
    // 于是安装器 7 分钟就失败退出，程序却要再等半小时才打印一次状态。
    // 这里把状态检查固定为 5 秒；`poll_interval_minutes` 只用于「多久算一次
    // 完成判定」的宽松节奏（避免高频 IO）。
    let judge_interval = ctx.config.poll_interval();
    let status_interval = Duration::from_secs(5);

    ctx.info(format!(
        "开始轮询检测（判定节奏 {:.2} 分钟，超时 {:.1} 分钟）",
        ctx.config.install.polling.poll_interval_minutes,
        ctx.config.install.polling.timeout_minutes
    ));
    if !install_started {
        ctx.warn("安装器未成功启动，轮询仍会执行以等待可能的后台安装进程");
    }

    if install_started && poll.process_check_enabled && !live_installer_processes().is_empty() {
        ctx.info("安装器进程仍在运行，继续等待安装器退出");
    }

    ctx.info(format!(
        "检测项：日志={} 进程={} 文件={}",
        poll.log_check_enabled, poll.process_check_enabled, poll.file_check_enabled
    ));
    ctx.info(format!(
        "主程序候选位置（按可能性排序，取第一个存在的）：{}",
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" | ")
    ));

    let mut last_status = Instant::now() - status_interval;
    let mut last_judge = Instant::now() - judge_interval;

    loop {
        if ctx.is_cancelled() {
            return Err("安装已被用户取消".to_string());
        }

        // 文件检测：逐个候选做**大小写不敏感**的存在性判断。
        let exe_path = find_existing_exe(&candidates);

        // 状态检查每 5 秒一次，与安装超时无关。
        if last_status.elapsed() >= status_interval {
            last_status = Instant::now();
            let signals = evaluate_completion(ctx, exe_path.as_deref());
            let language_pending = ctx.status.language_pending.load(Ordering::SeqCst);
            ctx.debug(format!(
                "轮询状态 · 日志={} 进程退出={} 可执行文件={} 语言包仍在进行={}",
                signals.log_ok, signals.process_ok, signals.file_ok, language_pending
            ));
            let satisfied = signals.satisfied(&ctx.config) as f64;
            ctx.progress(
                (satisfied / 3.0 * 100.0).min(100.0),
                if language_pending {
                    "等待语言包安装完成"
                } else {
                    "等待安装完成"
                },
            );
            if ctx.status.eta_seconds.load(Ordering::SeqCst) == 0 {
                let elapsed = started.elapsed().as_secs();
                ctx.status.eta_seconds.store(
                    timeout.as_secs().saturating_sub(elapsed),
                    Ordering::SeqCst,
                );
            }
        }

        // 完成判定按 `poll_interval_minutes` 的节奏走（默认 30 秒），
        // 免得高频做日志扫描这类重活。
        if last_judge.elapsed() >= judge_interval {
            last_judge = Instant::now();
            let signals = evaluate_completion(ctx, exe_path.as_deref());
            let language_pending = ctx.status.language_pending.load(Ordering::SeqCst);

            // 语言包还在（或还没开始）时绝不判定完成。
            if language_pending {
                if started.elapsed() >= timeout {
                    return Err(format!(
                        "轮询超时（{} 分钟）：语言包安装仍未结束。\
                         主程序状态 日志={} 进程退出={} 可执行文件={}。\
                         语言包可能卡住，请查看日志确认。",
                        ctx.config.install.polling.timeout_minutes,
                        signals.log_ok,
                        signals.process_ok,
                        signals.file_ok
                    ));
                }
            } else if signals.is_complete(&ctx.config) {
                ctx.success(format!(
                    "安装完成判定通过（日志={} 进程={} 文件={}）",
                    signals.log_ok, signals.process_ok, signals.file_ok
                ));
                return Ok(());
            } else if install_started
                && signals.process_ok
                && !signals.log_ok
                && !signals.file_ok
                && ctx.status.installer_exited.load(Ordering::SeqCst)
            {
                // 安装器**已经退出**，既没有成功日志也没有主程序 →
                // 只能判定失败。早点失败比空等 4 小时有用得多：
                // 实测这次安装器 7 分钟就退出了，程序却空等了 35 分钟。
                return Err(format!(
                    "安装器进程已退出，但既未发现成功日志，也未在以下位置找到 SLDWORKS.exe：\n  {}\n\
                     安装很可能失败。请检查：\n\
                     · 该目录是否存在（若存在但为空，多为权限或磁盘空间问题）\n\
                     · 是否缺少 Windows 前置组件（.NET 4.8 / Visual C++ / WebView2）\n\
                     · 以管理员身份运行本程序\n\
                     · 把 [workdir].cleanup_temp_after 设为 false 后重跑，保留现场排查",
                    candidates
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n  ")
                ));
            }

            if started.elapsed() >= timeout {
                return Err(format!(
                    "轮询超时（{} 分钟）：日志={} 进程退出={} 可执行文件={}",
                    ctx.config.install.polling.timeout_minutes,
                    signals.log_ok,
                    signals.process_ok,
                    signals.file_ok
                ));
            }
        }

        thread::sleep(Duration::from_millis(500));
    }
}

/// 在候选列表里找**实际存在**的主程序，大小写不敏感。
///
/// Windows 上 `Path::is_file()` 本身不区分大小写，
/// 但目录名（`SOLIDWORKS Corp` vs `SOLIDWORKS corp`）与
/// 文件名（`SLDWORKS.exe` vs `sldworks.exe`）都可能与预期不同，
/// 所以这里对每一段再兜一次「列目录 + 忽略大小写比对」。
fn find_existing_exe(candidates: &[PathBuf]) -> Option<PathBuf> {
    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate.clone());
        }
        if let Some(found) = resolve_case_insensitive(candidate) {
            return Some(found);
        }
    }
    None
}

/// 逐段用「列目录 + 忽略大小写比对」把一个路径解析成磁盘上的真实路径。
fn resolve_case_insensitive(path: &Path) -> Option<PathBuf> {
    let mut current = PathBuf::new();
    for part in path.components() {
        use std::path::Component;
        match part {
            Component::Prefix(prefix) => {
                current.push(prefix.as_os_str());
            }
            Component::RootDir => current.push(Component::RootDir.as_os_str()),
            Component::Normal(name) => {
                let wanted = name.to_string_lossy();
                let mut matched = None;
                if let Ok(entries) = std::fs::read_dir(&current) {
                    for entry in entries.flatten() {
                        if entry
                            .file_name()
                            .to_string_lossy()
                            .eq_ignore_ascii_case(&wanted)
                        {
                            matched = Some(entry.path());
                            break;
                        }
                    }
                }
                match matched {
                    Some(found) => current = found,
                    // 直拼也试一次（正常大小写时走这条）
                    None => {
                        let direct = current.join(name);
                        if direct.exists() {
                            current = direct;
                        } else {
                            return None;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    current.is_file().then_some(current)
}

fn evaluate_completion(
    ctx: &InstallContext,
    exe_path: Option<&Path>,
) -> CompletionSignals {
    let config = &ctx.config;
    let mut signals = CompletionSignals::default();

    if config.install.polling.log_check_enabled {
        signals.log_ok = scan_install_logs(ctx);
    }

    if config.install.polling.process_check_enabled {
        // 安装进程已退出（或从未驻留）视为满足。
        let alive = live_installer_processes();
        signals.process_ok = alive.is_empty();
        if !alive.is_empty() {
            ctx.debug(format!("仍在运行的安装进程: {}", alive.join(", ")));
        }
    }

    if config.install.polling.file_check_enabled {
        // `None` 表示所有候选位置都还没出现主程序。
        signals.file_ok = exe_path.map(|p| p.is_file()).unwrap_or(false);
    }

    signals
}

/// 正在运行的安装器进程名（用于判定“进程已退出”）。
fn live_installer_processes() -> Vec<String> {
    const PATTERNS: &[&str] = &[
        "StartSWInstall.exe",
        "setup.exe",
        "msiexec.exe",
        "sldworkssetup.exe",
        "swinstallmanager",
    ];
    win32_utils::list_processes()
        .into_iter()
        .map(|(_, name)| name)
        .filter(|name| {
            PATTERNS
                .iter()
                .any(|pattern| name.eq_ignore_ascii_case(pattern) || name.to_ascii_lowercase().contains(&pattern.to_ascii_lowercase().replace(".exe", "")))
        })
        .collect()
}

/// 扫描 SolidWorks 安装日志，查找成功标记。
fn scan_install_logs(ctx: &InstallContext) -> bool {
    const MARKERS: &[&str] = &[
        "安装成功",
        "Installation succeeded",
        "installation succeeded",
        "Setup completed successfully",
        "Installation completed successfully",
        "成功完成安装",
    ];
    const LOG_PATTERNS: &[&str] = &[
        "*.log",
        "*.txt",
        "*.html",
        "*.htm",
    ];

    // 日志位置：工作目录与安装目录下的常见日志目录。
    let mut roots: Vec<PathBuf> = vec![ctx.workdir.clone()];
    let install_dir = ctx.config.resolved_install_path();
    roots.push(install_dir.clone());
    roots.push(install_dir.join("SOLIDWORKS"));
    if let Ok(temp) = std::env::var("TEMP") {
        roots.push(PathBuf::from(temp));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(PathBuf::from(&local).join("Temp"));
        roots.push(PathBuf::from(&local).join("SOLIDWORKS"));
    }

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for pattern in LOG_PATTERNS {
            // 日志目录同样按不区分大小写匹配（安装器可能输出 .LOG / .TXT）。
            let mut files: Vec<PathBuf> = find_files_matching(&root, pattern)
                .into_iter()
                .filter(|path| {
                    path.metadata()
                        .map(|meta| meta.len() < 32 * 1024 * 1024)
                        .unwrap_or(false)
                })
                .collect();
            files.sort();
            files.reverse();

            for file in files.into_iter().take(12) {
                if log_contains_marker(&file, MARKERS) {
                    ctx.debug(format!("日志命中成功标记: {}", file.display()));
                    return true;
                }
            }
        }
    }

    false
}

fn log_contains_marker(path: &Path, markers: &[&str]) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    // 只读尾部 256 KiB：安装日志的关键结论都在末尾。
    let window = if bytes.len() > 256 * 1024 {
        &bytes[bytes.len() - 256 * 1024..]
    } else {
        &bytes[..]
    };

    let text = crate::decode_console_bytes(window);
    markers.iter().any(|marker| text.contains(marker))
}
