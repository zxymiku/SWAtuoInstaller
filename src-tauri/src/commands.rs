//! Tauri 命令层（前端 → Rust）。
//!
//! 与 `src/lib/api/commands.ts` 一一对应；`start_install` 额外接收
//! `tauri::ipc::Channel<InstallEvent>`，在单次调用内完成类型安全的流式推送。

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::config::{self, Config};
use crate::events::{EnvSnapshot, InstallEvent, InstallStatus, LogLevel};
use crate::installer::{self, InstallContext};
use crate::process_utils;
use crate::win32_utils;
use crate::{AppState, Reporter};

/// 记录前端按钮和交互事件，即使尚未开始安装也写入启动会话日志。
#[tauri::command]
pub async fn log_ui_event(state: State<'_, AppState>, message: String) -> Result<(), String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("界面事件内容为空".to_string());
    }
    state.logger.write(LogLevel::Info, &format!("界面事件：{message}"));
    Ok(())
}

/// 设置本次部署使用的本地压缩包或分卷文件夹。
#[tauri::command]
pub async fn set_local_archive(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<(), String> {
    let selected = path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    state.logger.write(
        LogLevel::Info,
        &format!(
            "部署来源已更新：{}",
            selected
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "网络下载".to_string())
        ),
    );
    if let Ok(mut guard) = state.local_archive.lock() {
        *guard = selected;
        Ok(())
    } else {
        Err("无法更新本地压缩包状态".to_string())
    }
}

/// 读取当前生效的配置（默认配置 ← 用户配置，结构保留合并）。
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    state.logger.write(LogLevel::Debug, "按钮/请求：读取配置");
    let config = config::load_effective_config(&state.app_data_dir)?;
    state.store(config.clone());
    Ok(config)
}

/// 保存配置：结构保留合并后写入 `{app_data_dir}/config.toml`。
#[tauri::command]
pub async fn save_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    state.logger.write(LogLevel::Info, "按钮：保存配置");
    let mut config = config;
    config.normalize();
    config::save_user_config(&state.app_data_dir, &config)?;
    state.store(config);
    Ok(())
}

/// 重置为嵌入二进制的默认配置。
#[tauri::command]
pub async fn reset_config(state: State<'_, AppState>) -> Result<Config, String> {
    state.logger.write(LogLevel::Info, "按钮：恢复默认配置");
    let config = config::reset_user_config(&state.app_data_dir)?;
    state.store(config.clone());
    Ok(config)
}

