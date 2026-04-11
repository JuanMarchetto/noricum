//! Differential test for the ported `lopcodes` module.
//!
//! Three layers of comparison:
//!
//! 1. Opcode-enum pinning — the discriminants of a handful of opcodes
//!    we can name from C are fetched via the oracle and compared
//!    against our Rust `OpCode` variants. This catches any enum
//!    reshuffle that would silently break bytecode compatibility.
//! 2. Opmode-table byte match — all 85 opmode bytes must agree.
//! 3. Instruction-decoding parity — synthesized instructions go
//!    through both Rust and C for every decoder, including the
//!    `luaP_isOT` / `luaP_isIT` helpers.
//!
//! Decision 6 of the full-port project (bytecode byte-compatibility)
//! depends on this test staying green.

use spike::lopcodes as lo;
use spike::{
    wr_lopcodes_get_opcode, wr_lopcodes_getarg_a, wr_lopcodes_getarg_ax, wr_lopcodes_getarg_b,
    wr_lopcodes_getarg_bx, wr_lopcodes_getarg_c, wr_lopcodes_getarg_k, wr_lopcodes_getarg_sbx,
    wr_lopcodes_getarg_sj, wr_lopcodes_getarg_vb, wr_lopcodes_getarg_vc, wr_lopcodes_is_it,
    wr_lopcodes_is_ot, wr_lopcodes_num_opcodes, wr_lopcodes_op_call, wr_lopcodes_op_extraarg,
    wr_lopcodes_op_return, wr_lopcodes_op_setlist, wr_lopcodes_op_tailcall, wr_lopcodes_opmode,
};

// -- Layer 1: opcode enum pinning ----------------------------------------

#[test]
fn num_opcodes_matches_c() {
    let c = unsafe { wr_lopcodes_num_opcodes() };
    assert_eq!(lo::NUM_OPCODES as i32, c);
}

#[test]
fn named_opcode_discriminants_match_c() {
    assert_eq!(
        lo::OpCode::OP_TAILCALL as i32,
        unsafe { wr_lopcodes_op_tailcall() }
    );
    assert_eq!(
        lo::OpCode::OP_SETLIST as i32,
        unsafe { wr_lopcodes_op_setlist() }
    );
    assert_eq!(
        lo::OpCode::OP_CALL as i32,
        unsafe { wr_lopcodes_op_call() }
    );
    assert_eq!(
        lo::OpCode::OP_RETURN as i32,
        unsafe { wr_lopcodes_op_return() }
    );
    assert_eq!(
        lo::OpCode::OP_EXTRAARG as i32,
        unsafe { wr_lopcodes_op_extraarg() }
    );
}

// -- Layer 2: full opmode table byte match --------------------------------

#[test]
fn opmode_table_is_byte_exact() {
    for op in 0..lo::NUM_OPCODES {
        let oracle = unsafe { wr_lopcodes_opmode(op as i32) };
        let rust = lo::LUAP_OPMODES[op] as i32;
        assert_eq!(
            rust, oracle,
            "opmode byte mismatch at op {op}: C = {oracle:#04x}, Rust = {rust:#04x}"
        );
    }
}

// -- Layer 3: instruction decoder parity ---------------------------------

/// Build an iABC instruction via Rust for comparison through the oracle.
fn abck(op: lo::OpCode, a: u32, b: u32, c: u32, k: bool) -> u32 {
    lo::create_abck(op, a, b, c, k)
}

fn vabck(op: lo::OpCode, a: u32, vb: u32, vc: u32, k: bool) -> u32 {
    lo::create_vabck(op, a, vb, vc, k)
}

fn abx(op: lo::OpCode, a: u32, bx: u32) -> u32 {
    lo::create_abx(op, a, bx)
}

fn ax(op: lo::OpCode, val: u32) -> u32 {
    lo::create_ax(op, val)
}

fn sj(op: lo::OpCode, j: u32, k: bool) -> u32 {
    lo::create_sj(op, j, k)
}

