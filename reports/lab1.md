# ch3 作业

## 实现的功能

1. 在 TaskControlBlock 中添加了 syscall_times 数组来统计系统调用次数
2. 在 TaskManager 中添加了两个公共方法来管理系统调用统计：
 - get_current_task_syscall_times: 获取当前任务的系统调用次数
 - increment_syscall_times: 增加当前任务的系统调用次数
3. 实现了 sys_trace 函数的三种功能：
 - trace_request = 0: 读取用户空间指定地址的一个字节
 - trace_request = 1: 写入一个字节到用户空间指定地址
 - trace_request = 2: 获取指定系统调用的调用次数
4. 在每次系统调用时自动更新统计信息

## 简答作业

1. 进入ci-user目录，执行`make test CHAPTER=2`，可以得到如下输出：

```shell
make[1]: Entering directory '/workspace/rustos/2025s-rcore-zhangyeda/os'
cargo build --release
timeout --foreground 30s qemu-system-riscv64 \
	-machine virt \
	-nographic \
	-bios ../bootloader/rustsbi-qemu.bin \
	-kernel target/riscv64gc-unknown-none-elf/release/os
[rustsbi] RustSBI version 0.3.0-alpha.2, adapting to RISC-V SBI v1.0.0
.______       __    __      _______.___________.  _______..______   __
|   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
|  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
|      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
|  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
| _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|
[rustsbi] Implementation     : RustSBI-QEMU Version 0.2.0-alpha.2
[rustsbi] Platform Name      : riscv-virtio,qemu
[rustsbi] Platform SMP       : 1
[rustsbi] Platform Memory    : 0x80000000..0x88000000
[rustsbi] Boot HART          : 0
[rustsbi] Device Tree Region : 0x87000000..0x87000ef2
[rustsbi] Firmware Address   : 0x80000000
[rustsbi] Supervisor Address : 0x80200000
[rustsbi] pmp01: 0x00000000..0x80000000 (-wr)
[rustsbi] pmp02: 0x80000000..0x80200000 (---)
[rustsbi] pmp03: 0x80200000..0x88000000 (xwr)
[rustsbi] pmp04: 0x88000000..0x00000000 (-wr)
[kernel] Hello, world!
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
[kernel] IllegalInstruction in application, kernel killed it.
[kernel] IllegalInstruction in application, kernel killed it.
[kernel] Panicked at src/syscall/fs.rs:11 called `Result::unwrap()` on an `Err` value: Utf8Error { valid_up_to: 3, error_len: Some(1) }
make[1]: Leaving directory '/workspace/rustos/2025s-rcore-zhangyeda/os'
```

可以看到三个bad测试用例报错分别如下：
```shell
[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.
[kernel] IllegalInstruction in application, kernel killed it.
[kernel] IllegalInstruction in application, kernel killed it.
```

使用的RustSBI version 0.3.0-alpha.2。

2. 深入理解 trap.S 中两个函数 __alltraps 和 __restore 的作用，并回答如下问题:

2.1 __restore 用两种使用场景：首次进入用户态和从异常处理返回。首次进入用户态时sp指向为新任务预先准备的 TrapContext；从异常返回用户程序时指向内核栈上的TrapContext，包含了即将恢复的用户程序的完整上下文。

2.2 tarp.S L43-L48 处理了sstatus (Supervisor Status Register) 、sepc (Supervisor Exception Program Counter) 、 sscratch (Supervisor Scratch Register) 三个特殊寄存器。sstatus控制处理器的特权级状态和全局中断使能，spec保存异常返回地址，sscratch用于保存用户栈指针。其作用详细解释如下：

```shell
# 从 TrapContext 中加载处理器状态到临时寄存器
ld t0, 32*8(sp)      # 加载 sstatus 值：从 sp+8*32 处读取处理器状态到 t0
ld t1, 33*8(sp)      # 加载 sepc 值：从 sp+8*33 处读取异常返回地址到 t1
ld t2, 2*8(sp)       # 加载用户栈指针：从 sp+8*2 处读取用户栈指针到 t2

# 将临时寄存器中的值写入到对应的 CSR (控制和状态寄存器)
csrw sstatus, t0     # 恢复处理器状态：将 t0 写入 sstatus，设置特权级和中断状态
csrw sepc, t1        # 设置返回地址：将 t1 写入 sepc，确定返回后执行的指令位置
csrw sscratch, t2    # 设置用户栈指针：将 t2 写入 sscratch，为用户态准备栈环境
```
2.3 跳过x2是因为x2是x2 是栈指针寄存器(sp)，在 __restore 函数的最后会专门处理 sp 的恢复。跳过 x4线程指针寄存器(tp)，在当前的操作系统实现中，用户程序不使用这个寄存器。

2.4 L60 将 sscratch 的值写入 sp，同时将 sp 的值写入 sscratch。这一步后sp指向用户栈，sscratch指向内核栈。

2.5 状态切换发生在最后的 sret指令，执行sret指令将会：

- 将 PC (程序计数器) 设置为 sepc 寄存器的值
- 根据 sstatus 寄存器的 SPP (Supervisor Previous Privilege) 位恢复特权级
- 将 sstatus.SPP 设置为 0
- 将 sstatus.SIE (Supervisor Interrupt Enable) 设置为 sstatus.SPIE 的值
- 将 sstatus.SPIE 设置为 1

我们的系统在初始化TrapContext时就将SPP设置为用户态，当 sret 执行时，会根据这个SPP位决定返回到哪个特权级，按照当前代码看会返回到用户态。

2.6 L13 起到的是交换 sscratch 和 sp 的效果。在这一行之前 sp 指向用户栈， sscratch 指向内核栈，现在 sp 指向内核栈， sscratch 指向用户栈。

2.7 从U态到S态是用户程序调用ecall指令时发生的，在 sbi.rs 中的系统调用实现执行ecall指令时会触发异常，在 trap.S 中没有触发特权级切换的指令，因为当 trap.S 的代码开始执行时，特权级切换已经发生了。trap.S 的主要工作是保存上下文和处理异常，而不是触发特权级切换。

## 荣誉准则

在完成本次实验的过程（含此前学习的过程）中，我曾分别与以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

- 与claude-3.5-sonnet 交流了sys_trace 实现

此外，我也参考了以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

- 与claude-3.5-sonnet 交流了TaskManager.get_current_task_syscall_times 和 TaskManager.increment_syscall_times 的实现。
- 简答2参考了第二章内容 https://learningos.cn/rCore-Tutorial-Guide-2025S/chapter2/4trap-handling.html

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
