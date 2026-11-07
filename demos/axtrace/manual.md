# 三层探针手工联调

当前构建的完整手工镜像：`target/axtrace/rootfs-nanosleep-manual.img`。
它包含显式 SYS_nanosleep 重试版本的 client，两个等待窗口各 60 秒，已加入匹配的：
`/vmimages/axtrace.o`、`/vmimages/axtrace-client.syms`、`/vmimages/linux-qemu-aarch64.syms`。

已验证设备往返、等待逻辑、宿主探针采集和 map 展示；下面的三层组合流程仍需本轮实际验收。

## 1. 宿主终端启动

```sh
cd /home/shxx1/Anything/26Fall/axebpf
qemu-system-aarch64 \
  -nographic -cpu cortex-a72 \
  -machine virt,virtualization=on,gic-version=3 -smp 4 -m 8g \
  -device virtio-blk-device,drive=disk0 \
  -drive id=disk0,if=none,format=raw,file=target/axtrace/rootfs-nanosleep-manual.img \
  -snapshot -append 'root=/dev/vda rw init=/init' \
  -kernel ../axvisor-ebpf/target/aarch64-unknown-none-softfloat/release/axvisor.bin
```

## 2. AxVisor shell：启动 guest 之前

```text
trace verbose on
vm create /vmconfigs/axtrace.toml
trace loadsyms vm1 /vmimages/linux-qemu-aarch64.syms
trace uloadelf vm1 /usr/bin/axtrace-client /vmimages/axtrace-client.syms
trace load file /vmimages/axtrace.o shell:shell_command
trace progs
```

以下假设提示 `Loaded program 0`；若不是，所有程序 ID 参数都替换为实际值。
程序只加载一次，三个动态探针共用这个 ID 和 map。静态 shell 挂载是当前加载接口所需，
axtrace 会忽略这种上下文。

```text
trace hprobe _RNvNtNtCsfvLR0vVeBus_4axvm4arch3cpu14axtrace_submit 0
trace uprobe vm1:/usr/bin/axtrace-client:demo_submit 0
trace list
vm start 1
```

guest 启动前 uprobe 为 pending 是正常的。Rust 符号对应当前构建；如果重新构建后找不到，
在宿主用 nm 检索 axvisor ELF 的 axtrace_submit，再替换命令中的完整符号。

## 3. 第一个 60 秒窗口：挂 guest kprobe

等模块符号和 `axtrace: MODULE_READY wait_seconds=60` 出现。
找到 `axtrace_submit [axtrace]` 行，取该行地址，不要误用 axtrace_write。
例如本次打印 `ffff80007afc0000 t axtrace_submit [axtrace]` 时执行：

```text
trace kprobe vm1:0xffff80007afc0000 0 --inject
trace list
```

以本轮输出地址为准。检查 Guest Kprobes 中对应条目已启用。日志可能覆盖提示符，
等输出稳定后按回车，再在 `axvisor:/$` 输入命令；此处不是 Linux shell。

## 4. 第二个 60 秒窗口：检查 uprobe

client 发出预热请求 0，然后打印：

```text
axtrace: CLIENT_READY pid=... wait_seconds=60; check trace list for active uprobe
```

此时执行 `trace list`。在 Guest Uprobes 中检查：

新 client 预热后应触发 `guest_uprobe_activate: ... count=1 reason=runtime_retry`，
表示 nanosleep 入口的激活重试成功。当前 demo_submit 地址是 0x400cf0，镜像内符号表已更新。

- PATH 为 `/usr/bin/axtrace-client`，SYMBOL 为 `demo_submit`，PROG_ID 正确。
- 对应运行实例 STATE 为 active，INSTANCE/RUNTIME_PC/MM 有值。
- 不是只有 waiting-for-instance 的 pending 条目。

active 仅表示框架认为探针已安装，最终仍以 map 中实际采到的 USER 记录为准。
预热请求可能漏采，不计入正式验收。等待不会因 pending 自动延长，未就绪则本轮不可判定通过。

## 5. 查看结果

等待 REQUESTS_BEGIN、USER_DONE 以及 `axtrace: DONE module retained; ...` 后执行：

```text
trace path 0
trace list
```

逐一检查 request=41、42、43：三层齐全，每层 hits=1，时间非零且 USER <= KERNEL <= HOST，
显示 `PASS: USER -> KERNEL -> HOST`。请求 0 是预热数据，单独忽略。
MISSING/INCOMPLETE、HITS_NOT_ONE、TIME_ORDER_ERROR 均需要保留日志排查，不能当作通过。
完全没有出现的正式请求也属于漏采。

## 6. 结束

本轮建议用 QEMU 的 Ctrl-a、再按 x 退出，下一轮重新启动以获得新 map。
`-snapshot` 保持实验镜像不变。暂不依赖在线 unhprobe 清理：之前曾出现等待提示符超时。
不要在 guest kprobe 仍挂载时卸载模块。
