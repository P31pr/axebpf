#![cfg(feature = "runtime")]

use axebpf::TraceContext;
use axebpf::context::{REGS_VALID_X0_X7, REGS_VALID_X8_X30};
use axebpf::runtime::EbpfProgram;

fn read_u64_program(offset: i16) -> EbpfProgram {
    read_program(0x79, offset)
}

fn read_u32_program(offset: i16) -> EbpfProgram {
    read_program(0x61, offset)
}

fn read_program(opcode: u8, offset: i16) -> EbpfProgram {
    let [lo, hi] = offset.to_le_bytes();
    EbpfProgram::new(
        &[
            opcode, 0x10, lo, hi, 0, 0, 0, 0, // load u32/u64 at r1 + offset
            0x95, 0, 0, 0, 0, 0, 0, 0, // exit
        ],
        None,
    )
    .unwrap()
}

#[test]
fn reordered_header_and_register_offsets_are_visible_to_ebpf() {
    let regs = core::array::from_fn(|i| 0x1000 + i as u64);
    let mut ctx = TraceContext::new(17)
        .with_vm(3, 5)
        .with_probe_type(2)
        .with_args(77, 0, 0, 0)
        .with_aarch64_gprs(&regs);
    ctx.timestamp_ns = 0x1122_3344_5566_7788;
    // Hard-coded ABI offsets deliberately independent of the Rust field offsets.
    for (offset, expected) in [
        (0, ctx.timestamp_ns),
        (24, 77),
        (56, regs[0]),
        (112, regs[7]),
        (120, regs[8]),
        (296, regs[30]),
    ] {
        assert_eq!(
            read_u64_program(offset)
                .execute_with_context(ctx.as_bytes_mut())
                .unwrap(),
            expected,
        );
    }
    for (offset, expected) in [(8, 17), (12, 3), (16, 5), (20, 2)] {
        assert_eq!(
            read_u32_program(offset)
                .execute_with_context(ctx.as_bytes_mut())
                .unwrap(),
            expected,
        );
    }
    assert_eq!(ctx.regs_valid, REGS_VALID_X0_X7 | REGS_VALID_X8_X30);
    assert_eq!(ctx.sp, 0);
    assert_eq!(ctx.pc, 0);
}

#[test]
fn tracepoint_has_correct_input_type_and_no_register_snapshot() {
    let mut ctx = TraceContext::for_tracepoint(17, 1234, 5678);
    assert_eq!(ctx.tracepoint_id, 17);
    assert_eq!(ctx.timestamp_ns, 1234);
    assert_eq!(ctx.arg0, 5678);
    assert_eq!(
        read_u32_program(20)
            .execute_with_context(ctx.as_bytes_mut())
            .unwrap(),
        4
    );
    assert_eq!(
        read_u64_program(344)
            .execute_with_context(ctx.as_bytes_mut())
            .unwrap(),
        0
    );
    assert_eq!(&ctx.as_bytes()[56..], &[0; 296]);
}

#[test]
fn padding_free_byte_access_accepts_all_bit_patterns() {
    let mut ctx = TraceContext::default();
    assert_eq!(ctx.as_bytes(), &[0; 352]);
    ctx.as_bytes_mut().fill(0xff);
    assert_eq!(ctx.timestamp_ns, u64::MAX);
    assert_eq!(ctx.tracepoint_id, u32::MAX);
    assert_eq!(ctx.syscallno, -1);
    assert_eq!(ctx.regs_x8_x30, [u64::MAX; 23]);
    assert_eq!(ctx.regs_valid, u64::MAX);
}

#[cfg(all(feature = "guest-kprobe", feature = "guest-uprobe"))]
#[test]
fn guest_probes_share_register_offsets_without_changing_argument_semantics() {
    use axebpf::probe::{kprobe, uprobe};

    let regs = core::array::from_fn(|i| 0x2000 + i as u64);
    let mut kernel = kprobe::handler::build_guest_ctx_for_test(3, false, 0xabcd, 0xef00, &regs);
    let mut user = uprobe::handler::build_uprobe_ctx_for_test(3, 0, 42, 43, 0x1000, false)
        .with_aarch64_gprs(&regs);
    let program = read_u64_program(296); // x30, in the extended register region
    for ctx in [&mut kernel, &mut user] {
        assert_eq!(
            program.execute_with_context(ctx.as_bytes_mut()).unwrap(),
            regs[30]
        );
        assert_eq!(ctx.regs_valid, REGS_VALID_X0_X7 | REGS_VALID_X8_X30);
    }
    assert_eq!(kernel.arg0, 0xabcd); // guest address
    assert_eq!(user.arg0, 42); // PID, not x0
}
