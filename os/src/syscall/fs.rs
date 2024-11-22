//! File and filesystem-related syscalls

use core::mem::size_of;
use core::ptr::copy_nonoverlapping;

use crate::fs::{open_file, OpenFlags, Stat};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if _fd >= inner.fd_table.len() {
        return -1;
    }
    
    if let Some(file) = &inner.fd_table[_fd]{
        let temp = file.fstat();
        println!("{:?}", temp);
        drop(inner);
        let mut ti = translated_byte_buffer(current_user_token(), _st as *const u8, core::mem::size_of::<Stat>());
        let total_bytes = size_of::<Stat>();
        let mut bytes_written = 0;
        for slice in ti.iter_mut(){
            let slice_len = slice.len();
            let mut offset = 0;
            while offset < slice_len && bytes_written < total_bytes{
                unsafe {
                    let to_write = (total_bytes - bytes_written).min(slice_len - offset);
                    let ptr = slice.as_mut_ptr().add(offset);
                    let struct_ptr = &temp as *const Stat as *const u8;
                    copy_nonoverlapping(struct_ptr.add(bytes_written), ptr, to_write);
                }
                offset += slice_len;
                bytes_written += slice_len;
            }
            if bytes_written >= total_bytes {
                break;
            }
        }
    }
    0
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _old_name);
    let new_path = translated_str(token, _new_name);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(2).unwrap()) {
        inode.link(new_path.as_str());
    }
    0
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _name);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(2).unwrap()) {
        inode.unlink(&path);
    }
    0
}
