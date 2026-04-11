//! Lua 5.4 opcodes and instruction encoding.
//!
//! Ported from `lopcodes.{c,h}`. Preserves the upstream on-wire format
//! byte-for-byte: the 7-bit opcode field, the 8+1+8+8 or 6+10 argument
//! layouts, the opmode flag encoding (MM/OT/IT/T/A/mode). Bytecode
//! compatibility with C Lua (decision 6) means every bit layout must
//! match exactly — the differential test verifies all 85 entries plus
//! the `luaP_isOT`/`luaP_isIT` decisions on crafted instructions.
//!
//! Names match C upstream directly (`OP_MOVE`, `iABC`, etc.) because
//! the on-wire format and the spec documentation both use them; a
//! Rust-style rename would create a permanent decoder ring between the
//! C source and the Rust port for every reader.
//!
//! Ground truth: `lopcodes.h` (layout + macros) and `lopcodes.c`
//! (opmode table + luaP_isOT / luaP_isIT).

#![allow(dead_code, non_camel_case_types, non_upper_case_globals)]

/// A 32-bit Lua instruction word.
pub type Instruction = u32;

// ---------------------------------------------------------------------------
// OpMode — the argument-layout families. `lopcodes.h`:
//     enum OpMode {iABC, ivABC, iABx, iAsBx, iAx, isJ};
// ---------------------------------------------------------------------------

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpMode {
    iABC = 0,
    ivABC = 1,
    iABx = 2,
    iAsBx = 3,
    iAx = 4,
    isJ = 5,
}

impl OpMode {
    const fn from_bits(bits: u8) -> OpMode {
        match bits {
            0 => OpMode::iABC,
            1 => OpMode::ivABC,
            2 => OpMode::iABx,
            3 => OpMode::iAsBx,
            4 => OpMode::iAx,
            5 => OpMode::isJ,
            // Unreachable on any valid opmode byte produced by [`opmode`].
            _ => OpMode::iABC,
        }
    }
}

// ---------------------------------------------------------------------------
// OpCode — all 85 opcodes in the `ORDER OP` order from `lopcodes.h`. The
// integer discriminants are on-wire values and must match the C enum.
// ---------------------------------------------------------------------------

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    OP_MOVE = 0,
    OP_LOADI = 1,
    OP_LOADF = 2,
    OP_LOADK = 3,
    OP_LOADKX = 4,
    OP_LOADFALSE = 5,
    OP_LFALSESKIP = 6,
    OP_LOADTRUE = 7,
    OP_LOADNIL = 8,
    OP_GETUPVAL = 9,
    OP_SETUPVAL = 10,
    OP_GETTABUP = 11,
    OP_GETTABLE = 12,
    OP_GETI = 13,
    OP_GETFIELD = 14,
    OP_SETTABUP = 15,
    OP_SETTABLE = 16,
    OP_SETI = 17,
    OP_SETFIELD = 18,
    OP_NEWTABLE = 19,
    OP_SELF = 20,
    OP_ADDI = 21,
    OP_ADDK = 22,
    OP_SUBK = 23,
    OP_MULK = 24,
    OP_MODK = 25,
    OP_POWK = 26,
    OP_DIVK = 27,
    OP_IDIVK = 28,
    OP_BANDK = 29,
    OP_BORK = 30,
    OP_BXORK = 31,
    OP_SHLI = 32,
    OP_SHRI = 33,
    OP_ADD = 34,
    OP_SUB = 35,
    OP_MUL = 36,
    OP_MOD = 37,
    OP_POW = 38,
    OP_DIV = 39,
    OP_IDIV = 40,
    OP_BAND = 41,
    OP_BOR = 42,
    OP_BXOR = 43,
    OP_SHL = 44,
    OP_SHR = 45,
    OP_MMBIN = 46,
    OP_MMBINI = 47,
    OP_MMBINK = 48,
    OP_UNM = 49,
    OP_BNOT = 50,
    OP_NOT = 51,
    OP_LEN = 52,
    OP_CONCAT = 53,
    OP_CLOSE = 54,
    OP_TBC = 55,
    OP_JMP = 56,
    OP_EQ = 57,
    OP_LT = 58,
    OP_LE = 59,
    OP_EQK = 60,
    OP_EQI = 61,
    OP_LTI = 62,
    OP_LEI = 63,
    OP_GTI = 64,
    OP_GEI = 65,
    OP_TEST = 66,
    OP_TESTSET = 67,
    OP_CALL = 68,
    OP_TAILCALL = 69,
    OP_RETURN = 70,
    OP_RETURN0 = 71,
    OP_RETURN1 = 72,
    OP_FORLOOP = 73,
    OP_FORPREP = 74,
    OP_TFORPREP = 75,
    OP_TFORCALL = 76,
    OP_TFORLOOP = 77,
    OP_SETLIST = 78,
    OP_CLOSURE = 79,
    OP_VARARG = 80,
    OP_GETVARG = 81,
    OP_ERRNNIL = 82,
    OP_VARARGPREP = 83,
    OP_EXTRAARG = 84,
}

