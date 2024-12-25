//! 物理地址和虚拟地址及页号的实现
use super::PageTableEntry;
use crate::config::{PAGE_SIZE, PAGE_SIZE_BITS};
use core::fmt::{self, Debug, Formatter};

const PA_WIDTH_SV39: usize = 56;  // 物理地址位宽
const VA_WIDTH_SV39: usize = 39;  // 虚拟地址位宽
const PPN_WIDTH_SV39: usize = PA_WIDTH_SV39 - PAGE_SIZE_BITS;  // 物理页号位宽
const VPN_WIDTH_SV39: usize = VA_WIDTH_SV39 - PAGE_SIZE_BITS;  // 虚拟页号位宽

/// 物理地址结构体
#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PhysAddr(pub usize);

/// 虚拟地址结构体
#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct VirtAddr(pub usize);

/// 物理页号（PPN）结构体
#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct PhysPageNum(pub usize);

/// 虚拟页号（VPN）结构体
#[repr(C)]
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct VirtPageNum(pub usize);

/// 调试输出实现

impl Debug for VirtAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("VA:{:#x}", self.0))  // 格式化虚拟地址输出
    }
}
impl Debug for VirtPageNum {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("VPN:{:#x}", self.0))  // 格式化虚拟页号输出
    }
}
impl Debug for PhysAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("PA:{:#x}", self.0))  // 格式化物理地址输出
    }
}
impl Debug for PhysPageNum {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("PPN:{:#x}", self.0))  // 格式化物理页号输出
    }
}

/// 从 usize 转换为物理地址、虚拟地址和页号
impl From<usize> for PhysAddr {
    fn from(v: usize) -> Self {
        Self(v & ((1 << PA_WIDTH_SV39) - 1))  // 保留物理地址的低 PA_WIDTH_SV39 位
    }
}
impl From<usize> for PhysPageNum {
    fn from(v: usize) -> Self {
        Self(v & ((1 << PPN_WIDTH_SV39) - 1))  // 保留物理页号的低 PPN_WIDTH_SV39 位
    }
}
impl From<usize> for VirtAddr {
    fn from(v: usize) -> Self {
        Self(v & ((1 << VA_WIDTH_SV39) - 1))  // 保留虚拟地址的低 VA_WIDTH_SV39 位
    }
}
impl From<usize> for VirtPageNum {
    fn from(v: usize) -> Self {
        Self(v & ((1 << VPN_WIDTH_SV39) - 1))  // 保留虚拟页号的低 VPN_WIDTH_SV39 位
    }
}
impl From<PhysAddr> for usize {
    fn from(v: PhysAddr) -> Self {
        v.0  // 从物理地址中提取 usize 类型
    }
}
impl From<PhysPageNum> for usize {
    fn from(v: PhysPageNum) -> Self {
        v.0  // 从物理页号中提取 usize 类型
    }
}
impl From<VirtAddr> for usize {
    fn from(v: VirtAddr) -> Self {
        if v.0 >= (1 << (VA_WIDTH_SV39 - 1)) {
            v.0 | (!((1 << VA_WIDTH_SV39) - 1))  // 如果虚拟地址大于等于 2^(VA_WIDTH_SV39-1)，扩展符号位
        } else {
            v.0  // 否则返回虚拟地址
        }
    }
}
impl From<VirtPageNum> for usize {
    fn from(v: VirtPageNum) -> Self {
        v.0  // 从虚拟页号中提取 usize 类型
    }
}

/// 虚拟地址相关实现
impl VirtAddr {
    /// 获取虚拟地址对应的页号（下取整）
    pub fn floor(&self) -> VirtPageNum {
        VirtPageNum(self.0 / PAGE_SIZE)
    }

    /// 获取虚拟地址对应的页号（上取整）
    pub fn ceil(&self) -> VirtPageNum {
        VirtPageNum((self.0 - 1 + PAGE_SIZE) / PAGE_SIZE)
    }

    /// 获取虚拟地址的页内偏移
    pub fn page_offset(&self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    /// 检查虚拟地址是否按照页大小对齐
    pub fn aligned(&self) -> bool {
        self.page_offset() == 0
    }
}
impl From<VirtAddr> for VirtPageNum {
    fn from(v: VirtAddr) -> Self {
        assert_eq!(v.page_offset(), 0);  // 确保虚拟地址页内偏移为 0
        v.floor()
    }
}
impl From<VirtPageNum> for VirtAddr {
    fn from(v: VirtPageNum) -> Self {
        Self(v.0 << PAGE_SIZE_BITS)  // 根据虚拟页号和页大小转换为虚拟地址
    }
}
impl PhysAddr {
    /// 获取物理地址对应的页号（下取整）
    pub fn floor(&self) -> PhysPageNum {
        PhysPageNum(self.0 / PAGE_SIZE)
    }

