//! lapi — the public Lua C API (`lua_*` functions).
//!
//! Port of Lua 5.4's `lapi.c` (1479 LOC C) split across Stage 4.2
//! sub-commits. This file hosts the Rust-side implementation of
//! the `lua_*` family as methods on [`LuaState`]. The `extern "C"`
//! wrappers that re-expose these as the drop-in ABI live in a
//! separate layer that lands at the end of Stage 4; Stage 4.2 is
//! about the semantics.
//!
//! ## Stage 4.2 split
//!
//! * **4.2.1 (this commit)** — stack manipulation:
//!   [`LuaState::absindex`], [`LuaState::get_top`],
//!   [`LuaState::set_top`], [`LuaState::push_value`],
//!   [`LuaState::rotate`], [`LuaState::copy`],
//!   [`LuaState::check_stack`]. Plus the `index_to_slot`
//!   translator that every later sub-commit will reuse.
//! * 4.2.2 — type and access functions (`type_at`, `is_*`, `to_*`).
//! * 4.2.3 — push functions (`push_nil`, `push_integer`, ...).
//! * 4.2.4 — table get/set wrappers over `ltable`.
//! * 4.2.5 — GC control (`gc` subcommands) + misc (`concat`,
//!   `len`, `next`, `error`).
//!
//! ## Index semantics
//!
//! Lua indices are 1-based from a "call frame base". A positive
//! index `i` refers to the `i`-th value on the stack relative to
//! that base; a negative index `-i` refers to the `i`-th value
//! from the top (so `-1` is the topmost value). `0` is invalid.
//!
//! Stage 4.2.1 treats the entire thread stack as a single flat
//! frame (`base == 0`). When Stage 5 introduces real call frames,
//! [`LuaState::index_to_slot`] gains a base offset and the rest
//! of the file is unchanged.
//!
//! Pseudo-indices (`LUA_REGISTRYINDEX`, upvalue slots) are
//! **deferred to Stage 4.2.4**, because they need the table-get
//! wrapper machinery to be of any use.
//!
//! ## Error model
//!
//! C Lua's `api_check` macro aborts on invalid indices. We match
//! that behavior with plain `assert!` (promoted from
//! `debug_assert!` after the Stage 2/3 checkpoint lesson: the
//! assertion needs to fire in release so bad C callers don't
//! silently corrupt state). Valid but out-of-range indices panic
//! loudly; recoverable errors (not enough stack) return booleans.

#![allow(dead_code)]

use crate::contract::{LuaState, Thread};

// ---------------------------------------------------------------------------
// Private helpers — current thread access and index translation.
// ---------------------------------------------------------------------------

impl LuaState {
    /// Borrow the currently running thread. Short-circuits through
    /// `current_thread` into the heap so every lapi call has a
    /// single-line path to the stack.
    #[inline]
    fn current_thread(&self) -> &Thread {
        self.global.heap.thread(self.current_thread)
    }

    /// Mutably borrow the currently running thread. Mirror of
    /// [`LuaState::current_thread`].
    #[inline]
    fn current_thread_mut(&mut self) -> &mut Thread {
        self.global.heap.thread_mut(self.current_thread)
    }

    /// Translate a Lua-style index (1-based positive, -1-based
    /// negative) into a 0-based slot index in the current thread's
    /// stack. Panics on `0` (invalid) or out-of-range indices —
    /// matches C's `api_check` contract.
    ///
    /// Pseudo-indices (`LUA_REGISTRYINDEX`, upvalue indices)
    /// aren't recognized by this helper yet; callers that need
    /// them will dispatch before falling into here. Stage 4.2.4
    /// wires that up.
    fn index_to_slot(&self, idx: i32) -> u32 {
        let top = self.current_thread().top;
        if idx > 0 {
            let slot = (idx - 1) as u32;
            assert!(
                slot < top,
                "lapi: positive stack index {} out of range (top={})",
                idx,
                top
            );
            slot
        } else if idx < 0 {
            let magnitude = idx.unsigned_abs();
            assert!(
                magnitude <= top,
                "lapi: negative stack index {} out of range (top={})",
                idx,
                top
            );
            top - magnitude
        } else {
            panic!("lapi: stack index 0 is invalid");
        }
    }
}

// ---------------------------------------------------------------------------
// Public stack-manipulation API — the Stage 4.2.1 deliverable.
// ---------------------------------------------------------------------------

impl LuaState {
    /// Convert `idx` to an absolute (positive) index. `0` is
    /// invalid and panics; positive indices are returned as-is;
    /// negative indices are translated as `top + idx + 1`.
    /// Matches `lua_absindex` in `lapi.c`.
    pub fn absindex(&self, idx: i32) -> i32 {
        if idx > 0 {
            return idx;
        }
        assert!(idx != 0, "lapi: stack index 0 is invalid");
        let top = self.current_thread().top as i32;
        top + idx + 1
    }

