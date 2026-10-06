//! 第 8 步：启动静默安装器 + 弹窗守护线程。
//!
//! 首选 `StartSWInstall.exe /install /now`（`/now` 跳过 5 分钟警告对话框），
//! 不可用时回退到 `msiexec`。弹窗守护用 Win32 API 处理安装器的对话框。
//!
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::events::LogLevel;
use crate::process_utils;
use crate::win32_utils;

use super::context::InstallContext;
use std::sync::atomic::AtomicBool;

pub fn launch_installer(ctx: &InstallContext, setup_root: &Path) -> Result<(), String> {
    let install_dir = ctx.config.resolved_install_path();
    ctx.info(format!("安装目标 {}", install_dir.display()));

    let start_sw = locate_start_swinstall(setup_root);

    // 只认 `[install].force_msiexec` 这一个开关，不再被"介质里有没有 setup.exe"左右。
    //
    // **踩过的坑**：曾经加过一个 `setup.exe` 分支，在根目录存在 setup.exe 时优先执行它。
    // 两个问题：
    //   1. `setup.exe` 是**图形化引导程序**，命令行无法让它静默 ——
    //      实测它只弹出安装向导，要求用户手动勾选组件，完全违背静默安装要求；
    //   2. 它被写成 `force_msiexec && !setup_exe.is_file()`，
    //      于是只要介质根目录有 setup.exe（真实 DVD/ISO 都有），
    //      `force_msiexec = true` 就被静默否决，msiexec 永远轮不到执行。
    //
    // 正确分工：
    //   · `force_msiexec = true`  → 直接 msiexec 装 solidworks.msi（**全静默**）
    //   · `force_msiexec = false` → `sldim\startswinstall.exe /install /now`
    // 两者都由配置决定，用户想切就能切。
    let force_msiexec = ctx.config.install.force_msiexec;

    match (&start_sw, force_msiexec) {
        (Some(start_sw), false) => {
            // StartSWInstall 走的是安装管理器的命令行模式：
            //   /install  开始安装（不加就只是打开界面）
            //   /now      跳过「5 分钟后开始安装」的警告对话框
            // 额外开关由 [install].install_switches 追加，便于用户自定义行为。
            let mut args: Vec<String> = vec!["/install".to_string(), "/now".to_string()];
            for extra in &ctx.config.install.install_switches {
                let trimmed = extra.trim();
                if !trimmed.is_empty() {
                    args.push(trimmed.to_string());
                }
            }

            ctx.info(format!("使用 {} {}", start_sw.display(), args.join(" ")));

            // 工作目录设为**安装介质根**：安装管理器要在同级找
            // `swwi\`、`PreReqs\` 等目录，改成 `sldim\` 会找不到组件。
            let mut command = process_utils::hidden_command(start_sw);
            command.args(&args);
            command.current_dir(setup_root);

            match process_utils::spawn_detached(command) {
                Ok(child) => {
                    ctx.success(format!("静默安装器已启动（pid {}）", child.id()));

                    // 组件白名单只对 msiexec 路径有效：StartSWInstall 读的是
                    // 管理员映像里生成的 .sldIM 配置，命令行不接受组件列表。
                    // 静默忽略会让用户以为选了组件其实没选，所以这里明确告警。
                    if !ctx.config.install.components_whitelist.is_empty() {
                        ctx.warn(format!(
                            "[install].components_whitelist 有 {} 项，但本次走的是 \
                             StartSWInstall 路径，**命令行不接受组件列表**，该配置不生效。\n\
                             要只装部分组件，请二选一：\n\
                             · 用安装管理器（sldIM.exe）生成带组件选择的 .sldIM 文件，\
                               再让安装器加载它；\n\
                             · 把 [install].force_msiexec 设为 true，改走 msiexec + ADDLOCAL 路径。",
                            ctx.config.install.components_whitelist.len()
                        ));
                    }

                    ctx.progress(5.0, "安装器已启动，等待安装完成");
                    return Ok(());
                }
                Err(error) => ctx.warn(format!(
                    "{} 启动失败: {error}，改用 msiexec",
                    start_sw.display()
                )),
            }
        }
        (Some(_), true) => {
            ctx.info("[install].force_msiexec = true，跳过 StartSWInstall，直接走 msiexec");
        }
        (None, _) => {
            ctx.warn(format!(
                "在 {} 下未找到 startswinstall.exe（已查找 sldim\\、根目录、sldIM\\），\
                 改用 msiexec 回退方案",
                setup_root.display()
            ));
        }
    }

    // 回退：直接调用 SOLIDWORKS.msi。
    //
    // 真实 DVD/ISO 布局里它在 `swwi\data\solidworks.msi`（已在
    // SolidWorks 2024 SP5 的 Premium.DVD 上核对）。其余候选是不同版本/映像的
    // 历史位置，一并作为兜底，顺序由熟悉到冷门。
    let msi = locate_solidworks_msi(setup_root);

    let Some(msi) = msi else {
        return Err(format!(
            "既没有 startswinstall.exe 也没有 solidworks.msi。\n\
             已查找：{}\\sldim\\startswinstall.exe、{}\\startswinstall.exe、\
             {}\\swwi\\data\\solidworks.msi。\n\
             请确认解压出的目录里确实包含这两种文件之一。",
            setup_root.display(),
            setup_root.display(),
            setup_root.display()
        ));
    };

    ctx.info(format!("使用安装包 {}", msi.display()));

    // ------------------------------------------------------------------
    // 主程序：官方文档《通过命令行从管理映像安装》给的模板
    //
    //   msiexec /i "...\SOLIDWORKS\SOLIDWORKS.Msi"
    //      INSTALLDIR="..."
    //      SOLIDWORKSSERIALNUMBER="xxxx ..."
    //      ENABLEPERFORMANCE=1 OFFICEOPTION=3
    //      ADDLOCAL=SolidWorks,SolidWorksToolbox
    //      TOOLBOXFOLDER="C:\SolidWorks Data\"
    //      /qb
    //
    // 注意属性名都是**官方文档里的原名**：
    //   · 序列号是 SOLIDWORKSSERIALNUMBER，**不是** SERIALNUMBER
    //   · Toolbox 目录是 TOOLBOXFOLDER
    //   · 性能数据上报是 ENABLEPERFORMANCE（1=发送，0=不发送）
    //   · OFFICEOPTION 是产品级别：0=Standard 1=Office 2=Professional 3=Premium
    // ------------------------------------------------------------------
    let mut args: Vec<String> = vec![
        "/i".to_string(),
        quoted_arg(&msi.display().to_string()),
        msi_property("INSTALLDIR", &install_dir.display().to_string()),
    ];

    if !ctx.config.install.components_whitelist.is_empty() {
        // ADDLOCAL 的 Feature 名大小写敏感，且必须含父级链。
        args.push(msi_property(
            "ADDLOCAL",
            &ctx.config.install.components_whitelist.join(","),
        ));
    }

    let serial = ctx.config.install.serial_number.trim();
    if !serial.is_empty() {
        args.push(msi_property("SOLIDWORKSSERIALNUMBER", serial));
    }

    // 只有真的装了 Toolbox 才传 TOOLBOXFOLDER —— 不装却传它是无意义的。
    let wants_toolbox = ctx
        .config
        .install
        .components_whitelist
        .iter()
        .any(|item| item.eq_ignore_ascii_case("SolidWorksToolbox"));
    if wants_toolbox {
        let folder = ctx.config.install.toolbox_folder.trim();
        if !folder.is_empty() {
            // 这一项就是踩坑的地方：默认值 `C:\SOLIDWORKS Data` 含空格，
            // 不加内层引号会让 msiexec 报 1639 并弹用法对话框。
            args.push(msi_property("TOOLBOXFOLDER", folder));
        }
    }

    // 不把性能数据发回厂商；与官方模板的 ENABLEPERFORMANCE=1 相反，
    // 这里取 0（不发送），是更保守的默认。
    args.push("ENABLEPERFORMANCE=0".to_string());
    args.push("/qb".to_string());
    args.push("/norestart".to_string());

    // 必须是 `/l*v`（详细日志）：安装失败时这是唯一能查明原因的线索。
    // 实测踩到的坑：没有日志时只能看到一个非零退出码，根本不知道哪一步错了。
    // 日志落在应用数据目录，`[workdir].cleanup_temp_after` 不会动它。
    let log_dir = ctx.app_data_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let msi_log = log_dir.join("msiexec-solidworks.log");
    let _ = std::fs::remove_file(&msi_log);

    // ⚠️ `/l*v` 与日志路径**必须作为两个独立的命令行参数**。
    //
    // 实机实测（本机 msiexec，只做参数解析）：
    //
    // | 写法 | 结果 |
    // |---|---|
    // | `"/l*v"` + `<路径>` 两个 argv | `1622`（参数已通过，仅日志文件创建失败） |
    // | `"/l*v <路径>"` 合成一个 argv | **卡住并弹出用法对话框**，永不返回 |
    //
    // 合成一个参数时 msiexec 无法识别 `/l*v` 开关，直接进交互模式 ——
    // 表现就是用户看到的「Windows Installer」用法对话框，安装完全不执行。
    args.push("/l*v".to_string());
    args.push(quoted_arg(&msi_log.display().to_string()));

    let mut command = process_utils::hidden_command(Path::new("msiexec"));
    command.args(&args);

    ctx.info(format!("[主程序] 执行 msiexec {}", args.join(" ")));
    ctx.info(format!("[主程序] 安装日志 {}", msi_log.display()));

    let main_pid = match process_utils::spawn_detached(command) {
        Ok(child) => {
            let pid = child.id();
            ctx.success(format!("主程序安装器已启动（pid {pid}）"));

            // 监控 msiexec 是否退出，并把日志尾部写进自己的日志。
            //
            // **为什么必须做**：实测这次 msiexec 7 分钟就失败了，
            // 但程序不知道，一路空等到超时。有了这个守护线程，
            // 至少能在日志里看到 MSI 最后写了什么。
            let reporter = ctx.reporter.clone();
            let cancel = Arc::clone(&ctx.cancel);
            let status = Arc::clone(&ctx.status);
            let log = msi_log.clone();
            thread::spawn(move || {
                while !cancel.load(Ordering::SeqCst)
                    && win32_utils::process_image_path(pid).is_some()
                {
                    thread::sleep(Duration::from_millis(500));
                }
                if cancel.load(Ordering::SeqCst) {
                    return;
                }
                for line in read_log_tail(&log, 12) {
                    reporter.log(LogLevel::Debug, format!("[msiexec] {line}"));
                }
                reporter.log(
                    LogLevel::Warn,
                    format!(
                        "[主程序] msiexec 已退出。若安装未成功，请查看完整日志：{}",
                        log.display()
                    ),
                );
                // 主程序结束 → 让轮询知道"安装器已不在跑"，
                // 从而能基于真实状态尽早判定成功或失败。
                status.installer_exited.store(true, Ordering::SeqCst);
            });
            pid
        }
        Err(error) => return Err(format!("msiexec 启动失败: {error}")),
    };

    // ------------------------------------------------------------------
    // 语言包：**独立的 MSI，必须单独安装**
    //
    // 官方文档：「SOLIDWORKS 法语安装组件必须单独安装：
    //          msiexec /i "...\SOLIDWORKS French\french.msi" /qb」
    // 并特别注明：「指定 SOLIDWORKS 语言组件安装命令时，请勿指定命令行参数。」
    // 所以这里**只传 /qb**，不传 INSTALLDIR / ADDLOCAL。
    // ------------------------------------------------------------------
    match locate_language_msi(setup_root, &ctx.config.install.language_pack) {
        Some(lang_msi) => {
            ctx.info(format!(
                "[语言包] {} → {}（主程序安装器退出后立即启动）",
                ctx.config.install.language_pack,
                lang_msi.display()
            ));

            // 不能阻塞编排线程等待主程序，所以交给一个守护线程去接续。
            // 同时置位 language_pending —— 轮询要等它清零才算安装完成。
            ctx.status.language_pending.store(true, Ordering::SeqCst);

            let reporter = ctx.reporter.clone();
            let cancel = Arc::clone(&ctx.cancel);
            let status = Arc::clone(&ctx.status);
            let path = lang_msi.clone();
            let label = ctx.config.install.language_pack.clone();
            thread::spawn(move || {
                // 等主程序这个具体 PID 退出，而不是固定等待一段时间或扫描全局
                // msiexec 进程。这样主程序完成后语言包立即接续，且不会争抢 MSI 锁。
                while !cancel.load(Ordering::SeqCst)
                    && win32_utils::process_image_path(main_pid).is_some()
                {
                    thread::sleep(Duration::from_millis(500));
                }
                if cancel.load(Ordering::SeqCst) {
                    reporter.log(LogLevel::Warn, "[语言包] 已取消，不再启动语言包安装");
                    status.language_pending.store(false, Ordering::SeqCst);
                    return;
                }

                reporter.log(
                    LogLevel::Info,
                    format!("[语言包] 主程序安装器已退出，立即启动 {label} 语言包"),
                );

                let mut command = process_utils::hidden_command(Path::new("msiexec"));
                // 路径一律加引号：语言包所在目录可能含空格
                // （管理映像布局是 `SOLIDWORKS French\`，用户也可能把介质放在带空格的路径下）。
                command.args([
                    "/i",
                    &quoted_arg(&path.to_string_lossy()),
                    "/qb",
                    "/norestart",
                ]);
                match process_utils::spawn_detached(command) {
                    Ok(child) => {
                        let pid = child.id();
                        reporter.log(
                            LogLevel::Success,
                            format!("[语言包] {label} 安装器已启动（pid {pid}）"),
                        );
                        // 等这个 msiexec 真正退出再清标记：
                        // 这样轮询看到"没有任何安装进程"时，语言包一定也已结束。
                        while !cancel.load(Ordering::SeqCst)
                            && win32_utils::process_image_path(pid).is_some()
                        {
                            thread::sleep(Duration::from_millis(500));
                        }
                        if cancel.load(Ordering::SeqCst) {
                            reporter.log(
                                LogLevel::Warn,
                                "[语言包] 已取消；语言包安装可能未完成，建议手动确认",
                            );
                        } else {
                            reporter.log(
                                LogLevel::Success,
                                format!("[语言包] {label} 安装进程已退出"),
                            );
                        }
                    }
                    Err(error) => reporter.log(
                        LogLevel::Error,
                        format!("[语言包] {label} 启动失败: {error}"),
                    ),
                }
                status.language_pending.store(false, Ordering::SeqCst);
            });
        }
        None => {
            let name = ctx.config.install.language_pack.trim();
            if name.is_empty() {
                ctx.info("[语言包] language_pack 为空，跳过语言包安装");
            } else {
                ctx.warn(format!(
                    "[语言包] 未找到 {name}.msi。已查找 {}\\swwi\\lang\\{name}\\{name}.msi \
                     与 {}\\64bit\\SOLIDWORKS {name}\\{name}.msi。\n\
                     可用的目录名就是介质 swwi\\lang\\ 下的子目录名\
                     （chinese / chinese-simplified / french / german / …）。",
                    setup_root.display(),
                    setup_root.display()
                ));
            }
        }
    }

    ctx.progress(5.0, "安装器已启动，等待安装完成");
    Ok(())
}

