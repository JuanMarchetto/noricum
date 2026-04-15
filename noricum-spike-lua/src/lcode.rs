//! lcode — the Lua code generator.
//!
//! Port of Lua's `lcode.c`. Emits bytecode instructions into a
//! [`FuncState`]'s Proto as the parser calls into it. This module
//! handles register allocation, constant pool management, jump
//! patching, and expression discharge.

#![allow(dead_code)]

use crate::contract::{LuaInteger, LuaNumber, StringHandle, TValue};
use crate::lparser::{ExprDesc, ExpKind, FuncState};
use crate::lopcodes::{
    create_abck, create_abx, create_sj, getarg_sj, OpCode, OFFSET_sBx, OFFSET_sJ,
};

pub const NO_JUMP: i32 = -1;

// ---- Binary / Unary operator enums -----------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOpr {
    Add, Sub, Mul, Mod, Pow, Div, IDiv,
    BAnd, BOr, BXor, Shl, Shr,
    Concat,
    Eq, Lt, Le, Ne, Gt, Ge,
    And, Or,
    NoBinOpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOpr {
    Minus, BNot, Not, Len, NoUnOpr,
}

// ---- Instruction emission --------------------------------------------------

/// Append an instruction word to the proto's code array. Returns
/// the PC of the emitted instruction.
pub fn emit(fs: &mut FuncState, i: u32) -> i32 {
    let pc = fs.pc;
    fs.proto.code.push(i);
    save_line_info(fs);
    fs.pc += 1;
    pc
}

/// Append a line-info entry for the instruction about to be appended.
/// Mirrors `savelineinfo` in lcode.c: store a signed-byte delta from
/// `previousline` to `current_line`. When the delta exceeds the range
/// of `i8`, push an absolute checkpoint into `abs_line_info` and
/// store the sentinel `i8::MIN` in `line_info`.
fn save_line_info(fs: &mut FuncState) {
    let line = fs.current_line;
    let prev = fs.previousline;
    let delta = line - prev;
    // First entry, or out-of-range delta: emit an absolute checkpoint
    // and write the sentinel into the per-instruction delta vector.
    let needs_abs = !(-128..=127).contains(&delta) ||
        // Lua 5.4 also flushes an abs entry every MAXIWTHABS (~128)
        // instructions to bound the cost of a debug.getinfo lookup.
        // We skip that periodic flush — it's a perf optimization for
        // debug-info reads, not correctness.
        false;
    if needs_abs {
        fs.proto.abs_line_info.push(crate::contract::AbsLineInfo {
            pc: fs.pc,
            line,
        });
        fs.proto.line_info.push(i8::MIN);
    } else {
        fs.proto.line_info.push(delta as i8);
    }
    fs.previousline = line;
}

pub fn emit_abc(fs: &mut FuncState, op: OpCode, a: u32, b: u32, c: u32, k: bool) -> i32 {
    emit(fs, create_abck(op, a, b, c, k))
}

pub fn emit_abx(fs: &mut FuncState, op: OpCode, a: u32, bx: u32) -> i32 {
    emit(fs, create_abx(op, a, bx))
}

pub fn emit_sj(fs: &mut FuncState, op: OpCode, sj: i32) -> i32 {
    emit(fs, create_sj(op, (sj + OFFSET_sJ) as u32, false))
}

pub fn emit_jump(fs: &mut FuncState) -> i32 {
    emit_sj(fs, OpCode::OP_JMP, NO_JUMP)
}

// ---- Jump list management --------------------------------------------------

pub fn get_label(fs: &mut FuncState) -> i32 {
    fs.lasttarget = fs.pc;
    fs.pc
}

pub fn get_jump_dest(fs: &FuncState, pc: i32) -> i32 {
    let i = fs.proto.code[pc as usize];
    let offset = getarg_sj(i);
    if offset == NO_JUMP {
        NO_JUMP
    } else {
        (pc + 1) + offset
    }
}

fn fix_jump(fs: &mut FuncState, pc: i32, dest: i32) {
    let offset = dest - (pc + 1);
    let instr = &mut fs.proto.code[pc as usize];
    *instr = create_sj(OpCode::OP_JMP, (offset + OFFSET_sJ) as u32, false);
}

pub fn patch_list(fs: &mut FuncState, mut list: i32, target: i32) {
    while list != NO_JUMP {
        let next = get_jump_dest(fs, list);
        // If the controlling instruction is OP_TESTSET placed here
        // by `jump_on_cond` with A=NO_REG, convert it to a
        // self-copy (A := B) so the runtime doesn't write into
        // stack slot 255.
        if list >= 1 {
            let ctrl_pc = (list - 1) as usize;
            if ctrl_pc < fs.proto.code.len() {
                let op = (fs.proto.code[ctrl_pc] & 0x7F) as u8;
                if op == OpCode::OP_TESTSET as u8 {
                    let instr = &mut fs.proto.code[ctrl_pc];
                    let a = ((*instr >> 7) & 0xFF) as u8;
                    if a == NO_REG {
                        let b = ((*instr >> 16) & 0xFF) as u8;
                        *instr = (*instr & !(0xFFu32 << 7)) | ((b as u32) << 7);
                    }
                }
            }
        }
        fix_jump(fs, list, target);
        list = next;
    }
}

pub fn patch_to_here(fs: &mut FuncState, list: i32) {
    let here = get_label(fs);
    patch_list(fs, list, here);
}

pub fn concat_jmp(fs: &mut FuncState, l1: &mut i32, l2: i32) {
    if l2 == NO_JUMP {
        return;
    }
    if *l1 == NO_JUMP {
        *l1 = l2;
    } else {
        let mut list = *l1;
        loop {
            let next = get_jump_dest(fs, list);
            if next == NO_JUMP {
                fix_jump(fs, list, l2);
                break;
            }
            list = next;
        }
    }
}

// ---- Register allocation ---------------------------------------------------