/// Total number of opcodes. Matches `NUM_OPCODES` in `lopcodes.h`.
pub const NUM_OPCODES: usize = OpCode::OP_EXTRAARG as usize + 1;

// ---------------------------------------------------------------------------
// Field sizes and positions — match `lopcodes.h` exactly.
// ---------------------------------------------------------------------------

pub const SIZE_OP: u32 = 7;
pub const SIZE_A: u32 = 8;
pub const SIZE_B: u32 = 8;
pub const SIZE_C: u32 = 8;
pub const SIZE_vB: u32 = 6;
pub const SIZE_vC: u32 = 10;
pub const SIZE_Bx: u32 = SIZE_C + SIZE_B + 1; // 17
pub const SIZE_Ax: u32 = SIZE_Bx + SIZE_A; // 25
pub const SIZE_sJ: u32 = SIZE_Bx + SIZE_A; // 25

pub const POS_OP: u32 = 0;
pub const POS_A: u32 = POS_OP + SIZE_OP; // 7
pub const POS_K: u32 = POS_A + SIZE_A; // 15
pub const POS_B: u32 = POS_K + 1; // 16
pub const POS_vB: u32 = POS_K + 1; // 16
pub const POS_C: u32 = POS_B + SIZE_B; // 24
pub const POS_vC: u32 = POS_vB + SIZE_vB; // 22
pub const POS_Bx: u32 = POS_K; // 15
pub const POS_Ax: u32 = POS_A; // 7
pub const POS_sJ: u32 = POS_A; // 7

pub const MAXARG_A: i32 = (1 << SIZE_A) - 1;
pub const MAXARG_B: i32 = (1 << SIZE_B) - 1;
pub const MAXARG_vB: i32 = (1 << SIZE_vB) - 1;
pub const MAXARG_C: i32 = (1 << SIZE_C) - 1;
pub const MAXARG_vC: i32 = (1 << SIZE_vC) - 1;
pub const MAXARG_Bx: i32 = (1 << SIZE_Bx) - 1;
pub const MAXARG_Ax: i32 = (1 << SIZE_Ax) - 1;
pub const MAXARG_sJ: i32 = (1 << SIZE_sJ) - 1;

pub const OFFSET_sBx: i32 = MAXARG_Bx >> 1;
pub const OFFSET_sJ: i32 = MAXARG_sJ >> 1;
pub const OFFSET_sC: i32 = MAXARG_C >> 1;

// ---------------------------------------------------------------------------
// Instruction decoding.
// ---------------------------------------------------------------------------

#[inline]
const fn bit_mask(size: u32) -> u32 {
    if size >= 32 {
        u32::MAX
    } else {
        (1u32 << size) - 1
    }
}

#[inline]
const fn getarg(i: Instruction, pos: u32, size: u32) -> u32 {
    (i >> pos) & bit_mask(size)
}

/// Raw opcode byte extracted from instruction bits 0..6. Equivalent to
/// the C `GET_OPCODE(i)` macro.
#[inline]
pub const fn get_opcode_raw(i: Instruction) -> u8 {
    (i & bit_mask(SIZE_OP)) as u8
}

#[inline]
pub const fn getarg_a(i: Instruction) -> i32 {
    getarg(i, POS_A, SIZE_A) as i32
}
#[inline]
pub const fn getarg_b(i: Instruction) -> i32 {
    getarg(i, POS_B, SIZE_B) as i32
}
#[inline]
pub const fn getarg_vb(i: Instruction) -> i32 {
    getarg(i, POS_vB, SIZE_vB) as i32
}
#[inline]
pub const fn getarg_sb(i: Instruction) -> i32 {
    getarg_b(i) - OFFSET_sC
}
#[inline]
pub const fn getarg_c(i: Instruction) -> i32 {
    getarg(i, POS_C, SIZE_C) as i32
}
#[inline]
pub const fn getarg_vc(i: Instruction) -> i32 {
    getarg(i, POS_vC, SIZE_vC) as i32
}
#[inline]
pub const fn getarg_sc(i: Instruction) -> i32 {
    getarg_c(i) - OFFSET_sC
}
#[inline]
pub const fn getarg_bx(i: Instruction) -> i32 {
    getarg(i, POS_Bx, SIZE_Bx) as i32
}
#[inline]
pub const fn getarg_sbx(i: Instruction) -> i32 {
    getarg_bx(i) - OFFSET_sBx
}
#[inline]
pub const fn getarg_ax(i: Instruction) -> i32 {
    getarg(i, POS_Ax, SIZE_Ax) as i32
}
#[inline]
pub const fn getarg_sj(i: Instruction) -> i32 {
    (getarg(i, POS_sJ, SIZE_sJ) as i32) - OFFSET_sJ
}
#[inline]
pub const fn getarg_k(i: Instruction) -> bool {
    ((i >> POS_K) & 1) != 0
}