/// 从远程 URL 拉取配置，与默认配置结构保留合并后返回。
///
/// 只返回合并结果，不自动落盘——由用户在设置页确认后点“保存”。
#[tauri::command]
pub async fn fetch_remote_config(
    state: State<'_, AppState>,
    url: String,
) -> Result<Config, String> {
    state.logger.write(LogLevel::Info, &format!("按钮：拉取远程配置；URL={url}"));
    let url = url.trim();
    if url.is_empty() {
        return Err("远程配置 URL 为空".to_string());
    }
    validate_remote_url(url)?;

    let current = state.snapshot();
    let timeout = current.fetch_timeout();

    let client = reqwest::Client::builder()
        .connect_timeout(current.connect_timeout())
        .timeout(timeout)
        .user_agent(concat!(
            "SolidWorksInstaller/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("拉取远程配置失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("远程配置返回 HTTP {}", response.status()));
    }

    const MAX_REMOTE_CONFIG_BYTES: usize = 4 * 1024 * 1024;
    if response
        .content_length()
        .is_some_and(|length| length as usize > MAX_REMOTE_CONFIG_BYTES)
    {
        return Err(format!(
            "远程配置过大（上限 {} MiB）",
            MAX_REMOTE_CONFIG_BYTES / 1024 / 1024
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取远程配置失败: {e}"))?;
    if bytes.len() > MAX_REMOTE_CONFIG_BYTES {
        return Err(format!(
            "远程配置过大（上限 {} MiB）",
            MAX_REMOTE_CONFIG_BYTES / 1024 / 1024
        ));
    }

    let text = crate::decode_console_bytes(&bytes);
    let (config, _merged) = config::merge_remote_into_default(&text)?;
    Ok(config)
}

fn validate_remote_url(raw: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(raw).map_err(|e| format!("URL 无效: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("远程配置只允许使用 HTTPS URL".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("远程配置 URL 缺少主机名".to_string());
    }
    Ok(())
}

/// 返回嵌入二进制的默认配置文本，供设置页“查看默认值”。
#[tauri::command]
pub async fn get_default_config() -> Result<String, String> {
    Ok(crate::default_config_text().to_string())
}

/// 环境快照：用户名、计算机名、ASCII 判定、管理员权限、目录、可用空间、7z 状态。
#[tauri::command]
pub async fn env_check(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<EnvSnapshot, String> {
    state.logger.write(LogLevel::Debug, "按钮/请求：刷新运行环境检测");
    let config = state.snapshot();
    let config_path = state.config_path.clone();
    let temp_dir = if config.workdir.temp_dir.trim().is_empty() {
        std::env::temp_dir()
    } else {
        PathBuf::from(config.workdir.temp_dir.trim())
    };

    let username = win32_utils::current_username();
    let computer_name = win32_utils::current_computer_name();
    let resource_dir = app.path().resource_dir().ok();

    // 7z 检测：只解析位置，不触发下载（下载发生在开始部署时）。
    let seven_zip = installer::seven_zip_status(&config, resource_dir.as_deref(), &state.app_data_dir);

    Ok(EnvSnapshot {
        username_ascii: win32_utils::is_pure_ascii(&username),
        computer_ascii: win32_utils::is_pure_ascii(&computer_name),
        username,
        computer_name,
        elevated: win32_utils::is_elevated(),
        temp_dir: temp_dir.display().to_string(),
        app_data_dir: state.app_data_dir.display().to_string(),
        config_path: config_path.display().to_string(),
        config_exists: config_path.is_file(),
        free_space_gb: win32_utils::free_space_gb(&config.install.install_drive),
        seven_zip,
    })
}

/// 打开原生文件对话框选择本地压缩包；取消时返回 `None`。
///
/// **不按扩展名筛选**：下载或手里的安装包经常挂着 `.php` / `.dat` / 无扩展名，
/// 但它们本身仍是合法压缩包。因此这里：
///   1. 对话框只给一个宽松的"常见压缩包"过滤器，但允许选择任意文件；
///   2. 选中后用 `7z t` 实际测试一次，能通过才接受；
///   3. 测试读不出格式时给出明确原因（而不是笼统的"格式不支持"）。
#[tauri::command]
pub async fn pick_archive(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    state.logger.write(LogLevel::Info, "按钮：选择本地压缩包");
    if state.install_running.load(Ordering::SeqCst) {
        return Err("安装进行中，无法切换压缩包".to_string());
    }

    // 文件对话框自带模态消息循环，放在独立线程里避免阻塞 async 调度。
    let selected = tauri::async_runtime::spawn_blocking(|| {
        win32_utils::pick_archive_file("选择 SolidWorks 2024 SP5 的压缩包（扩展名不限）")
    })
    .await
    .map_err(|e| format!("文件选择任务失败: {e}"))??;

    let Some(path) = selected else {
        return Ok(None);
    };

    let config = state.snapshot();
    let seven_zip = installer::resolve_seven_zip(
        &config,
        app.path().resource_dir().ok().as_deref(),
        Some(&state.app_data_dir),
    );
    let seven_zip = if installer::seven_zip_available(&seven_zip) {
        seven_zip
    } else if config.sevenzip.auto_download {
        state.logger.write(LogLevel::Info, "未找到 7z；按配置自动下载后继续验证本地压缩包");
        let logger = state.logger.clone();
        let app_data_dir = state.app_data_dir.clone();
        let resource_dir = app.path().resource_dir().ok();
        let ensure_config = config.clone();
        tauri::async_runtime::spawn_blocking(move || {
            installer::ensure_seven_zip(&ensure_config, &app_data_dir, resource_dir.as_deref(), &|level, message| logger.write(level, &message))
        }).await.map_err(|error| format!("下载 7z 任务失败: {error}"))??
    } else {
        return Err("未找到 7z，且 [sevenzip].auto_download = false；请先安装 7-Zip 或开启自动下载".to_string());
    };

    let kind = installer::sniff_archive_kind(&path);
    verify_picked_archive(&seven_zip.program, &path, kind, config.extract_timeout())?;

    if let Ok(mut guard) = state.local_archive.lock() {
        *guard = Some(path.clone());
    }
    state.logger.write(LogLevel::Success, &format!("本地压缩包选择完成：{}", path.display()));
    Ok(Some(path.display().to_string()))
}

/// 选择包含 `.001/.002/...` 分卷的本地文件夹，并验证首卷可由 7z 读取。
#[tauri::command]
pub async fn pick_archive_folder(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Option<String>, String> {
    state.logger.write(LogLevel::Info, "按钮：选择本地分卷文件夹");
    if state.install_running.load(Ordering::SeqCst) {
        return Err("安装进行中，无法切换压缩包".to_string());
    }

    let selected = tauri::async_runtime::spawn_blocking(|| {
        win32_utils::pick_archive_folder("选择 SolidWorks 2024 SP5 的分卷文件夹（包含 .001/.002...）")
    })
    .await
    .map_err(|e| format!("文件夹选择任务失败: {e}"))??;

    let Some(folder) = selected else {
        return Ok(None);
    };
    let config = state.snapshot();
    let seven_zip = installer::resolve_seven_zip(
        &config,
        app.path().resource_dir().ok().as_deref(),
        Some(&state.app_data_dir),
    );
    let seven_zip = if installer::seven_zip_available(&seven_zip) {
        seven_zip
    } else if config.sevenzip.auto_download {
        state.logger.write(LogLevel::Info, "未找到 7z；按配置自动下载后继续验证分卷文件夹");
        let logger = state.logger.clone();
        let app_data_dir = state.app_data_dir.clone();
        let resource_dir = app.path().resource_dir().ok();
        let ensure_config = config.clone();
        tauri::async_runtime::spawn_blocking(move || {
            installer::ensure_seven_zip(&ensure_config, &app_data_dir, resource_dir.as_deref(), &|level, message| logger.write(level, &message))
        }).await.map_err(|error| format!("下载 7z 任务失败: {error}"))??
    } else {
        return Err("未找到 7z，且 [sevenzip].auto_download = false；请先安装 7-Zip 或开启自动下载".to_string());
    };

    let entry = installer::resolve_local_archive(&folder)?;
    let kind = installer::sniff_archive_kind(&entry);
    verify_picked_archive(&seven_zip.program, &entry, kind, config.extract_timeout())?;

    if let Ok(mut guard) = state.local_archive.lock() {
        *guard = Some(folder.clone());
    }
    state.logger.write(LogLevel::Success, &format!("本地分卷文件夹选择完成：{}；入口：{}", folder.display(), entry.display()));
    Ok(Some(folder.display().to_string()))
}

/// 用 `7z t` 实际验证所选文件能否被识别与读取。
///
/// - 正常结束且退出码为 0 → 接受；
/// - 正常结束但退出码非 0（7z 明确说无法识别）→ 拒绝，并把 7z 的原话带回去；
/// - 超时 → 放行（大体积压缩包完整测试可能要很久，留给第 4 步的解压去判定）。
fn verify_picked_archive(
    seven_zip: &std::path::Path,
    path: &std::path::Path,
    kind: Option<&str>,
    extract_timeout: Duration,
) -> Result<(), String> {    // 测试预算取解压预算的 1/3，并限制在 1~10 分钟，避免用户干等。
    let test_timeout = (extract_timeout / 3)
        .clamp(Duration::from_secs(60), Duration::from_secs(600));

    let mut command = process_utils::hidden_command(seven_zip);
    command.args(["t", &path.to_string_lossy(), "-y"]);

    let outcome = process_utils::run_command(command, test_timeout, None)
        .map_err(|e| format!("无法调用 7z 验证所选文件: {e}"))?;

    if outcome.is_success() {
        return Ok(());
    }
    if !outcome.completed {
        // 超时：不阻塞用户，交由第 4 步解压时判定。
        return Ok(());
    }

    let format_hint = match kind {
        Some(kind) => format!("文件头识别为 {kind} 格式，"),
        None => "文件头未匹配已知压缩格式，".to_string(),
    };

    Err(format!(
        "所选文件无法作为压缩包读取（{format_hint}7z 退出码 {:?}）。\n\
         7z 输出末尾：\n{}",
        outcome.code,
        outcome.tail(5)
    ))
}

/// 开始安装。
///
/// 安装编排跑在独立 OS 线程中（大量同步 Win32 调用），本命令等待完成信号后返回。
/// 期间所有进度通过 `channel` 推送到前端。
#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: Channel<InstallEvent>,
    config: Config,
    local_archive: Option<String>,
) -> Result<(), String> {
    state.logger.write(LogLevel::Info, "按钮：开始自动部署");
    // 先把需要跨线程的字段取出来：`State` 带生命周期参数，不能移入线程闭包。
    let install_running = std::sync::Arc::clone(&state.install_running);
    let app_data_dir = state.app_data_dir.clone();
    let status = std::sync::Arc::clone(&state.status);
    let cancel = std::sync::Arc::clone(&state.cancel);
    let requested_local_archive = local_archive
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let local_archive = requested_local_archive.or_else(|| {
        state.local_archive.lock().ok().and_then(|guard| guard.clone())
    });

    state.logger.write(
        LogLevel::Info,
        &format!(
            "部署来源：{}",
            local_archive
                .as_deref()
                .map(|path| format!("本地路径 {}（跳过网络下载）", path.display()))
                .unwrap_or_else(|| "网络下载".to_string())
        ),
    );

    if install_running.swap(true, Ordering::SeqCst) {
        state.logger.write(LogLevel::Warn, "开始部署被拒绝：已有安装任务在运行");
        return Err("已有安装任务在运行".to_string());
    }

    // 归一化并落盘，保证本次运行与后续启动的配置一致。
    let mut config = config;
    config.normalize();
    if let Err(error) = config::save_user_config(&app_data_dir, &config) {
        install_running.store(false, Ordering::SeqCst);
        state.logger.write(LogLevel::Error, &format!("开始部署失败：保存配置失败：{error}"));
        return Err(format!("保存配置失败: {error}"));
    }
    state.store(config.clone());

    // 复位取消标记与状态快照。
    state.cancel.store(false, Ordering::SeqCst);
    state.status.reset();
    state.status.cancelled.store(false, Ordering::SeqCst);

    let reporter = Reporter::with_logger(
        {
            let channel = channel.clone();
            move |event: InstallEvent| {
                let _ = channel.send(event);
            }
        },
        state.logger.clone(),
    );
    reporter.log(
        LogLevel::Info,
        format!("详细日志文件：{}", state.logger.path.display()),
    );

    let resource_dir = app.path().resource_dir().ok();

    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();

    reporter.log(LogLevel::Info, "安装任务已进入队列");

    // `install_running` 会被移入线程闭包，错误分支需要独立的一份引用。
    let running_for_error = std::sync::Arc::clone(&install_running);

    let logger_for_spawn = state.logger.clone();
    let handle = std::thread::Builder::new()
        .name("sw-installer".to_string())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut ctx = InstallContext::new(
                    config,
                    app_data_dir,
                    std::sync::Arc::clone(&status),
                    std::sync::Arc::clone(&cancel),
                    reporter.clone(),
                    local_archive,
                );
                ctx.resource_dir = resource_dir;

                installer::run(&mut ctx)
            }));

            install_running.store(false, Ordering::SeqCst);
            status.running.store(false, Ordering::SeqCst);

            let result = match outcome {
                Ok(inner) => inner,
                Err(_) => {
                    reporter.log(
                        LogLevel::Error,
                        "安装线程发生 panic：Rust 安装线程异常退出，请检查详细日志和 Windows 事件查看器",
                    );
                    Err("安装线程发生 panic，请查看日志".to_string())
                }
            };

            let _ = done_tx.send(result);
        })
        .map_err(|e| {
            running_for_error.store(false, Ordering::SeqCst);
            logger_for_spawn.write(LogLevel::Error, &format!("创建安装线程失败：{e}"));
            format!("创建安装线程失败: {e}")
        })?;
    // 等待完成信号：既保持 Channel 打开，又让前端能 await 整个流程。
    let wait = tauri::async_runtime::spawn_blocking(move || {
        let received = done_rx.recv_timeout(Duration::from_secs(24 * 60 * 60));
        // 线程句柄只用于 join 兜底，不参与结果传递。
        let _ = handle.join();
        received
    })
    .await
    .map_err(|e| {
        state.logger.write(LogLevel::Error, &format!("等待安装任务失败：{e}"));
        format!("等待安装任务失败: {e}")
    })?;

    let result = match wait {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err("安装任务超过 24 小时未返回，已停止等待".to_string())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("安装线程异常退出，未返回结果".to_string())
        }
    };
    match &result {
        Ok(()) => state.logger.write(LogLevel::Success, "自动部署命令返回成功"),
        Err(error) => state.logger.write(LogLevel::Error, &format!("自动部署命令返回失败：{error}")),
    }
    result
}