    /// 获取物理地址对应的页号（上取整）
    pub fn ceil(&self) -> PhysPageNum {
        PhysPageNum((self.0 - 1 + PAGE_SIZE) / PAGE_SIZE)
    }

    /// 获取物理地址的页内偏移
    pub fn page_offset(&self) -> usize {
        self.0 & (PAGE_SIZE - 1)
    }

    /// 检查物理地址是否按照页大小对齐
    pub fn aligned(&self) -> bool {
        self.page_offset() == 0
    }
}
impl From<PhysAddr> for PhysPageNum {
    fn from(v: PhysAddr) -> Self {
        assert_eq!(v.page_offset(), 0);  // 确保物理地址页内偏移为 0
        v.floor()
    }
}
impl From<PhysPageNum> for PhysAddr {
    fn from(v: PhysPageNum) -> Self {
        Self(v.0 << PAGE_SIZE_BITS)  // 根据物理页号和页大小转换为物理地址
    }
}

/// 虚拟页号相关实现
impl VirtPageNum {
    /// 获取虚拟页号在页表中的索引
    pub fn indexes(&self) -> [usize; 3] {
        let mut vpn = self.0;
        let mut idx = [0usize; 3];
        for i in (0..3).rev() {
            idx[i] = vpn & 511;  // 每 9 位为一个索引，计算索引
            vpn >>= 9;
        }
        idx
    }
}

impl PhysAddr {
    /// 获取物理地址的不可变引用
    pub fn get_ref<T>(&self) -> &'static T {
        unsafe { (self.0 as *const T).as_ref().unwrap() }  // 获取物理地址的引用
    }

    /// 获取物理地址的可变引用
    pub fn get_mut<T>(&self) -> &'static mut T {
        unsafe { (self.0 as *mut T).as_mut().unwrap() }  // 获取物理地址的可变引用
    }
}
impl PhysPageNum {
    /// 获取页表条目数组的引用
    pub fn get_pte_array(&self) -> &'static mut [PageTableEntry] {
        let pa: PhysAddr = (*self).into();
        unsafe { core::slice::from_raw_parts_mut(pa.0 as *mut PageTableEntry, 512) }  // 获取物理页号对应的页表
    }

    /// 获取页的字节数组的引用
    pub fn get_bytes_array(&self) -> &'static mut [u8] {
        let pa: PhysAddr = (*self).into();
        unsafe { core::slice::from_raw_parts_mut(pa.0 as *mut u8, 4096) }  // 获取物理页对应的字节数组
    }

    /// 获取物理地址的可变引用
    pub fn get_mut<T>(&self) -> &'static mut T {
        let pa: PhysAddr = (*self).into();
        pa.get_mut()  // 获取物理地址的可变引用
    }
}

/// 用于遍历物理页号/虚拟页号的迭代器
pub trait StepByOne {
    /// 逐步增加一个元素（页号）
    fn step(&mut self);
}
impl StepByOne for VirtPageNum {
    fn step(&mut self) {
        self.0 += 1;  // 增加虚拟页号
    }
}
impl StepByOne for PhysPageNum {
    fn step(&mut self) {
        self.0 += 1;  // 增加物理页号
    }
}

#[derive(Copy, Clone)]
/// 一个简单的范围结构体，适用于类型 T
pub struct SimpleRange<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    l: T,  // 范围的起始值
    r: T,  // 范围的结束值
}
impl<T> SimpleRange<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    pub fn new(start: T, end: T) -> Self {
        assert!(start <= end, "start {:?} > end {:?}!", start, end);  // 确保起始值小于等于结束值
        Self { l: start, r: end }
    }

    pub fn get_start(&self) -> T {
        self.l  // 获取范围的起始值
    }

    pub fn get_end(&self) -> T {
        self.r  // 获取范围的结束值
    }
}
impl<T> IntoIterator for SimpleRange<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    type Item = T;
    type IntoIter = SimpleRangeIterator<T>;
    fn into_iter(self) -> Self::IntoIter {
        SimpleRangeIterator::new(self.l, self.r)  // 将范围转换为迭代器
    }
}

/// 简单范围结构体的迭代器
pub struct SimpleRangeIterator<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    current: T,  // 当前值
    end: T,      // 结束值
}
impl<T> SimpleRangeIterator<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    pub fn new(l: T, r: T) -> Self {
        Self { current: l, end: r }
    }
}
impl<T> Iterator for SimpleRangeIterator<T>
where
    T: StepByOne + Copy + PartialEq + PartialOrd + Debug,
{
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.end {
            None  // 如果当前值等于结束值，停止迭代
        } else {
            let t = self.current;
            self.current.step();  // 步进到下一个元素
            Some(t)
        }
    }
}

/// 用于虚拟页号的简单范围类型
pub type VPNRange = SimpleRange<VirtPageNum>;
