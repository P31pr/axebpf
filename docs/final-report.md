# AxVisor 中 eBPF 探针上下文统一与三层关联追踪

## 1. 项目目标

axebpf 在 AxVisor 内提供 Hypervisor、Guest 内核与 Guest 用户态的动态探针和 eBPF 执行能力。[中期报告](midterm-report.md)主要完成已有功能复现、测试基线建立和机制分析。期末阶段在此基础上，完成 AArch64 探针输入上下文统一，并构造一个可重复的请求通路，验证同一份 eBPF 程序能否跨三类探针采集和关联事件。

本阶段的核心目标是让已有能力使用统一的数据接口，并通过实际 eBPF 采集结果展示其用途。评估重点包括接口一致性、代码组织、程序复用和功能正确性。

## 2. 已完成工作

### 2.1 统一 AArch64 探针输入上下文

原实现的输出侧已经汇总为 `TraceEvent`，但输入侧存在两种布局：hprobe/hretprobe 直接将底层 kprobe 库的 `PtRegs` 交给 eBPF，guest 探针和静态 tracepoint 则使用 `TraceContext`。对于依赖上下文字段的程序，相同字节偏移在不同探针中可能具有不同含义。

本阶段扩展并重排 `TraceContext`，使 AArch64 路径使用统一的 352 字节布局：时间戳、探针类型等公共字段位于前部，寄存器区域容纳 x0–x30、SP、PC、PSTATE 及 `PtRegs` 辅助字段，并使用 `regs_valid` 标识有效的寄存器组。

| 探针路径 | 改造前 | 改造后 |
|---|---|---|
| hprobe / hretprobe | 原始 `PtRegs` 字节切片 | 在适配层构造 `TraceContext`，执行后将寄存器写回 `PtRegs` |
| guest kprobe / uprobe 及返回探针 | 原 `TraceContext`，仅提供 x0–x7 快照 | 使用新布局，提供 x0–x30 快照 |
| 静态 tracepoint | 原 `TraceContext` | 使用新布局和明确的静态探针类型，无寄存器快照 |

此外 hprobe 的入口和返回回调进一步抽取公共处理函数，统一程序调用、错误处理和日志采样逻辑。

### 2.2 实现一份可跨三类探针复用的采集程序

在 axebpf-programs 中新增 `axtrace.o`，统一读取 `TraceContext` 的 `probe_type`、`timestamp_ns` 和 `regs[0]`，并检查寄存器有效性。三个挂载点都约定第一个参数为请求 ID，因此无需为 hprobe、guest kprobe、uprobe 分别编写上下文解析逻辑。

采集结果写入 `AXTRACE_EVENTS`：

| 项目 | 内容 |
|---|---|
| key，16 字节 | 请求 ID、输入探针类型、显式置零的保留字段 |
| value，32 字节 | 首次时间、末次时间、命中次数、原样复制的 VM/vCPU 信息 |
| 容量 | 256 个条目；每个完整请求占用三个条目 |

运行时只加载一次程序，三个动态探针共用同一个程序 ID 和 map。重复命中会累加到对应行，避免通过覆盖记录掩盖重复采集。该版本面向单 Guest、串行请求和每轮唯一请求 ID；它不是每次事件都保留的完整日志流，也不提供同一 key 的并发原子累加保证。

旧联调曾将 `printk.o` 挂到多类探针，但各动态路径的可见日志与命中计数主要来自 Rust 框架，不能仅凭那些输出证明 eBPF 正确理解了不同上下文。本次验证改为读取 eBPF 自己写入的 map。已有 `hprobe_entry`、`hprobe_exit` 程序没有被本次 demo 合并或迁移，不能把仓库原有三个不同用途的程序直接算成“压缩为一个”。

### 2.3 构造用户态、Guest 内核态与 H 态的连续请求通路

为避免普通文件 I/O 的缓存、设备直通等行为使请求不经过 AxVisor，新增最小虚拟字符设备 `/dev/axtrace`。Guest 驱动通过 `miscdevice` 注册，只接收一个 64 位请求 ID，并通过专用 hypercall 将它交给 AxVisor。

```text
Guest 用户态：demo_submit(request_id, fd)       ← uprobe
    → write(fd, &request_id, 8)
Guest 内核态：axtrace_write → axtrace_submit   ← guest kprobe
    → hvc #0，传递请求 ID
Hypervisor：axvm::axtrace_submit               ← hprobe
    → 返回成功，用户 write 返回 8
```

AxVisor 增加实验 hypercall 编号 `0x41585401` 的处理分支，将结果写回 Guest x0；未知编号返回 `-ENOSYS`。这里是可控的 guest→host 消息实验，不是磁盘设备实现，也不意味着一般 Linux 系统调用必然进入 H 态。

