//! ldo — call / return machinery, protected mode, and error
//! propagation. Ports Lua 5.4's `ldo.c` (~700 LOC C) incrementally.
//!
//! Stage 5 is the biggest block of the project because it introduces
//! the Lua execution model: call frames, the protected-call
//! mechanism, metamethod dispatch, and the bytecode interpreter in
//! `lvm`. This file hosts the pieces that don't directly touch the
//! interpreter loop — everything that sits between `lapi`'s public
//! entry points and the raw VM in `lvm`.
//!
//! ## Stage 5.1 (this commit) — call frame bookkeeping
//!
//! Provides the minimal set of [`LuaState`] methods that every
//! higher-level call path needs before Stage 5.2 can dispatch a
//! real call:
//!
//! * [`LuaState::push_call_frame`] — push a new [`CallFrame`] onto
//!   the current thread's frame stack.
//! * [`LuaState::pop_call_frame`] — pop and return the top frame.
//! * [`LuaState::current_call_frame`] — peek at the top frame.
//! * [`LuaState::frame_base_index`] — absolute stack index of the
//!   first register (which is `func + 1` in Lua's layout).
//!
//! No actual control-flow transfer yet. Commit 5.2 adds the precall
//! / postcall pair that wires a real C-function call through these
//! frames. Commit 5.3 layers `pcall` on top.
//!
//! ## Frame layout
//!
//! C Lua keeps frames on a linked list of `CallInfo` nodes
//! (`global_State::ci`). We use a simple `Vec<CallFrame>` on
//! [`crate::contract::Thread`] instead — `Thread::frames` from
//! Stage 2's contract — because our manual arena doesn't need the
//! intrusive-list trick C uses to avoid heap allocation per call.
//!
//! A frame's `func` field is the absolute index of the callable
//! on the thread's value stack. Register `R(0)` of that frame is
//! at stack slot `func + 1`; register `R(k)` is at `func + 1 + k`.
//! Results flow back into slots `func ..= func + n_results - 1`
//! when the frame pops.

#![allow(dead_code)]

use crate::contract::{CallFrame, LuaError, LuaResult, LuaState, ThreadStatus, TValue};

impl LuaState {
    /// Push a new call frame onto the current thread's frame
    /// stack. Returns the index of the pushed frame (so callers
    /// that want a stable handle can keep it around).
    ///
    /// `func` is the absolute stack slot that holds the callable.
    /// `top` is the initial register-window end — C's `ci->top`,
    /// one slot past the last usable register. `n_results` is the
    /// expected return count (the value the caller passed to
    /// `lua_call` / `lua_pcall` via `nresults`), with `-1`
    /// meaning "all returns" (`LUA_MULTRET`).
    ///
    /// `saved_pc` starts at zero and is overwritten by the VM on
    /// frame entry. `call_status` also starts at zero; flag bits
    /// are defined in Stage 5 and will be set on tail calls,
    /// pending yields, and protected-mode wrappers.
    pub fn push_call_frame(&mut self, func: u32, top: u32, n_results: i16) -> usize {
        let frame = CallFrame {
            func,
            top,
            saved_pc: 0,
            n_results,
            call_status: 0,
            varargs: Vec::new(),
        };
        let thread = self.current_thread_mut();
        thread.frames.push(frame);
        thread.frames.len() - 1
    }

    /// Pop the top call frame and return it. Panics if the frame
    /// stack is empty — Stage 5.1 treats that as a bug rather
    /// than a recoverable condition because no legitimate caller
    /// should reach this point without having pushed a frame.
    pub fn pop_call_frame(&mut self) -> CallFrame {
        self.current_thread_mut()
            .frames
            .pop()
            .expect("ldo: pop_call_frame on empty frame stack")
    }

    /// Peek at the top call frame without popping. Returns
    /// `None` when there's no active frame (i.e., the thread is
    /// at the outermost level before any call has been made).
    pub fn current_call_frame(&self) -> Option<&CallFrame> {
        self.current_thread().frames.last()
    }

