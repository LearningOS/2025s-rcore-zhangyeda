# 启用宏替换功能
.altmacro

# 定义保存通用寄存器的宏，n是寄存器编号
# Define a macro to save a general-purpose register
.macro SAVE_GP n
    # 将寄存器xn保存到以sp为基址，偏移量为n*8的地址处
    sd x\n, \n*8(sp)
.endm

# 定义加载通用寄存器的宏，n是寄存器编号
# Define a macro to load a general-purpose register
.macro LOAD_GP n
    # 从以sp为基址，偏移量为n*8的地址处加载数据到寄存器xn
    ld x\n, \n*8(sp)
.endm

    # 将以下代码放入.text.trampoline段（跳板页）
    .section .text.trampoline
    # 声明全局符号
    .globl __alltraps
    .globl __restore
    # 对齐到4字节边界
    .align 2

# 陷入处理入口点
__alltraps:
    # 交换sp和sscratch的值：sp现在指向用户空间的TrapContext，sscratch保存用户栈指针
    csrrw sp, sscratch, sp
    # now sp->*TrapContext in user space, sscratch->user stack
    # save other general purpose registers
    
    # 保存通用寄存器x1（返回地址寄存器ra）
    sd x1, 1*8(sp)
    # skip sp(x2), we will save it later
    # 保存x3
    sd x3, 3*8(sp)
    # skip tp(x4), application does not use it
    # save x5~x31
    
    # 使用循环保存x5到x31的值
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr

    # 此时可以自由使用t0/t1/t2，因为它们已经被保存在TrapContext中
    # we can use t0/t1/t2 freely, because they have been saved in TrapContext
    # 保存CSR寄存器到TrapContext
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)

    # 从sscratch读取用户栈指针并保存到TrapContext
    # read user stack from sscratch and save it in TrapContext
    csrr t2, sscratch
    sd t2, 2*8(sp)

    # 准备切换到内核空间
    # 加载内核页表token到t0
    ld t0, 34*8(sp)
    # 加载trap处理函数地址到t1
    ld t1, 36*8(sp)
    # 切换到内核栈
    ld sp, 35*8(sp)

    # 切换到内核地址空间
    csrw satp, t0
    # 刷新TLB
    sfence.vma
    # 跳转到trap处理函数
    jr t1

# 恢复到用户空间的函数
__restore:
    # a0: 用户空间TrapContext的地址
    # a1: 用户空间的页表token
    # a0: *TrapContext in user space(Constant); a1: user space token

    # 切换回用户空间
    csrw satp, a1
    # 刷新TLB
    sfence.vma
    # 保存TrapContext地址到sscratch
    csrw sscratch, a0
    # sp指向TrapContext
    mv sp, a0

    # now sp points to TrapContext in user space, start restoring based on it
    # 恢复CSR寄存器
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    csrw sstatus, t0
    csrw sepc, t1

    # 恢复通用寄存器（除了x0/sp/tp）
    # restore general purpose registers except x0/sp/tp
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr

    # 恢复用户栈指针
    ld sp, 2*8(sp)
    # 返回用户空间
    sret
