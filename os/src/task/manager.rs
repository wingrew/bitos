//! [`TaskManager`] 的实现
//!
//! 实现任务管理器，用于管理任务的调度和运行。

use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;

/// 一个线程安全的 `TaskControlBlock` 队列
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>, // 就绪队列，存储任务的控制块
}

/// 一个简单的 FIFO 调度器
impl TaskManager {
    /// 创建一个空的 `TaskManager`
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// 将任务添加回就绪队列
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task); // 将任务加入队列尾部
    }
    /// 从就绪队列中取出一个任务
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        let mut id = 0; // 初始化最小 stride 的任务索引
        let inner1 = self.ready_queue.get(0).unwrap().inner_exclusive_access();
        let mut stride = inner1.stride; // 记录第一个任务的 stride 值
        drop(inner1); // 手动释放锁
        for (i, task) in self.ready_queue.iter_mut().enumerate() {
            // 遍历队列中的任务
            let inner = task.inner_exclusive_access();
            if inner.stride <= stride {
                // 找到 stride 最小的任务
                id = i;
                stride = inner.stride;
            }
            drop(inner); // 释放锁
        }
        self.ready_queue.remove(id) // 移除并返回 stride 最小的任务
        // 如果使用 FIFO 调度，可以直接替换为以下代码：
        // self.ready_queue.pop_front()
    }
}

lazy_static! {
    /// 全局唯一的 `TASK_MANAGER` 实例，通过 lazy_static 实现
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// 将任务添加到就绪队列中
pub fn add_task(task: Arc<TaskControlBlock>) {
    // trace!("kernel: TaskManager::add_task"); // 调试日志
    TASK_MANAGER.exclusive_access().add(task); // 调用 TaskManager 的 add 方法
}

/// 从就绪队列中取出一个任务
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    // trace!("kernel: TaskManager::fetch_task"); // 调试日志
    TASK_MANAGER.exclusive_access().fetch() // 调用 TaskManager 的 fetch 方法
}
