//! lauxlib — the `luaL_*` auxiliary library.
//!
//! Port of Lua 5.4's `lauxlib.c` (1202 LOC C). This is the "other
//! half" of the drop-in Lua ABI: convenience wrappers over `lapi`
//! that C library authors use to check arguments, report errors,
//! register library functions, and bootstrap a new Lua state.
//!
//! ## Stage 4.3 split
//!
//! * **4.3.1 (this commit)** — argument check/opt helpers. The
//!   most heavily used set of functions in every C library:
//!   `check_type`, `check_any`, `check_integer`, `check_number`,
//!   `check_lstring`, `check_string`, plus the `opt_*` variants
//!   with defaults.
//! * 4.3.2 — library registration: `new_state`, `new_lib_table`,
//!   `set_funcs`, plus the registry `ref` / `unref` mechanism.
//!
//! ## Deferred
//!
//! * `luaL_error` / `luaL_argerror` / `luaL_typeerror` — these
//!   all eventually call `lua_error` which raises via longjmp
//!   in C. Our Result-threading equivalent lands with Stage 5.
//!   Stage 4.3.1 panics with a formatted message instead; that's
//!   at least loud and diagnosable.
//! * `luaL_openlibs` — needs the standard library implementations
//!   that ship with Stage 8.
//! * `luaL_loadfile` / `luaL_loadbuffer` / `luaL_loadstring` —
//!   need the lexer and parser from Stage 6.
//! * `luaL_checkudata` with metatable validation — needs the
//!   metatable registry machinery from Stage 4.3.2.

#![allow(dead_code)]

use crate::contract::{LuaInteger, LuaNumber, LuaState, RawCFunction, TValue};
use crate::lapi::LUA_TNONE;

// ---------------------------------------------------------------------------
// Public constants for the registry ref mechanism, matching lauxlib.h.
// ---------------------------------------------------------------------------

/// Sentinel returned by [`LuaState::registry_ref`] when the value
/// at the top of the stack is `nil`. Matches `LUA_REFNIL` in
/// `lauxlib.h`.
pub const LUA_REFNIL: i32 = -1;

/// Sentinel indicating a reference that's known to be invalid.
/// Matches `LUA_NOREF` in `lauxlib.h`.
pub const LUA_NOREF: i32 = -2;

/// A single entry in the table passed to [`LuaState::set_funcs`].
/// Matches `luaL_Reg` in `lauxlib.h`, minus the null-terminator
/// sentinel — Rust slices carry their own length.
pub struct LuaLReg {
    pub name: &'static str,
    pub func: RawCFunction,
}

impl LuaState {
    /// Assert that `arg` is a valid (in-range) stack index.
    /// Matches `luaL_checkany`.
    pub fn check_any(&self, arg: i32) {
        if self.type_at(arg) == LUA_TNONE {
            panic!(
                "lauxlib: bad argument #{} (value expected, got no value)",
                arg
            );
        }
    }

    /// Assert that the value at `arg` has base type `expected_tt`.
    /// `expected_tt` is one of the `LUA_T*` base constants. Matches
    /// `luaL_checktype`.
    pub fn check_type(&self, arg: i32, expected_tt: i32) {
        let actual = self.type_at(arg);
        if actual != expected_tt {
            panic!(
                "lauxlib: bad argument #{} ({} expected, got {})",
                arg,
                LuaState::type_name(expected_tt),
                LuaState::type_name(actual)
            );
        }
    }

    /// Return the integer at `arg` or panic with a diagnostic
    /// message. Matches `luaL_checkinteger`. Integer-valued floats
    /// (e.g. `2.0`) are accepted via [`crate::lobject::to_integer_ns`]
    /// — that's the `luaL_checkinteger` semantics.
    pub fn check_integer(&self, arg: i32) -> LuaInteger {
        self.to_integer_x(arg).unwrap_or_else(|| {
            panic!(
                "lauxlib: bad argument #{} (number expected, got {})",
                arg,
                LuaState::type_name(self.type_at(arg))
            );
        })
    }

    /// Return the number at `arg` or panic with a diagnostic.
    /// Matches `luaL_checknumber`. Integers are promoted to float
    /// (Lua's usual numeric coercion).
    pub fn check_number(&self, arg: i32) -> LuaNumber {
        self.to_number_x(arg).unwrap_or_else(|| {
            panic!(
                "lauxlib: bad argument #{} (number expected, got {})",
                arg,
                LuaState::type_name(self.type_at(arg))
            );
        })
    }

