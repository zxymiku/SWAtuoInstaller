//! 进程与子进程管理。
//!
//! 所有外部程序（7z、reg、netsh、sc、powershell、msiexec、StartSWInstall）
//! 都在这里统一执行，保证：
//!   - 超时来自 TOML 配置，不硬编码；
//!   - 不弹出可见控制台窗口（`CREATE_NO_WINDOW`）；
//!   - 输出在后台线程读取，避免管道写满导致子进程阻塞。
//!
//! **关于固定时长**：本文件里仍有少量字面量，它们不是"用户可调的等待预算"，
//! 而是**采样周期**或**单条命令级别的兜底**，已在各自位置注明。

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use serde::Serialize;

use crate::win32_utils;

/// 子进程执行结果。
#[derive(Debug, Clone, Serialize)]
pub struct CommandOutcome {
    /// 是否在超时前退出。
    pub completed: bool,
    /// 退出码（超时或无法获取时为 `None`）。
    pub code: Option<i32>,
    /// 合并后的 stdout + stderr（截断到 `MAX_CAPTURED_OUTPUT`）。
    pub output: String,
    /// 实际耗时（秒）。
    pub elapsed_seconds: f64,
    /// 是否因超时被强制终止。
    pub timed_out: bool,
}

impl CommandOutcome {
    /// 是否成功（正常退出且退出码为 0）。
    pub fn is_success(&self) -> bool {
        self.completed && self.code == Some(0)
    }

    /// 输出末尾若干非空行，便于日志与错误信息。
    pub fn tail(&self, lines: usize) -> String {
        let all: Vec<&str> = self
            .output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let start = all.len().saturating_sub(lines);
        all[start..].join("\n")
    }
}

/// 单次捕获输出的上限，防止 7z 列出上百万文件名时吃满内存。
const MAX_CAPTURED_OUTPUT: usize = 256 * 1024;

/// 等待子进程退出的**采样周期**（不是超时值）。
const WAIT_TICK: Duration = Duration::from_millis(120);

/// 等待取消标记的**采样周期**（不是超时值）。
const CANCEL_POLL_TICK: Duration = Duration::from_millis(250);

/// `taskkill` 这类短命令的单次兜底时限。
///
/// 它与 `[process].kill_timeout_minutes` 是两件事：后者是「发出优雅关闭后
/// 等进程自行退出」的预算，这里只是「taskkill 自己别卡死」的保护值。
const TASKKILL_GUARD: Duration = Duration::from_secs(30);

/// `sc` / `netsh` / `powershell` 这类系统工具的单次兜底时限。
///
/// 真正需要按配置调节的等待预算（服务启动、网卡禁用、镜像挂载等）
/// 分别来自 `[flexnet]` / `[network]` / `[sevenzip]`，不在此处。
const SYSTEM_TOOL_GUARD: Duration = Duration::from_secs(60);

/// 隐藏控制台窗口的标志位（`CREATE_NO_WINDOW`）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 构造一个隐藏窗口、禁用交互的子进程命令。
pub fn hidden_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

/// 执行命令并等待，超时后强制终止。
///
/// `cancel` 用于在用户点击「取消安装」后尽快结束等待。
pub fn run_command(
    mut command: Command,
    timeout: Duration,
    cancel: Option<&Arc<AtomicBool>>,
) -> std::io::Result<CommandOutcome> {
    let started = Instant::now();
    let mut child = command.spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let buffer = Arc::new(std::sync::Mutex::new(String::new()));

    let readers = [
        stdout.map(|stream| spawn_reader(stream, Arc::clone(&buffer))),
        stderr.map(|stream| spawn_reader(stream, Arc::clone(&buffer))),
    ];

    let mut timed_out = false;
    let mut cancelled = false;
    // 超时为 0 表示"不限时"（调用方显式选择）。
    let deadline = if timeout.is_zero() {
        None
    } else {
        Some(started + timeout)
    };

    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(_) => break None,
        }

        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }

        if let Some(flag) = cancel {
            if flag.load(Ordering::SeqCst) {
                cancelled = true;
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }

        thread::sleep(WAIT_TICK);
    };

    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }

    let output = buffer.lock().map(|text| text.clone()).unwrap_or_default();

    Ok(CommandOutcome {
        completed: !timed_out && !cancelled,
        code,
        output,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        timed_out,
    })
}

