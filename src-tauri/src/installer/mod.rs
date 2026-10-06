//! 安装编排。
//!
//! 本模块原先是一个 3000+ 行的单文件，现按**安装阶段**拆分，每个子模块只负责一步：
//!
//! | 子模块 | 负责阶段 | 行数 |
//! |---|---|---|
//! | [`context`] | 运行上下文与状态单元（`InstallContext` / `InstallStatusCell`） | 小 |
//! | [`pipeline`] | 13 阶段主编排 `run()` | 中 |
//! | [`steps`] | 第 1 步环境检测、第 2.5 步安全软件处置 | 小 |
//! | [`sevenzip`] | 7z 定位 / 变体检测 / 自动下载 | 中 |
//! | [`archive`] | 第 2 步取包（单包、分片、嗅探、缓存） | 大 |
//! | [`extract`] | 第 4、5 步两阶段解压 | 小 |
//! | [`registry`] | 第 6 步导入注册表 | 小 |
//! | [`iso`] | 第 7 步挂载 ISO 与定位安装入口 | 小 |
//! | [`install_ops`] | 第 8 步启动安装器 + 弹窗守护 | 小 |
//! | [`verify`] | 第 9 步轮询检测完成 | 中 |
//! | [`postinstall`] | 第 10、11 步进程清理 + 文件替换 |
//! | [`prereqs`] | Windows 前置组件检测与安装（.NET / VC++ / WebView2 / VBA） | 中 |
//! | [`flexnet`] | 第 12 步 FlexNet 服务 | 小 |
//! | [`guards`] | `Drop` 兜底守卫（ISO / 临时目录） | 小 |
//! | [`paths`] | 大小写不敏感的文件系统查找 | 小 |
//!
//! # 依赖方向
//!
//! ```text
//! pipeline ──► 各阶段模块 ──► context / paths / guards
//! ```
//!
//! 阶段模块之间**不互相调用**（`extract` 只借用 `archive` 的纯函数
//! `sniff_archive_kind`），因此不存在循环依赖。
//!
//! # 对外接口
//!
//! 为了让 `commands.rs` / `lib.rs` 里的 `installer::xxx` 调用点完全不用改，
//! 这里把原先 `pub` 的项原样 re-export。这些项属于内部实现细节，
//! 因此标记 `#[doc(hidden)]`，避免污染公开文档。

pub mod archive;
pub mod context;
pub mod extract;
pub mod flexnet;
pub mod guards;
pub mod install_ops;
pub mod iso;
pub mod paths;
pub mod pipeline;
pub mod postinstall;
pub mod prereqs;
pub mod registry;
pub mod sevenzip;
pub mod steps;
pub mod verify;

#[doc(hidden)]
pub use context::{InstallContext, InstallStatusCell};
#[doc(hidden)]
pub use pipeline::run;
pub use sevenzip::{
    describe_sevenzip, download_seven_zip, ensure_seven_zip, resolve_seven_zip,
    seven_zip_available, seven_zip_status, SevenZipResolution, SevenZipSource,
    CACHED_SEVEN_ZIP_NAME,
};
#[doc(hidden)]
pub use archive::{
    acquire_archive, cache_file_name, is_volume_part_name, resolve_local_archive,
    sniff_archive_kind,
};
#[doc(hidden)]
pub use paths::{
    find_dirs_named, find_file_named, find_files_matching, find_files_named, find_first_dir,
    humanize_duration, walk_entries, wildcard_ci,
};
