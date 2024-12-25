//! 全局分配器
use crate::config::KERNEL_HEAP_SIZE;
use buddy_system_allocator::LockedHeap;

#[global_allocator]
/// 堆分配器实例
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
/// 堆内存分配错误时触发 panic
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("堆内存分配错误，布局 = {:?}", layout);
}

/// 堆空间，大小为 KERNEL_HEAP_SIZE 的字节数组
static mut HEAP_SPACE: [u8; KERNEL_HEAP_SIZE] = [0; KERNEL_HEAP_SIZE];

/// 初始化堆分配器
pub fn init_heap() {
    unsafe {
        // 锁定堆分配器并初始化堆空间
        HEAP_ALLOCATOR
            .lock()
            .init(HEAP_SPACE.as_ptr() as usize, KERNEL_HEAP_SIZE);
    }
}