/// 请求取消安装。当前阶段会在下一个检查点结束。
#[tauri::command]
pub async fn cancel_install(state: State<'_, AppState>) -> Result<(), String> {
    state.logger.write(LogLevel::Warn, "按钮：取消部署");
    state.cancel.store(true, Ordering::SeqCst);
    state.status.cancelled.store(true, Ordering::SeqCst);
    state.status.set_message("正在取消…");
    Ok(())
}

/// 安装状态快照（页面刷新或重新挂载时用于恢复 UI）。
#[tauri::command]
pub async fn get_install_status(state: State<'_, AppState>) -> Result<InstallStatus, String> {
    Ok(state.install_status())
}

/// 把前端持有的日志文本导出到 `{app_data_dir}/logs/`。
///
/// 由 Rust 落盘而不是走浏览器下载：Tauri 的 WebView 对 `blob:` 下载支持有限，
/// 而且写入应用数据目录更便于用户事后查找。
#[tauri::command]
pub async fn export_logs(
    state: State<'_, AppState>,
    contents: String,
    label: String,
) -> Result<String, String> {
    if contents.trim().is_empty() {
        return Err("没有可导出的日志内容".to_string());
    }

    let dir = state.app_data_dir.join("logs");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("创建日志目录失败 {}: {e}", dir.display()))?;

    let label = safe_file_label(&label, "install");
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("{label}-{stamp}.log"));

    std::fs::write(&path, contents.as_bytes())
        .map_err(|e| format!("写入日志文件失败 {}: {e}", path.display()))?;

    Ok(path.display().to_string())
}

