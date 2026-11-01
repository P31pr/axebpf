#ifndef AXEBPF_TRACE_CONTEXT_H
#define AXEBPF_TRACE_CONTEXT_H

#include <stddef.h>
#include <stdint.h>

/* Input IDs, NOT TraceEvent output IDs. */
enum axebpf_context_probe_type {
    AXEBPF_CTX_HPROBE = 0,
    AXEBPF_CTX_HRETPROBE = 1,
    AXEBPF_CTX_KPROBE = 2,
    AXEBPF_CTX_KRETPROBE = 3,
    AXEBPF_CTX_TRACEPOINT = 4,
    AXEBPF_CTX_UPROBE = 5,
    AXEBPF_CTX_URETPROBE = 6,
};

#define AXEBPF_REGS_VALID_X0_X7 (UINT64_C(1) << 0)
#define AXEBPF_REGS_VALID_X8_X30 (UINT64_C(1) << 1)
#define AXEBPF_REGS_VALID_CONTROL (UINT64_C(1) << 2)

/* Fixed AArch64 input ABI. See docs/trace-context-abi.md for writeback rules. */
struct axebpf_trace_context {
    uint64_t timestamp_ns;
    uint32_t tracepoint_id;
    uint32_t vm_id;
    uint32_t vcpu_id;
    uint32_t probe_type;
    uint64_t arg0;
    uint64_t arg1;
    uint64_t arg2;
    uint64_t arg3;
    uint64_t regs[8];
    uint64_t regs_x8_x30[23];
    uint64_t sp;
    uint64_t pc;
    uint64_t pstate;
    uint64_t orig_x0;
    int32_t syscallno;
    uint32_t unused2;
    uint64_t regs_valid;
} __attribute__((aligned(8)));

#define AXEBPF_CTX_OFFSET(field, expected) \
    _Static_assert(offsetof(struct axebpf_trace_context, field) == (expected), \
                   "unexpected TraceContext offset: " #field)
AXEBPF_CTX_OFFSET(timestamp_ns, 0);
AXEBPF_CTX_OFFSET(tracepoint_id, 8);
AXEBPF_CTX_OFFSET(vm_id, 12);
AXEBPF_CTX_OFFSET(vcpu_id, 16);
AXEBPF_CTX_OFFSET(probe_type, 20);
AXEBPF_CTX_OFFSET(arg0, 24);
AXEBPF_CTX_OFFSET(arg1, 32);
AXEBPF_CTX_OFFSET(arg2, 40);
AXEBPF_CTX_OFFSET(arg3, 48);
AXEBPF_CTX_OFFSET(regs, 56);
AXEBPF_CTX_OFFSET(regs_x8_x30, 120);
AXEBPF_CTX_OFFSET(sp, 304);
AXEBPF_CTX_OFFSET(pc, 312);
AXEBPF_CTX_OFFSET(pstate, 320);
AXEBPF_CTX_OFFSET(orig_x0, 328);
AXEBPF_CTX_OFFSET(syscallno, 336);
AXEBPF_CTX_OFFSET(unused2, 340);
AXEBPF_CTX_OFFSET(regs_valid, 344);
#undef AXEBPF_CTX_OFFSET
_Static_assert(sizeof(struct axebpf_trace_context) == 352, "unexpected TraceContext size");
_Static_assert(_Alignof(struct axebpf_trace_context) == 8, "unexpected TraceContext alignment");

#endif
