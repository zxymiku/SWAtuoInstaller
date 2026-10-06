//! Windows 原生能力封装。
//!
//! 窗口自动化、网络适配器管理、进程枚举、原生文件选择全部在这里直接用
//! `windows` crate 调用 Win32 API 完成——不跨进程、不依赖脚本宿主，
//! 与 Tauri 应用同进程运行。
//!
//! 非 Windows 目标下提供桩实现，使 `cargo check` 在其他平台也能通过。

#![allow(unsafe_op_in_unsafe_fn)]

use std::path::PathBuf;

/// 网络适配器（安装期间禁用 / 收尾恢复）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
    /// `netsh interface set interface "NAME"` 使用的连接名（FriendlyName）。
    pub name: String,
    /// 驱动描述，用于识别虚拟网卡。
    pub description: String,
    /// `IF_TYPE_*` 值。
    pub if_type: u32,
    /// 是否处于 Up 状态。
    pub up: bool,
}

/// 弹窗动作种类（不含数据，便于放进 `const` 表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupActionKind {
    /// 点击「确定」/OK。
    Confirm,
    /// 点击「是」/Yes。
    Yes,
    /// 点击「否」/No。
    No,
    /// 确认端口输入框内容后点击「确定」。
    EnsureServerField,
}

/// 弹窗识别规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupRule {
    pub id: &'static str,
    pub keywords: &'static [&'static str],
    pub action: PopupActionKind,
}

/// 一次弹窗处理的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupHandling {
    /// 命中的窗口标题。
    pub title: String,
    /// 实际执行的动作描述（推送给前端与日志）。
    pub action: String,
}

/// 「端口@服务器」输入框的期望值。
pub const DEFAULT_SERVER_VALUE: &str = "25734@localhost";

/// `HKEY_LOCAL_MACHINE` 的别名，供注册表辅助函数使用。
pub const HKEY_LOCAL_MACHINE: isize = -2147483646; // 0x8000_0002

/// `HKEY_CURRENT_USER` 的别名。
pub const HKEY_CURRENT_USER: isize = -2147483647; // 0x8000_0001

/// 默认弹窗规则表——与 `docs/INSTALL_FLOW.md` 的规则表一一对应。
pub const DEFAULT_POPUP_RULES: &[PopupRule] = &[
    PopupRule {
        id: "restart-pending",
        keywords: &["重启", "重新启动", "restart"],
        action: PopupActionKind::Confirm,
    },
    PopupRule {
        id: "server-unverifiable",
        keywords: &["不能核实此服务器存在", "无法核实此服务器存在", "cannot verify"],
        action: PopupActionKind::Yes,
    },
    PopupRule {
        id: "port-at-server",
        keywords: &["端口@服务器", "端口 @ 服务器", "port@server"],
        action: PopupActionKind::EnsureServerField,
    },
];

// ===========================================================================
// Windows 实现
// ===========================================================================

#[cfg(windows)]
mod imp {
    use super::*;

