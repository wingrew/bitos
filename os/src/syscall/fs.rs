use core::ptr::copy_nonoverlapping;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::fs::{chdir, make_pipe, open_file, search_pwd, OpenFlags};
use crate::mm::{translated_byte_buffer, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};
use super::AT_FDCWD;

/// sys_write 系统调用，向文件描述符写入数据
/// fd: 文件描述符
/// buf: 数据缓冲区
/// len: 写入的字节数
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    // 检查文件描述符是否合法
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // 手动释放当前任务 TCB，以避免多次借用
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

/// sys_read 系统调用，从文件描述符读取数据
/// fd: 文件描述符
/// buf: 数据缓冲区
/// len: 读取的字节数
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    // 检查文件描述符是否合法
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // 手动释放当前任务 TCB，以避免多次借用
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

/// sys_openat 系统调用，打开文件
/// fd: 基准文件描述符（可以是AT_FDCWD，表示当前工作目录）
pub fn sys_openat(fd: i64, path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let token = current_user_token();
    let binding = translated_str(token, path);
    
    let path = binding.as_str();
    if let Some(inode) = open_file(fd, path, OpenFlags::from_bits(flags).unwrap()) {
        
        let task = current_task().unwrap();
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

/// sys_close 系统调用，关闭文件描述符
pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    // 检查文件描述符是否合法
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    
    0
}

/// sys_getcwd 系统调用，获取当前工作目录
pub fn sys_getcwd(buf: *mut u8, size:u32) -> isize {
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    
    let pwd = inner.pwd.clone();
    if pwd.len() > size as usize{
        return -1;
    }
    drop(inner);

    let mut ti = translated_byte_buffer(current_user_token(),  buf, size as usize);
    let total_bytes = pwd.len();
    let mut bytes_written = 0;
    let src_ptr = pwd.as_ptr();
    for slice in ti.iter_mut(){
        let slice_len = slice.len();
        let mut offset = 0;
        while offset < slice_len && bytes_written < total_bytes{
            unsafe {
                let to_write = (total_bytes - bytes_written).min(slice_len - offset);
                let ptr = slice.as_mut_ptr().add(offset);
                copy_nonoverlapping(src_ptr.add(bytes_written), ptr, to_write);
            }
            offset += slice_len;
            bytes_written += slice_len;
        }
        if bytes_written >= total_bytes {
            break;
        }
    }
    return pwd.as_ptr() as isize;
}

/// sys_mkdirat 系统调用，创建目录
pub fn sys_mkdirat(fd: i64, path: *const u8, attri: u8) -> isize {
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    let inner = task.inner_exclusive_access();
    if fd as isize == AT_FDCWD {
        let pwd = inner.pwd.clone();
        if let Some(file) = search_pwd(pwd.as_str()) {
            file.create(path.as_str(), attri);
            return 0;
        } else {
            return -1;
        }
    } else {
        if let Some(file) = &inner.fd_table[fd as usize] {
            let osinode = file.as_osinode().unwrap();
            osinode.mkdir(path.as_str(), attri)
        } else {
            -1
        }
    }
}

/// sys_chdir 系统调用，改变当前工作目录
pub fn sys_chdir(path: *const u8) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    if chdir(path.as_str()) {
        return 0;
    } else {
        return -1;
    }
}

/// sys_dup 系统调用，复制文件描述符
pub fn sys_dup(fd:usize) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd < inner.fd_table.len() && !inner.fd_table[fd].is_none() {
        let newfd = inner.alloc_fd();
        inner.fd_table[newfd] = inner.fd_table[fd].clone();
        newfd as isize
    } else {
        -1
    }
}

/// sys_dup3 系统调用，复制文件描述符并指定新描述符
pub fn sys_dup3(fd:usize, newfd:usize) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd < inner.fd_table.len() && !inner.fd_table[fd].is_none() {
        for _ in inner.fd_table.len().. newfd + 1 {
            inner.fd_table.push(None);
        }
        inner.fd_table[newfd] = inner.fd_table[fd].clone();
        newfd as isize
    } else {
        -1
    }
}

/// sys_pipe2 系统调用，创建管道
pub fn sys_pipe2(pipe: *mut u32) -> isize {
    let task = current_task().unwrap();
    let token = current_user_token();
    let mut inner = task.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(pipe_read);
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(pipe_write);
    *translated_refmut(token, pipe) = read_fd as u32;
    *translated_refmut(token, unsafe { pipe.add(1) }) = write_fd as u32;
    0
}

/// sys_fstat 系统调用，获取文件状态信息
pub fn sys_fstat(fd:usize, lkstat:*mut u8) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd < inner.fd_table.len() && !inner.fd_table[fd].is_none() {
        let file = &inner.fd_table[fd];
        let vfile = file.clone().unwrap().as_osinode().unwrap().inner.exclusive_access().inode.clone();
        let all = vfile.stat().to_bytes();
        let mut ti = translated_byte_buffer(token,  lkstat, 128 as usize);
        let total_bytes = 128;
        let mut bytes_written = 0;
        let src_ptr = all.as_ptr();
        for slice in ti.iter_mut(){
            let slice_len = slice.len();
            let mut offset = 0;
            while offset < slice_len && bytes_written < total_bytes{
                unsafe {
                    let to_write = (total_bytes - bytes_written).min(slice_len - offset);
                    let ptr = slice.as_mut_ptr().add(offset);
                    copy_nonoverlapping(src_ptr.add(bytes_written), ptr, to_write);
                }
                offset += slice_len;
                bytes_written += slice_len;
            }
            if bytes_written >= total_bytes {
                break;
            }
        }
    } else {
        return -1;
    }
    0
}