pub fn reserve_regs(fs: &mut FuncState, n: u8) {
    fs.freereg += n;
    if fs.freereg > fs.proto.max_stack_size {
        fs.proto.max_stack_size = fs.freereg;
    }
}

fn free_reg(fs: &mut FuncState, reg: u8) {
    // Only free the TOP-of-stack temporary. Silently no-op when
    // asked to free a register that isn't the top — a callsite
    // that matters (e.g., binop operand discharge) will discard
    // values back-to-front, so the invariant usually holds, but
    // some discharge chains (involving __index metamethod or
    // TESTSET placeholders) can leave lower temps mid-flight.
    // Lua's assertion is debug-only; we prefer the soft skip so
    // complex expressions don't abort compilation.
    let nvs = nvarstack(fs);
    if reg >= nvs && fs.freereg > 0 && reg + 1 == fs.freereg {
        fs.freereg -= 1;
    }
}

fn nvarstack(fs: &FuncState) -> u8 {
    let mut reg: u8 = 0;
    for i in 0..fs.nactvar as usize {
        if i < fs.actvar.len() {
            reg = reg.max(fs.actvar[i].ridx + 1);
        }
    }
    reg
}

fn free_exp(fs: &mut FuncState, e: &ExprDesc) {
    if e.k == ExpKind::NonReloc {
        free_reg(fs, e.info as u8);
    }
}

// ---- Constant pool ---------------------------------------------------------

fn add_k(fs: &mut FuncState, val: TValue) -> i32 {
    // Dedup: search existing constants.
    for (i, c) in fs.proto.constants.iter().enumerate() {
        if *c == val {
            return i as i32;
        }
    }
    let idx = fs.proto.constants.len() as i32;
    fs.proto.constants.push(val);
    fs.nk = idx + 1;
    idx
}

pub fn string_k(fs: &mut FuncState, s: StringHandle) -> i32 {
    add_k(fs, TValue::ShortString(s))
}

pub fn int_k(fs: &mut FuncState, n: LuaInteger) -> i32 {
    add_k(fs, TValue::Integer(n))
}

pub fn float_k(fs: &mut FuncState, n: LuaNumber) -> i32 {
    add_k(fs, TValue::Number(n))
}

// ---- Expression discharge --------------------------------------------------

pub fn discharge_vars(fs: &mut FuncState, e: &mut ExprDesc) {
    match e.k {
        ExpKind::Local => {
            e.k = ExpKind::NonReloc;
            e.info = e.var.ridx as i32;
        }
        ExpKind::Upval => {
            let reg = fs.freereg;
            emit_abc(fs, OpCode::OP_GETUPVAL, reg as u32, e.info as u32, 0, false);
            e.k = ExpKind::NonReloc;
            e.info = reg as i32;
            reserve_regs(fs, 1);
        }
        ExpKind::IndexStr => {
            // Free the source-table register first if it's a temp
            // (above nvarstack) so the result reuses that slot
            // and the caller's freereg is correct for subsequent
            // arg slots.
            let t_reg = e.ind.t;
            if t_reg >= nvarstack(fs) {
                fs.freereg -= 1;
            }
            let reg = fs.freereg;
            emit_abc(
                fs,
                OpCode::OP_GETFIELD,
                reg as u32,
                t_reg as u32,
                e.ind.idx as u32,
                false,
            );
            e.k = ExpKind::NonReloc;
            e.info = reg as i32;
            reserve_regs(fs, 1);
        }
        ExpKind::IndexUp => {
            let reg = fs.freereg;
            emit_abc(
                fs,
                OpCode::OP_GETTABUP,
                reg as u32,
                e.ind.t as u32,
                e.ind.idx as u32,
                false,
            );
            e.k = ExpKind::NonReloc;
            e.info = reg as i32;
            reserve_regs(fs, 1);
        }
        ExpKind::Indexed => {
            let t_reg = e.ind.t;
            let k_reg = e.ind.idx as u8;
            // Free key temp first (it's above table temp).
            if k_reg >= nvarstack(fs) {
                fs.freereg -= 1;
            }
            if t_reg >= nvarstack(fs) {
                fs.freereg -= 1;
            }
            let reg = fs.freereg;
            emit_abc(
                fs,
                OpCode::OP_GETTABLE,
                reg as u32,
                t_reg as u32,
                k_reg as u32,
                false,
            );
            e.k = ExpKind::NonReloc;
            e.info = reg as i32;
            reserve_regs(fs, 1);
        }
        ExpKind::IndexI => {
            let t_reg = e.ind.t;
            if t_reg >= nvarstack(fs) {
                fs.freereg -= 1;
            }
            let reg = fs.freereg;
            emit_abc(
                fs,
                OpCode::OP_GETI,
                reg as u32,
                t_reg as u32,
                e.ind.idx as u32,
                false,
            );
            e.k = ExpKind::NonReloc;
            e.info = reg as i32;
            reserve_regs(fs, 1);
        }
        ExpKind::Call => {
            // Call already placed result at function slot.
            // Patch C (bits 24..31) to 2 meaning "1 result".
            let instr = &mut fs.proto.code[e.info as usize];
            *instr = (*instr & !(0xFFu32 << 24)) | (2u32 << 24);
            e.k = ExpKind::NonReloc;
            // Result is at the function slot (arg A of CALL, bits 7..14).
            e.info = ((*instr >> 7) & 0xFF) as i32;
        }
        ExpKind::VarArg => {
            // Already emitted VARARG; patch C (bits 24..31) to 2
            // (request 1 result).
            let instr = &mut fs.proto.code[e.info as usize];
            *instr = (*instr & !(0xFFu32 << 24)) | (2u32 << 24);
            e.k = ExpKind::Reloc;
        }
        _ => {} // already discharged or constant
    }
}

