# 基于 eBPF 的 AxVisor 动态观测机制分析与实验验证

## 课程中期报告

- 项目仓库：`axvisor-ebpf`
- 研究对象：`modules/axebpf` 及其与 AxVisor 的集成机制
- 当前阶段：手工联调复现、模块测试验证与核心实现分析

## 摘要

虚拟机监控器需要同时观察自身运行状态、Guest 内核行为与 Guest 用户程序执行情况。静态日志能够记录预先选定的事件，但对动态选择函数、获取调用参数和分析返回路径的支持有限。AxVisor 中的 axebpf 模块结合静态 tracepoint、动态探针和 eBPF 解释执行，为上述观察需求提供基础设施。

本阶段主要完成已有手工联调流程的复现、axebpf 模块测试的运行，以及符号表加载、地址翻译、异常分发和事件处理机制的代码分析。宿主机测试结果为：默认功能配合 `test-utils` 时有 120 个测试通过；进一步启用 hprobe、guest-kprobe 和 guest-uprobe 后，有 259 个测试通过，均无失败，另有 4 个文档测试被忽略。两组结果属于不同 feature 配置，不能相加。

本阶段工作的重点是复现与理解已有实现，尚未将架构重构、缺陷修复或性能优化作为已完成成果。后续计划围绕事件消费、程序标识生命周期、helper 缓冲区和探针资源管理开展回归测试与针对性改进。

**关键词：** AxVisor；eBPF；动态探针；虚拟机监控器；测试验证

## 1. 研究背景与目标

AxVisor 是虚拟机监控器，其运行涉及 VM 创建与销毁、Guest 执行、异常处理、内存映射和设备事件等过程。仅依靠固定日志，难以灵活地选择观察目标，也不便于将观测数据交给可替换的程序处理。

axebpf 在 Hypervisor 内提供 eBPF 运行环境，并连接三类观测位置：Hypervisor 自身、Guest 内核和 Guest 用户态。eBPF 程序可以接收探针上下文，通过 helper 操作 map 或输出信息；模块自身还提供事件缓冲和统计，供 AxVisor shell 查询。

本阶段目标包括：

1. 复现 axebpf 与 AxVisor 的手工联调流程，理解各类观测能力的使用方式。
2. 独立运行 axebpf 模块测试，明确测试配置、覆盖范围及其局限。
3. 梳理初始化、符号解析、地址翻译、异常处理和 eBPF 执行之间的调用关系。
4. 从现有代码中识别后续优化方向，为期末阶段的实现和验证建立依据。

## 2. 模块架构与关键原理

### 2.1 axebpf 与 AxVisor 的职责边界

AxVisor 提供 VM 实体、Guest 内存映射和真实异常入口；axebpf 管理探针、判断命中、构造上下文、执行 eBPF，并维护事件及统计数据。

| 部分 | 主要职责 | 代表代码 |
|---|---|---|
| 初始化与符号查询 | 解析内核符号表，提供名称与地址查询 | `lib.rs`、`symbols.rs`、`guest_symbols.rs` |
| 静态观测 | 注册预设 tracepoint，记录事件与耗时 | `tracepoint.rs`、`tracepoints/` |
| 动态探针 | 注册、启用、命中处理、单步恢复和卸载 | `probe/hprobe/`、`probe/kprobe/`、`probe/uprobe/` |
| eBPF 执行 | 加载字节码或 ELF，处理重定位，解释执行 | `runtime.rs` |
| helper 与 map | 为 eBPF 提供内存、时间和数据存储操作 | `helpers.rs`、`maps.rs`、`map_ops.rs` |
| 上下文与事件 | 定义输入 ABI、缓冲事件、更新统计 | `context.rs`、`event.rs` |
| AxVisor 适配 | 提供地址翻译和权限修改 hook，分发异常 | `kernel/src/vmm/mod.rs`、`modules/arm_vcpu/src/exception.rs` |

整体调用关系如下：

```text
AxVisor shell：加载程序、加载符号、挂载/卸载探针
                         ↓
                 axebpf 探针管理
                         ↓
           指令断点 / Stage-2 执行权限设置
                         ↓
Guest 或 Hypervisor 执行目标位置，产生异常
                         ↓
       arm_vcpu 异常入口 → axebpf handler
                         ↓
         上下文构造 → eBPF runtime → helper / map
                         ↓
              事件、统计与 shell 展示
```

