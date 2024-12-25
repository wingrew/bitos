//! 文件特征与 inode（目录、文件、管道、标准输入输出）

mod inode;
mod stdio;
mod pipe;
use crate::mm::UserBuffer;

/// 为所有文件类型定义的 File trait
/// 所有类型的文件（如普通文件、目录、管道等）都应实现这个 trait
pub trait File: Send + Sync {
    /// 判断文件是否可读
    fn readable(&self) -> bool;
    
    /// 判断文件是否可写
    fn writable(&self) -> bool;
    
    /// 从文件中读取数据到缓冲区 buf，返回读取的字节数
    fn read(&self, buf: UserBuffer) -> usize;
    
    /// 向文件写入数据从缓冲区 buf，返回写入的字节数
    fn write(&self, buf: UserBuffer) -> usize;
    
    /// 尝试获取该文件对应的 OSInode（操作系统级别的 inode）
    fn as_osinode(&self) -> Option<&OSInode> {
        None
    }
}

/// inode 的状态结构体
/// 描述文件的元数据（如设备 ID、inode 编号、文件类型等）
#[repr(C)]
#[derive(Debug)]
pub struct Stat {
    /// 文件所在的设备 ID
    pub dev: u64,
    
    /// inode 编号
    pub ino: u64,
    
    /// 文件的类型和模式（例如普通文件、目录等）
    pub mode: StatMode,
    
    /// 硬链接的数量
    pub nlink: u32,
    
    /// 填充字段，保持结构体对齐
    pad: [u64; 7],
}

impl Stat {
    /// 使用默认值来初始化 inode 的状态
    pub fn new_with_defaults(dev: u64, ino: u64, mode: StatMode, nlink: u32) -> Self {
        Stat {
            dev,
            ino,
            mode,
            nlink,
            pad: [0; 7],  // 默认填充字段初始化为零
        }
    }
}

bitflags! {
    /// inode 的模式（文件类型）
    /// 这里定义了 inode 的不同类型（如目录、普通文件等）
    pub struct StatMode: u32 {
        /// 空类型
        const NULL  = 0;
        
        /// 目录类型
        const DIR   = 0o040000;
        
        /// 普通文件类型
        const FILE  = 0o100000;
    }
}

pub use inode::ROOT_INODE;  // 引入 ROOT_INODE 常量，表示根目录 inode
pub use inode::{open_file, OSInode, OpenFlags, search_pwd, chdir};  // 引入与文件操作相关的函数和类型
pub use stdio::{Stdin, Stdout};  // 引入标准输入输出类型
pub use pipe::make_pipe;  // 引入管道创建函数

/// 列出所有应用程序
/// 遍历根目录下的文件，并打印出文件名
pub fn list_apps() -> i32 {
    // 获取根目录下的文件列表
    let name = ROOT_INODE.ls();
    
    match name {
        Some(value) => {
            // 遍历文件列表并打印文件名
            for i in &value {
                println!("{}", i.0);
            }
        }
        None => {
            // 如果没有文件，则返回 0
            0;
        }
    }
    
    0
}
