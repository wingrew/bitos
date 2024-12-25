// 内存管理实现
// 针对RV64系统的SV39页表虚拟内存架构，实现了与内存管理相关的所有功能，
// 如帧分配器、页表、映射区域以及内存集。
// 每个任务或进程都有一个`memory_set`用于控制其虚拟内存。

mod address; // 地址相关模块
mod frame_allocator; // 帧分配器模块
mod heap_allocator; // 堆分配器模块
mod memory_set; // 内存集模块
pub(crate) mod page_table; // 页表模块，仅限内部访问

// 对外暴露的模块和结构
pub use address::VPNRange; // 虚拟页号范围
pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum}; // 物理地址、虚拟地址及相关工具
pub use frame_allocator::{frame_alloc, frame_dealloc, FrameTracker}; // 帧分配与释放，帧跟踪器
pub use memory_set::remap_test; // 重新映射测试
pub use memory_set::{kernel_token, MapPermission, MemorySet, KERNEL_SPACE}; // 内核标识符、映射权限、内存集、内核空间
use page_table::PTEFlags; // 页表项标志
pub use page_table::{
    translated_byte_buffer, translated_ref, translated_refmut, translated_str, PageTable,
    PageTableEntry, UserBuffer, UserBufferIterator,
}; // 页表相关操作、用户缓冲区与迭代器

/// 初始化堆分配器、帧分配器和内核空间
pub fn init() {
    heap_allocator::init_heap(); // 初始化堆分配器
    frame_allocator::init_frame_allocator(); // 初始化帧分配器
    KERNEL_SPACE.exclusive_access().activate(); // 激活内核空间
}
