//! [`TaskContext`] 的实现
use crate::trap::trap_return;

#[repr(C)]
/// 任务上下文结构体，包含一些寄存器
pub struct TaskContext {
    /// 任务切换后返回的位置（ra 寄存器）
    ra: usize,
    /// 栈指针（sp 寄存器）
    sp: usize,
    /// s0-s11 寄存器，调用者保存
    s: [usize; 12],
}

impl TaskContext {
    /// 创建一个新的空任务上下文
    pub fn zero_init() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s: [0; 12],
        }
    }
    /// 创建一个带有陷入返回地址和内核栈指针的任务上下文
    pub fn goto_trap_return(kstack_ptr: usize) -> Self {
        Self {
            ra: trap_return as usize, // 设置返回地址为 `trap_return` 函数
            sp: kstack_ptr,          // 设置栈指针为传入的内核栈指针
            s: [0; 12],              // 初始化 s0-s11 为 0
        }
    }
}
