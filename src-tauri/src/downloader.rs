//! 多线程分块下载引擎。
//!
//! 设计要点（对应 AGENTS.md 的下载引擎章节）：
//!   - **预生成工作池**：worker 一次性生成并等待队列，连接池保持预热，
//!     避免每个分块都做一次任务生成开销；
//!   - **HTTP Range 分块**：分块数即 `config[download].threads`（1~255）；
//!   - **断点续传**：`.part` 数据文件 + `.state` 位图，中断后只补未完成分块；
//!   - **SHA-256 校验**：下载完成后重新读取文件计算哈希再改名；
//!   - **实时速度**：引擎内计算一次，通过进度回调推送；
//!   - **指数退避重试**：等待时长来自 `retry_backoff_minutes`。
//!
//! 写入模型：worker 只负责把区间字节取回并投递到无界通道，
//! 单一写入线程按偏移落盘——`File` 的 `seek + write_all` 在单线程下无需加锁，
//! 也不会出现两个 worker 交叉写同一分块的情况。

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::stream::{FuturesUnordered, StreamExt};
use reqwest::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use sha2::{Digest, Sha256};

use crate::events::LogLevel;
#[path = "downloader/chunk_state.rs"]
mod state;
use state::ChunkState;

/// 分块结果通道。
///
/// 使用 `std::sync::mpsc` 而非 tokio 通道：写入端在同步线程中消费，
/// std 通道的无界 `send` 不阻塞、也不需要运行时上下文。
type ChunkSender = std::sync::mpsc::Sender<ChunkResult>;
type ChunkReceiver = std::sync::mpsc::Receiver<ChunkResult>;

/// 进度回调。实现方负责把消息转成 `InstallEvent` 推到前端。
pub trait ProgressSink: Send + Sync + 'static {
    fn log(&self, level: LogLevel, message: &str);
    fn download_progress(
        &self,
        bytes_done: u64,
        bytes_total: u64,
        speed_bps: u64,
        chunks_done: usize,
        chunks_total: usize,
    );
    /// 上报一次剩余时间估算（秒）。
    fn estimate(&self, seconds: u64);
    /// 是否已请求取消。
    fn cancelled(&self) -> bool;

    /// 单个文件的下载状态。
    ///
    /// 单包下载时不关心；**多文件/分片下载**时用来渲染"每个文件下到哪了"的清单。
    /// 有默认空实现，避免所有 `ProgressSink` 都被迫实现它。
    #[allow(clippy::too_many_arguments)]
    fn file_status(
        &self,
        _index: usize,
        _total: usize,
        _name: &str,
        _bytes_done: u64,
        _bytes_total: u64,
        _speed_bps: u64,
        _phase: &str,
        _detail: &str,
    ) {
    }
}

/// 单次运行的结果。
#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    pub path: PathBuf,
    pub bytes: u64,
    pub resumed: bool,
    pub verified: bool,
    pub elapsed: Duration,
}

/// 下载错误。
#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    Network(String),
    Server(String),
    Io(String),
    Checksum { expected: String, actual: String },
    Busy,
    /// 服务器返回的是登录页 / HTML 页面，而不是文件。
    ///
    /// 典型场景：网盘分享链接是**私有分享**，未登录时 `download.aspx`
    /// 会 302 到 `login.live.com` 并返回一个 200 的 HTML 登录页。
    /// 如果不识别这种情况，程序会把登录页当成压缩包存下来，
    /// 然后在一堆看不懂的 7z 报错里浪费很久。
    AuthRequired(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::Cancelled => write!(f, "下载已取消"),
            DownloadError::Network(message) => write!(f, "网络错误: {message}"),
            DownloadError::Server(message) => write!(f, "服务器拒绝请求: {message}"),
            DownloadError::Io(message) => write!(f, "文件读写错误: {message}"),
            DownloadError::Checksum { expected, actual } => {
                write!(f, "SHA-256 校验失败：期望 {expected}，实际 {actual}")
            }
            DownloadError::Busy => write!(f, "已有下载任务在运行"),
            DownloadError::AuthRequired(detail) => write!(
                f,
                "服务器返回的是登录页而不是文件（需要登录网盘账号）: {detail}"
            ),
        }
    }
}

impl std::error::Error for DownloadError {}

/// `[download]` 段在下载引擎中的投影（单位已换算为 Duration）。
#[derive(Debug, Clone)]
pub struct DownloadSettings {
    pub url: String,
    pub threads: usize,
    pub checksum: String,
    pub retries: u32,
    /// 预计算的退避序列，长度 = `retries`。
    pub backoffs: Vec<Duration>,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
}

