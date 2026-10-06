// Windows 子系统应用程序。
//
// 真正的初始化逻辑在 `solidworks_installer_lib::run()` 中——
// 保持 `main.rs` 为薄壳，可以让 `cargo check` 只关注库代码结构。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    solidworks_installer_lib::run()
}
