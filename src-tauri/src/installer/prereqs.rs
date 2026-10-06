//! Windows 前置组件检测与安装。
//!
//! # 为什么需要这一步
//!
//! SolidWorks 2024 官方《安装与管理》手册（第 31~34 页「准备客户端以便通过命令行
//! 从管理映像进行安装」）明确说明：
//!
//! > 创建管理映像后，在通知客户端之前，您**必须安装**无法通过使用命令行或
//! > Microsoft Active Directory 创建的管理映像来安装的 Microsoft Windows 组件。
//! >
//! > **所有 SOLIDWORKS 产品（并非只是核心 SOLIDWORKS 产品）均要求有
//! > Visual C++ 可重新分发软件包和 .NET Framework 4.8。**
//!
//! 也就是说：**`msiexec /i solidworks.msi` 不会自动装这些前置组件。**
//! 平时双击 `setup.exe` 时是安装管理器替我们装了，一旦改走纯命令行路线，
//! 这一环就必须自己补上。
//!
//! 缺少它们的后果不是"装不上"，而是**装上了但运行不起来** ——
//! `solidworks.msi` 本身没有 `LaunchCondition` 表（已实测确认），
//! 所以安装会"成功"，但主程序启动时缺 DLL 才暴露问题。
//!
//! # 权威来源
//!
//! 该装哪些组件，以**介质自带的安装管理器清单** `sldim\sldim.xml` 为准
//! （实测该文件中 `PreReqs\` 下出现的目录即为本套介质的真实前置集合）：
//!
//! | 组件 | 介质路径 | 强制？ |
//! |---|---|---|
//! | Visual C++ 2015-2022 可再发行（x64） | `PreReqs\VCRedist17\VC_redist.x64.exe` | 是 |
//! | Visual C++ 2015-2022 可再发行（x86） | `PreReqs\VCRedist17\VC_redist.x86.exe` | 是 |
//! | .NET Framework 4.8 | `PreReqs\dotNetFx\ndp48-x86-x64-allos-enu.exe` | 是 |
//! | Microsoft Edge WebView2 Runtime | `sldim\MicrosoftEdgeWebView2RuntimeInstallerX64.exe` | 是 |
//! | Visual Basic for Applications 7.1 | `PreReqs\VBA\vba71.msi` + 语言包 | 可选 |
//!
//! > **关于 Visual C++ 2010**：手册第 32 页的通用前置表里提到需要
//! > 「Visual C++ 2010 和 2022」，但**这套介质里并没有 2010 的安装包**
//! > （`PreReqs\` 下只有 `VCRedist17`，已实测）。因此本模块不检查也不安装它 ——
//! > 以介质实际提供的内容为准，不去要求一个介质里根本不存在的东西。

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::InstallConfig;
use crate::events::LogLevel;
use crate::process_utils;
use crate::win32_utils;

/// 一个前置组件的检测与安装定义。
struct Prereq {
    /// 显示名（进日志与界面）
    label: &'static str,
    /// 介质里的相对路径（相对安装介质根）
    relative: &'static [&'static str],
    /// 安装命令行参数（`None` 表示自动推导）
    args: &'static [&'static str],
    /// 是否强制（强制组件缺失时会把整步标为"有未满足项"）
    required: bool,
    /// 检测函数：返回 `true` 表示**已安装**
    installed: fn() -> bool,
}

