# axtrace 虚拟字符设备实验

已在当前联调 Linux 镜像和 AxVisor 上跑通：

```text
demo_submit(request, fd)
  -> write(fd, &request, 8)
  -> /dev/axtrace 的 axtrace_write()
  -> guest axtrace_submit(request)
  -> hvc #0
  -> AxVisor Hypercall 分支
  -> host axtrace_submit(request, vm_id)
  -> 返回 0 -> write 返回 8
```

这是最小的 guest→host 消息设备，没有磁盘、DMA、共享内存和异步队列。
本次验证的是设备和 hypercall 通路；日志由 C/Rust 代码直接输出，**尚未证明
同一份 eBPF 程序能在三个探针上采集这条链路**。

## 已验证结果

- `use_hypercall=0`：普通 guest 启动，模块加载、三次 write、错误长度拒绝、模块卸载均成功。
- `use_hypercall=1`：AxVisor 内启动 guest，请求 41、42、43 均按以下顺序完成，自动断言通过：

```text
axtrace: USER request=41
axtrace: KERNEL request=41 hypercall=1
axtrace: HOST request=41 vm=1
axtrace: COMPLETE request=41 status=0
axtrace: USER_DONE request=41 bytes=8
```

最后输出 `axtrace: PASS` 和 `axtrace: UNLOAD PASS`。
完整本地日志在 `../../target/axtrace/hypercall.log`，实验镜像在
`../../target/axtrace/rootfs-hvc.img`（均为忽略的生成产物）。

## 协议与代码

- `client.c`：静态链接用户程序，发送三个 64 位请求 ID。
- `axtrace.c`：miscdevice 驱动，注册 `/dev/axtrace`，权限 `0600`；仅接受 8 字节 write。
- `launch.sh`：加载模块、打印模块符号与 MODULE_READY，默认等待 30 秒后执行 client，保留模块。
  `--attach-wait 0 --warmup-wait 0` 制作的镜像使用原自动执行并卸载流程。
- `guest.toml`：沿用现有单 vCPU guest 配置，使用 `/sbin/init`。
- `axvm-hypercall.patch`：对邻接仓库 `axvisor-ebpf/modules/axvm` 的修改副本。
  当前工作区已经应用，不需要重复应用。

私有实验协议：HVC 的 `x0=0x41585401`、`x1=request_id`；返回 `x0=0` 表示成功。
它不是现有标准 hypercall。宿主从正在运行的 vCPU 获取 VM ID，不由 guest 指定；
不读取 guest 提交的指针。未知 hypercall 返回 `-38`（ENOSYS）。原 PSCI 路径不变。

宿主实际修改文件：
`axvisor-ebpf/modules/axvm/src/arch/aarch64/cpu.rs`。
增加不内联的 `axtrace_submit`，替换原通用 Hypercall 分支的 `todo!()`，并将结果写回 x0。
HVC 返回沿用已有异常处理路径，没有额外递增 PC。

## 构建

以下命令从 axebpf 仓库根目录执行。需要交叉 GCC、make、flex、bison、QEMU、
e2fsprogs（debugfs/e2fsck）以及 Python pexpect。

本次 guest 内核版本是 `6.18.0-rc6-gfd95357fd8c6-dirty`。使用对应上游提交的源码，
从镜像提取实际 `.config`，恢复相同 release 后构建外部模块。镜像的 `dirty`
本地修改未能完全恢复，兼容性由当前镜像上的实际加载/运行验证，而非仅凭版本字符串保证。

```sh
curl -L --fail https://codeload.github.com/torvalds/linux/tar.gz/fd95357fd8c6 \
  -o /tmp/axtrace-linux.tar.gz
mkdir -p /tmp/axtrace-linux
tar -xf /tmp/axtrace-linux.tar.gz -C /tmp/axtrace-linux --strip-components=1
sh demos/axtrace/build.sh /tmp/axtrace-linux \
  ../axvisor-ebpf/tmp/axebpf-manual/images/linux-qemu-aarch64.bin
```

内核启用 MODULES，关闭 MODVERSIONS 和 MODULE_SIG。没有发行方的 Module.symvers，
所以构建使用 `KBUILD_MODPOST_WARN=1`，会报告缺少符号表的警告；加载时仍由内核正常解析
和校验符号，没有强制加载。已实测加载成功。换内核镜像需要重新确认配置和兼容性。