/// 一个分块的下载结果。
#[derive(Debug)]
struct ChunkResult {
    index: usize,
    start: u64,
    length: u64,
    attempt: u32,
    outcome: Result<Vec<u8>, String>,
}

/// 远端探测结果。
#[derive(Debug, Clone, Copy)]
struct Probe {
    length: u64,
    range_supported: bool,
}

/// 下载引擎。
pub struct DownloadEngine {
    settings: DownloadSettings,
    client: reqwest::Client,
    sink: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
    running: AtomicBool,
    target: PathBuf,
    completed_bytes: Arc<AtomicU64>,
    session_bytes: Arc<AtomicU64>,
    total_bytes: Arc<AtomicU64>,
    chunks_total: Arc<AtomicUsize>,
    chunks_done: Arc<AtomicUsize>,
}

impl DownloadEngine {
    /// 构造引擎。`target_file` 为最终文件路径，同目录下生成 `.part` / `.state`。
    pub fn new(
        settings: DownloadSettings,
        target_file: PathBuf,
        sink: Arc<dyn ProgressSink>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, DownloadError> {
        let client = reqwest::Client::builder()
            .connect_timeout(settings.connect_timeout)
            .timeout(settings.read_timeout)
            .pool_max_idle_per_host(settings.threads.clamp(1, 255))
            .user_agent(concat!(
                "SolidWorksInstaller/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| DownloadError::Network(format!("构建 HTTP 客户端失败: {e}")))?;

        Ok(Self {
            settings,
            client,
            sink,
            cancel,
            running: AtomicBool::new(false),
            target: target_file,
            completed_bytes: Arc::new(AtomicU64::new(0)),
            session_bytes: Arc::new(AtomicU64::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            chunks_total: Arc::new(AtomicUsize::new(0)),
            chunks_done: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// 取消标记，交给上层共享。
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst) || self.sink.cancelled()
    }

    /// 在已有 Tokio 运行时上执行下载（Tauri 的 async runtime）。
    pub async fn run(&self) -> Result<DownloadOutcome, DownloadError> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(DownloadError::Busy);
        }
        let result = self.run_inner().await;
        self.running.store(false, Ordering::SeqCst);
        result
    }

    async fn run_inner(&self) -> Result<DownloadOutcome, DownloadError> {
        let started = Instant::now();
        let part_path = part_path_of(&self.target);
        let state_path = state_path_of(&self.target);

        if let Some(parent) = self.target.parent() {
            fs::create_dir_all(parent).map_err(|e| DownloadError::Io(e.to_string()))?;
        }

        // --- 1. 远端探测 ---------------------------------------------------
        let probe = self.probe().await?;
        self.total_bytes.store(probe.length, Ordering::SeqCst);

        if probe.length == 0 {
            return self.download_streaming(&part_path, started).await;
        }

        let chunk_size = plan_chunk_size(probe.length, self.settings.threads);
        let chunks = probe.length.div_ceil(chunk_size) as usize;
        self.chunks_total.store(chunks, Ordering::SeqCst);

        self.sink.log(
            LogLevel::Info,
            &format!(
                "目标 {} 字节 · 分块 {} 个（每块 {} 字节）· 线程上限 {}",
                probe.length, chunks, chunk_size, self.settings.threads
            ),
        );
        self.sink.log(
            LogLevel::Debug,
            &format!("服务端支持 Range: {}", probe.range_supported),
        );

        // --- 2. 续传状态 ---------------------------------------------------
        let existing_len = fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        let mut state = ChunkState::load(state_path.clone(), chunks);

        if !probe.range_supported {
            if existing_len > 0 {
                self.sink.log(
                    LogLevel::Warn,
                    "服务端不支持 Range，丢弃已有分片重新开始",
                );
                let _ = fs::remove_file(&part_path);
            }
            state.flags.iter_mut().for_each(|flag| *flag = 0);
        } else if existing_len != probe.length {
            self.sink.log(
                LogLevel::Warn,
                &format!(
                    "分片长度 {existing_len} 与远端 {} 不一致，将按分块重新校验",
                    probe.length
                ),
            );
            // 文件被替换或 URL 指向了新版本时，旧位图不能继续使用；
            // 否则未下载的分块会与旧内容混合，且在未配置 SHA-256 时无法发现。
            state.flags.fill(0);
            state.persist().map_err(DownloadError::Io)?;
            let _ = fs::remove_file(&part_path);
        }

        let resumed = existing_len > 0 && state.done_count() > 0;
        self.chunks_done.store(state.done_count(), Ordering::SeqCst);
        self.completed_bytes.store(
            state
                .flags
                .iter()
                .enumerate()
                .filter(|(_, flag)| **flag == 1)
                .map(|(index, _)| chunk_len(index, chunk_size, probe.length))
                .sum::<u64>()
                .min(probe.length),
            Ordering::SeqCst,
        );

        if state.all_done() {
            self.sink.log(
                LogLevel::Success,
                "所有分块在此前会话中已完成，直接进入校验",
            );
        } else {
            self.sink.log(
                LogLevel::Info,
                &format!(
                    "{} · 待下载分块 {}/{}",
                    if resumed {
                        "命中续传状态"
                    } else {
                        "开始全新下载"
                    },
                    state.remaining(),
                    chunks
                ),
            );
        }

        // --- 3. 数据文件与写入线程 -----------------------------------------
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&part_path)
            .map_err(|e| DownloadError::Io(format!("打开分片文件失败: {e}")))?;
        // 预分配到目标长度：后续写入全部落在已分配区间内。
        file.set_len(probe.length.max(existing_len))
            .map_err(|e| DownloadError::Io(format!("预分配分片大小失败: {e}")))?;
        drop(file);

        let (chunk_tx, chunk_rx) = std::sync::mpsc::channel::<ChunkResult>();
        let writer = spawn_writer(
            part_path.clone(),
            chunk_rx,
            self.sink.clone(),
            self.is_cancelled_flag(),
        );

        // --- 4. 进度上报（固定 200ms 一拍） --------------------------------
        let progress_stop = Arc::new(AtomicBool::new(false));
        let progress_handle = spawn_progress_reporter(
            self.sink.clone(),
            Arc::clone(&self.completed_bytes),
            Arc::clone(&self.session_bytes),
            Arc::clone(&self.total_bytes),
            Arc::clone(&self.chunks_total),
            Arc::clone(&self.chunks_done),
            Arc::clone(&progress_stop),
            started,
        );

        // --- 5. 预生成 worker 池，从待办队列领分块 --------------------------
        let pending: Vec<usize> = (0..chunks).filter(|i| !state.is_done(*i)).collect();
        let queue: Arc<Vec<usize>> = Arc::new(pending);
        let cursor = Arc::new(AtomicUsize::new(0));
        let worker_count = self.settings.threads.clamp(1, 255).min(queue.len().max(1));

        self.sink.log(
            LogLevel::Info,
            &format!("生成 {worker_count} 个 worker 接管 {} 个分块", queue.len()),
        );

        let mut in_flight: FuturesUnordered<_> = FuturesUnordered::new();
        for _ in 0..worker_count {
            in_flight.push(self.run_worker(
                Arc::clone(&queue),
                Arc::clone(&cursor),
                chunk_tx.clone(),
                chunk_size,
                probe.length,
            ));
        }

        // --- 6. 等待所有 worker 收工 ---------------------------------------
        while in_flight.next().await.is_some() {}

        progress_stop.store(true, Ordering::SeqCst);
        let _ = progress_handle.join();

        // 关闭通道，让写入线程处理完剩余消息后退出。
        drop(chunk_tx);
        let mut failures = writer
            .join()
            .map_err(|_| DownloadError::Io("写入线程异常退出".to_string()))?
            .map_err(DownloadError::Io)?;

        if self.is_cancelled() {
            self.sink.log(
                LogLevel::Warn,
                "下载已取消，保留分片与状态文件以便下次续传",
            );
            return Err(DownloadError::Cancelled);
        }

        if !state.all_done() {
            // 二次尝试：把仍缺失的分块按序补齐（网络抖动常见于长连接边缘）。
            self.sink.log(
                LogLevel::Warn,
                &format!(
                    "首轮结束后仍有 {}/{} 个分块未完成，进行补齐轮次",
                    state.remaining(),
                    chunks
                ),
            );
            failures.extend(
                self.retry_missing(
                    &mut state,
                    chunk_size,
                    probe.length,
                    &part_path,
                )
                .await,
            );
        }

        if !state.all_done() {
            let detail = if failures.is_empty() {
                "存在未完成分块".to_string()
            } else {
                failures.join("; ")
            };
            return Err(DownloadError::Network(format!(
                "下载未完成（{}/{} 个分块）: {detail}",
                state.done_count(),
                chunks
            )));
        }

        // --- 7. 尺寸 + 哈希校验，改名为最终文件 -----------------------------
        let actual_len = fs::metadata(&part_path)
            .map_err(|e| DownloadError::Io(format!("读取分片大小失败: {e}")))?
            .len();
        if actual_len != probe.length {
            return Err(DownloadError::Io(format!(
                "分片大小 {actual_len} 与远端声明 {} 不一致",
                probe.length
            )));
        }

        let verified = self.verify_checksum(&part_path)?;

        if self.target.exists() {
            let _ = fs::remove_file(&self.target);
        }
        fs::rename(&part_path, &self.target)
            .map_err(|e| DownloadError::Io(format!("重命名分片文件失败: {e}")))?;
        state.remove();

        Ok(DownloadOutcome {
            path: self.target.clone(),
            bytes: actual_len,
            resumed,
            verified,
            elapsed: started.elapsed(),
        })
    }

    /// 共享给写入线程的取消标记。
    fn is_cancelled_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    /// 补齐轮次：对仍然缺失的分块串行重试，避免再次打满带宽。
    async fn retry_missing(
        &self,
        state: &mut ChunkState,
        chunk_size: u64,
        total: u64,
        part_path: &Path,
    ) -> Vec<String> {
        let missing: Vec<usize> = (0..state.flags.len())
            .filter(|index| !state.is_done(*index))
            .collect();

        let mut errors = Vec::new();
        for index in missing {
            if self.is_cancelled() {
                break;
            }
            let start = index as u64 * chunk_size;
            let length = chunk_len(index, chunk_size, total);
            let mut attempt = 0u32;

            loop {
                attempt += 1;
                match self.fetch_chunk(start, length).await {
                    Ok(bytes) => {
                        // 直接同步写入：此时已无并发 reader。
                        if let Err(error) = write_at(part_path, start, &bytes) {
                            errors.push(format!("分块 {index} 写入失败: {error}"));
                            break;
                        }
                        state.mark(index);
                        self.completed_bytes.fetch_add(bytes.len() as u64, Ordering::SeqCst);
                        self.session_bytes.fetch_add(bytes.len() as u64, Ordering::SeqCst);
                        self.chunks_done.fetch_add(1, Ordering::SeqCst);
                        if let Err(error) = state.persist() {
                            self.sink.log(LogLevel::Warn, &error);
                        }
                        break;
                    }
                    Err(error) => {
                        if attempt > self.settings.retries || self.is_cancelled() {
                            errors.push(format!("分块 {index} 补齐失败: {error}"));
                            break;
                        }
                        let wait = self.backoff_for(attempt);
                        tokio::time::sleep(wait).await;
                    }
                }
            }
        }
        errors
    }

    fn backoff_for(&self, attempt: u32) -> Duration {
        self.settings
            .backoffs
            .get((attempt.saturating_sub(1)) as usize)
            .copied()
            .unwrap_or_else(|| Duration::from_secs(1))
    }

    /// 远端探测：优先 HEAD，必要时回退到 `Range: bytes=0-0`。
    async fn probe(&self) -> Result<Probe, DownloadError> {
        // 先做一次 HEAD，顺带识别「登录页冒充文件」的情况。
        if let Ok(response) = self.client.head(&self.settings.url).send().await {
            let content_type = header_text(&response, reqwest::header::CONTENT_TYPE.as_str());
            let disposition =
                header_text(&response, reqwest::header::CONTENT_DISPOSITION.as_str());
            let final_url = response.url().to_string();

            // 明确是 HTML 且没有 Content-Disposition 时，几乎可以断定拿到的是网页。
            if looks_like_html(&content_type) && disposition.is_none() {
                // 再读一点内容确认（HEAD 没有 body，用一次范围 GET）。
                if let Some(reason) = self.detect_auth_wall().await? {
                    return Err(DownloadError::AuthRequired(reason));
                }
                // 读不到内容但确实声明是 HTML：同样按网页处理，避免把网页当压缩包。
                return Err(DownloadError::AuthRequired(format!(
                    "响应 Content-Type 为 {}（不是文件），最终地址 {final_url}",
                    content_type.as_deref().unwrap_or("<无>")
                )));
            }

            if response.status().is_success() {
                let length = header_u64(&response, CONTENT_LENGTH.as_str()).unwrap_or(0);
                let accepts_ranges = response
                    .headers()
                    .get(ACCEPT_RANGES)
                    .and_then(|value| value.to_str().ok())
                    .map(|text| text.to_ascii_lowercase().contains("bytes"))
                    .unwrap_or(false);
                if length > 0 {
                    return Ok(Probe {
                        length,
                        range_supported: accepts_ranges,
                    });
                }
            }
        }

        let response = self
            .client
            .get(&self.settings.url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
            .map_err(|e| DownloadError::Network(e.to_string()))?;

        match response.status() {
            StatusCode::PARTIAL_CONTENT => {
                let total = response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|text| text.rsplit('/').next().map(|s| s.trim().to_string()))
                    .and_then(|text| text.parse::<u64>().ok())
                    .unwrap_or(0);
                Ok(Probe {
                    length: total,
                    range_supported: true,
                })
            }
            status if status.is_success() => {
                // 200 而不是 206：服务端不支持 Range。此时要确认返回的确实是文件。
                let content_type =
                    header_text(&response, reqwest::header::CONTENT_TYPE.as_str());
                let disposition =
                    header_text(&response, reqwest::header::CONTENT_DISPOSITION.as_str());
                if looks_like_html(&content_type) && disposition.is_none() {
                    return Err(DownloadError::AuthRequired(format!(
                        "响应 Content-Type 为 {}（不是文件）",
                        content_type.as_deref().unwrap_or("<无>")
                    )));
                }
                Ok(Probe {
                    length: response.content_length().unwrap_or(0),
                    range_supported: false,
                })
            }
            status => Err(DownloadError::Server(format!(
                "探测请求返回 {status}（{}）",
                self.settings.url
            ))),
        }
    }

    /// 读少量内容判断是否落在登录页 / 错误页上。
    async fn detect_auth_wall(&self) -> Result<Option<String>, DownloadError> {
        const SNIFF_BYTES: u64 = 4096;

        let response = self
            .client
            .get(&self.settings.url)
            .header(RANGE, format!("bytes=0-{SNIFF_BYTES}"))
            .send()
            .await
            .map_err(|e| DownloadError::Network(e.to_string()))?;

        let final_url = response.url().to_string();
        let content_type = header_text(&response, reqwest::header::CONTENT_TYPE.as_str());

        // 只有声明是文本才读 body，避免把二进制文件读进来。
        if !looks_like_text(&content_type) {
            return Ok(None);
        }

        let body = response
            .text()
            .await
            .map_err(|e| DownloadError::Network(e.to_string()))?;
        let lower = body.to_ascii_lowercase();

        // 命中任一特征即认定是登录页。
        let hit = AUTH_PAGE_MARKERS
            .iter()
            .find(|marker| lower.contains(**marker));

        if let Some(marker) = hit {
            return Ok(Some(format!(
                "页面特征「{marker}」，最终地址 {final_url}"
            )));
        }

        // 最终地址落到登录域名也算。
        if final_url.contains("login.live.com") || final_url.contains("/Authenticate.aspx") {
            return Ok(Some(format!("被重定向到登录地址 {final_url}")));
        }

        Ok(None)
    }

    /// 单个 worker：循环领号 → 下载 → 投递，直到队列耗尽。
    async fn run_worker(
        &self,
        queue: Arc<Vec<usize>>,
        cursor: Arc<AtomicUsize>,
        tx: ChunkSender,
        chunk_size: u64,
        total: u64,
    ) {
        loop {
            if self.is_cancelled() {
                return;
            }
            let position = cursor.fetch_add(1, Ordering::SeqCst);
            let Some(&index) = queue.get(position) else {
                return;
            };

            let start = index as u64 * chunk_size;
            let length = chunk_len(index, chunk_size, total);

            let mut attempt = 0u32;
            let result = loop {
                attempt += 1;
                match self.fetch_chunk(start, length).await {
                    Ok(bytes) => break Ok(bytes),
                    Err(error) => {
                        if attempt > self.settings.retries || self.is_cancelled() {
                            break Err(error);
                        }
                        let wait = self.backoff_for(attempt);
                        self.sink.log(
                            LogLevel::Warn,
                            &format!(
                                "分块 {index} 第 {attempt} 次失败（{error}），{:.1} 秒后重试",
                                wait.as_secs_f64()
                            ),
                        );
                        self.sink.estimate(wait.as_secs());
                        tokio::time::sleep(wait).await;
                    }
                }
            };

            if tx
                .send(ChunkResult {
                    index,
                    start,
                    length,
                    attempt,
                    outcome: result,
                })
                .is_err()
            {
                // 写入线程已退出：说明磁盘或通道出错，停止领号。
                return;
            }
        }
    }

    /// 下载一个字节区间，返回完整字节内容。
    async fn fetch_chunk(&self, start: u64, length: u64) -> Result<Vec<u8>, String> {
        let end = start + length - 1;
        let response = self
            .client
            .get(&self.settings.url)
            .header(RANGE, format!("bytes={start}-{end}"))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let status = response.status();
        if !(status.is_success() || status == StatusCode::PARTIAL_CONTENT) {
            return Err(format!("HTTP {status}"));
        }
        // 服务端忽略 Range 时必须报错，否则会写坏文件。
        if status == StatusCode::OK && start != 0 {
            return Err("服务端忽略了 Range 请求".to_string());
        }

        let mut collected = Vec::with_capacity(length as usize);
        let mut stream = response.bytes_stream();
        while let Some(item) = stream.next().await {
            if self.is_cancelled() {
                return Err("已取消".to_string());
            }
            let bytes = item.map_err(|e| format!("读取响应流失败: {e}"))?;
            collected.extend_from_slice(&bytes);
        }

        if collected.len() as u64 != length {
            return Err(format!(
                "分块字节数 {} 与期望 {} 不符",
                collected.len(),
                length
            ));
        }
        Ok(collected)
    }

    /// 无 `Content-Length` 时的单连接回退（不支持断点续传）。
    async fn download_streaming(
        &self,
        part_path: &Path,
        started: Instant,
    ) -> Result<DownloadOutcome, DownloadError> {
        self.sink.log(
            LogLevel::Warn,
            "服务端未提供 Content-Length，回退到单连接流式下载（不支持续传）",
        );

        let response = self
            .client
            .get(&self.settings.url)
            .send()
            .await
            .map_err(|e| DownloadError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DownloadError::Server(format!(
                "下载请求返回 {}",
                response.status()
            )));
        }

        let mut file = File::create(part_path)
            .map_err(|e| DownloadError::Io(format!("创建分片文件失败: {e}")))?;
        let mut stream = response.bytes_stream();
        let mut written = 0u64;
        let mut last_report = Instant::now();

        while let Some(item) = stream.next().await {
            if self.is_cancelled() {
                return Err(DownloadError::Cancelled);
            }
            let bytes = item.map_err(|e| DownloadError::Network(e.to_string()))?;
            file.write_all(&bytes)
                .map_err(|e| DownloadError::Io(e.to_string()))?;
            written += bytes.len() as u64;
            self.completed_bytes.store(written, Ordering::SeqCst);
            self.session_bytes.fetch_add(bytes.len() as u64, Ordering::SeqCst);

            if last_report.elapsed() >= Duration::from_millis(200) {
                last_report = Instant::now();
                self.sink.download_progress(written, 0, 0, 0, 0);
            }
        }

        file.flush()
            .map_err(|e| DownloadError::Io(format!("刷新分片失败: {e}")))?;
        drop(file);

        let verified = self.verify_checksum(part_path)?;
        if self.target.exists() {
            let _ = fs::remove_file(&self.target);
        }
        fs::rename(part_path, &self.target)
            .map_err(|e| DownloadError::Io(format!("重命名分片文件失败: {e}")))?;

        Ok(DownloadOutcome {
            path: self.target.clone(),
            bytes: written,
            resumed: false,
            verified,
            elapsed: started.elapsed(),
        })
    }

    /// SHA-256 校验。`checksum_sha256` 为空时跳过并返回 `false`。
    fn verify_checksum(&self, path: &Path) -> Result<bool, DownloadError> {
        let expected = self.settings.checksum.trim().to_ascii_lowercase();
        if expected.is_empty() {
            self.sink
                .log(LogLevel::Info, "未配置 SHA-256，跳过完整性校验");
            return Ok(false);
        }

        self.sink.log(LogLevel::Info, "开始计算 SHA-256 …");
        let mut hasher = Sha256::new();
        let mut file = File::open(path).map_err(|e| DownloadError::Io(e.to_string()))?;
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|e| DownloadError::Io(e.to_string()))?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let actual = format!("{:x}", hasher.finalize());

        if actual == expected {
            self.sink
                .log(LogLevel::Success, &format!("SHA-256 校验通过: {actual}"));
            Ok(true)
        } else {
            // 校验失败必须删除分片，否则下次续传会沿用损坏数据。
            let _ = fs::remove_file(path);
            let _ = fs::remove_file(state_path_of(&self.target));
            Err(DownloadError::Checksum { expected, actual })
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 第 `index` 块的实际字节数。
fn chunk_len(index: usize, chunk_size: u64, total: u64) -> u64 {
    let start = index as u64 * chunk_size;
    let end = ((index as u64 + 1) * chunk_size).min(total);
    end.saturating_sub(start)
}

/// 分块大小：按线程数均分并钳制在 1~8 MiB，避免超大文件产生过多分块。
fn plan_chunk_size(total: u64, threads: usize) -> u64 {
    let threads = threads.clamp(1, 255) as u64;
    let ideal = total.div_ceil(threads);
    ideal.clamp(1024 * 1024, 8 * 1024 * 1024)
}

fn part_path_of(target: &Path) -> PathBuf {
    with_suffix(target, ".part")
}

fn state_path_of(target: &Path) -> PathBuf {
    with_suffix(target, ".state")
}

fn with_suffix(target: &Path, suffix: &str) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download.bin".to_string());
    name.push_str(suffix);
    target.with_file_name(name)
}

/// 登录页 / 错误页的识别特征（小写比较）。
///
/// 网盘私有分享在未登录时会返回 **200 + HTML 登录页**，
/// 状态码完全正常，只能靠内容特征识别。
const AUTH_PAGE_MARKERS: &[&str] = &[
    "login.live.com",
    "sign in to your account",
    "sign in",
    "login.microsoftonline.com",
    "oauth2/authorize",
    "microsoft account",
    "password",
    "access denied",
    "authentication required",
    "请登录",
    "登录 microsoft",
    "登录到你的帐户",
];

/// 取响应头文本。
fn header_text(response: &reqwest::Response, header: &str) -> Option<String> {
    response
        .headers()
        .get(header)
        .and_then(|value| value.to_str().ok())
        .map(|text| text.to_string())
}

/// Content-Type 是否为 HTML 类。
fn looks_like_html(content_type: &Option<String>) -> bool {
    content_type
        .as_deref()
        .map(|text| {
            let lower = text.to_ascii_lowercase();
            lower.contains("text/html") || lower.contains("application/xhtml")
        })
        .unwrap_or(false)
}

/// Content-Type 是否为可安全读入内存的文本类。
fn looks_like_text(content_type: &Option<String>) -> bool {
    match content_type {
        None => false,
        Some(text) => {
            let lower = text.to_ascii_lowercase();
            lower.starts_with("text/")
                || lower.contains("json")
                || lower.contains("xml")
                || lower.contains("javascript")
        }
    }
}

/// 对远端做一次 HEAD，拿回最终地址、长度与 `Content-Disposition`。
///
/// **只发 HEAD**：不下载正文，代价很小，可以放心在下载前为每个 URL 调一次。
/// 用途：
///
/// 1. 解析真实文件名（网盘直链的路径里往往没有文件名）；
/// 2. 提前发现「需要登录」而不是等下载完才发现存了个 HTML。
pub async fn probe_remote(
    url: &str,
    connect_timeout: Duration,
    read_timeout: Duration,
) -> Result<RemoteProbe, DownloadError> {
    let client = reqwest::Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(read_timeout)
        .build()
        .map_err(|e| DownloadError::Network(format!("构造 HTTP 客户端失败: {e}")))?;

    let response = client
        .head(url)
        .send()
        .await
        .map_err(|e| DownloadError::Network(e.to_string()))?;

    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = header_text(&response, reqwest::header::CONTENT_TYPE.as_str());
    let disposition =
        header_text(&response, reqwest::header::CONTENT_DISPOSITION.as_str());
    let length = header_u64(&response, CONTENT_LENGTH.as_str()).unwrap_or(0);

    // 需要登录：明确报出来，不要让调用方把它当成压缩包。
    if looks_like_html(&content_type) && disposition.is_none() {
        return Err(DownloadError::AuthRequired(format!(
            "响应 Content-Type 为 {}，最终地址 {final_url}",
            content_type.unwrap_or_else(|| "<无>".to_string())
        )));
    }
    if final_url.contains("login.live.com") {
        return Err(DownloadError::AuthRequired(format!(
            "被重定向到登录地址 {final_url}"
        )));
    }
    if !status.is_success() {
        return Err(DownloadError::Server(format!(
            "HEAD 返回 {status}（{url}）"
        )));
    }

    Ok(RemoteProbe {
        final_url,
        content_type,
        content_disposition: disposition,
        length,
    })
}

/// [`probe_remote`] 的结果。
#[derive(Debug, Clone)]
pub struct RemoteProbe {
    /// 跟随重定向后的最终地址。
    pub final_url: String,
    pub content_type: Option<String>,
    /// 服务器声明的文件名（最可信的来源）。
    pub content_disposition: Option<String>,
    /// 文件长度，0 表示未知。
    pub length: u64,
}

fn header_u64(response: &reqwest::Response, header: &str) -> Option<u64> {    response
        .headers()
        .get(header)
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
}

/// 按偏移写入分片文件（补齐轮次的串行写入路径）。
fn write_at(path: &Path, offset: u64, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("打开分片文件失败: {e}"))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("定位写入偏移失败: {e}"))?;
    file.write_all(bytes)
        .map_err(|e| format!("写入分片失败: {e}"))?;
    file.flush().map_err(|e| format!("刷新分片失败: {e}"))
}