静态 tracepoint 则由代码中的调用点直接触发，不需要先产生断点异常。静态事件可按名称执行绑定程序，动态探针还会按其保存的程序 ID 调用 runtime。

### 2.2 符号表与启动初始化

动态函数探针需要先将名称解析为地址。仓库通过构建工具从内核 ELF 中提取符号，生成压缩二进制符号表，并写入预留的 `.kallsyms` 区域。

链接脚本定义 `_kallsyms_start`、`_kallsyms_end`、`_stext` 和 `_etext`，分别标记符号表区域与代码区间的边界。这些是链接符号，不是在 Rust 中额外分配的四个变量。Rust 使用 `extern` 声明引用它们，启动时取得地址，构造覆盖符号表区域的字节切片。

符号表包含符号数量、地址数组、压缩名称及类型、名称偏移、名称排序索引、压缩词表和词表索引。`KallsymsMapped::from_blob()` 按格式解析各部分，构造借用原始数据的查询视图，而非在初始化时完整解压所有名称。

`init_with_symbols()` 先初始化全局符号表，再根据 feature 初始化 tracepoint、runtime 和事件 RingBuf。符号表主要服务于 Hypervisor 名称/地址查询；Guest 内核和 Guest 用户程序的符号通过独立路径加载。

当前初始化接口返回 `()`，符号表解析失败时记录日志后仍继续执行其他初始化步骤，因此“初始化流程结束”与“所有子系统均成功”需要区分。

### 2.3 七类观测能力

| 类型 | 目标位置 | 主要机制 |
|---|---|---|
| tracepoint | Hypervisor 预设调用点 | 静态插桩与事件处理 |
| hprobe | Hypervisor 函数入口 | 指令断点及底层 kprobe 库处理 |
| hretprobe | Hypervisor 函数返回 | 返回探针与跳板机制 |
| guest-kprobe | Guest 内核函数入口 | 断点注入或 Stage-2 执行权限异常 |
| guest-kretprobe | Guest 内核函数返回 | 返回地址断点、返回栈及引用计数 |
| uprobe | Guest 用户函数入口 | 文件符号与进程映射匹配、断点注入 |
| uretprobe | Guest 用户函数返回 | 用户态返回现场和返回地址断点管理 |

在 AArch64 环境下，Hypervisor、Guest 内核和 Guest 用户态分别对应 EL2、EL1 和 EL0。eBPF 程序在 Hypervisor 内的 runtime 中执行，Guest 产生被观察的行为。

### 2.4 Guest 地址翻译与执行恢复

Guest 函数地址不能直接当作 Host 指针访问。内核探针主要基于 TTBR1，用户探针主要基于 TTBR0，完成如下转换：

```text
Guest 虚拟地址 GVA
    → 遍历 Guest 页表得到 GPA
    → 通过 AxVisor 映射得到 HPA
    → 转换为 Hypervisor 可访问的 HVA
```

断点注入模式保存原指令并写入断点。命中后，异常入口将现场交给 axebpf，handler 处理事件及程序执行，再通过单步等路径执行原指令并恢复探针。Stage-2 模式通过限制执行权限捕获取指异常，需要处理目标地址与同一映射范围内其他地址的区别。

handler 返回处理决定，`arm_vcpu` 负责实际寄存器和硬件单步设置。这一分工使探针状态逻辑与真实 CPU 异常入口相互衔接。

### 2.5 用户态探针与进程生命周期

用户态 ELF 中的符号位置需要结合运行时映射才能确定实际 PC。uprobe 因而区分“文件路径与符号构成的待激活模板”和“某个地址空间中实际生效的实例”。

`linux_runtime_observer` 借助隐藏的 Guest 内核探针观察 exec、mmap 和地址空间销毁等过程，驱动实例激活与清理。这也是用户态探针依赖 Guest 内核观测能力的重要原因。

### 2.6 eBPF 执行与事件处理

`runtime` 支持原始 eBPF 字节码和 ELF。ELF 路径使用 `aya-obj` 解析对象、创建 map，并处理 map 引用和函数调用重定位；执行路径使用 rbpf 解释器。

