//! 断点续传分块状态文件。

use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(super) struct ChunkState {
    pub(super) path: PathBuf,
    pub(super) flags: Vec<u8>,
}

impl ChunkState {
    pub(super) fn load(path: PathBuf, chunks: usize) -> Self {
        let mut flags = vec![0u8; chunks];
        if let Ok(text) = fs::read(&path) {
            let body: Vec<u8> = text.into_iter().filter(|b| *b == b'0' || *b == b'1').collect();
            for (index, byte) in body.iter().enumerate().take(chunks) {
                flags[index] = u8::from(*byte == b'1');
            }
        }
        Self { path, flags }
    }

    pub(super) fn is_done(&self, index: usize) -> bool {
        self.flags.get(index).copied().unwrap_or(0) == 1
    }

    pub(super) fn mark(&mut self, index: usize) {
        if let Some(slot) = self.flags.get_mut(index) {
            *slot = 1;
        }
    }

    pub(super) fn all_done(&self) -> bool {
        !self.flags.is_empty() && self.flags.iter().all(|flag| *flag == 1)
    }

    pub(super) fn done_count(&self) -> usize {
        self.flags.iter().filter(|flag| **flag == 1).count()
    }

    pub(super) fn remaining(&self) -> usize {
        self.flags.len() - self.done_count()
    }

    pub(super) fn persist(&self) -> Result<(), String> {
        let mut text = String::with_capacity(self.flags.len() + 8);
        text.push_str("SWDL1\n");
        for flag in &self.flags {
            text.push(if *flag == 1 { '1' } else { '0' });
        }
        fs::write(&self.path, text).map_err(|e| format!("写入分块状态失败: {e}"))
    }

    pub(super) fn remove(&self) {
        let _ = fs::remove_file(&self.path);
    }
}

