//! 实现 [`PageTableEntry`] 和 [`PageTable`]。

use super::{frame_alloc, FrameTracker, PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use bitflags::*;

bitflags! {
    /// 页表项标志
    pub struct PTEFlags: u8 {
        const V = 1 << 0;  // 有效位
        const R = 1 << 1;  // 可读位
        const W = 1 << 2;  // 可写位
        const X = 1 << 3;  // 可执行位
        const U = 1 << 4;  // 用户态访问位
        const G = 1 << 5;  // 全局位
        const A = 1 << 6;  // 已访问位
        const D = 1 << 7;  // 已修改位
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
/// 页表项结构
pub struct PageTableEntry {
    /// 页表项的比特位
    pub bits: usize,
}

impl PageTableEntry {
    /// 创建新的页表项
    pub fn new(ppn: PhysPageNum, flags: PTEFlags) -> Self {
        PageTableEntry {
            bits: ppn.0 << 10 | flags.bits as usize,
        }
    }
    /// 创建空的页表项
    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }
    /// 从页表项获取物理页号
    pub fn ppn(&self) -> PhysPageNum {
        (self.bits >> 10 & ((1usize << 44) - 1)).into()
    }
    /// 从页表项获取标志位
    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits(self.bits as u8).unwrap()
    }
    /// 判断页表项指向的页面是否有效
    pub fn is_valid(&self) -> bool {
        (self.flags() & PTEFlags::V) != PTEFlags::empty()
    }
    /// 判断页表项指向的页面是否可读
    pub fn readable(&self) -> bool {
        (self.flags() & PTEFlags::R) != PTEFlags::empty()
    }
    /// 判断页表项指向的页面是否可写
    pub fn writable(&self) -> bool {
        (self.flags() & PTEFlags::W) != PTEFlags::empty()
    }
    /// 判断页表项指向的页面是否可执行
    pub fn executable(&self) -> bool {
        (self.flags() & PTEFlags::X) != PTEFlags::empty()
    }
}

/// 页表结构
pub struct PageTable {
    root_ppn: PhysPageNum,      // 根物理页号
    frames: Vec<FrameTracker>, // 页框的跟踪器
}

/// 假设创建/映射时不会发生内存不足。
impl PageTable {
    /// 创建新的页表
    pub fn new() -> Self {
        let frame = frame_alloc().unwrap();
        PageTable {
            root_ppn: frame.ppn,
            frames: vec![frame],
        }
    }
    /// 用于从用户空间获取参数
    pub fn from_token(satp: usize) -> Self {
        Self {
            root_ppn: PhysPageNum::from(satp & ((1usize << 44) - 1)),
            frames: Vec::new(),
        }
    }
    /// 根据虚拟页号查找页表项，如果不存在则为4KB页表创建一个框架
    fn find_pte_create(&mut self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                let frame = frame_alloc().unwrap();
                *pte = PageTableEntry::new(frame.ppn, PTEFlags::V);
                self.frames.push(frame);
            }
            ppn = pte.ppn();
        }
        result
    }
    /// 根据虚拟页号查找页表项
    fn find_pte(&self, vpn: VirtPageNum) -> Option<&mut PageTableEntry> {
        let idxs = vpn.indexes();
        let mut ppn = self.root_ppn;
        let mut result: Option<&mut PageTableEntry> = None;
        for (i, idx) in idxs.iter().enumerate() {
            let pte = &mut ppn.get_pte_array()[*idx];
            if i == 2 {
                result = Some(pte);
                break;
            }
            if !pte.is_valid() {
                return None;
            }
            ppn = pte.ppn();
        }
        result
    }
    /// 设置虚拟页号与物理页号之间的映射
    #[allow(unused)]
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) {
        let pte = self.find_pte_create(vpn).unwrap();
        assert!(!pte.is_valid(), "vpn {:?} 在映射之前已经映射", vpn);
        *pte = PageTableEntry::new(ppn, flags | PTEFlags::V);
    }
    /// 移除虚拟页号与物理页号之间的映射
    #[allow(unused)]
    pub fn unmap(&mut self, vpn: VirtPageNum) {
        let pte = self.find_pte(vpn).unwrap();
        assert!(pte.is_valid(), "vpn {:?} 在取消映射之前无效", vpn);
        *pte = PageTableEntry::empty();
    }
    /// 从虚拟页号获取页表项
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.find_pte(vpn).map(|pte| *pte)
    }
    /// 从虚拟地址获取物理地址
    pub fn translate_va(&self, va: VirtAddr) -> Option<PhysAddr> {
        self.find_pte(va.clone().floor()).map(|pte| {
            let aligned_pa: PhysAddr = pte.ppn().into();
            let offset = va.page_offset();
            let aligned_pa_usize: usize = aligned_pa.into();
            (aligned_pa_usize + offset).into()
        })
    }
    /// 从页表获取 token
    pub fn token(&self) -> usize {
        8usize << 60 | self.root_ppn.0
    }
}