`TraceContext` 是程序的输入布局，包含时间、VM 信息、探针类型、参数和部分寄存器。`TraceEvent` 是供事件缓冲及展示使用的记录。map 保存程序计算的数据，事件系统同时维护模块自身的命中和耗时统计。

因此，验证时应区分“探针产生事件”“绑定的 eBPF 程序被调用”和“程序输出符合预期”三个层次，不能仅凭命中日志推断所有程序语义均正确。

## 3. 实验环境与方法

宿主机测试复核环境如下，版本信息记录于报告整理时：

| 项目 | 配置 |
|---|---|
| 宿主机 Rust target | `x86_64-unknown-linux-gnu` |
| Rust 工具链 | `nightly-2025-12-12`，rustc `1.94.0-nightly` |
| Cargo | `1.94.0-nightly` |
| 本机 QEMU | `8.2.2` |
| AxVisor 构建 target | `aarch64-unknown-none-softfloat` |
| 构建功能配置 | `hprobe`、`guest-kprobe`、`guest-uprobe`、`fs` 等 |
| 报告整理时仓库 HEAD | `c5e2c3b`，工作区包含已有未提交修改 |

手工联调采用仓库指南中的 QEMU AArch64、AxVisor shell 与 Linux Guest 流程。指南配置为 Cortex-A72、GICv3、4 个 QEMU CPU 和 8 GB 内存；Guest 配置文件采用 `linux-qemu-smp1-axebpf-integration.toml`。这里的 QEMU CPU 数与 Guest vCPU 数不应混为一谈。

实验分成两个层次：真实虚拟化运行环境用于验证集成路径；宿主机模块测试用于验证数据结构、API 和模拟环境下的探针状态转换。

## 4. 已完成工作与结果

### 4.1 手工联调复现

已完成 axebpf 与 AxVisor 的手工联调复现。流程参考仓库的《Axebpf 主线手工联调指南》，主要步骤包括：

1. 构建 AxVisor，准备 Linux Guest 镜像、Guest 符号文件和 eBPF 对象文件。
2. 从本次构建的 ELF 查询 `notify_guest_vmexit` 的实际符号名，避免沿用旧构建的地址或名称。
3. 进入 AxVisor shell，创建 VM，加载 Guest 内核和 demo 用户程序的符号。
4. 加载 `printk.o`，按实际返回的程序 ID 绑定 tracepoint 和动态探针。
5. 在启动 Guest 前完成挂载，再启动 demo，观察入口、返回和 Guest 执行日志。

联调链路中，各执行产物的作用不同：

| 产物 | 执行环境 | 作用 |
|---|---|---|
| `axvisor.bin` | QEMU 模拟的 AArch64 CPU | 运行 Hypervisor 和其中的 axebpf |
| Linux Guest 内核 | Guest vCPU | 提供内核探针目标和用户程序运行环境 |
| `axebpf_integration_demo` | Guest Linux 用户态 | 产生可重复观察的函数调用等行为 |
| `printk.o` | Hypervisor 内的 axebpf runtime | 在观测点触发后执行 eBPF 计数和输出逻辑 |

`printk.o` 是包含 eBPF 指令和相关元数据的 ELF 目标文件，不是直接由 Guest CPU 执行的 AArch64 应用。会话中讨论的加载结果包含 `COUNTER_MAP` 和程序 ID 0，随后通过该程序 ID 将同一个程序绑定到不同观测点。实际复现时，应以本次加载返回的 ID 为准。

以下为该链路的代表性命令，假设加载所得程序 ID 为 0：

```text
trace load file /vmimages/printk.o shell:shell_command
trace enable vmm:timer_tick
trace load prog 0 vmm:timer_tick
trace hprobe <本次 ELF 中 notify_guest_vmexit 的符号名> 0
trace hretprobe <本次 ELF 中 notify_guest_vmexit 的符号名> 0
trace kprobe vm1:gic_handle_irq 0 --inject
trace kprobe vm1:__arm64_sys_getpid 0 --inject --ret
trace uprobe vm1:/usr/bin/axebpf_integration_demo:demo_user_entry 0
trace uprobe vm1:/usr/bin/axebpf_integration_demo:demo_user_compute 0 --ret
```

