//! 安全软件检测与处置。
//!
//! # 为什么需要这一步
//!
//! Windows Defender 与多数第三方杀软会把 `_SolidSQUAD_` 里的补丁文件判为威胁
//! 并**直接删除**，或者把 `rld.dll` / `sldworks.exe` 这类被替换的文件锁住，
//! 导致安装走到一半失败、且失败原因很难看出。所以编排在**解压之前**先处置杀软。
//!
//! # 检测
//!
//! 用 WMI 的 `root\SecurityCenter2` → `AntiVirusProduct`。这是 Windows 安全中心
//! 自己用的注册表，任何在系统里"登记过"的杀软都会出现在这里，比枚举服务或
//! 猜进程名可靠得多。
//!
//! # Defender 的移除
//!
//! 复用随仓库提供的 [windows-defender-remover]（LGPL，见其 LICENSE）：
//!
//! 1. `PowerRun.exe powershell ... RemoveSecHealthApp.ps1` —— 移除 Windows 安全中心 UWP 应用
//! 2. `PowerRun.exe regedit /s <每个 .reg>` —— 导入 `Remove_Defender/` 与
//!    `Remove_SecurityComp/` 下的全部策略注册表项
//! 3. 删除 SmartScreen 相关文件
//!
//! **刻意不调用 `Script_Run.ps1` 的 `y` / `a` 分支**：那两个分支的最后一步是
//! `shutdown /r /f /t 10`，会强制重启机器并直接打断我们的安装流程。
//! 这里只做与它相同的动作，**重启交给用户自己决定**（见 `warn_reboot_after_removal`）。
//!
//! # 第三方杀软
//!
//! 按 `[antivirus].third_party_mode` 分级处理：
//! `prompt` 只提示、`terminate` 终止进程/停服务、`uninstall` 再尝试静默卸载。
//! 任何一步失败都会收集成"需要用户手动从托盘退出"的清单回传前端，
//! 由用户自己处理——我们不会假装成功。
//!
//! [windows-defender-remover]: https://github.com/ionuttbara/windows-defender-remover

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::config::AntivirusConfig;
use crate::events::LogLevel;
use crate::process_utils;
use crate::win32_utils;

/// 已注册的安全软件条目。
#[derive(Debug, Clone, Serialize)]
pub struct SecurityProduct {
    /// 显示名，例如 `Windows Defender`、`Kaspersky`。
    pub display_name: String,
    /// `productState` 原始值（十六进制可解读启用/更新状态）。
    pub product_state: u32,
    /// 是否处于"开启且实时防护生效"状态。
    pub enabled: bool,
    /// 是否已是最新定义。
    pub up_to_date: bool,
    /// 是否为 Windows Defender（内置）。
    pub is_defender: bool,
    /// 厂商注册的路径，用于反查进程/服务。
    pub exe_path: String,
}

/// 一个安全软件的处置结论。
#[derive(Debug, Clone, Serialize)]
pub struct AvOutcome {
    pub display_name: String,
    pub is_defender: bool,
    /// 处置动作的简述，例如 "已移除"、"已终止 2 个进程"。
    pub action: String,
    /// 是否已确认不再生效。
    pub resolved: bool,
    /// 需要用户手动处理时给出的具体指引（已退出则为空）。
    pub manual_hint: String,
}

/// 整步的汇总结果。
#[derive(Debug, Clone, Serialize)]
pub struct AntivirusOutcome {
    /// 检测到的全部安全软件。
    pub detected: Vec<SecurityProduct>,
    /// 逐个的处置结论。
    pub outcomes: Vec<AvOutcome>,
    /// 需要用户手动从托盘退出的软件名（前端要高亮显示）。
    pub manual_required: Vec<String>,
    /// 是否移除了 Defender。
    pub defender_removed: bool,
    /// 移除后是否需要重启才彻底生效。
    pub reboot_recommended: bool,
    /// 人类可读的总结，直接进日志。
    pub summary: String,
}

