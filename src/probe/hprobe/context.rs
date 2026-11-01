//! AArch64 hprobe input ABI adapter. The kprobe framework retains its PtRegs.

use crate::context::{REGS_VALID_CONTROL, TraceContext};
use crate::probe::ProbeType;
use crate::runtime;
use kprobe::PtRegs;

pub(super) fn build_context(regs: &PtRegs, is_return: bool) -> TraceContext {
    let probe_type = if is_return {
        ProbeType::Hretprobe
    } else {
        ProbeType::Hprobe
    };
    let mut ctx = TraceContext::new(0)
        .with_probe_type(probe_type as u32)
        .with_aarch64_gprs(&regs.regs);
    // No generic guest association or tracepoint ID is available in this callback.
    // arg0..arg3 are a read snapshot only; register writes use regs/regs_x8_x30.
    ctx.arg0 = regs.regs[0];
    if !is_return {
        ctx.arg1 = regs.regs[1];
        ctx.arg2 = regs.regs[2];
        ctx.arg3 = regs.regs[3];
    }
    ctx.sp = regs.sp;
    ctx.pc = regs.pc;
    ctx.pstate = regs.pstate;
    ctx.orig_x0 = regs.orig_x0;
    ctx.syscallno = regs.syscallno;
    ctx.unused2 = regs.unused2;
    ctx.regs_valid |= REGS_VALID_CONTROL;
    ctx
}

pub(super) fn write_back_registers(ctx: &TraceContext, regs: &mut PtRegs) {
    regs.regs[..8].copy_from_slice(&ctx.regs);
    regs.regs[8..].copy_from_slice(&ctx.regs_x8_x30);
    regs.sp = ctx.sp;
    regs.pc = ctx.pc;
    regs.pstate = ctx.pstate;
    regs.orig_x0 = ctx.orig_x0;
    regs.syscallno = ctx.syscallno;
    regs.unused2 = ctx.unused2;
}

pub(super) fn run_program(
    prog_id: u32,
    regs: &mut PtRegs,
    is_return: bool,
) -> Result<u64, runtime::Error> {
    let mut ctx = build_context(regs, is_return);
    let result = runtime::run_program(prog_id, Some(ctx.as_bytes_mut()));
    // Match the previous directly borrowed PtRegs behavior: stores made before
    // an execution error are retained too. Do this before returning to kprobe,
    // which may subsequently redirect PC or replace LR itself.
    write_back_registers(&ctx, regs);
    result
}
