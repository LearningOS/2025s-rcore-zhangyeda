//! Process management syscalls
use crate::{
    task::{exit_current_and_run_next, suspend_current_and_run_next, TASK_MANAGER},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

/// 实现sys_trace函数的三种功能，claude-3.5-sonnet 帮助实现
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => {
            // 读取用户空间的一个字节
            let ptr = id as *const u8;
            unsafe {
                ptr.read_volatile() as isize
            }
        }
        1 => {
            // 写入一个字节到用户空间
            let ptr = id as *mut u8;
            unsafe {
                ptr.write_volatile(data as u8);
            }
            0
        }
        2 => {
            // 获取系统调用次数
            if id >= 500 {
                return -1;
            }
            TASK_MANAGER.get_current_task_syscall_times(id) as isize
        }
        _ => -1
    }
}
