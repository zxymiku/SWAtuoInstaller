//! 第 12 步：安装 FlexNet 许可服务。
//!
//! 运行 `server_remove.bat` / `server_install.bat` 并轮询服务状态至 `RUNNING`。
//!
use std::path::Path;
use std::time::Duration;

use crate::process_utils;

use super::context::InstallContext;
use super::paths::{find_file_named, find_first_dir, humanize_duration};

pub fn install_flexnet_service(ctx: &InstallContext, patch_root: &Path) -> Result<(), String> {
    const SERVICE: &str = "SolidWorks Flexnet Server";

    let squad = find_first_dir(patch_root, "_solidsquad_")?;
    let search_root = squad.unwrap_or_else(|| patch_root.to_path_buf());

    let install_bat = find_file_named(&search_root, "server_install.bat")
        .ok_or_else(|| format!("未找到 server_install.bat（搜索根 {}）", search_root.display()))?;
    let remove_bat = find_file_named(&search_root, "server_remove.bat");

    if ctx.config.flexnet.remove_old_server_first {
        match &remove_bat {
            Some(bat) => {
                ctx.info(format!("先执行 {}", bat.display()));
                // 卸载脚本与安装脚本共用同一份预算，避免两个旋钮互相矛盾。
                run_batch(
                    ctx,
                    bat,
                    ctx.config.server_install_timeout(),
                    "server_remove",
                )?;
            }
            None => {
                // 没有 remove 脚本时，直接清掉既有服务。
                for record in process_utils::remove_service(SERVICE, ctx.config.server_install_timeout())
                {
                    ctx.debug(record);
                }
            }
        }
    }

    ctx.info(format!("执行 {}", install_bat.display()));
    run_batch(
        ctx,
        &install_bat,
        ctx.config.server_install_timeout(),
        "server_install",
    )?;

    let interval = ctx.config.server_check_interval();
    let timeout = ctx.config.server_install_timeout();
    ctx.info(format!(
        "轮询服务状态（间隔 {:.2} 分钟，超时 {:.1} 分钟）",
        ctx.config.flexnet.server_check_interval_minutes,
        ctx.config.flexnet.server_install_timeout_minutes
    ));

    let (running, elapsed) = process_utils::wait_for_service(
        SERVICE,
        interval,
        timeout,
        Some(&ctx.cancel),
    );

    if running {
        ctx.success(format!(
            "许可服务已在 {} 内进入 RUNNING",
            humanize_duration(elapsed)
        ));
        Ok(())
    } else {
        Err(format!(
            "许可服务在 {} 内未进入 RUNNING，请检查 server_install.bat 输出与端口 25734",
            humanize_duration(elapsed)
        ))
    }
}

fn run_batch(
    ctx: &InstallContext,
    bat: &Path,
    timeout: Duration,
    label: &str,
) -> Result<(), String> {
    let mut command = process_utils::hidden_command(Path::new("cmd"));
    command.args(["/C", &bat.to_string_lossy()]);
    if let Some(parent) = bat.parent() {
        command.current_dir(parent);
    }

    let outcome = process_utils::run_command(command, timeout, Some(&ctx.cancel))
        .map_err(|e| format!("执行 {label} 失败: {e}"))?;

    for line in outcome.output.lines().take(40) {
        if !line.trim().is_empty() {
            ctx.debug(format!("[{label}] {}", line.trim()));
        }
    }

    if outcome.timed_out {
        return Err(format!(
            "{label} 超时（{}）",
            humanize_duration(timeout)
        ));
    }
    if !outcome.is_success() {
        ctx.warn(format!(
            "{label} 返回 code={:?}，继续后续步骤",
            outcome.code
        ));
    }
    Ok(())
}