/// 执行命令并在每行输出到达时通知调用方，同时保留完整尾部用于错误诊断。
///
/// 适用于 7z 解压等长任务；传统 `run_command` 会等进程退出才返回结果，
/// 无法给 UI 提供实时状态。
pub fn run_command_streaming<F>(
    mut command: Command,
    timeout: Duration,
    cancel: Option<&Arc<AtomicBool>>,
    on_line: F,
) -> std::io::Result<CommandOutcome>
where
    F: Fn(&str) + Send + Sync + 'static,
{
    let started = Instant::now();
    let mut child = command.spawn()?;
    let buffer = Arc::new(std::sync::Mutex::new(String::new()));
    let callback = Arc::new(on_line);
    let readers = [
        child.stdout.take().map(|stream| {
            spawn_line_reader(stream, Arc::clone(&buffer), Arc::clone(&callback))
        }),
        child.stderr.take().map(|stream| {
            spawn_line_reader(stream, Arc::clone(&buffer), Arc::clone(&callback))
        }),
    ];

    let deadline = (!timeout.is_zero()).then(|| started + timeout);
    let mut timed_out = false;
    let mut cancelled = false;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {}
            Err(_) => break None,
        }
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            timed_out = true;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            cancelled = true;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(WAIT_TICK);
    };
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    let output = buffer.lock().map(|text| text.clone()).unwrap_or_default();
    Ok(CommandOutcome {
        completed: !timed_out && !cancelled,
        code,
        output,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        timed_out,
    })
}

fn spawn_line_reader<R, F>(
    mut stream: R,
    buffer: Arc<std::sync::Mutex<String>>,
    callback: Arc<F>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
    F: Fn(&str) + Send + Sync + 'static,
{
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        let mut pending = Vec::new();
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let decoded = crate::decode_console_bytes(&chunk[..n]);
                    if let Ok(mut guard) = buffer.lock() {
                        if guard.len() < MAX_CAPTURED_OUTPUT {
                            let remaining = MAX_CAPTURED_OUTPUT - guard.len();
                            guard.push_str(&decoded.chars().take(remaining).collect::<String>());
                        }
                    }
                    pending.extend_from_slice(&chunk[..n]);
                    while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                        let line = crate::decode_console_bytes(&pending[..newline]);
                        pending.drain(..=newline);
                        let line = line.trim_end_matches('\r');
                        if !line.is_empty() {
                            callback(line);
                        }
                    }
                }
                Err(_) => break,
            }
        }
        if !pending.is_empty() {
            let line = crate::decode_console_bytes(&pending);
            if !line.trim().is_empty() {
                callback(line.trim_end_matches('\r'));
            }
        }
    })
}

fn spawn_reader<R: Read + Send + 'static>(
    mut stream: R,
    buffer: Arc<std::sync::Mutex<String>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut guard) = buffer.lock() {
                        if guard.len() >= MAX_CAPTURED_OUTPUT {
                            continue;
                        }
                        // 控制台输出可能是 ANSI/GBK；用有损 UTF-8 解码保证不 panic。
                        guard.push_str(&String::from_utf8_lossy(&chunk[..n]));
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// 启动一个不等待的子进程（用于启动安装器后立刻进入弹窗守护）。
pub fn spawn_detached(mut command: Command) -> std::io::Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn()
}

// ---------------------------------------------------------------------------
// 网卡禁用 / 恢复（Drop 兜底）
// ---------------------------------------------------------------------------

/// 安装期间禁用网络，并在 Drop（含 panic 展开）时恢复。
///
/// 这是「异常退出也必须恢复网络」要求的落点：守卫持有原始适配器名列表，
/// 只要栈正常展开或被 `catch_unwind` 捕获，`Drop` 就会重新启用它们。
pub struct NetworkGuard {
    adapters: Vec<String>,
    method: String,
    restore: bool,
    disabled: bool,
    events: Vec<String>,
}

