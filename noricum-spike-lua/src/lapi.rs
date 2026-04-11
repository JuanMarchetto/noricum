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

use crate::contract::{
    LuaInteger, LuaNumber, LuaState, LUA_TBOOLEAN, LUA_TFUNCTION, LUA_TLIGHTUSERDATA, LUA_TNIL,
    LUA_TNUMBER, LUA_TSTRING, LUA_TTABLE, LUA_TTHREAD, LUA_TUSERDATA, TValue, Thread,
};
use crate::lobject::{to_integer_ns, to_number_ns};

// ---------------------------------------------------------------------------
// Public constants matching `lua.h`. `LUA_TNONE` is exposed here because
// it's the "out of range" sentinel returned by [`LuaState::type_at`] and
// has no slot in the contract's type-tag table.
// ---------------------------------------------------------------------------

/// Returned by [`LuaState::type_at`] for indices that don't refer to any
/// valid stack value. Matches the `LUA_TNONE` constant in `lua.h`.
pub const LUA_TNONE: i32 = -1;

// ---------------------------------------------------------------------------
// Private helpers — current thread access and index translation.
// ---------------------------------------------------------------------------

impl LuaState {
    /// Borrow the currently running thread. Short-circuits through
    /// `current_thread` into the heap so every lapi call has a
    /// single-line path to the stack. `pub(crate)` so sibling
    /// modules like `ldo` can reuse the single-line path.
    #[inline]
    pub(crate) fn current_thread(&self) -> &Thread {
        self.global.heap.thread(self.current_thread)
    }

    /// Mutably borrow the currently running thread. Mirror of
    /// [`LuaState::current_thread`]. `pub(crate)` so `lauxlib`
    /// can reuse the single-line path.
    #[inline]
    pub(crate) fn current_thread_mut(&mut self) -> &mut Thread {
        self.global.heap.thread_mut(self.current_thread)
    }

    /// Base (absolute stack slot of register R(0)) of the
    /// current frame. Zero when no frame is active — matches
    /// C Lua's treatment of the outermost level where "base =
    /// L->stack_base = 0".
    #[inline]
    fn frame_base(&self) -> u32 {
        self.frame_base_index().unwrap_or(0)
    }

