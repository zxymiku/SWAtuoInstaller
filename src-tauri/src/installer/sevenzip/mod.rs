//! 7z 解压程序的定位、变体检测与自动下载。
//!
//! | 子模块 | 职责 |
//! |---|---|
//! | [`lookup`] | 定位与变体检测（只读） |
//! | [`fetch`] | 可用性判定 / 状态 / 自动下载 |

pub mod fetch;
pub mod lookup;

pub use fetch::{
    describe_sevenzip, download_seven_zip, ensure_seven_zip, seven_zip_available, seven_zip_status,
};
pub use lookup::{
    resolve_seven_zip, SevenZipResolution, SevenZipSource, CACHED_SEVEN_ZIP_NAME,
};
