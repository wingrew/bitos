//! Implementation of syscalls
//!
//! The single entry point to all system calls, [`syscall()`], is called
//! whenever userspace wishes to perform a system call using the `ecall`
//! instruction. In this case, the processor raises an 'Environment call from
//! U-mode' exception, which is handled as one of the cases in
//! [`crate::trap::trap_handler`].
//!
//! For clarity, each single syscall is implemented as its own function, named
//! `sys_` then the name of the syscall. You can find functions like this in
//! submodules, and you should also implement syscalls this way.

/// get cwd
const SYSCALL_GETCWD: usize = 17;
// /// dup
const SYSCALL_DUP: usize = 23;
/// dup3
const SYSCALL_DUP3: usize = 24;
/// mkdir
const SYSCALL_MKDIRT: usize = 34;
/// unlinkat syscall
const SYSCALL_UNLINKAT: usize = 35;
/// linkat syscall
const SYSCALL_LINKAT: usize = 37;
/// umount2
const SYSCALL_UMOUNNT2: usize = 39;
/// mount
const SYSCALL_MOUNT: usize = 40;
/// chdir
const SYSCALL_CHDIR: usize = 49;
/// open syscall
const SYSCALL_OPEN: usize = 56;
/// close syscall
const SYSCALL_CLOSE: usize = 57;
/// pipe2
const SYSCALL_PIPE2: usize = 59;
/// getdents
const SYSCALL_GETDENTS64: usize = 61;
/// read syscall
const SYSCALL_READ: usize = 63;
/// write syscall
const SYSCALL_WRITE: usize = 64;
/// fstat syscall
const SYSCALL_FSTAT: usize = 80;
/// exit syscall
const SYSCALL_EXIT: usize = 93;
/// nanosleep
const SYSCALL_NANOSLEEP: usize = 101;
/// yield syscall
const SYSCALL_YIELD: usize = 124;
/// setpriority syscall
const SYSCALL_SET_PRIORITY: usize = 140;
/// times
const SYSCALL_TIMES: usize = 153;
/// uname
const SYSCALL_UNAME: usize = 160;
/// gettime syscall
const SYSCALL_GET_TIME: usize = 169;
/// getpid syscall
const SYSCALL_GETPID: usize = 172;
/// getppid
const SYSCALL_GETPPID: usize = 173;
/// sbrk syscall
const SYSCALL_BRK: usize = 214;
/// munmap syscall
const SYSCALL_MUNMAP: usize = 215;
/// fork syscall
const SYSCALL_FORK: usize = 220;
/// exec syscall
const SYSCALL_EXEC: usize = 221;
/// mmap syscall
const SYSCALL_MMAP: usize = 222;
/// waitpid syscall
const SYSCALL_WAITPID: usize = 260;
/// spawn syscall
const SYSCALL_SPAWN: usize = 400;
/// taskinfo syscall
const SYSCALL_TASK_INFO: usize = 410;
/// fs
pub const AT_FDCWD: isize = -100;
/// shutdown
pub const SYSCALL_SHUTDOWN: usize = 210;
mod fs;
mod process;
use fat32::ATTRIBUTE_DIRECTORY;
use fs::*;
use process::*;

use crate::{task::processor::update_time, timer::get_time};

/// handle syscall exception with `syscall_id` and other arguments
pub fn syscall(syscall_id: usize, args: [usize; 6]) -> isize {
    let ms = get_time();
    let result = match syscall_id {
        SYSCALL_OPEN => sys_openat(args[0] as i64, args[1] as *const u8, args[2] as u32),
        SYSCALL_CLOSE => sys_close(args[0]),
        SYSCALL_DUP => sys_dup(args[0]),
        SYSCALL_DUP3 => sys_dup3(args[0], args[1]),
        // SYSCALL_LINKAT => sys_linkat(args[1] as *const u8, args[3] as *const u8),
        SYSCALL_READ => sys_read(args[0], args[1] as *const u8, args[2]),
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_FORK => sys_fork(args[0], args[1], args[2], args[3], args[4]),
        SYSCALL_EXEC => sys_exec(args[0] as *const u8),
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1] as *mut i32, args[2] as isize),
        SYSCALL_GET_TIME => sys_get_time(args[0] as *mut TimeVal, args[1]),
        SYSCALL_MMAP => sys_mmap(args[0] as usize, args[1] as usize, args[2] as usize, args[3] as i32, args[4] as i32, args[5] as i32),
        SYSCALL_MUNMAP => sys_munmap(args[0], args[1]),
        SYSCALL_BRK => sys_brk(args[0] as *const i64),
        SYSCALL_SPAWN => sys_spawn(args[0] as *const u8),
        SYSCALL_SET_PRIORITY => sys_set_priority(args[0] as isize),
        SYSCALL_GETCWD => sys_getcwd(args[0] as *mut u8, args[1] as u32),
        SYSCALL_MKDIRT => sys_mkdirat(args[0] as i64, args[1] as *const u8, ATTRIBUTE_DIRECTORY),
        SYSCALL_CHDIR => sys_chdir(args[0] as *const u8),
        SYSCALL_PIPE2 => sys_pipe2(args[0] as *mut u32),
        SYSCALL_GETPPID => sys_getppid(),
        SYSCALL_NANOSLEEP => sys_nanosleep(args[0] as *mut TimeVal, args[1] as *mut TimeVal),
        SYSCALL_TIMES => sys_times(args[0] as *mut u64, ms),
        SYSCALL_FSTAT => sys_fstat(args[0] as usize, args[1] as *mut u8),
        SYSCALL_UNLINKAT => sys_unlink(args[0] as i32, args[1] as *const u8),
        SYSCALL_UNAME => sys_uname(args[0] as *mut u8),
        SYSCALL_GETDENTS64 => sys_getdents64(args[0] as usize, args[1] as *mut u8, args[2] as usize),
        SYSCALL_SHUTDOWN => sys_shutdown(),
        SYSCALL_MOUNT => sys_mount(args[0] as *const u8, args[1] as *const u8, args[2] as *const u8, args[3] as i64, args[4] as *const u8),
        SYSCALL_UMOUNNT2 => sys_umount2(args[0] as *const u8, args[1] as i32),
        _ => panic!("Unsupported syscall_id: {}", syscall_id),
    };
    let ms1 = get_time();
    update_time(ms1-ms);
    return result;
}