/// 定位语言包 MSI。
///
/// 两种真实布局：
///
/// | 介质类型 | 路径 |
/// |---|---|
/// | **DVD / ISO**（本项目实际用的） | `swwi\lang\<名称>\<名称>.msi` |
/// | **管理映像**（官方文档的例子） | `64bit\SOLIDWORKS <名称>\<名称>.msi` |
///
/// 名称即 `[install].language_pack`，例如 `chinese-simplified`。
fn locate_language_msi(setup_root: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    // `french` → 管理映像里目录是 `SOLIDWORKS French`
    let title_case = {
        let mut chars = name.chars();
        match chars.next() {
            Some(first) => format!("SOLIDWORKS {}{}", first.to_uppercase(), chars.as_str()),
            None => String::new(),
        }
    };

    let candidates: Vec<PathBuf> = vec![
        // DVD / ISO
        setup_root
            .join("swwi")
            .join("lang")
            .join(name)
            .join(format!("{name}.msi")),
        // 管理映像（官方文档布局）
        setup_root
            .join("64bit")
            .join(&title_case)
            .join(format!("{name}.msi")),
        // 再宽松一点：语言目录直接放在根下
        setup_root.join(name).join(format!("{name}.msi")),
        setup_root.join(format!("{name}.msi")),
    ];

    let found = candidates.into_iter().find(|path| path.is_file());
    if found.is_none() {
        // 兜底：在 swwi\lang 下做一次不区分大小写的目录名匹配
        let lang_root = setup_root.join("swwi").join("lang");
        if let Ok(entries) = std::fs::read_dir(&lang_root) {
            for entry in entries.flatten() {
                let dir = entry.path();
                let matches = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case(name))
                    .unwrap_or(false);
                if matches {
                    let candidate = dir.join(format!("{name}.msi"));
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                    // 目录里只有一个 .msi 时就用它
                    if let Ok(files) = std::fs::read_dir(&dir) {
                        let msis: Vec<PathBuf> = files
                            .flatten()
                            .map(|f| f.path())
                            .filter(|p| {
                                p.extension()
                                    .map(|e| e.to_string_lossy().eq_ignore_ascii_case("msi"))
                                    .unwrap_or(false)
                            })
                            .collect();
                        if msis.len() == 1 {
                            return msis.into_iter().next();
                        }
                    }
                }
            }
        }
    }
    found
}