/// sys_unlink 系统调用，删除文件或目录
pub fn sys_unlink(dir:i32, path: *const u8) -> isize {
    let token = current_user_token();
    let mut path = translated_str(token, path);
    if path.chars().next().unwrap() == '/' {
        if let Some(vfile) = search_pwd(path.as_str()) {
            vfile.remove();
        } else {
            return -1;
        }
    } else {
        if path.chars().next().unwrap() == '.' {
            path = path[2..].to_string();
        }
        if dir as isize == AT_FDCWD {
            let task = current_task().unwrap();
            let inner = task.inner_exclusive_access();
            let mut pwd = inner.pwd.clone();
            if pwd != "/" {
                pwd.push_str("/");
            }
            pwd.push_str(&path);
            if let Some(vfile) = search_pwd(path.as_str()) {
                vfile.remove();
            } else {
                return -1;
            }
        } else {
            let task = current_task().unwrap();
            let inner = task.inner_exclusive_access();
            if let Some(file) = &inner.fd_table[dir as usize] {
                let osinode = file.as_osinode().unwrap();
                let vfile = osinode.inner.exclusive_access().inode.clone();
                let path: Vec<&str> = path.split('/').collect();
                if let Some(vfile1) = vfile.find_vfile_bypath(path) {
                    vfile1.remove();       
                } else {
                    return -1;
                }
            } else {
                return -1;
            }
        }
    }
    0
}

/// sys_uname 系统调用，获取系统信息
pub fn sys_uname(utsname:*mut u8) -> isize {
    let token = current_user_token();
    let sysname = "\nsysname:bitos\n";
    let nodename = "nodename:wingrew\n";
    let release = "release:0.1\n";
    let version = "version:0.1\n";
    let machine = "machine:riscv64\n";
    let domainname = "domainname:nudt";
    let mut all:[u8;65*6] = [0;65*6];
    all[..sysname.len()].copy_from_slice(sysname.as_bytes());
    all[65..65+nodename.len()].copy_from_slice(nodename.as_bytes());
    all[65*2..65*2+release.len()].copy_from_slice(release.as_bytes());
    all[65*3..65*3+version.len()].copy_from_slice(version.as_bytes());
    all[65*4..65*4+machine.len()].copy_from_slice(machine.as_bytes());
    all[65*5..65*5+domainname.len()].copy_from_slice(domainname.as_bytes());

    let mut ti = translated_byte_buffer(token,  utsname, 65*6 as usize);
    let total_bytes = 65*6;
    let mut bytes_written = 0;
    let src_ptr = all.as_ptr();
    for slice in ti.iter_mut(){
        let slice_len = slice.len();
        let mut offset = 0;
        while offset < slice_len && bytes_written < total_bytes{
            unsafe {
                let to_write = (total_bytes - bytes_written).min(slice_len - offset);
                let ptr = slice.as_mut_ptr().add(offset);
                copy_nonoverlapping(src_ptr.add(bytes_written), ptr, to_write);
            }
            offset += slice_len;
            bytes_written += slice_len;
        }
        if bytes_written >= total_bytes {
            break;
        }
    }
    0
}

/// sys_getdents64 系统调用，读取目录项
pub fn sys_getdents64(fd:usize, buf:*mut u8, len:usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd < inner.fd_table.len() && !inner.fd_table[fd].is_none() {
        let file = &inner.fd_table[fd];
        let vfile = file.clone().unwrap().as_osinode().unwrap().inner.exclusive_access().inode.clone();
        let all = vfile.dirent_info().unwrap().to_bytes();
        let mut ti = translated_byte_buffer(token,  buf, len as usize);
        let total_bytes = len;
        let mut bytes_written = 0;
        let src_ptr = all.as_ptr();
        for slice in ti.iter_mut(){
            let slice_len = slice.len();
            let mut offset = 0;
            while offset < slice_len && bytes_written < total_bytes{
                unsafe {
                    let to_write = (total_bytes - bytes_written).min(slice_len - offset);
                    let ptr = slice.as_mut_ptr().add(offset);
                    copy_nonoverlapping(src_ptr.add(bytes_written), ptr, to_write);
                }
                offset += slice_len;
                bytes_written += slice_len;
            }
            if bytes_written >= total_bytes {
                break;
            }
        }
    } else {
        return -1;
    }
    return len as isize;
}

/// sys_mount 系统调用，挂载文件系统
pub fn sys_mount(source:*const u8, target:*const u8, filesystem:*const u8, _flags:i64, data:*const u8) -> isize {
    let token = current_user_token();
    let source = translated_str(token, source);
    let target = translated_str(token, target);
    let filesystem = translated_str(token, filesystem);
    let mut data1:String = String::new();
    if !data.is_null(){
        data1 = translated_str(token, data);
    }
    if filesystem == "vfat" {
        if let Some(inode) = open_file(AT_FDCWD as i64, &target, OpenFlags::from_bits(0).unwrap()) {
            // todo()!
            return 0;    
        } else {
            return -1;
        }
    } else {
        return -1;
    }
}

/// sys_umount2 系统调用，卸载文件系统
pub fn sys_umount2(target:*const u8, flags:i32) -> isize {
    let token = current_user_token();
    let target = translated_str(token, target);
    if let Some(inode) = open_file(AT_FDCWD as i64, &target, OpenFlags::from_bits(0).unwrap()) {
        // todo()!
        return 0;    
    } else {
        return -1;
    }
}
