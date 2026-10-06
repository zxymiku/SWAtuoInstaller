//! 13 阶段安装流程的编排。
//!
//! | 物理阶段 | 逻辑步骤 |
//! |---|---|
//! | 1 环境检测 | 01 环境检测 |
//! | 2 获取压缩包 + 2.5 安全软件处置 | 02 获取压缩包 |
//! | 3 禁用网络 | 03 禁用网络 |
//! | 4 第一次解压 + 5 第二次解压 | 04 两阶段解压 |
//! | 6 导入注册表 | 05 导入注册表 |
//! | 7 挂载 ISO 并安装 | 06 挂载镜像 |
//! | 8 弹窗守护 + 启动安装器 | 07 静默安装 |
//! | 9 轮询检测完成 | 08 轮询检测 |
//! | 10 关闭进程 + 11 文件替换 + 12 FlexNet 服务 + 13 收尾 | 09 收尾恢复 |
//!
//! 所有超时/间隔都从 `Config` 读取，不硬编码。
//!
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crate::events::{ErrorKind, LogLevel, StepId};
use crate::process_utils::NetworkGuard;

use super::context::InstallContext;
use super::guards::{IsoMountGuard, TempDirGuard};
use super::steps::{environment_check, handle_antivirus};
use super::archive::acquire_archive;
use super::extract::{extract_archive, extract_solidsquad};
use super::registry::import_registry;
use super::iso::{locate_iso, locate_setup};
use super::install_ops::{launch_installer, spawn_popup_daemon};
use super::prereqs::handle_prerequisites;
use super::verify::poll_for_completion;
use super::postinstall::{kill_solidworks_processes, replace_files};
use super::flexnet::install_flexnet_service;
use super::paths::humanize_duration;
use std::sync::atomic::AtomicBool;