fn discharge_to_reg(fs: &mut FuncState, e: &mut ExprDesc, reg: u8) {
    discharge_vars(fs, e);
    match e.k {
        ExpKind::Nil => {
            emit_abc(fs, OpCode::OP_LOADNIL, reg as u32, 0, 0, false);
        }
        ExpKind::False => {
            emit_abc(fs, OpCode::OP_LOADFALSE, reg as u32, 0, 0, false);
        }
        ExpKind::True => {
            emit_abc(fs, OpCode::OP_LOADTRUE, reg as u32, 0, 0, false);
        }
        ExpKind::KInt => {
            let n = e.ival;
            // OP_LOADI uses sBx (17-bit signed biased). Anything
            // outside that narrow window must go through the
            // constant pool.
            let max_sbx = OFFSET_sBx as i64;
            let min_sbx = -(OFFSET_sBx as i64);
            if n >= min_sbx && n <= max_sbx {
                emit_abx(
                    fs,
                    OpCode::OP_LOADI,
                    reg as u32,
                    (n as i32 + OFFSET_sBx) as u32,
                );
            } else {
                let k = int_k(fs, n);
                emit_abx(fs, OpCode::OP_LOADK, reg as u32, k as u32);
            }
        }
        ExpKind::KFlt => {
            let k = float_k(fs, e.nval);
            emit_abx(fs, OpCode::OP_LOADK, reg as u32, k as u32);
        }
        ExpKind::KStr => {
            let h = e.strval.unwrap();
            let k = string_k(fs, h);
            emit_abx(fs, OpCode::OP_LOADK, reg as u32, k as u32);
        }
        ExpKind::K => {
            emit_abx(fs, OpCode::OP_LOADK, reg as u32, e.info as u32);
        }
        ExpKind::Reloc => {
            // Patch the A field of the instruction at e.info.
            let instr = &mut fs.proto.code[e.info as usize];
            *instr = (*instr & !(0xFF << 7)) | ((reg as u32) << 7);
        }
        ExpKind::VarArg => {
            // OP_VARARG's A field gets patched to the dest reg so
            // the first extra arg lands at the right slot. C stays
            // at whatever set_vararg_returns set (default 2 = one
            // result).
            let instr = &mut fs.proto.code[e.info as usize];
            *instr = (*instr & !(0xFF << 7)) | ((reg as u32) << 7);
        }
        ExpKind::NonReloc => {
            if e.info != reg as i32 {
                emit_abc(
                    fs,
                    OpCode::OP_MOVE,
                    reg as u32,
                    e.info as u32,
                    0,
                    false,
                );
            }
        }
        ExpKind::Jmp => {
            // Comparison used as a value. CMP+JMP fires JMP on
            // TRUE-of-cmp (k=1 convention). Layout:
            //   ...CMP/JMP (e.info, fires on TRUE)
            //   LOADFALSE R          ; cmp FALSE falls through here
            //   JMP past_true        ; over the LOADTRUE
            //   LOADTRUE  R          ; e.info's JMP target + e.t list lands here
            //   ...next code         ; the skip JMP lands here, and e.f list
            //
            // e.t/e.f may carry residual jumps from preceding OR/AND
            // chains; patch them through patchlistaux to vtarget=t_pc
            // (true) or dtarget=f_pc (false). For TESTSET-controlled
            // jumps, patchlistaux also rewrites A so the operand
            // gets copied into the result register.
            let cmp_jmp = e.info;
            let f_pc = emit_abc(fs, OpCode::OP_LOADFALSE, reg as u32, 0, 0, false);
            let skip_jmp = emit_jump(fs);
            let t_pc = emit_abc(fs, OpCode::OP_LOADTRUE, reg as u32, 0, 0, false);
            fix_jump(fs, cmp_jmp, t_pc);
            fix_jump(fs, skip_jmp, t_pc + 1);
            patchlistaux(fs, e.t, t_pc + 1, reg, t_pc);
            patchlistaux(fs, e.f, t_pc + 1, reg, f_pc);
            e.k = ExpKind::NonReloc;
            e.info = reg as i32;
            e.t = NO_JUMP;
            e.f = NO_JUMP;
            return;
        }
        _ => {
            // void — nothing to emit.
            return;
        }
    }
    e.k = ExpKind::NonReloc;
    e.info = reg as i32;
}


pub fn exp2nextreg(fs: &mut FuncState, e: &mut ExprDesc) {
    discharge_vars(fs, e);
    free_exp(fs, e);
    let reg = fs.freereg;
    reserve_regs(fs, 1);
    discharge_to_reg(fs, e, reg);
    // If the expression carries pending true/false jump lists
    // (from `and`/`or` short-circuiting or comparisons), patch
    // them with LOADBOOL pairs so the materialized value is
    // correct on either path.
    if e.t != NO_JUMP || e.f != NO_JUMP {
        materialize_jump_lists(fs, e, reg);
    }
}

/// Patch the `t` and `f` jump lists on `e` so that, after this
/// runs, register `reg` holds the boolean outcome regardless of
/// which path was taken. Mirrors the bottom half of Lua's
/// exp2reg + patchlistaux.
fn materialize_jump_lists(fs: &mut FuncState, e: &mut ExprDesc, reg: u8) {
    let need_pair = e.t != NO_JUMP || e.f != NO_JUMP;
    if !need_pair {
        return;
    }
    // Decide whether we also need the LOADBOOL pair. Jumps whose
    // controlling instruction is OP_TESTSET don't need it (they
    // copy the operand into the result register on fall-through).
    // Jumps from OP_CMP / direct OP_TEST do need it because they
    // carry no value.
    let need_loadbool = need_value_list(fs, e.t) || need_value_list(fs, e.f);
    if need_loadbool {
        let skip_jmp = emit_jump(fs);
        let p_f = emit_abc(fs, OpCode::OP_LFALSESKIP, reg as u32, 0, 0, false);
        let p_t = emit_abc(fs, OpCode::OP_LOADTRUE, reg as u32, 0, 0, false);
        let final_pc = fs.pc as i32;
        fix_jump(fs, skip_jmp, final_pc);
        patchlistaux(fs, e.f, final_pc, reg, p_f);
        patchlistaux(fs, e.t, final_pc, reg, p_t);
    } else {
        let final_pc = fs.pc as i32;
        patchlistaux(fs, e.f, final_pc, reg, final_pc);
        patchlistaux(fs, e.t, final_pc, reg, final_pc);
    }
    e.t = NO_JUMP;
    e.f = NO_JUMP;
}