impl Default for AntivirusOutcome {
    fn default() -> Self {
        Self {
            detected: Vec::new(),
            outcomes: Vec::new(),
            manual_required: Vec::new(),
            defender_removed: false,
            reboot_recommended: false,
            summary: "未执行安全软件处置".to_string(),
        }
    }
}

/// 带日志回调的处置入口。
pub fn handle_all(
    config: &AntivirusConfig,
    resource_dir: Option<&Path>,
    emit: &dyn Fn(LogLevel, String),
) -> AntivirusOutcome {
    let mut outcome = AntivirusOutcome::default();

    if !config.enabled {
        emit(
            LogLevel::Info,
            "跳过的原因：安全软件处置已关闭".to_string() + "（补丁文件可能被杀软删除）",
        );
        outcome.summary = "已按配置跳过安全软件处置".to_string();
        return outcome;
    }

    // ---------- 1. 检测 ----------
    let detected = if config.detect {
        emit(LogLevel::Info, "[antivirus] 正在检测已注册的安全软件（SecurityCenter2）…".to_string());
        let list = detect_products();
        if list.is_empty() {
            emit(
                LogLevel::Warn,
                "[antivirus] SecurityCenter2 未返回任何安全软件；可能查询被策略禁用，将按未知情况继续".to_string(),
            );
        } else {
            for product in &list {
                emit(
                    LogLevel::Info,
                    format!(
                        "[antivirus] 检测到：{}（{}，实时防护 {}，定义 {}）",
                        product.display_name,
                        if product.is_defender { "内置" } else { "第三方" },
                        if product.enabled { "开启" } else { "关闭" },
                        if product.up_to_date { "最新" } else { "过期" }
                    ),
                );
            }
        }
        list
    } else {
        emit(LogLevel::Info, "[antivirus] detect = false，跳过检测".to_string());
        Vec::new()
    };
    outcome.detected = detected.clone();

    // ---------- 2. Defender ----------
    let has_defender = detected.iter().any(|p| p.is_defender);
    if config.remove_defender && (has_defender || detected.is_empty()) {
        if has_defender {
            emit(
                LogLevel::Warn,
                "[antivirus] Windows Defender 会把 _SolidSQUAD_ 里的补丁判为威胁并删除，开始移除…".to_string(),
            );
        } else {
            emit(
                LogLevel::Info,
                "[antivirus] SecurityCenter2 未列出 Defender，但仍按配置尝试移除（可能是被隐藏或已部分移除）".to_string(),
            );
        }

        match remove_defender(config, resource_dir, emit) {
            Ok(()) => {
                outcome.defender_removed = true;
                outcome.reboot_recommended = config.warn_reboot_after_removal;
                outcome.outcomes.push(AvOutcome {
                    display_name: "Windows Defender".to_string(),
                    is_defender: true,
                    action: "已执行移除（安全中心应用 + 策略注册表 + SmartScreen 文件）".to_string(),
                    resolved: true,
                    manual_hint: String::new(),
                });

                // 复核：注册表策略是否真的写进去了。
                let still_registered = detect_products().iter().any(|p| p.is_defender);
                if still_registered {
                    emit(
                        LogLevel::Warn,
                        "[antivirus] 移除动作已完成，但 SecurityCenter2 仍列出 Defender；这通常是正常的：驱动与服务注册要**重启后**才彻底消失".to_string(),
                    );
                }
            }
            Err(error) => {
                emit(LogLevel::Error, format!("[antivirus] 移除 Defender 失败：{error}"));
                outcome.outcomes.push(AvOutcome {
                    display_name: "Windows Defender".to_string(),
                    is_defender: true,
                    action: format!("移除失败：{error}"),
                    resolved: false,
                    manual_hint:
                        "无法自动移除 Defender。请手动操作：Windows 安全中心 → 病毒和威胁防护 → \
                         管理设置 → 关闭「实时保护」，并添加排除项（把工作目录与安装目录都加进去）。"
                            .to_string(),
                });
                outcome
                    .manual_required
                    .push("Windows Defender（需手动关闭实时保护）".to_string());
            }
        }
    } else if has_defender {
        emit(
            LogLevel::Warn,
            "[antivirus] 按配置未移除 Defender；请确认它的排除项已覆盖工作目录与安装目录，否则解压出的补丁文件可能被删除".to_string(),
        );
    }

    // ---------- 3. 第三方 ----------
    let third_party: Vec<SecurityProduct> = detected
        .iter()
        .filter(|product| !product.is_defender)
        .cloned()
        .collect();

    if !third_party.is_empty() {
        emit(
            LogLevel::Info,
            format!(
                "[antivirus] 检测到 {} 个第三方安全软件，处置方式 = {}",
                third_party.len(),
                config.third_party_mode
            ),
        );

        let partial = handle_third_party(&third_party, config, emit);

        for item in partial {
            if !item.resolved {
                outcome.manual_required.push(item.display_name.clone());
            }
            outcome.outcomes.push(item);
        }
    }

    // ---------- 4. 等待落地 ----------
    let settle = minutes_to_duration(config.settle_minutes);
    if !settle.is_zero() && !outcome.outcomes.is_empty() {
        emit(
            LogLevel::Info,
            format!("[antivirus] 等待 {:.1} 秒让处置生效…", settle.as_secs_f64()),
        );
        thread::sleep(settle);
    }

    // ---------- 5. 总结 ----------
    outcome.summary = build_summary(&outcome);
    emit(
        if outcome.manual_required.is_empty() {
            LogLevel::Success
        } else {
            LogLevel::Warn
        },
        outcome.summary.clone(),
    );

    if outcome.reboot_recommended {
        emit(
            LogLevel::Warn,
            "[antivirus] 移除 Windows Defender 后**必须重启**才能彻底生效；本程序不会自动重启。建议：先让本次安装继续（Defender 已不会再删除文件），安装结束后重启系统，再检查 Defender 是否已消失。".to_string(),
        );
    }

    outcome
}