// ---------------------------------------------------------------------------
// Instruction construction helpers (CREATE_ABCk etc. in C).
// ---------------------------------------------------------------------------

/// Build an iABC instruction. Matches `CREATE_ABCk` in `lopcodes.h`.
#[inline]
pub const fn create_abck(op: OpCode, a: u32, b: u32, c: u32, k: bool) -> Instruction {
    ((op as u32) << POS_OP)
        | (a << POS_A)
        | ((k as u32) << POS_K)
        | (b << POS_B)
        | (c << POS_C)
}

/// Build an ivABC instruction. Matches `CREATE_vABCk` in `lopcodes.h`.
#[inline]
pub const fn create_vabck(op: OpCode, a: u32, vb: u32, vc: u32, k: bool) -> Instruction {
    ((op as u32) << POS_OP)
        | (a << POS_A)
        | ((k as u32) << POS_K)
        | (vb << POS_vB)
        | (vc << POS_vC)
}

/// Build an iABx instruction. Matches `CREATE_ABx`.
#[inline]
pub const fn create_abx(op: OpCode, a: u32, bx: u32) -> Instruction {
    ((op as u32) << POS_OP) | (a << POS_A) | (bx << POS_Bx)
}

/// Build an iAx instruction. Matches `CREATE_Ax`.
#[inline]
pub const fn create_ax(op: OpCode, ax: u32) -> Instruction {
    ((op as u32) << POS_OP) | (ax << POS_Ax)
}

/// Build an isJ instruction. Matches `CREATE_sJ`.
#[inline]
pub const fn create_sj(op: OpCode, j: u32, k: bool) -> Instruction {
    ((op as u32) << POS_OP) | (j << POS_sJ) | ((k as u32) << POS_K)
}

// ---------------------------------------------------------------------------
// luaP_opmodes table + bit predicates.
// ---------------------------------------------------------------------------

/// Pack a single opmode byte. Matches `opmode(mm,ot,it,t,a,m)` in
/// `lopcodes.c`:
///     ((mm << 7) | (ot << 6) | (it << 5) | (t << 4) | (a << 3) | m)
#[inline]
const fn opmode(mm: u8, ot: u8, it: u8, t: u8, a: u8, m: OpMode) -> u8 {
    (mm << 7) | (ot << 6) | (it << 5) | (t << 4) | (a << 3) | (m as u8)
}