/// 启动写入线程：按偏移落盘，并汇总失败分块。
fn spawn_writer(
    part_path: PathBuf,
    rx: ChunkReceiver,
    sink: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Result<Vec<String>, String>> {
    std::thread::spawn(move || {
        let mut file = OpenOptions::new()
            .write(true)
            .open(&part_path)
            .map_err(|e| format!("写入线程打开分片失败: {e}"))?;

        let mut failures = Vec::new();
        for chunk in rx.iter() {
            match chunk.outcome {
                Ok(bytes) => {
                    file.seek(SeekFrom::Start(chunk.start))
                        .map_err(|e| format!("写入线程定位偏移失败: {e}"))?;
                    file.write_all(&bytes)
                        .map_err(|e| format!("写入线程写盘失败: {e}"))?;
                    // 每块完成后立刻刷新，保证 .part 与 .state 位图同步推进。
                    file.flush()
                        .map_err(|e| format!("写入线程刷新失败: {e}"))?;
                    sink.log(
                        LogLevel::Debug,
                        &format!(
                            "分块 {} 落盘 {} 字节（第 {} 次尝试）",
                            chunk.index, chunk.length, chunk.attempt
                        ),
                    );
                }
                Err(error) => failures.push(format!(
                    "分块 {} 第 {} 次尝试失败: {error}",
                    chunk.index, chunk.attempt
                )),
            }

            // 取消后只要通道排空就立刻收工，不阻塞上层收尾。
            if cancel.load(Ordering::SeqCst) {
                // 通道关闭时 `iter` 自然结束；这里只需及时返回结果。
                continue;
            }
        }

        let _ = file.sync_all();
        Ok(failures)
    })
}

/// 启动进度上报任务（固定 200ms 采样一次，与线程数无关）。
#[allow(clippy::too_many_arguments)]
fn spawn_progress_reporter(
    sink: Arc<dyn ProgressSink>,
    completed: Arc<AtomicU64>,
    session: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    chunks_total: Arc<AtomicUsize>,
    chunks_done: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    started: Instant,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut last_session = 0u64;
        let mut last_tick = Instant::now();
        // 保留一位小数的稳定速度：滑动平均，避免瞬时抖动。
        let mut smoothed_speed = 0f64;

        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(200));
            if stop.load(Ordering::SeqCst) {
                break;
            }

            let done = completed.load(Ordering::SeqCst);
            let now = Instant::now();
            let elapsed = now.duration_since(last_tick).as_secs_f64().max(0.001);
            let session_now = session.load(Ordering::SeqCst);
            let delta = session_now.saturating_sub(last_session);
            last_session = session_now;
            last_tick = now;

            let instant_speed = delta as f64 / elapsed;
            smoothed_speed = if smoothed_speed <= 0.0 {
                instant_speed
            } else {
                smoothed_speed * 0.7 + instant_speed * 0.3
            };

            let total_bytes = total.load(Ordering::SeqCst);
            sink.download_progress(
                done,
                total_bytes,
                smoothed_speed.max(0.0) as u64,
                chunks_done.load(Ordering::SeqCst),
                chunks_total.load(Ordering::SeqCst),
            );

            if total_bytes > 0 && smoothed_speed > 1.0 && done <= total_bytes {
                let remain = (total_bytes - done) as f64 / smoothed_speed;
                sink.estimate(remain.max(0.0) as u64);
            }

            let _ = started;
        }
    })
}