    /// Return the raw byte payload of the string at `arg` or
    /// panic. Matches `luaL_checklstring` — the version that
    /// returns the length alongside the pointer in C. In Rust
    /// the slice carries its own length, so the separate `len`
    /// out-parameter is gone.
    ///
    /// Stage 4.3.1 does NOT do implicit number-to-string
    /// coercion (C's `luaL_checklstring` rewrites the slot if
    /// it sees a number). Needs the stack-mutating `to_lstring`
    /// path from later stages.
    pub fn check_lstring(&self, arg: i32) -> &[u8] {
        let Some(bytes) = self.to_lstring(arg) else {
            panic!(
                "lauxlib: bad argument #{} (string expected, got {})",
                arg,
                LuaState::type_name(self.type_at(arg))
            );
        };
        bytes
    }

    /// UTF-8 view of [`LuaState::check_lstring`]. Panics if the
    /// bytes aren't valid UTF-8. Lua strings are byte-string in
    /// theory, so callers that need byte semantics should use
    /// `check_lstring` instead.
    pub fn check_string(&self, arg: i32) -> &str {
        let bytes = self.check_lstring(arg);
        std::str::from_utf8(bytes).unwrap_or_else(|err| {
            panic!(
                "lauxlib: bad argument #{} (valid UTF-8 expected): {}",
                arg, err
            );
        })
    }

    /// If the argument is absent or nil, return `default`;
    /// otherwise check that it's an integer. Matches
    /// `luaL_optinteger`.
    pub fn opt_integer(&self, arg: i32, default: LuaInteger) -> LuaInteger {
        if self.is_none_or_nil(arg) {
            default
        } else {
            self.check_integer(arg)
        }
    }

    /// Same pattern for numbers. Matches `luaL_optnumber`.
    pub fn opt_number(&self, arg: i32, default: LuaNumber) -> LuaNumber {
        if self.is_none_or_nil(arg) {
            default
        } else {
            self.check_number(arg)
        }
    }

    /// Same pattern for byte strings. Matches `luaL_optlstring`.
    pub fn opt_lstring<'a>(&'a self, arg: i32, default: &'a [u8]) -> &'a [u8] {
        if self.is_none_or_nil(arg) {
            default
        } else {
            self.check_lstring(arg)
        }
    }

    /// Same for UTF-8 strings. Panics on non-UTF-8 if the
    /// argument is present. Matches `luaL_optstring`.
    pub fn opt_string<'a>(&'a self, arg: i32, default: &'a str) -> &'a str {
        if self.is_none_or_nil(arg) {
            default
        } else {
            self.check_string(arg)
        }
    }

    // ---- 4.3.2 library registration and ref mechanism -------------

    /// Bootstrap a fresh state with the default hash seed.
    /// Matches `luaL_newstate`.
    pub fn aux_new_state() -> LuaState {
        LuaState::new(0)
    }

    /// Push a freshly-created table onto the stack sized as a
    /// hint for `n` future entries (integer and record parts
    /// both pre-allocated to `n`). Matches `luaL_newlibtable`.
    pub fn new_lib_table(&mut self, n: usize) {
        self.create_table(0, n);
    }

    /// Register every `{ name, func }` pair in `regs` into the
    /// table at `table_idx`, storing each function as a
    /// `TValue::LightCFunction`. Stage 4.3.2 only supports the
    /// zero-upvalue form (C's `luaL_setfuncs(L, l, 0)`); the
    /// upvalue-capturing variant needs the C closure builder
    /// from Stage 4.2's deferred work.
    pub fn set_funcs(&mut self, table_idx: i32, regs: &[LuaLReg]) {
        let abs_idx = self.absindex(table_idx);
        for reg in regs {
            self.push_light_cfunction(reg.func);
            self.raw_set_field(abs_idx, reg.name);
        }
    }

    /// Convenience: create a fresh table and register every
    /// function in `regs` into it. Matches `luaL_newlib`. The
    /// new table is left on the top of the stack.
    pub fn new_lib(&mut self, regs: &[LuaLReg]) {
        self.new_lib_table(regs.len());
        // set_funcs takes an index; top is -1.
        self.set_funcs(-1, regs);
    }

