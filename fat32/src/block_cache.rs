use super::{BlockDevice, BLOCK_SZ};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
use spin::RwLock;
#[allow(unused)]


pub struct BlockCache {
    pub cache: [u8; BLOCK_SZ],  // 缓存大小
    block_id: usize,  // 块号
    block_device: Arc<dyn BlockDevice>,  // 块设备
    modified: bool,   // 是否被修改
    #[allow(unused)]
    time_stamp: usize,   // 时间戳
}

// BlockCache的实现
impl BlockCache {
    /// 新建一个BlockCache
    pub fn new(block_id: usize, block_device: Arc<dyn BlockDevice>) -> Self {
        let mut cache = [0u8; BLOCK_SZ];
        block_device.read_block(block_id, &mut cache);
        let time_stamp = 0;
        Self {
            cache,
            block_id,
            block_device,
            modified: false,
            time_stamp,
        }
    }

    // 获取偏移量的地址
    fn addr_of_offset(&self, offset: usize) -> usize {
        &self.cache[offset] as *const _ as usize
    }

    // 获取引用
    pub fn get_ref<T>(&self, offset: usize) -> &T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        let addr = self.addr_of_offset(offset);
        unsafe { &*(addr as *const T) }
    }

    // 获取可变引用
    pub fn get_mut<T>(&mut self, offset: usize) -> &mut T
    where
        T: Sized,
    {
        let type_size = core::mem::size_of::<T>();
        assert!(offset + type_size <= BLOCK_SZ);
        self.modified = true;
        let addr = self.addr_of_offset(offset);
        unsafe { &mut *(addr as *mut T) }
    }

    // 读取
    pub fn read<T, V>(&self, offset: usize, f: impl FnOnce(&T) -> V) -> V {
        f(self.get_ref(offset))
    }

    // 修改
    pub fn modify<T, V>(&mut self, offset: usize, f: impl FnOnce(&mut T) -> V) -> V {
        f(self.get_mut(offset))
    }

    // 写入
    pub fn sync(&mut self) {
        if self.modified {
            self.modified = false;
            self.block_device.write_block(self.block_id, &self.cache);
        }
    }
}

impl Drop for BlockCache {
    fn drop(&mut self) {
        self.sync()
    }
}
// cache块数
const BLOCK_CACHE_SIZE: usize = 10;
// BlockCacheManager的实现
pub struct BlockCacheManager {
    start_sec: usize,  
    queue: VecDeque<(usize, Arc<RwLock<BlockCache>>)>,  // cache块队列
}

impl BlockCacheManager {
    pub fn new() -> Self {
        Self {
            start_sec: 0,
            queue: VecDeque::new(),
        }
    }

    pub fn set_start_sec(&mut self, new_start_sec: usize) {
        self.start_sec = new_start_sec;
    }

    pub fn get_start_sec(&self) -> usize {
        self.start_sec
    }

    // 读取cache块
    pub fn read_block_cache(&self, block_id: usize) -> Option<Arc<RwLock<BlockCache>>> {
        if let Some(pair) = self.queue.iter().find(|pair| pair.0 == block_id) {
            Some(Arc::clone(&pair.1))
        } else {
            None
        }
    }

    // 获取cache块
    pub fn get_block_cache(
        &mut self,
        block_id: usize,
        block_device: Arc<dyn BlockDevice>,
    ) -> Arc<RwLock<BlockCache>> {
        if let Some(pair) = self.queue.iter().find(|pair| pair.0 == block_id) {
            Arc::clone(&pair.1)
        } else {
            if self.queue.len() == BLOCK_CACHE_SIZE {
                if let Some((idx, _)) = self
                    .queue
                    .iter()
                    .enumerate()
                    .find(|(_, pair)| Arc::strong_count(&pair.1) == 1)
                {
                    self.queue.drain(idx..=idx);
                } else {
                    panic!("Run out of BlockCache!");
                }
            }
            let block_cache = Arc::new(RwLock::new(BlockCache::new(
                block_id,
                Arc::clone(&block_device),
            )));
            self.queue.push_back((block_id, Arc::clone(&block_cache)));
            block_cache
        }
    }

    pub fn drop_all(&mut self) {
        self.queue.clear();
    }
}

lazy_static! {
    pub static ref DATA_BLOCK_CACHE_MANAGER: RwLock<BlockCacheManager> =
        RwLock::new(BlockCacheManager::new());
}

lazy_static! {
    pub static ref INFO_CACHE_MANAGER: RwLock<BlockCacheManager> =
        RwLock::new(BlockCacheManager::new());
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum CacheMode {
    READ,
    WRITE,
}

// 获取数据块cache
pub fn get_block_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    rw_mode: CacheMode,
) -> Arc<RwLock<BlockCache>> {
    let phy_blk_id = DATA_BLOCK_CACHE_MANAGER.read().get_start_sec() + block_id;
    if rw_mode == CacheMode::READ {
        // make sure the blk is in cache
        DATA_BLOCK_CACHE_MANAGER
            .write()
            .get_block_cache(phy_blk_id, block_device);
        DATA_BLOCK_CACHE_MANAGER
            .read()
            .read_block_cache(phy_blk_id)
            .unwrap()
    } else {
        DATA_BLOCK_CACHE_MANAGER
            .write()
            .get_block_cache(phy_blk_id, block_device)
    }
}

// 获取信息块cache
pub fn get_info_cache(
    block_id: usize,
    block_device: Arc<dyn BlockDevice>,
    rw_mode: CacheMode,
) -> Arc<RwLock<BlockCache>> {
    let phy_blk_id = INFO_CACHE_MANAGER.read().get_start_sec() + block_id;
    if rw_mode == CacheMode::READ {
        // make sure the blk is in cache
        INFO_CACHE_MANAGER
            .write()
            .get_block_cache(phy_blk_id, block_device);
        INFO_CACHE_MANAGER
            .read()
            .read_block_cache(phy_blk_id)
            .unwrap()
    } else {
        INFO_CACHE_MANAGER
            .write()
            .get_block_cache(phy_blk_id, block_device)
    }
}

// 设置起始扇区
pub fn set_start_sec(start_sec: usize) {
    INFO_CACHE_MANAGER.write().set_start_sec(start_sec);
    DATA_BLOCK_CACHE_MANAGER.write().set_start_sec(start_sec);
}

// 写入设备
pub fn write_to_dev() {
    INFO_CACHE_MANAGER.write().drop_all();
    DATA_BLOCK_CACHE_MANAGER.write().drop_all();
}
