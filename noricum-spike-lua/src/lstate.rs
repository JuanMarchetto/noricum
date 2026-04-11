//! State bootstrap — the Stage 2 end-cap.
//!
//! Stage 2 / commit 7 (final Stage 2 commit). Assembles the first
//! executable [`LuaState`] with an initialized main thread, an empty
//! registry table, and all 25 interned metamethod event names. This
//! is the piece that lets Stage 4/5 callers reach a working "there is
//! a VM here" starting point without pulling GC, panic handlers, or
//! the full registry contents into Stage 2.
//!
//! The C reference is `lua_newstate` in `lstate.c` (lines 341-393).
//! Most of what that function does is GC initialization (`gcstate`,
//! `gray` lists, `GCdebt`, `setgcparam` knobs, memory-error message)
//! and user-allocator plumbing (`frealloc`, `ud`, `warnf`). None of
//! those apply until Stage 3, so they are deliberately elided here.
//!
//! What [`LuaState::new`] does:
//!
//! 1. Create a default [`GlobalState`].
//! 2. Record the caller-chosen hash seed.
//! 3. Allocate the main [`Thread`] with an initial stack.
//! 4. Allocate the empty registry [`Table`].
//! 5. Intern the 25 metamethod event names via
//!    [`GlobalState::init_metamethod_names`].
//!
//! Thread operations (`push`, `pop`, `top_value`, `grow_stack`) live on
//! [`Thread`] directly since they don't need the full `LuaState`.

#![allow(dead_code)]

use crate::contract::{
    GlobalState, LuaState, Table, TValue, Thread, ThreadStatus, ThreadStatusInner,
};

/// Initial allocation for a fresh thread's value stack. Matches
/// `LUA_MINSTACK` in `lua.h` which sets the minimum number of free
/// slots the VM guarantees to C functions. Larger workloads grow the
/// stack on demand via [`Thread::grow_stack`].
pub const INITIAL_STACK_SIZE: usize = 20;

impl LuaState {
    /// Bootstrap a fresh [`LuaState`] with an initialized main
    /// thread, an empty registry table, and all metamethod event
    /// names interned. The `hash_seed` is stored on the global
    /// state and used by every subsequent short-string intern.
    ///
    /// Matches the state-skeleton portion of `lua_newstate` in
    /// `lstate.c`. Deliberately omitted:
    ///
    /// * User allocator (`frealloc` / `ud`) — handled by Stage 4's
    ///   drop-in ABI layer via `lua_setallocf` / `lua_getallocf`.
    /// * GC state (`gcstate`, `gray`, `grayagain`, `weak`, ...) —
    ///   Stage 3.
    /// * Memory-error message (`memerrmsg`) — Stage 3.
    /// * Per-type metatable slots (`g->mt[LUA_NUMTYPES]`) — Stage 4.
    /// * Panic handler (`panic`) — Stage 5 when we wire up the C API.
    pub fn new(hash_seed: u32) -> Self {
        let mut global = GlobalState {
            hash_seed,
            ..GlobalState::default()
        };

        let main_handle = global.heap.alloc_thread(Thread {
            status: ThreadStatusInner(ThreadStatus::Ok),
            stack: vec![TValue::Nil; INITIAL_STACK_SIZE],
            top: 0,
            frames: Vec::new(),
            open_upvals: Vec::new(),
        });
        global.main_thread = Some(main_handle);

        let registry = global.heap.alloc_table(Table::default());
        global.registry = Some(registry);

        global.init_metamethod_names();

        LuaState {
            global,
            current_thread: main_handle,
        }
    }
}

impl Thread {
    /// Push a value onto the thread's stack. Grows the backing
    /// storage if necessary. Matches the effect of `lua_push*` at
    /// the drop-in ABI layer, minus the `lua_checkstack` gate that
    /// the C API performs as a pre-condition.
    pub fn push(&mut self, value: TValue) {
        self.grow_stack(1);
        self.stack[self.top as usize] = value;
        self.top += 1;
    }

    /// Pop and return the topmost value, or `None` if the stack is
    /// empty. The freed slot is cleared to `TValue::Nil` so later
    /// traversals don't observe stale references.
    pub fn pop(&mut self) -> Option<TValue> {
        if self.top == 0 {
            return None;
        }
        self.top -= 1;
        let idx = self.top as usize;
        let value = self.stack[idx];
        self.stack[idx] = TValue::Nil;
        Some(value)
    }

    /// Peek at the topmost value without removing it.
    pub fn top_value(&self) -> Option<TValue> {
        if self.top == 0 {
            None
        } else {
            Some(self.stack[(self.top - 1) as usize])
        }
    }

    /// Access a stack slot by absolute index. `None` if out of range.
    pub fn stack_slot(&self, index: u32) -> Option<TValue> {
        self.stack.get(index as usize).copied()
    }

    /// Ensure the stack can hold at least `needed` more values above
    /// the current top. Grows by doubling (rounded to a power of two)
    /// to keep reallocation amortized. Matches the effect — but not
    /// the exact sizing policy — of `luaD_growstack` in `ldo.c`.
    pub fn grow_stack(&mut self, needed: u32) {
        let required = (self.top as usize).saturating_add(needed as usize);
        if required <= self.stack.len() {
            return;
        }
        let new_size = required.next_power_of_two().max(INITIAL_STACK_SIZE);
        self.stack.resize(new_size, TValue::Nil);
    }