/// 需要检查的前置组件。
///
/// 顺序即安装顺序：先 VC++ 运行时与 .NET，再 WebView2，最后 VBA。
const PREREQS: &[Prereq] = &[
    Prereq {
        label: "Visual C++ 2015-2022 可再发行组件 (x64)",
        relative: &["PreReqs", "VCRedist17", "VC_redist.x64.exe"],
        // /install /quiet /norestart 是微软官方 vc_redist 的静默参数
        args: &["/install", "/quiet", "/norestart"],
        required: true,
        installed: vcredist_x64_installed,
    },
    Prereq {
        label: "Visual C++ 2015-2022 可再发行组件 (x86)",
        relative: &["PreReqs", "VCRedist17", "VC_redist.x86.exe"],
        args: &["/install", "/quiet", "/norestart"],
        required: true,
        installed: vcredist_x86_installed,
    },
    Prereq {
        label: ".NET Framework 4.8",
        relative: &["PreReqs", "dotNetFx", "ndp48-x86-x64-allos-enu.exe"],
        // /q 静默、/norestart 不重启：官方 ndp48 的静默参数
        args: &["/q", "/norestart"],
        required: true,
        installed: net48_installed,
    },
    Prereq {
        label: "Microsoft Edge WebView2 Runtime",
        relative: &[
            "sldim",
            "MicrosoftEdgeWebView2RuntimeInstallerX64.exe",
        ],
        // WebView2 官方静默参数
        args: &["/silent", "/install"],
        required: true,
        installed: webview2_installed,
    },
    Prereq {
        label: "Visual Basic for Applications 7.1",
        relative: &["PreReqs", "VBA", "vba71.msi"],
        // MSI 走 msiexec
        args: &["/i"],
        required: false,
        installed: vba71_installed,
    },
];

/// 单个组件的处置结果。
#[derive(Debug, Clone)]
pub struct PrereqOutcome {
    pub label: String,
    /// 检测时是否已装
    pub already_installed: bool,
    /// 是否执行了安装
    pub installed_now: bool,
    /// 安装是否成功（已在装或装成功都为 true）
    pub ok: bool,
    /// 说明（错误原因 / 介质缺失 / 使用的安装文件）
    pub detail: String,
}

/// 整步汇总。
#[derive(Debug, Clone, Default)]
pub struct PrereqReport {
    pub outcomes: Vec<PrereqOutcome>,
    /// 缺失且未能安装的**强制**组件
    pub missing_required: Vec<String>,
}

impl PrereqReport {
    pub fn summary(&self) -> String {
        let total = self.outcomes.len();
        let already = self
            .outcomes
            .iter()
            .filter(|o| o.already_installed)
            .count();
        let installed = self.outcomes.iter().filter(|o| o.installed_now).count();
        let mut text = format!(
            "前置组件：共 {total} 项，已装 {already} 项，本次安装 {installed} 项"
        );
        if !self.missing_required.is_empty() {
            text.push_str(&format!(
                "；**未满足的强制项**：{}",
                self.missing_required.join("、")
            ));
        }
        text
    }
}

/// 检测并可选择安装前置组件。
///
/// `media_root` 是安装介质根目录（挂载后的盘符根，或解压出的目录）。
pub fn handle_prerequisites(
    config: &InstallConfig,
    media_root: &Path,
    emit: &dyn Fn(LogLevel, String),
) -> PrereqReport {
    let mut report = PrereqReport::default();

    // 第一步永远是检测：只有真的缺了才动手。
    emit(
        LogLevel::Info,
        format!("检查 Windows 前置组件（介质根 {}）", media_root.display()),
    );

    for prereq in PREREQS {
        let already = (prereq.installed)();

        if already {
            emit(
                LogLevel::Info,
                format!("  [已装] {}", prereq.label),
            );
            report.outcomes.push(PrereqOutcome {
                label: prereq.label.to_string(),
                already_installed: true,
                installed_now: false,
                ok: true,
                detail: "检测到已安装，跳过".to_string(),
            });
            continue;
        }

        if !config.install_prerequisites {
            emit(
                LogLevel::Warn,
                format!(
                    "  [缺失] {}（install_prerequisites = false，不自动安装）",
                    prereq.label
                ),
            );
            report.outcomes.push(PrereqOutcome {
                label: prereq.label.to_string(),
                already_installed: false,
                installed_now: false,
                ok: !prereq.required,
                detail: "缺失，但按配置未自动安装".to_string(),
            });
            if prereq.required {
                report.missing_required.push(prereq.label.to_string());
            }
            continue;
        }

        // 定位介质里的安装文件。
        let Some(source) = locate(media_root, prereq.relative) else {
            let detail = format!(
                "介质中未找到 {}（已查找 {}）",
                prereq.relative.join("\\"),
                media_root.display()
            );
            emit(LogLevel::Warn, format!("  [缺失] {}：{detail}", prereq.label));
            report.outcomes.push(PrereqOutcome {
                label: prereq.label.to_string(),
                already_installed: false,
                installed_now: false,
                ok: !prereq.required,
                detail,
            });
            if prereq.required {
                report.missing_required.push(prereq.label.to_string());
            }
            continue;
        };

        emit(
            LogLevel::Warn,
            format!(
                "  [缺失] {} → 正在安装 {}",
                prereq.label,
                source.display()
            ),
        );

        let result = install_one(config, prereq, &source, media_root, emit);

        // 装完再检测一次：这是唯一可靠的验收方式。
        let now_ok = (prereq.installed)();
        let ok = now_ok || result;

        report.outcomes.push(PrereqOutcome {
            label: prereq.label.to_string(),
            already_installed: false,
            installed_now: ok,
            ok,
            detail: if now_ok {
                format!("已从 {} 安装完成", source.display())
            } else {
                format!(
                    "安装命令已执行，但复检仍未检测到；\
                     可能需要重启或手动安装（{}）",
                    source.display()
                )
            },
        });

        if !ok && prereq.required {
            report.missing_required.push(prereq.label.to_string());
        }
    }

    let summary = report.summary();
    emit(
        if report.missing_required.is_empty() {
            LogLevel::Success
        } else {
            LogLevel::Warn
        },
        summary.clone(),
    );

    if !report.missing_required.is_empty() {
        emit(
            LogLevel::Warn,
            "有强制前置组件未满足。SOLIDWORKS 仍可能装上，但**启动时会因缺少依赖而失败**。\
             建议先手动装上这些组件（都在介质 PreReqs\\ 目录里），或改用 setup.exe 让\
             安装管理器处理。"
                .to_string(),
        );
    }

    report
}

