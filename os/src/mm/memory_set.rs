//! [`MapArea`] 和 [`MemorySet`] 的实现
use super::{frame_alloc, FrameTracker};
use super::{PTEFlags, PageTable, PageTableEntry};
use super::{PhysAddr, PhysPageNum, VirtAddr, VirtPageNum};
use super::{StepByOne, VPNRange};
use crate::config::{MEMORY_END, MMIO, PAGE_SIZE, TRAMPOLINE, TRAP_CONTEXT_BASE, USER_STACK_SIZE};
use crate::sync::UPSafeCell;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::arch::asm;
use lazy_static::*;
use riscv::register::satp;

extern "C" {
    fn stext();
    fn etext();
    fn srodata();
    fn erodata();
    fn sdata();
    fn edata();
    fn sbss_with_stack();
    fn ebss();
    fn ekernel();
    fn strampoline();
}

lazy_static! {
    /// 内核的初始内存映射（内核地址空间）
    pub static ref KERNEL_SPACE: Arc<UPSafeCell<MemorySet>> =
        Arc::new(unsafe { UPSafeCell::new(MemorySet::new_kernel()) });
}

/// 内核令牌
pub fn kernel_token() -> usize {
    KERNEL_SPACE.exclusive_access().token()
}

/// 地址空间
pub struct MemorySet {
    page_table: PageTable,
    areas: Vec<MapArea>,
}

