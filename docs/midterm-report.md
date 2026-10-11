# AxVisor 中 axebpf 模块的机制分析与实验验证

## 1. 项目目标

axebpf 是运行在 AxVisor 内部的动态观测与 eBPF 执行模块，支持观察 Hypervisor、Guest 内核和 Guest 用户程序。本阶段主要复现已有功能、建立测试基线，并阅读核心调用链，为后续代码重构和缺陷修复做准备。

## 2. 已完成工作

### 2.1 跑通 axebpf 模块测试

拉取 AxVisor 的 `feat/ebpf` 分支代码，跑通 axebpf 的宿主机测试。测试运行不需要启动 AxVisor，但当前构建依赖其 workspace 配置和测试资源：

- `axalloc` 使用 `workspace = true`，由根目录提供依赖来源，实际来自 ArceOS。
- runtime 测试使用仓库根目录 `target/bpf/` 下的预编译 eBPF 对象文件。

启用 `test-utils,hprobe,guest-kprobe,guest-uprobe` 后，已有验证结果为 **259 个测试通过、0 个失败，另有 4 个文档测试被忽略**。测试覆盖 runtime、maps、helpers、程序挂载、事件统计及探针状态管理，可作为后续修改的回归基线。

这些测试主要检查内部逻辑，Guest 相关用例使用模拟页表、内存和平台 hook，不能替代真实异常与硬件执行路径验证。

### 2.2 跑通 axebpf 主线手工联调

按照主线指南，在 QEMU 中启动 AxVisor 和 Linux Guest，通过 AxVisor shell 完成符号加载、eBPF 程序加载和探针挂载，再启动 Guest demo 触发观测点。

联调中各部分的作用如下：

| 部分 | 作用 |
|---|---|
| 符号表 | 将 Hypervisor、Guest 内核和用户程序的函数名称解析为目标位置 |
| `printk.o` | 提供命中后的 eBPF 计数和输出逻辑，可绑定到不同观测点 |
| `axebpf_integration_demo` | 在 Guest 中产生可重复的函数调用 |
| AxVisor 与 axebpf | 协同完成地址翻译、异常处理、探针管理及程序执行 |

目前已跑通七类观测能力：

| 能力 | 联调目标 |
|---|---|
| hprobe / hretprobe | Hypervisor 的 `notify_guest_vmexit` 入口与返回 |
| guest-kprobe | Guest 内核的 `gic_handle_irq` |
| guest-kretprobe | Guest 内核的 `__arm64_sys_getpid` 返回 |
| uprobe / uretprobe | demo 的 `demo_user_entry` 入口与 `demo_user_compute` 返回 |
| tracepoint | `vmm:timer_tick` |

运行期日志包含六类动态探针的命中和计数累加记录，Guest demo 正常执行结束。联调验证了真实虚拟化环境中的集成路径；模块测试则提供更细粒度的内部行为检查。

### 2.3 阅读并梳理 axebpf 源码

以 `trace hprobe` 为入口，梳理了从命令分发到 eBPF 执行的主要流程：

```text
shell 命令分发
    → symbols 查询函数地址
    → hprobe manager 注册目标与程序 ID
    → 底层 kprobe 库安装 BRK 断点和原指令槽
    → CPU 触发异常，进入 axebpf handler
    → 调用探针回调，按程序 ID 执行 eBPF
    → 执行原指令，恢复原函数运行
```

重点理解了以下机制：

- **符号定位：** 构建工具生成与 `ksym` 解析格式一致的压缩符号表，`symbols` 模块提供名称与地址查询；启动时加载的是 Hypervisor 自身的符号。
- **探针设置：** shell 调用 `trace_hprobe()`，manager 保存探针信息，底层库配合平台操作修改指令、处理权限和缓存。
- **命中处理：** handler 根据 `BRK #4/#6` 区分入口命中与指令槽执行完成阶段，再根据 PC 定位探针。
- **程序执行：** hprobe 入口回调将 `PtRegs` 现场交给 runtime；runtime 注册 helpers，并通过 rbpf 解释执行绑定的字节码。

## 3. 当前问题与后续计划

1. **梳理探针模块职责并评估重构。** `hprobe`、`kprobe` 和 `uprobe` 中部分代码较长，需要进一步区分注册管理、指令修改和异常处理的边界。`hprobe` 使用 `KprobeRegistry` 是值得分析的命名与抽象问题，不能仅凭名称认定实现有误。
2. **改善测试的独立性。** 为继承依赖提供明确来源，将预编译 eBPF 测试资源纳入独立可复现的准备流程，降低对 AxVisor workspace 布局的依赖。
3. **结合测试开展修改。** 对发现的具体问题先补充回归用例，再进行修复或重构，并复核受影响的手工联调路径。

## 4. 阶段总结

本阶段完成了模块测试和主线手工联调复现，并初步梳理了 hprobe 设置、命中和执行 eBPF 的源码链路。目前成果以验证和机制理解为主，尚未完成架构重构或缺陷修复；下一阶段将围绕模块职责和测试独立性推进具体改进。

## 附件

- [week3 周报](weekly-reports/week3.md)
- [联调探针状态截图](weekly-reports/trace_list.png)
- [手工联调运行日志](weekly-reports/week3.log)
- [模块测试验证日志](midterm-artifacts/full-tests.log)