/// 导出当前配置为 TOML 文件。
///
/// **合并语义**：先把传入的配置与磁盘上的用户配置做**结构保留合并**，
/// 再与嵌入式默认配置合并。效果是：
///
/// - 用户已有的值与注释原样保留；
/// - 前端只改了部分字段时，未涉及的项保持磁盘上的现有值（**不会被清空**）；
/// - 缺失的新字段从默认配置补齐（带默认注释）。
///
/// 导出目标由原生保存对话框决定；用户取消对话框时，自动写到
/// `{app_data_dir}/exports/` 下的时间戳文件，并把完整路径回给前端。
#[tauri::command]
pub async fn export_config(
    state: State<'_, AppState>,
    app: AppHandle,
    config: Config,
    suggested_name: Option<String>,
) -> Result<ExportOutcome, String> {
    let mut incoming = config;
    incoming.normalize();

    // 1) overlay = 磁盘上的用户配置 ← 前端传入的配置
    //
    //    这里用"留空保留"模式：表单里没填的项保持磁盘上的现有值，
    //    而不是被空字符串 / 空数组清掉 —— 这正是导出功能的语义要求。
    let user_text = config::read_user_config_text(&state.app_data_dir)?;
    let incoming_text = incoming.to_toml()?;
    let merged_user = config::merge_toml_preserving_empty(&user_text, &incoming_text)?;

    // 2) 再与默认配置合并，补齐缺失字段并带上默认注释
    let merged = config::merge_toml(config::DEFAULT_CONFIG_TOML, &merged_user)?;

    // 3) 导出前校验：合并结果必须能被解析回来，避免导出坏文件
    if let Err(error) = Config::from_toml(&merged) {
        return Err(format!(
            "合并后的配置无法解析，已取消导出以免写出损坏文件: {error}"
        ));
    }

    let default_name = suggested_name
        .as_deref()
        .map(|name| safe_file_label(name, "config"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            format!(
                "solidworks-config-{}",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            )
        });
    let default_name = if default_name.to_ascii_lowercase().ends_with(".toml") {
        default_name
    } else {
        format!("{default_name}.toml")
    };

    // 4) 原生保存对话框（带默认文件名）
    let picked = tauri::async_runtime::spawn_blocking({
        let title = "导出当前配置".to_string();
        let name = default_name.clone();
        move || win32_utils::pick_save_file(&title, &name)
    })
    .await
    .map_err(|e| format!("保存对话框任务失败: {e}"))??;

    let _ = app;

    let (path, used_fallback) = match picked {
        Some(path) => (path, false),
        None => {
            // 用户取消对话框 → 落到应用数据目录的 exports/，路径回给前端
            let dir = state.app_data_dir.join("exports");
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("创建导出目录失败 {}: {e}", dir.display()))?;
            (dir.join(&default_name), true)
        }
    };

    std::fs::write(&path, merged.as_bytes())
        .map_err(|e| format!("写入配置文件失败 {}: {e}", path.display()))?;

    Ok(ExportOutcome {
        path: path.display().to_string(),
        bytes: merged.len(),
        lines: merged.lines().count(),
        used_fallback,
    })
}

/// `export_config` 的返回值。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportOutcome {
    /// 实际写出的完整路径。
    pub path: String,
    /// 字节数。
    pub bytes: usize,
    /// 行数（便于前端提示"导出了多少行配置"）。
    pub lines: usize,
    /// 是否因为用户取消对话框而落到默认目录。
    pub used_fallback: bool,
}

/// 把任意字符串压成安全的文件名片段。
fn safe_file_label(label: &str, fallback: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(['-', '.'].as_slice());
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

/// 重新检测安全软件（只读，不执行任何处置）。
///
/// 用户在系统托盘里手动退出杀软后点「重新检测」，用这个命令确认结果：
/// 返回当前仍在 SecurityCenter2 中注册的安全软件，列表为空即表示已全部退出。
#[tauri::command]
pub async fn recheck_antivirus() -> Result<Vec<crate::antivirus::SecurityProduct>, String> {
    let products = tauri::async_runtime::spawn_blocking(crate::antivirus::detect_products)
        .await
        .map_err(|e| format!("检测任务失败: {e}"))?;
    Ok(products)
}
