//! 与任务管理相关的类型 & 完全更改 TCB 的函数
use super::TaskContext;
use super::{kstack_alloc, pid_alloc, KernelStack, PidHandle};
use crate::fs::{File, Stdin, Stdout};
use crate::config::{BIGSTRIDE, PAGE_SIZE, TRAP_CONTEXT_BASE};
use crate::mm::page_table::PTEFlags;
use crate::mm::{MemorySet, PhysPageNum, VirtAddr, VirtPageNum, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::timer::get_time;
use crate::trap::{trap_handler, TrapContext};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;

/// 任务信息结构体
#[derive(Copy, Clone)]
pub struct TaskInfo {
    /// 任务开始时间
    pub start:u64,
    /// 任务运行的总时间
    pub all:u64,
    /// 系统运行时间
    pub stime:u64,
    /// 子任务用户态运行时间
    pub cutime:u64,
    /// 子任务系统态运行时间
    pub cstime:u64,
}

impl TaskInfo {
    /// 初始化 TaskInfo，提供默认值
    pub fn new() -> Self {
        TaskInfo {
            start:get_time() as u64, // 设置为当前时间
            all:0,                  // 总时间初始为 0
            stime:0,                // 系统时间初始为 0
            cutime:0,               // 子任务用户态时间初始为 0
            cstime:0,               // 子任务系统态时间初始为 0
        }
    }

    /// 更新系统运行时间
    pub fn update_sys(mut self, ms:usize){
        self.stime += ms as u64; 
    }
    /// 更新子任务用户态运行时间
    pub fn update_cu(mut self, time:usize){
        self.cutime = time as u64;
    }
    /// 更新子任务系统态运行时间
    pub fn update_cs(mut self, time:usize){
        self.cstime = time as u64;
    }
}

/// 任务控制块结构体
///
/// 直接保存运行期间不会改变的内容
pub struct TaskControlBlock {
    // 不可变部分
    /// 进程标识符
    pub pid: PidHandle,
    /// 父进程 ID
    pub ppid: usize,
    /// 与 PID 对应的内核栈
    pub kernel_stack: KernelStack,
    /// 可变部分
    inner: UPSafeCell<TaskControlBlockInner>,
}

/// 任务控制块内部结构
pub struct TaskControlBlockInner {
    /// 放置陷阱上下文的帧的物理页号
    pub trap_cx_ppn: PhysPageNum,

    /// 应用程序数据只能出现在应用地址空间低于 `base_size` 的区域
    pub base_size: usize,

    /// 保存任务上下文
    pub task_cx: TaskContext,

    /// 维护当前进程的执行状态
    pub task_status: TaskStatus,

    /// 应用程序地址空间
    pub memory_set: MemorySet,

    /// 当前进程的父进程。
    /// 使用 `Weak` 不会影响父进程的引用计数
    pub parent: Option<Weak<TaskControlBlock>>,

    /// 包含当前进程所有子进程的 TCB 的向量
    pub children: Vec<Arc<TaskControlBlock>>,

    /// 当发生主动退出或执行错误时设置
    pub exit_code: i32,
    /// 文件描述符表
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,

    /// 堆底地址
    pub heap_bottom: usize,

    /// brk
    pub program_brk: usize,

    /// 任务信息
    pub task_info:Box<TaskInfo>,   

    /// 步幅值，用于 stride 调度
    pub stride: isize,

    /// 任务优先级
    pub pri: isize, 

    /// 当前工作目录
    pub pwd: String,
}


impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        self.trap_cx_ppn.get_mut()
    }
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    fn get_status(&self) -> TaskStatus {
        self.task_status
    }
    pub fn is_zombie(&self) -> bool {
        self.get_status() == TaskStatus::Zombie
    }
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
    pub fn set_pwd(&mut self, new_pwd:String){
        self.pwd = new_pwd;
    }
}

impl TaskControlBlock {
    /// 获取 TCB 内部结构的可变引用
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    /// 获取应用程序页表的地址
    pub fn get_user_token(&self) -> usize {
        let inner = self.inner_exclusive_access();
        inner.memory_set.token()
    }    

