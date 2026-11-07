# 11.2 ~ 11.8

## 本周进展

### 完成统一上下文的集成与采集程序实现

- **接入 AxVisor**：将功能分支的上下文改动同步到 `axvisor-ebpf/modules/axebpf`，适配 `printk.o` 的 tracepoint ID 读取偏移，完成 AxVisor 构建和基础联调。hprobe 的入口、返回回调进一步复用公共处理函数，集中程序执行、错误处理和日志采样逻辑。
- **统一采集程序**：在 `axebpf-programs` 中新增 `axtrace.o`，读取 `TraceContext` 的探针类型、时间戳和 `regs[0]`，检查寄存器有效性后，以“请求 ID＋探针类型”为 key，将首次/末次时间、命中次数和 VM/vCPU 信息写入 `AXTRACE_EVENTS`。三个动态探针共用一个程序实例和 map，不再仅凭框架自身的命中日志判断 eBPF 是否正确执行。

### 构建三层关联追踪 demo

- **可控请求通路**：新增 Linux `miscdevice` 驱动，注册 `/dev/axtrace`，接收用户程序写入的 64 位请求 ID；驱动通过专用 hypercall 将请求交给 AxVisor。宿主补充该 hypercall 的处理和返回逻辑，跑通“用户函数 → guest 驱动 → H 态处理函数 → 返回用户态”的连续路径。
- **三个挂载点**：uprobe 挂载用户程序 `demo_submit`，guest kprobe 挂载驱动 `axtrace_submit`，hprobe 挂载宿主 `axtrace_submit`。三个函数均以请求 ID 为第一个参数，使同一份 eBPF 程序能够统一读取 x0 并关联三层记录。该通路是专门构造的消息实验，不代表普通文件 I/O 都会经过 H 态。
- **联调时序**：启动脚本加载模块并打印函数地址，提供安装 guest kprobe 的等待窗口；client 用请求 0 预热，在同一进程内等待检查 uprobe 状态，再发送正式请求 41、42、43。提取并更新用户、内核及宿主符号，制作独立实验镜像，保留原镜像。
- **处理激活重试问题**：首次 mmap 后尝试安装 uprobe 时出现地址翻译失败。检查发现现有重试观察点位于 `__arm64_sys_nanosleep`，而 client 的 libc `sleep()` 实际走 `clock_nanosleep`。改为显式调用 `SYS_nanosleep` 后，观察到 `reason=runtime_retry` 激活成功及正式请求的 uprobe 命中。此修改解决 demo 与既有重试入口的衔接，尚未完善框架的通用激活机制。

### 展示采集结果

- **路径展示**：新增 AxVisor shell 命令 `trace path <程序ID>`，直接解码 eBPF map，按请求 ID 展示 USER、KERNEL、HOST 三层的时间和命中次数，检查缺失、重复采集及时间逆序。完成 3 项解码测试，并验证实际 map 的读取与展示。
- **手工联调结果**：分别使用 guest kprobe 的 `s2fault` 和 `--inject` 模式完成两轮实验。6 个正式请求实例均采到三层记录，其中 4 个满足每层单次命中及时间顺序要求；另外 2 个在 KERNEL 层各记录了两次命中。因此已有证据证明同一份 eBPF 程序能够跨三层关联采集，但计数准确性还未全部通过验收。

### 当前问题

- **guest kprobe 重复采集**：两种模式均出现重复命中，框架计数与 eBPF map 记录一致，不能归因于展示命令，也未因切换模式消失。预热请求同样存在重复现象，需要排查异常分发、单步恢复和探针重新启用过程。
- **控制台交互不稳定**：部分运行中 guest 启动后，AxVisor shell 输入只有回显、没有执行结果，自动三层验收也曾因此超时。宿主与 guest 共用 UART 是当前排查方向，但尚未完成输入消费归因，去掉 `console=ttyAMA0` 未彻底解决。
- **验证边界**：hprobe 的 VM/vCPU 归属仍不完整，跨层 vCPU 字段也尚未证明一致；当前依靠请求 ID 关联。在线拆除 hprobe 曾超时，实验通过退出 QEMU 收尾；尚未完成性能对比和最终版本的全量回归。

## 下一步计划

- 定位 guest kprobe 重复命中的触发条件，补充异常恢复相关的回归用例，避免通过结果去重掩盖问题。
- 验证控制台输入由谁消费，解决宿主与 guest 的交互冲突，再完善自动三层验收。
- 改善 uprobe 激活重试，减少对 demo 特定系统调用的依赖，并继续核对页表上下文和元信息。
- 整理各仓库的最终提交与配套镜像清单，准备演示录像和日志，明确汇报中已验证成果与剩余问题的边界。

## 相关材料

- [手工联调步骤](../../demos/axtrace/manual.md)
- [s2fault 模式日志](../final-artifacts/s2fault-three-layer.log)、[BRK inject 模式日志](../final-artifacts/brk-three-layer.log)
- [逐请求采集数据](../final-artifacts/request-records.csv)、[代码量统计](../final-artifacts/code-metrics.txt)
