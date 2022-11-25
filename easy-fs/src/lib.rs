#![no_std]

extern crate alloc;

mod block_dev;
pub mod layout;
mod efs;
mod bitmap;
mod vfs;
pub mod block_cache;

/// Use a block size of 512 bytes
pub const BLOCK_SZ: usize = 512;
pub use block_dev::BlockDevice;
pub use efs::EasyFileSystem;
pub use vfs::Inode;
pub use layout::*;
use bitmap::Bitmap;
pub use block_cache::{get_block_cache, block_cache_sync_all};