/// 在介质里定位一个文件（大小写不敏感地逐段查找）。
fn locate(root: &Path, relative: &[&str]) -> Option<PathBuf> {
    let direct = relative
        .iter()
        .fold(root.to_path_buf(), |acc, part| acc.join(part));
    if direct.is_file() {
        return Some(direct);
    }

    // 大小写不敏感：逐级在目录项里找同名（忽略大小写）的条目。
    let mut current = root.to_path_buf();
    for part in relative {
        let entries = std::fs::read_dir(&current).ok()?;
        let mut found = None;
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(part)
            {
                found = Some(entry.path());
                break;
            }
        }
        current = found?;
    }
    current.is_file().then_some(current)
}

/// 执行一个组件的安装命令。
fn install_one(
    config: &InstallConfig,
    prereq: &Prereq,
    source: &Path,
    media_root: &Path,
    emit: &dyn Fn(LogLevel, String),
) -> bool {
    // 超时复用 `[install].prerequisite_timeout_minutes`
    let timeout = Duration::from_secs_f64(
        config.prerequisite_timeout_minutes.max(0.1) * 60.0,
    );

    let is_msi = source
        .extension()
        .map(|e| e.to_string_lossy().eq_ignore_ascii_case("msi"))
        .unwrap_or(false);

    let mut command = if is_msi {
        let mut cmd = process_utils::hidden_command(Path::new("msiexec"));
        // `args` 里第一个是 `/i`，随后补文件路径与固定静默参数
        cmd.args(["/i", &source.to_string_lossy()]);
        cmd.args(["/qb", "/norestart"]);
        cmd
    } else {
        let mut cmd = process_utils::hidden_command(source);
        for arg in prereq.args {
            // MSI 的参数在 is_msi 分支里已处理，这里只处理 exe
            if *arg != "/i" {
                cmd.arg(arg);
            }
        }
        cmd
    };
    // 安装程序常要在自己的目录里找同级的 cab / mst
    command.current_dir(
        source
            .parent()
            .unwrap_or(media_root),
    );

    match process_utils::run_command(command, timeout, None) {
        Ok(outcome) => {
            let code = format!("{:?}", outcome.code);
            if outcome.is_success() {
                emit(
                    LogLevel::Success,
                    format!("    {} 安装命令返回成功（code={code}）", prereq.label),
                );
                true
            } else {
                // 3010 = 成功但需重启；对安装而言这是可接受的结果
                let needs_reboot = code.contains("3010");
                if needs_reboot {
                    emit(
                        LogLevel::Warn,
                        format!(
                            "    {} 安装成功但**需要重启**才能生效（code=3010）",
                            prereq.label
                        ),
                    );
                    true
                } else {
                    emit(
                        LogLevel::Warn,
                        format!(
                            "    {} 安装返回非零 code={code}；输出尾部：{}",
                            prereq.label,
                            outcome.tail(3)
                        ),
                    );
                    false
                }
            }
        }
        Err(error) => {
            emit(
                LogLevel::Warn,
                format!("    {} 启动安装程序失败：{error}", prereq.label),
            );
            false
        }
    }
}

