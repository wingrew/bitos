//! 实现 [`FrameAllocator`]，控制操作系统中的所有物理页面帧。
use super::{PhysAddr, PhysPageNum};
use crate::config::MEMORY_END;
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use core::fmt::{self, Debug, Formatter};
use lazy_static::*;

/// 物理页面帧分配和回收的追踪器
pub struct FrameTracker {
    /// 物理页面号
    pub ppn: PhysPageNum,
}

impl FrameTracker {
    /// 创建一个新的 FrameTracker
    pub fn new(ppn: PhysPageNum) -> Self {
        // 页面清理
        let bytes_array = ppn.get_bytes_array();
        for i in bytes_array {
            *i = 0;
        }
        Self { ppn }
    }
}

impl Debug for FrameTracker {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("FrameTracker:PPN={:#x}", self.ppn.0))
    }
}

impl Drop for FrameTracker {
    fn drop(&mut self) {
        // 当 FrameTracker 被销毁时，回收相应的物理页面帧
        frame_dealloc(self.ppn);
    }
}

/// 定义 FrameAllocator 特征，作为物理页面帧分配器的接口
trait FrameAllocator {
    fn new() -> Self;
    fn alloc(&mut self) -> Option<PhysPageNum>;
    fn dealloc(&mut self, ppn: PhysPageNum);
}

/// 物理页面帧分配器的栈式实现
pub struct StackFrameAllocator {
    current: usize,        // 当前分配的页面帧号
    end: usize,            // 最后一个页面帧号
    recycled: Vec<usize>,  // 回收的页面帧号列表
}

impl StackFrameAllocator {
    /// 初始化分配器，设定起始页号和结束页号
    pub fn init(&mut self, l: PhysPageNum, r: PhysPageNum) {
        self.current = l.0;
        self.end = r.0;
        // trace!("最后 {} 物理帧.", self.end - self.current);
    }
}

impl FrameAllocator for StackFrameAllocator {
    fn new() -> Self {
        Self {
            current: 0,
            end: 0,
            recycled: Vec::new(),
        }
    }

    /// 分配一个新的物理页面帧
    fn alloc(&mut self) -> Option<PhysPageNum> {
        // 如果有回收的页面帧，则直接从中取出
        if let Some(ppn) = self.recycled.pop() {
            Some(ppn.into())
        } else if self.current == self.end {
            // 如果已分配的页面帧达到结束，返回 None
            None
        } else {
            // 否则，分配一个新的页面帧
            self.current += 1;
            Some((self.current - 1).into())
        }
    }

    /// 释放一个已经分配的页面帧
    fn dealloc(&mut self, ppn: PhysPageNum) {
        let ppn = ppn.0;
        // 校验页面帧是否有效
        if ppn >= self.current || self.recycled.iter().any(|&v| v == ppn) {
            panic!("Frame ppn={:#x} 尚未分配！", ppn);
        }
        // 将页面帧加入回收列表
        self.recycled.push(ppn);
    }
}

/// FrameAllocator 的实现类型
type FrameAllocatorImpl = StackFrameAllocator;

lazy_static! {
    /// 通过 lazy_static! 实现的全局 FrameAllocator 实例
    pub static ref FRAME_ALLOCATOR: UPSafeCell<FrameAllocatorImpl> =
        unsafe { UPSafeCell::new(FrameAllocatorImpl::new()) };
}

/// 初始化页面帧分配器，使用 `ekernel` 和 `MEMORY_END` 作为起始和结束地址
pub fn init_frame_allocator() {
    extern "C" {
        fn ekernel();
    }
    FRAME_ALLOCATOR.exclusive_access().init(
        PhysAddr::from(ekernel as usize).ceil(),
        PhysAddr::from(MEMORY_END).floor(),
    );
}

/// 分配一个物理页面帧，返回 FrameTracker 样式的分配器
pub fn frame_alloc() -> Option<FrameTracker> {
    FRAME_ALLOCATOR
        .exclusive_access()
        .alloc()
        .map(FrameTracker::new)
}

/// 释放一个指定的物理页面帧
pub fn frame_dealloc(ppn: PhysPageNum) {
    FRAME_ALLOCATOR.exclusive_access().dealloc(ppn);
}