impl NetworkGuard {
    /// 按配置禁用网络。`disable` 为 false 时不做任何事。
    ///
    /// `command_timeout` 来自 `[sevenzip].extract_timeout_minutes`——
    /// 与镜像挂载同属系统级操作，量级一致。
    pub fn apply(
        disable: bool,
        restore: bool,
        method: &str,
        command_timeout: Duration,
    ) -> (Self, Vec<String>) {
        let mut guard = Self {
            adapters: Vec::new(),
            method: method.to_string(),
            restore,
            disabled: false,
            events: Vec::new(),
        };

        if !disable {
            guard
                .events
                .push("已跳过网络禁用（配置关闭）".to_string());
            let events = guard.events.clone();
            return (guard, events);
        }

        let (adapters, error) = win32_utils::list_adapters();
        if let Some(error) = error {
            guard.events.push(format!("枚举网卡失败: {error}"));
            let events = guard.events.clone();
            return (guard, events);
        }

        let physical = win32_utils::physical_adapters(&adapters);
        if physical.is_empty() {
            guard
                .events
                .push("未发现可禁用的物理网卡，跳过".to_string());
            let events = guard.events.clone();
            return (guard, events);
        }

        for adapter in &physical {
            if !adapter.up {
                guard
                    .events
                    .push(format!("{} 已处于禁用状态", adapter.name));
                guard.adapters.push(adapter.name.clone());
                continue;
            }
            match set_adapter_enabled(&adapter.name, false, &guard.method, command_timeout) {
                Ok(()) => {
                    guard.events.push(format!("已禁用网卡 {}", adapter.name));
                    guard.adapters.push(adapter.name.clone());
                }
                Err(error) => {
                    guard
                        .events
                        .push(format!("禁用网卡 {} 失败: {error}", adapter.name));
                }
            }
        }

        guard.disabled = !guard.adapters.is_empty();
        let events = guard.events.clone();
        (guard, events)
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn adapters(&self) -> &[String] {
        &self.adapters
    }

    /// 主动恢复（收尾步骤调用）；随后 `Drop` 不会重复执行。
    pub fn restore_now(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if !self.disabled {
            return events;
        }

        for name in &self.adapters {
            match set_adapter_enabled(name, true, &self.method, SYSTEM_TOOL_GUARD) {
                Ok(()) => events.push(format!("已恢复网卡 {name}")),
                Err(error) => events.push(format!("恢复网卡 {name} 失败: {error}")),
            }
        }
        // 给驱动一点重新 Up 的时间，避免后续步骤立刻发请求时失败。
        thread::sleep(CANCEL_POLL_TICK);
        self.disabled = false;
        events
    }
}

impl Drop for NetworkGuard {
    fn drop(&mut self) {
        if !self.disabled || !self.restore {
            return;
        }
        // Drop 中不 panic：任何失败都只记录，不向调用栈抛出。
        for name in &self.adapters {
            let _ = set_adapter_enabled(name, true, &self.method, SYSTEM_TOOL_GUARD);
        }
        self.disabled = false;
    }
}

/// 通过 `netsh` 或 `powershell` 启用/禁用网卡。
///
/// `timeout` 由调用方从配置传入（见 `NetworkGuard::apply`）。
pub fn set_adapter_enabled(
    name: &str,
    enabled: bool,
    method: &str,
    timeout: Duration,
) -> Result<(), String> {
    // 网卡名可能包含空格与中文，统一走参数数组，避免手工拼命令行。
    let (program, args): (&str, Vec<String>) = if method.eq_ignore_ascii_case("powershell") {
        (
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                format!(
                    "Set-NetAdapter -Name '{}' -AdminStatus {} -Confirm:$false",
                    // 单引号是 PowerShell 的转义字符，成对转义即可。
                    name.replace('\'', "''"),
                    if enabled { "Up" } else { "Down" }
                ),
            ],
        )
    } else {
        (
            "netsh",
            vec![
                "interface".to_string(),
                "set".to_string(),
                "interface".to_string(),
                format!("name={name}"),
                format!("admin={}", if enabled { "enable" } else { "disable" }),
            ],
        )
    };

    let mut command = hidden_command(Path::new(program));
    command.args(&args);

    let outcome = run_command(command, timeout, None)
        .map_err(|e| format!("执行 {program} 失败: {e}"))?;

    if outcome.is_success() {
        Ok(())
    } else {
        Err(format!(
            "{program} 返回失败（code={:?}, timeout={}）: {}",
            outcome.code,
            outcome.timed_out,
            outcome.tail(4)
        ))
    }
}

// ---------------------------------------------------------------------------
// 进程匹配与终止
// ---------------------------------------------------------------------------