// ---------------------------------------------------------------------------
// 检测函数
//
// 全部读注册表 —— 只读、无副作用，可以放心在每个组件上调用。
// ---------------------------------------------------------------------------

/// 读 32/64 位 Uninstall 表里是否存在匹配 `DisplayName` 的项。
fn uninstall_entry_matches(predicate: &dyn Fn(&str) -> bool) -> bool {
    const BASES: [&str; 2] = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];
    for base in BASES {
        for (sub, _) in win32_utils::registry_subkeys_with_value(
            win32_utils::HKEY_LOCAL_MACHINE,
            base,
            "DisplayName",
        ) {
            let path = format!("{base}\\{sub}");
            if let Some(name) = win32_utils::read_registry_string(
                win32_utils::HKEY_LOCAL_MACHINE,
                &path,
                "DisplayName",
            ) {
                if predicate(&name) {
                    return true;
                }
            }
        }
    }
    false
}

/// Visual C++ 2015-2022 x64 是否已装。
///
/// vc_redist 会写 `...\Uninstall` 项，名字形如
/// 「Microsoft Visual C++ 2015-2022 Redistributable (x64)」。
fn vcredist_x64_installed() -> bool {
    uninstall_entry_matches(&|name| {
        let lower = name.to_ascii_lowercase();
        lower.contains("visual c++")
            && (lower.contains("x64") || lower.contains("64-bit"))
            && (lower.contains("2015-2022")
                || lower.contains("2017")
                || lower.contains("2019")
                || lower.contains("2022")
                || lower.contains("14."))
    })
}

/// Visual C++ 2015-2022 x86 是否已装。
fn vcredist_x86_installed() -> bool {
    uninstall_entry_matches(&|name| {
        let lower = name.to_ascii_lowercase();
        lower.contains("visual c++")
            && (lower.contains("x86") || lower.contains("32-bit"))
            && (lower.contains("2015-2022")
                || lower.contains("2017")
                || lower.contains("2019")
                || lower.contains("2022")
                || lower.contains("14."))
    })
}

/// .NET Framework 4.8 是否已装。
///
/// 官方判定方式：`HKLM\SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full`
/// 的 `Release` DWORD ≥ 528040 即为 4.8 或更高。
fn net48_installed() -> bool {
    let raw = win32_utils::read_registry_dword(
        win32_utils::HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full",
        "Release",
    );
    raw.map(|value| value >= 528040).unwrap_or(false)
}

/// Microsoft Edge WebView2 Runtime 是否已装。
///
/// 官方检测位置（按优先级）：
/// 1. `HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`
/// 2. `HKLM\SOFTWARE\Microsoft\EdgeUpdate\Clients\{...}`（同为系统级）
/// 3. `HKCU\Software\Microsoft\EdgeUpdate\Clients\{...}`（仅当前用户）
fn webview2_installed() -> bool {
    const GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let candidates: [(isize, &str); 3] = [
        (
            win32_utils::HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients",
        ),
        (
            win32_utils::HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\EdgeUpdate\Clients",
        ),
        (
            win32_utils::HKEY_CURRENT_USER,
            r"Software\Microsoft\EdgeUpdate\Clients",
        ),
    ];
    for (root, base) in candidates {
        let key = format!("{base}\\{GUID}");
        // `pv` 存在即表示已安装（值是版本号）
        if win32_utils::read_registry_string(root, &key, "pv").is_some() {
            return true;
        }
    }
    false
}

/// Visual Basic for Applications 7.1 是否已装。
fn vba71_installed() -> bool {
    uninstall_entry_matches(&|name| {
        let lower = name.to_ascii_lowercase();
        lower.contains("visual basic for applications") && lower.contains("7.1")
    })
}