fn build_summary(outcome: &AntivirusOutcome) -> String {
    let mut parts = Vec::new();
    if outcome.defender_removed {
        parts.push("已移除 Windows Defender".to_string());
    }
    let handled = outcome
        .outcomes
        .iter()
        .filter(|item| !item.is_defender && item.resolved)
        .count();
    if handled > 0 {
        parts.push(format!("已处置 {handled} 个第三方安全软件"));
    }
    if !outcome.manual_required.is_empty() {
        parts.push(format!(
            "需要你手动退出：{}",
            outcome.manual_required.join("、")
        ));
    }
    if parts.is_empty() {
        "未检测到需要处置的安全软件".to_string()
    } else {
        parts.join("；")
    }
}

// ---------------------------------------------------------------------------
// 检测
// ---------------------------------------------------------------------------

/// 通过 `root\SecurityCenter2` 枚举已注册的安全软件。
pub fn detect_products() -> Vec<SecurityProduct> {
    // 用 PowerShell 查询最稳：Win32 没有直接暴露 SecurityCenter2 的简单 API，
    // 而 WMI 的 COM 接口在 Rust 里要写一堆样板。
    const SCRIPT: &str = concat!(
        "$ErrorActionPreference='SilentlyContinue';",
        "Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntiVirusProduct | ",
        "ForEach-Object { ",
        "$s=$_.productState; ",
        // 输出：displayName \t productState \t pathToSignedProductExe
        "\"$($_.displayName)`t$([int]$s)`t$($_.pathToSignedProductExe)\" }"
    );

    let mut command = process_utils::hidden_command(Path::new("powershell"));
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);

    let output = match process_utils::run_command(command, Duration::from_secs(60), None) {
        Ok(outcome) => outcome.output,
        Err(_) => return Vec::new(),
    };

    // PowerShell 输出可能是 GBK/ANSI，统一解码后再按行解析。
    let text = crate::decode_console_bytes(output.as_bytes());
    let mut products = Vec::new();

    for line in text.lines() {
        let mut fields = line.split('\t');
        let display_name = fields.next().unwrap_or("").trim().to_string();
        if display_name.is_empty() {
            continue;
        }
        let product_state = fields
            .next()
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let exe_path = fields.next().unwrap_or("").trim().to_string();

        let is_defender = display_name.to_ascii_lowercase().contains("defender")
            || exe_path.to_ascii_lowercase().contains("windowsdefender");

        products.push(SecurityProduct {
            enabled: decode_product_state_enabled(product_state),
            up_to_date: decode_product_state_updated(product_state),
            is_defender,
            display_name,
            product_state,
            exe_path,
        });
    }

    products
}