    /// Pop the top value, store it in the table at `table_idx`
    /// under a fresh integer key, and return that key. `nil`
    /// returns [`LUA_REFNIL`] without mutating the table.
    ///
    /// Stage 4.3.2 uses a simple "next available key" strategy
    /// based on [`crate::ltable::Heap::table_len`]. C Lua keeps
    /// a free-list linked through `table[0]` for O(1) reclaim
    /// on unref; we defer that optimization until a profile
    /// shows the linear scan is a problem.
    pub fn registry_ref(&mut self, table_idx: i32) -> i32 {
        let value = self
            .current_thread_mut()
            .pop()
            .unwrap_or(TValue::Nil);
        if matches!(value, TValue::Nil) {
            return LUA_REFNIL;
        }
        let abs_idx = self.absindex(table_idx);
        let table = self.require_table(abs_idx, "registry_ref");
        let next = self.global.heap.table_len(table) as LuaInteger + 1;
        self.global.table_set_int(table, next, value);
        next as i32
    }

    /// Remove the reference `ref_id` from the table at
    /// `table_idx`. Matches `luaL_unref`. Ignores
    /// [`LUA_REFNIL`] and [`LUA_NOREF`] — both are valid
    /// to pass as a "no-op" cleanup signal.
    pub fn registry_unref(&mut self, table_idx: i32, ref_id: i32) {
        if ref_id < 0 {
            return;
        }
        let abs_idx = self.absindex(table_idx);
        let table = self.require_table(abs_idx, "registry_unref");
        self.global
            .table_set_int(table, ref_id as LuaInteger, TValue::Nil);
    }
}

#[cfg(test)]
mod tests {
    use crate::contract::{LuaState, TValue, LUA_TBOOLEAN, LUA_TNIL, LUA_TTABLE};

    #[test]
    fn check_type_accepts_matching_type() {
        let mut state = LuaState::new(0);
        state.push_nil();
        state.check_type(1, LUA_TNIL as i32);
    }

    #[test]
    #[should_panic(expected = "bad argument #1")]
    fn check_type_panics_on_mismatch() {
        let mut state = LuaState::new(0);
        state.push_integer(42);
        state.check_type(1, LUA_TTABLE as i32);
    }

    #[test]
    #[should_panic(expected = "value expected")]
    fn check_any_panics_on_out_of_range() {
        let state = LuaState::new(0);
        state.check_any(1);
    }

    #[test]
    fn check_integer_accepts_integer_and_integer_float() {
        let mut state = LuaState::new(0);
        state.push_integer(42);
        state.push_number(7.0);
        assert_eq!(state.check_integer(1), 42);
        assert_eq!(state.check_integer(2), 7);
    }

    #[test]
    #[should_panic(expected = "number expected")]
    fn check_integer_panics_on_string() {
        let mut state = LuaState::new(0);
        state.push_string("not a number");
        let _ = state.check_integer(1);
    }

    #[test]
    #[should_panic(expected = "number expected")]
    fn check_integer_panics_on_non_integral_float() {
        let mut state = LuaState::new(0);
        state.push_number(1.5);
        let _ = state.check_integer(1);
    }

    #[test]
    fn check_number_promotes_integer_to_float() {
        let mut state = LuaState::new(0);
        state.push_integer(10);
        assert_eq!(state.check_number(1), 10.0);
    }

    #[test]
    fn check_lstring_returns_byte_slice() {
        let mut state = LuaState::new(0);
        state.push_string("hello");
        assert_eq!(state.check_lstring(1), b"hello");
    }

    #[test]
    fn check_string_returns_utf8_slice() {
        let mut state = LuaState::new(0);
        state.push_string("hola");
        assert_eq!(state.check_string(1), "hola");
    }

    #[test]
    fn opt_integer_returns_default_for_none_or_nil() {
        let mut state = LuaState::new(0);
        state.push_nil();
        assert_eq!(state.opt_integer(1, 99), 99);
        // Out-of-range also falls back to default.
        assert_eq!(state.opt_integer(2, 42), 42);
    }

    #[test]
    fn opt_integer_uses_provided_value_when_present() {
        let mut state = LuaState::new(0);
        state.push_integer(5);
        assert_eq!(state.opt_integer(1, 99), 5);
    }

    #[test]
    fn opt_number_and_opt_string_fall_back_on_nil() {
        let mut state = LuaState::new(0);
        state.push_nil();
        assert_eq!(state.opt_number(1, 3.5), 3.5);
        assert_eq!(state.opt_string(1, "default"), "default");
    }