    /// Absolute stack index of the first register of the top
    /// frame — i.e., `func + 1` in Lua's register numbering.
    /// Returns `None` when no frame is active.
    pub fn frame_base_index(&self) -> Option<u32> {
        self.current_call_frame().map(|ci| ci.func + 1)
    }

    /// How many frames are currently active on the current
    /// thread. Used by the VM and by pcall to mark return
    /// points for unwinding.
    pub fn call_depth(&self) -> usize {
        self.current_thread().frames.len()
    }
}

// ---------------------------------------------------------------------------
// Stage 5.2 — C function call dispatch. Wraps the precall / postcall
// pair that C Lua's `luaD_precall` + `luaD_poscall` implement for
// LUA_VLCF (light C function) and LUA_VCCL (C closure with captured
// upvalues). Lua-closure calls are deferred to Stage 5.4 / 5.5 when
// the bytecode interpreter lands.
// ---------------------------------------------------------------------------

impl LuaState {
    /// Call the value at `func_slot` with the arguments that sit
    /// immediately above it on the stack. `n_args` is the
    /// argument count; `n_results` is the expected return count
    /// (`-1` for Lua's `LUA_MULTRET`).
    ///
    /// Only light C functions and C closures are currently
    /// supported. Lua closures (`TValue::LuaClosure`) return
    /// `Err` until Stage 5.4 wires up the VM dispatcher;
    /// metamethod-based `__call` targets error similarly.
    ///
    /// On success the stack layout afterwards is:
    /// `[..., r1, r2, ..., rN]` where the first return sits at
    /// `func_slot`, the function slot is consumed, and N is
    /// either `n_results` (padded with nil or truncated as
    /// needed) or `n_returned_by_fn` when `n_results == -1`.
    pub fn call_value(
        &mut self,
        func_slot: u32,
        n_args: u32,
        n_results: i16,
    ) -> LuaResult<()> {
        let top = self.current_thread().top;
        assert_eq!(
            top,
            func_slot + 1 + n_args,
            "ldo: call_value: stack top must equal func_slot + 1 + n_args"
        );
        let func_value = self.current_thread().stack[func_slot as usize];
        match func_value {
            TValue::LightCFunction(raw_fn) => {
                self.invoke_c_function(func_slot, raw_fn, n_results)
            }
            TValue::CClosure(handle) => {
                let raw_fn = self.global.heap.cclosure(handle).f;
                self.invoke_c_function(func_slot, raw_fn, n_results)
            }
            TValue::LuaClosure(handle) => {
                self.invoke_lua_closure(func_slot, handle, n_results)
            }
            _ => self.try_call_metamethod(func_slot, n_args, n_results, func_value),
        }
    }

    /// Try `__call` metamethod for a non-callable value. If present,
    /// shift args up by one, place the original value at func_slot+1,
    /// install the metamethod at func_slot, and recurse into
    /// `call_value` with n_args + 1.
    fn try_call_metamethod(
        &mut self,
        func_slot: u32,
        n_args: u32,
        n_results: i16,
        func_value: TValue,
    ) -> LuaResult<()> {
        let mm = self.global.get_metamethod(func_value, crate::ltm::TagMethod::Call);
        if matches!(mm, TValue::Nil) {
            let msg = match func_value {
                TValue::Nil => "attempt to call a nil value",
                TValue::False | TValue::True => "attempt to call a boolean value",
                TValue::Integer(_) | TValue::Number(_) => "attempt to call a number value",
                TValue::ShortString(_) | TValue::LongString(_) => "attempt to call a string value",
                TValue::Table(_) => "attempt to call a table value",
                _ => "attempt to call a non-function value",
            };
            return Err(LuaError::Runtime(crate::lvm::make_error_string(
                &mut self.global,
                msg,
            )));
        }
        let thread = self.current_thread_mut();
        let base = func_slot as usize;
        let total = (n_args + 1) as usize;
        if thread.stack.len() < base + total + 1 {
            thread.stack.resize(base + total + 1, TValue::Nil);
        }
        for i in (0..total).rev() {
            thread.stack[base + 1 + i] = thread.stack[base + i];
        }
        thread.stack[base] = mm;
        thread.top = (base + total + 1) as u32;
        self.call_value(func_slot, n_args + 1, n_results)
    }