/// 解读 `productState` 的"是否启用"位。
///
/// 这个字段没有官方文档，社区总结为 3 字节：
/// `0x0XYYYY` 中第 2 字节的 `0x10` 位表示实时防护开启，`0x00` 表示关闭。
/// 取值不明确时按"已启用"处理——宁可多提醒用户，也不要漏报。
fn decode_product_state_enabled(state: u32) -> bool {
    let byte = ((state >> 12) & 0xFF) as u8;
    !matches!(byte, 0x00)
}

/// 解读 `productState` 的"定义是否最新"位。
fn decode_product_state_updated(state: u32) -> bool {
    let byte = ((state >> 4) & 0xFF) as u8;
    !matches!(byte, 0x00)
}

fn minutes_to_duration(minutes: f64) -> Duration {
    if !minutes.is_finite() || minutes <= 0.0 {
        Duration::from_secs(0)
    } else {
        Duration::from_secs_f64(minutes * 60.0)
    }
}

// ---------------------------------------------------------------------------
// Defender 移除
// ---------------------------------------------------------------------------

/// 定位 windows-defender-remover 的 `script` 目录。
pub fn resolve_defender_tool(
    config: &AntivirusConfig,
    resource_dir: Option<&Path>,
) -> Option<PathBuf> {
    /// 工具目录必须同时具备这两样，否则说明指错了地方。
    fn looks_like_tool_dir(dir: &Path) -> bool {
        dir.is_dir()
            && dir.join("PowerRun.exe").is_file()
            && dir.join("RemoveSecHealthApp.ps1").is_file()
    }

    let configured = config.defender_tool_path.trim();
    if !configured.is_empty() {
        let candidate = PathBuf::from(configured);
        if looks_like_tool_dir(&candidate) {
            return Some(candidate);
        }
    }

    const RELATIVE: &str = r"othertools\windows-defender-remover-main\script";

    let mut candidates: Vec<PathBuf> = Vec::new();

    // exe 同级（打包后 resources 会被放在这一层或其下）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(RELATIVE));
            candidates.push(dir.join("resources").join(RELATIVE));
            candidates.push(dir.join("resources").join("othertools")
                .join("windows-defender-remover-main").join("script"));
        }
    }
    // Tauri 资源目录
    if let Some(dir) = resource_dir {
        candidates.push(dir.join(RELATIVE));
        candidates.push(dir.join("othertools")
            .join("windows-defender-remover-main").join("script"));
    }
    // 开发期：源码目录
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(RELATIVE));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(RELATIVE));

    candidates.into_iter().find(|path| looks_like_tool_dir(path))
}