宿主侧在 `axvisor-ebpf` 根目录运行 `cargo xtask build`。
在新的未修改 axvm 检出中，可先进入 `modules/axvm`，对补丁执行 `git apply --check` 再应用。

## 制作独立镜像与验证

`prepare_image.py` 拒绝覆盖已有输出镜像。重复实验可以直接复用现有镜像；
更新模块/client 后应换一个输出文件名。

```sh
mkdir -p target/axtrace
python3 demos/axtrace/prepare_image.py \
  ../axvisor-ebpf/tmp/axebpf-manual/images/rootfs-linux-axebpf-unified-context.img \
  target/axtrace/rootfs-hvc.img \
  --client demos/axtrace/axtrace-client --module demos/axtrace/axtrace.ko --hypercall 1 --attach-wait 0 --warmup-wait 0

python3 tests/axtrace_smoke.py --axvisor \
  --kernel ../axvisor-ebpf/target/aarch64-unknown-none-softfloat/release/axvisor.bin \
  --image target/axtrace/rootfs-hvc.img --log target/axtrace/hypercall.log
```

测试脚本自动启动 QEMU，在 AxVisor shell 执行：

```text
vm create /vmconfigs/axtrace.toml
vm start 1
```

随后断言三个请求的 USER/KERNEL/HOST/COMPLETE/USER_DONE 顺序以及成功返回。
QEMU 使用临时 qcow2 overlay，不改动输入镜像，测试结束关闭 QEMU。
仅验证驱动时用 `--hypercall 0` 制作另一份镜像，测试命令不加 `--axvisor`，
并将 `--kernel` 改为 Linux 的 `linux-qemu-aarch64.bin`。

镜像制作时先在副本上执行 e2fsck，处理残留 journal，再离线写入并逐字节回读校验。
否则旧 journal 可能在启动时恢复旧 inode 元数据，导致脚本或 vmconfig 被截断。

## 下一阶段：三个探针共用采集程序

已预留的挂载点：

| 层次 | 函数 | 入口参数 x0 |
|---|---|---|
| guest 用户态 uprobe | `demo_submit` | request ID |
| guest 内核 kprobe | 模块中的 `axtrace_submit` | request ID |
| host hprobe | axvm 中的 `axtrace_submit` | request ID |

可让同一个 eBPF 程序读取 TraceContext 的 `probe_type`、`timestamp_ns`、`regs[0]`，
用 request ID 关联三层事件。不能拿现有 printk 的累计计数当作此项验证。

client 已具备预热与等待窗口。仍需联调验证：从加载后的 `/proc/kallsyms`
解析模块函数地址（不在原 Linux 符号表中）；为新 client 加载用户符号；挂载三类探针，
采集并核对 eBPF 输出。模块卸载前必须先移除其上的 guest 探针；挂探针时不要使用
两个等待均为 0 的自动卸载模式。宿主 Rust 符号和模块地址均应从本次构建/运行中获取。

### 模块地址获取与挂载等待窗口

制作镜像时省略 `--attach-wait`，默认等待 30 秒；手动操作可设为
`--attach-wait 120`。启动后先输出 `/proc/kallsyms` 中的 `axtrace_submit` 等符号，
随后输出 `axtrace: MODULE_READY wait_seconds=120`，等待结束才启动 client。
client 成功结束后输出 `axtrace: DONE module retained; detach probes before manual rmmod`，
不自动卸载模块。此配置保存在 guest 的 `/etc/axtrace-attach-wait`。

这是固定等待窗口，不是“探针就绪”握手。此时 client 尚未启动，不能把 MODULE_READY
当作 uprobe 已就绪。之后 client 在同一进程调用 demo_submit(0, fd) 预热，打印
`CLIENT_READY pid=... wait_seconds=60`，默认再等 60 秒，最后打印 REQUESTS_BEGIN，
发送正式请求 41/42/43。用 `--warmup-wait N` 配置第二个窗口；配置写入
`/etc/axtrace-warmup-wait`。设为 0 时跳过预热和第二次等待。

在 CLIENT_READY 窗口执行 `trace list`，检查 demo_submit 对应实例的 STATE 是否为
active；CLIENT_READY 仅表示预热完成，不表示探针已激活。请求 0 可能被部分或全部探针
采集，在 trace path 中作为预热数据排除，正式验收仍只看 41/42/43。
任一等待非零时保留模块；只有 `--attach-wait 0 --warmup-wait 0` 才自动卸载。