这次主线联调的 Guest 内核探针命令使用 `--inject`，对应断点注入；不能据此宣称已经手工验证 Stage-2 模式。hprobe 与 hretprobe 则同时挂到 `notify_guest_vmexit`，分别观察入口和返回。仅有入口/返回日志不能自动得到调用耗时，还需要匹配同一次调用并计算时间差。

主线指南选取的观察对象如下：

| 能力 | 目标 | 验收时关注的证据 |
|---|---|---|
| tracepoint | `vmm:timer_tick` | 事件流或对应 map 数据变化 |
| hprobe / hretprobe | `notify_guest_vmexit` | 入口与返回两类命中日志 |
| guest-kprobe | `gic_handle_irq` | Guest 内核入口命中日志 |
| guest-kretprobe | `__arm64_sys_getpid` | Guest 内核返回命中日志 |
| uprobe | `demo_user_entry` | 用户函数实际命中日志 |
| uretprobe | `demo_user_compute` | 返回点设置与实际返回命中日志 |

联调需要特别区分模板和运行实例：Guest demo 退出后，用户态探针实例被清理，列表中的模板可能重新显示为 pending，这本身不是联调失败。完整 Guest 流程应结合 `demo:start`、运行期命中记录和 `demo:done` 判断；收尾 shell 快照仅在终端仍可交互时获取。

证据解读时，`[hprobe] ENTRY/EXIT`、`guest_uprobe_hit` 等主要是框架自身的命中日志；程序相关计数与输出还需结合 eBPF 输出和 map 数据判断。eBPF 程序负责“执行什么”，探针或 tracepoint 负责“何时触发”，单纯加载程序不会使其自动持续运行。

本报告依据已完成联调的说明、指定 Codex 会话和仓库指南整理流程。该会话主要记录机制讨论，完整个人联调原始日志未随本报告归档，因此不列出未经核对的具体命中次数，也不将指南中的样例输出作为个人实测数据。

### 4.2 axebpf 模块测试

默认功能测试命令：

```bash
cargo test -p axebpf --locked --offline \
  --features test-utils -- --test-threads=1
```

扩展探针功能测试命令：

```bash
cargo test -p axebpf --locked --offline \
  --features "test-utils,hprobe,guest-kprobe,guest-uprobe" \
  -- --test-threads=1
```

| 配置 | 通过 | 失败 | 忽略 |
|---|---:|---:|---:|
| 默认功能 + `test-utils` | 120 | 0 | 4 个文档测试 |
| 默认功能 + `test-utils,hprobe,guest-kprobe,guest-uprobe` | 259 | 0 | 4 个文档测试 |

`test-utils` 提供集成测试所需的清理和测试辅助接口。直接运行未加该 feature 的默认测试时，`attach_tests` 会因为相关接口被条件编译排除而报错。

测试使用 `--test-threads=1`，降低同一测试程序中共享全局状态造成的相互干扰。`--locked` 固定依赖解析，`--offline` 利用本地已缓存依赖。ELF 测试还依赖当前已经准备好的 `target/bpf/printk.o`、`hprobe_entry.o` 和 `hprobe_exit.o`，新环境需要先准备这些文件。

### 4.3 测试覆盖范围

259 个通过的测试按功能归类如下：

| 类别 | 数量 | 主要覆盖内容 |
|---|---:|---|
| eBPF runtime | 16 | 字节码合法性、基础执行、上下文、加载卸载、ELF 加载 |
| Maps | 15 | 创建、增删改查、容量限制、LRU 淘汰、参数检查 |
| Helpers | 24 | 标准 helper、map 操作、Hypervisor 上下文相关 helper |
| 程序 attach | 14 | 挂载、卸载、重复挂载、无效程序、状态清理 |
| Tracepoint、事件与统计 | 34 | API、事件布局、命中/耗时统计、直方图、平台 mock |
| Hypervisor 符号表 | 12 | 未初始化、非法输入、边界调用与错误信息 |
| hprobe / hretprobe | 4 | 入口与返回共存、卸载、采样控制、返回事件 |
| Guest 内核探针 | 57 | 地址翻译、Guest 符号、断点与权限状态、单步、返回栈 |
| Guest 用户态探针 | 83 | 映射与激活、进程隔离、用户内存、返回探针、实例清理 |
| **合计** | **259** | |