/// 用随仓库的 windows-defender-remover 移除 Defender（**不含重启**）。
fn remove_defender(
    config: &AntivirusConfig,
    resource_dir: Option<&Path>,
    emit: &dyn Fn(LogLevel, String),
) -> Result<(), String> {
    let tool = resolve_defender_tool(config, resource_dir).ok_or_else(|| {
        "未找到 windows-defender-remover 的 script 目录。它必须包含 PowerRun.exe 与 \
         RemoveSecHealthApp.ps1。请把 `othertools\\windows-defender-remover-main\\script` \
         放在程序同级目录，或在 [antivirus].defender_tool_path 中填写绝对路径。\
         （已查找：配置路径、exe 同级、资源目录、源码目录）"
            .to_string()
    })?;

    let power_run = tool.join("PowerRun.exe");
    let sec_health = tool.join("RemoveSecHealthApp.ps1");
    let timeout = minutes_to_duration(config.command_timeout_minutes);

    emit(
        LogLevel::Info,
        format!("[antivirus] 使用工具目录 {}", tool.display()),
    );

    // ---- 第 1 步：移除 Windows 安全中心 UWP 应用 ----
    {
        let mut command = process_utils::hidden_command(&power_run);
        command.current_dir(&tool);
        command.args([
            "powershell.exe",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &sec_health.to_string_lossy(),
        ]);

        match process_utils::run_command(command, timeout.max(Duration::from_secs(120)), None) {
            Ok(outcome) => {
                log_tail(emit, "RemoveSecHealthApp", &outcome);
                if !outcome.is_success() && !outcome.timed_out {
                    emit(
                        LogLevel::Warn,
                        format!(
                            "[antivirus] 移除安全中心应用返回 code={:?}，继续导入策略注册表",
                            outcome.code
                        ),
                    );
                }
            }
            Err(error) => emit(
                LogLevel::Warn,
                format!("[antivirus] 调用 PowerRun 移除安全中心应用失败：{error}（继续后续步骤）"),
            ),
        }
    }

    // ---- 第 2 步：导入 Remove_Defender 与 Remove_SecurityComp 下的全部 .reg ----
    let mut reg_files: Vec<PathBuf> = Vec::new();
    for sub in ["Remove_Defender", "Remove_SecurityComp"] {
        let dir = tool.join(sub);
        if !dir.is_dir() {
            emit(
                LogLevel::Warn,
                format!("[antivirus] 缺少注册表目录 {}", dir.display()),
            );
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut found: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path
                            .extension()
                            .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("reg"))
                            .unwrap_or(false)
                })
                .collect();
            found.sort();
            reg_files.extend(found);
        }
    }

    if reg_files.is_empty() {
        return Err(format!(
            "{} 下没有找到任何 .reg 策略文件，工具目录可能不完整",
            tool.display()
        ));
    }

    emit(
        LogLevel::Info,
        format!("[antivirus] 导入 {} 个策略注册表文件…", reg_files.len()),
    );

    let mut failed = 0usize;
    for reg in &reg_files {
        let mut command = process_utils::hidden_command(&power_run);
        command.current_dir(&tool);
        command.args([
            "regedit.exe",
            "/s",
            &reg.to_string_lossy(),
        ]);

        match process_utils::run_command(command, timeout.max(Duration::from_secs(60)), None) {
            Ok(outcome) if outcome.is_success() => {
                emit(
                    LogLevel::Debug,
                    format!(
                        "[antivirus] 已导入 {}",
                        reg.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ),
                );
            }
            Ok(outcome) => {
                failed += 1;
                emit(
                    LogLevel::Warn,
                    format!(
                        "[antivirus] 导入 {} 失败（code={:?}）：{}",
                        reg.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        outcome.code,
                        outcome.tail(2)
                    ),
                );
            }
            Err(error) => {
                failed += 1;
                emit(
                    LogLevel::Warn,
                    format!("[antivirus] 调用 PowerRun 导入注册表失败：{error}"),
                );
            }
        }
    }

    // ---- 第 3 步：删除 SmartScreen 相关文件（与工具一致） ----
    let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".to_string());
    for name in ["smartscreen.exe", "smartscreen.dll", "smartscreenps.dll"] {
        let file = PathBuf::from(&windir).join("System32").join(name);
        if !file.is_file() {
            continue;
        }
        // 先接管所有权再删除：这些文件受 TrustedInstaller 保护。
        let _ = run_quiet(&power_run, &tool, &["cmd.exe", "/c", &format!(
            "takeown /f \"{}\" /a",
            file.display()
        )]);
        let _ = run_quiet(&power_run, &tool, &["cmd.exe", "/c", &format!(
            "icacls \"{}\" /grant administrators:F",
            file.display()
        )]);
        match std::fs::remove_file(&file) {
            Ok(()) => emit(
                LogLevel::Info,
                format!("[antivirus] 已删除 {}", file.display()),
            ),
            Err(error) => emit(
                LogLevel::Warn,
                format!(
                    "[antivirus] 删除 {} 失败：{error}（该文件可能正被占用，重启后会自动清理）",
                    file.display()
                ),
            ),
        }
    }

    if failed > 0 {
        emit(
            LogLevel::Warn,
            format!(
                "[antivirus] 有 {failed}/{} 个策略注册表文件导入失败；\
                 Defender 可能未被完全移除，请重启后到「Windows 安全中心」确认",
                reg_files.len()
            ),
        );
    }

    Ok(())
}