    use windows::core::{Interface, BOOL, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HLOCAL, HANDLE, HWND, LPARAM, WPARAM};
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH, GAA_FLAG_SKIP_ANYCAST,
        GAA_FLAG_SKIP_MULTICAST,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::Networking::WinSock::AF_UNSPEC;
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::SystemInformation::{
        ComputerNamePhysicalDnsHostname, GetComputerNameExW,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, TerminateProcess, PROCESS_TERMINATE,
    };
    use windows::Win32::System::WindowsProgramming::GetUserNameW;
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        CommandLineToArgvW, FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog,
        IFileSaveDialog, IShellItem, ShellExecuteW, SIGDN_FILESYSPATH, FOS_PICKFOLDERS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetClassNameW, GetDlgItem, GetWindowTextLengthW,
        GetWindowTextW, IsWindowVisible, SendMessageTimeoutW, SendMessageW, SetWindowTextW,
        SW_SHOWNORMAL, SMTO_ABORTIFHUNG,
    };

    const BM_CLICK: u32 = 0x00F5;
    /// `IDOK`——标准对话框的确定按钮。
    const IDOK: i32 = 1;
    /// `ERROR_CANCELLED`（HRESULT 形式），用户关闭文件对话框时返回。
    const HRESULT_CANCELLED: u32 = 0x8007_04C7;
    /// `ERROR_BUFFER_OVERFLOW`——`GetAdaptersAddresses` 需要更大的缓冲区。
    const ERROR_BUFFER_OVERFLOW: u32 = 111;

    // -- 管理员权限 --------------------------------------------------------

    /// 当前进程是否以提升权限（管理员）运行。
    pub fn is_elevated() -> bool {
        unsafe {
            let mut token = HANDLE::default();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
                return false;
            }
            let mut elevation = TOKEN_ELEVATION::default();
            let mut returned = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned,
            )
            .is_ok();
            let _ = CloseHandle(token);
            ok && elevation.TokenIsElevated != 0
        }
    }

    /// 通过 `ShellExecuteW` 的 `runas` 动词以管理员身份重新启动自身。
    pub fn relaunch_elevated(args: &str) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("无法定位自身可执行文件: {e}"))?;
        let file = to_wide(&exe.to_string_lossy());
        let params = to_wide(args);
        let verb = to_wide("runas");

        unsafe {
            let result = ShellExecuteW(
                None,
                PCWSTR(verb.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR(params.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
            // ShellExecuteW 返回值 <= 32 表示失败。
            if result.0 as usize <= 32 {
                return Err(format!(
                    "提权请求被拒绝或失败（ShellExecuteW 返回 {}）",
                    result.0 as usize
                ));
            }
        }
        Ok(())
    }

    // -- 命令行解析 --------------------------------------------------------

    /// 用 `CommandLineToArgvW` 把命令行还原成参数列表（保留引号语义）。
    pub fn split_command_line(command_line: &str) -> Vec<String> {
        if command_line.trim().is_empty() {
            return Vec::new();
        }
        let wide = to_wide(command_line);
        unsafe {
            let mut count = 0i32;
            let argv = CommandLineToArgvW(PCWSTR(wide.as_ptr()), &mut count);
            if argv.is_null() || count <= 0 {
                return Vec::new();
            }
            let slice = std::slice::from_raw_parts(argv, count as usize);
            let mut out = Vec::with_capacity(count as usize);
            for ptr in slice {
                if let Ok(text) = ptr.to_string() {
                    out.push(text);
                }
            }
            // CommandLineToArgvW 使用 LocalAlloc，必须用 LocalFree 释放。
            let _ = windows::Win32::Foundation::LocalFree(Some(HLOCAL(argv as *mut _)));
            out
        }
    }

    // -- 网络适配器 --------------------------------------------------------

    /// 枚举本机适配器。返回 `(适配器列表, 错误信息)`。
    pub fn list_adapters() -> (Vec<AdapterInfo>, Option<String>) {
        unsafe {
            let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
            let mut size: u32 = 16 * 1024;
            let mut buffer: Vec<u8> = vec![0; size as usize];

            let mut result = GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                flags,
                None,
                Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            );

            if result == ERROR_BUFFER_OVERFLOW {
                buffer = vec![0; size as usize];
                result = GetAdaptersAddresses(
                    AF_UNSPEC.0 as u32,
                    flags,
                    None,
                    Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                    &mut size,
                );
            }

            if result != 0 {
                return (
                    Vec::new(),
                    Some(format!("GetAdaptersAddresses 失败，错误码 {result}")),
                );
            }

            let mut adapters = Vec::new();
            let mut current = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
            while !current.is_null() {
                let node = &*current;
                let name = if node.FriendlyName.is_null() {
                    String::new()
                } else {
                    node.FriendlyName.to_string().unwrap_or_default()
                };
                let description = if node.Description.is_null() {
                    String::new()
                } else {
                    node.Description.to_string().unwrap_or_default()
                };

                if !name.is_empty() {
                    adapters.push(AdapterInfo {
                        name,
                        description,
                        if_type: node.IfType,
                        up: node.OperStatus == IfOperStatusUp,
                    });
                }
                current = node.Next;
            }

            (adapters, None)
        }
    }

    /// 是否属于「虚拟 / 不应禁用」的适配器，跳过以免破坏宿主网络栈。
    pub fn is_virtual_adapter(adapter: &AdapterInfo) -> bool {
        // IF_TYPE_SOFTWARE_LOOPBACK = 24
        if adapter.if_type == 24 {
            return true;
        }
        const VIRTUAL_KEYWORDS: &[&str] = &[
            "virtual",
            "vmware",
            "hyper-v",
            "vethernet",
            "loopback",
            "tap-",
            "wintun",
            "wireguard",
            "openvpn",
            "zerotier",
            "tailscale",
            "npcap",
            "bluetooth",
            "wi-fi direct",
            "docker",
            "wsl",
        ];
        let haystack = format!("{} {}", adapter.name, adapter.description).to_ascii_lowercase();
        VIRTUAL_KEYWORDS.iter().any(|kw| haystack.contains(kw))
    }

    // -- 进程枚举 ----------------------------------------------------------

    /// 枚举所有进程的 `(pid, 可执行文件名)`，名称匹配交给调用方的正则处理。
    pub fn list_processes() -> Vec<(u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
                Ok(handle) => handle,
                Err(_) => return out,
            };

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let end = entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    out.push((
                        entry.th32ProcessID,
                        String::from_utf16_lossy(&entry.szExeFile[..end]),
                    ));
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        out
    }

    /// 打开进程句柄并强制终止。
    pub fn force_kill_pid(pid: u32) -> bool {
        unsafe {
            match OpenProcess(PROCESS_TERMINATE, false, pid) {
                Ok(handle) => {
                    let ok = TerminateProcess(handle, 1).is_ok();
                    let _ = CloseHandle(handle);
                    ok
                }
                Err(_) => false,
            }
        }
    }

    // -- 窗口枚举与弹窗处理 ------------------------------------------------

    fn window_title(hwnd: HWND) -> String {
        unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return String::new();
            }
            let mut buffer = vec![0u16; (len + 1) as usize];
            let copied = GetWindowTextW(hwnd, &mut buffer);
            String::from_utf16_lossy(&buffer[..copied as usize])
        }
    }

    fn window_class(hwnd: HWND) -> String {
        unsafe {
            let mut buffer = [0u16; 256];
            let copied = GetClassNameW(hwnd, &mut buffer);
            String::from_utf16_lossy(&buffer[..copied as usize])
        }
    }

    struct WindowSearch {
        rules: &'static [PopupRule],
        matches: Vec<(HWND, String, PopupActionKind)>,
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut WindowSearch);
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        let title = window_title(hwnd);
        if title.is_empty() {
            return BOOL(1);
        }
        let lowered = title.to_ascii_lowercase();
        for rule in search.rules {
            let hit = rule.keywords.iter().any(|kw| {
                title.contains(kw) || lowered.contains(&kw.to_ascii_lowercase())
            });
            if hit {
                search.matches.push((hwnd, title.clone(), rule.action));
                break;
            }
        }
        BOOL(1)
    }

    /// 扫描一次顶层窗口，处理命中的第一个弹窗。
    ///
    /// `confirm` / `yes` / `no` 来自 `[install.popup]`：被配置禁用的动作会被跳过，
    /// 既不点击也不产生事件。
    pub fn scan_popup_once(
        rules: &'static [PopupRule],
        confirm: bool,
        yes: bool,
        no: bool,
        server_value: &str,
    ) -> Option<PopupHandling> {
        let mut search = WindowSearch {
            rules,
            matches: Vec::new(),
        };

        unsafe {
            let _ = EnumWindows(Some(enum_proc), LPARAM(&mut search as *mut _ as isize));
        }

        for (hwnd, title, kind) in search.matches {
            let (allowed, action_label) = match kind {
                PopupActionKind::Confirm => (confirm, "点击「确定」"),
                PopupActionKind::Yes => (yes, "点击「是」"),
                PopupActionKind::No => (no, "点击「否」"),
                PopupActionKind::EnsureServerField => (confirm, "确认服务器字段并点击「确定」"),
            };
            if !allowed {
                continue;
            }

            let executed = match kind {
                PopupActionKind::EnsureServerField => {
                    let entered = ensure_server_field(hwnd, server_value);
                    click_ok(hwnd);
                    if entered {
                        format!("{action_label}（已写入 {server_value}）")
                    } else {
                        format!("{action_label}（输入框未找到，沿用现有值）")
                    }
                }
                PopupActionKind::Confirm => {
                    click_ok(hwnd);
                    action_label.to_string()
                }
                PopupActionKind::Yes => {
                    click_button_text(hwnd, &["是(&Y)", "是(Y)", "&Yes", "Yes", "是"]);
                    action_label.to_string()
                }
                PopupActionKind::No => {
                    click_button_text(hwnd, &["否(&N)", "否(N)", "&No", "No", "否"]);
                    action_label.to_string()
                }
            };

            return Some(PopupHandling {
                title,
                action: executed,
            });
        }

        None
    }

    fn click_ok(hwnd: HWND) {
        unsafe {
            if let Ok(button) = GetDlgItem(Some(hwnd), IDOK) {
                if !button.is_invalid() {
                    let _ = SendMessageW(button, BM_CLICK, None, None);
                    return;
                }
            }
        }
        click_button_text(hwnd, &["确定", "OK", "&OK"]);
    }

    struct ButtonSearch {
        labels: Vec<String>,
        found: Option<HWND>,
    }

    unsafe extern "system" fn enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut ButtonSearch);
        if !window_class(hwnd).eq_ignore_ascii_case("Button") {
            return BOOL(1);
        }
        let text = window_title(hwnd).replace('&', "");
        let trimmed = text.trim();
        if search.labels.iter().any(|label| {
            let normalized = label.replace('&', "");
            trimmed == normalized.trim() || trimmed.contains(normalized.trim())
        }) {
            search.found = Some(hwnd);
            return BOOL(0); // 找到即停止枚举
        }
        BOOL(1)
    }

    fn click_button_text(hwnd: HWND, labels: &[&str]) {
        let mut search = ButtonSearch {
            labels: labels.iter().map(|s| s.to_string()).collect(),
            found: None,
        };
        unsafe {
            let _ = EnumChildWindows(
                Some(hwnd),
                Some(enum_child_proc),
                LPARAM(&mut search as *mut _ as isize),
            );
            if let Some(button) = search.found {
                let _ = SendMessageW(button, BM_CLICK, None, None);
            }
        }
    }

    struct EditSearch {
        found: Option<HWND>,
    }

    unsafe extern "system" fn enum_edit_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut EditSearch);
        if window_class(hwnd).eq_ignore_ascii_case("Edit") {
            search.found = Some(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }

    /// 确认（必要时写入）「端口@服务器」输入框；返回是否找到输入框。
    fn ensure_server_field(hwnd: HWND, value: &str) -> bool {
        let mut search = EditSearch { found: None };
        unsafe {
            let _ = EnumChildWindows(
                Some(hwnd),
                Some(enum_edit_proc),
                LPARAM(&mut search as *mut _ as isize),
            );
        }
        let Some(edit) = search.found else {
            return false;
        };

        if window_title(edit).trim() != value {
            let wide = to_wide(value);
            unsafe {
                let _ = SetWindowTextW(edit, PCWSTR(wide.as_ptr()));
            }
        }
        true
    }

    /// 带超时的消息发送，避免安装器繁忙时卡住守护线程。
    pub fn send_message_timeout(hwnd: HWND, message: u32, wparam: usize) -> bool {
        unsafe {
            let mut result = 0usize;
            SendMessageTimeoutW(
                hwnd,
                message,
                WPARAM(wparam),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                1000,
                Some(&mut result as *mut usize),
            )
            .0 != 0
        }
    }

    // -- 原生文件选择对话框（不依赖 tauri-plugin-dialog） ------------------

    /// 打开原生「选择压缩包」对话框；用户取消时返回 `Ok(None)`。
    pub fn pick_archive_file(title: &str) -> Result<Option<PathBuf>, String> {
        unsafe {
            // 对话框自带模态消息循环；COM 必须在本线程初始化。
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let need_uninit = hr.is_ok();

            let result = pick_archive_file_inner(title);

            if need_uninit {
                CoUninitialize();
            }
            result
        }
    }

    /// 打开原生「选择分卷文件夹」对话框；用户取消时返回 `Ok(None)`。
    pub fn pick_archive_folder(title: &str) -> Result<Option<PathBuf>, String> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let need_uninit = hr.is_ok();
            let result = pick_archive_folder_inner(title);
            if need_uninit {
                CoUninitialize();
            }
            result
        }
    }

    /// 打开原生「保存文件」对话框；用户取消时返回 `Ok(None)`。
    pub fn pick_save_file(title: &str, default_name: &str) -> Result<Option<PathBuf>, String> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let need_uninit = hr.is_ok();
            let result = pick_save_file_inner(title, default_name);
            if need_uninit {
                CoUninitialize();
            }
            result
        }
    }

    unsafe fn pick_save_file_inner(
        title: &str,
        default_name: &str,
    ) -> Result<Option<PathBuf>, String> {
        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("创建保存对话框失败: {e}"))?;

        let label = to_wide("TOML 配置");
        let spec = to_wide("*.toml");
        let filters = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&filters);

        let title_wide = to_wide(title);
        let _ = dialog.SetTitle(PCWSTR(title_wide.as_ptr()));

        // `SetFileName` 与 `Show` 都定义在 IFileDialog 上。
        let base: IFileDialog = dialog.cast().map_err(|e| format!("接口转换失败: {e}"))?;
        let name_wide = to_wide(default_name);
        let _ = base.SetFileName(PCWSTR(name_wide.as_ptr()));
        let extension = to_wide("toml");
        let _ = base.SetDefaultExtension(PCWSTR(extension.as_ptr()));

        if let Err(e) = base.Show(None) {
            if e.code().0 as u32 == HRESULT_CANCELLED {
                return Ok(None);
            }
            return Err(format!("保存对话框执行失败: {e}"));
        }

        let item: IShellItem = dialog
            .GetResult()
            .map_err(|e| format!("获取保存路径失败: {e}"))?;
        let path_ptr: PWSTR = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| format!("解析保存路径失败: {e}"))?;
        let path = if path_ptr.is_null() {
            String::new()
        } else {
            path_ptr.to_string().unwrap_or_default()
        };
        CoTaskMemFree(Some(path_ptr.0 as *const core::ffi::c_void));

        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    /// 取单个进程的完整映像路径（拿不到时返回 `None`）。
    ///
    /// 只做 Win32 层的事；命令行的获取与整批查询由 `process_utils` 负责，
    /// 保持本模块无外部命令依赖。
    pub fn process_image_path(pid: u32) -> Option<String> {
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = vec![0u16; 1024];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
            .is_ok();
            let _ = CloseHandle(handle);
            if !ok || size == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buffer[..size as usize]))
        }
    }

    unsafe fn pick_archive_file_inner(title: &str) -> Result<Option<PathBuf>, String> {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("创建文件对话框失败: {e}"))?;

        let label = to_wide("7z 压缩包");
        let spec = to_wide("*.7z;*.zip;*.rar;*.iso");
        let filters = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        }];
        let _ = dialog.SetFileTypes(&filters);

        let title_wide = to_wide(title);
        let _ = dialog.SetTitle(PCWSTR(title_wide.as_ptr()));

        // `Show` 定义在 IFileDialog 上，IFileOpenDialog 只继承其 vtable。
        let base: IFileDialog = dialog.cast().map_err(|e| format!("接口转换失败: {e}"))?;

        if let Err(e) = base.Show(None) {
            if e.code().0 as u32 == HRESULT_CANCELLED {
                return Ok(None);
            }
            return Err(format!("对话框执行失败: {e}"));
        }

        let item: IShellItem = dialog
            .GetResult()
            .map_err(|e| format!("获取选择结果失败: {e}"))?;
        let path_ptr: PWSTR = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| format!("解析选择路径失败: {e}"))?;

        let path = if path_ptr.is_null() {
            String::new()
        } else {
            path_ptr.to_string().unwrap_or_default()
        };
        CoTaskMemFree(Some(path_ptr.0 as *const core::ffi::c_void));

        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    unsafe fn pick_archive_folder_inner(title: &str) -> Result<Option<PathBuf>, String> {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| format!("创建文件夹对话框失败: {e}"))?;
        let base: IFileDialog = dialog.cast().map_err(|e| format!("接口转换失败: {e}"))?;
        base.SetOptions(FOS_PICKFOLDERS)
            .map_err(|e| format!("设置文件夹选择模式失败: {e}"))?;

        let title_wide = to_wide(title);
        let _ = base.SetTitle(PCWSTR(title_wide.as_ptr()));
        if let Err(e) = base.Show(None) {
            if e.code().0 as u32 == HRESULT_CANCELLED {
                return Ok(None);
            }
            return Err(format!("文件夹对话框执行失败: {e}"));
        }

        let item: IShellItem = dialog
            .GetResult()
            .map_err(|e| format!("获取文件夹路径失败: {e}"))?;
        let path_ptr: PWSTR = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| format!("解析文件夹路径失败: {e}"))?;
        let path = if path_ptr.is_null() {
            String::new()
        } else {
            path_ptr.to_string().unwrap_or_default()
        };
        CoTaskMemFree(Some(path_ptr.0 as *const core::ffi::c_void));

        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    // -- 环境信息 ----------------------------------------------------------

    /// 当前登录用户名。
    pub fn current_username() -> String {
        unsafe {
            let mut size = 0u32;
            let _ = GetUserNameW(None, &mut size);
            if size == 0 {
                return std::env::var("USERNAME").unwrap_or_default();
            }
            let mut buffer = vec![0u16; size as usize];
            if GetUserNameW(Some(PWSTR(buffer.as_mut_ptr())), &mut size).is_err() {
                return std::env::var("USERNAME").unwrap_or_default();
            }
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            String::from_utf16_lossy(&buffer[..end])
        }
    }

    /// 计算机名（物理 DNS 主机名，即 SolidWorks 许可服务实际使用的名字）。
    pub fn current_computer_name() -> String {
        unsafe {
            let mut size = 0u32;
            let _ = GetComputerNameExW(ComputerNamePhysicalDnsHostname, None, &mut size);
            if size == 0 {
                return std::env::var("COMPUTERNAME").unwrap_or_default();
            }
            let mut buffer = vec![0u16; size as usize];
            if GetComputerNameExW(
                ComputerNamePhysicalDnsHostname,
                Some(PWSTR(buffer.as_mut_ptr())),
                &mut size,
            )
            .is_err()
            {
                return std::env::var("COMPUTERNAME").unwrap_or_default();
            }
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            String::from_utf16_lossy(&buffer[..end])
        }
    }

    /// 目标磁盘可用空间（GB）；盘符不存在时返回 `-1.0`。
    pub fn free_space_gb(drive: &str) -> f64 {
        let root = to_wide(&format!("{}:\\", drive.trim_end_matches(':')));
        unsafe {
            let mut free = 0u64;
            if GetDiskFreeSpaceExW(
                PCWSTR(root.as_ptr()),
                Some(&mut free),
                None,
                None,
            )
            .is_err()
            {
                return -1.0;
            }
            free as f64 / 1024.0 / 1024.0 / 1024.0
        }
    }

    /// 枚举注册表某个键下的子键，返回那些含有 `value_name` 值的
    /// `(子键名, 值内容)` 列表。
    ///
    /// 用于扫描卸载信息（`...\Uninstall\<产品>`）按产品名找安装目录。
    pub fn registry_subkeys_with_value(
        root: isize,
        subkey: &str,
        value_name: &str,
    ) -> Vec<(String, String)> {
        use windows::Win32::System::Registry::{
            RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, KEY_READ,
        };

        let mut result = Vec::new();
        let subkey_wide = to_wide(subkey);

        unsafe {
            let mut handle = HKEY::default();
            if RegOpenKeyExW(
                HKEY(root as *mut core::ffi::c_void),
                PCWSTR(subkey_wide.as_ptr()),
                None,
                KEY_READ,
                &mut handle,
            )
            .is_err()
            {
                return result;
            }

            let mut index = 0u32;
            loop {
                let mut name = vec![0u16; 256];
                let mut name_len = name.len() as u32;
                let status = RegEnumKeyExW(
                    handle,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    None,
                    None,
                );
                if status.is_err() {
                    break;
                }
                index += 1;

                let child = String::from_utf16_lossy(&name[..name_len as usize]);
                let child_path = format!("{subkey}\\{child}");
                if let Some(value) = read_registry_string(root, &child_path, value_name) {
                    result.push((child, value));
                }
                // 防御：卸载项通常几十条，上千条说明键选错了。
                if index > 4000 {
                    break;
                }
            }

            let _ = RegCloseKey(handle);
        }

        result
    }

    /// 在卸载信息里按产品名找 `UninstallString`。
    ///
    /// 扫描 HKLM（含 WOW6432Node）与 HKCU 的 `...\Uninstall\*`，
    /// 用 `DisplayName` 做**不区分大小写**的双向子串匹配。
    pub fn find_uninstall_string(display_name: &str) -> Option<String> {
        let needle = display_name.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return None;
        }

        let targets: [(isize, &str); 3] = [
            (
                super::HKEY_LOCAL_MACHINE,
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
            (
                super::HKEY_LOCAL_MACHINE,
                r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
            (
                super::HKEY_CURRENT_USER,
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            ),
        ];

        for (root, base) in targets {
            for (sub, _) in registry_subkeys_with_value(root, base, "DisplayName") {
                let path = format!("{base}\\{sub}");
                let Some(name) = read_registry_string(root, &path, "DisplayName") else {
                    continue;
                };
                let haystack = name.to_ascii_lowercase();
                if !haystack.contains(&needle) && !needle.contains(&haystack) {
                    continue;
                }
                if let Some(command) = read_registry_string(root, &path, "UninstallString") {
                    let command = command.trim().to_string();
                    if !command.is_empty() {
                        return Some(command);
                    }
                }
            }
        }

        None
    }

    /// 读取注册表字符串值；键或值不存在时返回 `None`。
    ///
    /// `root` 用 [`super::HKEY_LOCAL_MACHINE`] 之类的常量。
    pub fn read_registry_string(root: isize, subkey: &str, value_name: &str) -> Option<String> {
        use windows::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, KEY_READ, REG_SZ,
        };

        let subkey_wide = to_wide(subkey);
        let value_wide = to_wide(value_name);

        unsafe {
            let mut handle = HKEY::default();
            let status = RegOpenKeyExW(
                HKEY(root as *mut core::ffi::c_void),
                PCWSTR(subkey_wide.as_ptr()),
                None,
                KEY_READ,
                &mut handle,
            );
            if status.is_err() {
                return None;
            }

            // 先用 0 长度查询拿到所需字节数。
            let mut kind = REG_SZ;
            let mut size = 0u32;
            let probe = RegQueryValueExW(
                handle,
                PCWSTR(value_wide.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            );
            if probe.is_err() || size == 0 {
                let _ = RegCloseKey(handle);
                return None;
            }

            let mut buffer = vec![0u8; size as usize];
            let read = RegQueryValueExW(
                handle,
                PCWSTR(value_wide.as_ptr()),
                None,
                Some(&mut kind),
                Some(buffer.as_mut_ptr()),
                Some(&mut size),
            );
            let _ = RegCloseKey(handle);
            if read.is_err() {
                return None;
            }

            // 注册表字符串是 UTF-16，可能带结尾的 0。
            let wide: Vec<u16> = buffer
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|unit| *unit != 0)
                .collect();
            if wide.is_empty() {
                return None;
            }
            Some(String::from_utf16_lossy(&wide))
        }
    }

    /// 读取注册表 `REG_DWORD` 值；键或值不存在时返回 `None`。
    ///
    /// 用于判断 .NET Framework 版本（`NDP\v4\Full` 的 `Release`）这类数值标志。
    pub fn read_registry_dword(root: isize, subkey: &str, value_name: &str) -> Option<u32> {
        use windows::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, KEY_READ,
        };

        let subkey_wide = to_wide(subkey);
        let value_wide = to_wide(value_name);

        unsafe {
            let mut handle = HKEY::default();
            let status = RegOpenKeyExW(
                HKEY(root as *mut core::ffi::c_void),
                PCWSTR(subkey_wide.as_ptr()),
                None,
                KEY_READ,
                &mut handle,
            );
            if status.is_err() {
                return None;
            }

            let mut kind = windows::Win32::System::Registry::REG_NONE;
            let mut buffer = [0u8; 4];
            let mut size = buffer.len() as u32;
            let read = RegQueryValueExW(
                handle,
                PCWSTR(value_wide.as_ptr()),
                None,
                Some(&mut kind),
                Some(buffer.as_mut_ptr()),
                Some(&mut size),
            );
            let _ = RegCloseKey(handle);
            if read.is_err() || size < 4 {
                return None;
            }
            Some(u32::from_le_bytes([
                buffer[0], buffer[1], buffer[2], buffer[3],
            ]))
        }
    }

    /// 当前进程是否为 64 位。
    pub fn is_64bit_process() -> bool {
        std::mem::size_of::<usize>() == 8
    }

    fn to_wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