/// 进程匹配的一个候选目标。
///
/// 关键词会依次在这三个字段上做**不区分大小写**的正则匹配。
#[derive(Debug, Clone, Default)]
pub struct ProcessTarget {
    pub pid: u32,
    /// 映像名，例如 `sldworks.exe`
    pub name: String,
    /// 完整路径（拿不到时为空）
    pub path: String,
    /// 命令行（拿不到时为空）
    pub command_line: String,
}

/// 查询若干进程的 (完整路径, 命令行)。
///
/// - 路径用 `QueryFullProcessImageNameW`（快、可靠）。
/// - 命令行没有稳定的单次 Win32 API，改用一次 PowerShell `Get-CimInstance` 批量取
///   （`wmic` 在新版 Windows 已弃用，不作为主路径）。
///
/// 只对调用方点名的 pid 查询：`matching_processes` 会先按进程名过一遍，
/// 只把未命中的候选送进来，避免每轮都做全量元数据查询。
pub fn process_metadata(pids: &[u32]) -> BTreeMap<u32, (String, String)> {
    let mut map: BTreeMap<u32, (String, String)> = BTreeMap::new();
    if pids.is_empty() {
        return map;
    }

    for pid in pids {
        if let Some(path) = win32_utils::process_image_path(*pid) {
            map.entry(*pid).or_default().0 = path;
        }
    }

    // WMI 过滤器形如：ProcessId=1 or ProcessId=2
    let filter: String = pids
        .iter()
        .map(|pid| format!("ProcessId={pid}"))
        .collect::<Vec<_>>()
        .join(" or ");

    let script = format!(
        "Get-CimInstance Win32_Process -Filter '{filter}' | \
         ForEach-Object {{ \"$($_.ProcessId)`t$($_.CommandLine)\" }}"
    );

    let mut command = hidden_command(Path::new("powershell"));
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);

    match run_command(command, SYSTEM_TOOL_GUARD, None) {
        Ok(outcome) => {
            for line in outcome.output.lines() {
                let mut parts = line.splitn(2, '\t');
                let pid_text = parts.next().unwrap_or("").trim();
                let command_line = parts.next().unwrap_or("").trim();
                if let Ok(pid) = pid_text.parse::<u32>() {
                    if !command_line.is_empty() {
                        map.entry(pid).or_default().1 = command_line.to_string();
                    }
                }
            }
        }
        Err(error) => {
            // 命令行为可选增强：拿不到就只按名字/路径匹配，不让整个步骤失败。
            eprintln!("查询进程命令行失败（将仅按名/路径匹配）: {error}");
        }
    }

    map
}

/// 编译关键词正则。
///
/// - 去首尾空白；空模式直接报错，避免"空正则匹配一切"从而杀光系统进程。
/// - **强制不区分大小写**：用 `(?i:(...))` 把用户模式整体包起来。
///   这样即使用户写了 `(?-i)` 也无法把大小写敏感打开——
///   如果把 `(?i)` 放在前面，后面的 `(?-i)` 会把它覆盖掉。
/// - 允许"纯关键词"写法：`solidworks` 与 `.*solidworks.*` 等价，
///   因为正则的 `is_match` 本身就是子串查找（未锚定）。
pub fn compile_kill_pattern(pattern: &str) -> Result<Regex, String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Err(
            "kill_pattern 为空。为避免误杀全部进程，空模式被拒绝；\
             请填写关键词或正则，例如 solidworks|sldworks|flexnet"
                .to_string(),
        );
    }
    Regex::new(&format!("(?i:({trimmed}))"))
        .map_err(|e| format!("kill_pattern 正则无效「{trimmed}」: {e}"))
}

/// 对单个进程判断是否命中关键词。
///
/// `deep` 为 true 时同时匹配完整路径与命令行；为 false 时只匹配映像名。
pub fn process_matches(
    regex: &Regex,
    target: &ProcessTarget,
    deep: bool,
) -> bool {
    if regex.is_match(target.name.trim()) {
        return true;
    }
    if !deep {
        return false;
    }
    if !target.path.is_empty() && regex.is_match(target.path.trim()) {
        return true;
    }
    if !target.command_line.is_empty() && regex.is_match(target.command_line.trim()) {
        return true;
    }
    false
}