client 使用请求 0 预热目标函数，再发送正式请求 41、42、43。启动脚本在模块加载后提供挂载窗口，client 预热后提供检查 uprobe 状态的窗口。模块函数地址来自本轮加载后的 `/proc/kallsyms`，用户与宿主符号来自对应构建产物，避免依赖猜测地址。

联调发现，首次 mmap 后安装 uprobe 可能因地址翻译失败而保持 pending；当前框架在 `__arm64_sys_nanosleep` 入口提供重试，而 libc 的 `sleep()` 在本镜像中走 `clock_nanosleep`。将 client 等待改为显式 `SYS_nanosleep` 后，观察到 `reason=runtime_retry` 激活成功和三次正式 uprobe 命中。这是让 demo 对接已有重试机制的修正，不是通用的自动激活机制修复；首次翻译失败的详细原因尚未最终定位。

代码与构建方法见 [demo 说明](../demos/axtrace/README.md)，操作步骤见 [手工联调指南](../demos/axtrace/manual.md)。

### 2.4 增加基于 eBPF map 的路径展示

AxVisor shell 新增 `trace path <程序ID>`，按请求 ID 解码 `AXTRACE_EVENTS`，将输入探针类型映射为 USER、KERNEL、HOST，展示时间戳和次数，并检查三层齐全、每层单次命中以及时间顺序。

## 3. 验证结果与量化分析

### 3.1 代码组织与复用收益

采用明确的 Git 对比范围，统计物理行数变化，排除 `.ai`、文档、测试和构建产物：

| 对比范围 | 新增行 | 删除行 | 净变化 |
|---|---:|---:|---:|
| 上下文统一及回调重构：`4eee9c2 → 2814176`，仅 `src/` | 203 | 97 | +106 |
| 其中 guest kprobe handler | 1 | 9 | −8 |
| 其中 guest uprobe handler | 1 | 9 | −8 |
| 其中静态 stats 路径 | 1 | 13 | −12 |
| 单独的 hprobe 回调重构：`317635a → 2814176`，manager 文件 | 38 | 45 | −7 |

最后一行是单独提交的局部对比，不能再与前几行相加。原始统计命令和输出见 [代码量记录](final-artifacts/code-metrics.txt)。

整体源码没有减少：完整寄存器适配与写回增加了代码。可证明的收益是重复处理逻辑集中、多个调用点简化，以及本次实际只用 **1 份采集程序、1 个运行实例、3 个动态挂载点**。尚未实现旧 ABI 下功能等价的多版本采集程序，所以没有声称“eBPF 源码减少了某个百分比”。

### 3.2 分层验证

| 验证层次 | 结果与边界 |
|---|---|
| 中期基线 | 历史记录为 259 个测试通过；不是本次最终代码全量重跑结果 |
| 上下文适配 | 10 月 30 日记录：宿主侧 15 项、AArch64 QEMU 用户态 4 项测试通过；覆盖布局、寄存器往返与写回等，不替代 EL2 异常路径验证 |
| 新采集程序 | 构建通过；2 项 ABI 测试和 1 项 rbpf 字节码测试通过，后者使用模拟 map helpers |
| 路径展示 | 3 项解码测试通过；实际宿主探针和 map 读取验证完成 |
| 设备与 hypercall | 模块加载、write 往返、错误长度拒绝、无探针时卸载，以及双等待流程验证完成 |
| 三层手工联调 | 两轮日志均采到正式请求的三层记录，但均存在一条正式请求的 guest kprobe 重复采集 |

上下文适配测试的依赖版本、命令和局限见 [阶段验证记录](final-artifacts/trace-context-validation.md)。本节区分历史基线、模拟执行、QEMU 用户态测试和 QEMU 中的真实虚拟化执行路径，不能将其合并表述为“最终版本所有测试通过”。

### 3.3 三层手工联调结果

两轮均在 QEMU AArch64 上运行 AxVisor 与 Linux Guest，三个动态挂载点均使用程序 ID 0，结果从 `trace path 0` 的 eBPF map 快照读取：

| 模式 | 正式请求 | USER 次数 | KERNEL 次数 | HOST 次数 | 判定 |
|---|---:|---:|---:|---:|---|
| s2fault | 41 | 1 | 1 | 1 | 通过 |
| s2fault | 42 | 1 | 2 | 1 | 三层齐全，内核层重复 |
| s2fault | 43 | 1 | 1 | 1 | 通过 |
| BRK inject | 41 | 1 | 2 | 1 | 三层齐全，内核层重复 |
| BRK inject | 42 | 1 | 1 | 1 | 通过 |
| BRK inject | 43 | 1 | 1 | 1 | 通过 |

