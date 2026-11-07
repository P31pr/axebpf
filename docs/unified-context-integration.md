# 统一 TraceContext 联调（2026-11-03）

本轮沿用所有探针共享 `printk` 的联调方式，只将程序读取 tracepoint_id 的偏移从 0 改为 8。动态探针仍共享 Map 键 0；本流程不验证按动态探测点分类计数或完整寄存器 ABI。

## 已准备的文件

- `../axebpf-programs/src/printk.rs`：适配偏移 8。
- `../axebpf-programs/output/printk.o`：新程序。
- `../axvisor-ebpf/target/bpf/printk.o`：同一产物。
- `../axvisor-ebpf/modules/axebpf`：同步当前功能分支的源码、测试、ABI 头文件及说明，包含 pre/ret handler 重构；未提交子模块修改。
- `../axvisor-ebpf/target/aarch64-unknown-none-softfloat/release/axvisor.bin`：已通过 `cargo xtask build` 构建。
- `../axvisor-ebpf/tmp/axebpf-manual/images/rootfs-linux-axebpf-unified-context.img`：从旧联调镜像复制，替换 `/vmimages/printk.o`，已逐字节校验；guest vmconfig 改为 `init=/sbin/init`，使用已有 `/etc/inittab` 启动 demo。原镜像保留。

这里只使用运行时文件加载，无需启用 `precompiled-ebpf`。两个旧 hprobe eBPF 程序未适配，不要在本轮使用。

已完成 QEMU 快照模式下的启动与加载检查：成功加载新 `printk.o`（1480 字节），静态 tracepoint 计数依次输出 1、2、3，`trace stat` 显示 `COUNTER_MAP` 的键为 100、值为 2。日志位于 `/tmp/axebpf-unified-smoke.log`。本次未启动 guest 或跑完七类探针联调。

## 启动

在宿主终端执行：

```bash
cd /home/shxx1/Anything/26Fall/axvisor-ebpf
nm -n target/aarch64-unknown-none-softfloat/release/axvisor | rg notify_guest_vmexit
qemu-system-aarch64 \
  -nographic -cpu cortex-a72 \
  -machine virt,virtualization=on,gic-version=3 -smp 4 -m 8g \
  -device virtio-blk-device,drive=disk0 \
  -drive id=disk0,if=none,format=raw,file=tmp/axebpf-manual/images/rootfs-linux-axebpf-unified-context.img \
  -snapshot -append 'root=/dev/vda rw init=/init' \
  -kernel target/aarch64-unknown-none-softfloat/release/axvisor.bin
```

`-snapshot` 使运行中的磁盘修改不写回准备好的镜像。宿主 AxVisor 使用 `init=/init`；guest 的启动参数在镜像中的 vmconfig 内，二者不同。

## AxVisor shell 内执行

先创建 guest、加载符号和程序，所有 attach 都放在 `vm start` 之前：

```text
trace verbose on
vm create /vmconfigs/linux-qemu-smp1-axebpf-integration.toml
trace loadsyms vm1 /vmimages/linux-qemu-aarch64.syms
trace uloadelf vm1 /usr/bin/axebpf_integration_demo /vmimages/axebpf-integration-demo.syms
trace load file /vmimages/printk.o shell:shell_command
trace progs
trace enable vmm:timer_tick
trace load prog 0 vmm:timer_tick
trace hprobe _RNvNtCsdQH8tYCRzGI_7axvisor3vmm19notify_guest_vmexit 0
trace hretprobe _RNvNtCsdQH8tYCRzGI_7axvisor3vmm19notify_guest_vmexit 0
trace kprobe vm1:gic_handle_irq 0 --inject
trace kprobe vm1:__arm64_sys_getpid 0 --inject --ret
trace uprobe vm1:/usr/bin/axebpf_integration_demo:demo_user_entry 0
trace uprobe vm1:/usr/bin/axebpf_integration_demo:demo_user_compute 0 --ret
trace list
vm start 1
```

上述命令假设新启动后程序 ID 为 0、guest ID 为 1，以实际加载/创建输出为准。hprobe 符号也以本次 ELF 的 `nm` 输出为准。

观察 `[eBPF] ... tp=shell:shell_command count=...` 是否增长，以及 hprobe ENTRY/EXIT、guest_kprobe_hits、guest_uprobe_hit、guest_uretprobe_hit 和 `demo:done`。静态计数证明新版程序能读取 ID 并更新 Map；动态命中日志是宿主输出，不等于 eBPF 结果验证。

若 demo 结束后 host shell 仍可交互，可执行：

```text
trace verbose off
trace stat
trace dump -n 20
```

退出 QEMU：`Ctrl-a` 后按 `x`。

## 重建 printk

```bash
cd /home/shxx1/Anything/26Fall/axebpf-programs
CARGO_PROFILE_RELEASE_OPT_LEVEL=2 cargo +nightly build --release --bin printk \
  -Z build-std=core --target bpfel-unknown-none --locked
cp target/bpfel-unknown-none/release/printk output/printk.o
cp output/printk.o ../axvisor-ebpf/target/bpf/printk.o
```

临时使用 O2 是因为当前 bpf-linker 不支持项目默认的 Oz。再次修改程序后还需在关机状态下更新磁盘镜像中的 `/vmimages/printk.o`；仅复制到宿主 `target/bpf` 不会更新运行时文件加载的程序。

本轮不要直接运行原 `scripts/axdemo.py`：它默认从临时缓存克隆并重建上游程序，可能重新使用偏移 0 的旧版本。上述手工流程明确使用已经更新的镜像。
