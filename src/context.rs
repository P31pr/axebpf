//! Trace context passed to eBPF programs.

use crate::platform;

/// x0-x7 are available in the AArch64 register snapshot.
pub const REGS_VALID_X0_X7: u64 = 1 << 0;
/// x8-x30 are available in the AArch64 register snapshot.
pub const REGS_VALID_X8_X30: u64 = 1 << 1;
/// SP, PC and PSTATE are available in the AArch64 register snapshot.
pub const REGS_VALID_CONTROL: u64 = 1 << 2;

/// Input-context type ID. These IDs differ from `event::PROBE_*` output IDs.
pub const PROBE_TRACEPOINT: u32 = 4;

/// Tracepoint context passed to eBPF programs.
///
/// Constructed when a tracepoint fires and passed as input data to eBPF programs.
/// Must match the definition in eBPF programs (C ABI).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct TraceContext {
    /// Trigger timestamp (nanoseconds)
    pub timestamp_ns: u64,
    /// Static tracepoint unique identifier (0 for dynamic probes).
    pub tracepoint_id: u32,
    /// VM ID (0 means not associated)
    pub vm_id: u32,
    /// vCPU ID (0 means not associated)
    pub vcpu_id: u32,
    /// Probe type
    /// (0=hprobe, 1=hretprobe, 2=kprobe, 3=kretprobe, 4=tracepoint, 5=uprobe, 6=uretprobe)
    pub probe_type: u32,
    /// Tracepoint argument 0
    pub arg0: u64,
    /// Tracepoint argument 1
    pub arg1: u64,
    /// Tracepoint argument 2
    pub arg2: u64,
    /// Tracepoint argument 3
    pub arg3: u64,
    /// Host or guest x0-x7. These are the authoritative writable register slots.
    pub regs: [u64; 8],
    /// AArch64 x8-x30, without duplicating the original x0-x7 slots.
    pub regs_x8_x30: [u64; 23],
    /// Saved stack pointer; available when REGS_VALID_CONTROL is set.
    pub sp: u64,
    /// Saved program counter; available when REGS_VALID_CONTROL is set.
    pub pc: u64,
    /// Saved processor state; available when REGS_VALID_CONTROL is set.
    pub pstate: u64,
    /// Preserved PtRegs auxiliary fields. The current hprobe TrapFrame bridge
    /// initializes these to zero; they do not describe a valid syscall context.
    pub orig_x0: u64,
    /// Preserved PtRegs syscall-number slot (not a valid hprobe syscall number).
    pub syscallno: i32,
    /// Preserved PtRegs reserved slot.
    pub unused2: u32,
    /// Available register groups (`REGS_VALID_*`). Input metadata, not a write mask.
    pub regs_valid: u64,
}


impl TraceContext {
    /// Build a static event input. Static events carry no register snapshot.
    pub fn for_tracepoint(tracepoint_id: u32, timestamp_ns: u64, duration_ns: u64) -> Self {
        Self {
            tracepoint_id,
            timestamp_ns,
            arg0: duration_ns,
            probe_type: PROBE_TRACEPOINT,
            ..Default::default()
        }
    }

    /// Create a new trace context.
    pub fn new(tracepoint_id: u32) -> Self {
        Self {
            tracepoint_id,
            timestamp_ns: platform::time_ns(),
            ..Default::default()
        }
    }

    /// Set VM information.
    pub fn with_vm(mut self, vm_id: u32, vcpu_id: u32) -> Self {
        self.vm_id = vm_id;
        self.vcpu_id = vcpu_id;
        self
    }

    /// Set arguments.
    pub fn with_args(mut self, arg0: u64, arg1: u64, arg2: u64, arg3: u64) -> Self {
        self.arg0 = arg0;
        self.arg1 = arg1;
        self.arg2 = arg2;
        self.arg3 = arg3;
        self
    }

    /// Set probe type
    pub fn with_probe_type(mut self, probe_type: u32) -> Self {
        self.probe_type = probe_type;
        self
    }

    /// Set x0-x7; preserves any other register groups already present.
    pub fn with_regs(mut self, regs: &[u64; 8]) -> Self {
        self.regs = *regs;
        self.regs_valid |= REGS_VALID_X0_X7;
        self
    }

    /// Set all AArch64 general-purpose registers. Does not supply SP/PC/PSTATE.
    pub fn with_aarch64_gprs(mut self, regs: &[u64; 31]) -> Self {
        self.regs.copy_from_slice(&regs[..8]);
        self.regs_x8_x30.copy_from_slice(&regs[8..]);
        self.regs_valid |= REGS_VALID_X0_X7 | REGS_VALID_X8_X30;
        self
    }

    /// Convert to byte slice (for passing to eBPF VM).
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self as *const Self as *const u8,
                core::mem::size_of::<Self>(),
            )
        }
    }

    /// Convert to mutable byte slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self as *mut Self as *mut u8,
                core::mem::size_of::<Self>(),
            )
        }
    }
}
