//! 任务 PID 实现
//!
//! 在这里为进程分配 PID。同时，根据 PID 确定应用程序内核栈的位置。

use crate::config::{KERNEL_STACK_SIZE, PAGE_SIZE, TRAMPOLINE};
use crate::mm::{MapPermission, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;

/// 回收分配器结构体，用于分配和回收 PID
pub struct RecycleAllocator {
    current: usize, // 当前可分配的最大 PID
    recycled: Vec<usize>, // 存储被回收的 PID
}

impl RecycleAllocator {
    /// 创建一个新的回收分配器
    pub fn new() -> Self {
        RecycleAllocator {
            current: 1, // 从 1 开始分配 PID
            recycled: Vec::new(),
        }
    }
    /// 分配一个新的 PID
    pub fn alloc(&mut self) -> usize {
        if let Some(id) = self.recycled.pop() {
            // 优先使用回收的 PID
            id
        } else {
            // 如果没有回收的 PID，则分配一个新的
            self.current += 1;
            self.current - 1
        }
    }
    /// 回收指定的 PID
    pub fn dealloc(&mut self, id: usize) {
        assert!(id < self.current); // 确保回收的 PID 小于当前最大 PID
        assert!(
            !self.recycled.iter().any(|i| *i == id),
            "id {} has been deallocated!",
            id
        ); // 确保回收的 PID 没有重复
        self.recycled.push(id); // 将回收的 PID 放入回收池
    }
}

lazy_static! {
    /// 全局 PID 分配器
    static ref PID_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
    /// 全局内核栈分配器
    static ref KSTACK_ALLOCATOR: UPSafeCell<RecycleAllocator> =
        unsafe { UPSafeCell::new(RecycleAllocator::new()) };
}

/// PID 抽象结构
pub struct PidHandle(pub usize);

/// 当 `PidHandle` 被释放时自动回收 PID
impl Drop for PidHandle {
    fn drop(&mut self) {
        //println!("drop pid {}", self.0);
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

/// 分配一个新的 PID
pub fn pid_alloc() -> PidHandle {
    PidHandle(PID_ALLOCATOR.exclusive_access().alloc())
}

/// 返回内核空间中内核栈的底部和顶部地址
pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    (bottom, top)
}

/// 表示进程（任务）的内核栈
pub struct KernelStack(pub usize);

/// 分配一个新的内核栈
pub fn kstack_alloc() -> KernelStack {
    let kstack_id = KSTACK_ALLOCATOR.exclusive_access().alloc();
    let (kstack_bottom, kstack_top) = kernel_stack_position(kstack_id);
    KERNEL_SPACE.exclusive_access().insert_framed_area(
        kstack_bottom.into(),
        kstack_top.into(),
        MapPermission::R | MapPermission::W, // 设置为可读写
    );
    KernelStack(kstack_id)
}

/// 当 `KernelStack` 被释放时自动回收内核栈
impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kernel_stack_bottom, _) = kernel_stack_position(self.0);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.into()); // 移除内核栈区域
        KSTACK_ALLOCATOR.exclusive_access().dealloc(self.0); // 回收内核栈 ID
    }
}

impl KernelStack {
    /// 将类型为 `T` 的变量压入内核栈顶部，并返回其原始指针
    #[allow(unused)]
    pub fn push_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        let kernel_stack_top = self.get_top(); // 获取内核栈顶部地址
        let ptr_mut = (kernel_stack_top - core::mem::size_of::<T>()) as *mut T; // 计算变量存储位置
        unsafe {
            *ptr_mut = value; // 将变量存储到内核栈中
        }
        ptr_mut // 返回指针
    }
    /// 获取内核栈的顶部地址
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.0);
        kernel_stack_top
    }
}
