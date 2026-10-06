//! 第 10、11 步：关闭相关进程 + 文件替换。
//!
//! 两步都作用于安装完成之后、服务安装之前，且共同决定补丁能否落到安装目录。
//!
use std::path::Path;

use crate::events::LogLevel;
use crate::process_utils;

use super::context::InstallContext;
use super::paths::{find_dirs_named, find_first_dir};

pub fn kill_solidworks_processes(ctx: &InstallContext) {
    if !ctx.config.process.kill_sw_processes {
        ctx.info("按配置跳过进程清理（kill_sw_processes = false）");
        return;
    }

    let completion_grace = ctx.config.kill_timeout();
    let force = ctx.config.process.force_kill_after_timeout;
    let deep = ctx.config.process.match_command_line;
    ctx.info(format!(
        "清理相关进程（关键词 {}；匹配范围 {}；等待 {:.1} 分钟；强制={}）",
        ctx.config.process.kill_pattern,
        if deep {
            "进程名 + 路径 + 命令行"
        } else {
            "仅进程名"
        },
        ctx.config.process.kill_timeout_minutes,
        force
    ));

    let records = process_utils::kill_matching_processes(
        &ctx.config.process.kill_pattern,
        completion_grace,
        force,
        deep,
        Some(&ctx.cancel),
    );
    for record in records {
        ctx.log(LogLevel::Debug, record);
    }

    // 停止并删除许可服务，为第 12 步重装做准备。
    let service = "SolidWorks Flexnet Server";
    if process_utils::service_is_running(service) {
        ctx.info(format!("停止许可服务 {service}"));
        for record in process_utils::remove_service(service, ctx.config.server_install_timeout()) {
            ctx.log(LogLevel::Debug, record);
        }
    } else {
        ctx.debug(format!("服务 {service} 当前未运行"));
    }
}

// ---------------------------------------------------------------------------
// 第 11 步：文件替换
// ---------------------------------------------------------------------------

pub fn replace_files(ctx: &InstallContext, patch_root: &Path) -> Result<(), String> {
    let squad =
        find_first_dir(patch_root, "_solidsquad_")?.unwrap_or_else(|| patch_root.to_path_buf());

    // 优先 _SolidSQUAD_/SolidWorks Corp/，兼容 Program Files 结构与多级嵌套。
    // 名字比较不区分大小写（SOLIDWORKS Corp / Solidworks Corp / solidworks corp）。
    let source = find_dirs_named(&squad, "solidworks corp")
        .into_iter()
        .next()
        .or_else(|| find_dirs_named(&squad, "program files").into_iter().next())
        .unwrap_or_else(|| squad.clone());

    // `_SolidSQUAD_/SOLIDWORKS Corp` is a source container, not an extra
    // directory in the installed layout. Its children are component roots
    // (SOLIDWORKS, eDrawings, Composer, ...), so merge them directly into
    // the configured installation root.
    let target = ctx.config.resolved_install_path();

    ctx.info(format!(
        "文件替换 {} 的组件内容 → {}",
        source.display(),
        target.display()
    ));

    if !source.is_dir() {
        return Err(format!("替换源目录不存在: {}", source.display()));
    }

    std::fs::create_dir_all(&target)
        .map_err(|e| format!("创建目标目录失败 {}: {e}", target.display()))?;

    let mut copied = 0usize;
    let mut failed = 0usize;
    copy_tree(ctx, &source, &target, &mut copied, &mut failed)?;

    ctx.success(format!(
        "文件替换完成：复制 {copied} 个文件，失败 {failed} 个"
    ));

    if failed > 0 {
        return Err(format!("文件替换存在 {failed} 个失败项，请查看日志"));
    }
    Ok(())
}

fn copy_tree(
    ctx: &InstallContext,
    source: &Path,
    target: &Path,
    copied: &mut usize,
    failed: &mut usize,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(source).map_err(|e| format!("读取目录失败 {}: {e}", source.display()))?;

    for entry in entries {
        if ctx.is_cancelled() {
            return Err("文件替换被用户取消".to_string());
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                ctx.warn(format!("读取目录项失败: {error}"));
                *failed += 1;
                continue;
            }
        };

        let path = entry.path();
        let destination = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("读取文件类型失败 {}: {e}", path.display()))?;

        if file_type.is_dir() {
            if let Err(error) = std::fs::create_dir_all(&destination) {
                ctx.warn(format!("创建目录失败 {}: {error}", destination.display()));
                *failed += 1;
                continue;
            }
            copy_tree(ctx, &path, &destination, copied, failed)?;
        } else {
            match std::fs::copy(&path, &destination) {
                Ok(_) => *copied += 1,
                Err(error) => {
                    // 被占用或只读的文件：尝试清除只读位后重试一次。
                    ctx.warn(format!(
                        "复制 {} 失败: {error}，尝试清除只读属性后重试",
                        path.display()
                    ));
                    // 在 metadata 的权限副本上清除只读位，再用 set_permissions 应用。
                    // 不使用 `Permissions::set_readonly(false)`——在 Windows 上它只改动
                    // 内存中的标志、不影响其他权限位，clippy 也明确不建议这样用。
                    if let Ok(metadata) = std::fs::metadata(&destination) {
                        let mut permissions = metadata.permissions();
                        #[allow(clippy::permissions_set_readonly_false)]
                        permissions.set_readonly(false);
                        let _ = std::fs::set_permissions(&destination, permissions);
                    }
                    match std::fs::copy(&path, &destination) {
                        Ok(_) => *copied += 1,
                        Err(error) => {
                            ctx.error(format!("复制 {} 仍然失败: {error}", path.display()));
                            *failed += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