/// 执行完整的 13 阶段安装流程。
///
/// 返回 `Ok(())` 表示全部阶段成功；`Err` 是人类可读的失败原因。
/// 无论成功、失败还是 panic，`Drop` 守卫都会恢复网络与卸载镜像。
pub fn run(ctx: &mut InstallContext) -> Result<(), String> {
    let started = Instant::now();
    ctx.status.reset();
    if let Ok(mut guard) = ctx.status.started_at.lock() {
        *guard = Some(crate::now_timestamp());
    }
    ctx.status.running.store(true, Ordering::SeqCst);

    ctx.info(format!(
        "开始部署流程 · 配置来自 {}",
        ctx.app_data_dir.join("config.toml").display()
    ));

    // 守卫：即使中途 panic 也会恢复网络、卸载镜像、清理临时目录。
    let mut network_guard: Option<NetworkGuard> = None;
    let mut iso_guard: Option<IsoMountGuard> = None;
    let mut temp_guard: Option<TempDirGuard> = None;
    let mut popup_stop: Option<Arc<AtomicBool>> = None;
    // 前置组件检测结果。用 Option 是因为任何提前 `?` 返回都可能让它没被赋值。
    let mut prerequisites_report: Option<super::prereqs::PrereqReport> = None;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
        // ---------------- 第 1 步：环境检测 ------------------------------
        ctx.enter_step(StepId::Environment);
        let snapshot = environment_check(ctx)?;
        ctx.debug(format!(
            "环境快照 · 用户 {} · 主机 {} · 管理员 {} · 可用 {:.1} GB",
            snapshot.username,
            snapshot.computer_name,
            snapshot.elevated,
            snapshot.free_space_gb
        ));

        // ---------------- 第 2 步：获取压缩包 ----------------------------
        ctx.enter_step(StepId::Acquire);
        let archive = acquire_archive(ctx)?;

        // ---------------- 第 2.5 步：安全软件处置 -------------------------
        //
        // 放在解压**之前**是关键：Defender 与多数第三方杀软会把 _SolidSQUAD_
        // 里的补丁文件判为威胁并直接删除，等解压完再处理就已经晚了。
        // 本步**不阻断**流程——最坏情况是提醒用户手动退出杀软，安装继续。
        handle_antivirus(ctx);

        // ---------------- 第 3 步：禁用网络 ------------------------------
        ctx.enter_step(StepId::Network);
        let (guard, events) = NetworkGuard::apply(
            ctx.config.network.disable_network,
            ctx.config.network.restore_network_after,
            &ctx.config.network.adapter_disable_method,
            // 网卡开关命令的等待预算：与镜像挂载同属系统级操作，复用解压超时
            ctx.config.extract_timeout(),
        );
        for event in events {
            ctx.info(event);
        }
        ctx.status
            .network_disabled
            .store(guard.is_disabled(), Ordering::SeqCst);
        network_guard = Some(guard);

        // 工作目录
        std::fs::create_dir_all(&ctx.workdir)
            .map_err(|e| format!("创建工作目录失败 {}: {e}", ctx.workdir.display()))?;
        ctx.info(format!("工作目录已创建：{}", ctx.workdir.display()));
        temp_guard = Some(TempDirGuard::new(
            ctx.workdir.clone(),
            ctx.config.workdir.cleanup_temp_after,
        ));

        // ---------------- 第 4 + 5 步：两阶段解压 ------------------------
        ctx.enter_step(StepId::Extract);
        let folder1 = ctx.workdir.join("folder1");
        std::fs::create_dir_all(&folder1)
            .map_err(|e| format!("创建解压目录失败: {e}"))?;
        extract_archive(ctx, &archive, &folder1, 1)?;
        let patch_root = extract_solidsquad(ctx, &folder1)?;
        ctx.info(format!("补丁根目录已确定：{}", patch_root.display()));

        // ---------------- 第 6 步：导入注册表 ----------------------------
        ctx.enter_step(StepId::Registry);
        import_registry(ctx, &patch_root)?;

        // ---------------- 第 7 步：挂载 ISO 并启动安装 -------------------
        ctx.enter_step(StepId::MountIso);
        let iso_path = locate_iso(&folder1)?;
        let timeout = ctx.config.extract_timeout();
        let mounted = IsoMountGuard::mount(&iso_path, timeout)?;
        for event in &mounted.events {
            ctx.info(event.clone());
        }
        iso_guard = Some(mounted);
        ctx.status.iso_mounted.store(true, Ordering::SeqCst);

        let (setup_root, setup_drive) = locate_setup(ctx, iso_guard.as_mut())?;
        if let Some(guard) = iso_guard.as_mut() {
            if guard.drive().is_none() {
                guard.set_drive(setup_drive.clone());
            }
        }
        ctx.info(format!("安装介质根目录 {}", setup_root.display()));

        // ---------------- 第 7.5 步：Windows 前置组件 --------------------
        //
        // 官方手册（第 31~34 页）明确：命令行安装**不会**自动装前置组件
        // （.NET 4.8 / Visual C++ 可再发行 / WebView2 / VBA）。平时双击
        // setup.exe 是安装管理器代劳；改走 msiexec 就必须自己补。
        //
        // 缺失的后果不是"装不上"，而是"装上了跑不起来"——
        // solidworks.msi 没有 LaunchCondition 表（已实测），安装会报成功。
        //
        // 本步**不终止流程**：最坏情况是告警 + 让用户自己补，
        // 因为强行中断会连"部分装好"的机会都丢掉。
        prerequisites_report = Some(handle_prerequisites(
            &ctx.config.install,
            &setup_root,
            &|level, message| ctx.reporter.log(level, message),
        ));

        // ---------------- 第 8 步：弹窗守护 + 启动安装器 -----------------
        ctx.enter_step(StepId::Install);
        let stop_flag = Arc::new(AtomicBool::new(false));
        popup_stop = Some(Arc::clone(&stop_flag));
        let popup_daemon = spawn_popup_daemon(ctx, Arc::clone(&stop_flag));

        let install_result = launch_installer(ctx, &setup_root);

        // ---------------- 第 9 步：轮询检测完成 -------------------------
        let install_ok = match install_result {
            Ok(()) => true,
            Err(error) => {
                ctx.warn(format!("安装器启动/执行返回异常: {error}"));
                false
            }
        };

        ctx.enter_step(StepId::Verify);
        let verify_result = poll_for_completion(ctx, install_ok);

        // 弹窗守护到此为止：后续步骤不再需要处理安装器弹窗。
        if let Some(flag) = &popup_stop {
            flag.store(true, Ordering::SeqCst);
        }
        if let Some(handle) = popup_daemon {
            let _ = handle.join();
        }

        verify_result?;

        // ---------------- 第 10 步：关闭 SW 相关进程 ---------------------
        ctx.enter_step(StepId::Finalize);
        kill_solidworks_processes(ctx);

        // ---------------- 第 11 步：文件替换 ----------------------------
        replace_files(ctx, &patch_root)?;

        // ---------------- 第 12 步：FlexNet 服务 ------------------------
        install_flexnet_service(ctx, &patch_root)?;

        // ---------------- 第 13 步：收尾 --------------------------------
        // 先把前置组件的最终结论汇总进日志：如果有强制项没满足，
        // 用户需要在"装完了但可能跑不起来"之前就看到原因。
        if let Some(report) = prerequisites_report.as_ref() {
            ctx.info(format!("前置组件结论：{}", report.summary()));
            for item in &report.outcomes {
                if !item.already_installed {
                    ctx.log(
                        if item.ok {
                            LogLevel::Success
                        } else {
                            LogLevel::Warn
                        },
                        format!("  · {}：{}", item.label, item.detail),
                    );
                }
            }
            if !report.missing_required.is_empty() {
                ctx.warn(format!(
                    "以下强制前置组件未满足，SOLIDWORKS 启动时可能失败：{}。\
                     安装包都在介质 PreReqs\\ 目录里，可手动补装。",
                    report.missing_required.join("、")
                ));
                ctx.status
                    .prereq_missing
                    .store(report.missing_required.len() as u64, Ordering::SeqCst);
            }
        }

        if let Some(guard) = network_guard.as_mut() {
            for event in guard.restore_now() {
                ctx.info(event);
            }
        }
        ctx.status.network_disabled.store(false, Ordering::SeqCst);

        if let Some(guard) = iso_guard.as_mut() {
            for event in guard.dismount_now() {
                ctx.info(event);
            }
        }
        ctx.status.iso_mounted.store(false, Ordering::SeqCst);

        if let Some(guard) = temp_guard.as_mut() {
            for event in guard.cleanup_now() {
                ctx.log(LogLevel::Debug, event);
            }
        }

        ctx.success(format!(
            "部署完成，总耗时 {}",
            humanize_duration(started.elapsed())
        ));
        Ok(())
    }));

    // 无论结果如何，先把守护线程与剩余守卫收干净。
    if let Some(flag) = &popup_stop {
        flag.store(true, Ordering::SeqCst);
    }
    drop(popup_stop);

    let final_result = match result {
        Ok(inner) => inner,
        Err(_) => Err("安装流程发生内部 panic，已通过守卫恢复网络与镜像".to_string()),
    };

    if final_result.is_err() {
        if let Some(guard) = temp_guard.as_mut() {
            guard.preserve_on_failure = true;
            ctx.warn(format!("部署失败，保留工作目录供排查：{}", guard.path.display()));
        }
    }

    ctx.status.running.store(false, Ordering::SeqCst);
    ctx.status.finished.store(true, Ordering::SeqCst);
    if let Ok(mut guard) = ctx.status.finished_at.lock() {
        *guard = Some(crate::now_timestamp());
    }

    let cancelled = ctx.is_cancelled();
    match &final_result {
        Ok(()) => {
            ctx.status.success.store(true, Ordering::SeqCst);
            ctx.reporter.completed(true, "部署完成");
        }
        Err(message) => {
            let kind = if cancelled {
                ErrorKind::Cancelled
            } else {
                ErrorKind::Install
            };
            ctx.reporter.error(kind, message, false);
            ctx.reporter
                .completed(false, if cancelled { "已取消" } else { "部署失败" });
        }
    }

    // 释放守卫（Drop 会补做任何尚未完成的恢复动作）。
    drop(network_guard);
    drop(iso_guard);
    drop(temp_guard);

    final_result
}
