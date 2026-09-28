# 第二周周报

## 本周目标

搭建作者配套的 AxVisor 开发环境，理解 axebpf 的集成方式，
完成基础追踪功能验证，并准备 Linux guest 联调所需材料。

## 已完成工作

### 1. 拉取完整代码

根据作者开发周报，找到配套仓库 Iscreamx/axvisor 的
feature/ebpf 分支，完成仓库及子模块克隆。

### 2. 构建并启动 AxVisor

使用项目指定的 nightly-2025-12-12 工具链，
生成 qemu-aarch64 配置并完成构建。

通过 QEMU 模拟 AArch64 环境，成功进入 AxVisor shell。
当前启动配置尚未加载 Linux guest。

### 3. 验证 axebpf 基础功能

在 AxVisor shell 中完成以下操作：

- 使用 trace list 查看追踪点。
- 启用和关闭 shell:shell_command 追踪点。
- 执行 shell 命令，通过 trace stat 观察事件统计。
- 使用 ksym search notify_guest_vmexit 查询内核符号。

观察到追踪点开关会临时修改代码页权限。

```bash
axvisor:$ trace enable shell:shell\_command
[ 74.442159 1:2 axebpf::page\_table:160] page\_table: set\_kernel\_text\_writable 0xf8000008c000-0xf8000008d000 writable=true
[ 74.442322 1:2 axebpf::page\_table:169] page\_table: TTBR0\_EL2 = 0x407b9000
[ 74.442567 1:2 axebpf::page\_table:160] page\_table: set\_kernel\_text\_writable 0xf8000008c000-0xf8000008d000 writable=false
[ 74.442931 1:2 axebpf::page\_table:169] page\_table: TTBR0\_EL2 = 0x407b9000
Enabled: shell:shell\_command
axvisor:$ uname -a
ArceOS 0.3.0\
axvisor:$ vm list
No virtual machines found.
axvisor:$ uname -a
ArceOS 0.3.0\
axvisor:$ trace stat
PROBE STATISTICS:
EVENT                               COUNT        MIN        MAX        AVG    LAST\_TS

---

vmm:vmm\_init                            1          -          -          -  389082688
shell:shell\_command                     7          -          -          - 89607713824
shell:shell\_init                        1          -          -          -  397567680

axvisor:$ trace disable shell:shell\_command
[ 96.676571 1:2 axebpf::page\_table:160] page\_table: set\_kernel\_text\_writable 0xf8000008c000-0xf8000008d000 writable=true
[ 96.676776 1:2 axebpf::page\_table:169] page\_table: TTBR0\_EL2 = 0x407b9000
[ 96.676925 1:2 axebpf::page\_table:160] page\_table: set\_kernel\_text\_writable 0xf8000008c000-0xf8000008d000 writable=false
[ 96.677039 1:2 axebpf::page\_table:169] page\_table: TTBR0\_EL2 = 0x407b9000
Disabled: shell:shell\_command
axvisor:$ ksym search notify\_guest\_vmexit
Found 1 symbol(s) matching 'notify\_guest\_vmexit':
0x0000f80000079240  \_RNvNtCs7u6XUW0Xy4Q\_7axvisor3vmm19notify\_guest\_vmexit
```

`trace` 命令属于 axebpf 提供的静态追踪功能，命令实现位于 axvisor 的 `kernel/src/shell/commands/trace.rs`，底层功能主要有 `modules/axebpf` 提供。

### 4. 准备 eBPF 程序和 Linux guest 联调材料

阅读 scripts/axdemo.py，梳理镜像准备流程：

下载基础镜像 → 复制并扩容 rootfs → 准备 Linux 内核符号表
→ 构建示例程序与 eBPF 程序 → 注入联调文件。

目前已成功编译：

- printk.o
- hprobe_entry.o
- hprobe_exit.o

## 遇到的问题与处理

| 问题 | 原因与处理 |
|---|---|
| axalloc 无法继承 workspace 依赖 | 改为在作者配套的 AxVisor workspace 中使用 axebpf |
| phytium-mci 下载失败 | 旧仓库地址不可访问，改用 drivercraft 地址，并保留相同提交 |
| bpf-linker 安装时报找不到 LLVM | 使用官方预编译二进制 |

## 当前验证范围

已验证 AxVisor 启动、追踪点开关、事件统计和符号查询，
并完成三个 eBPF 程序的构建。

尚未确认完整 Linux guest 七类探针联调通过，
也未完成 axebpf/tests 下的全部 Rust 测试。

## 下周计划

启动 Linux guest，先验证 tracepoint 和 eBPF 程序加载。