/// 1-byte opmode descriptor per opcode. Byte layout:
///
/// | Bit | Meaning                                              |
/// |-----|------------------------------------------------------|
/// | 7   | is MM instruction (calls a metamethod)               |
/// | 6   | sets `L->top` for the next instruction               |
/// | 5   | uses `L->top` set by the previous instruction        |
/// | 4   | operator is a test (next instruction must be a jump) |
/// | 3   | instruction sets register A                          |
/// | 0-2 | OpMode                                               |
#[rustfmt::skip]
pub const LUAP_OPMODES: [u8; NUM_OPCODES] = [
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_MOVE
    opmode(0, 0, 0, 0, 1, OpMode::iAsBx),  // OP_LOADI
    opmode(0, 0, 0, 0, 1, OpMode::iAsBx),  // OP_LOADF
    opmode(0, 0, 0, 0, 1, OpMode::iABx),   // OP_LOADK
    opmode(0, 0, 0, 0, 1, OpMode::iABx),   // OP_LOADKX
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_LOADFALSE
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_LFALSESKIP
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_LOADTRUE
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_LOADNIL
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_GETUPVAL
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_SETUPVAL
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_GETTABUP
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_GETTABLE
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_GETI
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_GETFIELD
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_SETTABUP
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_SETTABLE
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_SETI
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_SETFIELD
    opmode(0, 0, 0, 0, 1, OpMode::ivABC),  // OP_NEWTABLE
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_SELF
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_ADDI
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_ADDK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_SUBK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_MULK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_MODK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_POWK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_DIVK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_IDIVK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_BANDK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_BORK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_BXORK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_SHLI
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_SHRI
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_ADD
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_SUB
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_MUL
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_MOD
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_POW
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_DIV
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_IDIV
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_BAND
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_BOR
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_BXOR
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_SHL
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_SHR
    opmode(1, 0, 0, 0, 0, OpMode::iABC),   // OP_MMBIN
    opmode(1, 0, 0, 0, 0, OpMode::iABC),   // OP_MMBINI
    opmode(1, 0, 0, 0, 0, OpMode::iABC),   // OP_MMBINK
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_UNM
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_BNOT
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_NOT
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_LEN
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_CONCAT
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_CLOSE
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_TBC
    opmode(0, 0, 0, 0, 0, OpMode::isJ),    // OP_JMP
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_EQ
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_LT
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_LE
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_EQK
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_EQI
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_LTI
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_LEI
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_GTI
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_GEI
    opmode(0, 0, 0, 1, 0, OpMode::iABC),   // OP_TEST
    opmode(0, 0, 0, 1, 1, OpMode::iABC),   // OP_TESTSET
    opmode(0, 1, 1, 0, 1, OpMode::iABC),   // OP_CALL
    opmode(0, 1, 1, 0, 1, OpMode::iABC),   // OP_TAILCALL
    opmode(0, 0, 1, 0, 0, OpMode::iABC),   // OP_RETURN
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_RETURN0
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_RETURN1
    opmode(0, 0, 0, 0, 1, OpMode::iABx),   // OP_FORLOOP
    opmode(0, 0, 0, 0, 1, OpMode::iABx),   // OP_FORPREP
    opmode(0, 0, 0, 0, 0, OpMode::iABx),   // OP_TFORPREP
    opmode(0, 0, 0, 0, 0, OpMode::iABC),   // OP_TFORCALL
    opmode(0, 0, 0, 0, 1, OpMode::iABx),   // OP_TFORLOOP
    opmode(0, 0, 1, 0, 0, OpMode::ivABC),  // OP_SETLIST
    opmode(0, 0, 0, 0, 1, OpMode::iABx),   // OP_CLOSURE
    opmode(0, 1, 0, 0, 1, OpMode::iABC),   // OP_VARARG
    opmode(0, 0, 0, 0, 1, OpMode::iABC),   // OP_GETVARG
    opmode(0, 0, 0, 0, 0, OpMode::iABx),   // OP_ERRNNIL
    opmode(0, 0, 1, 0, 0, OpMode::iABC),   // OP_VARARGPREP
    opmode(0, 0, 0, 0, 0, OpMode::iAx),    // OP_EXTRAARG
];

#[inline]
pub const fn get_op_mode(op: u8) -> OpMode {
    OpMode::from_bits(LUAP_OPMODES[op as usize] & 7)
}

#[inline]
pub const fn test_a_mode(op: u8) -> bool {
    LUAP_OPMODES[op as usize] & (1 << 3) != 0
}
#[inline]
pub const fn test_t_mode(op: u8) -> bool {
    LUAP_OPMODES[op as usize] & (1 << 4) != 0
}
#[inline]
pub const fn test_it_mode(op: u8) -> bool {
    LUAP_OPMODES[op as usize] & (1 << 5) != 0
}
#[inline]
pub const fn test_ot_mode(op: u8) -> bool {
    LUAP_OPMODES[op as usize] & (1 << 6) != 0
}
#[inline]
pub const fn test_mm_mode(op: u8) -> bool {
    LUAP_OPMODES[op as usize] & (1 << 7) != 0
}

// ---------------------------------------------------------------------------
// luaP_isOT and luaP_isIT — direct ports of `lopcodes.c`.
// ---------------------------------------------------------------------------

/// Check whether `i` sets `L->top` for the next instruction. Port of
/// `luaP_isOT`. `OP_TAILCALL` is unconditionally true; every other
/// opcode is true iff `testOTMode(op) && GETARG_C(i) == 0`.
pub fn lua_p_is_ot(i: Instruction) -> bool {
    let op = get_opcode_raw(i);
    if op == OpCode::OP_TAILCALL as u8 {
        return true;
    }
    test_ot_mode(op) && getarg_c(i) == 0
}

