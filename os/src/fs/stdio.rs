//! Stdin & Stdout
use super::File;
use crate::mm::UserBuffer;
use crate::sbi::console_getchar;
use crate::task::suspend_current_and_run_next;

/// 代表从控制台获取字符的 stdin 文件
pub struct Stdin;

/// 代表将字符输出到控制台的 stdout 文件
pub struct Stdout;

impl File for Stdin {
    // stdin 是可读的
    fn readable(&self) -> bool {
        true
    }

    // stdin 不是可写的
    fn writable(&self) -> bool {
        false
    }

    // 从 stdin 读取一个字符
    fn read(&self, mut user_buf: UserBuffer) -> usize {
        assert_eq!(user_buf.len(), 1);  // 确保用户缓冲区的大小为 1
        // 持续循环直到获取一个有效的字符
        let mut c: usize;
        loop {
            c = console_getchar(); // 从控制台获取字符
            if c == 0 {
                // 如果没有读取到字符，挂起当前任务并切换到下一个任务
                suspend_current_and_run_next();
                continue;
            } else {
                // 成功读取到字符，退出循环
                break;
            }
        }
        let ch = c as u8;  // 转换为 u8 字符
        unsafe {
            // 将读取到的字符写入用户缓冲区
            user_buf.buffers[0].as_mut_ptr().write_volatile(ch);
        }
        1  // 返回读取的字节数，始终是 1
    }

    // 禁止向 stdin 写入
    fn write(&self, _user_buf: UserBuffer) -> usize {
        panic!("无法向 stdin 写入数据！");
    }
}

impl File for Stdout {
    // stdout 不是可读的
    fn readable(&self) -> bool {
        false
    }

    // stdout 是可写的
    fn writable(&self) -> bool {
        true
    }

    // 禁止从 stdout 读取
    fn read(&self, _user_buf: UserBuffer) -> usize {
        panic!("无法从 stdout 读取数据！");
    }

    // 向 stdout 写入数据
    fn write(&self, user_buf: UserBuffer) -> usize {
        // 遍历用户缓冲区并打印内容
        for buffer in user_buf.buffers.iter() {
            // 将每个缓冲区的内容作为字符串输出到控制台
            print!("{}", core::str::from_utf8(*buffer).unwrap());
        }
        user_buf.len()  // 返回写入的字节数
    }
}
