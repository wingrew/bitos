//! virtio_blk 设备驱动

mod virtio_blk;

pub use virtio_blk::VirtIOBlock;

use alloc::sync::Arc;
use fat32::BlockDevice;
use lazy_static::*;

/// 定义 BlockDeviceImpl 类型为 virtio_blk::VirtIOBlock
type BlockDeviceImpl = virtio_blk::VirtIOBlock;

lazy_static! {
    /// 使用 lazy_static 创建一个全局的块设备驱动实例: BLOCK_DEVICE，它实现了 BlockDevice 特性
    pub static ref BLOCK_DEVICE: Arc<dyn BlockDevice> = Arc::new(BlockDeviceImpl::new());
}

#[allow(unused)]
/// 测试块设备的功能
pub fn block_device_test() {
    let block_device = BLOCK_DEVICE.clone();  // 克隆 BLOCK_DEVICE 实例
    let mut write_buffer = [0u8; 512];        // 写入缓冲区，大小为 512 字节
    let mut read_buffer = [0u8; 512];         // 读取缓冲区，大小为 512 字节
    
    // 循环测试每个块（共512个块）
    for i in 0..512 {
        // 填充写入缓冲区
        for byte in write_buffer.iter_mut() {
            *byte = i as u8;  // 填充当前块的内容
        }
        
        // 写入当前块
        block_device.write_block(i as usize, &write_buffer);
        
        // 从当前块读取数据
        block_device.read_block(i as usize, &mut read_buffer);
        
        // 校验写入的数据与读取的数据是否一致
        assert_eq!(write_buffer, read_buffer);
    }
    
    // 如果测试通过，输出成功信息
    println!("block device test passed!");
}