    /// Push a call frame, hand control to `raw_fn` via a raw
    /// state pointer, then transfer its return values back into
    /// the caller's frame. Shared path for light C functions
    /// and C closures (which differ only in where the function
    /// pointer lives).
    ///
    /// If the callee sets [`crate::contract::Thread::pending_error`]
    /// via [`LuaState::raise_error_value`], the frame is popped
    /// and the error is returned as `Err(LuaError::..)` without
    /// going through the normal result-transfer path. That's the
    /// Result-threading equivalent of C Lua's longjmp out of
    /// `lua_error`; [`LuaState::pcall`] catches it.
    fn invoke_c_function(
        &mut self,
        func_slot: u32,
        raw_fn: crate::contract::RawCFunction,
        n_results: i16,
    ) -> LuaResult<()> {
        let saved_top = self.current_thread().top;
        self.push_call_frame(func_slot, saved_top, n_results);

        // Raw-pointer bridge into the extern "C" function. The
        // function operates on the same LuaState instance we
        // borrow mutably here; when it returns, no concurrent
        // borrow outlives this call because `self` was passed
        // by &mut self. Unsafe is local to the bridge.
        let state_ptr = self as *mut LuaState;
        let n_returned = unsafe { raw_fn(state_ptr) };
        assert!(
            n_returned >= 0,
            "ldo: C function returned negative result count ({})",
            n_returned
        );
        let n_returned = n_returned as u32;

        // If the callee raised an error, abort the normal
        // result-transfer and propagate. Popping the frame
        // restores the caller's frame as "current" so pcall
        // can correctly reset the stack.
        if let Some(err) = self.current_thread_mut().pending_error.take() {
            let _ = self.pop_call_frame();
            return Err(err);
        }

        self.finish_c_call(func_slot, n_returned, n_results);
        Ok(())
    }

    // --- Stage 5.3 — protected calls ------------------------------

    /// Raise a Lua-visible error with `value` as the error
    /// object. Called from inside an extern "C" function body —
    /// the equivalent of C Lua's `lua_error(L)` where the
    /// error object has already been pushed. In our
    /// Result-threading model we stash the error on the
    /// current thread and let [`LuaState::invoke_c_function`]
    /// pick it up after the callee returns.
    ///
    /// The caller's C function should return 0 (or any value —
    /// the return count is ignored once the error flag is set)
    /// immediately after calling this.
    pub fn raise_error_value(&mut self, value: TValue) {
        self.current_thread_mut().pending_error = Some(LuaError::Runtime(value));
    }

    /// Protected variant of [`LuaState::call_value`]. Catches
    /// [`LuaError`] from the called function, pops the stack
    /// down to `func_slot`, pushes the error value, and returns
    /// the matching [`ThreadStatus`]. On success the return
    /// value is [`ThreadStatus::Ok`] and the stack holds the
    /// call's results at `func_slot..func_slot + n_results`
    /// (or as many as the callee produced when `n_results == -1`).
    ///
    /// Matches `lua_pcall` — minus message-handler support
    /// (Lua's `msgh` argument), which needs the Stage 5.4 VM
    /// to invoke a Lua function on the error object.
    pub fn pcall(
        &mut self,
        func_slot: u32,
        n_args: u32,
        n_results: i16,
    ) -> ThreadStatus {
        // Remember the frame depth so we can unwind any frames
        // pushed by the errored call tree.
        let saved_depth = self.call_depth();
        let _ = n_results;
        match self.call_value(func_slot, n_args, n_results) {
            Ok(()) => ThreadStatus::Ok,
            Err(err) => {
                let status = err.matching_status();
                let err_value = match err {
                    LuaError::Runtime(v) => v,
                    _ => TValue::Nil,
                };
                // Pop any frames that were pushed during the
                // errored call and never unwound.
                while self.call_depth() > saved_depth {
                    let _ = self.pop_call_frame();
                }
                // Reset the stack back to the call site and
                // push the error object where the caller
                // expects to find it.
                let thread = self.current_thread_mut();
                thread.top = func_slot;
                thread.pending_error = None;
                thread.push(err_value);
                status
            }
        }
    }