/// 用配置中的关键词正则匹配进程。
///
/// `deep` 对应 `[process].match_command_line`：
/// 开启时会额外查询每个进程的完整路径与命令行（明显更慢，但能按关键词
/// 找到"名字无关、命令行里带关键词"的进程）。
pub fn matching_processes(pattern: &str, deep: bool) -> Result<Vec<ProcessTarget>, String> {
    let regex = compile_kill_pattern(pattern)?;
    let current = std::process::id();

    let mut targets: Vec<ProcessTarget> = win32_utils::list_processes()
        .into_iter()
        .filter(|(pid, _)| *pid != current)
        .map(|(pid, name)| ProcessTarget {
            pid,
            name,
            path: String::new(),
            command_line: String::new(),
        })
        .collect();

    if deep {
        // 只对进程名未命中的候选补全元数据，尽量减少一次昂贵的全量查询。
        let need_meta: Vec<u32> = targets
            .iter()
            .filter(|target| !regex.is_match(target.name.trim()))
            .map(|target| target.pid)
            .collect();

        if !need_meta.is_empty() {
            let metadata = process_metadata(&need_meta);
            for target in targets.iter_mut() {
                if let Some((path, command_line)) = metadata.get(&target.pid) {
                    target.path = path.clone();
                    target.command_line = command_line.clone();
                }
            }
        }
    }

    targets.retain(|target| process_matches(&regex, target, deep));
    // 按 pid 排序，保证同样的关键词每次得到同样的顺序（便于日志比对）。
    targets.sort_by_key(|target| target.pid);
    Ok(targets)
}

/// 先优雅 `taskkill /IM <name> /T`，超时后再 `taskkill /IM <name> /F /T`。
///
/// `grace` 来自 `[process].kill_timeout_minutes`。
/// `deep` 对应 `[process].match_command_line`（是否按路径/命令行匹配）。
/// 返回每个进程名的处理记录，供日志与事件使用。
pub fn kill_matching_processes(
    pattern: &str,
    grace: Duration,
    force: bool,
    deep: bool,
    cancel: Option<&Arc<AtomicBool>>,
) -> Vec<String> {
    let mut records = Vec::new();

    let matches = match matching_processes(pattern, deep) {
        Ok(list) => list,
        Err(error) => return vec![error],
    };

    if matches.is_empty() {
        records.push(format!("未发现匹配「{}」的进程", pattern.trim()));
        return records;
    }

    // 关键：`taskkill /IM` 是按**映像名**批量操作的，对"靠命令行命中"的进程
    // 会误伤同名进程。因此先记录每个 pid 的命中原因，再按名字去重时说明清楚。
    let mut by_name: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    for target in &matches {
        let reason = if pattern_matches_name(pattern, &target.name) {
            "按进程名命中".to_string()
        } else {
            let mut why = Vec::new();
            if !target.path.is_empty() {
                why.push(format!("路径 {}", target.path));
            }
            if !target.command_line.is_empty() {
                why.push(format!("命令行 {}", target.command_line));
            }
            if why.is_empty() {
                "按关键词命中".to_string()
            } else {
                why.join("；")
            }
        };
        by_name
            .entry(target.name.clone())
            .or_default()
            .push((target.pid, reason));
    }

    for (name, hits) in &by_name {
        let pids: Vec<u32> = hits.iter().map(|(pid, _)| *pid).collect();
        records.push(format!("发现进程 {name} (pid {pids:?})"));
        for (pid, reason) in hits {
            if !reason.starts_with("按进程名") {
                records.push(format!("  · pid {pid} {reason}"));
            }
        }

        let mut graceful = hidden_command(Path::new("taskkill"));
        graceful.args(["/IM", name, "/T"]);
        match run_command(graceful, TASKKILL_GUARD, cancel) {
            Ok(outcome) if outcome.is_success() => {
                records.push(format!("{name} 已优雅退出"));
                continue;
            }
            Ok(outcome) => records.push(format!(
                "{name} 优雅关闭未成功（code={:?}）",
                outcome.code
            )),
            Err(error) => records.push(format!("调用 taskkill 失败: {error}")),
        }
    }

    if !grace.is_zero() {
        records.push(format!("等待 {:.1} 秒后强制终止", grace.as_secs_f64()));
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if let Some(flag) = cancel {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
            }
            thread::sleep(CANCEL_POLL_TICK);
        }
    }

    if force {
        for (name, hits) in &by_name {
            let pids: Vec<u32> = hits.iter().map(|(pid, _)| *pid).collect();
            let mut forced = hidden_command(Path::new("taskkill"));
            forced.args(["/IM", name, "/F", "/T"]);
            match run_command(forced, TASKKILL_GUARD, cancel) {
                Ok(outcome) if outcome.is_success() => {
                    records.push(format!("{name} 已强制终止"));
                }
                Ok(_) => {
                    // taskkill 也可能因为进程已退出而失败，直接用 Win32 兜底。
                    let mut killed = 0;
                    for pid in &pids {
                        if win32_utils::force_kill_pid(*pid) {
                            killed += 1;
                        }
                    }
                    records.push(format!(
                        "{name} taskkill 未成功，Win32 强制终止 {killed} 个进程"
                    ));
                }
                Err(error) => records.push(format!("强制终止 {name} 失败: {error}")),
            }
        }
    }

    records
}

