// 任务管理实现
// 有关任务管理的所有内容，例如启动和切换任务，均在此模块中实现。
// 在整个操作系统中，由一个全局唯一的 [`TaskManager`] 实例 `TASK_MANAGER` 控制所有任务。
// 每个核心都有一个全局唯一的 [`Processor`] 实例 `PROCESSOR`，负责监控当前运行的任务。
// 全局唯一的 `PID_ALLOCATOR` 实例用于为用户应用分配 PID。
// 当你看到 `switch.S` 文件中的 `__switch` 汇编函数时请务必小心。该函数周围的控制流可能并不像你预期的那样。

mod context;       // 任务上下文模块
mod id;            // PID 分配模块
mod manager;       // 任务管理器模块
pub(crate) mod processor; // 处理器模块
mod switch;        // 任务切换模块
#[allow(clippy::module_inception)]
#[allow(rustdoc::private_intra_doc_links)]
mod task;          // 任务模块

use crate::{loader::get_app_data_by_name, timer::get_time}; // 导入应用加载器和计时器模块
use alloc::sync::Arc; // 引用计数同步模块
pub use context::TaskContext; // 导出任务上下文
use lazy_static::*; // 懒加载静态变量
pub use manager::{fetch_task, TaskManager}; // 导出任务管理器
use switch::__switch; // 使用任务切换的低级实现
pub use task::{TaskControlBlock, TaskStatus, TaskInfo}; // 导出任务控制块、状态和信息

pub use id::{kstack_alloc, pid_alloc, KernelStack, PidHandle}; // 导出 PID 和内核栈分配相关
pub use manager::add_task; // 导出添加任务方法
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, schedule, take_current_task,
    Processor,
}; // 导出处理器的功能接口

/// 挂起当前状态为 "Running" 的任务，并运行任务列表中的下一个任务。
pub fn suspend_current_and_run_next() {
    // 当前一定有任务正在运行。
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // 将状态改为 Ready
    task_inner.task_status = TaskStatus::Ready;
    let ms = get_time();
    task_inner.task_info.all += ms as u64 - task_inner.task_info.start;
    drop(task_inner);
    // 将任务重新加入就绪队列。
    add_task(task);
    // 跳转到调度循环
    schedule(task_cx_ptr);
}

/// 用户测试应用程序在 `make run TEST=1` 中的 pid
pub const IDLE_PID: usize = 0;

/// 退出当前状态为 "Running" 的任务，并运行任务列表中的下一个任务。
pub fn exit_current_and_run_next(exit_code: i32) {
    // 从处理器中取出当前任务
    let task = take_current_task().unwrap();

    let pid = task.getpid();
    if pid == IDLE_PID {
        println!(
            "[kernel] 空闲进程以退出码 {} 退出 ...",
            exit_code
        );
        panic!("所有应用程序已完成！");
    }
    let mut inner = task.inner_exclusive_access();
    // 将状态改为 Zombie（僵尸态）
    let ms = get_time();
    inner.task_info.all += ms as u64 - inner.task_info.start;
    inner.task_status = TaskStatus::Zombie;
    // 记录退出码
    inner.exit_code = exit_code;
    // 将任务移动到 `initproc` 的子任务下，而非其父任务
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    
    inner.children.clear();
    // 回收用户空间内存
    
    inner.memory_set.recycle_data_pages();
    // 清空文件描述符表
    
    inner.fd_table.clear();
    drop(inner);
    // 手动释放任务以正确维护引用计数
    drop(task);
    // 无需保存任务上下文
    let mut _unused = TaskContext::zero_init();

    schedule(&mut _unused as *mut _);
    
}

lazy_static! {
    /// 初始化进程的创建
    ///
    /// 名称 "initproc" 可以改为任何其他应用程序名称，比如 "usertests"，
    /// 但我们已经有用户 Shell，因此不需要更改。
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new(TaskControlBlock::new(
        get_app_data_by_name("ch6b_user_shell").unwrap()
        // let vfile = ROOT_INODE.find_vfile_byname("ch6b_initproc.elf").unwrap();
        // let v1 = OSInode::new(true, false, vfile);
        // let v = v1.read_all();
        // TaskControlBlock::new(v.as_slice())
    ));
    
}

/// 将初始化进程添加到任务管理器中
pub fn add_initproc() {
    add_task(INITPROC.clone());
}