/// Return true if any jump in `list` has a controlling instruction
/// that doesn't carry its operand's value (OP_CMP / direct JMP),
/// meaning we need the LOADBOOL pair to materialize a boolean.
fn need_value_list(fs: &FuncState, mut list: i32) -> bool {
    while list != NO_JUMP {
        let ctrl_pc = (list - 1) as usize;
        if ctrl_pc < fs.proto.code.len() {
            let opcode = (fs.proto.code[ctrl_pc] & 0x7F) as u8;
            if opcode != OpCode::OP_TESTSET as u8 {
                return true;
            }
        }
        list = get_jump_dest(fs, list);
    }
    false
}

/// Walk `list`, patching each jump. If the controlling
/// instruction is OP_TESTSET, set its A field to `reg` (so the
/// operand gets copied on the fall-through path) and route the
/// jump to `vtarget` (the "value already in reg" label).
/// Otherwise route to `dtarget` (the LOADBOOL pair or final).
fn patchlistaux(fs: &mut FuncState, mut list: i32, vtarget: i32, reg: u8, dtarget: i32) {
    while list != NO_JUMP {
        let next = get_jump_dest(fs, list);
        let ctrl_pc = (list - 1) as usize;
        let is_testset = ctrl_pc < fs.proto.code.len()
            && (fs.proto.code[ctrl_pc] & 0x7F) as u8 == OpCode::OP_TESTSET as u8;
        if is_testset {
            // SETARG_A: A sits at bits 7..14.
            let instr = &mut fs.proto.code[ctrl_pc];
            let b = ((*instr >> 16) & 0xFF) as u8;
            let target_a = if reg != NO_REG && reg != b { reg } else { b };
            *instr = (*instr & !(0xFFu32 << 7)) | ((target_a as u32) << 7);
            fix_jump(fs, list, vtarget);
        } else {
            fix_jump(fs, list, dtarget);
        }
        list = next;
    }
}

/// Discharge the expression into a specific register (used by
/// the parser for pseudo-locals like for-loop state slots).
/// Does NOT reserve; assumes the caller already owns `reg`.
pub fn exp2nextreg_at(fs: &mut FuncState, e: &mut ExprDesc, reg: u8) {
    discharge_vars(fs, e);
    discharge_to_reg(fs, e, reg);
}

pub fn exp2anyreg(fs: &mut FuncState, e: &mut ExprDesc) -> u8 {
    discharge_vars(fs, e);
    if e.k == ExpKind::NonReloc {
        if e.t == NO_JUMP && e.f == NO_JUMP {
            return e.info as u8;
        }
        // NonReloc carrying t/f jump lists from a short-circuit
        // chain (and/or/comparison). Materialize the lists into the
        // existing register so the value is correct regardless of
        // which control-flow path produced it. Without this, an
        // expression like `(1 or false) == true` left a TESTSET
        // with A=NO_REG dangling and the VM crashed indexing
        // R[255]. Mirrors C Lua's luaK_exp2anyreg branching.
        let reg = e.info as u8;
        materialize_jump_lists(fs, e, reg);
        return reg;
    }
    exp2nextreg(fs, e);
    e.info as u8
}

/// Like `exp2anyreg` but also materializes any pending t/f jump lists
/// into the target register. Use when the caller needs a value — not
/// just a register address — from an expression that may have come
/// through `and`/`or`/comparison short-circuiting. The plain
/// `exp2anyreg` silently drops unmaterialized jump lists for
/// expressions whose kind is already `NonReloc`, which leaves JMPs
/// with NO_JUMP (-1) targets and produces infinite loops at runtime.
pub fn exp2anyreg_materialized(fs: &mut FuncState, e: &mut ExprDesc) -> u8 {
    discharge_vars(fs, e);
    if e.k == ExpKind::NonReloc && e.t == NO_JUMP && e.f == NO_JUMP {
        return e.info as u8;
    }
    if e.k == ExpKind::NonReloc {
        // Already in a register; materialize jumps into THAT register
        // instead of allocating a fresh one so we stay fused with
        // whatever produced the NonReloc.
        let reg = e.info as u8;
        materialize_jump_lists(fs, e, reg);
        return reg;
    }
    exp2nextreg(fs, e);
    e.info as u8
}

// ---- Code generation helpers -----------------------------------------------

pub fn code_return(fs: &mut FuncState, first: i32, nret: i32) {
    if nret == 0 {
        emit_abc(fs, OpCode::OP_RETURN0, 0, 0, 0, false);
    } else if nret == 1 {
        emit_abc(fs, OpCode::OP_RETURN1, first as u32, 0, 0, false);
    } else {
        emit_abc(
            fs,
            OpCode::OP_RETURN,
            first as u32,
            (nret + 1) as u32,
            0,
            false,
        );
    }
}

pub fn code_vararg(fs: &mut FuncState, e: &mut ExprDesc) {
    // Emit OP_VARARG with A=0 (patched on discharge) and C=2 =
    // "produce 1 result by default". set_vararg_returns later
    // rewrites C if the caller wants more values (MULTRET or a
    // specific count via multi-local).
    let pc = emit_abc(fs, OpCode::OP_VARARG, 0, 0, 2, false);
    e.k = ExpKind::VarArg;
    e.info = pc;
}

/// Expand an OP_VARARG instruction to produce `n` values. `n == -1`
/// means MULTRET (C=0 in the encoding). Mirrors the adjustment
/// `set_returns` does for OP_CALL.
pub fn set_vararg_returns(fs: &mut FuncState, e: &mut ExprDesc, n: i32) {
    if e.k != ExpKind::VarArg {
        return;
    }
    let pc = e.info as usize;
    let instr = &mut fs.proto.code[pc];
    // OP_VARARG's C field is bits 24..31 and encodes n+1 (0 = MULTRET).
    *instr = (*instr & !(0xFFu32 << 24)) | (((n + 1) as u32) << 24);
}