    /// Shift the `n_returned` values now sitting at the top of
    /// the stack down into the slots starting at `func_slot`,
    /// pop the call frame, and adjust the stack top based on
    /// `n_expected`. Nil-pads the gap when the caller expected
    /// more returns than the function produced. Matches the
    /// tail of `luaD_poscall` minus the hook machinery.
    fn finish_c_call(&mut self, func_slot: u32, n_returned: u32, n_expected: i16) {
        {
            let thread = self.current_thread_mut();
            let top = thread.top;
            debug_assert!(top >= n_returned);
            let first_ret = top - n_returned;
            // Copy returned values down to the function slot.
            // The old function-slot contents get overwritten
            // by the first return value (if any).
            for i in 0..n_returned {
                let src = (first_ret + i) as usize;
                let dst = (func_slot + i) as usize;
                thread.stack[dst] = thread.stack[src];
            }
        }

        let _ = self.pop_call_frame();

        // Decide how many results to leave on the stack. `-1`
        // keeps everything the function returned; anything
        // else pads with nil or truncates.
        let leave = if n_expected < 0 {
            n_returned
        } else {
            n_expected as u32
        };
        let new_top = func_slot + leave;

        {
            let thread = self.current_thread_mut();
            // Grow storage for the new top if we're expanding.
            if (new_top as usize) > thread.stack.len() {
                thread.grow_stack(new_top.saturating_sub(thread.top));
            }
            // Nil-pad any gap between the actual returns and
            // the requested count. This covers two cases:
            // (a) caller expected N returns, function produced
            // fewer — slots after the returns are set to nil;
            // (b) the function produced zero returns and the
            // old function slot is now inside the visible
            // region and must be cleared.
            if leave > n_returned {
                for i in n_returned..leave {
                    thread.stack[(func_slot + i) as usize] = TValue::Nil;
                }
            }
            thread.top = new_top;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::contract::{LuaError, LuaState, RawCFunction, ThreadStatus, TValue};

    #[test]
    fn fresh_state_has_no_active_call_frame() {
        let state = LuaState::new(0);
        assert!(state.current_call_frame().is_none());
        assert!(state.frame_base_index().is_none());
        assert_eq!(state.call_depth(), 0);
    }

    #[test]
    fn push_call_frame_returns_index_and_stores_fields() {
        let mut state = LuaState::new(0);
        // Fake a callable at stack slot 0.
        state.push_integer(42);
        let idx = state.push_call_frame(0, 1, 0);
        assert_eq!(idx, 0);
        let frame = state
            .current_call_frame()
            .expect("frame should be active");
        assert_eq!(frame.func, 0);
        assert_eq!(frame.top, 1);
        assert_eq!(frame.n_results, 0);
        assert_eq!(frame.saved_pc, 0);
        assert_eq!(frame.call_status, 0);
    }

    #[test]
    fn frame_base_index_points_to_first_register() {
        let mut state = LuaState::new(0);
        // Function at slot 3; register 0 should be at slot 4.
        state.push_call_frame(3, 8, -1);
        assert_eq!(state.frame_base_index(), Some(4));
    }

    #[test]
    fn nested_frames_each_report_their_own_base() {
        let mut state = LuaState::new(0);
        state.push_call_frame(0, 2, 0); // outer: base at slot 1
        state.push_call_frame(2, 4, 0); // inner: base at slot 3
        assert_eq!(state.call_depth(), 2);
        // The inner frame is the current one.
        assert_eq!(state.frame_base_index(), Some(3));
        // Pop it; outer is back.
        let popped = state.pop_call_frame();
        assert_eq!(popped.func, 2);
        assert_eq!(state.frame_base_index(), Some(1));
    }

    #[test]
    fn pop_call_frame_returns_the_top_frame_by_value() {
        let mut state = LuaState::new(0);
        state.push_call_frame(5, 10, -1);
        let f = state.pop_call_frame();
        assert_eq!(f.func, 5);
        assert_eq!(f.top, 10);
        assert_eq!(f.n_results, -1);
        assert_eq!(state.call_depth(), 0);
    }

    #[test]
    #[should_panic(expected = "pop_call_frame on empty")]
    fn pop_call_frame_panics_on_empty_stack() {
        let mut state = LuaState::new(0);
        let _ = state.pop_call_frame();
    }

    // ---- Stage 5.2 C function call dispatch -----------------------

    /// Test helper — a C function that returns zero results and
    /// leaves the stack empty below `func_slot`.
    unsafe extern "C" fn c_return_zero(
        _state: *mut LuaState,
    ) -> std::os::raw::c_int {
        0
    }

    /// Test helper — reads the first two arguments as integers,
    /// pushes their sum, returns 1.
    unsafe extern "C" fn c_sum_two(
        state: *mut LuaState,
    ) -> std::os::raw::c_int {
        let state = unsafe { &mut *state };
        let a = state.to_integer_x(1).unwrap_or(0);
        let b = state.to_integer_x(2).unwrap_or(0);
        state.set_top(0);
        state.push_integer(a + b);
        1
    }

    /// Test helper — pushes three integers and returns 3.
    unsafe extern "C" fn c_return_three(
        state: *mut LuaState,
    ) -> std::os::raw::c_int {
        let state = unsafe { &mut *state };
        state.set_top(0);
        state.push_integer(1);
        state.push_integer(2);
        state.push_integer(3);
        3
    }

    #[test]
    fn call_value_invokes_light_c_function_with_zero_returns() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_return_zero as RawCFunction);
        // func_slot = 0, n_args = 0, n_results = 0
        state.call_value(0, 0, 0).expect("call succeeds");
        assert_eq!(state.get_top(), 0, "stack should be empty");
    }

