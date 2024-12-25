use super::BlockDevice;
use crate::mm::{
    frame_alloc, frame_dealloc, kernel_token, FrameTracker, PageTable, PhysAddr, PhysPageNum,
    StepByOne, VirtAddr,
};
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use lazy_static::*;
use virtio_drivers::{Hal, VirtIOBlk, VirtIOHeader};

/// Virtio_Block 设备中控制寄存器的基地址
#[allow(unused)]
const VIRTIO0: usize = 0x10001000;

/// VirtIOBlock 驱动程序结构体，用于处理 virtio_blk 设备
pub struct VirtIOBlock(UPSafeCell<VirtIOBlk<'static, VirtioHal>>);

lazy_static! {
    /// 队列帧的静态引用，用于存储和管理 VirtIO 队列的帧
    static ref QUEUE_FRAMES: UPSafeCell<Vec<FrameTracker>> = unsafe { UPSafeCell::new(Vec::new()) };
}

impl BlockDevice for VirtIOBlock {
    /// 从虚拟块设备读取一个块
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        self.0
            .exclusive_access()
            .read_block(block_id, buf)
            .expect("读取 VirtIOBlk 时出错");
    }

    /// 向虚拟块设备写入一个块
    fn write_block(&self, block_id: usize, buf: &[u8]) {
        self.0
            .exclusive_access()
            .write_block(block_id, buf)
            .expect("写入 VirtIOBlk 时出错");
    }
}

impl VirtIOBlock {
    #[allow(unused)]
    /// 创建一个新的 VirtIOBlock 驱动，基地址为 VIRTIO0，适用于 virtio_blk 设备
    pub fn new() -> Self {
        unsafe {
            Self(UPSafeCell::new(
                VirtIOBlk::<VirtioHal>::new(&mut *(VIRTIO0 as *mut VirtIOHeader)).unwrap(),
            ))
        }
    }
}

pub struct VirtioHal;

impl Hal for VirtioHal {
    /// 分配物理页面内存，返回分配的起始物理地址
    fn dma_alloc(pages: usize) -> usize {
        let mut ppn_base = PhysPageNum(0);
        for i in 0..pages {
            let frame = frame_alloc().unwrap();
            if i == 0 {
                ppn_base = frame.ppn; // 获取第一个页面的物理页号
            }
            assert_eq!(frame.ppn.0, ppn_base.0 + i); // 确保页面连续
            QUEUE_FRAMES.exclusive_access().push(frame); // 将帧添加到队列中
        }
        let pa: PhysAddr = ppn_base.into(); // 将物理页号转换为物理地址
        pa.0
    }

    /// 释放物理页面内存
    fn dma_dealloc(pa: usize, pages: usize) -> i32 {
        let pa = PhysAddr::from(pa); // 将地址转换为 PhysAddr 类型
        let mut ppn_base: PhysPageNum = pa.into(); // 将物理地址转换为物理页号
        for _ in 0..pages {
            frame_dealloc(ppn_base); // 释放相应的页面
            ppn_base.step(); // 移动到下一个物理页
        }
        0 // 返回 0 表示成功
    }

    /// 物理地址转虚拟地址，暂时直接返回物理地址（在某些架构中可能需要映射）
    fn phys_to_virt(addr: usize) -> usize {
        addr
    }

    /// 虚拟地址转物理地址，使用页表进行地址转换
    fn virt_to_phys(vaddr: usize) -> usize {
        PageTable::from_token(kernel_token())
            .translate_va(VirtAddr::from(vaddr)) // 将虚拟地址转换为物理地址
            .unwrap()
            .0 // 返回物理地址
    }
}