    #[test]
    fn check_type_message_contains_type_names() {
        // Regression: the panic message must include both the
        // expected and actual type names so the C caller can
        // diagnose without stepping through the port.
        let mut state = LuaState::new(0);
        state.push_integer(1);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            state.check_type(1, LUA_TBOOLEAN as i32);
        }));
        let msg = result
            .err()
            .and_then(|b| b.downcast::<String>().ok().map(|s| *s))
            .unwrap_or_default();
        assert!(msg.contains("boolean"), "message {msg:?}");
        assert!(msg.contains("number"), "message {msg:?}");
    }

    #[test]
    fn unused_tvalue_import_silencer() {
        // TValue isn't directly named in most tests (we push via
        // the lapi helpers). This no-op test keeps the import
        // from showing up as unused if a future refactor drops
        // the handful of explicit TValue references in this mod.
        let _ = TValue::Nil;
    }

    // ---- 4.3.2 library registration + ref mechanism ---------------

    use super::{LuaLReg, LUA_REFNIL};
    use crate::contract::{LuaInteger, LuaState as _Alias, RawCFunction};

    /// A tiny C function stub used by the registration tests —
    /// it's never actually called, it just needs to be a valid
    /// `RawCFunction` pointer.
    unsafe extern "C" fn noop_cfn(
        _state: *mut crate::contract::LuaState,
    ) -> std::os::raw::c_int {
        0
    }

    #[test]
    fn new_state_bootstraps_usable_state() {
        let state = LuaState::aux_new_state();
        // A freshly-bootstrapped state has no stack slots but a
        // live registry and main thread.
        assert_eq!(state.get_top(), 0);
        assert!(state.global.registry.is_some());
        assert!(state.global.main_thread.is_some());
    }

    #[test]
    fn new_lib_table_creates_empty_table_with_capacity_hint() {
        let mut state = LuaState::new(0);
        state.new_lib_table(8);
        assert!(state.is_table(-1));
        assert_eq!(state.raw_len(-1), 0);
    }

    #[test]
    fn set_funcs_registers_every_entry_as_light_cfunction() {
        let mut state = LuaState::new(0);
        let regs = [
            LuaLReg {
                name: "a",
                func: noop_cfn as RawCFunction,
            },
            LuaLReg {
                name: "b",
                func: noop_cfn as RawCFunction,
            },
        ];
        state.new_table();
        state.set_funcs(-1, &regs);
        // Read back each entry — each should be a function.
        state.raw_get_field(1, "a");
        assert!(state.is_cfunction(-1));
        state.set_top(-2); // pop
        state.raw_get_field(1, "b");
        assert!(state.is_cfunction(-1));
    }

    #[test]
    fn new_lib_combines_new_lib_table_and_set_funcs() {
        let mut state = LuaState::new(0);
        let regs = [LuaLReg {
            name: "only",
            func: noop_cfn as RawCFunction,
        }];
        state.new_lib(&regs);
        // Top of stack is now a table containing "only".
        assert!(state.is_table(-1));
        state.raw_get_field(-1, "only");
        assert!(state.is_cfunction(-1));
    }

    #[test]
    fn registry_ref_stores_value_and_returns_integer_key() {
        let mut state = LuaState::new(0);
        state.new_table(); // registry-like
        state.push_integer(42);
        let ref_id = state.registry_ref(1);
        assert!(ref_id > 0);
        // Read back via raw_get_i.
        state.raw_get_i(1, ref_id as LuaInteger);
        assert_eq!(state.to_integer_x(-1), Some(42));
    }

    #[test]
    fn registry_ref_on_nil_returns_lua_refnil() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.push_nil();
        let ref_id = state.registry_ref(1);
        assert_eq!(ref_id, LUA_REFNIL);
    }

    #[test]
    fn registry_unref_clears_the_stored_slot() {
        let mut state = LuaState::new(0);
        state.new_table();
        state.push_string("keepme");
        let ref_id = state.registry_ref(1);
        state.registry_unref(1, ref_id);
        // Look it up — should now be nil.
        state.raw_get_i(1, ref_id as LuaInteger);
        assert!(state.is_nil(-1));
    }

    #[test]
    fn registry_unref_ignores_lua_refnil_and_noref() {
        let mut state = LuaState::new(0);
        state.new_table();
        // These are no-ops, must not panic or touch the table.
        state.registry_unref(1, LUA_REFNIL);
        state.registry_unref(1, super::LUA_NOREF);
    }

    #[test]
    fn unused_alias_silencer() {
        // The `_Alias` import keeps the cross-reference visible
        // in case a future refactor reshapes the test imports.
        let _ = _Alias::new(0);
    }
}