    /// 创建一个新进程
    ///
    /// 当前仅用于创建 `initproc`
    pub fn new(elf_data: &[u8]) -> Self {
        // 从 ELF 程序头创建 memory_set，并包含 trampoline、trap 上下文以及用户栈
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        
        // 获取陷阱上下文所在物理页号
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // 分配 PID 并在内核空间分配一个内核栈
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // 在内核栈顶推入一个任务上下文，用于跳转到 `trap_return`
        let task_control_block = Self {
            pid: pid_handle,
            ppid: 0,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> 标准输入 stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> 标准输出 stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> 标准错误 stderr
                        Some(Arc::new(Stdout)),
                    ],
                    heap_bottom: user_sp,
                    program_brk: user_sp + PAGE_SIZE,
                    task_info:Box::new(TaskInfo::new()),
                    stride: 0,
                    pri: 16,
                    pwd: String::from("/"),
                })
            },
        };
        // 准备用户空间的 TrapContext
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
       
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        task_control_block
    }

    /// 加载一个新的 ELF 文件以替换原来的应用程序地址空间，并开始执行
    pub fn exec(&self, elf_data: &[u8]) {
        // 从 ELF 程序头创建 memory_set，并包含 trampoline、trap 上下文以及用户栈
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // **** 独占访问当前 TCB
        let mut inner = self.inner_exclusive_access();
        // 替换 memory_set
        inner.memory_set = memory_set;
        // 更新 trap_cx 的物理页号
        inner.trap_cx_ppn = trap_cx_ppn;
        
        // 初始化 trap_cx
        let trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        
        *inner.get_trap_cx() = trap_cx;
        // **** 释放当前 PCB
    }

    /// 父进程 fork 子进程
    pub fn fork(self: &Arc<TaskControlBlock>) -> Arc<TaskControlBlock> {
        // ---- 锁定父 PCB
        let mut parent_inner = self.inner_exclusive_access();
        // 拷贝用户空间（包括陷阱上下文）
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // 在内核空间分配 PID 和内核栈
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        // 拷贝文件描述符表
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent_inner.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            ppid: self.getpid(),
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    heap_bottom: parent_inner.heap_bottom,
                    program_brk: parent_inner.program_brk,
                    task_info:Box::new(TaskInfo::new()),
                    stride: 0,
                    pri: 16,
                    pwd: parent_inner.pwd.clone(),
                })
            },
        });
        // 添加子进程
        parent_inner.children.push(task_control_block.clone());
        // 修改 trap_cx 中的 kernel_sp
        // **** 独占访问子 PCB
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        // 返回子进程
        task_control_block
        // **** 释放子 PCB
        // ---- 释放父 PCB
    }

    /// spawn 创建子进程
    pub fn spawn(self: &Arc<Self>, elf_data: &[u8]) -> Arc<Self> {
        // ---- 独占访问父 PCB
        let mut parent_inner = self.inner_exclusive_access();
        // 拷贝用户空间（包括陷阱上下文）
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT_BASE).into())
            .unwrap()
            .ppn();
        // 分配 PID 和内核栈
        let pid_handle = pid_alloc();
        let kernel_stack = kstack_alloc();
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            ppid: self.getpid(),
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> 标准输入 stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> 标准输出 stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> 标准错误 stderr
                        Some(Arc::new(Stdout)),
                    ],
                    heap_bottom: parent_inner.heap_bottom,
                    program_brk: parent_inner.program_brk,
                    task_info:Box::new(TaskInfo::new()),
                    stride: 0,
                    pri: 16,
                    pwd: parent_inner.pwd.clone(),
                })
            },
        });
        // 添加子进程
        parent_inner.children.push(task_control_block.clone());
        // 修改 trap_cx 中的 kernel_sp
        // **** 独占访问子 PCB
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        // 返回子进程
        task_control_block
        // **** 释放子 PCB
        // ---- 释放父 PCB
    }

    /// 获取进程的 pid
    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    /// 获取父进程的 pid
    pub fn getppid(&self) -> usize{
        self.ppid
    }

    /// 设置优先级
    pub fn set_priority(&self, prio: isize){
        let mut inner = self.inner_exclusive_access();
        inner.pri = prio;
        drop(inner);
    }

    /// 更新 stride 值
    pub fn update_stri(&self){
        let mut inner = self.inner_exclusive_access();
        inner.stride += BIGSTRIDE/inner.pri;
        drop(inner);
    }

    /// 修改brk
    pub fn change_program_brk(&self, new_add: i64) -> Option<usize> {
        let mut inner = self.inner_exclusive_access();
        let heap_bottom = inner.heap_bottom;
        let old_break = inner.program_brk;
        if new_add == 0{
            return Some(old_break as usize);
        }
        let size = new_add - old_break as i64;
        let new_brk = inner.program_brk as isize + size as isize;
        if new_brk < heap_bottom as isize {
            return None;
        }
        if size > PAGE_SIZE as i64{
            let result = if size < 0 {
                inner
                    .memory_set
                    .shrink_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
            } else {
                inner
                    .memory_set
                    .append_to(VirtAddr(heap_bottom), VirtAddr(new_brk as usize))
            };
            if result {
                inner.program_brk = new_brk as usize;
                Some(old_break)
            } else {
                None
            }
        }else{
            inner.program_brk = new_brk as usize;
            Some(new_brk as usize)
        }

    }

    /// 显示任务信息
    pub fn show_info(&self) -> TaskInfo{
        let inner = self.inner.exclusive_access();
        let task_info = *inner.task_info;
        drop(inner);
        task_info
    }

    /// 映射虚拟页号到物理页号
    pub fn map(&self, vpn: VirtPageNum, ppn: PhysPageNum, flags: PTEFlags) -> isize{
        let mut inner = self.inner.exclusive_access();
        let task = &mut inner.memory_set;
        task.map(vpn, ppn, flags);
        drop(inner);
        0
    }

    /// 取消映射虚拟页号
    pub fn unmap(&self, vpn: VirtPageNum) -> isize{
        let mut inner = self.inner.exclusive_access();
        let task = &mut inner.memory_set;
        task.unmap(vpn);
        drop(inner);
        0
    }
}


#[derive(Copy, Clone, PartialEq)]
/// task status: UnInit, Ready, Running, Exited
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Zombie,
}