/// 读取日志文件的最后 `lines` 行（用于把 msiexec 的关键输出带进我们的日志）。
fn read_log_tail(path: &Path, lines: usize) -> Vec<String> {
    let Ok(text) = std::fs::read(path) else {
        return Vec::new();
    };
    // MSI 日志可能是 UTF-16（msiexec 默认写 UTF-16LE 带 BOM）
    let decoded = if text.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = text[2..]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        crate::decode_console_bytes(&text)
    };

    // 只保留有信息量的行，跳过 MSI 的进度噪声
    decoded
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error")
                || lower.contains("return value")
                || lower.contains("failed")
                || lower.contains("note: 1:")
                || lower.contains("mainengine")
                || lower.contains("installation success")
                || lower.contains("installation failed")
        })
        .rev()
        .take(lines)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// 把 MSI 属性拼成命令行参数（值含空格时加引号，仅作形式上的稳妥处理）。
///
/// # 实测结论：加引号**并不能**让含空格的值被接受
///
/// 在本机用真实 `msiexec`（`/qn`，只做参数解析）逐项实测：
///
/// | 属性值写法 | 结果 |
/// |---|---|
/// | 无空格（如 `C:\SOLIDWORKS_Data`） | ✅ 被接受，日志生成 |
/// | 含空格、不加引号（`C:\SOLIDWORKS Data`） | ❌ 卡住，弹出用法对话框 |
/// | 含空格、**加引号**（`"C:\SOLIDWORKS Data"`） | ❌ **一样卡住** |
///
/// 也就是说 **msiexec 不接受含空格的属性值**，加引号救不回来。
/// 因此本项目的做法是：**默认值一律不含空格**
/// （`C:\SOLIDWORKS_Data`，用下划线）。
///
/// 这里仍然为含空格的值加引号，只是为了避免生成明显畸形的命令行，
/// **不要指望它能工作** —— 用户若确实需要带空格的路径，请提醒他改用无空格目录。
fn msi_property(name: &str, value: &str) -> String {
    let value = value.trim();
    if value.contains([' ', '\t', '"']) {
        format!("{name}=\"{}\"", value.replace('"', "\\\""))
    } else {
        format!("{name}={value}")
    }
}