fn run_quiet(
    power_run: &Path,
    tool_dir: &Path,
    args: &[&str],
) -> Result<(), String> {
    let mut command = process_utils::hidden_command(power_run);
    command.current_dir(tool_dir);
    command.args(args);
    match process_utils::run_command(command, Duration::from_secs(90), None) {
        Ok(_) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn log_tail(emit: &dyn Fn(LogLevel, String), label: &str, outcome: &process_utils::CommandOutcome) {
    let text = crate::decode_console_bytes(outcome.output.as_bytes());
    for line in text.lines().filter(|line| !line.trim().is_empty()).take(12) {
        emit(LogLevel::Debug, format!("[{label}] {}", line.trim()));
    }
    if outcome.timed_out {
        emit(LogLevel::Warn, format!("[{label}] 执行超时"));
    }
}

// ---------------------------------------------------------------------------
// 第三方杀软
// ---------------------------------------------------------------------------

/// 处置第三方杀软。返回每个的结论。
fn handle_third_party(
    products: &[SecurityProduct],
    config: &AntivirusConfig,
    emit: &dyn Fn(LogLevel, String),
) -> Vec<AvOutcome> {
    let mode = config.third_party_mode.as_str();
    let timeout = minutes_to_duration(config.command_timeout_minutes);
    let mut results = Vec::new();

    for product in products {
        // 用产品名与可执行文件路径提炼关键词，用于匹配进程与服务。
        let keywords = keywords_for(product);
        emit(
            LogLevel::Info,
            format!(
                "[antivirus] 处理 {}（关键词：{}）",
                product.display_name,
                keywords.join(", ")
            ),
        );

        let mut killed_processes = Vec::new();
        let mut stopped_services = Vec::new();

        if mode == "terminate" || mode == "uninstall" {
            // ---- 终止进程 ----
            killed_processes = terminate_matching_processes(&keywords, timeout, emit);

            // ---- 停止服务 ----
            stopped_services = stop_matching_services(&keywords, timeout, emit);
        }

        // ---- 静默卸载（可选，风险最高，默认不开） ----
        let mut uninstalled = false;
        if mode == "uninstall" {
            uninstalled = try_silent_uninstall(product, timeout, emit);
        }

        // ---- 复核 ----
        let still_present = detect_products().iter().any(|current| {
            current.display_name.eq_ignore_ascii_case(&product.display_name)
                || current.path_to_key() == product.path_to_key()
        });

        let action = {
            let mut parts = Vec::new();
            if !killed_processes.is_empty() {
                parts.push(format!("已终止 {} 个进程", killed_processes.len()));
            }
            if !stopped_services.is_empty() {
                parts.push(format!("已停止 {} 个服务", stopped_services.len()));
            }
            if uninstalled {
                parts.push("已尝试静默卸载".to_string());
            }
            if parts.is_empty() {
                match mode {
                    "prompt" => "仅检测（按配置未尝试终止）".to_string(),
                    _ => "未能终止其进程或服务".to_string(),
                }
            } else {
                parts.join("；")
            }
        };

        // 关键：即使杀进程成功，只要它仍注册在 SecurityCenter2，
        // 就认为尚未真正失效——它的驱动/自我保护可能让实时防护继续工作。
        let resolved = !still_present;
        let manual_hint = if resolved {
            String::new()
        } else {
            format!(
                "无法自动停用「{}」。请手动操作：在**系统托盘**右键该软件图标 → \
                 选择「退出 / 暂停防护 / 关闭实时监控」，然后回来点「我已手动退出，重试」。\
                 若托盘没有图标，请打开它的主界面关闭实时防护，或在「设置 → 应用」中卸载。",
                product.display_name
            )
        };

        emit(
            if resolved { LogLevel::Success } else { LogLevel::Warn },
            format!(
                "[antivirus] {}：{}（{}）",
                product.display_name,
                action,
                if resolved {
                    "已不再注册为活动防护"
                } else {
                    "仍在 SecurityCenter2 中注册为活动防护"
                }
            ),
        );

        results.push(AvOutcome {
            display_name: product.display_name.clone(),
            is_defender: false,
            action,
            resolved,
            manual_hint,
        });
    }

    results
}

impl SecurityProduct {
    /// 用于跨次比对同一产品的稳定键（厂商路径优先，其次显示名）。
    fn path_to_key(&self) -> String {
        if self.exe_path.trim().is_empty() {
            self.display_name.to_ascii_lowercase()
        } else {
            self.exe_path.to_ascii_lowercase()
        }
    }
}

/// 从产品信息里提炼用于匹配进程/服务的关键词。
///
/// 只保留长度 ≥ 3 的字母数字片段，避免用 "of"、"the" 这类词误杀。
fn keywords_for(product: &SecurityProduct) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();

    // 显示名：整体去掉空格后作为一个关键词（例如 "Windows Defender" → "windowsdefender"）
    let compact: String = product
        .display_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if compact.len() >= 4 {
        set.insert(compact);
    }

    // 显示名里的词
    for word in product.display_name.split(|c: char| !c.is_ascii_alphanumeric()) {
        let word = word.trim().to_ascii_lowercase();
        if word.len() >= 4 && !GENERIC_WORDS.contains(&word.as_str()) {
            set.insert(word);
        }
    }

    // 厂商可执行文件路径里的文件名（不含扩展名）
    if let Some(stem) = Path::new(&product.exe_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
    {
        if stem.len() >= 4 && !GENERIC_WORDS.contains(&stem.as_str()) {
            set.insert(stem);
        }
    }

    set.into_iter().collect()
}

/// 太通用的词，单独拿来匹配进程名会误杀。
const GENERIC_WORDS: &[&str] = &[
    "windows", "security", "antivirus", "anti", "virus", "product", "system", "service",
    "center", "total", "internet", "suite", "pro", "plus", "free", "home", "version",
];

/// 终止名字里含关键词的进程。返回被终止的进程名。
fn terminate_matching_processes(
    keywords: &[String],
    timeout: Duration,
    emit: &dyn Fn(LogLevel, String),
) -> Vec<String> {
    let mut killed = Vec::new();
    let processes = win32_utils::list_processes();

    for (pid, name) in processes {
        let lower = name.to_ascii_lowercase();
        if !keywords.iter().any(|keyword| lower.contains(keyword)) {
            continue;
        }
        // 绝不终止自己。
        if pid == std::process::id() {
            continue;
        }

        let mut command = process_utils::hidden_command(Path::new("taskkill"));
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        match process_utils::run_command(command, timeout.max(Duration::from_secs(30)), None) {
            Ok(outcome) if outcome.is_success() => {
                emit(LogLevel::Info, format!("[antivirus] 已终止进程 {name} (pid {pid})"));
                killed.push(name);
            }
            Ok(outcome) => emit(
                LogLevel::Debug,
                format!(
                    "[antivirus] 终止 {name} (pid {pid}) 未成功（code={:?}），可能受自我保护保护",
                    outcome.code
                ),
            ),
            Err(error) => emit(
                LogLevel::Debug,
                format!("[antivirus] 调用 taskkill 失败：{error}"),
            ),
        }
    }

    killed
}

/// 停止名字里含关键词的服务。返回被停止的服务名。
fn stop_matching_services(
    keywords: &[String],
    timeout: Duration,
    emit: &dyn Fn(LogLevel, String),
) -> Vec<String> {
    // 一次取回全部服务名+显示名，避免逐个 sc query。
    const SCRIPT: &str = concat!(
        "$ErrorActionPreference='SilentlyContinue';",
        "Get-Service | ForEach-Object { \"$($_.Name)`t$($_.DisplayName)\" }"
    );
    let mut command = process_utils::hidden_command(Path::new("powershell"));
    command.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);

    let output = match process_utils::run_command(command, Duration::from_secs(60), None) {
        Ok(outcome) => crate::decode_console_bytes(outcome.output.as_bytes()),
        Err(_) => return Vec::new(),
    };

    let mut stopped = Vec::new();
    for line in output.lines() {
        let mut fields = line.split('\t');
        let name = fields.next().unwrap_or("").trim().to_string();
        let display = fields.next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let haystack = format!("{name} {display}").to_ascii_lowercase();
        if !keywords.iter().any(|keyword| haystack.contains(keyword)) {
            continue;
        }

        let mut stop = process_utils::hidden_command(Path::new("sc"));
        stop.args(["stop", &name]);
        match process_utils::run_command(stop, timeout.max(Duration::from_secs(60)), None) {
            Ok(outcome) if outcome.is_success() => {
                emit(
                    LogLevel::Info,
                    format!("[antivirus] 已停止服务 {name}（{display}）"),
                );
                stopped.push(name);
            }
            _ => emit(
                LogLevel::Debug,
                format!("[antivirus] 停止服务 {name} 未成功（可能受保护或不存在）"),
            ),
        }
    }

    stopped
}

/// 尝试按卸载信息静默卸载。
///
/// 只认常见的静默参数；绝大多数个人版杀软会弹交互界面或自保护，
/// 因此失败是**预期结果**，会如实回报给用户。
fn try_silent_uninstall(
    product: &SecurityProduct,
    timeout: Duration,
    emit: &dyn Fn(LogLevel, String),
) -> bool {
    let Some(uninstall) = win32_utils::find_uninstall_string(&product.display_name) else {
        emit(
            LogLevel::Debug,
            format!(
                "[antivirus] 未在注册表中找到 {} 的卸载命令",
                product.display_name
            ),
        );
        return false;
    };

    emit(
        LogLevel::Warn,
        format!(
            "[antivirus] 尝试静默卸载 {}：{uninstall}",
            product.display_name
        ),
    );

    // 卸载命令通常是一条完整命令行（可能带参数），统一交给 cmd 解析，
    // 避免我们自己拆错引号。逐个尝试常见静默参数。
    for extra in [Some("/S"), Some("/silent"), Some("/quiet"), None] {
        let line = match extra {
            Some(flag) => format!("{uninstall} {flag}"),
            None => uninstall.clone(),
        };
        let mut command = process_utils::hidden_command(Path::new("cmd"));
        command.args(["/C", &line]);

        match process_utils::run_command(command, timeout, None) {
            Ok(outcome) if outcome.is_success() => {
                emit(
                    LogLevel::Success,
                    format!("[antivirus] {} 的卸载命令返回成功", product.display_name),
                );
                return true;
            }
            Ok(outcome) => emit(
                LogLevel::Debug,
                format!(
                    "[antivirus] 卸载尝试（{:?}）返回 code={:?}",
                    extra, outcome.code
                ),
            ),
            Err(error) => emit(
                LogLevel::Debug,
                format!("[antivirus] 卸载尝试（{extra:?}）失败：{error}"),
            ),
        }
    }

    emit(
        LogLevel::Warn,
        format!(
            "[antivirus] {} 无法静默卸载（个人版杀软普遍如此），需要你手动退出",
            product.display_name
        ),
    );
    false
}