共观察到 6 个正式请求实例：三层记录均存在，其中 4 个满足“每层恰好一次且时间顺序正确”。这是两轮小样本的结果，不用于估计稳定性或一般成功率。预热请求 0 在两轮中均为 USER 缺失、KERNEL 3 次、HOST 1 次：预热时 USER 尚未激活可以解释其缺失，但 KERNEL 重复仍是异常，不能因为排除预热验收就忽略它。

例如 BRK inject 模式的请求 42，map 中得到：

```text
USER    timestamp_ns=149782879296  hits=1
KERNEL  timestamp_ns=149783523072  hits=1
HOST    timestamp_ns=149784184176  hits=1
PASS: USER -> KERNEL -> HOST
```

这些记录直接支持“同一份程序通过统一输入布局采集三层，并按同一请求关联”的结论。时间戳顺序用来验证本次因果通路，不作为性能测试；日志输出、解释执行、断点与单步开销均可能影响间隔。

原始证据：[s2fault 日志](final-artifacts/s2fault-three-layer.log)、[BRK inject 日志](final-artifacts/brk-three-layer.log)。逐行 map 数据已整理为 [CSV](final-artifacts/request-records.csv)。

## 4. 当前问题与后续计划

1. **guest kprobe 重复采集。** 两种模式均有重复现象，框架计数与 map 计数相符，不是展示命令独有的问题。需要追踪异常分发、单步恢复与探针重新启用，定位一次调用被多次处理的原因，再补回归测试；当前未声称已修复。
2. **控制台交互不稳定。** 某些运行中 guest 启动后宿主命令只有回显、没有执行结果。代码中存在宿主与 guest 共用 UART 的读取路径，但尚未完成逐字节归因；不能认定去掉启动参数已彻底解决，也不能把延长等待作为修复。应先验证输入消费和调度，再考虑独立控制台或明确输入所有权。
3. **uprobe 激活机制仍有局限。** demo 依赖特定系统调用触发重试；首次页表翻译失败的具体原因尚需诊断。后续应使重试绑定可靠的目标进程上下文，而不是要求业务程序配合某个睡眠调用。
4. **元信息与通用性不足。** hprobe 的 VM/vCPU 信息仍可能为零，日志中不同层的 vCPU 字段也不一致；本实验用请求 ID 关联，没有证明这些元信息已经统一正确。当前采集器也没有解决多 VM 请求编号冲突、同一 key 并发更新和长期运行时 map 容量问题。
5. **收尾与性能验证未完成。** 在线拆除 hprobe 曾出现等待提示符超时，当前实验通过结束 QEMU 实例收尾。未进行受控的重构前后性能对比；hprobe 的上下文复制和写回可能带来额外成本，不能宣称性能提升。

后续优先处理重复采集和控制台交互，再完善自动验收与元信息；VS Code 接入、源码级远程调试和通用跨层因果追踪不属于本阶段已完成范围。

## 5. 阶段总结

本阶段从已有能力复现推进到实际接口改造与应用验证：完成 AArch64 输入上下文统一和 hprobe 回调整理，新增统一采集程序、最小虚拟字符设备通路及路径展示命令，并获得同一 eBPF 程序关联用户态、Guest 内核态、Hypervisor 态行为的直接证据。

目前可以展示完整的实验过程与可读结果，但结果同时揭示了重复采集和交互稳定性问题。准确结论是：**跨三层关联追踪的功能已得到验证，计数准确性和运行可靠性仍需完善。**

## 附件

- [中期报告](midterm-report.md)、[week7 周报](weekly-reports/week7.md)
- [ABI 说明](final-artifacts/trace-context-abi.md)、[上下文适配验证记录](final-artifacts/trace-context-validation.md)
- [改造补丁](final-artifacts/unified-context.patch)、[代码量统计](final-artifacts/code-metrics.txt)
- [uprobe 激活失败日志](final-artifacts/uprobe-pending.log)、[s2fault 三层日志](final-artifacts/s2fault-three-layer.log)、[BRK inject 三层日志](final-artifacts/brk-three-layer.log)
- [逐请求数据](final-artifacts/request-records.csv)、[日志校验清单](final-artifacts/log-manifest.json)
- [demo 代码与构建说明](../demos/axtrace/README.md)、[手工演示步骤](../demos/axtrace/manual.md)

**版本说明：** 本报告保存在 axebpf 的 dev 分支；上下文改造位于功能分支的上述提交，联调还包含 axvisor-ebpf、axebpf-programs 的工作区改动。归档日志是运行证据，不代表所有跨仓库改动已经提交或合并。正式交付时应补齐各仓库最终提交号及配套镜像的构建清单。
