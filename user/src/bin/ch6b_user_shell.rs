#![no_std]
#![no_main]
extern crate alloc;
#[macro_use]
extern crate user_lib;
const LF: u8 = 0x0au8;
const CR: u8 = 0x0du8;
const DL: u8 = 0x7fu8;
const BS: u8 = 0x08u8;

use alloc::string::String;
use user_lib::console::getchar;
use user_lib::{exec, flush, fork, getpwd, shutdown, waitpid};
const SIZE: usize = 60;
const APP:[&str; 33] = ["brk\0", "chdir\0", "clone\0", "close\0", "dup\0", "dup2\0", "execve\0", "exit\0",
                        "fork\0", "fstat\0", "getcwd\0", "getdents\0", "getpid\0", "getppid\0", "gettimeofday\0",
                        "mkdir_\0", "open\0", "openat\0", "pipe\0", "read\0", "sleep\0", "test_echo\0", "times\0", "uname\0",
                        "unlink\0", "wait\0", "waitpid\0", "write\0", "yield\0", "mount\0", "umount\0", "mmap\0", "munmap\0"];
#[no_mangle]
pub fn main() -> i32 {

    println!("Rust user shell");
    let mut line: String = String::new();
    let mut buf:String = String::new();
    getpwd(&mut buf, SIZE as u32);
    flush();
    for app in APP.iter() {
        let pid = fork();
        if pid == 0 {
            // child process
            if exec(app, &[0 as *const u8]) == -1 {
                println!("Error when executing!");
                return -4;
            }
            unreachable!();
        } else {
            let mut exit_code: i32 = 0;
            let exit_pid = waitpid(pid as usize, &mut exit_code);
            assert_eq!(pid, exit_pid);
            println!("Shell: Process {} exited with code {}", pid, exit_code);
        }
    }
    print!("\nPS HXH:{}>$", buf);
    flush();
    loop {
        let c = getchar();
        match c {
            LF | CR => {
                print!("\n");
                if !line.is_empty() {
                    line.push('\0');
                    let pid = fork();
                    if pid == 0 {
                        // child process
                        if exec(line.as_str(), &[0 as *const u8]) == -1 {
                            println!("Error when executing!");
                            return -4;
                        }
                        unreachable!();
                    } else {
                        let mut exit_code: i32 = 0;
                        let exit_pid = waitpid(pid as usize, &mut exit_code);
                        assert_eq!(pid, exit_pid);
                        println!("Shell: Process {} exited with code {}", pid, exit_code);
                    }
                    line.clear();
                }
                getpwd(&mut buf, SIZE as u32);
                print!("PS HXH:{}>$", buf);
                flush();
            }
            BS | DL => {
                if !line.is_empty() {
                    print!("{}", BS as char);
                    print!(" ");
                    print!("{}", BS as char);
                    flush();
                    line.pop();
                }
            }
            _ => {
                print!("{}", c as char);
                flush();
                line.push(c as char);
            }
        }
    }
}
