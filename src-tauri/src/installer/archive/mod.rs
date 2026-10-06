//! 第 2 步：获取压缩包。
//!
//! | 子模块 | 职责 |
//! |---|---|
//! | [`sniff`] | 格式嗅探与缓存命名（纯函数） |
//! | [`filename`] | 从 URL / 响应头解析真实文件名（网盘直链必需） |
//! | [`fetch`] | 单包下载（有副作用） |
//! | [`multipart`] | 分片/分卷下载、命名归一、拼接 |

pub mod fetch;
pub mod filename;
pub mod multipart;
pub mod sniff;

pub use fetch::{acquire_archive, resolve_local_archive};
pub use filename::{
    parse_content_disposition, percent_decode, resolve_from_url, resolve_name,
    set_volume_number, volume_base, volume_number, NameOrigin, ResolvedName,
};
pub use sniff::{cache_file_name, is_volume_part_name, sniff_archive_kind};