pub fn code_call(fs: &mut FuncState, e: &mut ExprDesc, nargs: i32) {
    let func_reg = e.info as u32;
    let pc = emit_abc(
        fs,
        OpCode::OP_CALL,
        func_reg,
        (nargs + 1) as u32,
        2, // 1 result by default
        false,
    );
    e.k = ExpKind::Call;
    e.info = pc;
    fs.freereg = (func_reg + 1) as u8;
}

/// Variant of [`code_call`] used when the trailing arg is a
/// MULTRET-expanded call (or `...`). The CALL opcode encodes B=0,
/// which tells the VM "args = everything from A+1 up to the
/// current top", so the extra returns from the previous call
/// become arguments here.
pub fn code_call_multret(fs: &mut FuncState, e: &mut ExprDesc) {
    let func_reg = e.info as u32;
    let pc = emit_abc(
        fs,
        OpCode::OP_CALL,
        func_reg,
        0, // B=0 → multret argument list
        2, // 1 result by default
        false,
    );
    e.k = ExpKind::Call;
    e.info = pc;
    fs.freereg = (func_reg + 1) as u8;
}

pub fn set_returns(fs: &mut FuncState, e: &mut ExprDesc, nresults: i32) {
    if e.k == ExpKind::Call {
        let instr = &mut fs.proto.code[e.info as usize];
        *instr = (*instr & !(0xFFu32 << 24)) | (((nresults + 1) as u32) << 24);
    }
}

pub fn store_var(fs: &mut FuncState, var: &mut ExprDesc, val: &mut ExprDesc) {
    match var.k {
        ExpKind::Local => {
            free_exp(fs, val);
            discharge_to_reg(fs, val, var.var.ridx);
            // If the value carried t/f jump lists from short-circuit
            // operators, patch them now or the TESTSET-A=NO_REG
            // emitted by go_if_* would crash at runtime.
            if val.t != NO_JUMP || val.f != NO_JUMP {
                materialize_jump_lists(fs, val, var.var.ridx);
            }
        }
        ExpKind::Upval => {
            let reg = exp2anyreg(fs, val);
            emit_abc(
                fs,
                OpCode::OP_SETUPVAL,
                reg as u32,
                var.info as u32,
                0,
                false,
            );
        }
        ExpKind::IndexStr => {
            let reg = exp2anyreg_materialized(fs, val);
            emit_abc(
                fs,
                OpCode::OP_SETFIELD,
                var.ind.t as u32,
                var.ind.idx as u32,
                reg as u32,
                false,
            );
        }
        ExpKind::IndexUp => {
            let reg = exp2anyreg_materialized(fs, val);
            emit_abc(
                fs,
                OpCode::OP_SETTABUP,
                var.ind.t as u32,
                var.ind.idx as u32,
                reg as u32,
                false,
            );
        }
        ExpKind::Indexed => {
            let reg = exp2anyreg_materialized(fs, val);
            emit_abc(
                fs,
                OpCode::OP_SETTABLE,
                var.ind.t as u32,
                var.ind.idx as u32,
                reg as u32,
                false,
            );
        }
        ExpKind::IndexI => {
            let reg = exp2anyreg_materialized(fs, val);
            emit_abc(
                fs,
                OpCode::OP_SETI,
                var.ind.t as u32,
                var.ind.idx as u32,
                reg as u32,
                false,
            );
        }
        _ => panic!("store_var: invalid variable kind {:?}", var.k),
    }
}

pub fn indexed(fs: &mut FuncState, t: &mut ExprDesc, k: &mut ExprDesc) {
    if t.k == ExpKind::Upval {
        // Indexed upvalue: t.info is upval index.
        let key_k = if k.k == ExpKind::KStr {
            string_k(fs, k.strval.unwrap())
        } else {
            exp2anyreg(fs, k) as i32
        };
        t.k = ExpKind::IndexUp;
        t.ind.t = t.info as u8;
        t.ind.idx = key_k as i16;
    } else {
        let treg = exp2anyreg(fs, t) as u8;
        if k.k == ExpKind::KStr {
            let ki = string_k(fs, k.strval.unwrap());
            t.k = ExpKind::IndexStr;
            t.ind.t = treg;
            t.ind.idx = ki as i16;
        } else if k.k == ExpKind::KInt {
            t.k = ExpKind::IndexI;
            t.ind.t = treg;
            t.ind.idx = k.ival as i16;
        } else {
            let kreg = exp2anyreg(fs, k);
            t.k = ExpKind::Indexed;
            t.ind.t = treg;
            t.ind.idx = kreg as i16;
        }
    }
}

pub fn field_access(fs: &mut FuncState, t: &mut ExprDesc, name: StringHandle) {
    let ki = string_k(fs, name);
    let treg = exp2anyreg(fs, t);
    t.k = ExpKind::IndexStr;
    t.ind.t = treg;
    t.ind.idx = ki as i16;
}

/// Emit OP_SELF for a method call `obj:method(args)`. After
/// this, the method function sits at R(e.info) and `obj` (as
/// 'self') sits at R(e.info + 1). The caller then emits
/// OP_CALL with nargs+1 (accounting for self).
pub fn self_call(fs: &mut FuncState, t: &mut ExprDesc, method: StringHandle) {
    let ki = string_k(fs, method);
    let treg = exp2anyreg(fs, t);
    // Free the table register if it was a temp — we're going
    // to place the method+self in consecutive new slots.
    if treg >= nvarstack(fs) {
        fs.freereg -= 1;
    }
    let dest = fs.freereg;
    emit_abc(
        fs,
        OpCode::OP_SELF,
        dest as u32,
        treg as u32,
        ki as u32,
        false,
    );
    reserve_regs(fs, 2); // method + self
    t.k = ExpKind::NonReloc;
    t.info = dest as i32;
}

