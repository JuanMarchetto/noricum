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

use crate::contract::{LuaInteger, LuaNumber, LuaState};
use crate::lapi::LUA_TNONE;

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
}