检查等待模式可给 `tests/axtrace_smoke.py` 同样传入 `--attach-wait N --warmup-wait M`；脚本会检查两个等待
窗口，以及预热请求和正式请求的顺序，并等待“模块保留”标记，而不是卸载标记。

### 查看 eBPF 采集的路径

AxVisor shell 已增加 `trace path <PROG_ID>`。加载 axtrace.o 并挂载探针，等请求结束后执行：

```text
trace path 0
```

`0` 换成实际加载的程序 ID。命令直接读取该程序的 `AXTRACE_EVENTS` map，不要求程序
挂在静态 tracepoint 上，也不读取框架自身的 hit 计数。按请求 ID 排序，并以
USER/KERNEL/HOST 的固定顺序显示 first_ns、last_ns、hits、vm、vcpu。

只有三层齐全、每层 hits=1、时间戳非零且顺序正确时才显示
`PASS: USER -> KERNEL -> HOST`。缺失显示 MISSING/INCOMPLETE，重复命中显示
HITS_NOT_ONE，时间逆序显示 TIME_ORDER_ERROR。零 VM/vCPU 值原样输出，可能表示没有归属信息。
它展示的是累计 map 快照，不是完整实时事件流；请在请求执行完后查看。
整条请求完全没有采到时不会出现在 map 中，还需与预期的 41/42/43 对照。

实现位置：axvisor-ebpf 的 `kernel/src/shell/commands/trace.rs` 和 `axtrace_view.rs`。
解码测试在该仓库根目录 `tests/axtrace_view.rs`，可用 rustc --test 单独运行。
`axebpf/tests/axtrace_path_smoke.py` 使用实际 axtrace.o 和宿主探针验证 HOST 记录展示，
并确认未挂的 USER/KERNEL 两层被报告为缺失；这项测试不代表三层联调完成。

guest.toml 沿用原联调的控制台配置，不显式设置 `console=ttyAMA0`，避免 guest
接管串口输入后宿主 shell 无法交互。此前制作的镜像不会随源码自动更新，需重新制作。

本次实际验证日志：`target/axtrace/path-command-final.log`，请求 41/42/43 均显示 HOST
单次命中和 USER/KERNEL 缺失。另一次试验在执行 `trace unhprobe` 后等待提示符超时
（`target/axtrace/path-command-console.log`）；原因尚未定位。当前 smoke test 在读取结果后
结束整个 QEMU 实例，不将在线拆除探针算作已通过的验证。

### 双等待镜像

```sh
python3 demos/axtrace/prepare_image.py \
  ../axvisor-ebpf/tmp/axebpf-manual/images/rootfs-linux-axebpf-unified-context.img \
  target/axtrace/rootfs-warmup.img \
  --client demos/axtrace/axtrace-client --module demos/axtrace/axtrace.ko \
  --hypercall 1 --attach-wait 60 --warmup-wait 60
```

client 修改后必须重新编译并使用新镜像；旧镜像不会自动更新。
用户函数地址也可能变化，请重新生成并加载对应的 axtrace-client.syms。
两个窗口都是定时等待，不会阻止在探针仍为 pending 时发送正式请求。

### 显式 nanosleep 重试验证

client 的第二个等待窗口已改为 `syscall(SYS_nanosleep, ...)`，并在 EINTR 时继续等待
剩余时间。原因是当前 runtime observer 在 `__arm64_sys_nanosleep` 入口重试激活；
libc 的 sleep/nanosleep 在当前二进制中会走 clock_nanosleep，无法触发该观察点。
这只修改 demo，不是框架的通用激活机制修复。

新手工镜像：`target/axtrace/rootfs-nanosleep-manual.img`，两个窗口各 60 秒，包含
匹配的新 client、符号表和 axtrace.o。当前 demo_submit 地址为 0x400cf0。
仍在使用旧镜像时不会生效。

已观察到：首次 mmap 激活仍报 invalid L0 descriptor，但在预热后的 SYS_nanosleep
入口输出 `count=1 reason=runtime_retry`，随后正式请求产生 demo_submit 的 uprobe 命中。
证据日志：`target/axtrace/nanosleep-three-layer-v2.log`。该次自动测试在启动后的
guest kprobe 命令交互上超时，未完成三层 map 验收，不能将此日志当作三层全部通过。
三层自动验收脚本位于 `tests/axtrace_three_layer_smoke.py`。