/// 通过页表将一个 `ptr[u8]` 数组（长度为 `len`）翻译并复制到一个可变的 `u8` 向量
pub fn translated_byte_buffer(token: usize, ptr: *const u8, len: usize) -> Vec<&'static mut [u8]> {
    let page_table = PageTable::from_token(token);
    let mut start = ptr as usize;
    let end = start + len;
    let mut v = Vec::new();
    while start < end {
        let start_va = VirtAddr::from(start);
        let mut vpn = start_va.floor();
        let ppn = page_table.translate(vpn).unwrap().ppn();
        vpn.step();
        let mut end_va: VirtAddr = vpn.into();
        end_va = end_va.min(VirtAddr::from(end));
        if end_va.page_offset() == 0 {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..]);
        } else {
            v.push(&mut ppn.get_bytes_array()[start_va.page_offset()..end_va.page_offset()]);
        }
        start = end_va.into();
    }
    v
}

/// 通过页表将一个以 `\0` 结尾的 `ptr[u8]` 数组翻译为一个 `String`
pub fn translated_str(token: usize, ptr: *const u8) -> String {
    let page_table = PageTable::from_token(token);
    let mut string = String::new();
    let mut va = ptr as usize;
    loop {
        let ch: u8 = *(page_table
            .translate_va(VirtAddr::from(va))
            .unwrap()
            .get_mut());
        if ch == 0 {
            break;
        }
        string.push(ch as char);
        va += 1;
    }
    string
}

#[allow(unused)]
/// 通过页表将一个 `ptr[u8]` 数组翻译为 `T` 类型的引用
pub fn translated_ref<T>(token: usize, ptr: *const T) -> &'static T {
    let page_table = PageTable::from_token(token);
    page_table
        .translate_va(VirtAddr::from(ptr as usize))
        .unwrap()
        .get_ref()
}
/// 通过页表将一个 `ptr[u8]` 数组翻译为 `T` 类型的可变引用
pub fn translated_refmut<T>(token: usize, ptr: *mut T) -> &'static mut T {
    let page_table = PageTable::from_token(token);
    let va = ptr as usize;
    
    page_table
        .translate_va(VirtAddr::from(va))
        .unwrap()
        .get_mut()
}

/// 一个抽象结构，用于表示从用户空间传递到内核空间的缓冲区
pub struct UserBuffer {
    /// 缓冲区的列表
    pub buffers: Vec<&'static mut [u8]>,
}

impl UserBuffer {
    /// 构造一个 UserBuffer 实例
    ///
    /// # 参数
    /// - `buffers`: 一个包含多个缓冲区的向量
    pub fn new(buffers: Vec<&'static mut [u8]>) -> Self {
        Self { buffers }
    }

    /// 获取缓冲区的总长度
    ///
    /// # 返回值
    /// 缓冲区中所有字节的总长度
    pub fn len(&self) -> usize {
        let mut total: usize = 0;
        for b in self.buffers.iter() {
            total += b.len();
        }
        total
    }
}

impl IntoIterator for UserBuffer {
    type Item = *mut u8;
    type IntoIter = UserBufferIterator;

    /// 将 `UserBuffer` 转换为迭代器
    fn into_iter(self) -> Self::IntoIter {
        UserBufferIterator {
            buffers: self.buffers,
            current_buffer: 0,
            current_idx: 0,
        }
    }
}

/// UserBuffer 的迭代器
pub struct UserBufferIterator {
    /// 缓冲区的列表
    buffers: Vec<&'static mut [u8]>,
    /// 当前正在迭代的缓冲区索引
    current_buffer: usize,
    /// 当前缓冲区内的偏移量
    current_idx: usize,
}

impl Iterator for UserBufferIterator {
    type Item = *mut u8;

    /// 获取下一个缓冲区中的指针
    ///
    /// # 返回值
    /// 如果有下一个元素，则返回指向缓冲区内容的可变指针；否则返回 `None`
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_buffer >= self.buffers.len() {
            // 如果当前缓冲区索引超出范围，则迭代结束
            None
        } else {
            // 获取当前缓冲区内的指针
            let r = &mut self.buffers[self.current_buffer][self.current_idx] as *mut _;
            if self.current_idx + 1 == self.buffers[self.current_buffer].len() {
                // 如果当前缓冲区已完全迭代，则移动到下一个缓冲区
                self.current_idx = 0;
                self.current_buffer += 1;
            } else {
                // 否则继续迭代当前缓冲区
                self.current_idx += 1;
            }
            Some(r)
        }
    }
}
