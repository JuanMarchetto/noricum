//! lvm — the Lua bytecode interpreter.
//!
//! Port of Lua 5.4's `lvm.c` (~2500 LOC C). This is the hardest
//! single module of the port because it's where the whole language
//! semantics — arithmetic coercion, metamethod dispatch, upvalue
//! handling, coroutine yield suspension — come together behind 85
//! opcodes. Stage 5.4+ lands opcodes in batches.
//!
//! ## Stage 5.4.1 (this commit) — execution skeleton
//!
//! Minimal dispatch loop + the handful of instructions you need to
//! run a trivial function:
//!
//! * [`LuaState::execute`] — the inner interpreter loop. Runs one
//!   Lua frame until it returns. Recursive calls push a new frame
//!   and recurse; we're not CPS-converted yet (that's Stage 5 v2
//!   when coroutines land for real).
//! * `OP_MOVE` — `R(A) := R(B)`
//! * `OP_LOADI` — `R(A) := sBx` (signed integer load)
//! * `OP_LOADFALSE` / `OP_LOADTRUE` — constant bool
//! * `OP_LOADNIL` — `R(A) ..= R(A + B) := nil`
//! * `OP_RETURN0` — return zero values
//! * `OP_RETURN1` — return `R(A)`
//! * `OP_RETURN` — return `R(A) ..= R(A + B - 2)` (or up-to-top for B == 0)
//!
//! Plus the [`LuaState::invoke_lua_closure`] bridge that
//! [`LuaState::call_value`] calls for `TValue::LuaClosure` targets.
//! It sets up the frame (base, nil-pads the register window,
//! pushes a CallFrame) and delegates to `execute`.
//!
//! Everything else — arithmetic, branches, table ops, calls from
//! inside bytecode — is a stub: the opcode `match` hits the
//! fallthrough arm and panics with the opcode number so the first
//! unimplemented instruction is loud and localizable. Follow-up
//! commits replace each stub with the real semantics.

#![allow(dead_code)]

use crate::contract::{
    LClosure, LClosureHandle, LuaError, LuaResult, LuaState, TValue, TableHandle,
    UpVal, UpValHandle, UpValState,
};
use crate::lobject::{raw_arith, to_number_ns, ArithOp};
use crate::ltm::TagMethod;
use crate::lopcodes::{
    get_opcode_raw, getarg_a, getarg_b, getarg_bx, getarg_c, getarg_k, getarg_sb,
    getarg_sbx, getarg_sc, getarg_sj, OpCode,
};

// Raw u8 discriminants for the opcodes we dispatch on. Rust's
// `match` lets us pattern on these as long as they're `const`s.
const OP_MOVE_U8: u8 = OpCode::OP_MOVE as u8;
const OP_LOADI_U8: u8 = OpCode::OP_LOADI as u8;
const OP_LOADFALSE_U8: u8 = OpCode::OP_LOADFALSE as u8;
const OP_LOADTRUE_U8: u8 = OpCode::OP_LOADTRUE as u8;
const OP_LOADNIL_U8: u8 = OpCode::OP_LOADNIL as u8;
const OP_RETURN_U8: u8 = OpCode::OP_RETURN as u8;
const OP_RETURN0_U8: u8 = OpCode::OP_RETURN0 as u8;
const OP_RETURN1_U8: u8 = OpCode::OP_RETURN1 as u8;

const OP_ADD_U8: u8 = OpCode::OP_ADD as u8;
const OP_SUB_U8: u8 = OpCode::OP_SUB as u8;
const OP_MUL_U8: u8 = OpCode::OP_MUL as u8;
const OP_MOD_U8: u8 = OpCode::OP_MOD as u8;
const OP_POW_U8: u8 = OpCode::OP_POW as u8;
const OP_DIV_U8: u8 = OpCode::OP_DIV as u8;
const OP_IDIV_U8: u8 = OpCode::OP_IDIV as u8;
const OP_BAND_U8: u8 = OpCode::OP_BAND as u8;
const OP_BOR_U8: u8 = OpCode::OP_BOR as u8;
const OP_BXOR_U8: u8 = OpCode::OP_BXOR as u8;
const OP_SHL_U8: u8 = OpCode::OP_SHL as u8;
const OP_SHR_U8: u8 = OpCode::OP_SHR as u8;
const OP_UNM_U8: u8 = OpCode::OP_UNM as u8;
const OP_BNOT_U8: u8 = OpCode::OP_BNOT as u8;

const OP_LOADK_U8: u8 = OpCode::OP_LOADK as u8;
const OP_JMP_U8: u8 = OpCode::OP_JMP as u8;
const OP_EQ_U8: u8 = OpCode::OP_EQ as u8;
const OP_LT_U8: u8 = OpCode::OP_LT as u8;
const OP_LE_U8: u8 = OpCode::OP_LE as u8;
const OP_EQK_U8: u8 = OpCode::OP_EQK as u8;
const OP_EQI_U8: u8 = OpCode::OP_EQI as u8;
const OP_LTI_U8: u8 = OpCode::OP_LTI as u8;
const OP_LEI_U8: u8 = OpCode::OP_LEI as u8;
const OP_GTI_U8: u8 = OpCode::OP_GTI as u8;
const OP_GEI_U8: u8 = OpCode::OP_GEI as u8;
const OP_TEST_U8: u8 = OpCode::OP_TEST as u8;
const OP_TESTSET_U8: u8 = OpCode::OP_TESTSET as u8;

const OP_NEWTABLE_U8: u8 = OpCode::OP_NEWTABLE as u8;
const OP_GETTABLE_U8: u8 = OpCode::OP_GETTABLE as u8;
const OP_GETI_U8: u8 = OpCode::OP_GETI as u8;
const OP_GETFIELD_U8: u8 = OpCode::OP_GETFIELD as u8;
const OP_SETTABLE_U8: u8 = OpCode::OP_SETTABLE as u8;
const OP_SETI_U8: u8 = OpCode::OP_SETI as u8;
const OP_SETFIELD_U8: u8 = OpCode::OP_SETFIELD as u8;
const OP_GETTABUP_U8: u8 = OpCode::OP_GETTABUP as u8;
const OP_SETTABUP_U8: u8 = OpCode::OP_SETTABUP as u8;

const OP_CALL_U8: u8 = OpCode::OP_CALL as u8;
const OP_TAILCALL_U8: u8 = OpCode::OP_TAILCALL as u8;
const OP_FORLOOP_U8: u8 = OpCode::OP_FORLOOP as u8;
const OP_FORPREP_U8: u8 = OpCode::OP_FORPREP as u8;

const OP_ADDI_U8: u8 = OpCode::OP_ADDI as u8;
const OP_ADDK_U8: u8 = OpCode::OP_ADDK as u8;
const OP_SUBK_U8: u8 = OpCode::OP_SUBK as u8;
const OP_MULK_U8: u8 = OpCode::OP_MULK as u8;
const OP_MODK_U8: u8 = OpCode::OP_MODK as u8;
const OP_POWK_U8: u8 = OpCode::OP_POWK as u8;
const OP_DIVK_U8: u8 = OpCode::OP_DIVK as u8;
const OP_IDIVK_U8: u8 = OpCode::OP_IDIVK as u8;
const OP_BANDK_U8: u8 = OpCode::OP_BANDK as u8;
const OP_BORK_U8: u8 = OpCode::OP_BORK as u8;
const OP_BXORK_U8: u8 = OpCode::OP_BXORK as u8;
const OP_SHLI_U8: u8 = OpCode::OP_SHLI as u8;
const OP_SHRI_U8: u8 = OpCode::OP_SHRI as u8;

const OP_CLOSURE_U8: u8 = OpCode::OP_CLOSURE as u8;
const OP_GETUPVAL_U8: u8 = OpCode::OP_GETUPVAL as u8;
const OP_SETUPVAL_U8: u8 = OpCode::OP_SETUPVAL as u8;

const OP_CONCAT_U8: u8 = OpCode::OP_CONCAT as u8;
const OP_LEN_U8: u8 = OpCode::OP_LEN as u8;

const OP_SETLIST_U8: u8 = OpCode::OP_SETLIST as u8;
const OP_TFORPREP_U8: u8 = OpCode::OP_TFORPREP as u8;
const OP_TFORCALL_U8: u8 = OpCode::OP_TFORCALL as u8;
const OP_TFORLOOP_U8: u8 = OpCode::OP_TFORLOOP as u8;

const OP_VARARG_U8: u8 = OpCode::OP_VARARG as u8;
const OP_VARARGPREP_U8: u8 = OpCode::OP_VARARGPREP as u8;

/// Maximum depth of metamethod chain walks for `__index` /
/// `__newindex`. Matches `MAXTAGLOOP` in `lvm.c`.
const MAX_TAG_LOOP: u32 = 2000;

/// Build a runtime error `TValue::ShortString` from a formatted
/// message. Interns the bytes through `GlobalState::new_string`
/// so the GC can reach it.
pub(crate) fn make_error_string(gs: &mut crate::contract::GlobalState, msg: &str) -> TValue {
    let seed = gs.hash_seed;
    let handle = gs.new_string(msg.as_bytes(), seed);
    TValue::ShortString(handle)
}

impl LuaState {
    /// Build a `LuaError::Runtime` carrying a formatted error
    /// message as a short string interned on the heap. This
    /// replaces the old `LuaError::Runtime(TValue::Nil)` stubs.
    fn make_lua_error(&mut self, msg: &str) -> LuaError {
        LuaError::Runtime(make_error_string(&mut self.global, msg))
    }
}

