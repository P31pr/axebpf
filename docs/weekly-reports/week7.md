# 10.26 ~ 11.1

## 本周进展

### 实现 AArch64 探针输入上下文统一

- **实现范围**：在 `feat/aarch64-unified-hprobe-context` 分支完成改动（提交 `317635a`），使 AArch64 的 hprobe/hretprobe、guest kprobe/uprobe 及静态 tracepoint 均向 eBPF 传递 `TraceContext`。保留 kprobe 框架和 `PtRegs`，`runtime::run_program` 仍接收字节切片；其他架构的 hprobe 暂时沿用原始 `PtRegs`。
- **上下文布局**：扩展 `TraceContext`，复用 `regs[8]` 保存 x0–x7，追加 x8–x30、SP、PC、PSTATE 及 `PtRegs` 辅助字段。将时间戳放在首部，随后集中排列四个 `u32` 元信息字段，消除隐式 padding，结构体大小为 352 字节。与上周仅追加字段的设想不同，本次重排改变了旧字段偏移，旧 eBPF 程序需要同步适配并重新编译。
- **hprobe 适配**：入口和返回回调统一经适配层构造上下文，填充时间戳、探针类型和寄存器快照；执行结束后将寄存器字段写回 `PtRegs`，保留原有寄存器修改能力。程序执行报错前已完成的写入同样保留。VM/vCPU 归属仍没有通用来源，未因格式统一而补齐。
- **其他探针适配**：guest kprobe/uprobe 从仅传递 x0–x7 改为传递完整 x0–x30；静态 tracepoint 使用统一构造方法，并将输入探针类型修正为 `4`。新增 `regs_valid` 标记寄存器组是否有效：hprobe 为 `0b111`，guest 探针为 `0b011`，静态 tracepoint 为 `0`。该标记不控制寄存器写回，guest 路径也未新增写回能力。
- **接口边界**：统一的是内存布局，各类探针的 `arg0..arg3` 业务含义仍有区别；输出侧 `TraceEvent` 保持原有格式。补充 ABI 说明和 C 头文件，明确字段偏移、有效性及写回规则。

### 验证与联调准备

- **联调准备**：检查 `axvisor-ebpf` 的依赖和异常入口，确认其使用 `modules/axebpf` 子模块，现有 `TrapFrame` 接入接口无需因本次改动调整。拉取并检查 `axebpf-programs`：公共上下文定义仍为旧版，两个 hprobe 程序硬编码了 `PtRegs` 偏移，`printk` 仍从偏移 0 读取 tracepoint ID，均需适配。当前只完成检查，尚未修改程序源码或同步 AxVisor 子模块。

## 下一步计划

- 更新 `axebpf-programs` 的公共 `TraceContext` 定义，让三个程序通过字段或 `offset_of!` 读取上下文，重新生成 `.o`。
- 将运行库改动同步到 `axvisor-ebpf/modules/axebpf`，替换 `target/bpf/` 中的程序产物，启用预编译程序 feature 并重新构建 AxVisor。
- 调整自动联调脚本，使其使用本地适配后的程序仓库，实际检查 eBPF 读取的参数、返回值和元信息，而不只检查探针命中日志；随后验证寄存器写回、异常恢复，并比较改造前后的命中耗时。