#[test]
fn abck_decoders_match_c() {
    for a in [0u32, 1, 7, 42, 255] {
        for b in [0u32, 1, 128, 255] {
            for c in [0u32, 1, 128, 255] {
                for k in [false, true] {
                    let i = abck(lo::OpCode::OP_ADD, a, b, c, k);
                    assert_eq!(
                        lo::get_opcode_raw(i) as i32,
                        unsafe { wr_lopcodes_get_opcode(i) },
                        "GET_OPCODE at {i:#010x}"
                    );
                    assert_eq!(
                        lo::getarg_a(i),
                        unsafe { wr_lopcodes_getarg_a(i) },
                        "GETARG_A at {i:#010x}"
                    );
                    assert_eq!(
                        lo::getarg_b(i),
                        unsafe { wr_lopcodes_getarg_b(i) },
                        "GETARG_B at {i:#010x}"
                    );
                    assert_eq!(
                        lo::getarg_c(i),
                        unsafe { wr_lopcodes_getarg_c(i) },
                        "GETARG_C at {i:#010x}"
                    );
                    assert_eq!(
                        lo::getarg_k(i) as i32,
                        unsafe { wr_lopcodes_getarg_k(i) },
                        "GETARG_k at {i:#010x}"
                    );
                }
            }
        }
    }
}

#[test]
fn vabck_decoders_match_c() {
    for a in [0u32, 1, 100, 255] {
        for vb in [0u32, 1, 31, 63] {
            for vc in [0u32, 1, 512, 1023] {
                let i = vabck(lo::OpCode::OP_NEWTABLE, a, vb, vc, false);
                assert_eq!(lo::getarg_vb(i), unsafe { wr_lopcodes_getarg_vb(i) });
                assert_eq!(lo::getarg_vc(i), unsafe { wr_lopcodes_getarg_vc(i) });
            }
        }
    }
}

#[test]
fn bx_sbx_decoders_match_c() {
    for bx in [0u32, 1, 65535, 65536, 131071] {
        let i = abx(lo::OpCode::OP_LOADK, 5, bx);
        assert_eq!(lo::getarg_bx(i), unsafe { wr_lopcodes_getarg_bx(i) });
        assert_eq!(lo::getarg_sbx(i), unsafe { wr_lopcodes_getarg_sbx(i) });
    }
}

#[test]
fn ax_decoder_matches_c() {
    for a_val in [0u32, 1, 1024, 33554431] {
        let i = ax(lo::OpCode::OP_EXTRAARG, a_val);
        assert_eq!(lo::getarg_ax(i), unsafe { wr_lopcodes_getarg_ax(i) });
    }
}

#[test]
fn sj_decoder_matches_c() {
    for j in [0u32, 1, 16777215, 33554431] {
        let i = sj(lo::OpCode::OP_JMP, j, false);
        assert_eq!(lo::getarg_sj(i), unsafe { wr_lopcodes_getarg_sj(i) });
    }
}

#[test]
fn is_ot_matches_c_on_every_opcode() {
    // For every opcode we emit a canonical iABC / ivABC / iABx
    // instruction with A=B=C=0, k=false and ask both implementations
    // whether it sets top. Then repeat with C=1 to catch the OT-mode
    // gating.
    for op in 0..lo::NUM_OPCODES {
        for c in [0u32, 1] {
            let raw: u32 =
                ((op as u32) << lo::POS_OP) | (0u32 << lo::POS_A) | (0u32 << lo::POS_B) | (c << lo::POS_C);
            assert_eq!(
                lo::lua_p_is_ot(raw),
                unsafe { wr_lopcodes_is_ot(raw) } != 0,
                "lua_p_is_ot mismatch at op={op}, c={c}"
            );
        }
    }
}

#[test]
fn is_it_matches_c_on_every_opcode() {
    for op in 0..lo::NUM_OPCODES {
        for b in [0u32, 1] {
            let raw: u32 =
                ((op as u32) << lo::POS_OP) | (0u32 << lo::POS_A) | (b << lo::POS_B);
            assert_eq!(
                lo::lua_p_is_it(raw),
                unsafe { wr_lopcodes_is_it(raw) } != 0,
                "lua_p_is_it mismatch at op={op}, b={b}"
            );
        }
    }
}