impl LuaState {
    /// Run the bytecode interpreter for the top call frame until
    /// it returns. Expects the frame's callable slot to hold a
    /// [`TValue::LuaClosure`]; panics otherwise because this is
    /// the VM loop's precondition, not a recoverable error.
    ///
    /// Returns `Ok(())` on clean return. Errors propagate as
    /// `Err(LuaError::..)` via Result-threading; [`LuaState::pcall`]
    /// catches them.
    pub fn execute(&mut self) -> LuaResult<()> {
        loop {
            // Fetch the current instruction and advance PC.
            let (instruction, base, func_slot, n_expected) = {
                let frame = self
                    .current_call_frame()
                    .expect("execute: no active call frame");
                let pc = frame.saved_pc;
                let func_slot = frame.func;
                let n_expected = frame.n_results;
                let closure = match self.current_thread().stack[func_slot as usize] {
                    TValue::LuaClosure(h) => h,
                    other => panic!(
                        "execute: frame func slot holds non-LClosure ({:?})",
                        other
                    ),
                };
                let proto_handle = self.global.heap.lclosure(closure).proto;
                let instruction = self.global.heap.proto(proto_handle).code[pc as usize];
                let base = func_slot + 1;
                (instruction, base, func_slot, n_expected)
            };
            {
                let frame_mut = self
                    .current_thread_mut()
                    .frames
                    .last_mut()
                    .expect("execute: frame vanished mid-step");
                frame_mut.saved_pc += 1;
            }

            let opcode = get_opcode_raw(instruction);
            match opcode {
                OP_MOVE_U8 => {
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let value = self.current_thread().stack[(base + b) as usize];
                    self.current_thread_mut().stack[(base + a) as usize] = value;
                }
                OP_LOADI_U8 => {
                    let a = getarg_a(instruction) as u32;
                    let sbx = getarg_sbx(instruction);
                    self.current_thread_mut().stack[(base + a) as usize] =
                        TValue::Integer(sbx as i64);
                }
                OP_LOADFALSE_U8 => {
                    let a = getarg_a(instruction) as u32;
                    self.current_thread_mut().stack[(base + a) as usize] = TValue::False;
                }
                OP_LOADTRUE_U8 => {
                    let a = getarg_a(instruction) as u32;
                    self.current_thread_mut().stack[(base + a) as usize] = TValue::True;
                }
                OP_LOADNIL_U8 => {
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let thread = self.current_thread_mut();
                    for i in 0..=b {
                        thread.stack[(base + a + i) as usize] = TValue::Nil;
                    }
                }
                OP_LOADK_U8 => {
                    // R(A) := K[Bx] — load a constant from the
                    // proto's constant pool into a register.
                    let a = getarg_a(instruction) as u32;
                    let bx = getarg_bx(instruction) as usize;
                    let proto_handle = match self.current_thread().stack[func_slot as usize] {
                        TValue::LuaClosure(h) => self.global.heap.lclosure(h).proto,
                        _ => unreachable!("LOADK in non-Lua frame"),
                    };
                    let value = self.global.heap.proto(proto_handle).constants[bx];
                    self.current_thread_mut().stack[(base + a) as usize] = value;
                }
                OP_JMP_U8 => {
                    // Unconditional branch: pc += sJ. saved_pc
                    // was already advanced by 1, so we add the
                    // signed offset on top of the already-bumped
                    // value. Negative offsets walk backwards
                    // (loops).
                    let sj = getarg_sj(instruction);
                    let frame_mut = self
                        .current_thread_mut()
                        .frames
                        .last_mut()
                        .expect("JMP: frame vanished");
                    frame_mut.saved_pc = ((frame_mut.saved_pc as i32) + sj) as u32;
                }
                OP_EQ_U8 => {
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let rb = self.current_thread().stack[(base + b) as usize];
                    let equal = self.lua_equals_mm(ra, rb)?;
                    if equal != k {
                        self.skip_next_instruction();
                    }
                }
                OP_LT_U8 => {
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let rb = self.current_thread().stack[(base + b) as usize];
                    let less = self.lua_less_than_mm(ra, rb)?;
                    if less != k {
                        self.skip_next_instruction();
                    }
                }
                OP_LE_U8 => {
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let rb = self.current_thread().stack[(base + b) as usize];
                    let le = self.lua_less_equal_mm(ra, rb)?;
                    if le != k {
                        self.skip_next_instruction();
                    }
                }
                OP_EQK_U8 => {
                    // if (R(A) == K[B]) != k then pc++
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as usize;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let rb = self.constant_at(func_slot, b);
                    let equal = self.lua_equals_mm(ra, rb)?;
                    if equal != k {
                        self.skip_next_instruction();
                    }
                }
                OP_EQI_U8 => {
                    // if (R(A) == sB) != k then pc++
                    let a = getarg_a(instruction) as u32;
                    let sb = getarg_sb(instruction) as i64;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let equal = self.lua_equals_mm(ra, TValue::Integer(sb))?;
                    if equal != k {
                        self.skip_next_instruction();
                    }
                }
                OP_LTI_U8 => {
                    // if (R(A) < sB) != k then pc++
                    let a = getarg_a(instruction) as u32;
                    let sb = getarg_sb(instruction) as i64;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let less = self.lua_less_than_mm(ra, TValue::Integer(sb))?;
                    if less != k {
                        self.skip_next_instruction();
                    }
                }
                OP_LEI_U8 => {
                    // if (R(A) <= sB) != k then pc++
                    let a = getarg_a(instruction) as u32;
                    let sb = getarg_sb(instruction) as i64;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let le = self.lua_less_equal_mm(ra, TValue::Integer(sb))?;
                    if le != k {
                        self.skip_next_instruction();
                    }
                }
                OP_GTI_U8 => {
                    // if (R(A) > sB) != k then pc++ — i.e. sB < R(A)
                    let a = getarg_a(instruction) as u32;
                    let sb = getarg_sb(instruction) as i64;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let gt = self.lua_less_than_mm(TValue::Integer(sb), ra)?;
                    if gt != k {
                        self.skip_next_instruction();
                    }
                }
                OP_GEI_U8 => {
                    // if (R(A) >= sB) != k then pc++ — i.e. sB <= R(A)
                    let a = getarg_a(instruction) as u32;
                    let sb = getarg_sb(instruction) as i64;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let ge = self.lua_less_equal_mm(TValue::Integer(sb), ra)?;
                    if ge != k {
                        self.skip_next_instruction();
                    }
                }
                OP_TEST_U8 => {
                    // if (!R(A) == k) pc++ — skip the next
                    // instruction (typically a JMP) when the
                    // test value's truthiness doesn't match k.
                    let a = getarg_a(instruction) as u32;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    if ra.is_truthy() != k {
                        self.skip_next_instruction();
                    }
                }
                OP_TESTSET_U8 => {
                    // if (!R(B) == k) pc++ else R(A) := R(B)
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let k = getarg_k(instruction);
                    let rb = self.current_thread().stack[(base + b) as usize];
                    if rb.is_truthy() != k {
                        self.skip_next_instruction();
                    } else {
                        self.current_thread_mut().stack[(base + a) as usize] = rb;
                    }
                }
                // --- Table opcodes ---------------------------------
                OP_NEWTABLE_U8 => {
                    // R(A) := {} — Stage 5.7 ignores the B/C
                    // array/hash capacity hints and creates an
                    // empty table. Stage 5 v2 can preallocate
                    // from B and C (plus the following
                    // OP_EXTRAARG that carries the large array
                    // size) to avoid early rehashes on
                    // constructors that know their size.
                    let a = getarg_a(instruction) as u32;
                    let handle = self
                        .global
                        .heap
                        .alloc_table(crate::contract::Table::default());
                    self.current_thread_mut().stack[(base + a) as usize] =
                        TValue::Table(handle);
                    // The OP_NEWTABLE opcode is followed by an
                    // OP_EXTRAARG in Lua 5.4. Skip it so the
                    // next iteration doesn't try to dispatch it.
                    self.skip_next_instruction();
                }
                OP_GETTABLE_U8 => {
                    // R(A) := R(B)[R(C)]
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let c = getarg_c(instruction) as u32;
                    let rb = self.current_thread().stack[(base + b) as usize];
                    let rc = self.current_thread().stack[(base + c) as usize];
                    self.lua_get_index(rb, rc, base + a)?;
                }
                OP_GETI_U8 => {
                    // R(A) := R(B)[C]  — C is a small integer
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let c = getarg_c(instruction);
                    let rb = self.current_thread().stack[(base + b) as usize];
                    self.lua_get_index(rb, TValue::Integer(c as i64), base + a)?;
                }
                OP_GETFIELD_U8 => {
                    // R(A) := R(B)[K[C]]  — C indexes the constant pool
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let c = getarg_c(instruction) as usize;
                    let rb = self.current_thread().stack[(base + b) as usize];
                    let key = self.constant_at(func_slot, c);
                    self.lua_get_index(rb, key, base + a)?;
                }
                OP_SETTABLE_U8 => {
                    // R(A)[R(B)] := R/K(C)   (k flag selects K)
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let c = getarg_c(instruction) as u32;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let rb = self.current_thread().stack[(base + b) as usize];
                    let rc = self.rk_value(func_slot, base, c, k);
                    self.lua_set_index(ra, rb, rc)?;
                }
                OP_SETI_U8 => {
                    // R(A)[B] := R/K(C)
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction);
                    let c = getarg_c(instruction) as u32;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let rc = self.rk_value(func_slot, base, c, k);
                    self.lua_set_index(ra, TValue::Integer(b as i64), rc)?;
                }
                OP_SETFIELD_U8 => {
                    // R(A)[K[B]] := R/K(C)
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as usize;
                    let c = getarg_c(instruction) as u32;
                    let k = getarg_k(instruction);
                    let ra = self.current_thread().stack[(base + a) as usize];
                    let key = self.constant_at(func_slot, b);
                    let rc = self.rk_value(func_slot, base, c, k);
                    self.lua_set_index(ra, key, rc)?;
                }
                OP_GETTABUP_U8 => {
                    // R(A) := UpValue[B][K(C):shortstring]
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as usize;
                    let c = getarg_c(instruction) as usize;
                    // Read the upvalue as the target.
                    let closure_handle = match self.current_thread().stack[func_slot as usize] {
                        TValue::LuaClosure(h) => h,
                        _ => unreachable!("GETTABUP in non-Lua frame"),
                    };
                    let upv_h = self.global.heap.lclosure(closure_handle).upvalues[b];
                    let upv_val = match &self.global.heap.upval(upv_h).state {
                        UpValState::Closed(v) => *v,
                        UpValState::Open { thread, stack_index } => {
                            self.global.heap.thread(*thread).stack[*stack_index as usize]
                        }
                    };
                    let key = self.constant_at(func_slot, c);
                    self.lua_get_index(upv_val, key, base + a)?;
                }
                OP_SETTABUP_U8 => {
                    // UpValue[A][K(B):shortstring] := R/K(C)
                    let a = getarg_a(instruction) as usize;
                    let b = getarg_b(instruction) as usize;
                    let c = getarg_c(instruction) as u32;
                    let k = getarg_k(instruction);
                    let closure_handle = match self.current_thread().stack[func_slot as usize] {
                        TValue::LuaClosure(h) => h,
                        _ => unreachable!("SETTABUP in non-Lua frame"),
                    };
                    let upv_h = self.global.heap.lclosure(closure_handle).upvalues[a];
                    let upv_val = match &self.global.heap.upval(upv_h).state {
                        UpValState::Closed(v) => *v,
                        UpValState::Open { thread, stack_index } => {
                            self.global.heap.thread(*thread).stack[*stack_index as usize]
                        }
                    };
                    let key = self.constant_at(func_slot, b);
                    let rc = self.rk_value(func_slot, base, c, k);
                    self.lua_set_index(upv_val, key, rc)?;
                }
                OP_LEN_U8 => {
                    // R(A) := #R(B) — length operator with
                    // __len metamethod dispatch.
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let rb = self.current_thread().stack[(base + b) as usize];
                    let len_value = self.lua_len_mm(rb)?;
                    self.current_thread_mut().stack[(base + a) as usize] = len_value;
                }
                OP_CONCAT_U8 => {
                    // R(A) := R(A) .. R(A+1) .. ... .. R(A+B-1).
                    // B is the total number of operands to
                    // concatenate. Dispatches to `lua_concat_mm`
                    // which fuses raw string/number runs and
                    // consults `__concat` on non-coercible
                    // operands.
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    if b < 2 {
                        continue;
                    }
                    self.lua_concat_mm(base + a, b)?;
                }
                OP_CLOSURE_U8 => {
                    // R(A) := closure(inner_protos[Bx]).
                    // Reads the inner proto's upvalue descriptors
                    // to set up each captured upvalue:
                    //   in_stack=true, idx=N → open upvalue
                    //     pointing to R(N) on the current frame.
                    //   in_stack=false, idx=M → share the M-th
                    //     upvalue of the enclosing closure.
                    let a = getarg_a(instruction) as u32;
                    let bx = getarg_bx(instruction) as usize;
                    let outer_closure = match self.current_thread().stack[func_slot as usize] {
                        TValue::LuaClosure(h) => h,
                        _ => unreachable!("CLOSURE in non-Lua frame"),
                    };
                    let outer_proto = self.global.heap.lclosure(outer_closure).proto;
                    let inner_proto = self.global.heap.proto(outer_proto).inner_protos[bx];
                    let descs: Vec<_> =
                        self.global.heap.proto(inner_proto).upvalues.clone();
                    let mut upvalues = Vec::with_capacity(descs.len());
                    for desc in &descs {
                        let upv = if desc.in_stack {
                            let slot = base + desc.idx as u32;
                            self.find_or_create_open_upval(slot)
                        } else {
                            let outer_uvs = &self.global.heap.lclosure(outer_closure).upvalues;
                            outer_uvs[desc.idx as usize]
                        };
                        upvalues.push(upv);
                    }
                    let handle = self.global.heap.alloc_lclosure(LClosure {
                        proto: inner_proto,
                        upvalues,
                    });
                    self.current_thread_mut().stack[(base + a) as usize] =
                        TValue::LuaClosure(handle);
                }
                OP_GETUPVAL_U8 => {
                    // R(A) := UpValue[B]
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as usize;
                    let closure_handle = match self.current_thread().stack[func_slot as usize] {
                        TValue::LuaClosure(h) => h,
                        _ => unreachable!("GETUPVAL in non-Lua frame"),
                    };
                    let upv_handle = self.global.heap.lclosure(closure_handle).upvalues[b];
                    let value = match &self.global.heap.upval(upv_handle).state {
                        UpValState::Closed(v) => *v,
                        UpValState::Open { thread, stack_index } => {
                            // For open upvalues, read through the
                            // source thread's stack. Only the
                            // current thread is supported here —
                            // cross-thread open upvalues land with
                            // coroutine support.
                            self.global
                                .heap
                                .thread(*thread)
                                .stack[*stack_index as usize]
                        }
                    };
                    self.current_thread_mut().stack[(base + a) as usize] = value;
                }
                OP_SETUPVAL_U8 => {
                    // UpValue[B] := R(A)
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as usize;
                    let value = self.current_thread().stack[(base + a) as usize];
                    let closure_handle = match self.current_thread().stack[func_slot as usize] {
                        TValue::LuaClosure(h) => h,
                        _ => unreachable!("SETUPVAL in non-Lua frame"),
                    };
                    let upv_handle = self.global.heap.lclosure(closure_handle).upvalues[b];
                    // For closed upvalues, overwrite in place.
                    // For open upvalues, write through to the
                    // source slot.
                    match self.global.heap.upval(upv_handle).state.clone() {
                        UpValState::Closed(_) => {
                            self.global.heap.upval_mut(upv_handle).state =
                                UpValState::Closed(value);
                        }
                        UpValState::Open { thread, stack_index } => {
                            self.global
                                .heap
                                .thread_mut(thread)
                                .stack[stack_index as usize] = value;
                        }
                    }
                }
                OP_FORPREP_U8 => {
                    // Initialize a numeric for loop. R(A) =
                    // initial, R(A+1) = limit, R(A+2) = step.
                    // Stage 5.9 implements the integer-only
                    // case; float loops fall through to the
                    // unimplemented panic.
                    let a = getarg_a(instruction) as u32;
                    let bx = getarg_bx(instruction) as u32;
                    let init = self.current_thread().stack[(base + a) as usize];
                    let limit = self.current_thread().stack[(base + a + 1) as usize];
                    let step = self.current_thread().stack[(base + a + 2) as usize];
                    match (init, limit, step) {
                        (
                            TValue::Integer(init),
                            TValue::Integer(limit),
                            TValue::Integer(step),
                        ) => {
                            if step == 0 {
                                return Err(self.make_lua_error(
                                    "'for' step is zero",
                                ));
                            }
                            // Empty loop: (step > 0 && init > limit)
                            // or (step < 0 && init < limit)
                            let skip = (step > 0 && init > limit)
                                || (step < 0 && init < limit);
                            if skip {
                                // Jump past the loop body and the
                                // trailing FORLOOP instruction. C
                                // Lua encodes this as "pc += Bx + 1"
                                // because Bx is offset-to-FORLOOP
                                // and we want to skip FORLOOP too.
                                let frame_mut = self
                                    .current_thread_mut()
                                    .frames
                                    .last_mut()
                                    .expect("FORPREP: no frame");
                                frame_mut.saved_pc += bx + 1;
                            } else {
                                // R(A+3) = R(A) — the visible loop
                                // variable, separate from the
                                // internal counter R(A).
                                let thread = self.current_thread_mut();
                                thread.stack[(base + a + 3) as usize] =
                                    TValue::Integer(init);
                            }
                        }
                        _ => {
                            return Err(self.make_lua_error(
                                "'for' initial value must be a number",
                            ));
                        }
                    }
                }
                OP_FORLOOP_U8 => {
                    // R(A) += R(A+2); if the loop still runs,
                    // R(A+3) = R(A); pc -= Bx.
                    let a = getarg_a(instruction) as u32;
                    let bx = getarg_bx(instruction) as u32;
                    let counter = self.current_thread().stack[(base + a) as usize];
                    let limit = self.current_thread().stack[(base + a + 1) as usize];
                    let step = self.current_thread().stack[(base + a + 2) as usize];
                    match (counter, limit, step) {
                        (
                            TValue::Integer(counter),
                            TValue::Integer(limit),
                            TValue::Integer(step),
                        ) => {
                            let next = counter.wrapping_add(step);
                            // Loop continues if next is still
                            // within [limit] in the direction of
                            // step.
                            let still_running = if step > 0 {
                                next <= limit
                            } else {
                                next >= limit
                            };
                            if still_running {
                                let thread = self.current_thread_mut();
                                thread.stack[(base + a) as usize] =
                                    TValue::Integer(next);
                                thread.stack[(base + a + 3) as usize] =
                                    TValue::Integer(next);
                                let frame_mut = thread
                                    .frames
                                    .last_mut()
                                    .expect("FORLOOP: no frame");
                                // Jump back: saved_pc -= Bx. pc
                                // was already advanced past
                                // FORLOOP, so this points at the
                                // first instruction of the loop
                                // body.
                                frame_mut.saved_pc = frame_mut.saved_pc.wrapping_sub(bx);
                            }
                        }
                        _ => {
                            return Err(self.make_lua_error(
                                "'for' loop variables must be numbers",
                            ));
                        }
                    }
                }
                OP_CALL_U8 => {
                    // Nested function call from inside bytecode.
                    //   R(A), R(A+1), ..., R(A+B-1) := R(A)(R(A+1), ...)
                    // B == 0 means "args go up to current top".
                    // C is encoded as n_returns + 1 (0 means
                    // MULTRET, 1 means 0 returns, 2 means 1 return).
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let c = getarg_c(instruction) as u32;

                    let func_abs = base + a;
                    let n_args = if b == 0 {
                        self.current_thread()
                            .top
                            .saturating_sub(func_abs + 1)
                    } else {
                        b - 1
                    };
                    // call_value's precondition: top == func_abs + 1 + n_args.
                    self.current_thread_mut().top = func_abs + 1 + n_args;

                    let n_results: i16 = if c == 0 {
                        -1
                    } else {
                        (c - 1) as i16
                    };
                    self.call_value(func_abs, n_args, n_results)?;

                    // After the callee returns, restore the
                    // caller's frame top so subsequent
                    // instructions see their full register
                    // window. The callee's results already sit
                    // at R(A)..R(A + (n_results or actual)),
                    // which is below this frame top.
                    let caller_top = self
                        .current_call_frame()
                        .expect("after CALL: caller frame vanished")
                        .top;
                    if self.current_thread().top < caller_top {
                        let thread = self.current_thread_mut();
                        thread.top = caller_top;
                    }
                }
                OP_TAILCALL_U8 => {
                    // return R(A)(R(A+1), ..., R(A+B-1))
                    //
                    // We implement it as a nested call that
                    // inherits the current frame's `n_expected`,
                    // followed by an immediate return — correct
                    // semantics but not a true in-place frame
                    // reuse (the Rust call stack still grows by
                    // one frame per Lua tail call). Stage 5 v2
                    // revisits this once the CPS rewrite lands.
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let func_abs = base + a;
                    let n_args = if b == 0 {
                        self.current_thread()
                            .top
                            .saturating_sub(func_abs + 1)
                    } else {
                        b - 1
                    };
                    self.current_thread_mut().top = func_abs + 1 + n_args;
                    self.call_value(func_abs, n_args, n_expected)?;
                    // Move results from func_abs down to
                    // func_slot so the caller sees them at the
                    // expected location.
                    let n_actually = self
                        .current_thread()
                        .top
                        .saturating_sub(func_abs);
                    let n_returned = if n_expected < 0 {
                        n_actually
                    } else {
                        n_expected as u32
                    };
                    for i in 0..n_returned {
                        let v = self.current_thread().stack[(func_abs + i) as usize];
                        self.current_thread_mut().stack[(func_slot + i) as usize] = v;
                    }
                    // Pop this frame.
                    let _ = self.pop_call_frame();
                    self.current_thread_mut().top = func_slot + n_returned;
                    return Ok(());
                }
                // Binary arithmetic opcodes — R(A) := R(B) OP R(C).
                // Every variant dispatches through lobject::raw_arith
                // so integer/float coercion, error on non-numbers,
                // and floor-division semantics all stay in one place.
                OP_ADD_U8 => self.exec_arith_binary(base, instruction, ArithOp::Add)?,
                OP_SUB_U8 => self.exec_arith_binary(base, instruction, ArithOp::Sub)?,
                OP_MUL_U8 => self.exec_arith_binary(base, instruction, ArithOp::Mul)?,
                OP_MOD_U8 => self.exec_arith_binary(base, instruction, ArithOp::Mod)?,
                OP_POW_U8 => self.exec_arith_binary(base, instruction, ArithOp::Pow)?,
                OP_DIV_U8 => self.exec_arith_binary(base, instruction, ArithOp::Div)?,
                OP_IDIV_U8 => self.exec_arith_binary(base, instruction, ArithOp::IDiv)?,
                OP_BAND_U8 => self.exec_arith_binary(base, instruction, ArithOp::BAnd)?,
                OP_BOR_U8 => self.exec_arith_binary(base, instruction, ArithOp::BOr)?,
                OP_BXOR_U8 => self.exec_arith_binary(base, instruction, ArithOp::BXor)?,
                OP_SHL_U8 => self.exec_arith_binary(base, instruction, ArithOp::Shl)?,
                OP_SHR_U8 => self.exec_arith_binary(base, instruction, ArithOp::Shr)?,
                // K-variants: R(A) := R(B) op K[C]. K[C] is a
                // constant from the proto's constant pool.
                OP_ADDK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::Add)?,
                OP_SUBK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::Sub)?,
                OP_MULK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::Mul)?,
                OP_MODK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::Mod)?,
                OP_POWK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::Pow)?,
                OP_DIVK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::Div)?,
                OP_IDIVK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::IDiv)?,
                OP_BANDK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::BAnd)?,
                OP_BORK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::BOr)?,
                OP_BXORK_U8 => self.exec_arith_k(func_slot, base, instruction, ArithOp::BXor)?,
                // I-variants: R(A) := R(B) op sC (small signed
                // immediate). Only ADDI, SHLI, and SHRI exist
                // in Lua 5.4's opcode set.
                OP_ADDI_U8 => self.exec_arith_i(base, instruction, ArithOp::Add)?,
                OP_SHLI_U8 => self.exec_arith_i(base, instruction, ArithOp::Shl)?,
                OP_SHRI_U8 => self.exec_arith_i(base, instruction, ArithOp::Shr)?,
                // Unary arithmetic — R(A) := OP R(B).
                OP_UNM_U8 => self.exec_arith_unary(base, instruction, ArithOp::Unm)?,
                OP_BNOT_U8 => self.exec_arith_unary(base, instruction, ArithOp::BNot)?,
                OP_RETURN0_U8 => {
                    self.finish_vm_return(func_slot, base, 0, 0, n_expected);
                    return Ok(());
                }
                OP_RETURN1_U8 => {
                    let a = getarg_a(instruction) as u32;
                    self.finish_vm_return(func_slot, base, a, 1, n_expected);
                    return Ok(());
                }
                OP_RETURN_U8 => {
                    let a = getarg_a(instruction) as u32;
                    let b = getarg_b(instruction) as u32;
                    let n_to_return = if b == 0 {
                        // "Up to the current top" — results run
                        // from R(A) to top.
                        let top = self.current_thread().top;
                        top.saturating_sub(base + a)
                    } else {
                        b - 1
                    };
                    self.finish_vm_return(func_slot, base, a, n_to_return, n_expected);
                    return Ok(());
                }
                OP_SETLIST_U8 => {
                    // R(A)[vC+i] := R(A+i), 1 <= i <= vB.
                    // vB == 0 means "up to top".
                    // vC is the base offset (0-based in the
                    // instruction, but array indices are 1-based
                    // at the Lua level).
                    let a = getarg_a(instruction) as u32;
                    let vb = crate::lopcodes::getarg_vb(instruction) as u32;
                    let vc = crate::lopcodes::getarg_vc(instruction) as u32;
                    let table_handle = match self.current_thread().stack[(base + a) as usize] {
                        TValue::Table(h) => h,
                        _ => return Err(self.make_lua_error(
                            "SETLIST target is not a table",
                        )),
                    };
                    let n = if vb == 0 {
                        let top = self.current_thread().top;
                        top.saturating_sub(base + a + 1)
                    } else {
                        vb
                    };
                    for i in 1..=n {
                        let v = self.current_thread().stack[(base + a + i) as usize];
                        self.global
                            .table_set_int(table_handle, (vc + i) as i64, v);
                    }
                    if vb == 0 {
                        let caller_top = self
                            .current_call_frame()
                            .expect("SETLIST: frame vanished")
                            .top;
                        self.current_thread_mut().top = caller_top;
                    }
                }
                OP_TFORPREP_U8 => {
                    // Generic-for prep: swap R(A+2) ↔ R(A+3)
                    // (closing ↔ control), then jump forward by
                    // Bx to the TFORCALL at the bottom of the
                    // loop body. TBC upvalue creation skipped
                    // for now (deferred until close semantics
                    // are implemented).
                    let a = getarg_a(instruction) as u32;
                    let bx = getarg_bx(instruction);
                    {
                        let thread = self.current_thread_mut();
                        let slot2 = (base + a + 2) as usize;
                        let slot3 = (base + a + 3) as usize;
                        thread.stack.swap(slot2, slot3);
                    }
                    let frame_mut = self
                        .current_thread_mut()
                        .frames
                        .last_mut()
                        .expect("TFORPREP: frame vanished");
                    frame_mut.saved_pc =
                        ((frame_mut.saved_pc as i32) + bx) as u32;
                }
                OP_TFORCALL_U8 => {
                    // Call the iterator:
                    //   R(A+3) = R(A), R(A+4) = R(A+1), R(A+5) = R(A+3_old_ctrl)
                    //   R(A+3), ..., R(A+3+C) := call(R(A+3), 2 args, C results)
                    let a = getarg_a(instruction) as u32;
                    let c = getarg_c(instruction) as u32;
                    // Read originals before overwriting.
                    let ctrl = self.current_thread().stack[(base + a + 3) as usize];
                    let state = self.current_thread().stack[(base + a + 1) as usize];
                    let func = self.current_thread().stack[(base + a) as usize];
                    {
                        let thread = self.current_thread_mut();
                        thread.stack[(base + a + 5) as usize] = ctrl;
                        thread.stack[(base + a + 4) as usize] = state;
                        thread.stack[(base + a + 3) as usize] = func;
                        thread.top = base + a + 3 + 3;
                    }
                    let n_results: i16 = if c == 0 { -1 } else { c as i16 };
                    self.call_value(base + a + 3, 2, n_results)?;
                    // Restore caller frame top.
                    let caller_top = self
                        .current_call_frame()
                        .expect("after TFORCALL: caller frame vanished")
                        .top;
                    if self.current_thread().top < caller_top {
                        self.current_thread_mut().top = caller_top;
                    }
                }
                OP_TFORLOOP_U8 => {
                    // if R(A+3) != nil then pc -= Bx (continue loop).
                    let a = getarg_a(instruction) as u32;
                    let bx = getarg_bx(instruction);
                    let r_a3 = self.current_thread().stack[(base + a + 3) as usize];
                    if !matches!(r_a3, TValue::Nil) {
                        let frame_mut = self
                            .current_thread_mut()
                            .frames
                            .last_mut()
                            .expect("TFORLOOP: frame vanished");
                        frame_mut.saved_pc =
                            ((frame_mut.saved_pc as i32) - bx) as u32;
                    }
                }
                OP_VARARGPREP_U8 => {
                    // Relocate the function + fixed params above
                    // any extras passed on the current frame. A is
                    // the number of declared fixed parameters.
                    let n_fixed = getarg_a(instruction) as u32;
                    self.adjust_varargs(func_slot, n_fixed);
                }
                OP_VARARG_U8 => {
                    // R(A), R(A+1), ..., R(A+C-2) := vararg.
                    // If C == 0, copy all available extras.
                    let a = getarg_a(instruction) as u32;
                    let c = getarg_c(instruction) as u32;
                    let wanted = if c == 0 { None } else { Some(c - 1) };
                    self.get_varargs(base + a, wanted);
                }
                _ => {
                    panic!(
                        "lvm: unimplemented opcode {} at pc {}",
                        opcode,
                        self.current_call_frame().unwrap().saved_pc - 1
                    );
                }
            }
        }
    }

    /// Advance the saved PC by one instruction. Used after
    /// comparison / TEST opcodes that "skip" the instruction
    /// that follows (typically a JMP) when the condition
    /// doesn't match the `k` flag.
    fn skip_next_instruction(&mut self) {
        let frame_mut = self
            .current_thread_mut()
            .frames
            .last_mut()
            .expect("skip: no frame");
        frame_mut.saved_pc += 1;
    }

    /// Read the constant at index `c` from the Proto of the
    /// closure currently executing (whose slot on the stack is
    /// `func_slot`). Used by OP_LOADK, OP_GETFIELD,
    /// OP_SETFIELD, and the K variants of SETTABLE/SETI.
    fn constant_at(&self, func_slot: u32, c: usize) -> TValue {
        let proto_handle = match self.current_thread().stack[func_slot as usize] {
            TValue::LuaClosure(h) => self.global.heap.lclosure(h).proto,
            _ => unreachable!("constant_at in non-Lua frame"),
        };
        self.global.heap.proto(proto_handle).constants[c]
    }

    /// Read an "RK" value — either a register `R(c)` (when `k`
    /// is false) or a constant `K[c]` (when `k` is true). This
    /// is the standard Lua encoding for opcodes that accept
    /// "register or constant" for the final operand.
    fn rk_value(&self, func_slot: u32, base: u32, c: u32, k: bool) -> TValue {
        if k {
            self.constant_at(func_slot, c as usize)
        } else {
            self.current_thread().stack[(base + c) as usize]
        }
    }

    /// Dispatch a binary arithmetic opcode (R(A) := R(B) op R(C)).
    /// Reads the operands out of the current frame, calls
    /// [`crate::lobject::raw_arith`], writes the result back to
    /// R(A). Falls back to a binary metamethod (`__add`,
    /// `__sub`, ...) when neither operand admits a raw
    /// computation. Returns `Err` when raw_arith reports a
    /// domain error (e.g. integer div-by-zero).
    fn exec_arith_binary(
        &mut self,
        base: u32,
        instruction: u32,
        op: ArithOp,
    ) -> LuaResult<()> {
        let a = getarg_a(instruction) as u32;
        let b = getarg_b(instruction) as u32;
        let c = getarg_c(instruction) as u32;
        let rb = self.current_thread().stack[(base + b) as usize];
        let rc = self.current_thread().stack[(base + c) as usize];
        let result = raw_arith(op, &rb, &rc)?;
        match result {
            Some(v) => {
                self.current_thread_mut().stack[(base + a) as usize] = v;
                Ok(())
            }
            None => self.try_binary_metamethod(op, rb, rc, base + a),
        }
    }

    /// K-variant arithmetic dispatcher — R(A) := R(B) op K[C].
    /// Same shape as [`LuaState::exec_arith_binary`] but the
    /// right operand comes from the constant pool.
    fn exec_arith_k(
        &mut self,
        func_slot: u32,
        base: u32,
        instruction: u32,
        op: ArithOp,
    ) -> LuaResult<()> {
        let a = getarg_a(instruction) as u32;
        let b = getarg_b(instruction) as u32;
        let c = getarg_c(instruction) as usize;
        let rb = self.current_thread().stack[(base + b) as usize];
        let rc = self.constant_at(func_slot, c);
        let result = raw_arith(op, &rb, &rc)?;
        match result {
            Some(v) => {
                self.current_thread_mut().stack[(base + a) as usize] = v;
                Ok(())
            }
            None => self.try_binary_metamethod(op, rb, rc, base + a),
        }
    }

    /// I-variant arithmetic dispatcher — R(A) := R(B) op sC
    /// (where sC is a small signed immediate encoded in the
    /// C field). Only OP_ADDI, OP_SHLI, and OP_SHRI use this
    /// path in Lua 5.4.
    fn exec_arith_i(
        &mut self,
        base: u32,
        instruction: u32,
        op: ArithOp,
    ) -> LuaResult<()> {
        let a = getarg_a(instruction) as u32;
        let b = getarg_b(instruction) as u32;
        let sc = getarg_sc(instruction) as i64;
        let rb = self.current_thread().stack[(base + b) as usize];
        let immediate = TValue::Integer(sc);
        let result = raw_arith(op, &rb, &immediate)?;
        match result {
            Some(v) => {
                self.current_thread_mut().stack[(base + a) as usize] = v;
                Ok(())
            }
            None => self.try_binary_metamethod(op, rb, immediate, base + a),
        }
    }

    /// Unary arithmetic dispatcher — R(A) := op R(B). Follows
    /// raw_arith's convention of passing the same operand twice
    /// for unary ops so the type-check short-circuit sees a
    /// valid number on both sides (the result ignores the
    /// right operand's value).
    fn exec_arith_unary(
        &mut self,
        base: u32,
        instruction: u32,
        op: ArithOp,
    ) -> LuaResult<()> {
        let a = getarg_a(instruction) as u32;
        let b = getarg_b(instruction) as u32;
        let rb = self.current_thread().stack[(base + b) as usize];
        let result = raw_arith(op, &rb, &rb)?;
        match result {
            Some(v) => {
                self.current_thread_mut().stack[(base + a) as usize] = v;
                Ok(())
            }
            // Unary metamethods (__unm, __bnot) also receive two
            // operand slots in C Lua — the operand is duplicated.
            None => self.try_binary_metamethod(op, rb, rb, base + a),
        }
    }

    // ------------------------------------------------------------------
    // Open-upvalue management.
    //
    // When `OP_CLOSURE` captures a local from the enclosing frame
    // (`in_stack = true`), we either reuse an existing open upvalue
    // pointing to that stack slot or create a fresh one and push it
    // onto `Thread::open_upvals` so multiple closures sharing the
    // same local see the same `UpVal`.
    // ------------------------------------------------------------------

    /// Find an existing open upvalue for `stack_index` on the
    /// current thread, or create a new one and register it.
    /// Returns the handle.
    pub(crate) fn find_or_create_open_upval(
        &mut self,
        stack_index: u32,
    ) -> UpValHandle {
        let thread_handle = self.current_thread;
        // Check if an open upvalue already points here.
        for &uv_h in &self.current_thread().open_upvals {
            if let UpValState::Open {
                stack_index: si, ..
            } = &self.global.heap.upval(uv_h).state
            {
                if *si == stack_index {
                    return uv_h;
                }
            }
        }
        let uv = self.global.heap.alloc_upval(UpVal {
            state: UpValState::Open {
                thread: thread_handle,
                stack_index,
            },
        });
        self.current_thread_mut().open_upvals.push(uv);
        uv
    }

    // ------------------------------------------------------------------
    // Vararg handling — OP_VARARGPREP / OP_VARARG.
    //
    // `adjust_varargs` mirrors `luaT_adjustvarargs` in ltm.c: at
    // function entry, it copies `func` and the fixed params above
    // the currently-passed arguments so that any extras live just
    // below the new function slot. The CallFrame's `func` and
    // `top` fields are bumped in place so the rest of execute()
    // sees the new base without any per-opcode overhead.
    //
    // `get_varargs` mirrors `luaT_getvarargs`: it reads extras
    // from `frame.func - n_extra_args .. frame.func` and lands
    // them starting at `dest_slot`, padding with nil when the
    // caller asks for more than are available.
    // ------------------------------------------------------------------

    /// VARARGPREP handler. Extracts any extras passed above
    /// the declared fixed parameters into `frame.varargs`,
    /// then trims `top` down to `func + 1 + n_fixed` so the
    /// register window starts cleanly at the first fixed param.
    pub(crate) fn adjust_varargs(&mut self, func_slot: u32, n_fixed: u32) {
        let thread = self.current_thread_mut();
        let actual = thread.top.saturating_sub(func_slot + 1);
        let n_extra = actual.saturating_sub(n_fixed);
        let mut captured = Vec::with_capacity(n_extra as usize);
        for i in 0..n_extra {
            let v = thread.stack[(func_slot + 1 + n_fixed + i) as usize];
            captured.push(v);
            thread.stack[(func_slot + 1 + n_fixed + i) as usize] = TValue::Nil;
        }
        thread.top = func_slot + 1 + n_fixed;
        thread
            .frames
            .last_mut()
            .expect("adjust_varargs: no frame")
            .varargs = captured;
    }

    /// Copy varargs into `dest_slot..`. `wanted == None` (i.e.
    /// `OP_VARARG C == 0`) copies all available extras and
    /// bumps `top` accordingly; otherwise `wanted` extras are
    /// copied with nil padding and `top` is not touched.
    pub(crate) fn get_varargs(&mut self, dest_slot: u32, wanted: Option<u32>) {
        let extras = {
            let frame = self
                .current_call_frame()
                .expect("get_varargs: no frame");
            frame.varargs.clone()
        };
        let n_extra = extras.len() as u32;
        let n = wanted.unwrap_or(n_extra);
        let thread = self.current_thread_mut();
        if dest_slot + n > thread.stack.len() as u32 {
            let needed = (dest_slot + n).saturating_sub(thread.top);
            thread.grow_stack(needed);
        }
        for i in 0..n {
            let v = if i < n_extra {
                extras[i as usize]
            } else {
                TValue::Nil
            };
            thread.stack[(dest_slot + i) as usize] = v;
        }
        if wanted.is_none() {
            thread.top = dest_slot + n_extra;
        }
    }

    // ------------------------------------------------------------------
    // Arithmetic metamethod dispatch (__add, __sub, __unm, ...).
    //
    // Matches `luaT_trybinTM` in ltm.c. Reads the metamethod from
    // either operand (left first), pushes a scratch call frame,
    // and writes the return value into `dest_slot`.
    // ------------------------------------------------------------------

    /// Map an [`ArithOp`] to the corresponding [`TagMethod`].
    fn arith_tag_method(op: ArithOp) -> TagMethod {
        match op {
            ArithOp::Add => TagMethod::Add,
            ArithOp::Sub => TagMethod::Sub,
            ArithOp::Mul => TagMethod::Mul,
            ArithOp::Mod => TagMethod::Mod,
            ArithOp::Pow => TagMethod::Pow,
            ArithOp::Div => TagMethod::Div,
            ArithOp::IDiv => TagMethod::IDiv,
            ArithOp::BAnd => TagMethod::BAnd,
            ArithOp::BOr => TagMethod::BOr,
            ArithOp::BXor => TagMethod::BXor,
            ArithOp::Shl => TagMethod::Shl,
            ArithOp::Shr => TagMethod::Shr,
            ArithOp::Unm => TagMethod::Unm,
            ArithOp::BNot => TagMethod::BNot,
        }
    }

    /// Look up a binary (or unary) arithmetic metamethod on
    /// either operand (left first, right as fallback), call it
    /// as `metamethod(a, b)`, and write the return value into
    /// `dest_slot`. Returns the original error shape when
    /// neither operand provides the metamethod — that's a
    /// genuine type error at the Lua level.
    pub(crate) fn try_binary_metamethod(
        &mut self,
        op: ArithOp,
        a: TValue,
        b: TValue,
        dest_slot: u32,
    ) -> LuaResult<()> {
        let event = Self::arith_tag_method(op);
        let mut tm = self.global.get_metamethod(a, event);
        if matches!(tm, TValue::Nil) {
            tm = self.global.get_metamethod(b, event);
        }
        if matches!(tm, TValue::Nil) || !Self::is_callable(tm) {
            return Err(self.make_lua_error(
                "attempt to perform arithmetic on a non-numeric value",
            ));
        }
        self.call_binary_metamethod(tm, a, b, dest_slot)
    }

    /// Invoke a binary metamethod `tm(a, b)` and land the
    /// single return value in `dest_slot`. Uses a scratch stack
    /// region above the current top.
    fn call_binary_metamethod(
        &mut self,
        tm: TValue,
        a: TValue,
        b: TValue,
        dest_slot: u32,
    ) -> LuaResult<()> {
        let saved_top = self.current_thread().top;
        let func_slot = saved_top;
        {
            let thread = self.current_thread_mut();
            thread.grow_stack(3);
            thread.stack[func_slot as usize] = tm;
            thread.stack[(func_slot + 1) as usize] = a;
            thread.stack[(func_slot + 2) as usize] = b;
            thread.top = func_slot + 3;
        }
        self.call_value(func_slot, 2, 1)?;
        let result = self.current_thread().stack[func_slot as usize];
        self.current_thread_mut().stack[dest_slot as usize] = result;
        self.current_thread_mut().top = saved_top;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Comparison / concat / length metamethod dispatch.
    //
    // lua_equals_mm / lua_less_than_mm / lua_less_equal_mm invoke
    // __eq / __lt / __le when the raw numeric/string path can't
    // decide. lua_concat_mm handles multi-arg concat with __concat
    // fallback. lua_len_mm handles #t with __len.
    // ------------------------------------------------------------------

    /// Lua equality with metamethod fallback. Mirrors
    /// `luaV_equalobj` — raw equality first, then same-type
    /// `__eq` metamethod lookup (only when both operands are of
    /// a type that can have __eq, i.e. tables and userdata).
    pub(crate) fn lua_equals_mm(
        &mut self,
        a: TValue,
        b: TValue,
    ) -> LuaResult<bool> {
        if let Some(decided) = lua_raw_equals(&a, &b) {
            return Ok(decided);
        }
        // Undecided: must be same-type tables or userdata whose
        // identity differs. Try __eq on either operand.
        let mut tm = self.global.get_metamethod(a, TagMethod::Eq);
        if matches!(tm, TValue::Nil) {
            tm = self.global.get_metamethod(b, TagMethod::Eq);
        }
        if matches!(tm, TValue::Nil) || !Self::is_callable(tm) {
            return Ok(false);
        }
        let result = self.call_binary_metamethod_to_register(tm, a, b)?;
        Ok(result.is_truthy())
    }

    /// Lua `<` with metamethod fallback. Adds
    /// byte-lexicographic string comparison on top of the
    /// numeric fast path, then `__lt` on either operand.
    pub(crate) fn lua_less_than_mm(
        &mut self,
        a: TValue,
        b: TValue,
    ) -> LuaResult<bool> {
        if let Some(decided) = lua_raw_less_than(&a, &b) {
            return Ok(decided);
        }
        if let Some(decided) = self.try_string_compare(a, b, /*strict*/ true) {
            return Ok(decided);
        }
        let mut tm = self.global.get_metamethod(a, TagMethod::Lt);
        if matches!(tm, TValue::Nil) {
            tm = self.global.get_metamethod(b, TagMethod::Lt);
        }
        if matches!(tm, TValue::Nil) || !Self::is_callable(tm) {
            return Err(self.make_lua_error(
                "attempt to compare two values",
            ));
        }
        let result = self.call_binary_metamethod_to_register(tm, a, b)?;
        Ok(result.is_truthy())
    }

    /// Lua `<=` with metamethod fallback.
    pub(crate) fn lua_less_equal_mm(
        &mut self,
        a: TValue,
        b: TValue,
    ) -> LuaResult<bool> {
        if let Some(decided) = lua_raw_less_equal(&a, &b) {
            return Ok(decided);
        }
        if let Some(decided) = self.try_string_compare(a, b, /*strict*/ false) {
            return Ok(decided);
        }
        let mut tm = self.global.get_metamethod(a, TagMethod::Le);
        if matches!(tm, TValue::Nil) {
            tm = self.global.get_metamethod(b, TagMethod::Le);
        }
        if matches!(tm, TValue::Nil) || !Self::is_callable(tm) {
            return Err(self.make_lua_error(
                "attempt to compare two values",
            ));
        }
        let result = self.call_binary_metamethod_to_register(tm, a, b)?;
        Ok(result.is_truthy())
    }

    /// Byte-lexicographic comparison for string operands.
    /// Returns `Some(bool)` when both sides are strings,
    /// `None` otherwise so the caller can try __lt / __le.
    fn try_string_compare(
        &self,
        a: TValue,
        b: TValue,
        strict: bool,
    ) -> Option<bool> {
        let (ha, hb) = match (a, b) {
            (
                TValue::ShortString(ha) | TValue::LongString(ha),
                TValue::ShortString(hb) | TValue::LongString(hb),
            ) => (ha, hb),
            _ => return None,
        };
        let sa = self.global.heap.string(ha).bytes.as_slice();
        let sb = self.global.heap.string(hb).bytes.as_slice();
        Some(if strict { sa < sb } else { sa <= sb })
    }

    /// Invoke a metamethod with two args and return its single
    /// return value, leaving the stack top where it was. Shared
    /// by the __eq / __lt / __le / __concat / __len paths where
    /// the result is consumed in a register slot known only to
    /// the caller.
    fn call_binary_metamethod_to_register(
        &mut self,
        tm: TValue,
        a: TValue,
        b: TValue,
    ) -> LuaResult<TValue> {
        let saved_top = self.current_thread().top;
        let func_slot = saved_top;
        {
            let thread = self.current_thread_mut();
            thread.grow_stack(3);
            thread.stack[func_slot as usize] = tm;
            thread.stack[(func_slot + 1) as usize] = a;
            thread.stack[(func_slot + 2) as usize] = b;
            thread.top = func_slot + 3;
        }
        self.call_value(func_slot, 2, 1)?;
        let result = self.current_thread().stack[func_slot as usize];
        self.current_thread_mut().top = saved_top;
        Ok(result)
    }

    /// Length operation with `__len` metamethod fallback.
    /// Handles strings (raw byte length), tables (luaH_getn or
    /// __len if present), and userdata (data length or __len).
    pub(crate) fn lua_len_mm(&mut self, v: TValue) -> LuaResult<TValue> {
        match v {
            TValue::ShortString(h) | TValue::LongString(h) => Ok(TValue::Integer(
                self.global.heap.string(h).bytes.len() as i64,
            )),
            TValue::Table(h) => {
                // Check __len first: Lua 5.4 honors __len on
                // tables too — it overrides the default luaH_getn
                // result when present.
                let tm = self.global.get_metamethod(v, TagMethod::Len);
                if !matches!(tm, TValue::Nil) && Self::is_callable(tm) {
                    return self.call_unary_metamethod_to_register(tm, v);
                }
                Ok(TValue::Integer(self.global.heap.table_len(h) as i64))
            }
            TValue::UserData(h) => {
                let tm = self.global.get_metamethod(v, TagMethod::Len);
                if !matches!(tm, TValue::Nil) && Self::is_callable(tm) {
                    return self.call_unary_metamethod_to_register(tm, v);
                }
                Ok(TValue::Integer(
                    self.global.heap.userdata_get(h).data.len() as i64,
                ))
            }
            _ => {
                let tm = self.global.get_metamethod(v, TagMethod::Len);
                if matches!(tm, TValue::Nil) || !Self::is_callable(tm) {
                    return Err(self.make_lua_error(
                        "attempt to get length of a non-string/table value",
                    ));
                }
                self.call_unary_metamethod_to_register(tm, v)
            }
        }
    }

    /// Invoke a unary metamethod `tm(v)` and return its result.
    fn call_unary_metamethod_to_register(
        &mut self,
        tm: TValue,
        v: TValue,
    ) -> LuaResult<TValue> {
        let saved_top = self.current_thread().top;
        let func_slot = saved_top;
        {
            let thread = self.current_thread_mut();
            thread.grow_stack(2);
            thread.stack[func_slot as usize] = tm;
            thread.stack[(func_slot + 1) as usize] = v;
            thread.top = func_slot + 2;
        }
        self.call_value(func_slot, 1, 1)?;
        let result = self.current_thread().stack[func_slot as usize];
        self.current_thread_mut().top = saved_top;
        Ok(result)
    }

    /// Concatenate `n` values starting at `first_slot` (absolute
    /// stack index) and write the result back to `first_slot`.
    /// Mirrors `luaV_concat`'s right-to-left walk: contiguous
    /// string/number operands fuse into a single Rust buffer;
    /// when a non-coercible operand is encountered, `__concat`
    /// is consulted on it or its neighbour.
    pub(crate) fn lua_concat_mm(
        &mut self,
        first_slot: u32,
        n: u32,
    ) -> LuaResult<()> {
        if n < 2 {
            return Ok(());
        }
        // Right-to-left. We repeatedly try to absorb as many
        // adjacent string/number operands as possible into a
        // single flat buffer; when that fails we invoke
        // __concat on the boundary and retry.
        let mut remaining = n;
        while remaining >= 2 {
            // Look at the two values at the tail: the one at
            // first_slot + remaining - 2 and first_slot + remaining - 1.
            let left_idx = first_slot + remaining - 2;
            let right_idx = first_slot + remaining - 1;
            let left = self.current_thread().stack[left_idx as usize];
            let right = self.current_thread().stack[right_idx as usize];
            if !Self::is_concat_coercible(left) || !Self::is_concat_coercible(right)
            {
                // At least one end isn't string/number: consult
                // __concat. `__concat` is not one of the fast
                // events, so slow-path lookup.
                let mut tm = self.global.get_metamethod(left, TagMethod::Concat);
                if matches!(tm, TValue::Nil) {
                    tm = self.global.get_metamethod(right, TagMethod::Concat);
                }
                if matches!(tm, TValue::Nil) || !Self::is_callable(tm) {
                    return Err(self.make_lua_error(
                        "attempt to concatenate a non-string value",
                    ));
                }
                let result = self.call_binary_metamethod_to_register(tm, left, right)?;
                self.current_thread_mut().stack[left_idx as usize] = result;
                remaining -= 1;
                continue;
            }
            // Both sides are coercible — gather the maximal
            // run ending at `right_idx` and fuse. This match
            // C Lua's optimisation: chained concats like a..b..c..d
            // allocate a single intermediate.
            let mut run_start = left_idx;
            while run_start > first_slot {
                let candidate =
                    self.current_thread().stack[(run_start - 1) as usize];
                if Self::is_concat_coercible(candidate) {
                    run_start -= 1;
                } else {
                    break;
                }
            }
            let run_len = right_idx - run_start + 1;
            let mut buffer: Vec<u8> = Vec::new();
            for i in 0..run_len {
                let v = self.current_thread().stack[(run_start + i) as usize];
                self.append_concat_operand(&mut buffer, v);
            }
            let seed = self.global.hash_seed;
            let handle = self.global.new_string(&buffer, seed);
            let fused = if buffer.len() <= crate::lstring::LUAI_MAXSHORTLEN {
                TValue::ShortString(handle)
            } else {
                TValue::LongString(handle)
            };
            self.current_thread_mut().stack[run_start as usize] = fused;
            remaining -= run_len - 1;
        }
        // After the loop the result lives at `first_slot`.
        Ok(())
    }

    /// True if `v` can be consumed by the raw concat path
    /// (string or number). Other types require `__concat`.
    fn is_concat_coercible(v: TValue) -> bool {
        matches!(
            v,
            TValue::ShortString(_)
                | TValue::LongString(_)
                | TValue::Integer(_)
                | TValue::Number(_)
        )
    }

    /// Append one concat operand to the output buffer. Assumes
    /// [`Self::is_concat_coercible`] returned true.
    fn append_concat_operand(&self, buffer: &mut Vec<u8>, v: TValue) {
        match v {
            TValue::ShortString(h) | TValue::LongString(h) => {
                buffer.extend_from_slice(&self.global.heap.string(h).bytes);
            }
            TValue::Integer(n) => {
                buffer.extend_from_slice(n.to_string().as_bytes());
            }
            TValue::Number(f) => {
                buffer.extend_from_slice(format!("{f}").as_bytes());
            }
            _ => unreachable!("append_concat_operand requires coercible operand"),
        }
    }

    // ------------------------------------------------------------------
    // Table read/write with __index / __newindex metamethod dispatch.
    //
    // Matches `luaV_finishget` / `luaV_finishset` in lvm.c. Walks
    // metatable chains up to `MAX_TAG_LOOP` deep. When the hop is a
    // function we call it via `call_value` on a scratch stack region;
    // when it's a table we tail-recurse the lookup.
    // ------------------------------------------------------------------

    /// Direct raw-table lookup. Returns `TValue::Nil` if the key is
    /// absent (or present with Nil; Lua can't distinguish the two).
    pub(crate) fn raw_table_get(&self, handle: TableHandle, key: TValue) -> TValue {
        self.global
            .heap
            .table_get(handle, key)
            .unwrap_or(TValue::Nil)
    }

    /// Perform a Lua indexing operation (`R(a) := t[key]`) with
    /// full metamethod dispatch. On entry, the result slot is
    /// written to `dest_slot` (an absolute slot index on the
    /// current thread's stack). `dest_slot` MUST be distinct from
    /// whatever register holds `t` and `key` — callers use
    /// per-opcode register allocation to ensure that.
    pub(crate) fn lua_get_index(
        &mut self,
        t: TValue,
        key: TValue,
        dest_slot: u32,
    ) -> LuaResult<()> {
        let mut target = t;
        let k = key;
        for _ in 0..MAX_TAG_LOOP {
            match target {
                TValue::Table(h) => {
                    let direct = self.raw_table_get(h, k);
                    if !matches!(direct, TValue::Nil) {
                        self.current_thread_mut().stack[dest_slot as usize] =
                            direct;
                        return Ok(());
                    }
                    // Empty slot: consult __index.
                    let tm = self
                        .global
                        .get_metamethod(target, TagMethod::Index);
                    if matches!(tm, TValue::Nil) {
                        self.current_thread_mut().stack[dest_slot as usize] =
                            TValue::Nil;
                        return Ok(());
                    }
                    if Self::is_callable(tm) {
                        return self.call_index_metamethod(
                            tm, target, k, dest_slot,
                        );
                    }
                    // Tail into the metatable value.
                    target = tm;
                    continue;
                }
                _ => {
                    let tm = self
                        .global
                        .get_metamethod(target, TagMethod::Index);
                    if matches!(tm, TValue::Nil) {
                        return Err(self.make_lua_error(
                            "attempt to index a non-table value",
                        ));
                    }
                    if Self::is_callable(tm) {
                        return self.call_index_metamethod(
                            tm, target, k, dest_slot,
                        );
                    }
                    target = tm;
                    continue;
                }
            }
        }
        Err(self.make_lua_error(
            "'__index' chain too long; possible loop",
        ))
    }

    /// Perform a Lua store operation (`t[key] := value`) with
    /// full metamethod dispatch. Mirrors `lua_get_index` for the
    /// write path.
    pub(crate) fn lua_set_index(
        &mut self,
        t: TValue,
        key: TValue,
        value: TValue,
    ) -> LuaResult<()> {
        let mut target = t;
        let k = key;
        for _ in 0..MAX_TAG_LOOP {
            match target {
                TValue::Table(h) => {
                    // Raw check: if the slot already has a non-nil
                    // value, we write straight without consulting
                    // __newindex (Lua semantics).
                    let existing = self.raw_table_get(h, k);
                    if !matches!(existing, TValue::Nil) {
                        self.global.table_set(h, k, value);
                        return Ok(());
                    }
                    let tm = self
                        .global
                        .get_metamethod(target, TagMethod::NewIndex);
                    if matches!(tm, TValue::Nil) {
                        // No metamethod — raw set on the table.
                        self.global.table_set(h, k, value);
                        return Ok(());
                    }
                    if Self::is_callable(tm) {
                        return self.call_newindex_metamethod(
                            tm, target, k, value,
                        );
                    }
                    target = tm;
                    continue;
                }
                _ => {
                    let tm = self
                        .global
                        .get_metamethod(target, TagMethod::NewIndex);
                    if matches!(tm, TValue::Nil) {
                        return Err(self.make_lua_error(
                            "attempt to index a non-table value",
                        ));
                    }
                    if Self::is_callable(tm) {
                        return self.call_newindex_metamethod(
                            tm, target, k, value,
                        );
                    }
                    target = tm;
                    continue;
                }
            }
        }
        Err(self.make_lua_error(
            "'__newindex' chain too long; possible loop",
        ))
    }

    /// True for the three "function" variants Lua recognises as
    /// callable via plain call: Lua closures, C closures, and
    /// light C functions. Tables and userdata with `__call`
    /// metamethods are NOT yet handled (Stage 5.14).
    pub(crate) fn is_callable(v: TValue) -> bool {
        matches!(
            v,
            TValue::LuaClosure(_) | TValue::CClosure(_) | TValue::LightCFunction(_)
        )
    }

    /// Invoke a `__index` metamethod as `metamethod(t, key)` and
    /// move the single return value into `dest_slot`. Uses a
    /// scratch stack region above the current top so it doesn't
    /// clobber the caller's registers.
    fn call_index_metamethod(
        &mut self,
        metamethod: TValue,
        t: TValue,
        key: TValue,
        dest_slot: u32,
    ) -> LuaResult<()> {
        let saved_top = self.current_thread().top;
        let func_slot = saved_top;
        {
            let thread = self.current_thread_mut();
            thread.grow_stack(3);
            thread.stack[func_slot as usize] = metamethod;
            thread.stack[(func_slot + 1) as usize] = t;
            thread.stack[(func_slot + 2) as usize] = key;
            thread.top = func_slot + 3;
        }
        self.call_value(func_slot, 2, 1)?;
        let result = self.current_thread().stack[func_slot as usize];
        self.current_thread_mut().stack[dest_slot as usize] = result;
        // Restore the top (call_value leaves it at func_slot + 1).
        self.current_thread_mut().top = saved_top;
        Ok(())
    }

    /// Invoke a `__newindex` metamethod as
    /// `metamethod(t, key, value)` discarding return values.
    fn call_newindex_metamethod(
        &mut self,
        metamethod: TValue,
        t: TValue,
        key: TValue,
        value: TValue,
    ) -> LuaResult<()> {
        let saved_top = self.current_thread().top;
        let func_slot = saved_top;
        {
            let thread = self.current_thread_mut();
            thread.grow_stack(4);
            thread.stack[func_slot as usize] = metamethod;
            thread.stack[(func_slot + 1) as usize] = t;
            thread.stack[(func_slot + 2) as usize] = key;
            thread.stack[(func_slot + 3) as usize] = value;
            thread.top = func_slot + 4;
        }
        self.call_value(func_slot, 3, 0)?;
        self.current_thread_mut().top = saved_top;
        Ok(())
    }

    /// Called by [`LuaState::call_value`] when the callable is a
    /// Lua closure. Sets up the register window, pushes a call
    /// frame, and delegates to [`LuaState::execute`].
    ///
    /// Matches C Lua's `luaD_precall`:
    ///
    /// * `narg` = arguments actually passed
    ///   (`thread.top - func_slot - 1`).
    /// * If `narg < num_params`, missing fixed params are nil
    ///   padded by bumping `top` up.
    /// * The register window above `top` up to
    ///   `func + 1 + max_stack_size` is nil-filled in the
    ///   backing `stack` Vec so every register read sees a
    ///   defined value — but `top` stays at
    ///   `func + 1 + max(narg, num_params)` so OP_VARARGPREP
    ///   can see the extras.
    pub(crate) fn invoke_lua_closure(
        &mut self,
        func_slot: u32,
        closure: LClosureHandle,
        n_results: i16,
    ) -> LuaResult<()> {
        let proto_handle = self.global.heap.lclosure(closure).proto;
        let (max_stack, num_params) = {
            let p = self.global.heap.proto(proto_handle);
            (p.max_stack_size as u32, p.num_params as u32)
        };
        let base = func_slot + 1;
        let frame_top = base + max_stack;
        // Grow backing storage to the full register window so
        // any register read above `top` sees nil and doesn't
        // trip a bounds check.
        {
            let thread = self.current_thread_mut();
            let current_top = thread.top;
            if frame_top > thread.stack.len() as u32 {
                thread.grow_stack(frame_top - current_top);
            }
            // Nil-pad missing fixed params, bumping top.
            let args_top = current_top.max(base + num_params);
            for i in current_top..args_top {
                thread.stack[i as usize] = TValue::Nil;
            }
            // Fill the rest of the register window with nil in
            // the backing storage (without bumping top).
            for i in args_top..frame_top {
                thread.stack[i as usize] = TValue::Nil;
            }
            thread.top = args_top;
        }
        self.push_call_frame(func_slot, frame_top, n_results);
        self.execute()
    }

    /// Result-transfer + frame-pop path for every OP_RETURN
    /// variant. Moves the `n_returned` register values at
    /// `R(first_reg)..R(first_reg + n_returned)` down to
    /// `func_slot..func_slot + n_returned`, pops the frame, and
    /// nil-pads to `n_expected`. Shares the same semantics as
    /// `finish_c_call` but reads from register indices instead
    /// of the raw top of stack.
    fn finish_vm_return(
        &mut self,
        func_slot: u32,
        base: u32,
        first_reg: u32,
        n_returned: u32,
        n_expected: i16,
    ) {
        {
            let thread = self.current_thread_mut();
            let src_start = base + first_reg;
            for i in 0..n_returned {
                let value = thread.stack[(src_start + i) as usize];
                thread.stack[(func_slot + i) as usize] = value;
            }
        }
        let _ = self.pop_call_frame();

        let leave = if n_expected < 0 {
            n_returned
        } else {
            n_expected as u32
        };
        let new_top = func_slot + leave;
        {
            let thread = self.current_thread_mut();
            if (new_top as usize) > thread.stack.len() {
                thread.grow_stack(new_top.saturating_sub(thread.top));
            }
            if leave > n_returned {
                for i in n_returned..leave {
                    thread.stack[(func_slot + i) as usize] = TValue::Nil;
                }
            }
            thread.top = new_top;
        }
    }
}

