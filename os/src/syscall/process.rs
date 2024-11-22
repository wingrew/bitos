//! Process management syscalls
//!
use alloc::sync::Arc;
use crate::{
    fs::{open_file, OpenFlags},
    mm::{self, frame_alloc, page_table::PTEFlags, translated_byte_buffer, translated_refmut, translated_str, VPNRange, VirtAddr },
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next, processor::{map_one, show_info, unmap_one}, suspend_current_and_run_next, TaskInfo
    },
    timer::{get_time_ms, get_time_us},
};
use core::{mem::size_of, ptr::{copy_nonoverlapping, write_unaligned}};
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}


pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}



/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let us = get_time_us();
    let tv_sec = us / 1_000_000;
    let tv_usec = us % 1_000_000;
    let mut ts = translated_byte_buffer(current_user_token(), _ts as *const u8, core::mem::size_of::<TimeVal>());

    unsafe {
        // 获取缓冲区的原始指针
        let ptr = ts[0].as_mut_ptr() as *mut i64;

        // 将 tv_sec 写入偏移 0 的位置
        write_unaligned(ptr, tv_sec as i64);

        // 将 tv_usec 写入偏移 8 的位置
        write_unaligned(ptr.add(1), tv_usec as i64);        
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!(
        "kernel:pid[{}] sys_task_info NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let mut temp = show_info();
    temp.time = get_time_ms() - temp.time;
    let mut ti = translated_byte_buffer(current_user_token(), _ti as *const u8, core::mem::size_of::<TaskInfo>());
    let total_bytes = size_of::<TaskInfo>();
    let mut bytes_written = 0;
    for slice in ti.iter_mut(){
        let slice_len = slice.len();
        let mut offset = 0;
        while offset < slice_len && bytes_written < total_bytes{
            unsafe {
                let to_write = (total_bytes - bytes_written).min(slice_len - offset);
                let ptr = slice.as_mut_ptr().add(offset);
                let struct_ptr = &temp as *const TaskInfo as *const u8;
                copy_nonoverlapping(struct_ptr.add(bytes_written), ptr, to_write);
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

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _start % 4096 != 0 || _port & !0x7 != 0 || _port & 0x7 == 0{
        return -1;
    }

    let start_va = VirtAddr::from(_start).floor();
    let end_va = VirtAddr::from(_start + _len).ceil();
    let vir = VPNRange::new(start_va, end_va);
    let port = (_port as u8) << 5 >> 4;
    let mut flag = PTEFlags::U;
    flag |= PTEFlags::from_bits(port).unwrap();
    for vpn in vir{
        let page_table = mm::page_table::PageTable::from_token(current_user_token());
        let frame = frame_alloc().unwrap();
        let result = page_table.translate(vpn);
        match result{
            Some(pey) => {
                if !pey.is_valid(){
                    map_one(vpn, frame.ppn, flag);
                }else{
                    
                    return -1;
                }
            },
            None => {
                map_one(vpn, frame.ppn, flag);
            },
        }
    }
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _start % 4096 != 0{
        return -1;
    }
    let start_va = VirtAddr::from(_start).floor();
    let end_va = VirtAddr::from(_start + _len).ceil();
    let vir = VPNRange::new(start_va, end_va);    
    for vpn in vir{
        let page_table = mm::page_table::PageTable::from_token(current_user_token());
        let result = page_table.translate(vpn);
        match result{
            Some(pey) => {
                if !pey.is_valid(){
                    return -1;
                }
                unmap_one(vpn);
            },
            None => return -1,
        }
    }
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        let new_task = task.spawn(all_data.as_slice());
        let new_pid = new_task.pid.0;
        add_task(new_task);
        new_pid as isize
    } else {
        -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio <= 1{
        return -1;
    }
    let task = current_task().unwrap();
    task.set_priority(_prio);
    _prio

}
