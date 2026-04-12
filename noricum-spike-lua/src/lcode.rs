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
    fs.pc += 1;
    pc
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
    if reg >= nvarstack(fs) {
        fs.freereg -= 1;
        assert_eq!(reg, fs.freereg);
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
            if n >= i32::MIN as i64 && n <= i32::MAX as i64 {
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
        _ => {
            // void / jmp — nothing to emit.
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
        return e.info as u8;
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
    let pc = emit_abc(fs, OpCode::OP_VARARG, 0, 0, 1, false);
    e.k = ExpKind::Reloc;
    e.info = pc;
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
            let reg = exp2anyreg(fs, val);
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
            let reg = exp2anyreg(fs, val);
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
            let reg = exp2anyreg(fs, val);
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
            let reg = exp2anyreg(fs, val);
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
            if e.k == ExpKind::KInt {
                e.ival = -e.ival;
                return;
            }
            let reg = exp2anyreg(fs, e);
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
            concat_jmp(fs, &mut e2.f, e1.f);
            *e1 = e2.clone();
        }
        BinOpr::Or => {
            discharge_vars(fs, e2);
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
    let r1 = exp2anyreg(fs, e1);
    let r2 = exp2anyreg(fs, e2);
    // Free operand temps BEFORE allocating the result slot so
    // the result reuses the topmost of the freed slots.
    free_exp(fs, e2);
    free_exp(fs, e1);
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
    let k = op == BinOpr::Ne;
    let _pc = emit_abc(fs, opcode, ra as u32, rb as u32, 0, k);
    let jmp = emit_jump(fs);
    e1.k = ExpKind::Jmp;
    e1.info = jmp;
    e1.t = NO_JUMP;
    e1.f = jmp;
}

// ---- Conditional code generation -------------------------------------------

pub fn go_if_true(fs: &mut FuncState, e: &mut ExprDesc) {
    discharge_vars(fs, e);
    match e.k {
        ExpKind::Jmp => {
            // Already a conditional jump; its false path is e.f
            // (set by code_comparison). Do nothing more — the
            // caller patches e.f to exit on false.
        }
        ExpKind::True | ExpKind::KInt | ExpKind::KFlt | ExpKind::KStr | ExpKind::K => {
            // Always true; no jump needed, f is NO_JUMP.
        }
        ExpKind::Nil | ExpKind::False => {
            // Always false — emit unconditional jump to exit.
            let jmp = emit_jump(fs);
            concat_jmp(fs, &mut e.f, jmp);
        }
        _ => {
            let reg = exp2anyreg(fs, e);
            // OP_TEST R(A), k=0: skip next if R(A) is FALSE (not
            // truthy). So when R(A) is truthy → fall through.
            // Emit TEST with k=1: skip next if truthy → JMP runs
            // when falsy. Actually Lua's convention: k=C flag,
            // skip if (truthy != k). k=0 means skip when truthy,
            // k=1 means skip when falsy. We want: skip JMP when
            // truthy (continue into body), so k=0.
            emit_abc(fs, OpCode::OP_TEST, reg as u32, 0, 0, false);
            let jmp = emit_jump(fs);
            concat_jmp(fs, &mut e.f, jmp);
        }
    }
}

pub fn go_if_false(fs: &mut FuncState, e: &mut ExprDesc) {
    discharge_vars(fs, e);
    match e.k {
        ExpKind::Jmp => {
            // Conditional jump — swap t and f.
            std::mem::swap(&mut e.t, &mut e.f);
        }
        ExpKind::Nil | ExpKind::False => {
            // Always false — no jump needed.
        }
        ExpKind::True | ExpKind::KInt | ExpKind::KFlt | ExpKind::KStr | ExpKind::K => {
            // Always true — emit unconditional jump.
            let jmp = emit_jump(fs);
            concat_jmp(fs, &mut e.t, jmp);
        }
        _ => {
            let reg = exp2anyreg(fs, e);
            emit_abc(fs, OpCode::OP_TEST, reg as u32, 0, 0, true);
            let jmp = emit_jump(fs);
            concat_jmp(fs, &mut e.t, jmp);
        }
    }
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