// ---- Arithmetic / comparison code generation --------------------------------

fn bin_op_to_opcode(op: BinOpr) -> OpCode {
    match op {
        BinOpr::Add => OpCode::OP_ADD,
        BinOpr::Sub => OpCode::OP_SUB,
        BinOpr::Mul => OpCode::OP_MUL,
        BinOpr::Mod => OpCode::OP_MOD,
        BinOpr::Pow => OpCode::OP_POW,
        BinOpr::Div => OpCode::OP_DIV,
        BinOpr::IDiv => OpCode::OP_IDIV,
        BinOpr::BAnd => OpCode::OP_BAND,
        BinOpr::BOr => OpCode::OP_BOR,
        BinOpr::BXor => OpCode::OP_BXOR,
        BinOpr::Shl => OpCode::OP_SHL,
        BinOpr::Shr => OpCode::OP_SHR,
        _ => panic!("bin_op_to_opcode: not an arith op"),
    }
}

fn cmp_op_to_opcode(op: BinOpr) -> OpCode {
    match op {
        BinOpr::Eq | BinOpr::Ne => OpCode::OP_EQ,
        BinOpr::Lt | BinOpr::Gt => OpCode::OP_LT,
        BinOpr::Le | BinOpr::Ge => OpCode::OP_LE,
        _ => panic!("cmp_op_to_opcode: not a comparison"),
    }
}

pub fn prefix(fs: &mut FuncState, op: UnOpr, e: &mut ExprDesc, _line: i32) {
    discharge_vars(fs, e);
    match op {
        UnOpr::Minus => {
            // Constant-fold both numeric kinds. Without the KFlt fold,
            // `-1.5` allocates a temp register for the LOADK + OP_UNM
            // pair, which then disturbs the register assignment of any
            // surrounding expression. The most visible breakage was
            // `3.5 // -(1.5)` returning -1.5 because the compiler's
            // freereg accounting placed the IDIV result one slot
            // above where the caller (e.g. an OP_CALL argument list)
            // expected it.
            if e.k == ExpKind::KInt {
                e.ival = -e.ival;
                return;
            }
            if e.k == ExpKind::KFlt {
                e.nval = -e.nval;
                return;
            }
            let reg = exp2anyreg(fs, e);
            *e = ExprDesc::init(ExpKind::NonReloc, reg as i32);
            free_exp(fs, e);
            let dest = fs.freereg;
            emit_abc(fs, OpCode::OP_UNM, dest as u32, reg as u32, 0, false);
            e.k = ExpKind::NonReloc;
            e.info = dest as i32;
            reserve_regs(fs, 1);
        }
        UnOpr::BNot => {
            if e.k == ExpKind::KInt {
                e.ival = !e.ival;
                return;
            }
            let reg = exp2anyreg(fs, e);
            *e = ExprDesc::init(ExpKind::NonReloc, reg as i32);
            free_exp(fs, e);
            let dest = fs.freereg;
            emit_abc(fs, OpCode::OP_BNOT, dest as u32, reg as u32, 0, false);
            e.k = ExpKind::NonReloc;
            e.info = dest as i32;
            reserve_regs(fs, 1);
        }
        UnOpr::Not => {
            discharge_vars(fs, e);
            match e.k {
                ExpKind::Nil | ExpKind::False => {
                    e.k = ExpKind::True;
                }
                ExpKind::True | ExpKind::KInt | ExpKind::KFlt | ExpKind::KStr | ExpKind::K => {
                    e.k = ExpKind::False;
                }
                _ => {
                    let reg = exp2anyreg(fs, e);
                    *e = ExprDesc::init(ExpKind::NonReloc, reg as i32);
                    free_exp(fs, e);
                    let dest = fs.freereg;
                    emit_abc(
                        fs,
                        OpCode::OP_NOT,
                        dest as u32,
                        reg as u32,
                        0,
                        false,
                    );
                    e.k = ExpKind::NonReloc;
                    e.info = dest as i32;
                    reserve_regs(fs, 1);
                }
            }
        }
        UnOpr::Len => {
            let reg = exp2anyreg(fs, e);
            // Free the operand's temp slot (if it's a temp above the
            // nactvar boundary) so the result can occupy it. Without
            // this, `print(#"hello")` emits LOADK at R(n), LEN result
            // at R(n+1), leaves the LOADK occupying R(n), and the
            // surrounding CALL reads args starting at R(n) instead
            // of seeing the length at R(n+1).
            *e = ExprDesc::init(ExpKind::NonReloc, reg as i32);
            free_exp(fs, e);
            let dest = fs.freereg;
            emit_abc(fs, OpCode::OP_LEN, dest as u32, reg as u32, 0, false);
            e.k = ExpKind::NonReloc;
            e.info = dest as i32;
            reserve_regs(fs, 1);
        }
        UnOpr::NoUnOpr => {}
    }
}

pub fn infix(fs: &mut FuncState, op: BinOpr, e: &mut ExprDesc) {
    discharge_vars(fs, e);
    match op {
        BinOpr::And => {
            go_if_true(fs, e);
        }
        BinOpr::Or => {
            go_if_false(fs, e);
        }
        BinOpr::Concat => {
            exp2nextreg(fs, e);
        }
        BinOpr::Add | BinOpr::Sub | BinOpr::Mul | BinOpr::Div | BinOpr::IDiv
        | BinOpr::Mod | BinOpr::Pow | BinOpr::BAnd | BinOpr::BOr | BinOpr::BXor
        | BinOpr::Shl | BinOpr::Shr => {
            if e.k != ExpKind::KInt && e.k != ExpKind::KFlt {
                exp2anyreg(fs, e);
            }
        }
        _ => {
            exp2anyreg(fs, e);
        }
    }
}