impl MemorySet {
    /// 创建一个新的空的 `MemorySet`。
    pub fn new_bare() -> Self {
        Self {
            page_table: PageTable::new(),
            areas: Vec::new(),
        }
    }
    /// 获取页表令牌
    pub fn token(&self) -> usize {
        self.page_table.token()
    }
    /// 假设没有冲突。
    pub fn insert_framed_area(
        &mut self,
        start_va: VirtAddr,
        end_va: VirtAddr,
        permission: MapPermission,
    ) {
        self.push(
            MapArea::new(start_va, end_va, MapType::Framed, permission),
            None,
        );
    }
    /// 移除指定起始虚拟页号的区域
    pub fn remove_area_with_start_vpn(&mut self, start_vpn: VirtPageNum) {
        if let Some((idx, area)) = self
            .areas
            .iter_mut()
            .enumerate()
            .find(|(_, area)| area.vpn_range.get_start() == start_vpn)
        {
            area.unmap(&mut self.page_table);
            self.areas.remove(idx);
        }
    }
    /// 向该 `MemorySet` 中添加一个新的 `MapArea`。
    /// 假设虚拟地址空间中没有冲突。
    fn push(&mut self, mut map_area: MapArea, data: Option<&[u8]>) {
        map_area.map(&mut self.page_table);
        if let Some(data) = data {
            map_area.copy_data(&mut self.page_table, data);
        }
        self.areas.push(map_area);
    }
    /// 提到 trampoline 不会被区域回收。
    fn map_trampoline(&mut self) {
        self.page_table.map(
            VirtAddr::from(TRAMPOLINE).into(),
            PhysAddr::from(strampoline as usize).into(),
            PTEFlags::R | PTEFlags::X,
        );
    }
    /// 不包含内核栈。
    pub fn new_kernel() -> Self {
        let mut memory_set = Self::new_bare();
        // 映射 trampoline
        memory_set.map_trampoline();
        // 映射内核段
        info!(".text [{:#x}, {:#x})", stext as usize, etext as usize);
        info!(".rodata [{:#x}, {:#x})", srodata as usize, erodata as usize);
        info!(".data [{:#x}, {:#x})", sdata as usize, edata as usize);
        info!(
            ".bss [{:#x}, {:#x})",
            sbss_with_stack as usize, ebss as usize
        );
        info!("映射 .text 段");
        memory_set.push(
            MapArea::new(
                (stext as usize).into(),
                (etext as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::X,
            ),
            None,
        );
        info!("映射 .rodata 段");
        memory_set.push(
            MapArea::new(
                (srodata as usize).into(),
                (erodata as usize).into(),
                MapType::Identical,
                MapPermission::R,
            ),
            None,
        );
        info!("映射 .data 段");
        memory_set.push(
            MapArea::new(
                (sdata as usize).into(),
                (edata as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        info!("映射 .bss 段");
        memory_set.push(
            MapArea::new(
                (sbss_with_stack as usize).into(),
                (ebss as usize).into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        info!("映射物理内存");
        memory_set.push(
            MapArea::new(
                (ekernel as usize).into(),
                MEMORY_END.into(),
                MapType::Identical,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        info!("映射内存映射寄存器");
        for pair in MMIO {
            memory_set.push(
                MapArea::new(
                    (*pair).0.into(),
                    ((*pair).0 + (*pair).1).into(),
                    MapType::Identical,
                    MapPermission::R | MapPermission::W,
                ),
                None,
            );
        }
        memory_set
    }
    /// 包含 elf 中的各个段和 trampoline、TrapContext、用户栈，
    /// 同时返回用户栈基址和入口点。
    pub fn from_elf(elf_data: &[u8]) -> (Self, usize, usize) {
        let mut memory_set = Self::new_bare();
        // 映射 trampoline
        memory_set.map_trampoline();
        // 映射 elf 的程序头，带有 U 标志
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "无效的 elf 文件！");
        let ph_count = elf_header.pt2.ph_count();
        let mut max_end_vpn = VirtPageNum(0);
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_va: VirtAddr = (ph.virtual_addr() as usize).into();
                let end_va: VirtAddr = ((ph.virtual_addr() + ph.mem_size()) as usize).into();
                let mut map_perm = MapPermission::U;
                let ph_flags = ph.flags();
                if ph_flags.is_read() {
                    map_perm |= MapPermission::R;
                }
                if ph_flags.is_write() {
                    map_perm |= MapPermission::W;
                }
                if ph_flags.is_execute() {
                    map_perm |= MapPermission::X;
                }
                let map_area = MapArea::new(start_va, end_va, MapType::Framed, map_perm);
                max_end_vpn = map_area.vpn_range.get_end();
                memory_set.push(
                    map_area,
                    Some(&elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize]),
                );
            }
        }
        // 映射用户栈，带有 U 标志
        let max_end_va: VirtAddr = max_end_vpn.into();
        let mut user_stack_bottom: usize = max_end_va.into();
        // 保护页
        user_stack_bottom += PAGE_SIZE;
        let user_stack_top = user_stack_bottom + USER_STACK_SIZE;
        memory_set.push(
            MapArea::new(
                user_stack_bottom.into(),
                user_stack_top.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        // 用于 sbrk
        memory_set.push(
            MapArea::new(
                user_stack_top.into(),
                (user_stack_top+4).into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W | MapPermission::U,
            ),
            None,
        );
        // 映射 TrapContext
        memory_set.push(
            MapArea::new(
                TRAP_CONTEXT_BASE.into(),
                TRAMPOLINE.into(),
                MapType::Framed,
                MapPermission::R | MapPermission::W,
            ),
            None,
        );
        (
            memory_set,
            user_stack_top,
            elf.header.pt2.entry_point() as usize,
        )
    }
    /// 通过复制退出进程的地址空间中的代码和数据创建新的地址空间。
    pub fn from_existed_user(user_space: &Self) -> Self {
        let mut memory_set = Self::new_bare();
        // 映射 trampoline
        memory_set.map_trampoline();
        // 复制数据段、trap_context、用户栈
        for area in user_space.areas.iter() {
            let new_area = MapArea::from_another(area);
            memory_set.push(new_area, None);
            // 从另一个空间复制数据
            for vpn in area.vpn_range {
                let src_ppn = user_space.translate(vpn).unwrap().ppn();
                let dst_ppn = memory_set.translate(vpn).unwrap().ppn();
                dst_ppn
                    .get_bytes_array()
                    .copy_from_slice(src_ppn.get_bytes_array());
            }
        }
        memory_set
    }
    /// 通过写入 satp CSR 寄存器更改页表。
    pub fn activate(&self) {
        let satp = self.page_table.token();
        unsafe {
            satp::write(satp);
            asm!("sfence.vma");
        }
    }
    /// 将虚拟页号转换为页表项
    pub fn translate(&self, vpn: VirtPageNum) -> Option<PageTableEntry> {
        self.page_table.translate(vpn)
    }

    /// 清除所有 `MapArea`
    pub fn recycle_data_pages(&mut self) {
        self.areas.clear();
    }

    /// 将区域缩小到新的结束地址
    #[allow(unused)]
    pub fn shrink_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        if let Some(area) = self
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == start.floor())
        {
            area.shrink_to(&mut self.page_table, new_end.ceil());
            true
        } else {
            false
        }
    }

    /// 将区域扩展到新的结束地址
    #[allow(unused)]
    pub fn append_to(&mut self, start: VirtAddr, new_end: VirtAddr) -> bool {
        if let Some(area) = self
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == start.floor())
        {   
            area.append_to(&mut self.page_table, new_end.ceil());
            true
        } else {
            false
        }
    }

    /// 映射
    pub fn map(&mut self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) -> isize{
        let _ = self.page_table.map(vpn, ppn, flags);
        0
    }

    /// 解除映射
    pub fn unmap(&mut self, vpn: VirtPageNum) -> isize{
        let _ = self.page_table.unmap(vpn);
        0
    }    
}

/// 映射区域结构，控制一个连续的虚拟内存区域
pub struct MapArea {
    vpn_range: VPNRange, // 虚拟页号范围
    data_frames: BTreeMap<VirtPageNum, FrameTracker>, // 存储虚拟页号到帧跟踪器的映射
    map_type: MapType, // 映射类型
    map_perm: MapPermission, // 映射权限
}

impl MapArea {
    /// 创建一个新的映射区域
    pub fn new(
        start_va: VirtAddr, // 起始虚拟地址
        end_va: VirtAddr, // 结束虚拟地址
        map_type: MapType, // 映射类型
        map_perm: MapPermission, // 映射权限
    ) -> Self {
        let start_vpn: VirtPageNum = start_va.floor(); // 起始虚拟页号
        let end_vpn: VirtPageNum = end_va.ceil(); // 结束虚拟页号
        Self {
            vpn_range: VPNRange::new(start_vpn, end_vpn), // 设置虚拟页号范围
            data_frames: BTreeMap::new(), // 初始化数据帧为空
            map_type, // 映射类型
            map_perm, // 映射权限
        }
    }

    /// 通过另一个映射区域创建新映射区域
    pub fn from_another(another: &Self) -> Self {
        Self {
            vpn_range: VPNRange::new(another.vpn_range.get_start(), another.vpn_range.get_end()), // 复制虚拟页号范围
            data_frames: BTreeMap::new(), // 数据帧为空
            map_type: another.map_type, // 映射类型
            map_perm: another.map_perm, // 映射权限
        }
    }

    /// 映射一个虚拟页号到物理页号
    pub fn map_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        let ppn: PhysPageNum;
        match self.map_type {
            MapType::Identical => {
                ppn = PhysPageNum(vpn.0); // 如果是Identical映射，则物理页号与虚拟页号相同
            }
            MapType::Framed => {
                let frame = frame_alloc().unwrap(); // 分配一个新的帧
                ppn = frame.ppn;
                self.data_frames.insert(vpn, frame); // 将虚拟页号和帧映射关系存入data_frames
            }
        }
        let pte_flags = PTEFlags::from_bits(self.map_perm.bits).unwrap(); // 获取页表项标志
        page_table.map(vpn, ppn, pte_flags); // 在页表中进行映射
    }

    /// 解除映射一个虚拟页号
    pub fn unmap_one(&mut self, page_table: &mut PageTable, vpn: VirtPageNum) {
        if self.map_type == MapType::Framed {
            self.data_frames.remove(&vpn); // 如果是Framed类型，移除数据帧
        }
        page_table.unmap(vpn); // 解除页表中的映射
    }

    /// 映射整个虚拟页号范围
    pub fn map(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.map_one(page_table, vpn); // 对每个虚拟页号执行映射
        }
    }

    /// 解除整个虚拟页号范围的映射
    pub fn unmap(&mut self, page_table: &mut PageTable) {
        for vpn in self.vpn_range {
            self.unmap_one(page_table, vpn); // 对每个虚拟页号执行解除映射
        }
    }

    /// 缩小映射区域到新的结束虚拟页号
    #[allow(unused)]
    pub fn shrink_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(new_end, self.vpn_range.get_end()) {
            self.unmap_one(page_table, vpn) // 解除新结束虚拟页号之后的映射
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end); // 更新虚拟页号范围
    }