    /// Like [`LuaState::index_to_slot`] but returns `None` on
    /// out-of-range indices instead of panicking. Used by the
    /// non-fatal queries in 4.2.2 (`type_at`, `is_*`, `to_*`) so
    /// callers can safely probe indices without pre-bound-checking.
    ///
    /// Indices are frame-base-relative: `1` means `base[0]`, the
    /// first argument or local of the current call frame. When
    /// no frame is active, base == 0 and the behavior collapses
    /// to the absolute-index version.
    fn try_index_to_slot(&self, idx: i32) -> Option<u32> {
        let base = self.frame_base();
        let top = self.current_thread().top;
        if idx > 0 {
            let slot = base.checked_add((idx - 1) as u32)?;
            if slot < top { Some(slot) } else { None }
        } else if idx < 0 {
            let magnitude = idx.unsigned_abs();
            if magnitude == 0 {
                return None;
            }
            // Negative indices count back from top, bounded by base.
            let frame_size = top - base;
            if magnitude <= frame_size {
                Some(top - magnitude)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Read the value at `idx` without panicking. Returns `None`
    /// when the index is out of range or zero.
    #[inline]
    fn value_at(&self, idx: i32) -> Option<TValue> {
        let slot = self.try_index_to_slot(idx)?;
        Some(self.current_thread().stack[slot as usize])
    }

    /// Translate a Lua-style index (1-based positive, -1-based
    /// negative) into a 0-based slot index in the current thread's
    /// stack. Panics on `0` (invalid) or out-of-range indices —
    /// matches C's `api_check` contract.
    ///
    /// Frame-base-relative semantics: index `1` always means
    /// register `R(0)` of the current call frame, which lives at
    /// absolute stack slot `frame.func + 1`. With no active
    /// frame, base is zero and the translation collapses to the
    /// flat form the outermost caller sees.
    ///
    /// Pseudo-indices (`LUA_REGISTRYINDEX`, upvalue indices)
    /// aren't recognized by this helper yet; callers that need
    /// them will dispatch before falling into here.
    fn index_to_slot(&self, idx: i32) -> u32 {
        let base = self.frame_base();
        let top = self.current_thread().top;
        if idx > 0 {
            let slot = base + (idx - 1) as u32;
            assert!(
                slot < top,
                "lapi: positive stack index {} out of range (top={}, base={})",
                idx,
                top,
                base
            );
            slot
        } else if idx < 0 {
            let magnitude = idx.unsigned_abs();
            let frame_size = top - base;
            assert!(
                magnitude <= frame_size,
                "lapi: negative stack index {} out of range (top={}, base={})",
                idx,
                top,
                base
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
    /// Convert `idx` to an absolute (positive, frame-relative)
    /// index. `0` is invalid and panics; positive indices pass
    /// through; negative indices are translated as
    /// `get_top() + idx + 1`. Matches `lua_absindex`.
    pub fn absindex(&self, idx: i32) -> i32 {
        if idx > 0 {
            return idx;
        }
        assert!(idx != 0, "lapi: stack index 0 is invalid");
        self.get_top() + idx + 1
    }

    /// Return the current frame's top — the number of values
    /// visible above the current frame base. Matches `lua_gettop`,
    /// which is frame-relative.
    pub fn get_top(&self) -> i32 {
        let base = self.frame_base();
        (self.current_thread().top - base) as i32
    }

    /// Set the frame top to `idx`. Positive `idx` sets the top to
    /// exactly `base + idx` (nil-padding newly exposed slots);
    /// negative `idx` pops `|idx| - 1` values (so `-1` is a
    /// no-op, `-2` pops one, …); `0` clears the frame entirely
    /// down to the base. Matches `lua_settop`.
    pub fn set_top(&mut self, idx: i32) {
        let base = self.frame_base();
        let new_top = if idx >= 0 {
            base + idx as u32
        } else {
            let top = self.current_thread().top as i32;
            let candidate = top + idx + 1;
            assert!(
                (candidate as u32) >= base,
                "lapi: settop({}) with top={}, base={} underflows the frame",
                idx,
                top,
                base
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
// Stage 4.2.2 — type and access functions. These are the read side of
// the Lua API: every `lua_is*` / `lua_to*` / `lua_type*` entry point.
// None of them mutate the stack; they're pure observations used by C
// code to inspect values the interpreter pushed.
// ---------------------------------------------------------------------------

impl LuaState {
    /// Integer type tag at `idx`, or [`LUA_TNONE`] for out-of-range
    /// indices. Matches `lua_type`. Uses the `LUA_T*` base constants
    /// from `lua.h`, not the subtype-aware `LUA_V*` variant tags on
    /// [`TValue::variant_tag`].
    pub fn type_at(&self, idx: i32) -> i32 {
        let Some(v) = self.value_at(idx) else {
            return LUA_TNONE;
        };
        match v {
            TValue::Nil => LUA_TNIL as i32,
            TValue::False | TValue::True => LUA_TBOOLEAN as i32,
            TValue::LightUserData(_) => LUA_TLIGHTUSERDATA as i32,
            TValue::Integer(_) | TValue::Number(_) => LUA_TNUMBER as i32,
            TValue::ShortString(_) | TValue::LongString(_) => LUA_TSTRING as i32,
            TValue::Table(_) => LUA_TTABLE as i32,
            TValue::LuaClosure(_) | TValue::LightCFunction(_) | TValue::CClosure(_) => {
                LUA_TFUNCTION as i32
            }
            TValue::UserData(_) => LUA_TUSERDATA as i32,
            TValue::Thread(_) => LUA_TTHREAD as i32,
        }
    }

    /// Lua-visible name for a type tag. Matches `lua_typename`.
    /// Returns `"no value"` for `LUA_TNONE` and the static
    /// [`TValue::type_name`] string for everything else.
    pub const fn type_name(tt: i32) -> &'static str {
        match tt {
            LUA_TNONE => "no value",
            x if x == LUA_TNIL as i32 => "nil",
            x if x == LUA_TBOOLEAN as i32 => "boolean",
            x if x == LUA_TLIGHTUSERDATA as i32 => "userdata",
            x if x == LUA_TNUMBER as i32 => "number",
            x if x == LUA_TSTRING as i32 => "string",
            x if x == LUA_TTABLE as i32 => "table",
            x if x == LUA_TFUNCTION as i32 => "function",
            x if x == LUA_TUSERDATA as i32 => "userdata",
            x if x == LUA_TTHREAD as i32 => "thread",
            _ => "unknown",
        }
    }

    /// Matches `lua_isnil`.
    pub fn is_nil(&self, idx: i32) -> bool {
        matches!(self.value_at(idx), Some(TValue::Nil))
    }

    /// Matches `lua_isnone`. Index is out of range entirely.
    pub fn is_none(&self, idx: i32) -> bool {
        self.value_at(idx).is_none()
    }

    /// Matches `lua_isnoneornil`.
    pub fn is_none_or_nil(&self, idx: i32) -> bool {
        match self.value_at(idx) {
            None | Some(TValue::Nil) => true,
            Some(_) => false,
        }
    }

    /// Matches `lua_isboolean`.
    pub fn is_boolean(&self, idx: i32) -> bool {
        matches!(self.value_at(idx), Some(TValue::False | TValue::True))
    }

    /// Matches `lua_isinteger` — true only for exact integer
    /// subtype, not for integer-valued floats.
    pub fn is_integer(&self, idx: i32) -> bool {
        matches!(self.value_at(idx), Some(TValue::Integer(_)))
    }

    /// Matches `lua_isnumber` — true for both integer and float
    /// subtypes. C Lua additionally coerces strings that look
    /// like numbers; that coercion needs the string-to-number
    /// machinery from Stage 4.2.3 / later.
    pub fn is_number(&self, idx: i32) -> bool {
        matches!(
            self.value_at(idx),
            Some(TValue::Integer(_) | TValue::Number(_))
        )
    }

    /// Matches `lua_isstring`. C Lua also returns true for any
    /// number (since `lua_tolstring` would coerce it). Same
    /// caveat — coercion lands with the number-to-string path.
    pub fn is_string(&self, idx: i32) -> bool {
        matches!(
            self.value_at(idx),
            Some(TValue::ShortString(_) | TValue::LongString(_))
        )
    }

    /// Matches `lua_istable`.
    pub fn is_table(&self, idx: i32) -> bool {
        matches!(self.value_at(idx), Some(TValue::Table(_)))
    }

    /// Matches `lua_isfunction`. True for all three function
    /// flavors (Lua closure, C closure, light C function).
    pub fn is_function(&self, idx: i32) -> bool {
        matches!(
            self.value_at(idx),
            Some(TValue::LuaClosure(_) | TValue::CClosure(_) | TValue::LightCFunction(_))
        )
    }

    /// Matches `lua_iscfunction`. True for C closures and light
    /// C functions, false for Lua closures.
    pub fn is_cfunction(&self, idx: i32) -> bool {
        matches!(
            self.value_at(idx),
            Some(TValue::CClosure(_) | TValue::LightCFunction(_))
        )
    }

    /// Matches `lua_isuserdata`. True for both full and light
    /// userdata.
    pub fn is_userdata(&self, idx: i32) -> bool {
        matches!(
            self.value_at(idx),
            Some(TValue::UserData(_) | TValue::LightUserData(_))
        )
    }

    /// Matches `lua_isthread`.
    pub fn is_thread(&self, idx: i32) -> bool {
        matches!(self.value_at(idx), Some(TValue::Thread(_)))
    }

    /// Matches `lua_toboolean`: `nil` and `false` are false,
    /// everything else (including `0` and the empty string) is
    /// true.
    pub fn to_boolean(&self, idx: i32) -> bool {
        match self.value_at(idx) {
            Some(v) => v.is_truthy(),
            None => false,
        }
    }

    /// Matches `lua_tointegerx` with string coercion disabled.
    /// Returns `None` when the value at `idx` isn't an integer
    /// subtype and isn't an integer-valued float. Uses
    /// [`crate::lobject::to_integer_ns`] for the F2Ieq semantics
    /// Stage 2 already ported.
    pub fn to_integer_x(&self, idx: i32) -> Option<LuaInteger> {
        to_integer_ns(&self.value_at(idx)?)
    }

    /// Matches `lua_tonumberx` with string coercion disabled.
    /// Returns `None` when the value isn't numeric. Integers are
    /// promoted to `f64`.
    pub fn to_number_x(&self, idx: i32) -> Option<LuaNumber> {
        to_number_ns(&self.value_at(idx)?)
    }

    /// Matches `lua_tolstring` in the "no implicit coercion"
    /// mode. Returns the raw byte payload of a short or long
    /// string without mutating the stack. C Lua's version
    /// rewrites numbers in place as their decimal
    /// representation; we defer that to the Stage 5 format
    /// conversion path because it needs the write side of the
    /// API plus the number-printer.
    pub fn to_lstring(&self, idx: i32) -> Option<&[u8]> {
        let slot = self.try_index_to_slot(idx)?;
        let v = self.current_thread().stack[slot as usize];
        match v {
            TValue::ShortString(h) | TValue::LongString(h) => {
                Some(self.global.heap.string(h).bytes.as_slice())
            }
            _ => None,
        }
    }

    /// Raw length of the value at `idx`. Matches `lua_rawlen`:
    /// byte length for strings, [`crate::ltable::Heap::table_len`]
    /// border for tables, payload length for full userdata, and
    /// `0` for anything else.
    pub fn raw_len(&self, idx: i32) -> u64 {
        let Some(v) = self.value_at(idx) else {
            return 0;
        };
        match v {
            TValue::ShortString(h) | TValue::LongString(h) => {
                self.global.heap.string(h).bytes.len() as u64
            }
            TValue::Table(h) => self.global.heap.table_len(h),
            TValue::UserData(h) => self.global.heap.userdata_get(h).data.len() as u64,
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Stage 4.2.3 — push functions. These are the write side of the
// scalar API: every `lua_push*` entry point that takes a C value and
// puts it on the Lua stack. Composite constructors (`push_cclosure`
// with upvalues, `push_fstring` with format args) are deferred until
// the format parser and the C closure builder land.
// ---------------------------------------------------------------------------

impl LuaState {
    /// Matches `lua_pushnil`.
    pub fn push_nil(&mut self) {
        self.current_thread_mut().push(TValue::Nil);
    }

    /// Matches `lua_pushboolean`.
    pub fn push_boolean(&mut self, b: bool) {
        let v = if b { TValue::True } else { TValue::False };
        self.current_thread_mut().push(v);
    }

    /// Matches `lua_pushinteger`.
    pub fn push_integer(&mut self, i: LuaInteger) {
        self.current_thread_mut().push(TValue::Integer(i));
    }

    /// Matches `lua_pushnumber`.
    pub fn push_number(&mut self, n: LuaNumber) {
        self.current_thread_mut().push(TValue::Number(n));
    }

    /// Matches `lua_pushlstring`. The bytes are copied into the
    /// string arena and interned when short; long strings are
    /// allocated fresh. The stack slot gets the matching
    /// [`TValue::ShortString`] / [`TValue::LongString`] variant.
    ///
    /// Returns the [`crate::contract::StringHandle`] of the
    /// newly-created (or existing, for short strings) entry so
    /// callers that need to reference it without going back
    /// through the stack can.
    pub fn push_lstring(&mut self, bytes: &[u8]) -> crate::contract::StringHandle {
        let seed = self.global.hash_seed;
        let handle = self.global.new_string(bytes, seed);
        let value = if bytes.len() <= crate::lstring::LUAI_MAXSHORTLEN {
            TValue::ShortString(handle)
        } else {
            TValue::LongString(handle)
        };
        self.current_thread_mut().push(value);
        handle
    }

    /// Convenience wrapper over [`LuaState::push_lstring`] that
    /// accepts a `&str`. Mirrors C's `lua_pushstring` which takes
    /// a null-terminated `const char *`.
    pub fn push_string(&mut self, s: &str) -> crate::contract::StringHandle {
        self.push_lstring(s.as_bytes())
    }

    /// Matches `lua_pushlightuserdata`. Stores the raw pointer
    /// verbatim; the GC ignores it.
    pub fn push_light_userdata(&mut self, p: *mut std::os::raw::c_void) {
        self.current_thread_mut().push(TValue::LightUserData(p));
    }

    /// Matches `lua_pushcfunction` (the no-upvalue C function
    /// case — the "light" C function variant in our split).
    /// The closure variant with captured values lives in
    /// Stage 4.2.5 because it needs to read values off the stack.
    pub fn push_light_cfunction(&mut self, f: crate::contract::RawCFunction) {
        self.current_thread_mut().push(TValue::LightCFunction(f));
    }
}

// ---------------------------------------------------------------------------
// Stage 4.2.4 — table get/set via the stack. These are the wrappers
// over ltable's raw table methods that pop keys/values from the stack,
// dispatch through the arena, and push results back.
//
// Metamethod-invoking variants (lua_gettable, lua_settable, which
// trigger __index / __newindex) are deferred until Stage 5 ships the
// VM and the metamethod dispatch machinery. What lands here is the
// "raw" family plus table creation and metatable install/query.
// ---------------------------------------------------------------------------

impl LuaState {
    /// Push a fresh, empty table onto the stack. `narr` and `nrec`
    /// are hints for the initial array and hash capacities
    /// respectively — C's `lua_createtable` uses them to pre-size
    /// the arena bookkeeping and avoid early rehashes. Matches
    /// `lua_createtable`.
    pub fn create_table(&mut self, narr: usize, nrec: usize) {
        let t = crate::contract::Table {
            array: Vec::with_capacity(narr),
            hash: std::collections::HashMap::with_capacity(nrec),
            metatable: None,
            meta_cache_flags: 0,
        };
        let handle = self.global.heap.alloc_table(t);
        self.current_thread_mut().push(TValue::Table(handle));
    }

    /// Convenience: `create_table(0, 0)`. Matches `lua_newtable`.
    pub fn new_table(&mut self) {
        self.create_table(0, 0);
    }

    /// Pop the key off the top, look it up in the table at `idx`,
    /// and push the result (or nil for missing). Bypasses
    /// metamethods — this is `lua_rawget`, not `lua_gettable`.
    pub fn raw_get(&mut self, idx: i32) -> i32 {
        let table = self.require_table(idx, "raw_get");
        let key = self
            .current_thread_mut()
            .pop()
            .expect("lapi: raw_get stack underflow");
        let value = self
            .global
            .heap
            .table_get(table, key)
            .unwrap_or(TValue::Nil);
        self.current_thread_mut().push(value);
        self.type_at(-1)
    }

    /// Look up `key` (integer) in the table at `idx` and push the
    /// result. Bypasses metamethods. Matches `lua_rawgeti`.
    pub fn raw_get_i(&mut self, idx: i32, key: LuaInteger) -> i32 {
        let table = self.require_table(idx, "raw_get_i");
        let value = self
            .global
            .heap
            .table_get_int(table, key)
            .unwrap_or(TValue::Nil);
        self.current_thread_mut().push(value);
        self.type_at(-1)
    }

    /// Look up short-string `key` in the table at `idx` and push
    /// the result. Creates the string in the intern table if it
    /// doesn't already live there. Matches `lua_getfield` with
    /// the raw-access semantics forced.
    pub fn raw_get_field(&mut self, idx: i32, key: &str) -> i32 {
        let table = self.require_table(idx, "raw_get_field");
        let seed = self.global.hash_seed;
        let key_handle = self.global.new_string(key.as_bytes(), seed);
        let value = self
            .global
            .heap
            .table_get_shortstr(table, key_handle)
            .unwrap_or(TValue::Nil);
        self.current_thread_mut().push(value);
        self.type_at(-1)
    }

    /// Pop value-then-key from the top and raw-set the table at
    /// `idx`. Fires the write barrier via [`GlobalState::table_set`].
    /// Matches `lua_rawset`.
    pub fn raw_set(&mut self, idx: i32) {
        let table = self.require_table(idx, "raw_set");
        // Stack layout: [..., key, value]. Pop value, then key.
        let value = self
            .current_thread_mut()
            .pop()
            .expect("lapi: raw_set stack underflow (value)");
        let key = self
            .current_thread_mut()
            .pop()
            .expect("lapi: raw_set stack underflow (key)");
        self.global.table_set(table, key, value);
    }

    /// Pop the value off the top, raw-set `table[key] = value`
    /// where `key` is an integer. Matches `lua_rawseti`.
    pub fn raw_set_i(&mut self, idx: i32, key: LuaInteger) {
        let table = self.require_table(idx, "raw_set_i");
        let value = self
            .current_thread_mut()
            .pop()
            .expect("lapi: raw_set_i stack underflow");
        self.global.table_set_int(table, key, value);
    }

    /// Pop the value off the top, raw-set `table[key] = value`
    /// where `key` is a string literal. Matches `lua_setfield`
    /// with metamethods bypassed (the metamethod-aware variant
    /// lands with Stage 5).
    pub fn raw_set_field(&mut self, idx: i32, key: &str) {
        let table = self.require_table(idx, "raw_set_field");
        let value = self
            .current_thread_mut()
            .pop()
            .expect("lapi: raw_set_field stack underflow");
        let seed = self.global.hash_seed;
        let key_handle = self.global.new_string(key.as_bytes(), seed);
        self.global
            .table_set_shortstr(table, key_handle, value);
    }

    /// If the value at `idx` has a metatable, push it onto the
    /// stack and return `true`; otherwise return `false` and
    /// leave the stack unchanged. Matches `lua_getmetatable`.
    pub fn get_metatable(&mut self, idx: i32) -> bool {
        let Some(v) = self.value_at(idx) else {
            return false;
        };
        let mt = match v {
            TValue::Table(h) => self.global.heap.table(h).metatable,
            TValue::UserData(h) => self.global.heap.userdata_get(h).metatable,
            _ => None,
        };
        match mt {
            Some(mt_handle) => {
                self.current_thread_mut().push(TValue::Table(mt_handle));
                true
            }
            None => false,
        }
    }

    /// Pop the metatable from the top of the stack (must be a
    /// table or nil) and install it on the value at `idx`
    /// (a table or a userdata). Matches `lua_setmetatable`.
    /// Fires the forward write barrier on (parent, mt) so the
    /// GC invariant is preserved.
    pub fn set_metatable(&mut self, idx: i32) -> bool {
        let target = match self.value_at(idx) {
            Some(v @ (TValue::Table(_) | TValue::UserData(_))) => v,
            _ => return false,
        };
        let top_value = self
            .current_thread_mut()
            .pop()
            .expect("lapi: set_metatable stack underflow");
        let new_mt = match top_value {
            TValue::Nil => None,
            TValue::Table(h) => Some(h),
            _ => panic!("lapi: set_metatable expects table or nil"),
        };
        match target {
            TValue::Table(h) => {
                self.global.heap.table_mut(h).metatable = new_mt;
                if let Some(mt) = new_mt {
                    self.global.barrier_forward(
                        crate::lgc::AnyHandle::Table(h),
                        crate::lgc::AnyHandle::Table(mt),
                    );
                }
                true
            }
            TValue::UserData(h) => {
                self.global.heap.userdata_mut(h).metatable = new_mt;
                if let Some(mt) = new_mt {
                    self.global.barrier_forward(
                        crate::lgc::AnyHandle::UserData(h),
                        crate::lgc::AnyHandle::Table(mt),
                    );
                }
                true
            }
            _ => unreachable!("target was already checked to be a table or userdata"),
        }
    }

    /// Read the table handle at `idx`, panicking with a
    /// diagnostic that names `ctx` if the slot is missing or
    /// not a table. Centralizes the error wording for the
    /// raw get/set family. `pub(crate)` so `lauxlib` can reuse
    /// the same panic message format.
    pub(crate) fn require_table(&self, idx: i32, ctx: &str) -> crate::contract::TableHandle {
        match self.value_at(idx) {
            Some(TValue::Table(h)) => h,
            Some(other) => panic!(
                "lapi: {ctx}: value at idx {idx} is {} (expected table)",
                other.type_name()
            ),
            None => panic!("lapi: {ctx}: idx {idx} out of range"),
        }
    }
}

// ---------------------------------------------------------------------------
// Stage 4.2.5 — GC control + misc. Wraps the collector driver from
// commit 7 of Stage 3 under `lua_gc`-style entry points, plus the
// handful of utility calls that don't fit cleanly into the earlier
// sub-commits: `raw_equal`, `len`, `concat`, `next_key`.
// ---------------------------------------------------------------------------

/// Subcommands accepted by [`LuaState::gc`], mirroring the
/// `LUA_GC*` constants in `lua.h`. Stage 4.2.5 implements the
/// subset that maps cleanly onto the Stage 3 v1 incremental
/// collector; generational and step-mul tuning live in Stage 3 v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcOp {
    /// `LUA_GCCOLLECT` — run a full cycle to completion.
    Collect,
    /// `LUA_GCSTEP` — advance the state machine one step.
    Step,
    /// `LUA_GCCOUNT` — approximate live object count (not kilobytes).
    Count,
    /// `LUA_GCSTOP` — stop the incremental collector. Stage 3 v1
    /// doesn't have a stop flag yet, so this is a recorded no-op
    /// until Stage 3 v2 wires up the gate.
    Stop,
    /// `LUA_GCRESTART` — resume after a `Stop`. Paired no-op.
    Restart,
}

impl LuaState {
    /// Dispatch to a [`GcOp`] subcommand. Matches `lua_gc`'s fan-out
    /// pattern but with a typed enum instead of integer codes. The
    /// return value is command-specific: `Collect`, `Step`, `Stop`,
    /// and `Restart` return `0`; `Count` returns the live object
    /// count.
    pub fn gc(&mut self, op: GcOp) -> i64 {
        match op {
            GcOp::Collect => {
                self.global.full_gc();
                0
            }
            GcOp::Step => {
                let _ = self.global.gc_step();
                0
            }
            GcOp::Count => self.gc_live_count() as i64,
            GcOp::Stop | GcOp::Restart => 0,
        }
    }

    /// Count of live (non-freed) objects across every arena.
    /// Placeholder for C's kilobyte-granularity `LUA_GCCOUNT`.
    pub fn gc_live_count(&self) -> u64 {
        let heap = &self.global.heap;
        let total = live_count(&heap.strings)
            + live_count(&heap.tables)
            + live_count(&heap.protos)
            + live_count(&heap.lclosures)
            + live_count(&heap.cclosures)
            + live_count(&heap.upvals)
            + live_count(&heap.threads)
            + live_count(&heap.userdata);
        total as u64
    }

    /// Raw equality — bit-pattern equality on the two values,
    /// with no `__eq` metamethod dispatch. Matches `lua_rawequal`.
    /// Out-of-range indices are never equal (C returns 0 for those).
    pub fn raw_equal(&self, idx1: i32, idx2: i32) -> bool {
        match (self.value_at(idx1), self.value_at(idx2)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Push the length of the value at `idx` onto the stack.
    /// Matches `lua_len` with the `__len` metamethod disabled —
    /// the metamethod path needs Stage 5's dispatch infra.
    pub fn len(&mut self, idx: i32) {
        let n = self.raw_len(idx);
        self.push_integer(n as LuaInteger);
    }

    /// Pop the top `n` values, concatenate them into a single
    /// string, push the result. Strings and numbers are both
    /// accepted; anything else panics. Matches `lua_concat` in
    /// spirit — without the number-format precision guarantee
    /// that C's `lua_Number2str` / `LUAI_NUMFMT` provide.
    ///
    /// `n == 0` is a no-op that pushes the empty string (C Lua's
    /// behavior).
    pub fn concat(&mut self, n: u32) {
        if n == 0 {
            self.push_string("");
            return;
        }
        if n == 1 {
            // Single argument: leave in place. C's lua_concat does
            // the same — no copy, no-op for n=1.
            return;
        }
        let top = self.current_thread().top;
        assert!(
            top >= n,
            "lapi: concat({}) underflows the stack (top={})",
            n,
            top
        );
        let start = top - n;
        let mut buffer: Vec<u8> = Vec::new();
        for i in start..top {
            let v = self.current_thread().stack[i as usize];
            match v {
                TValue::ShortString(h) | TValue::LongString(h) => {
                    buffer.extend_from_slice(&self.global.heap.string(h).bytes);
                }
                TValue::Integer(i) => {
                    buffer.extend_from_slice(i.to_string().as_bytes());
                }
                TValue::Number(f) => {
                    buffer.extend_from_slice(format!("{f}").as_bytes());
                }
                other => panic!(
                    "lapi: concat: slot {} holds {} (expected string or number)",
                    i,
                    other.type_name()
                ),
            }
        }
        // Pop the n inputs and push the concatenation.
        self.current_thread_mut().set_top(start);
        let _ = self.push_lstring(&buffer);
    }

    /// Table iteration step — pops the current key from the top of
    /// the stack, pushes the next `(key, value)` pair, and returns
    /// `true`. Returns `false` and pops the key when the traversal
    /// is exhausted. Matches `lua_next`.
    pub fn next_key(&mut self, idx: i32) -> bool {
        let table = self.require_table(idx, "next_key");
        let current = self
            .current_thread_mut()
            .pop()
            .expect("lapi: next_key stack underflow");
        let current_key = if matches!(current, TValue::Nil) {
            None
        } else {
            crate::ltable::table_key_from_tvalue(current)
        };
        match self.global.heap.table_next(table, current_key) {
            Some((key, value)) => {
                let Some(key_tvalue) = tvalue_from_table_key(key) else {
                    return false;
                };
                self.current_thread_mut().push(key_tvalue);
                self.current_thread_mut().push(value);
                true
            }
            None => false,
        }
    }
}

/// Helper for [`LuaState::gc_live_count`] — number of `Some`
/// entries in a slot Vec.
fn live_count<T>(slots: &[Option<T>]) -> usize {
    slots.iter().filter(|s| s.is_some()).count()
}

/// Round-trip a [`crate::contract::TableKey`] back into a
/// [`TValue`]. The `LightCFunction` variant stores a `usize` that
/// can't be safely cast back to a function pointer from safe
/// Rust, so that one branch returns `None` — the caller (currently
/// just [`LuaState::next_key`]) signals "end of traversal" in that
/// case, which is a safe lie for Stage 4.
fn tvalue_from_table_key(k: crate::contract::TableKey) -> Option<TValue> {
    use crate::contract::TableKey;
    Some(match k {
        TableKey::False => TValue::False,
        TableKey::True => TValue::True,
        TableKey::Integer(i) => TValue::Integer(i),
        TableKey::Number(bits) => TValue::Number(f64::from_bits(bits)),
        TableKey::LightUserData(p) => {
            TValue::LightUserData(p as *mut std::os::raw::c_void)
        }
        TableKey::ShortString(h) => TValue::ShortString(h),
        TableKey::LongString(h) => TValue::LongString(h),
        TableKey::Table(h) => TValue::Table(h),
        TableKey::LuaClosure(h) => TValue::LuaClosure(h),
        TableKey::LightCFunction(_) => return None,
        TableKey::CClosure(h) => TValue::CClosure(h),
        TableKey::UserData(h) => TValue::UserData(h),
        TableKey::Thread(h) => TValue::Thread(h),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{GcOp, LUA_TNONE};
    use crate::contract::{
        LuaState, TValue, LUA_TBOOLEAN, LUA_TNIL, LUA_TNUMBER, LUA_TSTRING, LUA_TTABLE,
    };

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

    // ---- 4.2.2 type and access functions --------------------------

    #[test]
    fn type_at_returns_lua_tnone_for_out_of_range_index() {
        let state = LuaState::new(0);
        assert_eq!(state.type_at(1), LUA_TNONE);
        assert_eq!(state.type_at(-1), LUA_TNONE);
    }

    #[test]
    fn type_at_maps_every_tvalue_to_the_matching_base_tag() {
        let mut state = LuaState::new(0);
        let s = state.global.heap.alloc_string(crate::contract::LuaString {
            bytes: b"x".to_vec(),
            hash: 0,
            reserved: 0,
            is_long: false,
            hash_ready: false,
        });
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        {
            let thread = state.current_thread_mut();
            thread.push(TValue::Nil);
            thread.push(TValue::True);
            thread.push(TValue::Integer(1));
            thread.push(TValue::Number(1.5));
            thread.push(TValue::ShortString(s));
            thread.push(TValue::Table(t));
        }
        assert_eq!(state.type_at(1), LUA_TNIL as i32);
        assert_eq!(state.type_at(2), LUA_TBOOLEAN as i32);
        assert_eq!(state.type_at(3), LUA_TNUMBER as i32);
        assert_eq!(state.type_at(4), LUA_TNUMBER as i32);
        assert_eq!(state.type_at(5), LUA_TSTRING as i32);
        assert_eq!(state.type_at(6), LUA_TTABLE as i32);
    }

    #[test]
    fn type_name_handles_lua_tnone_and_regular_tags() {
        assert_eq!(LuaState::type_name(LUA_TNONE), "no value");
        assert_eq!(LuaState::type_name(LUA_TNIL as i32), "nil");
        assert_eq!(LuaState::type_name(LUA_TNUMBER as i32), "number");
        assert_eq!(LuaState::type_name(LUA_TTABLE as i32), "table");
    }

    #[test]
    fn is_nil_distinguishes_nil_from_absent_slot() {
        let state = state_with_values(&[TValue::Nil, TValue::Integer(1)]);
        assert!(state.is_nil(1));
        assert!(!state.is_nil(2));
        // Out-of-range is NOT nil — it's none.
        assert!(!state.is_nil(3));
        assert!(state.is_none(3));
        assert!(state.is_none_or_nil(1));
        assert!(state.is_none_or_nil(3));
        assert!(!state.is_none_or_nil(2));
    }

    #[test]
    fn is_integer_vs_is_number_distinguishes_subtypes() {
        let state = state_with_values(&[TValue::Integer(1), TValue::Number(1.5)]);
        assert!(state.is_integer(1));
        assert!(!state.is_integer(2));
        assert!(state.is_number(1));
        assert!(state.is_number(2));
    }

    #[test]
    fn is_function_covers_all_three_function_flavors() {
        // LuaClosure -> is_function true, is_cfunction false.
        let mut state = LuaState::new(0);
        let proto = state.global.heap.alloc_proto(crate::contract::Proto::default());
        let closure = state.global.heap.alloc_lclosure(crate::contract::LClosure {
            proto,
            upvalues: vec![],
        });
        state
            .current_thread_mut()
            .push(TValue::LuaClosure(closure));
        assert!(state.is_function(1));
        assert!(!state.is_cfunction(1));
    }

    #[test]
    fn to_boolean_nil_and_false_are_false_rest_is_true() {
        let state = state_with_values(&[
            TValue::Nil,
            TValue::False,
            TValue::True,
            TValue::Integer(0),
        ]);
        assert!(!state.to_boolean(1));
        assert!(!state.to_boolean(2));
        assert!(state.to_boolean(3));
        // 0 is truthy in Lua (unlike C).
        assert!(state.to_boolean(4));
        // Out-of-range is false.
        assert!(!state.to_boolean(99));
    }

    #[test]
    fn to_integer_converts_integer_subtypes_exactly() {
        let state = state_with_values(&[
            TValue::Integer(42),
            TValue::Number(7.0),   // exact integer float
            TValue::Number(7.5),   // non-integer float
            TValue::Nil,
        ]);
        assert_eq!(state.to_integer_x(1), Some(42));
        assert_eq!(state.to_integer_x(2), Some(7));
        assert_eq!(state.to_integer_x(3), None);
        assert_eq!(state.to_integer_x(4), None);
    }

    #[test]
    fn to_number_promotes_integers_to_float() {
        let state = state_with_values(&[
            TValue::Integer(42),
            TValue::Number(1.5),
            TValue::Nil,
        ]);
        assert_eq!(state.to_number_x(1), Some(42.0));
        assert_eq!(state.to_number_x(2), Some(1.5));
        assert_eq!(state.to_number_x(3), None);
    }

    #[test]
    fn to_lstring_returns_raw_bytes_for_string_slots() {
        let mut state = LuaState::new(0);
        let h = state.global.heap.alloc_string(crate::contract::LuaString {
            bytes: b"hello".to_vec(),
            hash: 0,
            reserved: 0,
            is_long: false,
            hash_ready: false,
        });
        state.current_thread_mut().push(TValue::ShortString(h));
        assert_eq!(state.to_lstring(1), Some(b"hello".as_slice()));
        // Non-string returns None (no coercion in Stage 4.2.2).
        state.current_thread_mut().push(TValue::Integer(42));
        assert_eq!(state.to_lstring(2), None);
    }

    #[test]
    fn raw_len_reports_byte_length_for_strings() {
        let mut state = LuaState::new(0);
        let h = state.global.heap.alloc_string(crate::contract::LuaString {
            bytes: b"12345".to_vec(),
            hash: 0,
            reserved: 0,
            is_long: false,
            hash_ready: false,
        });
        state.current_thread_mut().push(TValue::ShortString(h));
        assert_eq!(state.raw_len(1), 5);
    }

    #[test]
    fn raw_len_reports_table_border_via_table_len() {
        let mut state = LuaState::new(0);
        let t = state.global.heap.alloc_table(crate::contract::Table {
            array: vec![
                TValue::Integer(1),
                TValue::Integer(2),
                TValue::Integer(3),
            ],
            ..crate::contract::Table::default()
        });
        state.current_thread_mut().push(TValue::Table(t));
        assert_eq!(state.raw_len(1), 3);
    }

    // ---- 4.2.3 push functions -------------------------------------

    #[test]
    fn push_nil_increments_top_and_stores_nil() {
        let mut state = LuaState::new(0);
        state.push_nil();
        assert_eq!(state.get_top(), 1);
        assert!(state.is_nil(1));
    }

    #[test]
    fn push_boolean_true_and_false_store_variant() {
        let mut state = LuaState::new(0);
        state.push_boolean(true);
        state.push_boolean(false);
        assert!(state.to_boolean(1));
        assert!(!state.to_boolean(2));
    }

    #[test]
    fn push_integer_then_read_back_as_integer_and_number() {
        let mut state = LuaState::new(0);
        state.push_integer(42);
        assert_eq!(state.to_integer_x(1), Some(42));
        assert_eq!(state.to_number_x(1), Some(42.0));
        assert!(state.is_integer(1));
    }

    #[test]
    fn push_number_then_read_back_as_number() {
        let mut state = LuaState::new(0);
        state.push_number(3.25);
        assert_eq!(state.to_number_x(1), Some(3.25));
        assert!(!state.is_integer(1));
    }

    #[test]
    fn push_lstring_short_creates_short_variant_and_returns_handle() {
        let mut state = LuaState::new(0);
        let handle = state.push_lstring(b"hello");
        assert_eq!(state.get_top(), 1);
        assert!(state.is_string(1));
        assert_eq!(state.to_lstring(1), Some(b"hello".as_slice()));
        // Returned handle matches the pushed stack slot.
        let t = state.current_thread();
        match t.stack[0] {
            TValue::ShortString(h) => assert_eq!(h, handle),
            other => panic!("expected ShortString, got {:?}", other),
        }
    }

    #[test]
    fn push_lstring_long_creates_long_variant() {
        let mut state = LuaState::new(0);
        let long_bytes = vec![b'a'; crate::lstring::LUAI_MAXSHORTLEN + 10];
        state.push_lstring(&long_bytes);
        let t = state.current_thread();
        assert!(matches!(t.stack[0], TValue::LongString(_)));
    }

    #[test]
    fn push_lstring_interns_short_strings_on_repeat() {
        // Two pushes of the same short byte slice should share
        // the underlying StringHandle (that's what the intern
        // cache is for).
        let mut state = LuaState::new(0);
        let h1 = state.push_lstring(b"foo");
        let h2 = state.push_lstring(b"foo");
        assert_eq!(h1, h2, "short strings must intern");
    }

    #[test]
    fn push_string_delegates_to_push_lstring() {
        let mut state = LuaState::new(0);
        state.push_string("world");
        assert_eq!(state.to_lstring(1), Some(b"world".as_slice()));
    }

    #[test]
    fn push_light_userdata_stores_raw_pointer() {
        let mut state = LuaState::new(0);
        let ptr: *mut std::os::raw::c_void = 0xDEAD_BEEF as *mut _;
        state.push_light_userdata(ptr);
        let v = state.current_thread().stack[0];
        match v {
            TValue::LightUserData(p) => assert_eq!(p as usize, 0xDEAD_BEEF),
            other => panic!("expected LightUserData, got {:?}", other),
        }
    }

    #[test]
    fn multiple_pushes_stack_in_order() {
        let mut state = LuaState::new(0);
        state.push_integer(10);
        state.push_string("mid");
        state.push_boolean(true);
        assert_eq!(state.get_top(), 3);
        assert_eq!(state.to_integer_x(1), Some(10));
        assert_eq!(state.to_lstring(2), Some(b"mid".as_slice()));
        assert!(state.to_boolean(3));
    }

    // ---- 4.2.4 table get/set via stack ----------------------------

    #[test]
    fn new_table_pushes_empty_table() {
        let mut state = LuaState::new(0);
        state.new_table();
        assert_eq!(state.get_top(), 1);
        assert!(state.is_table(1));
        assert_eq!(state.raw_len(1), 0);
    }

    #[test]
    fn raw_set_i_then_raw_get_i_round_trips_integer_value() {
        let mut state = LuaState::new(0);
        state.new_table();
        // Push value to set, then raw_set_i consumes it.
        state.push_integer(99);
        state.raw_set_i(1, 1); // t[1] = 99
        // Read it back.
        state.raw_get_i(1, 1);
        assert_eq!(state.to_integer_x(-1), Some(99));
    }

    #[test]
    fn raw_set_field_then_raw_get_field_round_trips_string_key() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.push_string("world");
        state.raw_set_field(1, "greeting"); // t.greeting = "world"
        state.raw_get_field(1, "greeting");
        assert_eq!(state.to_lstring(-1), Some(b"world".as_slice()));
    }

    #[test]
    fn raw_get_missing_key_pushes_nil() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.raw_get_i(1, 42);
        assert!(state.is_nil(-1));
        assert_eq!(state.type_at(-1), LUA_TNIL as i32);
    }

    #[test]
    fn raw_set_then_raw_get_with_generic_key() {
        let mut state = LuaState::new(0);
        state.new_table();
        // Stack layout: [table, key, value]
        state.push_string("key");
        state.push_integer(7);
        state.raw_set(1); // pops key+value, stores in table
        // Look it up.
        state.push_string("key");
        state.raw_get(1);
        assert_eq!(state.to_integer_x(-1), Some(7));
    }

    #[test]
    fn get_metatable_returns_false_when_absent() {
        let mut state = LuaState::new(0);
        state.new_table();
        assert!(!state.get_metatable(1));
        // Stack is unchanged — still just the original table.
        assert_eq!(state.get_top(), 1);
    }

    #[test]
    fn set_metatable_then_get_metatable_round_trips() {
        let mut state = LuaState::new(0);
        state.new_table(); // slot 1: target table
        state.new_table(); // slot 2: metatable (on top)
        assert!(state.set_metatable(1));
        // After set_metatable, top = 1 (metatable was popped).
        assert_eq!(state.get_top(), 1);
        // Now get the metatable back.
        assert!(state.get_metatable(1));
        assert!(state.is_table(-1));
    }

    #[test]
    fn set_metatable_with_nil_clears_existing_metatable() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.new_table();
        state.set_metatable(1);
        // Now clear it.
        state.push_nil();
        state.set_metatable(1);
        assert!(!state.get_metatable(1));
    }

    // ---- 4.2.5 GC control + misc ---------------------------------

    #[test]
    fn gc_collect_runs_full_cycle_and_returns_to_pause() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.gc(GcOp::Collect);
        assert_eq!(state.global.gc_state, crate::lgc::GcState::Pause);
    }

    #[test]
    fn gc_step_advances_state_machine_one_step() {
        let mut state = LuaState::new(0);
        // Start collection — takes us to Propagate via gc_step.
        state.gc(GcOp::Step);
        assert_eq!(state.global.gc_state, crate::lgc::GcState::Propagate);
    }

    #[test]
    fn gc_live_count_reports_allocated_objects() {
        let mut state = LuaState::new(0);
        let baseline = state.gc_live_count();
        state.new_table();
        state.new_table();
        assert_eq!(state.gc_live_count(), baseline + 2);
    }

    #[test]
    fn raw_equal_compares_values_by_bit_pattern() {
        let mut state = LuaState::new(0);
        state.push_integer(42);
        state.push_integer(42);
        state.push_integer(43);
        assert!(state.raw_equal(1, 2));
        assert!(!state.raw_equal(1, 3));
    }

    #[test]
    fn raw_equal_interned_strings_compare_equal_by_handle() {
        let mut state = LuaState::new(0);
        state.push_string("foo");
        state.push_string("foo");
        // Short-string interning means both slots hold the same handle.
        assert!(state.raw_equal(1, 2));
    }

    #[test]
    fn len_pushes_raw_length_of_string_value() {
        let mut state = LuaState::new(0);
        state.push_string("hello");
        state.len(1);
        assert_eq!(state.to_integer_x(-1), Some(5));
    }

    #[test]
    fn len_pushes_raw_length_of_table_border() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.push_integer(10);
        state.raw_set_i(1, 1);
        state.push_integer(20);
        state.raw_set_i(1, 2);
        state.len(1);
        assert_eq!(state.to_integer_x(-1), Some(2));
    }

    #[test]
    fn concat_joins_strings_in_order_and_pops_inputs() {
        let mut state = LuaState::new(0);
        state.push_string("hello ");
        state.push_string("world");
        state.concat(2);
        // Only the result is left.
        assert_eq!(state.get_top(), 1);
        assert_eq!(state.to_lstring(-1), Some(b"hello world".as_slice()));
    }

    #[test]
    fn concat_accepts_integer_operands_via_display() {
        let mut state = LuaState::new(0);
        state.push_string("answer=");
        state.push_integer(42);
        state.concat(2);
        assert_eq!(state.to_lstring(-1), Some(b"answer=42".as_slice()));
    }

    #[test]
    fn concat_with_n_equal_to_one_is_noop() {
        let mut state = LuaState::new(0);
        state.push_string("alone");
        state.concat(1);
        assert_eq!(state.get_top(), 1);
        assert_eq!(state.to_lstring(-1), Some(b"alone".as_slice()));
    }

    #[test]
    fn next_key_iterates_array_part_then_returns_false_at_end() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.push_integer(10);
        state.raw_set_i(1, 1);
        state.push_integer(20);
        state.raw_set_i(1, 2);

        // Start iteration with nil as the current key.
        state.push_nil();
        assert!(state.next_key(1));
        // Stack: [table, key, value] — verify and pop value.
        assert_eq!(state.to_integer_x(-2), Some(1)); // first key
        assert_eq!(state.to_integer_x(-1), Some(10));
        state.set_top(-2); // drop value, keep key as next cursor

        assert!(state.next_key(1));
        assert_eq!(state.to_integer_x(-2), Some(2));
        assert_eq!(state.to_integer_x(-1), Some(20));
        state.set_top(-2);

        assert!(!state.next_key(1));
    }

    #[test]
    fn raw_set_fires_forward_barrier_during_propagate() {
        // Put GC in propagate with a black target table and a
        // white value to store. raw_set should call barrier_forward
        // and mark the value GRAY.
        let mut state = LuaState::new(0);
        state.new_table();
        let target = match state.current_thread().stack[0] {
            TValue::Table(h) => h,
            _ => unreachable!(),
        };
        state.push_string("x");
        let value_handle = match state.current_thread().stack[1] {
            TValue::ShortString(h) => h,
            _ => unreachable!(),
        };
        // Put the state into Propagate with the target black.
        state.global.heap.marks_tables[target.slot as usize] =
            crate::lgc::BLACK;
        state.global.heap.marks_strings[value_handle.slot as usize] =
            crate::lgc::WHITE;
        state.global.gc_state = crate::lgc::GcState::Propagate;
        // Stack is [table, "x"]. Push the key so we can raw_set.
        state.push_integer(1); // key
        state.push_string("x"); // value on top
        // Re-stamp the value's mark since intern gave us the same handle.
        state.global.heap.marks_strings[value_handle.slot as usize] =
            crate::lgc::WHITE;
        state.raw_set(1);
        assert_eq!(
            state.global.heap.marks_strings[value_handle.slot as usize],
            crate::lgc::GRAY,
            "barrier must mark the stored string gray"
        );
    }
}