    /// Return the current stack top — i.e., the number of values
    /// on the stack. Matches `lua_gettop`.
    pub fn get_top(&self) -> i32 {
        self.current_thread().top as i32
    }

    /// Set the stack top to `idx`. Positive `idx` sets the top to
    /// exactly that many values, nil-padding any newly exposed
    /// slots; negative `idx` removes `|idx| - 1` values (so `-1`
    /// is a no-op, `-2` pops one, and so on); `0` clears the
    /// stack entirely. Matches `lua_settop`.
    pub fn set_top(&mut self, idx: i32) {
        let new_top = if idx >= 0 {
            idx as u32
        } else {
            let top = self.current_thread().top as i32;
            let candidate = top + idx + 1;
            assert!(
                candidate >= 0,
                "lapi: settop({}) with top={} underflows",
                idx,
                top
            );
            candidate as u32
        };
        self.current_thread_mut().set_top(new_top);
    }

    /// Duplicate the value at `idx` onto the top of the stack.
    /// Matches `lua_pushvalue`.
    pub fn push_value(&mut self, idx: i32) {
        let slot = self.index_to_slot(idx);
        let value = self.current_thread().stack[slot as usize];
        self.current_thread_mut().push(value);
    }

    /// Copy the value at `from_idx` to `to_idx`. The destination
    /// overwrites whatever was there. Matches `lua_copy`.
    pub fn copy(&mut self, from_idx: i32, to_idx: i32) {
        let from_slot = self.index_to_slot(from_idx);
        let to_slot = self.index_to_slot(to_idx);
        if from_slot == to_slot {
            return;
        }
        let value = self.current_thread().stack[from_slot as usize];
        self.current_thread_mut().stack[to_slot as usize] = value;
    }

    /// Rotate the stack slice `[idx .. top]` by `n` positions. A
    /// positive `n` rotates towards the top (values near the top
    /// wrap around to `idx`); a negative `n` rotates towards
    /// `idx`. Matches `lua_rotate`. Implemented via the classic
    /// three-reversal trick so rotation is `O(k)` in the slice
    /// length with no extra allocation.
    pub fn rotate(&mut self, idx: i32, n: i32) {
        let lo = self.index_to_slot(idx) as usize;
        let hi = self.current_thread().top as usize;
        let slice_len = hi.saturating_sub(lo);
        if slice_len <= 1 || n == 0 {
            return;
        }
        // Normalize `n` into the range `[0, slice_len)` matching
        // the C semantics: positive `n` rotates towards `top`.
        let n_mod = n.rem_euclid(slice_len as i32) as usize;
        if n_mod == 0 {
            return;
        }
        let stack = &mut self.current_thread_mut().stack;
        let slice = &mut stack[lo..hi];
        // `slice.rotate_right(n_mod)` puts the last `n_mod`
        // elements at the front, which matches Lua's positive-n
        // semantics.
        slice.rotate_right(n_mod);
    }

