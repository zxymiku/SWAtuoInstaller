//! 第 6 步：导入注册表补丁。
//!
//! 收集 `_SolidSQUAD_` 下全部 `.reg` 并逐个 `reg import`（扩展名不区分大小写）。
//!
use std::path::Path;

use crate::process_utils;

use super::context::InstallContext;
use super::paths::{find_files_matching, find_first_dir};

pub fn import_registry(ctx: &InstallContext, patch_root: &Path) -> Result<(), String> {
    let squad = find_first_dir(patch_root, "_solidsquad_")?
        .unwrap_or_else(|| patch_root.to_path_buf());

    // 扩展名不区分大小写（.reg / .REG / .Reg）。
    let files = find_files_matching(&squad, "*.reg");

    if files.is_empty() {
        ctx.warn(format!(
            "{} 下没有 .reg 文件，跳过注册表导入",
            squad.display()
        ));
        return Ok(());
    }

    ctx.info(format!("发现 {} 个注册表文件，依次导入", files.len()));
    let mut failures = Vec::new();

    for (index, file) in files.iter().enumerate() {
        if ctx.is_cancelled() {
            return Err("注册表导入被用户取消".to_string());
        }

        let mut command = process_utils::hidden_command(Path::new("reg"));
        command.args(["import", &file.to_string_lossy()]);

        match process_utils::run_command(
            command,
            // 单个注册表文件的导入预算：与镜像挂载同属系统级操作，复用解压超时
            ctx.config.extract_timeout(),
            Some(&ctx.cancel),
        ) {
            Ok(outcome) if outcome.is_success() => {
                ctx.success(format!("已导入 {}", file.display()));
            }
            Ok(outcome) => {
                let message = format!(
                    "导入 {} 失败（code={:?}）: {}",
                    file.display(),
                    outcome.code,
                    outcome.tail(3)
                );
                ctx.warn(message.clone());
                failures.push(message);
            }
            Err(error) => failures.push(format!("调用 reg 失败: {error}")),
        }

        let percent = (index + 1) as f64 / files.len() as f64 * 100.0;
        ctx.progress(percent, format!("注册表 {}/{}", index + 1, files.len()));
    }

    if !failures.is_empty() {
        return Err(format!("注册表导入存在失败项: {}", failures.join("; ")));
    }
    Ok(())
}
