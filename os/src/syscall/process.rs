//! Process management syscalls
use crate::task::{change_program_brk, exit_current_and_run_next, suspend_current_and_run_next, current_user_token, mmap, munmap, TASK_MANAGER};
use crate::timer::get_time_us;
use crate::mm::translated_byte_buffer;
use crate::mm::PageTable;
use crate::mm::VirtAddr;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    // 获取当前任务的页表token
    let token = current_user_token();
    
    // 获取微秒级时间
    let us = get_time_us();
    
    // 计算秒和微秒
    let sec = us / 1_000_000;
    let usec = us % 1_000_000;
    
    // 创建TimeVal结构体的字节表示
    let time_val = TimeVal {
        sec,
        usec,
    };
    
    // 将TimeVal结构体的字节表示转换为字节数组
    let time_bytes = unsafe {
        core::slice::from_raw_parts(
            &time_val as *const TimeVal as *const u8,
            core::mem::size_of::<TimeVal>()
        )
    };
    
    // 获取用户空间TimeVal结构体对应的缓冲区列表（可能跨页）
    let mut buffers = translated_byte_buffer(token, ts as *const u8, core::mem::size_of::<TimeVal>());
    
    // 将时间值写入用户空间内存
    let mut total_len = 0;
    for buffer in buffers.iter_mut() {
        let len = buffer.len();
        buffer.copy_from_slice(&time_bytes[total_len..total_len + len]);
        total_len += len;
    }
    
    0
}
   
/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    
    // 获取当前任务的token
    let token = current_user_token();
    
    match trace_request {
        // 读取内存中的数据
        0 => {
            // 检查地址是否在有效范围内
            // TRAMPOLINE是用户空间最高地址，而KERNEL_ENTRY_PA(0x80200000)是内核区域开始
            // 用户地址应该在0到TRAMPOLINE之间，且不应该达到内核区域
            if id == 0 || id >= crate::config::TRAMPOLINE || id >= 0x80200000 {
                return -1;
            }
            
            // 检查内存是否可读
            let page_table = PageTable::from_token(token);
            let data_vpn = VirtAddr::from(id).floor();
            if let Some(pte) = page_table.translate(data_vpn) {
                if !pte.readable() {
                    return -1;
                }
            } else {
                return -1;
            }
            
            // 获取内存缓冲区
            let buffers = translated_byte_buffer(token, id as *const u8, 1);
            if buffers.is_empty() {
                return -1;
            }
            
            // 读取字节并返回其值作为isize
            let value = buffers[0][0] as isize;
            value
        }
        // 写入数据到内存
        1 => {
            // 检查地址是否在有效范围内
            if id == 0 || id >= crate::config::TRAMPOLINE || id >= 0x80200000 {
                return -1;
            }
            
            // 检查内存是否可写
            let page_table = PageTable::from_token(token);
            let data_vpn = VirtAddr::from(id).floor();
            if let Some(pte) = page_table.translate(data_vpn) {
                if !pte.writable() {
                    return -1;
                }
            } else {
                return -1;
            }
            
            // 获取内存缓冲区
            let mut buffers = translated_byte_buffer(token, id as *const u8, 1);
            if buffers.is_empty() {
                return -1;
            }
            
            // 写入一个字节，使用data参数
            buffers[0][0] = data as u8;
            
            info!("[kernel] Tracing syscall, addr: {:#x}, data: {}", id, data);
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

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap");
    mmap(start, len, prot)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    munmap(start, len)
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