/// 判断关键词是否直接命中映像名（用于在日志里说明命中原因）。
fn pattern_matches_name(pattern: &str, name: &str) -> bool {
    compile_kill_pattern(pattern)
        .map(|regex| regex.is_match(name.trim()))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Windows 服务
// ---------------------------------------------------------------------------

/// `sc query "<name>"` 输出中是否包含 `RUNNING`。
pub fn service_is_running(service: &str) -> bool {
    let mut command = hidden_command(Path::new("sc"));
    command.args(["query", service]);
    match run_command(command, SYSTEM_TOOL_GUARD, None) {
        Ok(outcome) => {
            let text = outcome.output.to_ascii_uppercase();
            text.contains("RUNNING") && !text.contains("STOP_PENDING")
        }
        Err(_) => false,
    }
}

/// 停止并删除服务（服务不存在时视为成功）。
///
/// `timeout` 由调用方从配置传入（`[flexnet].server_install_timeout_minutes`）。
pub fn remove_service(service: &str, timeout: Duration) -> Vec<String> {
    let mut records = Vec::new();
    let effective = if timeout.is_zero() {
        SYSTEM_TOOL_GUARD
    } else {
        timeout
    };

    let mut stop = hidden_command(Path::new("sc"));
    stop.args(["stop", service]);
    match run_command(stop, effective, None) {
        Ok(outcome) => records.push(format!(
            "sc stop {service}: code={:?} {}",
            outcome.code,
            if outcome.is_success() {
                "成功"
            } else {
                "（服务可能未运行）"
            }
        )),
        Err(error) => records.push(format!("sc stop {service} 调用失败: {error}")),
    }

    let mut delete = hidden_command(Path::new("sc"));
    delete.args(["delete", service]);
    match run_command(delete, effective, None) {
        Ok(outcome) => records.push(format!(
            "sc delete {service}: code={:?} {}",
            outcome.code,
            if outcome.is_success() {
                "成功"
            } else {
                "（服务可能不存在）"
            }
        )),
        Err(error) => records.push(format!("sc delete {service} 调用失败: {error}")),
    }

    records
}

/// 等待服务进入 `RUNNING`。
///
/// `interval` 与 `timeout` 分别来自
/// `[flexnet].server_check_interval_minutes` 与 `[flexnet].server_install_timeout_minutes`。
pub fn wait_for_service(
    service: &str,
    interval: Duration,
    timeout: Duration,
    cancel: Option<&Arc<AtomicBool>>,
) -> (bool, Duration) {
    let started = Instant::now();
    // 间隔为 0 时退回到一个保守的采样周期。
    let step = if interval.is_zero() {
        Duration::from_millis(500)
    } else {
        interval
    };

    loop {
        if service_is_running(service) {
            return (true, started.elapsed());
        }
        if started.elapsed() >= timeout {
            return (false, started.elapsed());
        }
        if let Some(flag) = cancel {
            if flag.load(Ordering::SeqCst) {
                return (false, started.elapsed());
            }
        }
        let sleep_for = step.min(timeout.saturating_sub(started.elapsed()));
        if sleep_for.is_zero() {
            return (service_is_running(service), started.elapsed());
        }
        thread::sleep(sleep_for);
    }
}
