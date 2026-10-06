//! 资源守卫：Drop 兜底恢复。
//!
//! 安装过程中申请了必须归还的资源（已挂载的 ISO、临时目录）。
//! 写成守卫后，只要栈正常展开或被 `catch_unwind` 捕获，`Drop` 就会收尾——
//! 即使流程中途失败或 panic，也不会留下已挂载的镜像或未清理的临时目录。
//!
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process_utils;

pub struct IsoMountGuard {
    /// 传给 `Dismount-DiskImage` 的镜像路径。
    pub image_path: PathBuf,
    /// 卸载命令的等待预算，来自 `[sevenzip].extract_timeout_minutes`。
    pub timeout: Duration,
    /// 挂载后的盘符（`Z` 这种形式）。
    pub drive: Option<String>,
    pub mounted: bool,
    pub events: Vec<String>,
}

impl IsoMountGuard {
    pub fn mount(image: &Path, timeout: Duration) -> Result<Self, String> {
        let mut command = process_utils::hidden_command(Path::new("powershell"));
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Mount-DiskImage -ImagePath '{}' -PassThru | Get-Volume | Select-Object -ExpandProperty DriveLetter",
                image.display().to_string().replace('\'', "''")
            ),
        ]);

        let outcome = process_utils::run_command(command, timeout, None)
            .map_err(|e| format!("调用 Mount-DiskImage 失败: {e}"))?;

        if !outcome.completed || !outcome.is_success() {
            return Err(format!(
                "挂载 ISO 失败（code={:?}, timeout={}）: {}",
                outcome.code,
                outcome.timed_out,
                outcome.tail(3)
            ));
        }

        let drive = outcome
            .output
            .lines()
            .map(|line| line.trim())
            .find(|line| line.len() == 1 && line.chars().all(|c| c.is_ascii_alphabetic()))
            .map(|line| line.to_ascii_uppercase());

        let mut guard = Self {
            image_path: image.to_path_buf(),
            timeout,
            drive,
            mounted: true,
            events: Vec::new(),
        };

        match &guard.drive {
            Some(letter) => guard.events.push(format!("ISO 已挂载到 {letter}:")),
            None => {
                // 没解析出盘符时退回到「查找 setup.exe 所在盘」。
                guard
                    .events
                    .push("挂载成功但未解析出盘符，将通过 setup.exe 反查".to_string());
            }
        }

        Ok(guard)
    }

    pub fn drive(&self) -> Option<&str> {
        self.drive.as_deref()
    }

    pub fn set_drive(&mut self, letter: String) {
        self.drive = Some(letter);
    }

    /// 主动卸载；随后 Drop 不再重复执行。
    pub fn dismount_now(&mut self) -> Vec<String> {
        if !self.mounted {
            return Vec::new();
        }
        let events = vec![self.dismount_once()];
        self.mounted = false;
        events
    }

    pub fn dismount_once(&self) -> String {
        let mut command = process_utils::hidden_command(Path::new("powershell"));
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Dismount-DiskImage -ImagePath '{}'",
                self.image_path.display().to_string().replace('\'', "''")
            ),
        ]);
        match process_utils::run_command(command, self.timeout, None) {
            Ok(outcome) if outcome.is_success() => {
                format!("已卸载镜像 {}", self.image_path.display())
            }
            Ok(outcome) => format!(
                "卸载镜像返回 code={:?}（可能已自动卸载）",
                outcome.code
            ),
            Err(error) => format!("卸载镜像失败: {error}"),
        }
    }
}

impl Drop for IsoMountGuard {
    fn drop(&mut self) {
        if self.mounted {
            let _ = self.dismount_once();
            self.mounted = false;
        }
    }
}

/// 临时目录守卫：按配置决定是否在收尾时清理。
pub struct TempDirGuard {
    pub path: PathBuf,
    pub cleanup: bool,
    pub active: bool,
    pub preserve_on_failure: bool,
}

impl TempDirGuard {
    pub fn new(path: PathBuf, cleanup: bool) -> Self {
        Self {
            path,
            cleanup,
            active: true,
            preserve_on_failure: false,
        }
    }

    /// 主动清理；随后 Drop 不再重复执行。
    pub fn cleanup_now(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if !self.active {
            return events;
        }
        self.active = false;
        if self.cleanup && !self.preserve_on_failure {
            match std::fs::remove_dir_all(&self.path) {
                Ok(()) => events.push(format!("已清理工作目录 {}", self.path.display())),
                Err(error) => events.push(format!("清理工作目录失败: {error}")),
            }
        } else {
            events.push(format!(
                "按配置保留工作目录 {}（cleanup_temp_after = false）",
                self.path.display()
            ));
        }
        events
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.active && self.cleanup && !self.preserve_on_failure {
            let _ = std::fs::remove_dir_all(&self.path);
            self.active = false;
        }
    }
}
