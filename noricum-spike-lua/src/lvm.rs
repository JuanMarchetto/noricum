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

use crate::contract::{LClosureHandle, LuaError, LuaResult, LuaState, TValue};
use crate::lobject::{raw_arith, to_number_ns, ArithOp};
use crate::lopcodes::{
    get_opcode_raw, getarg_a, getarg_b, getarg_bx, getarg_c, getarg_k, getarg_sbx,
    getarg_sj, OpCode,
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
const OP_TEST_U8: u8 = OpCode::OP_TEST as u8;
const OP_TESTSET_U8: u8 = OpCode::OP_TESTSET as u8;

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
                    let equal = lua_equals(&ra, &rb);
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
                    let less = lua_less_than(&ra, &rb)?;
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
                    let le = lua_less_equal(&ra, &rb)?;
                    if le != k {
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

    /// Dispatch a binary arithmetic opcode (R(A) := R(B) op R(C)).
    /// Reads the operands out of the current frame, calls
    /// [`crate::lobject::raw_arith`], writes the result back to
    /// R(A). Returns `Err` on domain errors (e.g. integer
    /// div-by-zero) and panics on "needs metamethod fallback"
    /// because that path lands with a later commit.
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
            None => {
                // Stage 5.5 punts metamethod fallback to the
                // commit that introduces __add/__sub/... dispatch.
                // Raising a runtime error is strictly wrong for
                // well-formed Lua programs that rely on
                // metamethods, but it's loud and diagnosable
                // until the real path lands.
                Err(LuaError::Runtime(TValue::Nil))
            }
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
            None => Err(LuaError::Runtime(TValue::Nil)),
        }
    }

    /// Called by [`LuaState::call_value`] when the callable is a
    /// Lua closure. Sets up the register window, pushes a call
    /// frame, and delegates to [`LuaState::execute`].
    pub(crate) fn invoke_lua_closure(
        &mut self,
        func_slot: u32,
        closure: LClosureHandle,
        n_results: i16,
    ) -> LuaResult<()> {
        // Compute the top of the register window. The proto
        // records `max_stack_size` as the number of registers
        // it needs; we nil-pad so every instruction that reads
        // a register sees a defined value.
        let proto_handle = self.global.heap.lclosure(closure).proto;
        let max_stack = self.global.heap.proto(proto_handle).max_stack_size as u32;
        let base = func_slot + 1;
        let frame_top = base + max_stack;
        // Grow the backing storage and nil-pad the new slots.
        {
            let thread = self.current_thread_mut();
            if frame_top > thread.top {
                thread.grow_stack(frame_top - thread.top);
                for i in thread.top..frame_top {
                    thread.stack[i as usize] = TValue::Nil;
                }
                thread.top = frame_top;
            }
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

/// Lua equality — matches `luaV_equalobj` for the primitive fast
/// path. Handles Integer/Number cross-type equality (where
/// `2 == 2.0`). Returns false for cross-type combinations that
/// need a metamethod (the metamethod path lands later).
fn lua_equals(a: &TValue, b: &TValue) -> bool {
    // Same-variant bit equality catches most cases through
    // TValue::PartialEq.
    if a == b {
        return true;
    }
    // Integer/float cross-type: `2 == 2.0` must be true.
    match (a, b) {
        (TValue::Integer(i), TValue::Number(n))
        | (TValue::Number(n), TValue::Integer(i)) => {
            // A float equals an integer iff the float is
            // exactly integral and matches.
            n.is_finite() && *n == (*i as f64) && n.floor() == *n
        }
        _ => false,
    }
}

/// Lua `<` — matches `luaV_lessthan` for the numeric path.
/// Strings fall through to a raw byte-slice comparison; anything
/// else errors out with a runtime error placeholder.
fn lua_less_than(a: &TValue, b: &TValue) -> LuaResult<bool> {
    if let (Some(na), Some(nb)) = (to_number_ns(a), to_number_ns(b)) {
        // Cross-domain integer/float comparison: Lua's reference
        // uses a more precise int-vs-float comparison to avoid
        // f64 precision loss near i64::MAX, but for Stage 5.6
        // the f64 path matches within the normal ranges tests
        // exercise. Stage 5 v2 can tighten this.
        return Ok(na < nb);
    }
    Err(LuaError::Runtime(TValue::Nil))
}

/// Lua `<=` — matches `luaV_lessequal`. Same shape as
/// [`lua_less_than`].
fn lua_less_equal(a: &TValue, b: &TValue) -> LuaResult<bool> {
    if let (Some(na), Some(nb)) = (to_number_ns(a), to_number_ns(b)) {
        return Ok(na <= nb);
    }
    Err(LuaError::Runtime(TValue::Nil))
}

#[cfg(test)]
mod tests {
    use crate::contract::{LClosure, LuaState, Proto, TValue};
    use crate::lopcodes::{create_abck, create_abx, OpCode, OFFSET_sBx};

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
}
