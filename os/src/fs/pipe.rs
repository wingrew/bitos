use alloc::{sync::Weak, sync::Arc};
use spin::Mutex;
use crate::{mm::UserBuffer, task::suspend_current_and_run_next};
use super::File;

// 定义环形缓冲区的大小
const RING_BUFFER_SIZE: usize = 32;

// 当前环形缓冲区的状态
#[derive(Copy, Clone, PartialEq)]
enum RingBufferStatus {
    FULL,   // 缓冲区已满
    EMPTY,  // 缓冲区为空
    NORMAL, // 缓冲区正常
}

/// 管道环形缓冲区
pub struct PipeRingBuffer {
    arr: [u8; RING_BUFFER_SIZE], // 环形缓冲区存储空间
    head: usize,  // 读指针
    tail: usize,  // 写指针
    status: RingBufferStatus,  // 当前状态
    write_end: Option<Weak<Pipe>>,  // 写端 (弱引用)
}

// 管道结构体
pub struct Pipe{
    readable: bool,  // 是否可读
    writable: bool,  // 是否可写
    buffer:Arc<Mutex<PipeRingBuffer>>,  // 环形缓冲区
}

impl PipeRingBuffer {
    // 创建新的空环形缓冲区
    pub fn new() -> Self {
        Self {
            arr: [0; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            status: RingBufferStatus::EMPTY,
            write_end: None,
        }
    }
}

impl PipeRingBuffer {
    // 设置写端
    pub fn set_write_end(&mut self, write_end: &Arc<Pipe>) {
        self.write_end = Some(Arc::downgrade(write_end));
    }

    // 读取一个字节
    pub fn read_byte(&mut self) -> u8 {
        self.status = RingBufferStatus::NORMAL;
        let c = self.arr[self.head];
        self.head = (self.head + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::EMPTY;
        }
        c
    }

    // 写入一个字节
    pub fn write_byte(&mut self, byte: u8) -> bool{
        self.status = RingBufferStatus::NORMAL;
        self.arr[self.tail] = byte;
        self.tail = (self.tail + 1) % RING_BUFFER_SIZE;
        if self.head == self.tail {
            self.status = RingBufferStatus::FULL;
            return false; // 缓冲区已满，不能继续写入
        } else {
            return true; // 写入成功
        }
    }

    // 获取可读取的字节数
    pub fn available_read(&self) -> usize {
        if self.status == RingBufferStatus::EMPTY {
            0
        } else {
            if self.tail > self.head {
                self.tail - self.head
            } else {
                self.tail + RING_BUFFER_SIZE - self.head
            }
        }
    }

    // 获取可写入的字节数
    pub fn available_write(&self) -> usize {
        if self.status == RingBufferStatus::FULL {
            0
        } else {
            if self.tail >= self.head {
                self.head + RING_BUFFER_SIZE - self.tail
            } else {
                self.head - self.tail
            }
        }
    }

    // 检查是否所有写端都已关闭
    pub fn all_write_ends_closed(&self) -> bool {
        self.write_end.as_ref().unwrap().upgrade().is_none()
    }
}

impl Pipe {
    // 创建读端
    pub fn read_end_with_buffer(buffer: Arc<Mutex<PipeRingBuffer>>) -> Self {
        Self {
            readable: true,
            writable: false,
            buffer,
        }
    }

    // 创建写端
    pub fn write_end_with_buffer(buffer: Arc<Mutex<PipeRingBuffer>>) -> Self {
        Self {
            readable: false,
            writable: true,
            buffer,
        }
    }
}

/// 创建管道，返回读端和写端
pub fn make_pipe() -> (Arc<Pipe>, Arc<Pipe>) {
    let buffer = Arc::new(Mutex::new(PipeRingBuffer::new()));
    let read_end = Arc::new(
        Pipe::read_end_with_buffer(buffer.clone())
    );
    let write_end = Arc::new(
        Pipe::write_end_with_buffer(buffer.clone())
    );
    buffer.lock().set_write_end(&write_end); // 设置写端
    (read_end, write_end)
}

impl File for Pipe {
    // 通过管道读取数据
    fn read(&self, buf: UserBuffer) -> usize {
        assert_eq!(self.readable, true);
        let mut buf_iter = buf.into_iter();
        let mut read_size = 0usize;
        loop {
            let mut ring_buffer = self.buffer.lock();
            let loop_read = ring_buffer.available_read();
            if loop_read == 0 {
                // 如果没有可读字节且所有写端都已关闭，返回读取的字节数
                if ring_buffer.all_write_ends_closed() {
                    return read_size;
                }
                drop(ring_buffer);
                suspend_current_and_run_next(); // 当前任务挂起，切换到下一个任务
                continue;
            }
            // 读取最多 loop_read 字节
            for _ in 0..loop_read {
                if let Some(byte_ref) = buf_iter.next() {
                    unsafe { *byte_ref = ring_buffer.read_byte(); }
                    read_size += 1;
                } else {
                    return read_size;
                }
            }
        }
    }

    // 通过管道写入数据
    fn write(&self, buf: UserBuffer) -> usize {
        assert_eq!(self.writable, true);
        let mut buf_iter = buf.into_iter();
        let mut write_size = 0usize;
        loop {
            let mut ring_buffer = self.buffer.lock();
            let loop_write = ring_buffer.available_write();
            if loop_write == 0 {
                drop(ring_buffer);
                suspend_current_and_run_next(); // 当前任务挂起，切换到下一个任务
                continue;
            }

            // 写入最多 loop_write 字节
            for _ in 0..loop_write {
                if let Some(byte_ref) = buf_iter.next() {
                    unsafe { ring_buffer.write_byte(*byte_ref); }
                    write_size += 1;
                } else {
                    return write_size;
                }
            }
        }
    }

    // 判断是否可读
    fn readable(&self) -> bool {
        self.readable
    }

    // 判断是否可写
    fn writable(&self) -> bool {
        self.writable
    }
}