    /// 扩展映射区域到新的结束虚拟页号
    #[allow(unused)]
    pub fn append_to(&mut self, page_table: &mut PageTable, new_end: VirtPageNum) {
        for vpn in VPNRange::new(self.vpn_range.get_end(), new_end) {
            self.map_one(page_table, vpn) // 为新的虚拟页号范围执行映射
        }
        self.vpn_range = VPNRange::new(self.vpn_range.get_start(), new_end); // 更新虚拟页号范围
    }

    /// 复制数据到映射区域中（假设所有帧已被清除）
    pub fn copy_data(&mut self, page_table: &mut PageTable, data: &[u8]) {
        assert_eq!(self.map_type, MapType::Framed); // 确保映射类型是Framed
        let mut start: usize = 0;
        let mut current_vpn = self.vpn_range.get_start();
        let len = data.len();
        loop {
            let src = &data[start..len.min(start + PAGE_SIZE)]; // 源数据切片
            let dst = &mut page_table
                .translate(current_vpn) // 获取当前虚拟页号的物理页号
                .unwrap()
                .ppn()
                .get_bytes_array()[..src.len()]; // 获取目标地址的字节数组
            dst.copy_from_slice(src); // 将数据复制到目标位置
            start += PAGE_SIZE;
            if start >= len {
                break;
            }
            current_vpn.step(); // 移动到下一个虚拟页号
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
/// 映射类型，表示内存集合的类型：Identical 或 Framed
pub enum MapType {
    Identical, // Identical类型映射
    Framed, // Framed类型映射
}

bitflags! {
    /// 映射权限，表示页表项中的权限：`R W X U`
    pub struct MapPermission: u8 {
        /// 可读
        const R = 1 << 1;
        /// 可写
        const W = 1 << 2;
        /// 可执行
        const X = 1 << 3;
        /// 用户模式下可访问
        const U = 1 << 4;
    }
}

/// 内核空间中的重映射测试
#[allow(unused)]
pub fn remap_test() {
    let mut kernel_space = KERNEL_SPACE.exclusive_access();
    let mid_text: VirtAddr = ((stext as usize + etext as usize) / 2).into();
    let mid_rodata: VirtAddr = ((srodata as usize + erodata as usize) / 2).into();
    let mid_data: VirtAddr = ((sdata as usize + edata as usize) / 2).into();
    assert!(!kernel_space
        .page_table
        .translate(mid_text.floor())
        .unwrap()
        .writable(),); // 检查文本段中间位置是否不可写
    assert!(!kernel_space
        .page_table
        .translate(mid_rodata.floor())
        .unwrap()
        .writable(),); // 检查只读数据段中间位置是否不可写
    assert!(!kernel_space
        .page_table
        .translate(mid_data.floor())
        .unwrap()
        .executable(),); // 检查数据段中间位置是否不可执行
    println!("remap_test passed!"); // 如果测试通过，输出提示信息
}