    #[test]
    fn call_value_with_two_args_and_one_result_reads_args_and_returns_sum() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_sum_two as RawCFunction);
        state.push_integer(10);
        state.push_integer(32);
        // Before: [func, 10, 32]. func_slot=0, n_args=2, n_results=1.
        state.call_value(0, 2, 1).expect("call succeeds");
        assert_eq!(state.get_top(), 1, "one result left");
        assert_eq!(state.to_integer_x(1), Some(42));
    }

    #[test]
    fn call_value_with_multret_keeps_all_returns() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_return_three as RawCFunction);
        state.call_value(0, 0, -1).expect("call succeeds");
        assert_eq!(state.get_top(), 3);
        assert_eq!(state.to_integer_x(1), Some(1));
        assert_eq!(state.to_integer_x(2), Some(2));
        assert_eq!(state.to_integer_x(3), Some(3));
    }

    #[test]
    fn call_value_with_fewer_expected_returns_truncates() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_return_three as RawCFunction);
        state.call_value(0, 0, 1).expect("call succeeds");
        // Only one result should survive.
        assert_eq!(state.get_top(), 1);
        assert_eq!(state.to_integer_x(1), Some(1));
    }

    #[test]
    fn call_value_with_more_expected_returns_pads_with_nil() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_return_zero as RawCFunction);
        state.call_value(0, 0, 3).expect("call succeeds");
        assert_eq!(state.get_top(), 3);
        assert!(state.is_nil(1));
        assert!(state.is_nil(2));
        assert!(state.is_nil(3));
    }

    #[test]
    fn call_value_on_nil_target_returns_runtime_error() {
        let mut state = LuaState::new(0);
        state.push_nil();
        let result = state.call_value(0, 0, 0);
        assert!(matches!(result, Err(LuaError::Runtime(_))));
    }

    #[test]
    fn call_value_pops_frame_after_c_function_returns() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_return_zero as RawCFunction);
        let depth_before = state.call_depth();
        state.call_value(0, 0, 0).unwrap();
        assert_eq!(state.call_depth(), depth_before);
    }

    // ---- Stage 5.3 protected calls --------------------------------

    /// Test helper — raises an error via the pending_error side
    /// channel and returns 0. Simulates a C function calling
    /// the future lua_error.
    unsafe extern "C" fn c_raise_runtime_error(
        state: *mut LuaState,
    ) -> std::os::raw::c_int {
        let state = unsafe { &mut *state };
        state.raise_error_value(TValue::Integer(999));
        0
    }

    /// Test helper — adds two integers, same as c_sum_two.
    /// Duplicated for the pcall test so the name makes sense
    /// in that context.
    unsafe extern "C" fn c_pcall_sum(
        state: *mut LuaState,
    ) -> std::os::raw::c_int {
        let state = unsafe { &mut *state };
        let a = state.to_integer_x(1).unwrap_or(0);
        let b = state.to_integer_x(2).unwrap_or(0);
        state.set_top(0);
        state.push_integer(a + b);
        1
    }

    #[test]
    fn pcall_returns_ok_when_c_function_succeeds() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_pcall_sum as RawCFunction);
        state.push_integer(3);
        state.push_integer(4);
        let status = state.pcall(0, 2, 1);
        assert_eq!(status, ThreadStatus::Ok);
        assert_eq!(state.to_integer_x(1), Some(7));
        assert_eq!(state.get_top(), 1);
    }

    #[test]
    fn pcall_catches_pending_error_and_returns_runtime_error_status() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_raise_runtime_error as RawCFunction);
        let status = state.pcall(0, 0, 0);
        assert_eq!(status, ThreadStatus::RuntimeError);
    }

    #[test]
    fn pcall_leaves_error_value_at_func_slot() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_raise_runtime_error as RawCFunction);
        state.pcall(0, 0, 0);
        // After pcall, the stack has the error value at slot 1
        // (where the function used to be).
        assert_eq!(state.get_top(), 1);
        assert_eq!(state.to_integer_x(1), Some(999));
    }

    #[test]
    fn pcall_clears_pending_error_after_catching() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_raise_runtime_error as RawCFunction);
        state.pcall(0, 0, 0);
        // The pending_error field must be cleared so a
        // subsequent pcall doesn't see stale state.
        assert!(state.current_thread().pending_error.is_none());
    }

    #[test]
    fn pcall_on_non_callable_returns_runtime_error_status() {
        let mut state = LuaState::new(0);
        state.push_nil();
        let status = state.pcall(0, 0, 0);
        assert_eq!(status, ThreadStatus::RuntimeError);
    }

    #[test]
    fn raise_error_value_sets_pending_error() {
        let mut state = LuaState::new(0);
        state.raise_error_value(TValue::Integer(42));
        assert!(matches!(
            state.current_thread().pending_error,
            Some(LuaError::Runtime(TValue::Integer(42)))
        ));
    }

    #[test]
    fn pcall_pops_frame_even_on_error_path() {
        let mut state = LuaState::new(0);
        state.push_light_cfunction(c_raise_runtime_error as RawCFunction);
        let depth_before = state.call_depth();
        state.pcall(0, 0, 0);
        assert_eq!(state.call_depth(), depth_before);
    }
}