Guest 探针测试通过 mock 页表、内存和平台 hook 检验状态逻辑，覆盖挂载失败回滚、断点恢复、返回地址引用计数、进程隔离等场景。它们没有在测试过程中启动真实 Guest，因此不能替代硬件单步、TLB/cache、多核并发与真实异常路径验证。

部分符号表测试只检查接口调用不崩溃或错误信息，没有使用有效符号数据检验精确查询结果。ELF 加载测试也不等价于验证所有已加载程序的完整运行语义。测试通过是后续修改的回归基线，而非完整正确性证明。

### 4.4 本阶段个人工作边界

本阶段完成的是已有系统的环境复现、测试运行、结果整理与代码机制分析。对链接脚本、kallsyms 注入、`init_with_symbols()`、Guest 地址翻译及异常恢复的理解，为后续独立修改建立了基础。

目前尚未完成新的探针机制、性能优化或架构重构，因此不将仓库已有功能计为本阶段自行实现的成果。

## 5. 发现的问题与改进方向

代码分析发现以下值得进一步验证的问题。除测试配置问题外，下表属于静态分析线索，尚未全部通过专门用例复现。

| 方向 | 当前实现中的现象 | 后续验证与改进 |
|---|---|---|
| 事件消费 | RingBuf 写入后仍保留 fallback 副本，消费接口依次读取两者 | 验证重复返回，明确备份与消费语义 |
| RingBuf 生命周期 | 重复初始化时覆盖全局 FD，未显式释放旧 map | 测试重复初始化，设计幂等初始化或受控替换 |
| 程序 ID 生命周期 | 程序卸载后槽位可复用，挂载保存数字 ID | 测试旧引用是否执行新程序，考虑引用约束或代次标识 |
| helper 返回数据 | map lookup 使用共享缓冲区并返回指针 | 测试连续 lookup 与多核覆盖，设计执行上下文和指针生命周期 |
| 嵌套 Guest 状态 | guard 析构删除状态，没有恢复嵌套前的值 | 添加嵌套作用域测试，评估保存/恢复旧状态 |
| VM 资源清理 | 统一 cleanup 入口未显式覆盖全部 uprobe 管理状态 | 审计调用链，验证 VM 销毁和 ID 复用后的残留资源 |
| manager 职责 | uprobe/kprobe manager 同时承担注册、patch、权限和清理 | 按资源职责拆分，并明确锁顺序和回滚边界 |

这些问题与已通过测试并不矛盾：现有测试主要覆盖特定输入与串行状态转换，尚未穷尽共享缓冲区、资源复用和组合生命周期等情况。

## 6. 后续计划与验收方式

后续优先采用“回归用例复现—小范围修复—模块测试—必要的联调复核”的方式推进，避免先大规模改动再定位问题。

| 阶段 | 工作 | 验收方式 |
|---|---|---|
| 第一阶段 | 归档联调日志，补齐重复事件与重复初始化回归测试 | 保存可复现证据；确认消费与资源行为符合定义 |
| 第二阶段 | 修正程序标识生命周期和嵌套状态恢复 | 旧引用不得误执行新程序；嵌套退出恢复外层状态 |
| 第三阶段 | 设计 helper 缓冲区及多次 lookup 的语义 | 连续查询不互相覆盖；明确可写指针与 map 更新规则 |
| 第四阶段 | 梳理探针 patch、权限和 VM 清理职责 | 保持已有 API 行为，新增失败回滚及生命周期测试 |
| 第五阶段 | 复核受影响的 AArch64 联调路径，视进度测量开销 | 使用相同工作负载对照验证，单独报告性能数据 |

重构将重点分离注册表、指令 patch、Stage-2 权限管理、生命周期协调和平台操作。尤其需要明确持锁期间能否调用平台 hook，避免拆分文件后仍保留隐含锁依赖。

## 7. 阶段总结

目前已复现 axebpf 与 AxVisor 的手工联调，并建立宿主机模块测试基线。机制分析明确了：链接符号定位镜像中的符号表；平台 hook 提供 Guest 内存与权限操作；异常入口与 handler 协作恢复执行；runtime 和 helper 处理 eBPF；事件系统与 map 提供观测结果。

下一阶段将从已识别的资源生命周期和共享状态问题入手，完成可复现、可测试的小范围修复，再逐步调整模块职责。期末成果拟包括缺陷复现用例、针对性修复、架构改进说明及对应验证结果。