// ---------------------------------------------------------------------------
// Comparison helpers. These match `luaV_equalobj` / `luaV_lessthan` /
// `luaV_lessequal` in `lvm.c` in their raw-numeric paths. Metamethod
// fallback lands with the commit that introduces __eq/__lt/__le
// dispatch; until then, non-numeric comparisons on non-matching types
// report a runtime error.
// ---------------------------------------------------------------------------

/// Lua equality fast path without metamethod fallback — matches
/// `luaV_equalobj`'s primitive checks. Handles Integer/Number
/// cross-type equality (`2 == 2.0`). Returns `None` for
/// same-type-different-identity cases that need a metamethod
/// walk, `Some(true/false)` for decidable cases.
fn lua_raw_equals(a: &TValue, b: &TValue) -> Option<bool> {
    if a == b {
        return Some(true);
    }
    match (a, b) {
        (TValue::Integer(i), TValue::Number(n))
        | (TValue::Number(n), TValue::Integer(i)) => Some(
            n.is_finite() && *n == (*i as f64) && n.floor() == *n,
        ),
        // Strings intern so identity ≡ value; not equal bit-for-bit
        // means not equal at the Lua level. `nil`/`bool` only compare
        // equal to themselves (handled by the top check). Numbers of
        // different precisions compared above.
        (TValue::ShortString(_), TValue::ShortString(_))
        | (TValue::LongString(_), TValue::LongString(_)) => Some(false),
        // Tables and userdata: decidable only when identity matches.
        // When not identical, consult __eq.
        (TValue::Table(_), TValue::Table(_)) => None,
        (TValue::UserData(_), TValue::UserData(_)) => None,
        _ => Some(false),
    }
}

