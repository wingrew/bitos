#![no_std]
extern crate alloc;

// block大小（即 sector 大小为 512 bytes）
// 1 cluster 设定为 1 sector
// BiosParamter: 0 sector
// Fs info: 1 sector
// FAT1: 2-5 sector
// FAT2: 6-9 sector
// DirEntry: 10-21 sector
// Data: 22-8191 sector

mod block_cache;
mod block_dev;
mod fat;
mod layout;
mod vfs;

// fat32 文件系统的一些常量
pub const BLOCK_SZ: usize = 512;
pub const SECTOR_SIZE: usize = 8192;
pub const FAT_SIZE: usize = 400;
pub const DATA_SIZE: usize = 7390;

pub const FIRST_FAT_SEC: usize = 2;
extern crate lazy_static;
extern crate spin;
use block_cache::{get_block_cache, get_info_cache, set_start_sec, write_to_dev, CacheMode};
pub use block_dev::BlockDevice;
pub use fat::FAT32Manager;
pub use layout::ShortDirEntry;
pub use layout::*;
pub use vfs::VFile;

pub fn clone_into_array<A, T>(slice: &[T]) -> A
where
    A: Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}
