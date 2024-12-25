use super::File;
use crate::task::current_task;
use crate::{drivers::BLOCK_DEVICE, syscall::AT_FDCWD};
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use fat32::{FAT32Manager, VFile, ATTRIBUTE_ARCHIVE};
use lazy_static::*;

/// 文件系统中的 inode
/// 包装一个文件系统 inode，方便在操作系统中实现 File trait
pub struct OSInode {
    readable: bool,    // 是否可读
    writable: bool,    // 是否可写
    /// 存储在 UPSafeCell 中的 inode 内部结构
    pub inner: UPSafeCell<OSInodeInner>,
}

/// 存储在 UPSafeCell 中的 inode 的内部结构
pub struct OSInodeInner {
    offset: usize,     // 当前读取/写入的偏移量
    pub inode: Arc<VFile>, // 文件的 VFile 对象
}

impl OSInode {
    /// 创建一个新的 inode
    pub fn new(readable: bool, writable: bool, inode: Arc<VFile>) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner { offset: 0, inode }) },
        }
    }

    /// 从 inode 中读取所有数据
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();  // 获取排他访问
        let mut buffer = [0u8; 512];  // 缓冲区
        let mut v: Vec<u8> = Vec::new();  // 存放读取数据的 Vector
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);  // 读取数据
            if len == 0 {
                break;
            }
            inner.offset += len;  // 更新偏移量
            v.extend_from_slice(&buffer[..len]);  // 将读取的数据扩展到结果 Vector 中
        }
        v
    }

    /// 创建目录
    pub fn mkdir(&self, name:&str, attribute:u8) -> isize {
        let inner = self.inner.exclusive_access();
        inner.inode.create(name, attribute);  // 调用 VFile 创建目录
        0  // 返回 0，表示成功
    }
}

lazy_static! {
    /// 文件系统根目录的 inode
    pub static ref ROOT_INODE: Arc<VFile> = {
        let efs = FAT32Manager::open(BLOCK_DEVICE.clone());  // 打开 FAT32 文件系统
        Arc::new(FAT32Manager::get_root_vfile(&efs))  // 获取根目录的 VFile
    };
}

/// 查找当前工作目录的文件
pub fn search_pwd(name: &str) -> Option<Arc<VFile>> {
    let path: Vec<&str> = name.split('/').collect();  // 将路径按 '/' 切割
    ROOT_INODE.find_vfile_bypath(path)  // 根据路径查找文件
}

bitflags! {
    /// open() 系统调用的 flags 参数，表示文件操作的权限和选项
    pub struct OpenFlags: u32 {
        /// 只读
        const RDONLY = 0;
        /// 只写
        const WRONLY = 1 << 0;
        /// 读写
        const RDWR = 1 << 1;
        /// 创建新文件
        const CREATE = 1 << 6;
        /// 截断文件大小为 0
        const TRUNC = 1 << 10;
        /// 目录
        const O_DIRECTORY = 1 << 21;
    }
}

impl OpenFlags {
    /// 根据 flags 返回文件的可读和可写权限
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)  // 默认是只读
        } else if self.contains(Self::WRONLY) {
            (false, true)  // 只写
        } else {
            (true, true)  // 读写
        }
    }
}

