// Tauri 构建脚本。
//
// 负责生成权限清单、Windows 资源文件（需要 `icons/icon.ico`）并把
// `tauri.conf.json` 编译进二进制。
fn main() {
    tauri_build::build()
}