    /// Total number of allocated stack slots.
    pub fn stack_size(&self) -> usize {
        self.stack.len()
    }

    /// Set the logical top index explicitly, nil-padding any slots
    /// below the new top that weren't yet populated. Used by the
    /// C API `lua_settop` at the drop-in boundary.
    pub fn set_top(&mut self, new_top: u32) {
        self.grow_stack(new_top.saturating_sub(self.top));
        // Clear any slots being abandoned.
        if new_top < self.top {
            for slot in &mut self.stack[new_top as usize..self.top as usize] {
                *slot = TValue::Nil;
            }
        }
        self.top = new_top;
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ltm::{TagMethod, EVENT_NAMES, TM_N};

    fn fresh_state() -> LuaState {
        LuaState::new(0)
    }

    #[test]
    fn bootstrap_populates_main_thread_registry_and_tm_names() {
        let state = fresh_state();
        let g = &state.global;

        // Main thread exists and current_thread matches it.
        let main = g.main_thread.expect("main thread should be set");
        assert_eq!(state.current_thread, main);
        let t = g.heap.thread(main);
        assert_eq!(t.top, 0);
        assert_eq!(t.stack.len(), INITIAL_STACK_SIZE);

        // Registry exists.
        let reg = g.registry.expect("registry should be set");
        let _ = g.heap.table(reg); // smoke check

        // Metamethod names are all interned.
        assert_eq!(g.tm_names.len(), TM_N);
        for (i, &handle) in g.tm_names.iter().enumerate() {
            assert_eq!(g.heap.string(handle).bytes, EVENT_NAMES[i]);
        }
    }

    #[test]
    fn bootstrap_records_hash_seed() {
        let state = LuaState::new(0xDEAD_BEEF);
        assert_eq!(state.global.hash_seed, 0xDEAD_BEEF);
    }

    #[test]
    fn bootstrap_allows_tag_method_name_lookup() {
        let state = fresh_state();
        let h = state.global.tag_method_name(TagMethod::Index);
        assert_eq!(state.global.heap.string(h).bytes, b"__index");
    }

    #[test]
    fn thread_push_and_pop_roundtrip() {
        let mut state = fresh_state();
        let main = state.current_thread;
        let t = state.global.heap.thread_mut(main);
        t.push(TValue::Integer(1));
        t.push(TValue::Integer(2));
        t.push(TValue::Integer(3));
        assert_eq!(t.top, 3);
        assert!(matches!(t.top_value(), Some(TValue::Integer(3))));
        assert!(matches!(t.pop(), Some(TValue::Integer(3))));
        assert!(matches!(t.pop(), Some(TValue::Integer(2))));
        assert!(matches!(t.pop(), Some(TValue::Integer(1))));
        assert!(t.pop().is_none());
        assert_eq!(t.top, 0);
    }

    #[test]
    fn thread_pop_clears_slot_to_nil() {
        let mut state = fresh_state();
        let main = state.current_thread;
        let t = state.global.heap.thread_mut(main);
        t.push(TValue::Integer(42));
        let _ = t.pop();
        // The slot should be cleared so no stale value is visible.
        assert!(matches!(t.stack[0], TValue::Nil));
    }

    #[test]
    fn grow_stack_doubles_and_rounds_up() {
        let mut state = fresh_state();
        let main = state.current_thread;
        let t = state.global.heap.thread_mut(main);
        assert_eq!(t.stack.len(), INITIAL_STACK_SIZE);
        // Ask for more than the initial capacity; must at least
        // reach the requested size and be a power of two.
        t.grow_stack(100);
        assert!(t.stack.len() >= 100);
        assert!(t.stack.len().is_power_of_two());
    }

    #[test]
    fn grow_stack_is_noop_when_room_exists() {
        let mut state = fresh_state();
        let main = state.current_thread;
        let t = state.global.heap.thread_mut(main);
        let before = t.stack.len();
        t.grow_stack(5);
        assert_eq!(t.stack.len(), before);
    }

    #[test]
    fn set_top_clears_abandoned_slots() {
        let mut state = fresh_state();
        let main = state.current_thread;
        let t = state.global.heap.thread_mut(main);
        for i in 0..5 {
            t.push(TValue::Integer(i));
        }
        assert_eq!(t.top, 5);

        t.set_top(2);
        assert_eq!(t.top, 2);
        // Slots 2..5 should be nil.
        for slot in &t.stack[2..5] {
            assert!(matches!(slot, TValue::Nil));
        }
        // Slots 0..2 keep their values.
        assert!(matches!(t.stack[0], TValue::Integer(0)));
        assert!(matches!(t.stack[1], TValue::Integer(1)));
    }

    #[test]
    fn set_top_above_current_grows_and_nil_pads() {
        let mut state = fresh_state();
        let main = state.current_thread;
        let t = state.global.heap.thread_mut(main);
        t.set_top(10);
        assert_eq!(t.top, 10);
        for slot in &t.stack[..10] {
            assert!(matches!(slot, TValue::Nil));
        }
    }
}