/// 打开文件
pub fn open_file(fd: i64, mut name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();  // 获取文件的读写权限
    let task = current_task().unwrap();  // 获取当前任务
    let inner = task.inner_exclusive_access();  // 获取当前任务的排他访问
    let binding1 = inner.pwd.clone();
    let pwd = binding1.as_str();  // 当前工作目录
    let mut vfile: Arc<VFile>;
    let path: Vec<&str> = name.split('/').collect();  // 将路径按 '/' 切割
    
    if name.chars().next().unwrap() == '/' {  // 如果路径以 '/' 开头
        if let Some(vfile) = search_pwd(name) {  // 查找路径对应的文件
            return Some(Arc::new(OSInode::new(readable, writable, vfile)));
        } else {
            return ROOT_INODE
                .create(name, ATTRIBUTE_ARCHIVE)  // 创建文件
                .map(|inode| Arc::new(OSInode::new(readable, writable, inode)));
        }
    } else if fd as isize == AT_FDCWD || name == "." {  // 如果是相对路径
        if pwd == "/" && name != "." {
            if flags.contains(OpenFlags::CREATE) {
                if let Some(inode) = ROOT_INODE.find_vfile_bypath(path) {
                    // 清空文件大小
                    inode.clear();
                    return Some(Arc::new(OSInode::new(readable, writable, inode)));
                } else {
                    // 创建文件
                    if name.chars().next().unwrap() == '.' {
                        if name.chars().nth(1).unwrap() == '/' {
                            name = &name[2..];
                        }
                    }
                    return ROOT_INODE
                        .create(name, ATTRIBUTE_ARCHIVE)
                        .map(|inode| Arc::new(OSInode::new(readable, writable, inode)));
                }
            } else {
                match ROOT_INODE.find_vfile_bypath(path) {
                    Some(inode) => {
                        if flags.contains(OpenFlags::TRUNC) {
                            inode.clear();  // 清空文件
                        }
                        return Some(Arc::new(OSInode::new(readable, writable, inode)));
                    }
                    None => return None,  // 文件不存在
                }
            }
        } else {
            vfile = search_pwd(pwd).unwrap();
        }
    } else {
        if let Some(file) = &inner.fd_table[fd as usize] {
            let osinode = file.as_osinode().unwrap();
            vfile = osinode.inner.exclusive_access().inode.clone();
            drop(inner);
        } else {
            drop(inner);
            return None;
        }
    }

    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = vfile.find_vfile_bypath(path) {
            // 清空文件大小
            inode.clear();
            return Some(Arc::new(OSInode::new(readable, writable, inode)));
        } else {
            // 创建文件
            return vfile
                .create(name, ATTRIBUTE_ARCHIVE)
                .map(|inode| Arc::new(OSInode::new(readable, writable, inode)));
        }
    } else {
        match vfile.find_vfile_bypath(path) {
            Some(inode) => {
                if flags.contains(OpenFlags::TRUNC) {
                    inode.clear();  // 清空文件
                }
                return Some(Arc::new(OSInode::new(readable, writable, inode)));
            }
            None => return None,  // 文件不存在
        }
    }
}

/// 改变当前工作目录
pub fn chdir(name: &str) -> bool {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let binding1 = inner.pwd.clone();
    let pwd = binding1.as_str();
    let path: Vec<&str> = name.split('/').collect();
    let path1: Vec<&str> = name.split('/').collect();
    let mut newpwd: Vec<&str> = pwd.split('/').collect();
    
    if pwd == "/" || name.chars().next().unwrap() == '/' {
        if path[0] == ".." {
            return false;  // 无效路径
        }
        if let Some(_) = ROOT_INODE.find_vfile_bypath(path) {
            inner.set_pwd(String::from(name));  // 设置新路径
            return true;
        } else {
            return false;
        }
    } else {
        let vfile = search_pwd(name).unwrap();
        if let Some(_) = vfile.find_vfile_bypath(path) {
            for pa in path1 {
                if pa == ".." {
                    newpwd.pop();  // 返回上一级目录
                } else if pa == "." {
                    continue;  // 当前目录，不做任何操作
                } else {
                    newpwd.push(pa);  // 添加新目录
                }
            }
            let new_path = newpwd.join("/");
            inner.set_pwd(new_path);  // 设置新路径
            return true;
        } else {
            return false;
        }
    }
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable  // 返回文件是否可读
    }
    fn writable(&self) -> bool {
        self.writable  // 返回文件是否可写
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);  // 从文件读取数据
            if read_size == 0 {
                break;  // 如果没有数据了，停止读取
            }
            inner.offset += read_size;  // 更新偏移量
            total_read_size += read_size;  // 累加读取字节数
        }
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at(inner.offset, *slice);  // 向文件写入数据
            assert_eq!(write_size, slice.len());  // 确保写入的字节数与预期一致
            inner.offset += write_size;  // 更新偏移量
            total_write_size += write_size;  // 累加写入字节数
        }
        total_write_size
    }
    
    // 将文件转换为 OSInode 类型
    fn as_osinode(&self) -> Option<&OSInode> {
        Some(self)
    }
}