/// Raw `<` check without metamethod fallback. Returns
/// `Ok(Some(bool))` for decidable cases, `Ok(None)` to signal
/// "needs __lt metamethod", `Err` never in this path.
fn lua_raw_less_than(a: &TValue, b: &TValue) -> Option<bool> {
    if let (Some(na), Some(nb)) = (to_number_ns(a), to_number_ns(b)) {
        return Some(na < nb);
    }
    None
}

/// Raw `<=` check — same shape as [`lua_raw_less_than`].
fn lua_raw_less_equal(a: &TValue, b: &TValue) -> Option<bool> {
    if let (Some(na), Some(nb)) = (to_number_ns(a), to_number_ns(b)) {
        return Some(na <= nb);
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::contract::{LClosure, LuaState, Proto, TValue, UpVal, UpValState};
    use crate::lopcodes::{create_abck, create_abx, OpCode, OFFSET_sBx};
    use crate::ltm::TagMethod;

    /// Helper that builds a simple Proto with the supplied code
    /// and `max_stack_size`, then wraps it in an LClosure with no
    /// upvalues, allocates it on the heap, and pushes it onto
    /// the current thread's stack.
    fn push_simple_closure(state: &mut LuaState, code: Vec<u32>, max_stack: u8) {
        let proto = Proto {
            max_stack_size: max_stack,
            code,
            ..Proto::default()
        };
        let proto_handle = state.global.heap.alloc_proto(proto);
        let closure = LClosure {
            proto: proto_handle,
            upvalues: vec![],
        };
        let closure_handle = state.global.heap.alloc_lclosure(closure);
        state.current_thread_mut().push(TValue::LuaClosure(closure_handle));
    }

    fn loadi(a: u32, sbx: i32) -> u32 {
        create_abx(OpCode::OP_LOADI, a, (sbx + OFFSET_sBx) as u32)
    }

    fn move_reg(a: u32, b: u32) -> u32 {
        create_abck(OpCode::OP_MOVE, a, b, 0, false)
    }

    fn return0() -> u32 {
        create_abck(OpCode::OP_RETURN0, 0, 0, 0, false)
    }

    fn return1(a: u32) -> u32 {
        create_abck(OpCode::OP_RETURN1, a, 0, 0, false)
    }

    #[test]
    fn empty_function_with_return0_runs_cleanly() {
        let mut state = LuaState::new(0);
        push_simple_closure(&mut state, vec![return0()], 1);
        state.call_value(0, 0, 0).expect("call succeeds");
        assert_eq!(state.get_top(), 0);
    }

    #[test]
    fn loadi_then_return1_returns_the_integer() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![loadi(0, 42), return1(0)],
            1,
        );
        state.call_value(0, 0, 1).expect("call succeeds");
        assert_eq!(state.get_top(), 1);
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn loadi_with_negative_sbx_writes_negative_integer() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![loadi(0, -17), return1(0)],
            1,
        );
        state.call_value(0, 0, 1).expect("call succeeds");
        assert_eq!(state.to_integer_x(1), Some(-17));
    }

    #[test]
    fn move_copies_register_then_return_reads_destination() {
        let mut state = LuaState::new(0);
        // R(0) = 7; R(1) = R(0); return R(1)
        push_simple_closure(
            &mut state,
            vec![loadi(0, 7), move_reg(1, 0), return1(1)],
            2,
        );
        state.call_value(0, 0, 1).expect("call succeeds");
        assert_eq!(state.to_integer_x(1), Some(7));
    }

    #[test]
    fn loadfalse_and_loadtrue_write_booleans() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_LOADFALSE, 0, 0, 0, false),
                return1(0),
            ],
            1,
        );
        state.call_value(0, 0, 1).expect("call succeeds");
        assert!(!state.to_boolean(1));

        let mut state2 = LuaState::new(0);
        push_simple_closure(
            &mut state2,
            vec![
                create_abck(OpCode::OP_LOADTRUE, 0, 0, 0, false),
                return1(0),
            ],
            1,
        );
        state2.call_value(0, 0, 1).expect("call succeeds");
        assert!(state2.to_boolean(1));
    }

    #[test]
    fn loadnil_writes_nil_to_range_of_registers() {
        let mut state = LuaState::new(0);
        // Seed registers 0..=2 with non-nil first, then clear
        // them all via LOADNIL(0, 2) (R(0)..R(0+2) := nil).
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 10),
                loadi(1, 20),
                loadi(2, 30),
                create_abck(OpCode::OP_LOADNIL, 0, 2, 0, false),
                return1(1),
            ],
            3,
        );
        state.call_value(0, 0, 1).expect("call succeeds");
        assert!(state.is_nil(1));
    }

    #[test]
    fn call_value_with_lua_closure_runs_the_vm_loop() {
        // Smoke test that the LuaClosure dispatch in call_value
        // (added in this commit) correctly delegates to the VM.
        let mut state = LuaState::new(0);
        push_simple_closure(&mut state, vec![loadi(0, 100), return1(0)], 1);
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(100));
    }

    // ---- Stage 5.5 arithmetic opcodes -----------------------------

    fn arith_binary(op: OpCode, a: u32, b: u32, c: u32) -> u32 {
        create_abck(op, a, b, c, false)
    }

    #[test]
    fn op_add_two_integers_produces_sum() {
        // R(0) = 10; R(1) = 32; R(2) = R(0) + R(1); return R(2)
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 10),
                loadi(1, 32),
                arith_binary(OpCode::OP_ADD, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_sub_two_integers_produces_difference() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 50),
                loadi(1, 8),
                arith_binary(OpCode::OP_SUB, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_mul_two_integers_produces_product() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 6),
                loadi(1, 7),
                arith_binary(OpCode::OP_MUL, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_div_produces_float_even_for_integer_inputs() {
        // Lua's `/` is always float division.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 10),
                loadi(1, 4),
                arith_binary(OpCode::OP_DIV, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_number_x(1), Some(2.5));
    }

    #[test]
    fn op_idiv_produces_floor_division_of_integers() {
        // 10 // 3 == 3. The integer floor-div path in raw_arith.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 10),
                loadi(1, 3),
                arith_binary(OpCode::OP_IDIV, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(3));
    }

    #[test]
    fn op_mod_produces_floor_modulo() {
        // -5 mod 3 in Lua's floor-mod semantics is 1, not -2.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, -5),
                loadi(1, 3),
                arith_binary(OpCode::OP_MOD, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn op_band_two_integers_produces_bitwise_and() {
        // 0b1100 & 0b1010 = 0b1000 = 8
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 12),
                loadi(1, 10),
                arith_binary(OpCode::OP_BAND, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(8));
    }

    #[test]
    fn op_unm_unary_minus_flips_sign() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 5),
                arith_binary(OpCode::OP_UNM, 1, 0, 0),
                return1(1),
            ],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(-5));
    }

    // ---- Stage 5.6 branches + comparison --------------------------

    /// Build a proto that carries a populated constant pool plus
    /// the supplied code.
    fn push_closure_with_constants(
        state: &mut LuaState,
        code: Vec<u32>,
        constants: Vec<TValue>,
        max_stack: u8,
    ) {
        let proto = Proto {
            max_stack_size: max_stack,
            code,
            constants,
            ..Proto::default()
        };
        let proto_handle = state.global.heap.alloc_proto(proto);
        let closure = crate::contract::LClosure {
            proto: proto_handle,
            upvalues: vec![],
        };
        let closure_handle = state.global.heap.alloc_lclosure(closure);
        state
            .current_thread_mut()
            .push(TValue::LuaClosure(closure_handle));
    }

    fn loadk(a: u32, bx: u32) -> u32 {
        crate::lopcodes::create_abx(OpCode::OP_LOADK, a, bx)
    }

    fn jmp(sj: i32) -> u32 {
        // isJ format — encode sJ as the upper 25 bits with
        // OFFSET_sJ added for the biased representation.
        use crate::lopcodes::OFFSET_sJ;
        ((sj + OFFSET_sJ) as u32) << 7 | (OpCode::OP_JMP as u32)
    }

    fn cmp(op: OpCode, a: u32, b: u32, k: bool) -> u32 {
        create_abck(op, a, b, 0, k)
    }

    fn test(a: u32, k: bool) -> u32 {
        create_abck(OpCode::OP_TEST, a, 0, 0, k)
    }

    #[test]
    fn op_loadk_loads_constant_from_proto_pool() {
        let mut state = LuaState::new(0);
        push_closure_with_constants(
            &mut state,
            vec![loadk(0, 0), return1(0)],
            vec![TValue::Integer(777)],
            1,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(777));
    }

    #[test]
    fn op_jmp_unconditionally_advances_pc() {
        // loadi 0 10; jmp +1; loadi 0 99; return1 0
        // jump skips the "loadi 99" so the result is 10.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 10),
                jmp(1),
                loadi(0, 99),
                return1(0),
            ],
            1,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(10));
    }

    #[test]
    fn op_eq_skips_next_when_inequality_mismatches_k() {
        // R(0) = 5; R(1) = 5; EQ 0 1 k=1  ; skip-next if (==) != true
        // The comparison is true so we DON'T skip; the next
        // instruction (LOADI 99 into R(2)) runs.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 5),
                loadi(1, 5),
                cmp(OpCode::OP_EQ, 0, 1, true),
                loadi(2, 99),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        // LOADI 99 ran because the compare succeeded.
        assert_eq!(state.to_integer_x(1), Some(99));
    }

    #[test]
    fn op_eq_unequal_integers_trigger_skip() {
        // R(0) = 5; R(1) = 6; EQ 0 1 k=1
        // Compare is false, and the k flag asks for "true ==
        // skip-next". 5 != 6 so comparison is false, false !=
        // true(k) → skip next. The loadi below is skipped.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 5),
                loadi(1, 6),
                cmp(OpCode::OP_EQ, 0, 1, true),
                loadi(2, 99),   // skipped
                loadi(2, 11),   // executed
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(11));
    }

    #[test]
    fn op_lt_less_than_passes_through_when_true() {
        // R(0) = 3; R(1) = 7; LT 0 1 k=1 (true => don't skip)
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 3),
                loadi(1, 7),
                cmp(OpCode::OP_LT, 0, 1, true),
                loadi(2, 42),   // executes: 3 < 7 is true
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_le_handles_equal_values() {
        // 5 <= 5 is true.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 5),
                loadi(1, 5),
                cmp(OpCode::OP_LE, 0, 1, true),
                loadi(2, 100), // executes
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(100));
    }

    #[test]
    fn op_test_on_truthy_register_with_k_true_does_not_skip() {
        // R(0) = 1 (truthy); TEST 0 k=1 — truthy matches k so
        // don't skip the next instruction.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 1),
                test(0, true),
                loadi(1, 55), // executes
                return1(1),
            ],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(55));
    }

    #[test]
    fn op_test_on_false_register_with_k_true_skips() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_LOADFALSE, 0, 0, 0, false),
                test(0, true),
                loadi(1, 77), // skipped
                loadi(1, 88), // executed
                return1(1),
            ],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(88));
    }

    // ---- Stage 5.7 table opcodes -----------------------------------

    fn extraarg() -> u32 {
        create_abck(OpCode::OP_EXTRAARG, 0, 0, 0, false)
    }

    #[test]
    fn op_newtable_creates_empty_table_in_register() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                return1(0),
            ],
            1,
        );
        state.call_value(0, 0, 1).unwrap();
        assert!(state.is_table(1));
    }

    #[test]
    fn op_seti_then_geti_round_trips_integer_key_value() {
        // t = {}; t[1] = 42; return t[1]
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                loadi(1, 42),
                // SETI R(0)[1] := R(1)   (k flag false → R operand)
                create_abck(OpCode::OP_SETI, 0, 1, 1, false),
                // GETI R(2) := R(0)[1]
                create_abck(OpCode::OP_GETI, 2, 0, 1, false),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_setfield_then_getfield_round_trips_string_key() {
        // t = {}; t["name"] = "noricum"; return t["name"]
        let mut state = LuaState::new(0);
        let name_key = TValue::ShortString(
            state.global.new_string(b"name", 0),
        );
        let name_val = TValue::ShortString(
            state.global.new_string(b"noricum", 0),
        );
        push_closure_with_constants(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                loadk(1, 1),
                // SETFIELD R(0)[K[0]] := R(1)
                create_abck(OpCode::OP_SETFIELD, 0, 0, 1, false),
                // GETFIELD R(2) := R(0)[K[0]]
                create_abck(OpCode::OP_GETFIELD, 2, 0, 0, false),
                return1(2),
            ],
            vec![name_key, name_val],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_lstring(1), Some(b"noricum".as_slice()));
    }

    #[test]
    fn op_settable_uses_register_for_key() {
        // t = {}; t[5] = 100; return t[5]
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                loadi(1, 5),     // key
                loadi(2, 100),   // value
                // SETTABLE R(0)[R(1)] := R(2)
                create_abck(OpCode::OP_SETTABLE, 0, 1, 2, false),
                // GETI R(3) := R(0)[5]
                create_abck(OpCode::OP_GETI, 3, 0, 5, false),
                return1(3),
            ],
            4,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(100));
    }

    #[test]
    fn op_getfield_on_missing_key_returns_nil() {
        let mut state = LuaState::new(0);
        let key = TValue::ShortString(state.global.new_string(b"absent", 0));
        push_closure_with_constants(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                create_abck(OpCode::OP_GETFIELD, 1, 0, 0, false),
                return1(1),
            ],
            vec![key],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert!(state.is_nil(1));
    }

    // ---- Stage 5.8 OP_CALL ----------------------------------------

    /// Helper: build a Proto from code + max_stack_size, without
    /// pushing it — the caller stores the returned ProtoHandle in
    /// an outer closure's constant pool.
    fn make_inner_proto(
        state: &mut LuaState,
        code: Vec<u32>,
        max_stack: u8,
    ) -> crate::contract::ProtoHandle {
        let proto = Proto {
            max_stack_size: max_stack,
            code,
            ..Proto::default()
        };
        state.global.heap.alloc_proto(proto)
    }

    #[test]
    fn op_call_invokes_light_c_function_from_bytecode() {
        unsafe extern "C" fn c_forty_two(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.set_top(0);
            state.push_integer(42);
            1
        }

        let mut state = LuaState::new(0);
        // R(0) = LOADK 0 (the c_forty_two light C function)
        // CALL R(0), 1, 2  (0 args, 1 result)
        // RETURN1 R(0)
        let lcf = TValue::LightCFunction(c_forty_two as crate::contract::RawCFunction);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_CALL, 0, 1, 2, false),
                return1(0),
            ],
            vec![lcf],
            1,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_call_nested_lua_closure_returns_computed_value() {
        // Outer function:
        //   const[0] = inner_proto (an LClosure returning 7)
        //   R(0) = LOADK 0  -- load inner closure TValue
        //   CALL R(0), 1, 2
        //   RETURN1 R(0)
        //
        // But we can't store an LClosure in the constant pool
        // directly — closures are Heap objects. Instead, we
        // alloc the inner LClosure up-front and plant it in
        // the outer proto's constants as a TValue::LuaClosure.
        let mut state = LuaState::new(0);
        let inner_proto = make_inner_proto(
            &mut state,
            vec![loadi(0, 7), return1(0)],
            1,
        );
        let inner_closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto: inner_proto,
            upvalues: vec![],
        });
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_CALL, 0, 1, 2, false),
                return1(0),
            ],
            vec![TValue::LuaClosure(inner_closure)],
            1,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(7));
    }

    #[test]
    fn op_call_passes_arguments_to_nested_closure() {
        // Inner function: R(0) and R(1) are args; returns R(0) + R(1).
        let mut state = LuaState::new(0);
        let inner_proto = make_inner_proto(
            &mut state,
            vec![
                arith_binary(OpCode::OP_ADD, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        let inner_closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto: inner_proto,
            upvalues: vec![],
        });
        // Outer: R(0) = inner; R(1) = 10; R(2) = 20;
        //        CALL R(0), 3, 2; RETURN1 R(0)
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadi(1, 10),
                loadi(2, 20),
                create_abck(OpCode::OP_CALL, 0, 3, 2, false),
                return1(0),
            ],
            vec![TValue::LuaClosure(inner_closure)],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(30));
    }

    // ---- Stage 5.9 numeric for loops ------------------------------

    fn forprep(a: u32, bx: u32) -> u32 {
        crate::lopcodes::create_abx(OpCode::OP_FORPREP, a, bx)
    }

    fn forloop(a: u32, bx: u32) -> u32 {
        crate::lopcodes::create_abx(OpCode::OP_FORLOOP, a, bx)
    }

    #[test]
    fn integer_for_loop_sums_one_to_ten() {
        // local sum = 0; for i = 1, 10 do sum = sum + i end; return sum
        // Register layout:
        //   R(0) = sum
        //   R(1) = loop counter (internal)
        //   R(2) = loop limit (10)
        //   R(3) = loop step (1)
        //   R(4) = loop variable i (visible to body)
        //
        // PC layout:
        //   0: LOADI R(0) 0      ; sum = 0
        //   1: LOADI R(1) 1      ; init
        //   2: LOADI R(2) 10     ; limit
        //   3: LOADI R(3) 1      ; step
        //   4: FORPREP R(1), body_len   ; jumps to PC 4+body_len+1 if loop empty
        //   5: ADD R(0), R(0), R(4)     ; sum = sum + i   (body start)
        //   6: FORLOOP R(1), body_len   ; jumps back to PC 5 (body start)
        //   7: RETURN1 R(0)
        let mut state = LuaState::new(0);
        // body_len = 1 (one instruction between FORPREP and FORLOOP).
        let body_len: u32 = 1;
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 0),
                loadi(1, 1),
                loadi(2, 10),
                loadi(3, 1),
                forprep(1, body_len),
                arith_binary(OpCode::OP_ADD, 0, 0, 4),
                forloop(1, body_len + 1),
                return1(0),
            ],
            5,
        );
        state.call_value(0, 0, 1).unwrap();
        // 1 + 2 + ... + 10 = 55
        assert_eq!(state.to_integer_x(1), Some(55));
    }

    #[test]
    fn integer_for_loop_with_empty_range_skips_body_entirely() {
        // for i = 10, 1, 1 do ... end — empty range, body never
        // runs. FORPREP jumps past the body and FORLOOP.
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 42),      // sentinel before the loop
                loadi(1, 10),      // init
                loadi(2, 1),       // limit
                loadi(3, 1),       // step (positive, so 10 > 1 = empty)
                forprep(1, 2),     // body_len = 2
                // Body that MUST not run.
                loadi(0, 999),     // this would overwrite the sentinel
                arith_binary(OpCode::OP_ADD, 0, 0, 4), // garbage
                forloop(1, 3),
                return1(0),
            ],
            5,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    // ---- Stage 5.10 K / I variants of arithmetic -----------------

    fn addi(a: u32, b: u32, sc: i32) -> u32 {
        use crate::lopcodes::OFFSET_sC;
        create_abck(OpCode::OP_ADDI, a, b, (sc + OFFSET_sC) as u32, false)
    }

    fn addk(a: u32, b: u32, c: u32) -> u32 {
        create_abck(OpCode::OP_ADDK, a, b, c, false)
    }

    #[test]
    fn op_addi_adds_small_signed_immediate_to_register() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![loadi(0, 10), addi(1, 0, 32), return1(1)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_addi_handles_negative_immediate() {
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![loadi(0, 50), addi(1, 0, -8), return1(1)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn op_addk_adds_constant_pool_value() {
        // Constant K[0] = 100; R(0) = -1; R(1) = R(0) + K[0]; return R(1)
        let mut state = LuaState::new(0);
        push_closure_with_constants(
            &mut state,
            vec![loadi(0, -1), addk(1, 0, 0), return1(1)],
            vec![TValue::Integer(100)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(99));
    }

    // ---- Stage 5.11 OP_CLOSURE + upvalue opcodes -----------------

    #[test]
    fn op_closure_creates_new_lclosure_from_inner_proto() {
        // Outer proto has an inner_proto that just returns 13.
        // Outer code: CLOSURE R(0), 0; CALL R(0), 1, 2; RETURN1 R(0)
        let mut state = LuaState::new(0);
        // Build inner proto.
        let inner_proto_handle = state.global.heap.alloc_proto(Proto {
            max_stack_size: 1,
            code: vec![loadi(0, 13), return1(0)],
            upvalues: vec![],
            ..Proto::default()
        });
        // Build outer proto with the inner proto referenced.
        let outer_code = vec![
            crate::lopcodes::create_abx(OpCode::OP_CLOSURE, 0, 0),
            create_abck(OpCode::OP_CALL, 0, 1, 2, false),
            return1(0),
        ];
        let outer_proto = Proto {
            max_stack_size: 2,
            code: outer_code,
            inner_protos: vec![inner_proto_handle],
            ..Proto::default()
        };
        let outer_proto_handle = state.global.heap.alloc_proto(outer_proto);
        let outer_closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto: outer_proto_handle,
            upvalues: vec![],
        });
        state
            .current_thread_mut()
            .push(TValue::LuaClosure(outer_closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(13));
    }

    // ---- Stage 5.12 CONCAT + LEN ----------------------------------

    #[test]
    fn op_concat_joins_strings_from_consecutive_registers() {
        // R(0) = "hello ", R(1) = "world"
        // CONCAT R(0), 2  -> R(0) = "hello world"
        let mut state = LuaState::new(0);
        let hello = TValue::ShortString(state.global.new_string(b"hello ", 0));
        let world = TValue::ShortString(state.global.new_string(b"world", 0));
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadk(1, 1),
                create_abck(OpCode::OP_CONCAT, 0, 2, 0, false),
                return1(0),
            ],
            vec![hello, world],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_lstring(1), Some(b"hello world".as_slice()));
    }

    #[test]
    fn op_concat_accepts_integer_operands_via_display() {
        // R(0) = "answer=", R(1) = 42
        let mut state = LuaState::new(0);
        let prefix = TValue::ShortString(state.global.new_string(b"answer=", 0));
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadi(1, 42),
                create_abck(OpCode::OP_CONCAT, 0, 2, 0, false),
                return1(0),
            ],
            vec![prefix],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_lstring(1), Some(b"answer=42".as_slice()));
    }

    #[test]
    fn op_len_on_string_returns_byte_length() {
        let mut state = LuaState::new(0);
        let s = TValue::ShortString(state.global.new_string(b"12345", 0));
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_LEN, 1, 0, 0, false),
                return1(1),
            ],
            vec![s],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(5));
    }

    #[test]
    fn op_len_on_table_returns_border() {
        // t = {}; t[1] = 10; t[2] = 20; t[3] = 30; return #t
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                loadi(1, 10),
                create_abck(OpCode::OP_SETI, 0, 1, 1, false),
                loadi(1, 20),
                create_abck(OpCode::OP_SETI, 0, 2, 1, false),
                loadi(1, 30),
                create_abck(OpCode::OP_SETI, 0, 3, 1, false),
                create_abck(OpCode::OP_LEN, 2, 0, 0, false),
                return1(2),
            ],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(3));
    }

    #[test]
    fn op_setupval_then_getupval_round_trips_via_closed_upvalue() {
        // Construct a function with one upvalue. SETUPVAL 0, 0
        // stores R(0) into upvalue 0. GETUPVAL 1, 0 reads it
        // back to R(1). Return R(1).
        //
        // The outer caller supplies the closure directly — we
        // cheat by building one with a single nil-closed upvalue
        // (matching what OP_CLOSURE would produce).
        let mut state = LuaState::new(0);
        let proto = Proto {
            max_stack_size: 2,
            code: vec![
                loadi(0, 77),
                create_abck(OpCode::OP_SETUPVAL, 0, 0, 0, false),
                create_abck(OpCode::OP_GETUPVAL, 1, 0, 0, false),
                return1(1),
            ],
            upvalues: vec![crate::contract::UpvalDesc {
                name: None,
                in_stack: false,
                idx: 0,
                kind: 0,
            }],
            ..Proto::default()
        };
        let proto_handle = state.global.heap.alloc_proto(proto);
        let upv = state.global.heap.alloc_upval(UpVal {
            state: UpValState::Closed(TValue::Nil),
        });
        let closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto: proto_handle,
            upvalues: vec![upv],
        });
        state
            .current_thread_mut()
            .push(TValue::LuaClosure(closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(77));
    }

    #[test]
    fn op_shli_shifts_left_by_immediate() {
        // 1 << 3 = 8
        use crate::lopcodes::OFFSET_sC;
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 1),
                create_abck(
                    OpCode::OP_SHLI,
                    1,
                    0,
                    (3 + OFFSET_sC) as u32,
                    false,
                ),
                return1(1),
            ],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(8));
    }

    #[test]
    fn integer_for_loop_with_negative_step_counts_down() {
        // for i = 5, 1, -1 do sum = sum + i end; returns 15.
        let mut state = LuaState::new(0);
        let body_len: u32 = 1;
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 0),
                loadi(1, 5),
                loadi(2, 1),
                loadi(3, -1),
                forprep(1, body_len),
                arith_binary(OpCode::OP_ADD, 0, 0, 4),
                forloop(1, body_len + 1),
                return1(0),
            ],
            5,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(15));
    }

    #[test]
    fn op_call_with_zero_results_discards_callee_returns() {
        // Call a function that returns 42, but ask for 0 results.
        // R(0) is overwritten with nothing visible at the top.
        let mut state = LuaState::new(0);
        let inner_proto = make_inner_proto(
            &mut state,
            vec![loadi(0, 42), return1(0)],
            1,
        );
        let inner_closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto: inner_proto,
            upvalues: vec![],
        });
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                // CALL R(0), 1 arg-slot (no args), 0 results
                //   B=1 (0 args), C=1 (0 results)
                create_abck(OpCode::OP_CALL, 0, 1, 1, false),
                // Now R(0) should hold nil (filled by finish_vm_return).
                // Load a sentinel and return it.
                loadi(1, 999),
                return1(1),
            ],
            vec![TValue::LuaClosure(inner_closure)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(999));
    }

    // ---- Stage 5.13 __index / __newindex metamethods --------------

    /// Build a metatable that maps `__index` to the given
    /// fallback value. Used by tests that want to observe the
    /// metamethod walk.
    fn install_index_table(
        state: &mut LuaState,
        target: crate::contract::TableHandle,
        fallback_table: crate::contract::TableHandle,
    ) {
        let mt = state.global.heap.alloc_table(crate::contract::Table::default());
        let index_name = state.global.tag_method_name(TagMethod::Index);
        state
            .global
            .table_set_shortstr(mt, index_name, TValue::Table(fallback_table));
        state.global.heap.table_mut(target).metatable = Some(mt);
    }

    #[test]
    fn gettable_walks_index_table_chain() {
        // parent = {x = 100}; child = setmetatable({}, {__index = parent});
        // assert child.x == 100.
        let mut state = LuaState::new(0);
        let parent = state.global.heap.alloc_table(crate::contract::Table::default());
        let x_name = state.global.new_string(b"x", 0);
        state.global.table_set_shortstr(parent, x_name, TValue::Integer(100));

        let child = state.global.heap.alloc_table(crate::contract::Table::default());
        install_index_table(&mut state, child, parent);

        // Proto: R(0) := child; R(1) := R(0).x; return R(1)
        let x_tv = TValue::ShortString(x_name);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_GETFIELD, 1, 0, 1, false),
                return1(1),
            ],
            vec![TValue::Table(child), x_tv],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(100));
    }

    #[test]
    fn gettable_returns_nil_when_key_and_metamethod_both_absent() {
        // empty {} with no metatable returns nil for any access.
        let mut state = LuaState::new(0);
        let x_name = state.global.new_string(b"x", 0);
        let x_tv = TValue::ShortString(x_name);
        push_closure_with_constants(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                create_abck(OpCode::OP_GETFIELD, 1, 0, 0, false),
                return1(1),
            ],
            vec![x_tv],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert!(state.is_nil(1));
    }

    #[test]
    fn gettable_calls_index_function_metamethod() {
        // __index = function(t, k) return 77 end
        unsafe extern "C" fn index_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            // Arguments are at index 1 (table) and 2 (key); we
            // ignore both and push a constant.
            state.push_integer(77);
            1
        }

        let mut state = LuaState::new(0);
        let child = state.global.heap.alloc_table(crate::contract::Table::default());
        let mt = state.global.heap.alloc_table(crate::contract::Table::default());
        let index_name = state.global.tag_method_name(TagMethod::Index);
        state.global.table_set_shortstr(
            mt,
            index_name,
            TValue::LightCFunction(index_fn as crate::contract::RawCFunction),
        );
        state.global.heap.table_mut(child).metatable = Some(mt);

        let x_name = state.global.new_string(b"x", 0);
        let x_tv = TValue::ShortString(x_name);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_GETFIELD, 1, 0, 1, false),
                return1(1),
            ],
            vec![TValue::Table(child), x_tv],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(77));
    }

    #[test]
    fn settable_with_newindex_table_redirects_write() {
        // storage = {}; target = setmetatable({}, {__newindex = storage})
        // target.key = 42; assert target.key == nil; assert storage.key == 42
        let mut state = LuaState::new(0);
        let storage = state.global.heap.alloc_table(crate::contract::Table::default());
        let target = state.global.heap.alloc_table(crate::contract::Table::default());
        let mt = state.global.heap.alloc_table(crate::contract::Table::default());
        let newindex_name = state.global.tag_method_name(TagMethod::NewIndex);
        state
            .global
            .table_set_shortstr(mt, newindex_name, TValue::Table(storage));
        state.global.heap.table_mut(target).metatable = Some(mt);

        let key_handle = state.global.new_string(b"key", 0);
        let key_tv = TValue::ShortString(key_handle);

        // Proto: R(0) := target; R(1) := 42; SETFIELD R(0)[K[1]] := R(1); RETURN0
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadi(1, 42),
                create_abck(OpCode::OP_SETFIELD, 0, 1, 1, false),
                return0(),
            ],
            vec![TValue::Table(target), key_tv],
            2,
        );
        state.call_value(0, 0, 0).unwrap();
        // Target still has no direct entry.
        assert!(
            state
                .global
                .heap
                .table_get_shortstr(target, key_handle)
                .is_none()
        );
        // Storage received the write.
        assert_eq!(
            state
                .global
                .heap
                .table_get_shortstr(storage, key_handle),
            Some(TValue::Integer(42))
        );
    }

    #[test]
    fn settable_with_newindex_function_metamethod_invokes_it() {
        // __newindex = function(t, k, v) side_effect += v end
        // Side effect is stored in a thread_local we read after the VM runs.
        use std::cell::Cell;
        thread_local! {
            static SINK: Cell<i64> = const { Cell::new(0) };
        }
        SINK.with(|c| c.set(0));

        unsafe extern "C" fn newindex_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            // arg 3 (value) is at stack slot 3 (function @1, t @2, k @3?
            // actually func is at index 0 conceptually, args at 1..).
            // Use lapi to_integer_x which is frame-base-relative.
            let v = state.to_integer_x(3).unwrap_or(0);
            SINK.with(|c| c.set(c.get() + v));
            0
        }

        let mut state = LuaState::new(0);
        let target = state.global.heap.alloc_table(crate::contract::Table::default());
        let mt = state.global.heap.alloc_table(crate::contract::Table::default());
        let newindex_name = state.global.tag_method_name(TagMethod::NewIndex);
        state.global.table_set_shortstr(
            mt,
            newindex_name,
            TValue::LightCFunction(newindex_fn as crate::contract::RawCFunction),
        );
        state.global.heap.table_mut(target).metatable = Some(mt);

        let key_handle = state.global.new_string(b"k", 0);
        let key_tv = TValue::ShortString(key_handle);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadi(1, 99),
                create_abck(OpCode::OP_SETFIELD, 0, 1, 1, false),
                return0(),
            ],
            vec![TValue::Table(target), key_tv],
            2,
        );
        state.call_value(0, 0, 0).unwrap();
        assert_eq!(SINK.with(|c| c.get()), 99);
    }

    #[test]
    fn gettable_raw_slot_shadows_metamethod() {
        // child = {x = 10}; parent = {x = 200};
        // mt.__index = parent; child's own x wins over the walk.
        let mut state = LuaState::new(0);
        let parent = state.global.heap.alloc_table(crate::contract::Table::default());
        let child = state.global.heap.alloc_table(crate::contract::Table::default());
        let x_name = state.global.new_string(b"x", 0);
        state.global.table_set_shortstr(parent, x_name, TValue::Integer(200));
        state.global.table_set_shortstr(child, x_name, TValue::Integer(10));
        install_index_table(&mut state, child, parent);

        let x_tv = TValue::ShortString(x_name);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_GETFIELD, 1, 0, 1, false),
                return1(1),
            ],
            vec![TValue::Table(child), x_tv],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(10));
    }

    // ---- Stage 5.14 arithmetic metamethods ------------------------

    /// Register `event` on `target`'s metatable. Allocates a
    /// fresh metatable if needed.
    fn install_metamethod(
        state: &mut LuaState,
        target: crate::contract::TableHandle,
        event: TagMethod,
        value: TValue,
    ) {
        let mt = match state.global.heap.table(target).metatable {
            Some(h) => h,
            None => {
                let mt = state.global.heap.alloc_table(crate::contract::Table::default());
                state.global.heap.table_mut(target).metatable = Some(mt);
                mt
            }
        };
        let name = state.global.tag_method_name(event);
        state.global.table_set_shortstr(mt, name, value);
    }

    #[test]
    fn add_metamethod_dispatches_when_operand_is_table() {
        // __add = function(a, b) return 99 end
        unsafe extern "C" fn add_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_integer(99);
            1
        }

        let mut state = LuaState::new(0);
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        install_metamethod(
            &mut state,
            t,
            TagMethod::Add,
            TValue::LightCFunction(add_fn as crate::contract::RawCFunction),
        );

        // Proto: R(0) := t; R(1) := 5; R(2) := R(0) + R(1); return R(2)
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadi(1, 5),
                arith_binary(OpCode::OP_ADD, 2, 0, 1),
                return1(2),
            ],
            vec![TValue::Table(t)],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(99));
    }

    #[test]
    fn add_metamethod_consulted_on_right_operand_when_left_absent() {
        // Left operand is 5 (no metamethod), right operand is a table
        // with __add -> 77.
        unsafe extern "C" fn add_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_integer(77);
            1
        }

        let mut state = LuaState::new(0);
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        install_metamethod(
            &mut state,
            t,
            TagMethod::Add,
            TValue::LightCFunction(add_fn as crate::contract::RawCFunction),
        );

        // Proto: R(0) := 5; R(1) := t; R(2) := R(0) + R(1); return R(2)
        push_closure_with_constants(
            &mut state,
            vec![
                loadi(0, 5),
                loadk(1, 0),
                arith_binary(OpCode::OP_ADD, 2, 0, 1),
                return1(2),
            ],
            vec![TValue::Table(t)],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(77));
    }

    #[test]
    fn unm_metamethod_dispatched_on_table_operand() {
        // __unm = function(t) return -42 end
        unsafe extern "C" fn unm_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_integer(-42);
            1
        }

        let mut state = LuaState::new(0);
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        install_metamethod(
            &mut state,
            t,
            TagMethod::Unm,
            TValue::LightCFunction(unm_fn as crate::contract::RawCFunction),
        );

        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_UNM, 1, 0, 0, false),
                return1(1),
            ],
            vec![TValue::Table(t)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(-42));
    }

    #[test]
    fn addk_variant_falls_through_to_metamethod() {
        // __add returns sentinel 123; ADDK R(2) = R(0) + K[0]
        // where K[0] is a float.
        unsafe extern "C" fn add_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_integer(123);
            1
        }
        let mut state = LuaState::new(0);
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        install_metamethod(
            &mut state,
            t,
            TagMethod::Add,
            TValue::LightCFunction(add_fn as crate::contract::RawCFunction),
        );
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                arith_binary(OpCode::OP_ADDK, 1, 0, 1),
                return1(1),
            ],
            vec![TValue::Table(t), TValue::Number(3.5)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(123));
    }

    #[test]
    fn gettable_walks_nested_index_chain() {
        // grand = {x = 7}; parent = {__index = grand}; child = {__index = parent}
        // child.x == 7
        let mut state = LuaState::new(0);
        let grand = state.global.heap.alloc_table(crate::contract::Table::default());
        let parent = state.global.heap.alloc_table(crate::contract::Table::default());
        let child = state.global.heap.alloc_table(crate::contract::Table::default());
        let x_name = state.global.new_string(b"x", 0);
        state.global.table_set_shortstr(grand, x_name, TValue::Integer(7));
        install_index_table(&mut state, parent, grand);
        install_index_table(&mut state, child, parent);

        let x_tv = TValue::ShortString(x_name);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_GETFIELD, 1, 0, 1, false),
                return1(1),
            ],
            vec![TValue::Table(child), x_tv],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(7));
    }

    // ---- Stage 5.15 comparison / concat / len metamethods ---------

    #[test]
    fn eq_metamethod_called_on_distinct_tables() {
        // Two separate tables, __eq on the left returns true
        // regardless of the right operand.
        unsafe extern "C" fn eq_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_boolean(true);
            1
        }

        let mut state = LuaState::new(0);
        let t1 = state.global.heap.alloc_table(crate::contract::Table::default());
        let t2 = state.global.heap.alloc_table(crate::contract::Table::default());
        install_metamethod(
            &mut state,
            t1,
            TagMethod::Eq,
            TValue::LightCFunction(eq_fn as crate::contract::RawCFunction),
        );
        // OP_EQ with k=1: skip next instruction if (R(A) == R(B)) != true.
        // We test by branching: EQ R(0), R(1), k=1; JMP +1 (skipped when equal); loadi(2, 1); return1(2)
        // If not equal: fall through to loadi(2, 0); return1(2)
        let sbx_forward = |n: i32| jmp(n);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadk(1, 1),
                // EQ R(0), R(1), k=1 -- if eq != true (false), skip next JMP
                create_abck(OpCode::OP_EQ, 0, 1, 0, true),
                // JMP forward past the "not equal" path
                sbx_forward(2),
                // Equal path (fall through when condition skipped? actually:
                // The logic is: if comparison result != k, pc++. So k=1 means
                // "if equal, don't skip; else skip the JMP". When equal: JMP runs → +2.
                // When not equal: JMP skipped → fall to loadi(2, 0).
                loadi(2, 0),
                return1(2),
                loadi(2, 1),
                return1(2),
            ],
            vec![TValue::Table(t1), TValue::Table(t2)],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn lt_metamethod_dispatched_when_operand_is_table() {
        // __lt returns true
        unsafe extern "C" fn lt_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_boolean(true);
            1
        }

        let mut state = LuaState::new(0);
        let t1 = state.global.heap.alloc_table(crate::contract::Table::default());
        let t2 = state.global.heap.alloc_table(crate::contract::Table::default());
        install_metamethod(
            &mut state,
            t1,
            TagMethod::Lt,
            TValue::LightCFunction(lt_fn as crate::contract::RawCFunction),
        );
        let sbx_forward = |n: i32| jmp(n);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadk(1, 1),
                // LT R(0), R(1), k=1
                create_abck(OpCode::OP_LT, 0, 1, 0, true),
                sbx_forward(2),
                loadi(2, 0),
                return1(2),
                loadi(2, 1),
                return1(2),
            ],
            vec![TValue::Table(t1), TValue::Table(t2)],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn string_lt_uses_byte_lexicographic_order() {
        // "abc" < "abd" at the Lua level.
        let mut state = LuaState::new(0);
        let s_abc = state.global.new_string(b"abc", 0);
        let s_abd = state.global.new_string(b"abd", 0);
        let sbx_forward = |n: i32| jmp(n);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadk(1, 1),
                create_abck(OpCode::OP_LT, 0, 1, 0, true),
                sbx_forward(2),
                loadi(2, 0),
                return1(2),
                loadi(2, 1),
                return1(2),
            ],
            vec![TValue::ShortString(s_abc), TValue::ShortString(s_abd)],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn concat_metamethod_dispatched_on_table_operand() {
        // __concat returns a sentinel string.
        unsafe extern "C" fn concat_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_string("meta");
            1
        }

        let mut state = LuaState::new(0);
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        install_metamethod(
            &mut state,
            t,
            TagMethod::Concat,
            TValue::LightCFunction(concat_fn as crate::contract::RawCFunction),
        );
        let hello = state.global.new_string(b"hello", 0);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadk(1, 1),
                create_abck(OpCode::OP_CONCAT, 0, 2, 0, false),
                return1(0),
            ],
            vec![TValue::ShortString(hello), TValue::Table(t)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_lstring(1), Some(b"meta".as_slice()));
    }

    #[test]
    fn len_metamethod_overrides_table_length() {
        // __len returns 999.
        unsafe extern "C" fn len_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_integer(999);
            1
        }

        let mut state = LuaState::new(0);
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        // Install 3 entries so table_len would otherwise return 3.
        state.global.table_set_int(t, 1, TValue::Integer(10));
        state.global.table_set_int(t, 2, TValue::Integer(20));
        state.global.table_set_int(t, 3, TValue::Integer(30));
        install_metamethod(
            &mut state,
            t,
            TagMethod::Len,
            TValue::LightCFunction(len_fn as crate::contract::RawCFunction),
        );

        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_LEN, 1, 0, 0, false),
                return1(1),
            ],
            vec![TValue::Table(t)],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(999));
    }

    #[test]
    fn op_lti_branches_true_for_less_than() {
        use crate::lopcodes::OFFSET_sC;
        // R(0) = 5; if R(0) < 10 then jmp forward.
        let mut state = LuaState::new(0);
        let sbx_forward = |n: i32| jmp(n);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 5),
                create_abck(OpCode::OP_LTI, 0, (10 + OFFSET_sC) as u32, 0, true),
                sbx_forward(2),
                loadi(1, 0),
                return1(1),
                loadi(1, 1),
                return1(1),
            ],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn op_gti_branches_true_for_greater_than() {
        use crate::lopcodes::OFFSET_sC;
        // R(0) = 10; if R(0) > 5 then jmp forward.
        let mut state = LuaState::new(0);
        let sbx_forward = |n: i32| jmp(n);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 10),
                create_abck(OpCode::OP_GTI, 0, (5 + OFFSET_sC) as u32, 0, true),
                sbx_forward(2),
                loadi(1, 0),
                return1(1),
                loadi(1, 1),
                return1(1),
            ],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    // ---- Stage 5.19 open upvalue capture + SETLIST -----------------

    #[test]
    fn closure_captures_open_upvalue_from_enclosing_frame() {
        // outer: local x = 10; inner = function() return x end; return inner()
        // inner should see x = 10 via an open upvalue.
        let mut state = LuaState::new(0);
        let inner_proto = Proto {
            max_stack_size: 1,
            code: vec![
                create_abck(OpCode::OP_GETUPVAL, 0, 0, 0, false),
                return1(0),
            ],
            upvalues: vec![crate::contract::UpvalDesc {
                name: None,
                in_stack: true,
                idx: 0,
                kind: 0,
            }],
            ..Proto::default()
        };
        let inner_ph = state.global.heap.alloc_proto(inner_proto);

        let outer_proto = Proto {
            max_stack_size: 3,
            code: vec![
                loadi(0, 10), // R(0) = local x = 10
                crate::lopcodes::create_abx(OpCode::OP_CLOSURE, 1, 0), // R(1) = closure(0)
                create_abck(OpCode::OP_CALL, 1, 1, 2, false),          // call R(1)(), 1 result
                return1(1), // return the result
            ],
            inner_protos: vec![inner_ph],
            ..Proto::default()
        };
        let outer_ph = state.global.heap.alloc_proto(outer_proto);
        let outer_closure = state.global.heap.alloc_lclosure(LClosure {
            proto: outer_ph,
            upvalues: vec![],
        });
        state
            .current_thread_mut()
            .push(TValue::LuaClosure(outer_closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(10));
    }

    #[test]
    fn two_closures_share_same_open_upvalue() {
        // outer: local x = 0
        //        inc = function() x = x + 1; return x end
        //        get = function() return x end
        //        inc(); return get()
        // Should return 1.
        let mut state = LuaState::new(0);
        // inc_proto: x = x + 1; return x
        let inc_proto = Proto {
            max_stack_size: 2,
            code: vec![
                create_abck(OpCode::OP_GETUPVAL, 0, 0, 0, false),
                crate::lopcodes::create_abck(
                    OpCode::OP_ADDI,
                    0,
                    0,
                    (1 + crate::lopcodes::OFFSET_sC) as u32,
                    false,
                ),
                create_abck(OpCode::OP_SETUPVAL, 0, 0, 0, false),
                create_abck(OpCode::OP_GETUPVAL, 0, 0, 0, false),
                return1(0),
            ],
            upvalues: vec![crate::contract::UpvalDesc {
                name: None,
                in_stack: true,
                idx: 0,
                kind: 0,
            }],
            ..Proto::default()
        };
        let inc_ph = state.global.heap.alloc_proto(inc_proto);

        // get_proto: return x
        let get_proto = Proto {
            max_stack_size: 1,
            code: vec![
                create_abck(OpCode::OP_GETUPVAL, 0, 0, 0, false),
                return1(0),
            ],
            upvalues: vec![crate::contract::UpvalDesc {
                name: None,
                in_stack: true,
                idx: 0,
                kind: 0,
            }],
            ..Proto::default()
        };
        let get_ph = state.global.heap.alloc_proto(get_proto);

        // outer: local x=0; inc=closure(0); get=closure(1);
        // call inc() discarding results; return get()
        // R(1)=inc, R(2)=get. But calling inc() at R(1) clobbers
        // R(2)+ as inner frame registers. Move get to R(3) so
        // it survives the call. inc needs max_stack=2 so inner
        // frame uses R(2)..R(3) — still collides. Move get to R(4).
        let outer_proto = Proto {
            max_stack_size: 6,
            code: vec![
                loadi(0, 0), // R(0) = x = 0
                crate::lopcodes::create_abx(OpCode::OP_CLOSURE, 1, 0), // R(1) = inc
                crate::lopcodes::create_abx(OpCode::OP_CLOSURE, 4, 1), // R(4) = get
                create_abck(OpCode::OP_CALL, 1, 1, 1, false),          // inc() (0 results)
                create_abck(OpCode::OP_CALL, 4, 1, 2, false),          // get() (1 result)
                return1(4),
            ],
            inner_protos: vec![inc_ph, get_ph],
            ..Proto::default()
        };
        let outer_ph = state.global.heap.alloc_proto(outer_proto);
        let outer_closure = state.global.heap.alloc_lclosure(LClosure {
            proto: outer_ph,
            upvalues: vec![],
        });
        state
            .current_thread_mut()
            .push(TValue::LuaClosure(outer_closure));
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn setlist_populates_table_array_part() {
        // t = {}; t[1], t[2], t[3] = 10, 20, 30; return #t
        let mut state = LuaState::new(0);
        push_simple_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_NEWTABLE, 0, 0, 0, false),
                extraarg(),
                loadi(1, 10),
                loadi(2, 20),
                loadi(3, 30),
                crate::lopcodes::create_vabck(OpCode::OP_SETLIST, 0, 3, 0, false),
                create_abck(OpCode::OP_LEN, 4, 0, 0, false),
                return1(4),
            ],
            5,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(3));
    }

    // ---- Stage 5.18 generic-for ------------------------------------

    #[test]
    fn generic_for_iterates_with_c_function_iterator() {
        // Simulates: for v in iter, state, 0 do sum = sum + v end
        // where iter(state, control) returns control + 1 while <= 3,
        // then nil. Expect sum = 1 + 2 + 3 = 6.
        //
        // Stack layout:
        // R(0) = iter (light C function)
        // R(1) = state (nil, unused)
        // R(2) = closing var (nil, unused by our iter)
        // R(3) = control = 0 (initial)
        // R(4) = sum accumulator
        //
        // Code:
        //   R(0) = LOADK 0  (iter)
        //   R(1) = LOADNIL 0 (state)
        //   R(3) = LOADI 0  (control = 0)
        //   R(2) = LOADNIL 0 (closing)
        //   R(4) = LOADI 0  (sum = 0)
        //   TFORPREP 0, Bx=3 (swap R2↔R3, jump fwd 3 to TFORCALL)
        //     body: R(4) = R(4) + R(3)
        //   TFORCALL 0, C=1
        //   TFORLOOP 0, Bx=2 (jump back past body)
        //   RETURN1 R(4)

        unsafe extern "C" fn iter_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            // Args: state (ignored at idx 1), control at idx 2.
            let ctrl = state.to_integer_x(2).unwrap_or(0);
            let next = ctrl + 1;
            if next > 3 {
                state.push_nil();
                return 1;
            }
            state.push_integer(next);
            1
        }

        let mut state = LuaState::new(0);
        let lcf = TValue::LightCFunction(iter_fn as crate::contract::RawCFunction);
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),                                           // R(0) = iter
                create_abck(OpCode::OP_LOADNIL, 1, 0, 0, false),      // R(1) = nil
                loadi(3, 0),                                           // R(3) = 0 (init ctrl)
                create_abck(OpCode::OP_LOADNIL, 2, 0, 0, false),      // R(2) = nil (closing)
                // R(3)..R(5) are TFORCALL scratch, so place sum
                // at R(7) to avoid clobbering.
                loadi(7, 0),                                           // R(7) = sum = 0
                // Before TFORPREP: R(2) = 0 (init ctrl), R(3) = nil
                // (closing). After swap: R(2) = nil, R(3) = 0.
                crate::lopcodes::create_abx(OpCode::OP_TFORPREP, 0, 1), // Bx=1 → TFORCALL
                // Body: sum += R(3) (the first loop result)
                arith_binary(OpCode::OP_ADD, 7, 7, 3),
                create_abck(OpCode::OP_TFORCALL, 0, 0, 1, false),
                crate::lopcodes::create_abx(OpCode::OP_TFORLOOP, 0, 3),
                return1(7),
            ],
            vec![lcf],
            10,  // need R(0)..R(5) + scratch for call
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(6));
    }

    // ---- Stage 5.17 tailcalls --------------------------------------

    #[test]
    fn tailcall_to_light_c_function_returns_its_result() {
        // inner() returns 77.
        unsafe extern "C" fn inner_fn(
            state: *mut LuaState,
        ) -> std::os::raw::c_int {
            let state = unsafe { &mut *state };
            state.push_integer(77);
            1
        }

        let mut state = LuaState::new(0);
        let lcf = TValue::LightCFunction(inner_fn as crate::contract::RawCFunction);
        // Outer: R(0) = lcf; TAILCALL R(0), B=1 (0 args); unreachable.
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_TAILCALL, 0, 1, 0, false),
                // Safety fallback (should not execute).
                return0(),
            ],
            vec![lcf],
            1,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(77));
    }

    #[test]
    fn tailcall_to_lua_closure_returns_its_result() {
        // inner returns 123.
        let mut state = LuaState::new(0);
        let inner_proto = make_inner_proto(
            &mut state,
            vec![loadi(0, 123), return1(0)],
            1,
        );
        let inner_closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto: inner_proto,
            upvalues: vec![],
        });
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                create_abck(OpCode::OP_TAILCALL, 0, 1, 0, false),
                return0(),
            ],
            vec![TValue::LuaClosure(inner_closure)],
            1,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(123));
    }

    #[test]
    fn tailcall_passes_arguments_and_returns_their_sum() {
        // inner(a, b) = a + b
        let mut state = LuaState::new(0);
        let inner_proto = make_inner_proto(
            &mut state,
            vec![
                arith_binary(OpCode::OP_ADD, 2, 0, 1),
                return1(2),
            ],
            3,
        );
        let inner_closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto: inner_proto,
            upvalues: vec![],
        });
        push_closure_with_constants(
            &mut state,
            vec![
                loadk(0, 0),
                loadi(1, 6),
                loadi(2, 36),
                create_abck(OpCode::OP_TAILCALL, 0, 3, 0, false),
                return0(),
            ],
            vec![TValue::LuaClosure(inner_closure)],
            3,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    // ---- Stage 5.16 varargs ----------------------------------------

    /// Helper to build a vararg proto. `num_params` is the
    /// number of fixed parameters.
    fn push_vararg_closure(
        state: &mut LuaState,
        code: Vec<u32>,
        num_params: u8,
        max_stack: u8,
    ) {
        let proto = Proto {
            num_params,
            is_vararg: true,
            max_stack_size: max_stack,
            code,
            ..Proto::default()
        };
        let proto_handle = state.global.heap.alloc_proto(proto);
        let closure = LClosure {
            proto: proto_handle,
            upvalues: vec![],
        };
        let closure_handle = state.global.heap.alloc_lclosure(closure);
        state
            .current_thread_mut()
            .push(TValue::LuaClosure(closure_handle));
    }

    #[test]
    fn vararg_returns_all_extras_when_requested() {
        // function f(a, ...) return ... end  -- three extras
        // Call with f(1, 2, 3, 4) → returns 2, 3, 4
        let mut state = LuaState::new(0);
        push_vararg_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_VARARGPREP, 1, 0, 0, false),
                // VARARG R(1), C=0 (all available)
                create_abck(OpCode::OP_VARARG, 1, 0, 0, false),
                // RETURN R(1), B=0 (all the way to top)
                create_abck(OpCode::OP_RETURN, 1, 0, 0, false),
            ],
            1, // num_params
            8, // max_stack
        );
        // Push arguments: 1, 2, 3, 4
        state.current_thread_mut().push(TValue::Integer(1));
        state.current_thread_mut().push(TValue::Integer(2));
        state.current_thread_mut().push(TValue::Integer(3));
        state.current_thread_mut().push(TValue::Integer(4));
        // Expect 3 results (2, 3, 4).
        state.call_value(0, 4, 3).unwrap();
        assert_eq!(state.to_integer_x(1), Some(2));
        assert_eq!(state.to_integer_x(2), Some(3));
        assert_eq!(state.to_integer_x(3), Some(4));
    }

    #[test]
    fn vararg_returns_nil_when_no_extras() {
        // function f(a, ...) return ... end  -- zero extras
        // Call with f(42). Return count = 1, extras = none → nil.
        let mut state = LuaState::new(0);
        push_vararg_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_VARARGPREP, 1, 0, 0, false),
                // VARARG R(1), C=2 (want 1 value)
                create_abck(OpCode::OP_VARARG, 1, 0, 2, false),
                // RETURN1 R(1)
                create_abck(OpCode::OP_RETURN1, 1, 0, 0, false),
            ],
            1,
            4,
        );
        state.current_thread_mut().push(TValue::Integer(42));
        state.call_value(0, 1, 1).unwrap();
        assert!(state.is_nil(1));
    }

    #[test]
    fn vararg_fixed_param_still_accessible_after_prep() {
        // function f(a, ...) return a end
        // Call with f(99, 7, 8). Fixed param is still R(0).
        let mut state = LuaState::new(0);
        push_vararg_closure(
            &mut state,
            vec![
                create_abck(OpCode::OP_VARARGPREP, 1, 0, 0, false),
                // RETURN1 R(0) — the first fixed param.
                create_abck(OpCode::OP_RETURN1, 0, 0, 0, false),
            ],
            1,
            4,
        );
        state.current_thread_mut().push(TValue::Integer(99));
        state.current_thread_mut().push(TValue::Integer(7));
        state.current_thread_mut().push(TValue::Integer(8));
        state.call_value(0, 3, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(99));
    }

    #[test]
    fn op_eqi_branches_true_on_integer_equality() {
        use crate::lopcodes::OFFSET_sC;
        // R(0) = 42; if R(0) == 42 then jmp forward.
        let mut state = LuaState::new(0);
        let sbx_forward = |n: i32| jmp(n);
        push_simple_closure(
            &mut state,
            vec![
                loadi(0, 42),
                create_abck(OpCode::OP_EQI, 0, (42 + OFFSET_sC) as u32, 0, true),
                sbx_forward(2),
                loadi(1, 0),
                return1(1),
                loadi(1, 1),
                return1(1),
            ],
            2,
        );
        state.call_value(0, 0, 1).unwrap();
        assert_eq!(state.to_integer_x(1), Some(1));
    }
}