/// 单个路径参数（如 `/l*v <日志>`）也按同样规则加引号。
fn quoted_arg(value: &str) -> String {
    let value = value.trim();
    if value.contains([' ', '\t', '"']) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

/// 定位 `startswinstall.exe`。
///
/// **真实布局**（已在 SolidWorks 2024 SP5 Premium.DVD 上核对）：
///
/// ```text
/// <介质根>\
///   setup.exe                ← 引导程序，点两下用；命令行模式在下面那个
///   sldim\
///     startswinstall.exe     ← 就是这个
///     sldIM.exe              ← 图形化安装管理器
///   swwi\data\solidworks.msi
/// ```
///
/// 早期版本的映像把它放在根目录，所以两者都查。
/// Windows 路径不区分大小写，因此 `sldim` / `sldIM` 都能命中。
fn locate_start_swinstall(setup_root: &Path) -> Option<PathBuf> {
    const CANDIDATES: &[&[&str]] = &[
        &["sldim", "startswinstall.exe"],
        &["startswinstall.exe"],
        &["sldIM", "StartSWInstall.exe"],
        &["setup", "startswinstall.exe"],
    ];

    CANDIDATES
        .iter()
        .map(|parts| {
            parts
                .iter()
                .fold(setup_root.to_path_buf(), |acc, p| acc.join(p))
        })
        .find(|path| path.is_file())
}

/// 定位 `solidworks.msi`。
///
/// **真实布局**：`swwi\data\solidworks.msi`。
fn locate_solidworks_msi(setup_root: &Path) -> Option<PathBuf> {
    const CANDIDATES: &[&[&str]] = &[
        &["swwi", "data", "solidworks.msi"],
        &["64bit", "SOLIDWORKS", "SOLIDWORKS.msi"],
        &["SOLIDWORKS", "SOLIDWORKS.msi"],
        &["solidworks.msi"],
    ];

    CANDIDATES
        .iter()
        .map(|parts| {
            parts
                .iter()
                .fold(setup_root.to_path_buf(), |acc, p| acc.join(p))
        })
        .find(|path| path.is_file())
}

/// 启动弹窗处理守护线程。
pub fn spawn_popup_daemon(
    ctx: &InstallContext,
    stop: Arc<AtomicBool>,
) -> Option<thread::JoinHandle<()>> {
    let interval =
        Duration::from_millis(ctx.config.install.popup.scan_interval_ms.clamp(50, 60_000));
    let confirm = ctx.config.install.popup.auto_click_confirm;
    let yes = ctx.config.install.popup.auto_click_yes;
    let no = ctx.config.install.popup.auto_click_no;
    let popup_timeout = ctx.config.popup_timeout();
    let reporter = ctx.reporter.clone();
    let status = Arc::clone(&ctx.status);

    ctx.info(format!(
        "弹窗守护已启动（扫描间隔 {} 毫秒，确定={confirm} 是={yes} 否={no}）",
        interval.as_millis()
    ));

    thread::Builder::new()
        .name("sw-popup-daemon".to_string())
        .spawn(move || {
            let mut handled_titles: Vec<(String, Instant)> = Vec::new();
            let mut last_heartbeat = Instant::now();
            let mut scanned: u64 = 0;

            while !stop.load(Ordering::SeqCst) {
                if let Some(handling) = win32_utils::scan_popup_once(
                    win32_utils::DEFAULT_POPUP_RULES,
                    confirm,
                    yes,
                    no,
                    win32_utils::DEFAULT_SERVER_VALUE,
                ) {
                    // 同一个标题在超时窗口内只处理一次，避免与用户手动操作打架。
                    let already = handled_titles.iter().any(|(title, at)| {
                        title == &handling.title && at.elapsed() < popup_timeout
                    });
                    if !already {
                        handled_titles.push((handling.title.clone(), Instant::now()));
                        handled_titles.retain(|(_, at)| at.elapsed() < popup_timeout);
                        status.popups_handled.fetch_add(1, Ordering::SeqCst);

                        reporter.log(
                            LogLevel::Success,
                            format!("弹窗「{}」→ {}", handling.title, handling.action),
                        );
                        reporter.popup(&handling.title, &handling.action);
                    }
                }

                scanned += 1;

                // 每分钟一次心跳：让日志能证明守护线程仍在扫描，
                // 而不是"没有弹窗"与"线程已死"无法区分。
                if last_heartbeat.elapsed() >= Duration::from_secs(60) {
                    reporter.log(
                        LogLevel::Debug,
                        format!(
                            "弹窗守护心跳 · 已扫描 {scanned} 轮 · 累计处理 {} 个弹窗",
                            status.popups_handled.load(Ordering::SeqCst)
                        ),
                    );
                    last_heartbeat = Instant::now();
                }

                thread::sleep(interval);
            }

            reporter.log(
                LogLevel::Debug,
                format!("弹窗守护已退出（共扫描 {scanned} 轮）"),
            );
        })
        .ok()
}