    /// Ensure the stack has room for at least `extra` more values
    /// above the current top. Returns `true` on success; `false`
    /// if the grow would exceed an implementation limit (which
    /// Stage 4.2.1 doesn't enforce yet — we always succeed).
    /// Matches `lua_checkstack`.
    pub fn check_stack(&mut self, extra: u32) -> bool {
        self.current_thread_mut().grow_stack(extra);
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::contract::{LuaState, TValue};

    fn state_with_values(values: &[TValue]) -> LuaState {
        let mut state = LuaState::new(0);
        let t = state.current_thread_mut();
        for v in values {
            t.push(*v);
        }
        state
    }

    #[test]
    fn absindex_positive_passes_through() {
        let state = state_with_values(&[TValue::Integer(1), TValue::Integer(2)]);
        assert_eq!(state.absindex(1), 1);
        assert_eq!(state.absindex(2), 2);
    }

    #[test]
    fn absindex_negative_converts_to_top_relative() {
        let state = state_with_values(&[
            TValue::Integer(10),
            TValue::Integer(20),
            TValue::Integer(30),
        ]);
        // top = 3. -1 is slot 3, -3 is slot 1.
        assert_eq!(state.absindex(-1), 3);
        assert_eq!(state.absindex(-3), 1);
    }

    #[test]
    #[should_panic(expected = "stack index 0 is invalid")]
    fn absindex_rejects_zero() {
        let state = LuaState::new(0);
        let _ = state.absindex(0);
    }

    #[test]
    fn get_top_reports_pushed_count() {
        let state = state_with_values(&[
            TValue::Integer(1),
            TValue::Integer(2),
            TValue::Integer(3),
        ]);
        assert_eq!(state.get_top(), 3);
    }

    #[test]
    fn set_top_zero_clears_stack() {
        let mut state = state_with_values(&[TValue::Integer(1), TValue::Integer(2)]);
        state.set_top(0);
        assert_eq!(state.get_top(), 0);
    }

    #[test]
    fn set_top_negative_pops_relative_to_top() {
        let mut state = state_with_values(&[
            TValue::Integer(1),
            TValue::Integer(2),
            TValue::Integer(3),
            TValue::Integer(4),
        ]);
        // -1 is a no-op (top == 4, new_top = 4 + (-1) + 1 = 4).
        state.set_top(-1);
        assert_eq!(state.get_top(), 4);
        // -2 pops one.
        state.set_top(-2);
        assert_eq!(state.get_top(), 3);
    }

    #[test]
    fn set_top_positive_beyond_top_nil_pads() {
        let mut state = state_with_values(&[TValue::Integer(1)]);
        state.set_top(5);
        assert_eq!(state.get_top(), 5);
        // Slots 1..=4 (0-based 1..=4) are newly exposed and nil.
        let t = state.current_thread();
        assert!(matches!(t.stack[1], TValue::Nil));
        assert!(matches!(t.stack[4], TValue::Nil));
    }

    #[test]
    fn push_value_duplicates_stack_slot_to_top() {
        let mut state = state_with_values(&[
            TValue::Integer(10),
            TValue::Integer(20),
            TValue::Integer(30),
        ]);
        state.push_value(1); // copy slot 1 (value 10) to top
        assert_eq!(state.get_top(), 4);
        let t = state.current_thread();
        assert_eq!(t.stack[3], TValue::Integer(10));
    }

    #[test]
    fn push_value_with_negative_index_reads_from_top() {
        let mut state = state_with_values(&[
            TValue::Integer(10),
            TValue::Integer(20),
            TValue::Integer(30),
        ]);
        state.push_value(-1); // duplicate top (30)
        let t = state.current_thread();
        assert_eq!(t.stack[3], TValue::Integer(30));
    }

    #[test]
    fn copy_writes_source_over_destination() {
        let mut state = state_with_values(&[
            TValue::Integer(1),
            TValue::Integer(2),
            TValue::Integer(3),
        ]);
        state.copy(1, 3); // slot 1 (value 1) -> slot 3 (was 3)
        let t = state.current_thread();
        assert_eq!(t.stack[2], TValue::Integer(1));
        // Top unchanged.
        assert_eq!(state.get_top(), 3);
    }

    #[test]
    fn copy_self_is_noop() {
        let mut state = state_with_values(&[TValue::Integer(42)]);
        state.copy(1, 1);
        assert_eq!(state.current_thread().stack[0], TValue::Integer(42));
    }

    #[test]
    fn rotate_positive_moves_top_element_to_idx_position() {
        let mut state = state_with_values(&[
            TValue::Integer(1),
            TValue::Integer(2),
            TValue::Integer(3),
            TValue::Integer(4),
        ]);
        // Rotate slice [2..=4] (stack slots 1..4) right by 1.
        // Before: [1, 2, 3, 4]. After: [1, 4, 2, 3].
        state.rotate(2, 1);
        let t = state.current_thread();
        assert_eq!(t.stack[0], TValue::Integer(1));
        assert_eq!(t.stack[1], TValue::Integer(4));
        assert_eq!(t.stack[2], TValue::Integer(2));
        assert_eq!(t.stack[3], TValue::Integer(3));
    }

    #[test]
    fn rotate_negative_moves_idx_element_to_top() {
        let mut state = state_with_values(&[
            TValue::Integer(1),
            TValue::Integer(2),
            TValue::Integer(3),
            TValue::Integer(4),
        ]);
        // Rotate slice [2..=4] left by 1.
        // Before: [1, 2, 3, 4]. After: [1, 3, 4, 2].
        state.rotate(2, -1);
        let t = state.current_thread();
        assert_eq!(t.stack[1], TValue::Integer(3));
        assert_eq!(t.stack[2], TValue::Integer(4));
        assert_eq!(t.stack[3], TValue::Integer(2));
    }

    #[test]
    fn rotate_single_element_slice_is_noop() {
        let mut state = state_with_values(&[TValue::Integer(42)]);
        state.rotate(1, 5);
        assert_eq!(state.current_thread().stack[0], TValue::Integer(42));
    }

    #[test]
    fn check_stack_grows_storage_beyond_initial_capacity() {
        let mut state = LuaState::new(0);
        assert!(state.check_stack(200));
        let t = state.current_thread();
        assert!(
            t.stack.len() >= 200,
            "check_stack must grow the backing storage"
        );
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn index_to_slot_rejects_positive_overflow() {
        let state = state_with_values(&[TValue::Integer(1)]);
        let _ = state.index_to_slot(5);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn index_to_slot_rejects_negative_overflow() {
        let state = state_with_values(&[TValue::Integer(1)]);
        let _ = state.index_to_slot(-5);
    }
}
