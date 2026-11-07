# Context adapter validation — 2026-10-30

The checks below were rerun after reordering the header to timestamp-first and
grouping the four u32 metadata fields. The current ABI is 352 bytes with no
implicit padding; it replaces the 360-byte layout first tested on 2026-10-27.

## Results

- x86_64 library check, with `hprobe,guest-kprobe,guest-uprobe,test-utils`: passed.
- `aarch64-unknown-none-softfloat` library check with the same features: passed.
- C11 ABI header checks with host `cc` and `aarch64-linux-gnu-gcc`: passed.
- x86_64 tests: 15 passed (4 context ABI, 4 guest kprobe context, 2 guest uprobe
  context, 2 hprobe coexistence, 1 hretprobe event, 2 static event statistics).
  ABI tests use hard-coded offsets and eBPF u32/u64 loads to verify the reordered
  timestamp, each metadata field, and the register region.
- AArch64 Linux tests under QEMU user mode: 4 passed. These use the actual
  AArch64 `kprobe::PtRegs`, the adapter, and axebpf/rbpf execution. They cover
  complete register round trips, shared x0 reads, register writes versus payload
  writes, and retaining stores made before an execution error.
- Formatting and patch whitespace checks: passed.

The QEMU tests do not execute EL2 breakpoints or the hypervisor's exception
return path. Single-step/trampoline behavior and hit latency still need an
AxVisor boot/integration run. No performance improvement is claimed.

## Build environment and limits

The standalone repository inherits `axalloc` from a workspace that is absent
here. An isolated workspace at `/tmp/axebpf-context-validation` supplied that
dependency; its package links to this repository's source and tests. Repository
dependency configuration was not changed. The temporary workspace/cache was
recreated for the October 30 run; the recorded Aya revision was restored from
the local Git object database.

Resolving current Git heads exposed pre-existing API drift in `kbpf-basic`,
`ksym`, and `aya-obj`; the newest installed nightly also rejects the old
`printf-compat` VaList API. Validation therefore used the project's AxVisor
toolchain, `nightly-2025-12-12`, with previously cached compatible dependency
sources selected in the temporary manifests:

| Dependency | Revision / version |
|---|---|
| `kprobe` | `985e06a` |
| `rbpf` | `a54b100` |
| `kbpf-basic` | `e94c2ee` |
| `aya-obj` | `dd9bb52` |
| `ksym` | `83ed84a` |
| `ktracepoint` | `503e1fc` |
| `tp-lexer` | `065d96a` |
| `printf-compat` | `6a67f5b` |
| `axalloc` | ArceOS `dev-251216`, `1609656` |
| `spin` 0.10 dependency | Cached `0.10.0` path override (registry version yanked) |

Only temporary dependency manifests were redirected; dependency Rust source
was not modified for these passing checks. Commands used `--offline
--ignore-rust-version` with the temporary Cargo cache at
`/tmp/axebpf-context-cargo-cache`. These results establish compatibility with
that dependency set, not with arbitrary current upstream heads.

Representative test commands (after constructing that workspace):

```sh
export CARGO_HOME=/tmp/axebpf-context-cargo-cache
cargo +nightly-2025-12-12 test --offline --ignore-rust-version \
  --manifest-path /tmp/axebpf-context-validation/Cargo.toml -p axebpf \
  --features hprobe,guest-kprobe,guest-uprobe,test-utils \
  --test trace_context_tests --test guest_kprobe_context_tests \
  --test guest_uprobe_context_tests --test hprobe_coexistence_tests \
  --test hretprobe_event_tests --test stats_tests -- --test-threads=1

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER='/tmp/axebpf-context-validation/qemu-user/usr/bin/qemu-aarch64 -L /usr/aarch64-linux-gnu'
cargo +nightly-2025-12-12 test --offline --ignore-rust-version \
  --manifest-path /tmp/axebpf-context-validation/Cargo.toml -p axebpf \
  --target aarch64-unknown-linux-gnu \
  --features hprobe,guest-kprobe,guest-uprobe,test-utils \
  --test hprobe_context_tests -- --test-threads=1
```

QEMU was downloaded and extracted under `/tmp`; no system QEMU package was
installed. The AArch64 Linux standard-library target added to the existing
`nightly-2025-12-12` toolchain on October 27 was reused for the user-mode tests.

External `axebpf-programs` sources and old `.o` files were not available for
migration. Rebuild programs using either PtRegs or the older TraceContext
layouts with the [new ABI header](../include/axebpf_trace_context.h) before
loading them through paths using the reordered TraceContext.

The hprobe context tests were moved to `tests/hprobe_context_tests.rs` on
November 1. The command above reflects the new location; the recorded passing
AArch64 run predates this move.