// ===========================================================================
// 非 Windows 桩实现
// ===========================================================================

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::path::Path;

    const UNSUPPORTED: &str = "当前平台不支持 Windows 原生能力";

    pub fn is_elevated() -> bool {
        false
    }

    pub fn relaunch_elevated(_args: &str) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn split_command_line(command_line: &str) -> Vec<String> {
        command_line
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn list_adapters() -> (Vec<AdapterInfo>, Option<String>) {
        (Vec::new(), Some(UNSUPPORTED.to_string()))
    }

    pub fn is_virtual_adapter(_adapter: &AdapterInfo) -> bool {
        true
    }

    pub fn list_processes() -> Vec<(u32, String)> {
        Vec::new()
    }

    pub fn force_kill_pid(_pid: u32) -> bool {
        false
    }

    pub fn scan_popup_once(
        _rules: &'static [PopupRule],
        _confirm: bool,
        _yes: bool,
        _no: bool,
        _server_value: &str,
    ) -> Option<PopupHandling> {
        None
    }

    pub fn pick_archive_file(_title: &str) -> Result<Option<PathBuf>, String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn pick_archive_folder(_title: &str) -> Result<Option<PathBuf>, String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn pick_save_file(_title: &str, _default_name: &str) -> Result<Option<PathBuf>, String> {
        Err(UNSUPPORTED.to_string())
    }

    pub fn process_image_path(_pid: u32) -> Option<String> {
        None
    }

    pub fn current_username() -> String {
        std::env::var("USERNAME").unwrap_or_default()
    }

    pub fn current_computer_name() -> String {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }

    pub fn free_space_gb(_drive: &str) -> f64 {
        -1.0
    }

    pub fn is_usable_file(path: &Path) -> bool {
        path.is_file() && path.metadata().map(|m| m.len() > 0).unwrap_or(false)
    }

    pub fn read_registry_string(_root: isize, _subkey: &str, _value_name: &str) -> Option<String> {
        None
    }

    pub fn registry_subkeys_with_value(
        _root: isize,
        _subkey: &str,
        _value_name: &str,
    ) -> Vec<(String, String)> {
        Vec::new()
    }

    pub fn find_uninstall_string(_display_name: &str) -> Option<String> {
        None
    }

    pub fn read_registry_dword(_root: isize, _subkey: &str, _value_name: &str) -> Option<u32> {
        None
    }

    pub fn is_64bit_process() -> bool {
        std::mem::size_of::<usize>() == 8
    }
}

pub use imp::*;

// ===========================================================================
// 平台无关辅助
// ===========================================================================

/// ASCII 判定：用户名与计算机名只允许可打印 ASCII。
///
/// SolidWorks / FlexNet 无法在含中文或特殊符号的计算机名下启动许可服务。
pub fn is_pure_ascii(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii() && !c.is_ascii_control())
}

/// 从适配器列表中挑选出「应当被禁用」的物理网卡。
pub fn physical_adapters(adapters: &[AdapterInfo]) -> Vec<AdapterInfo> {
    adapters
        .iter()
        .filter(|adapter| !is_virtual_adapter(adapter))
        .cloned()
        .collect()
}
