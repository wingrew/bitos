//! 进程管理系统调用
//!
use alloc::sync::Arc;
use crate::{
    config::PAGE_SIZE, fs::{open_file, OpenFlags}, mm::{self, frame_alloc, page_table::PTEFlags, translated_byte_buffer, translated_ref, translated_refmut, translated_str, VPNRange, VirtAddr }, syscall::AT_FDCWD, task::{
        add_task, current_task, current_user_token, exit_current_and_run_next, processor::{map_one, unmap_one}, suspend_current_and_run_next, TaskInfo
    }, timer::{get_time, get_time_us}
};
use core::ptr::write_unaligned;

// 用于存储时间的结构体
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,  // 秒
    pub usec: usize, // 微秒
}

// 进程退出系统调用
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code); // 退出当前进程并运行下一个进程
    panic!("Unreachable in sys_exit!"); // 如果代码运行到这里，则会发生错误
}

// 进程调度让步系统调用
pub fn sys_yield() -> isize {
    suspend_current_and_run_next(); // 挂起当前进程，调度下一个进程
    0
}

// 获取当前进程的 PID 系统调用
pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

// 进程创建（fork）系统调用
pub fn sys_fork(flags:usize, stack:usize, ptid:usize, tls:usize, ctid:usize) -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork(); // 创建新进程
    let new_pid = new_task.pid.0;
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    trap_cx.x[10] = 0; // 设置系统调用的返回值
    if stack != 0{
        trap_cx.set_sp(stack); // 如果指定了栈地址，则设置栈指针
    }
    add_task(new_task); // 将新进程添加到调度队列
    new_pid as isize
}

// 进程执行（exec）系统调用
pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path); // 获取进程的路径
    if let Some(app_inode) = open_file(AT_FDCWD as i64, path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all(); // 读取文件数据
        let task = current_task().unwrap();
        task.exec(all_data.as_slice()); // 执行新程序
        0
    } else {
        -1 // 文件打开失败
    }
}

// 等待指定进程结束的系统调用
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32, options:isize) -> isize{
    loop{
        match waitpid(pid, exit_code_ptr){ // 调用等待函数
            -2 => {sys_yield();} // 如果没有找到进程，挂起当前进程
            n => {return n;} // 返回子进程的 PID 或错误码
        }
    }
}

// 等待进程结束的实现函数
pub fn waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1; // 如果没有找到指定 PID 的子进程，返回错误
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid()) // 查找已结束的子进程
    });
    
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx); // 移除子进程
        assert_eq!(Arc::strong_count(&child), 1); // 确保子进程没有其他引用
        let found_pid = child.getpid();
        let exit_code = child.inner_exclusive_access().exit_code;
        if exit_code_ptr != core::ptr::null_mut(){
            *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code << 8; // 将退出码写入用户内存
        }
        found_pid as isize
    } else {
        -2 // 如果子进程没有结束，返回 -2
    }
}

// 获取当前时间的系统调用
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let us = get_time_us(); // 获取当前时间（微秒）
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

// 内存映射系统调用
pub fn sys_mmap(_start: usize, _len: usize, _port: usize, flags:i32, fd:i32, offset:i32) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // 检查映射的起始地址和端口
    let token = current_user_token();
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    let mut start:usize = _start;
    if _start % 4096 != 0 || _port & !0x7 != 0 || _port & 0x7 == 0{
        return -1; // 地址不对齐或端口无效
    }else if _start == 0{
        start = inner.program_brk + PAGE_SIZE * 8;
    }
    let start_va = VirtAddr::from(start).floor();
    let end_va = VirtAddr::from(start + _len).ceil();
    let vir = VPNRange::new(start_va, end_va);
    let port = (_port as u8) << 5 >> 4;
    let mut flag = PTEFlags::U;
    drop(inner);
    flag |= PTEFlags::from_bits(port).unwrap();
    for vpn in vir{
        let page_table = mm::page_table::PageTable::from_token(token);
        let frame = frame_alloc().unwrap();
        let result = page_table.translate(vpn);
        match result{
            Some(pey) => {
                if !pey.is_valid(){
                    map_one(vpn, frame.ppn, flag);
                }else{
                    return -1; // 页面已存在，无法映射
                }
            },
            None => {
                map_one(vpn, frame.ppn, flag);
            },
        }
    }
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if let Some(file) = &inner.fd_table[fd as usize] {
        let osinode = file.as_osinode().unwrap();
        let vfile = osinode.inner.exclusive_access().inode.clone();
        let ts = translated_byte_buffer(token, (start_va.0 * PAGE_SIZE) as *const u8, vfile.get_size() as usize);
        let mut read = 0;
        for slice in ts{
            let len = vfile.read_at(read,slice);
            read += len;
        }
        return (start_va.0 * PAGE_SIZE) as *const u8 as isize;
    }else{
        drop(inner);
        return -1; // 文件映射失败
    }
}

// 内存解除映射系统调用
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _start % 4096 != 0{
        return -1; // 地址不对齐
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
                    return -1; // 页面无效
                }
                unmap_one(vpn); // 解除映射
            },
            None => return -1, // 未找到页面
        }
    }
    0
}

// 进程内存增长系统调用
pub fn sys_brk(size: *const i64) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(new_brk) = current_task().unwrap().change_program_brk(size as i64) {
        new_brk as isize
    } else {
        -1 // 内存增长失败
    }
}

// 启动新进程
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(app_inode) = open_file(AT_FDCWD as i64, path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        let new_task = task.spawn(all_data.as_slice()); // 启动新进程
        let new_pid = new_task.pid.0;
        add_task(new_task); // 将新进程添加到调度队列
        new_pid as isize
    } else {
        -1 // 文件打开失败
    }
}

// 设置进程优先级系统调用
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if _prio <= 1{
        return -1; // 无效的优先级值
    }
    let task = current_task().unwrap();
    task.set_priority(_prio); // 设置进程优先级
    _prio
}

// 获取父进程的 PID 系统调用
pub fn sys_getppid() -> isize{
    current_task().unwrap().ppid as isize
}

// 纳秒级睡眠系统调用
pub fn sys_nanosleep(ti:*mut TimeVal, te:*mut TimeVal) -> isize{
    let us = get_time_us(); // 获取当前时间（微秒）
    let token = current_user_token();
    let target = translated_ref(token, ti);
    let t_us = target.sec * 1_000_000 + target.usec;
    loop{
        let now = get_time_us();
        if now - us < t_us{
            suspend_current_and_run_next(); // 睡眠并让出 CPU
        }else{
            return 0; // 睡眠时间结束
        }
    }
}

// 获取进程时间信息系统调用
pub fn sys_times(time:*mut u64, ms:usize) -> isize{
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    let utime = inner.task_info.all - inner.task_info.stime;
    let mut cutime:u64 = 0;
    let mut cstime:u64 = 0;
    let ms1 = get_time() as u64;
    for child in inner.children.iter(){
        let little = child.inner_exclusive_access();
        if little.is_zombie(){
            cutime += little.task_info.cutime;
            cstime += little.task_info.cstime;
        }
    }
    *translated_refmut(token, time) = utime + ms1 - inner.task_info.start;
    *translated_refmut(token, unsafe { time.add(1) }) = inner.task_info.stime + ms1 - ms as u64;
    *translated_refmut(token, unsafe { time.add(2) }) = cutime;
    *translated_refmut(token, unsafe { time.add(3) }) = cstime;
    return inner.task_info.all as isize;
}

// 系统关闭（关机）调用
pub fn sys_shutdown() -> isize{
    crate::sbi::shutdown(); // 调用 SBI 关机接口
    0
}
