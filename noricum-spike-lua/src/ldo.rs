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

use crate::contract::{CallFrame, LuaState};

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

#[cfg(test)]
mod tests {
    use crate::contract::LuaState;

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
}