/// Check whether `i` consumes `L->top` set by the previous instruction.
/// Port of `luaP_isIT`. `OP_SETLIST` reads `vB` (the ivABC short B
/// field); every other opcode reads `B`.
pub fn lua_p_is_it(i: Instruction) -> bool {
    let op = get_opcode_raw(i);
    if op == OpCode::OP_SETLIST as u8 {
        return test_it_mode(op) && getarg_vb(i) == 0;
    }
    test_it_mode(op) && getarg_b(i) == 0
}

// ---------------------------------------------------------------------------
// Unit tests — invariants that are cheap to check inside the Rust layer
// without going through the C oracle. The byte-exact table + predicate
// parity lives in `tests/differential_lopcodes.rs`.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_opcodes_is_85() {
        assert_eq!(NUM_OPCODES, 85);
    }

    #[test]
    fn position_layout_is_consistent() {
        assert_eq!(POS_OP, 0);
        assert_eq!(POS_A, 7);
        assert_eq!(POS_K, 15);
        assert_eq!(POS_B, 16);
        assert_eq!(POS_C, 24);
        assert_eq!(POS_vB, 16);
        assert_eq!(POS_vC, 22);
        // Everything must fit inside 32 bits.
        assert_eq!(POS_OP + SIZE_OP + SIZE_A + 1 + SIZE_B + SIZE_C, 32);
        assert_eq!(POS_OP + SIZE_OP + SIZE_A + 1 + SIZE_vB + SIZE_vC, 32);
        assert_eq!(POS_OP + SIZE_OP + SIZE_A + SIZE_Bx, 32);
        assert_eq!(POS_OP + SIZE_OP + SIZE_Ax, 32);
        assert_eq!(POS_OP + SIZE_OP + SIZE_sJ, 32);
    }

    #[test]
    fn create_and_decode_abck_roundtrip() {
        let i = create_abck(OpCode::OP_ADD, 1, 2, 3, true);
        assert_eq!(get_opcode_raw(i), OpCode::OP_ADD as u8);
        assert_eq!(getarg_a(i), 1);
        assert_eq!(getarg_b(i), 2);
        assert_eq!(getarg_c(i), 3);
        assert!(getarg_k(i));
    }

    #[test]
    fn create_and_decode_vabck_roundtrip() {
        let i = create_vabck(OpCode::OP_NEWTABLE, 4, 5, 6, false);
        assert_eq!(get_opcode_raw(i), OpCode::OP_NEWTABLE as u8);
        assert_eq!(getarg_a(i), 4);
        assert_eq!(getarg_vb(i), 5);
        assert_eq!(getarg_vc(i), 6);
        assert!(!getarg_k(i));
    }

    #[test]
    fn create_and_decode_abx_roundtrip() {
        let bx = 12345u32;
        let i = create_abx(OpCode::OP_LOADK, 7, bx);
        assert_eq!(getarg_a(i), 7);
        assert_eq!(getarg_bx(i), bx as i32);
    }

    #[test]
    fn sbx_is_excess_encoded() {
        let sbx = -42i32;
        let i = create_abx(OpCode::OP_LOADI, 3, (sbx + OFFSET_sBx) as u32);
        assert_eq!(getarg_sbx(i), sbx);
    }

    #[test]
    fn sj_is_excess_encoded() {
        let j = -1000i32;
        let i = create_sj(OpCode::OP_JMP, (j + OFFSET_sJ) as u32, false);
        assert_eq!(getarg_sj(i), j);
    }

    #[test]
    fn tailcall_always_is_ot() {
        let i = create_abck(OpCode::OP_TAILCALL, 0, 1, 2, false);
        assert!(lua_p_is_ot(i));
    }

    #[test]
    fn call_is_ot_iff_c_is_zero() {
        assert!(lua_p_is_ot(create_abck(OpCode::OP_CALL, 0, 2, 0, false)));
        assert!(!lua_p_is_ot(create_abck(OpCode::OP_CALL, 0, 2, 1, false)));
    }

    #[test]
    fn is_it_uses_b_by_default_and_vb_for_setlist() {
        // OP_SETLIST (ivABC), IT set, vB == 0 → true
        assert!(lua_p_is_it(create_vabck(OpCode::OP_SETLIST, 0, 0, 0, false)));
        // OP_SETLIST with vB != 0 → false
        assert!(!lua_p_is_it(create_vabck(OpCode::OP_SETLIST, 0, 1, 0, false)));
        // OP_RETURN (iABC, IT set), B == 0 → true
        assert!(lua_p_is_it(create_abck(OpCode::OP_RETURN, 0, 0, 0, false)));
        // OP_RETURN with B != 0 → false
        assert!(!lua_p_is_it(create_abck(OpCode::OP_RETURN, 0, 3, 0, false)));
    }
}
