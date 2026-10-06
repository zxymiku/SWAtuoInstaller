//! 第 1 步环境检测与第 2.5 步安全软件处置。
//!
//! 两步都作用于「开始动文件之前」，因此放在一起。
//!
use crate::events::ErrorKind;
use crate::win32_utils;

use super::context::InstallContext;
use std::sync::atomic::Ordering;
use super::sevenzip::seven_zip_status;

pub fn environment_check(ctx: &mut InstallContext) -> Result<crate::events::EnvSnapshot, String> {
    let username = win32_utils::current_username();
    let computer_name = win32_utils::current_computer_name();
    let username_ascii = win32_utils::is_pure_ascii(&username);
    let computer_ascii = win32_utils::is_pure_ascii(&computer_name);
    let elevated = win32_utils::is_elevated();
    let free_space = win32_utils::free_space_gb(&ctx.config.install.install_drive);

    ctx.reporter.environment_check(
        &username,
        &computer_name,
        username_ascii,
        computer_ascii,
        elevated,
        free_space,
    );

    ctx.info(format!("当前用户 {username} · 计算机名 {computer_name}"));
    ctx.info(format!(
        "目标盘 {}: 可用空间 {:.1} GB（64 位进程: {}）",
        ctx.config.install.install_drive,
        free_space,
        win32_utils::is_64bit_process()
    ));

    if !computer_ascii {
        let message = format!(
            "计算机名「{computer_name}」包含非 ASCII 字符。SolidWorks FlexNet 许可服务无法在该名称下启动，\
             请改为纯英文并**重启**后再运行本程序。"
        );
        ctx.reporter.error(ErrorKind::Environment, &message, true);
        return Err(message);
    }

    if !username_ascii {
        let message = format!(
            "用户名「{username}」包含非 ASCII 字符。请改用纯英文用户名，或新建一个英文账户后再安装。"
        );
        ctx.reporter.error(ErrorKind::Environment, &message, true);
        return Err(message);
    }

    if !elevated {
        let message =
            "当前不是以管理员身份运行。注册表导入、网卡禁用、服务安装都需要管理员权限。".to_string();
        ctx.reporter.error(ErrorKind::Environment, &message, true);
        // 尝试自动提权，交由前端提示用户重新运行。
        match win32_utils::relaunch_elevated("") {
            Ok(()) => {
                ctx.info("已发起管理员提权请求，请在弹出的 UAC 对话框中确认");
            }
            Err(error) => ctx.warn(format!("自动提权失败: {error}")),
        }
        return Err(message);
    }

    let install_path = ctx.config.resolved_install_path();
    if (0.0..20.0).contains(&free_space) {
        ctx.warn(format!(
            "目标盘可用空间仅 {free_space:.1} GB，SolidWorks 完整安装通常需要 20 GB 以上"
        ));
    }
    ctx.info(format!("安装目标目录 {}", install_path.display()));

    // 7z 是本地分卷识别和后续解压的前置依赖；在第一步就确保可用，
    // 找不到时按配置自动下载，避免用户选完分卷才发现无法验证。
    ctx.info("开始准备 7z：检查配置路径、资源目录、应用缓存、已安装程序和 PATH");
    let resolution = {
        let reporter = ctx.reporter.clone();
        crate::installer::ensure_seven_zip(
            &ctx.config,
            &ctx.app_data_dir,
            ctx.resource_dir.as_deref(),
            &|level, message| reporter.log(level, message),
        )?
    };
    ctx.info(format!(
        "第一步已确认 7z 可用：{}（来源：{}）",
        resolution.program.display(),
        resolution.source.label()
    ));
    ctx.seven_zip = resolution.program;

    let config_path = ctx.app_data_dir.join("config.toml");

    Ok(crate::events::EnvSnapshot {
        username,
        computer_name,
        username_ascii,
        computer_ascii,
        elevated,
        temp_dir: ctx.workdir.display().to_string(),
        app_data_dir: ctx.app_data_dir.display().to_string(),
        config_exists: config_path.is_file(),
        config_path: config_path.display().to_string(),
        free_space_gb: free_space,
        seven_zip: seven_zip_status(&ctx.config, ctx.resource_dir.as_deref(), &ctx.app_data_dir),
    })
}

// ---------------------------------------------------------------------------
// 第 2.5 步：安全软件处置
// ---------------------------------------------------------------------------

/// 检测并处置会把补丁文件删掉的安全软件。
///
/// **本步永不返回 Err**：杀软处置失败不应该让整个安装失败——
/// 最坏情况只是提醒用户手动退出，而安装本身仍可能成功（例如用户已经把
/// 工作目录加进了排除项）。结论通过 [`InstallEvent::AntivirusReport`] 推给前端。
pub fn handle_antivirus(ctx: &InstallContext) {
    let config = &ctx.config.antivirus;

    if !config.enabled {
        ctx.info("[antivirus] enabled = false，跳过安全软件处置");
        return;
    }

    ctx.info(
        "开始安全软件检测与处置（Defender 会把 _SolidSQUAD_ 里的补丁判为威胁并删除，\
         因此必须在解压前处理）",
    );

    let reporter = ctx.reporter.clone();
    let outcome = crate::antivirus::handle_all(
        config,
        ctx.resource_dir.as_deref(),
        &|level, message| reporter.log(level, message),
    );

    // 把结论同步到状态快照（供 get_install_status 与 UI 恢复）
    if let Ok(mut guard) = ctx.status.antivirus.lock() {
        *guard = Some(outcome.clone());
    }
    ctx.status
        .antivirus_manual_required
        .store(outcome.manual_required.len() as u64, Ordering::SeqCst);
    if outcome.defender_removed {
        ctx.status
            .antivirus_defender_removed
            .store(true, Ordering::SeqCst);
    }
    if outcome.reboot_recommended {
        ctx.status
            .reboot_required
            .store(true, Ordering::SeqCst);
    }

    reporter.antivirus_report(&outcome);

    // 明确告知用户下一步该做什么（这是需求里"告知用户"的落点）
    if !outcome.manual_required.is_empty() {
        ctx.warn(format!(
            "以下安全软件**无法自动停用**，需要你手动从系统托盘退出后再继续：{}",
            outcome.manual_required.join("、")
        ));
        for item in &outcome.outcomes {
            if !item.resolved && !item.manual_hint.is_empty() {
                ctx.warn(format!("  · {}：{}", item.display_name, item.manual_hint));
            }
        }
        ctx.warn(
            "若这些软件仍在运行，解压出的补丁文件可能被它们删除，\
             导致安装在第 11 步「文件替换」时缺少源文件。",
        );
    }
}
