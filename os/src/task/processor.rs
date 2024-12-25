// 实现 [`Processor`] 和控制流的交叉
// 在这里，维护了用户应用程序在 CPU 上的连续运行，
// 记录了 CPU 当前的运行状态，
// 并执行了不同应用程序的控制流替换和切换。

use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::mm::page_table::PTEFlags;
use crate::mm::{PhysPageNum, VirtPageNum};
use crate::sync::UPSafeCell;
use crate::timer::get_time;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;

/// 处理器管理结构
pub struct Processor {
    /// 当前处理器上正在执行的任务
    current: Option<Arc<TaskControlBlock>>,

    /// 每个核心的基本控制流，辅助选择和切换进程
    idle_task_cx: TaskContext,
}

impl Processor {
    /// 创建一个空的处理器
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    /// 获取 `idle_task_cx` 的可变引用
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    /// 获取当前任务（移动语义）
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    /// 获取当前任务（克隆语义）
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(Arc::clone)
    }
}

lazy_static! {
    /// 全局唯一的处理器实例
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

/// 进程执行与调度的核心部分
/// 循环调用 `fetch_task` 获取需要运行的进程，并通过 `__switch` 切换进程
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // 独占访问即将运行任务的 TCB
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            let ms1 = get_time();
            task_inner.task_info.start = ms1 as u64;
            // 手动释放 task_inner 的独占访问
            drop(task_inner);
            // 手动释放任务的 TCB
            task.update_stri();
            processor.current = Some(task);
            // 手动释放处理器的独占访问
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            warn!("在 run_tasks 中没有可用的任务");
        }
    }
}

/// 通过 take 获取当前任务，同时留下一个 None
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// 获取当前任务的副本
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// 获取当前用户态的 token（页表地址）
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    task.get_user_token()
}

/// 获取当前任务的 trap 上下文的可变引用
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// 更新任务的时间信息
pub fn update_time(ms: usize) {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .task_info.update_sys(ms);
}

/// 映射一页虚拟内存到物理内存
pub fn map_one(vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) -> isize {
    current_task()
        .unwrap()
        .map(vpn, ppn, flags)
}

/// 取消映射一页虚拟内存
pub fn unmap_one(vpn: VirtPageNum) -> isize {
    current_task()
        .unwrap()
        .unmap(vpn)
}

/// 返回到空闲的控制流以便进行新的调度
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
