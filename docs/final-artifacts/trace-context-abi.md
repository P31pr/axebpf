# AArch64 tracing input ABI

`TraceContext` is the input for AArch64 hprobe/hretprobe, guest kprobe/kretprobe,
guest uprobe/uretprobe, and static tracepoint programs. `runtime::run_program`
continues to accept `Option<&mut [u8]>`. Other architectures' hprobe paths retain
their native `PtRegs` input; they are not covered by this migration.

## Layout

The structure is 352 bytes, aligned to 8 bytes. `timestamp_ns` comes first,
followed by four u32 metadata fields (`tracepoint_id`, `vm_id`, `vcpu_id`,
`probe_type`). This 24-byte header is followed by the u64 payload and register
fields. Grouping these fields removes both `_header_pad` and `_reserved` from
the earlier 360-byte adapter layout without introducing implicit padding.
`unused2` remains because it preserves an actual upstream PtRegs slot.

This is an intentional ABI break from both the original 128-byte TraceContext
and the earlier 360-byte adapter layout. Rust definitions and eBPF programs
must be updated together; use the constructors / `..Default::default()` for
new Rust struct literals.

Use [the C11 header](../include/axebpf_trace_context.h) for eBPF programs. Both
the header and Rust implementation assert field offsets and size. The register
layout is fixed to AArch64 even when compiling tools/tests on another host.

| Field | Offset | Meaning |
|---|---:|---|
| `timestamp_ns` | 0 | Platform time at construction, or the supplied static-event time |
| `tracepoint_id` | 8 | Static event ID; hprobe currently supplies 0 |
| `vm_id`, `vcpu_id` | 12, 16 | Existing source-specific association |
| `probe_type` | 20 | Input type ID, distinct from `TraceEvent` IDs |
| `arg0..arg3` | 24–48 | Source-specific payload, not writable register aliases |
| `regs[8]` | 56 | x0–x7 |
| `regs_x8_x30[23]` | 120 | x8–x30 |
| `sp`, `pc`, `pstate` | 304, 312, 320 | Saved control registers |
| `orig_x0`, `syscallno`, `unused2` | 328, 336, 340 | Preserved PtRegs auxiliary fields |
| `regs_valid` | 344 | Available register groups |

`regs_valid` bits are: bit 0 for x0–x7, bit 1 for x8–x30, bit 2 for
SP/PC/PSTATE. Zero-valued registers can still be valid. Static tracepoints set
no bits; guest probes currently supply GPRs only; AArch64 hprobe supplies all
three groups. The current hprobe TrapFrame bridge zeroes the auxiliary tail,
so those fields must not be interpreted as a syscall context.

Input type IDs remain hprobe=0, hretprobe=1, kprobe=2, kretprobe=3,
tracepoint=4, uprobe=5, uretprobe=6. Static tracepoint construction now uses 4.
Output event IDs are unchanged.

## hprobe construction and writeback

The adapter copies `PtRegs` immediately before eBPF execution and writes register
fields back before returning from the kprobe callback. Framework PC redirects,
LR replacement, single-step handling, and the final TrapFrame copy remain in
their existing order. The framework can still overwrite PC/LR after a callback;
the adapter does not grant new control-flow modification guarantees.

Entry inputs set `arg0..arg3` to an initial x0–x3 snapshot. Return inputs set
`arg0` to x0 and leave `arg1..arg3` zero. Writes to these payload slots, type IDs,
timestamps, VM IDs, or validity bits do not change PtRegs. Only the register
fields (including auxiliary fields) are copied back. `regs_valid` describes
availability; it is not an eBPF-controlled writeback mask.

As with the old directly borrowed PtRegs buffer, register stores performed
before a runtime error are retained. Guest probes still use snapshots without
register writeback. Static tracepoint payloads retain their previous semantics
(`arg0` is duration); guest payloads retain their address/process semantics.

hprobe supplies no guest association: VM/vCPU IDs stay zero. Adding a platform
association provider is separate work; a host probe does not inherently belong
to a guest. No physical CPU ID is substituted for a vCPU ID.

## Migrating programs

Old hprobe bytecode using PtRegs offsets is incompatible with this input.
For example, x0 moves from offset 0 to 56, and PC from 256 to 312. Update the
entry/return program definitions and accesses in `axebpf-programs`, then rebuild
`hprobe_entry.o`, `hprobe_exit.o`, and any custom hprobe objects before loading
them. The loader cannot infer the expected context ABI from arbitrary bytecode;
it does not automatically reject or translate old objects.

Programs using the old TraceContext must also be rebuilt: timestamp moves from
8 to 0, tracepoint ID from 0 to 8, probe type from 56 to 20, and x0 from 64 to 56.
Although `arg0..arg3` retain their offsets, the old prefix is no longer ABI
compatible. New programs should check validity before reading extended registers.
Existing external eBPF sources and compiled objects are not included in this
repository, so they must be migrated separately using the supplied header.
