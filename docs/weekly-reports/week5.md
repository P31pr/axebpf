# 10.19 ~ 10.25

## 本周进展

### 调研探针输入上下文统一方案

- **现状**：各类探针的输出已汇入 `TraceEvent`；输入均通过 `runtime::run_program(prog_id, Option<&mut [u8]>)` 传递，但 hprobe/hretprobe 使用 `kprobe` 库的 `PtRegs`，guest kprobe/uprobe 和静态 tracepoint 使用 `TraceContext`。执行接口相同，内存布局和字段语义尚未完全统一，同一段 eBPF 程序不能直接按相同偏移读取两种上下文。
- **适配方向（尚未实现）**：先完成 AArch64 上的跨探针统一，保留 kprobe 框架及 `PtRegs`，在 hprobe 的入口、返回回调中构造 `TraceContext`，执行 eBPF 后按约定写回寄存器。现有 `regs[8]` 可复用为 x0–x7，保持旧字段偏移，追加 x8–x30、SP、PC、PSTATE 等信息，并标记字段有效性。当前 hprobe 从异常现场复制的有效寄存器范围是 `PtRegs` 的前 272 字节，尾部系统调用相关字段初始化为零。
- **元信息来源**：回调可以确定探针类型，通过平台接口获取时间戳，并从 `HprobeUserData` 取得探测地址、符号名和程序 ID；后三者需要另行约定在上下文中的表示。
- **收益与代价**：统一后可共享公共字段读取逻辑，减少 eBPF 程序对上游 `PtRegs` 布局的依赖。完整快照与回写会增加数据复制和栈占用，性能影响需要测量；保留 kprobe 框架本身不意味着必须降低性能。旧 hprobe eBPF 程序需要同步迁移，追加字段只保持旧 `TraceContext` 字段的偏移兼容。

## 下一步计划

- 明确扩展上下文的布局、字段有效性和寄存器回写规则；修改 `context.rs`、hprobe 两个执行回调及其他上下文构造点，保持 `run_program` 的字节切片接口。
- 同步 `axebpf-programs` 中的上下文定义和 hprobe 程序，补充布局、跨探针读取、寄存器回写及异常恢复验证，并比较改造前后的命中耗时。
