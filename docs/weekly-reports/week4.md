# 10.12 ~ 10.18

## 本周进展

### 进一步阅读 axebpf 源码

#### axebpf的执行链路

根据开发者 [Iscreamx 的技术总结报告](https://iscreamx.github.io/2026%E5%BC%80%E6%BA%90%E7%A4%BE%E5%8C%BA%E5%AE%9E%E4%B9%A0/%E6%8A%80%E6%9C%AF%E6%80%BB%E7%BB%93%E6%8A%A5%E5%91%8A.html)，axebpf 希望让不同的 tracing 能力共享一条统一运行链路：

1. 用户先把 eBPF 程序装入系统。
2. 用户再把程序绑定到某个事件源或探测目标。
3. 目标在运行中被触发或命中。
4. 系统把这次触发整理成统一上下文。*（以上为前半段）*
5. runtime 执行对应程序。
6. 执行结果进入统一事件流和状态统计。
7. 用户通过统一命令查看状态、事件和统计结果。

其中第 1 ~ 4 步根据不同的 tracing 能力需要提供有较大差异的实现（h/k/uprobe 或静态 tracepoint），而第 5 ~ 7 步则复用程度较高，不同的 tracing 能力基本能够共享这段链路。

#### 后半段链路

当前后半段链路所需能力主要分成 **ebpf 程序执行、map 基础设施、事件统计和结果输出** 四组。

| 能力组 | 主要模块 | 职责 |
|---|---|---|
| **ebpf 程序执行** | `runtime.rs` | 定义 ebpf 程序结构体 `EbpfProgram` 并维护全局注册表 `static PROGRAM_REGISTRY: Mutex<Vec<Option<EbpfProgram>>>` ，执行调用 rbpf 库 |
| | `helpers.rs` | 注册程序执行中可调用的宿主函数（map，读内存，时间，调试输出，CPU ID，事件名称（axebpf 扩展）） |
| **map 基础设施** | `maps.rs` | 提供 map 创建、查询、更新、删除接口供 ebpf helper调用 |
| | `map_ops.rs` | 管理底层 map 注册表，适配内存与平台操作 |
| **事件统计** | `event.rs` | 定义事件记录、缓冲事件、累计命中和耗时统计 |
| | `tracepoints/histogram.rs` | 耗时分桶、百分位计算 |
| | `tracepoints/registry.rs` | 维护事件名称与 ID 的对应关系 |
| **结果输出** | `output.rs` | 格式化并打印 map 中的程序结果 |
| | AxVisor 的 `shell/commands/trace.rs` | 查询程序、maps、事件和统计，向用户显示 |

另外有几项配套能力：

| 模块 | 作用与边界 |
|---|---|
| `context.rs` | 定义 `TraceContext`，位于前半段构造输入与后半段消费输入的边界，但 hprobe 使用 `PtRegs`，**目前不统一之处**|
| `attach.rs`、`tracepoints/stats.rs` | 按事件名称查绑定程序、构造上下文并执行，是静态事件到 runtime 的桥接层 |
| `platform.rs` | 提供时间、CPU ID 等环境能力 |
| `vmap.rs` | 为底层 RingBuf 提供虚拟内存映射支撑 |
| `tracepoints/hypervisor_helpers.rs` | 提供 VM/vCPU 等上下文相关的扩展 helper 能力 |

### 下一步计划

- 确认 hprobe 运行链路能否与其他 tracing 能力统一上下文格式，若能统一则考虑复用
- 进一步阅读前半段链路不同 tracing 能力的源码，理解不同 tracing 功能所需能力的区别
- 拉取最新版本 AxVisor，调研如何将 axebpf 适配到最新版 AxVisor