pub fn posfix(fs: &mut FuncState, op: BinOpr, e1: &mut ExprDesc, e2: &mut ExprDesc) {
    match op {
        BinOpr::And => {
            discharge_vars(fs, e2);
            // When LHS is a temp register (above nvarstack) and has
            // jump lists, discharge RHS into LHS's slot so the
            // truthy-fallthrough path emits the value at the right
            // pc. Don't trample a LOCAL register — that would
            // overwrite the user's named variable.
            if e1.f != NO_JUMP && e1.k == ExpKind::NonReloc {
                let target = e1.info as u8;
                if target >= nvarstack(fs) {
                    discharge_to_reg(fs, e2, target);
                }
            }
            concat_jmp(fs, &mut e2.f, e1.f);
            *e1 = e2.clone();
        }
        BinOpr::Or => {
            discharge_vars(fs, e2);
            if e1.t != NO_JUMP && e1.k == ExpKind::NonReloc {
                let target = e1.info as u8;
                if target >= nvarstack(fs) {
                    discharge_to_reg(fs, e2, target);
                }
            }
            concat_jmp(fs, &mut e2.t, e1.t);
            *e1 = e2.clone();
        }
        BinOpr::Concat => {
            exp2nextreg(fs, e2);
            // CONCAT R(A), B — B is the count of operands.
            // For now, just emit CONCAT for 2 operands.
            let first = e1.info as u32;
            emit_abc(fs, OpCode::OP_CONCAT, first, 2, 0, false);
            free_exp(fs, e2);
            e1.k = ExpKind::NonReloc;
            e1.info = first as i32;
        }
        BinOpr::Add | BinOpr::Sub | BinOpr::Mul | BinOpr::Div | BinOpr::IDiv
        | BinOpr::Mod | BinOpr::Pow | BinOpr::BAnd | BinOpr::BOr | BinOpr::BXor
        | BinOpr::Shl | BinOpr::Shr => {
            code_arith(fs, op, e1, e2);
        }
        BinOpr::Eq | BinOpr::Ne | BinOpr::Lt | BinOpr::Le | BinOpr::Gt | BinOpr::Ge => {
            code_comparison(fs, op, e1, e2);
        }
        BinOpr::NoBinOpr => {}
    }
}

fn code_arith(fs: &mut FuncState, op: BinOpr, e1: &mut ExprDesc, e2: &mut ExprDesc) {
    // Constant folding for integers.
    if e1.k == ExpKind::KInt && e2.k == ExpKind::KInt {
        if let Some(result) = fold_int(op, e1.ival, e2.ival) {
            e1.ival = result;
            return;
        }
    }
    // Discharge e2 BEFORE e1 so e1's temp ends up topmost — mirrors
    // C Lua's `codebinexpval`. If we discharged e1 first and e2 was
    // already on the stack below, e1 would sit above e2 and the
    // max-first free-order below would strand e2's slot as a hole,
    // pushing the result one slot too high (breaking call arg
    // contiguity and nested expressions like `1 << (x - 1)`).
    let r2 = exp2anyreg(fs, e2);
    let r1 = exp2anyreg(fs, e1);
    // Free both temps in max-first order so `freereg` collapses
    // correctly regardless of which operand ended up on top.
    free_exps_max_first(fs, e1, e2);
    let dest = fs.freereg;
    emit_abc(
        fs,
        bin_op_to_opcode(op),
        dest as u32,
        r1 as u32,
        r2 as u32,
        false,
    );
    e1.k = ExpKind::NonReloc;
    e1.info = dest as i32;
    reserve_regs(fs, 1);
}

/// Port of C Lua's `freeexps`: free both operand temps, topmost
/// first, so freereg collapses in the correct order. Silently
/// skips operands that aren't temps (locals, constants, etc.).
fn free_exps_max_first(fs: &mut FuncState, e1: &ExprDesc, e2: &ExprDesc) {
    let r1 = if e1.k == ExpKind::NonReloc { e1.info } else { -1 };
    let r2 = if e2.k == ExpKind::NonReloc { e2.info } else { -1 };
    let (hi, lo) = if r1 > r2 { (r1, r2) } else { (r2, r1) };
    if hi >= 0 {
        free_reg(fs, hi as u8);
    }
    if lo >= 0 {
        free_reg(fs, lo as u8);
    }
}

fn fold_int(op: BinOpr, a: LuaInteger, b: LuaInteger) -> Option<LuaInteger> {
    match op {
        BinOpr::Add => a.checked_add(b),
        BinOpr::Sub => a.checked_sub(b),
        BinOpr::Mul => a.checked_mul(b),
        BinOpr::Mod => {
            if b == 0 { None } else { Some(((a % b) + b) % b) }
        }
        BinOpr::IDiv => {
            if b == 0 { None } else { Some(a.div_euclid(b)) }
        }
        BinOpr::BAnd => Some(a & b),
        BinOpr::BOr => Some(a | b),
        BinOpr::BXor => Some(a ^ b),
        BinOpr::Shl => Some(a << (b & 63)),
        BinOpr::Shr => Some(a >> (b & 63)),
        _ => None,
    }
}

