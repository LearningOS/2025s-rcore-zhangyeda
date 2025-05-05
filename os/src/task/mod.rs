//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::loader::{get_app_data, get_num_app};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use crate::mm::{MapPermission, VirtAddr, VirtPageNum, VPNRange};
use crate::config::PAGE_SIZE;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: Vec<TaskControlBlock>,
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` global instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        println!("init TASK_MANAGER");
        let num_app = get_num_app();
        println!("num_app = {}", num_app);
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch4, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut _, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Get the current 'Running' task's token.
    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    /// Get the current 'Running' task's trap contexts.
    fn get_current_trap_cx(&self) -> &'static mut TrapContext {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_trap_cx()
    }

    /// Change the current 'Running' task's program break
    pub fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

    /// 获取当前任务的系统调用次数，claude-3.5-sonnet 帮助实现
    pub fn get_current_task_syscall_times(&self, syscall_id: usize) -> usize {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscall_times[syscall_id]
    }

    /// 增加当前任务的系统调用次数，claude-3.5-sonnet 帮助实现
    pub fn increment_syscall_times(&self, syscall_id: usize) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].syscall_times[syscall_id] += 1;
    }

    /// 为当前任务添加映射区域
    pub fn mmap_current_task(&self, start_va: VirtAddr, end_va: VirtAddr, perm: MapPermission) -> bool {
        debug!("mmap_current_task: processing start_va={:#x}, end_va={:#x}, perm={:?}", start_va.0, end_va.0, perm);
        
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        
        // 获取内存集
        let memory_set = &mut inner.tasks[current].memory_set;
        
        // 确保地址范围合法
        if start_va.0 >= end_va.0 {
            debug!("mmap_current_task: empty mapping, returning success");
            return true;  // 对于空映射，直接返回成功
        }
        
        // 检查要映射的所有页是否都未被映射
        let start_vpn = start_va.floor();
        let end_vpn = end_va.ceil();
        debug!("mmap_current_task: checking page range [{:?}, {:?})", start_vpn, end_vpn);
        
        // Print all current memory areas for debugging
        debug!("mmap_current_task: current memory areas:");
        for (i, area) in memory_set.get_areas().iter().enumerate() {
            debug!("  area {}: [{:?}, {:?})", i, area.get_start(), area.get_end());
        }
        
        // Check if any pages in the range are already mapped
        let mut already_mapped = false;
        for vpn in VPNRange::new(start_vpn, end_vpn) {
            if memory_set.translate(vpn).is_some() {
                debug!("mmap_current_task: page already mapped vpn={:?}", vpn);
                already_mapped = true;
                break;
            }
        }
        
        if already_mapped {
            debug!("mmap_current_task: some pages already mapped, returning failure");
            return false;
        }
        
        // 添加映射区域
        debug!("mmap_current_task: adding map area");
        memory_set.insert_framed_area(start_va, end_va, perm);
        debug!("mmap_current_task: mapping successful");
        true
    }
    
    /// 为当前任务取消映射区域
    pub fn munmap_current_task(&self, start_va: VirtAddr, end_va: VirtAddr) -> bool {
        debug!("munmap_current_task: processing start_va={:#x}, end_va={:#x}", start_va.0, end_va.0);
        
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        let memory_set = &mut inner.tasks[current].memory_set;
        
        // 确保地址范围合法
        if start_va.0 >= end_va.0 {
            debug!("munmap_current_task: failed - invalid range");
            return false;  // 无效的范围
        }
        
        // 使用memory_set的remove_area_with_start_vpn方法来取消映射
        debug!("munmap_current_task: attempting to unmap [{:?}, {:?})", start_va.floor(), end_va.ceil());
        let result = memory_set.remove_area_with_start_vpn(start_va.floor(), end_va.ceil());
        
        if result {
            debug!("munmap_current_task: successfully unmapped");
        } else {
            debug!("munmap_current_task: failed - area not found or cannot be unmapped");
        }
        
        result
    }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

/// Get the current 'Running' task's token.
pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

/// Get the current 'Running' task's trap contexts.
pub fn current_trap_cx() -> &'static mut TrapContext {
    TASK_MANAGER.get_current_trap_cx()
}

/// Change the current 'Running' task's program break
pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

/// 添加内存映射区域
pub fn mmap_area(start_va: VirtAddr, end_va: VirtAddr, permission: MapPermission) -> bool {
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    inner.tasks[current].memory_set.insert_framed_area(start_va, end_va, permission);
    true
}

/// 检查内存映射
pub fn check_vm_area(start_vpn: VirtPageNum, end_vpn: VirtPageNum, writable: bool) -> bool {
    debug!("check_vm_area: checking [{:?}, {:?}), writable={}", start_vpn, end_vpn, writable);
    let inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    let memory_set = &inner.tasks[current].memory_set;
    
    // 使用VPNRange遍历虚拟页号
    for vpn in VPNRange::new(start_vpn, end_vpn) {
        if let Some(pte) = memory_set.translate(vpn) {
            if writable && !pte.writable() {
                debug!("check_vm_area: page {:?} not writable", vpn);
                return false;
            }
        } else {
            debug!("check_vm_area: page {:?} not mapped", vpn);
            return false;
        }
    }
    debug!("check_vm_area: all checks passed");
    true
}

/// 删除内存映射区域
pub fn unmap_area(start_vpn: VirtPageNum, end_vpn: VirtPageNum) -> bool {
    debug!("unmap_area: attempting to unmap [{:?}, {:?})", start_vpn, end_vpn);
    let mut inner = TASK_MANAGER.inner.exclusive_access();
    let current = inner.current_task;
    
    // 直接使用memory_set的remove_area_with_start_vpn方法
    let result = inner.tasks[current].memory_set.remove_area_with_start_vpn(start_vpn, end_vpn);
    
    if result {
        debug!("unmap_area: successfully unmapped");
    } else {
        debug!("unmap_area: failed to unmap");
    }
    
    result
}

/// 为当前任务添加内存映射
/// 
/// # 参数
/// 
/// * `start` - 映射的起始地址，必须按页对齐
/// * `len` - 映射的长度，将按页大小向上取整
/// * `prot` - 映射的权限，低3位分别表示可读(1)、可写(2)、可执行(4)
/// 
/// # 返回值
/// 
/// * `0` - 映射成功
/// * `-1` - 映射失败，可能是参数错误或内存已被映射
pub fn mmap(start: usize, len: usize, prot: usize) -> isize {
    debug!("mmap: processing start={:#x}, len={:#x}, prot={:#b}", start, len, prot);
    
    // 检查起始地址是否页对齐
    if VirtAddr(start).page_offset() != 0 {
        debug!("mmap: failed - start address not page-aligned: {:#x}", start);
        return -1;
    }
    
    // 对于len=0的情况特殊处理
    if len == 0 {
        debug!("mmap: success - zero length, returning immediately");
        return 0;  // 直接返回成功
    }
    
    // 检查权限是否有效
    if (prot & !0x7) != 0 {
        debug!("mmap: failed - protection flags contain invalid bits: {:#b}", prot);
        return -1;
    }
    if (prot & 0x7) == 0 {
        debug!("mmap: failed - protection flags do not include any valid permissions");
        return -1;
    }
    
    // 如果地址超出范围，返回错误
    if start >= crate::config::TRAMPOLINE {
        debug!("mmap: failed - start address out of range: {:#x} >= {:#x}", start, crate::config::TRAMPOLINE);
        return -1;
    }
    
    // 计算映射的结束地址
    let end = start + ((len - 1) / PAGE_SIZE + 1) * PAGE_SIZE;  // 向上取整到页大小的倍数
    debug!("mmap: calculated mapping end address: {:#x}", end);
    
    // 如果结束地址超出范围，返回错误
    if end > crate::config::TRAMPOLINE {
        debug!("mmap: failed - end address out of range: {:#x} > {:#x}", end, crate::config::TRAMPOLINE);
        return -1;
    }
    
    // 设置映射权限
    let mut map_perm = MapPermission::U;
    if (prot & 0x1) != 0 { map_perm |= MapPermission::R; }
    if (prot & 0x2) != 0 { map_perm |= MapPermission::W; }
    if (prot & 0x4) != 0 { map_perm |= MapPermission::X; }
    debug!("mmap: setting permissions: {:?}", map_perm);
    
    // 尝试添加映射
    debug!("mmap: attempting to add mapping: [{:#x}, {:#x})", start, end);
    let result = TASK_MANAGER.mmap_current_task(VirtAddr(start), VirtAddr(end), map_perm);
    if result { 
        debug!("mmap: successfully added mapping: [{:#x}, {:#x})", start, end);
        0 
    } else { 
        debug!("mmap: failed - unable to add mapping: [{:#x}, {:#x})", start, end);
        -1 
    }
}

/// 取消当前任务的内存映射
/// 
/// # 参数
/// 
/// * `start` - 要取消映射的起始地址，必须按页对齐
/// * `len` - 要取消映射的长度，将按页大小向上取整
/// 
/// # 返回值
/// 
/// * `0` - 取消映射成功
/// * `-1` - 取消映射失败，可能是参数错误或内存未被映射
pub fn munmap(start: usize, len: usize) -> isize {
    debug!("munmap: processing start={:#x}, len={:#x}", start, len);
    
    // 检查起始地址是否页对齐
    if VirtAddr(start).page_offset() != 0 {
        debug!("munmap: failed - start address not page-aligned: {:#x}", start);
        return -1;
    }
    
    // 检查长度是否为0
    if len == 0 {
        debug!("munmap: failed - zero length");
        return -1;  // 根据规范，长度为0时返回错误
    }
    
    // 检查地址是否在有效范围内
    if start >= crate::config::TRAMPOLINE {
        debug!("munmap: failed - start address out of range: {:#x} >= {:#x}", start, crate::config::TRAMPOLINE);
        return -1;
    }
    
    // 计算取消映射的结束地址
    let end = start + ((len - 1) / PAGE_SIZE + 1) * PAGE_SIZE;
    debug!("munmap: calculated unmapping end address: {:#x}", end);
    
    // 检查结束地址是否在有效范围内
    if end > crate::config::TRAMPOLINE {
        debug!("munmap: failed - end address out of range: {:#x} > {:#x}", end, crate::config::TRAMPOLINE);
        return -1;
    }
    
    // 尝试取消映射
    debug!("munmap: attempting to unmap: [{:#x}, {:#x})", start, end);
    let result = TASK_MANAGER.munmap_current_task(VirtAddr(start), VirtAddr(end));
    if result {
        debug!("munmap: successfully unmapped: [{:#x}, {:#x})", start, end);
        0
    } else {
        debug!("munmap: failed - unable to unmap: [{:#x}, {:#x})", start, end);
        -1
    }
}