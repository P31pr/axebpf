#![cfg(all(
    feature = "hprobe",
    feature = "runtime",
    feature = "test-utils",
    target_arch = "aarch64"
))]

use axebpf::hprobe_manager::{
    build_context_for_test as build_context, execute_probe_program_for_test as run_program,
    write_back_registers_for_test as write_back_registers,
};
use axebpf::{PtRegs, runtime};

fn registers() -> PtRegs {
    PtRegs {
        regs: core::array::from_fn(|i| 0x1000 + i as u64),
        sp: 0x2000,
        pc: 0x3000,
        pstate: 0x3c5,
        orig_x0: 0x4000,
        syscallno: -1,
        unused2: 0x55,
    }
}

fn insn(code: u8, registers: u8, offset: i16, imm: i32) -> [u8; 8] {
    let [lo, hi] = offset.to_le_bytes();
    let [a, b, c, d] = imm.to_le_bytes();
    [code, registers, lo, hi, a, b, c, d]
}

fn execute(bytecode: &[u8], regs: &mut PtRegs, is_return: bool) -> Result<u64, runtime::Error> {
    let id = runtime::load_program(bytecode, None).unwrap();
    let result = run_program(id, regs, is_return);
    runtime::unload_program(id).unwrap();
    result
}

#[test]
fn full_ptregs_round_trip_and_metadata() {
    let input = registers();
    for is_return in [false, true] {
        let ctx = build_context(&input, is_return);
        assert_eq!(ctx.probe_type, if is_return { 1 } else { 0 });
        assert_eq!((ctx.tracepoint_id, ctx.vm_id, ctx.vcpu_id), (0, 0, 0));
        assert_eq!(ctx.regs_valid, 7);
        assert_eq!(ctx.arg0, input.regs[0]);
        assert_eq!(ctx.arg1, if is_return { 0 } else { input.regs[1] });
        let mut output = registers();
        output.regs.fill(0);
        output.sp = 0;
        output.pc = 0;
        output.pstate = 0;
        output.orig_x0 = 0;
        output.syscallno = 0;
        output.unused2 = 0;
        write_back_registers(&ctx, &mut output);
        assert_eq!(output.regs, input.regs);
        assert_eq!(
            (output.sp, output.pc, output.pstate),
            (input.sp, input.pc, input.pstate)
        );
        assert_eq!(
            (output.orig_x0, output.syscallno, output.unused2),
            (input.orig_x0, input.syscallno, input.unused2)
        );
    }
}

#[test]
fn ebpf_reads_same_x0_for_entry_return_and_guest_contexts() {
    let code = [insn(0x79, 0x10, 56, 0), insn(0x95, 0, 0, 0)].concat();
    for is_return in [false, true] {
        let mut regs = registers();
        assert_eq!(execute(&code, &mut regs, is_return).unwrap(), 0x1000);
    }
    #[cfg(feature = "guest-kprobe")]
    {
        let mut ctx = axebpf::probe::kprobe::handler::build_guest_ctx_for_test(
            1,
            false,
            0,
            0,
            &registers().regs,
        );
        let program = runtime::EbpfProgram::new(&code, None).unwrap();
        assert_eq!(
            program.execute_with_context(ctx.as_bytes_mut()).unwrap(),
            0x1000
        );
    }
}

#[test]
fn ebpf_writes_registers_but_metadata_and_argument_aliases_do_not_control_writeback() {
    let mut code = Vec::new();
    for (offset, value) in [
        (56, 11),
        (296, 30),
        (304, 31),
        (312, 32),
        (320, 33),
        (328, 34),
    ] {
        code.extend_from_slice(&insn(0x7a, 1, offset, value));
    }
    code.extend_from_slice(&insn(0x62, 1, 336, -2));
    code.extend_from_slice(&insn(0x62, 1, 340, 35));
    code.extend_from_slice(&insn(0x7a, 1, 24, 999)); // arg0 is not an x0 write alias
    code.extend_from_slice(&insn(0x7a, 1, 344, 0)); // validity is not a write mask
    code.extend_from_slice(&insn(0xb7, 0, 0, 0));
    code.extend_from_slice(&insn(0x95, 0, 0, 0));
    let mut regs = registers();
    execute(&code, &mut regs, true).unwrap();
    assert_eq!(regs.regs[0], 11);
    assert_eq!(regs.regs[30], 30);
    assert_eq!(regs.regs[1], 0x1001);
    assert_eq!((regs.sp, regs.pc, regs.pstate), (31, 32, 33));
    assert_eq!((regs.orig_x0, regs.syscallno, regs.unused2), (34, -2, 35));
}

#[test]
fn stores_before_runtime_error_are_written_back() {
    let code = [
        insn(0x7a, 1, 56, 42),
        insn(0x79, 0x10, 4096, 0), // outside context: fail after the store
        insn(0x95, 0, 0, 0),
    ]
    .concat();
    let mut regs = registers();
    assert!(execute(&code, &mut regs, false).is_err());
    assert_eq!(regs.regs[0], 42);
    assert_eq!(regs.pc, 0x3000);
}