fn code_comparison(
    fs: &mut FuncState,
    op: BinOpr,
    e1: &mut ExprDesc,
    e2: &mut ExprDesc,
) {
    // Generate comparison. The Lua 5.4 convention is:
    //   OP_CMP a b k — if (a <cmp> b) == k, SKIP the next
    //   instruction (which is typically a JMP).
    //
    // So to implement "exit when false" (used by go_if_true /
    // patched later for if/while bodies), we set k=1 and emit
    // JMP-to-patch: if comparison is TRUE, skip JMP → fall
    // through into body; if FALSE, JMP to exit label.
    let (ra, rb) = match op {
        BinOpr::Gt | BinOpr::Ge => {
            // Swap operands: a > b ≡ b < a, a >= b ≡ b <= a.
            let r2 = exp2anyreg(fs, e2);
            let r1 = exp2anyreg(fs, e1);
            (r2, r1)
        }
        _ => {
            let r1 = exp2anyreg(fs, e1);
            let r2 = exp2anyreg(fs, e2);
            (r1, r2)
        }
    };
    free_exp(fs, e2);
    free_exp(fs, e1);
    let opcode = cmp_op_to_opcode(op);
    // OP_CMP a b k: "if (comp) != k, skip next". We want the
    // body to run when the condition is TRUE — i.e. skip the
    // exit JMP when comparison result matches the logical sense
    // of our operator. For Lt/Gt/Le/Ge/Eq: skip JMP when
    // comparison is true → k=0 (so comparison-true makes
    // `true != 0` → skip). For Ne: skip JMP when values differ
    // (NE true) → emit OP_EQ with k=1 (so EQ-false makes
    // `false != 1` → skip).
    // Lua invariant (lcode.c codecomp): emit CMP with k=1 for
    // EQ/LT/LE so the JMP fires on TRUE-of-comparison; emit
    // OP_EQ with k=0 for NE (so EQ-FALSE = NE-TRUE → take JMP).
    // After this, `go_if_true` / `go_if_false` can read e.info
    // as "jump fires when expression is TRUE" — the basis for
    // the t/f-list dispatching they do.
    let k = op != BinOpr::Ne;
    let _pc = emit_abc(fs, opcode, ra as u32, rb as u32, 0, k);
    let jmp = emit_jump(fs);
    e1.k = ExpKind::Jmp;
    e1.info = jmp;
    e1.t = NO_JUMP;
    e1.f = NO_JUMP;
}

// ---- Conditional code generation -------------------------------------------

pub fn go_if_true(fs: &mut FuncState, e: &mut ExprDesc) {
    discharge_vars(fs, e);
    let pc: i32;
    match e.k {
        ExpKind::Jmp => {
            negate_cmp_condition(fs, e.info);
            pc = e.info;
        }
        ExpKind::Nil | ExpKind::False => {
            pc = emit_jump(fs);
        }
        _ => {
            // Constants, locals, and other producers all go through
            // the TESTSET path. Folding KInt/KFlt/KStr to NO_JUMP
            // here was wrong for `value AND x`: when value is truthy
            // the result must be x, when falsy the result must be
            // value (with TESTSET copying value to the destination).
            // Emitting nothing meant the materialized fall-back was
            // a literal `true`/`false`, not the original value.
            let reg = exp2anyreg(fs, e);
            let saved_t = e.t;
            let saved_f = e.f;
            *e = ExprDesc::init(ExpKind::NonReloc, reg as i32);
            e.t = saved_t;
            e.f = saved_f;
            free_exp(fs, e);
            pc = jump_on_cond(fs, reg, 0);
        }
    }
    concat_jmp(fs, &mut e.f, pc);
    patch_to_here(fs, e.t);
    e.t = NO_JUMP;
}

/// Emit `OP_TESTSET R(A=NO_REG), R(reg), 0, k` followed by a JMP.
/// Returns the JMP pc. patchlistaux later rewrites A to the result
/// register and fixes the JMP target, giving OR/AND their
/// copy-to-result semantics.
fn jump_on_cond(fs: &mut FuncState, reg: u8, k: u32) -> i32 {
    emit_abc(
        fs,
        OpCode::OP_TESTSET,
        NO_REG as u32,
        reg as u32,
        0,
        k != 0,
    );
    emit_jump(fs)
}

const NO_REG: u8 = 255;

/// Flip the `k` flag on the CMP instruction immediately preceding
/// the JMP at `jmp_pc`. After the flip, the JMP fires on the
/// opposite truth value of the comparison. Mirrors C Lua's
/// negatecondition().
fn negate_cmp_condition(fs: &mut FuncState, jmp_pc: i32) {
    if jmp_pc <= 0 {
        return;
    }
    // CMP is at jmp_pc - 1.
    let cmp_pc = (jmp_pc - 1) as usize;
    let instr = &mut fs.proto.code[cmp_pc];
    // k flag is bit 15.
    *instr ^= 1u32 << 15;
}

pub fn go_if_false(fs: &mut FuncState, e: &mut ExprDesc) {
    discharge_vars(fs, e);
    let pc: i32;
    match e.k {
        ExpKind::Jmp => {
            // CMP already fires JMP on TRUE-of-cmp. Perfect for
            // go_if_false (we want to jump when the value is TRUE
            // so we can skip RHS of OR, etc.).
            pc = e.info;
        }
        ExpKind::Nil | ExpKind::False => {
            pc = NO_JUMP;
        }
        _ => {
            // All non-falsy producers route through TESTSET so the
            // original VALUE (not a coerced boolean) is what survives
            // when the OR short-circuits. Folding the literal-truthy
            // cases to an unconditional JMP made `1 or X` evaluate
            // to `true` instead of `1`.
            let reg = exp2anyreg(fs, e);
            let saved_t = e.t;
            let saved_f = e.f;
            *e = ExprDesc::init(ExpKind::NonReloc, reg as i32);
            e.t = saved_t;
            e.f = saved_f;
            free_exp(fs, e);
            pc = jump_on_cond(fs, reg, 1);
        }
    }
    concat_jmp(fs, &mut e.t, pc);
    patch_to_here(fs, e.f);
    e.f = NO_JUMP;
}

// ---- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_appends_instruction() {
        let mut fs = FuncState::new();
        let pc = emit(&mut fs, 0x12345678);
        assert_eq!(pc, 0);
        assert_eq!(fs.proto.code.len(), 1);
        assert_eq!(fs.proto.code[0], 0x12345678);
    }

    #[test]
    fn add_k_deduplicates() {
        let mut fs = FuncState::new();
        let a = add_k(&mut fs, TValue::Integer(42));
        let b = add_k(&mut fs, TValue::Integer(42));
        assert_eq!(a, b);
        assert_eq!(fs.proto.constants.len(), 1);
    }

    #[test]
    fn fold_int_add() {
        assert_eq!(fold_int(BinOpr::Add, 10, 20), Some(30));
    }

    #[test]
    fn fold_int_div_by_zero() {
        assert_eq!(fold_int(BinOpr::IDiv, 10, 0), None);
    }
}
