//! lbaselib — the Lua base library (minimal).
//!
//! Port of lbaselib.c. Registers `print`, `tostring`, `type`,
//! `tonumber`, `assert`, `error`, `rawequal`, `rawlen` into a
//! globals table accessible as _ENV.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue};

/// Allocate or reuse a globals table and register every
/// base-library function on it. Returns the globals table
/// handle so the caller can hook it up as _ENV upvalue 0 for
/// compiled scripts.
pub fn open_base(state: &mut LuaState) -> crate::contract::TableHandle {
    let globals = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, globals, "print", lua_print);
    register(state, globals, "tostring", lua_tostring);
    register(state, globals, "type", lua_type);
    register(state, globals, "tonumber", lua_tonumber);
    register(state, globals, "assert", lua_assert);
    register(state, globals, "error", lua_error_fn);
    register(state, globals, "rawequal", lua_rawequal);
    register(state, globals, "rawlen", lua_rawlen);
    globals
}

fn register(
    state: &mut LuaState,
    globals: crate::contract::TableHandle,
    name: &str,
    f: RawCFunction,
) {
    let handle = state.global.new_string(name.as_bytes(), state.global.hash_seed);
    state.global.table_set_shortstr(globals, handle, TValue::LightCFunction(f));
}

/// Collect `_G`-keyed output captured during `print` calls for
/// tests. Replaces stdout-based output so unit tests can assert
/// on printed content.
static mut PRINT_BUFFER: Option<Vec<String>> = None;

pub fn take_print_buffer() -> Vec<String> {
    unsafe {
        #[allow(static_mut_refs)]
        PRINT_BUFFER.take().unwrap_or_default()
    }
}

pub fn reset_print_buffer() {
    unsafe {
        PRINT_BUFFER = Some(Vec::new());
    }
}

fn push_to_buffer(s: String) {
    unsafe {
        #[allow(static_mut_refs)]
        match PRINT_BUFFER.as_mut() {
            Some(buf) => buf.push(s),
            None => println!("{}", s),
        }
    }
}

unsafe extern "C" fn lua_print(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top();
    let mut out = String::new();
    for i in 1..=(top as i32) {
        if i > 1 {
            out.push('\t');
        }
        out.push_str(&tv_to_string(state, i));
    }
    push_to_buffer(out);
    0
}

unsafe extern "C" fn lua_tostring(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let s = tv_to_string(state, 1);
    state.push_string(&s);
    1
}

unsafe extern "C" fn lua_type(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let name = tv_type_name(state, 1);
    state.push_string(name);
    1
}

unsafe extern "C" fn lua_tonumber(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if let Some(i) = state.to_integer_x(1) {
        state.push_integer(i);
    } else if let Some(f) = state.to_number_x(1) {
        state.push_number(f);
    } else {
        state.push_nil();
    }
    1
}

unsafe extern "C" fn lua_assert(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if !state.to_boolean(1) {
        let h = state.global.new_string(b"assertion failed!", 0);
        state.raise_error_value(TValue::ShortString(h));
        return 0;
    }
    state.get_top() as i32
}

unsafe extern "C" fn lua_error_fn(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Push arg 1 as the error value via index.
    let h = state.global.new_string(b"error", 0);
    state.raise_error_value(TValue::ShortString(h));
    0
}

unsafe extern "C" fn lua_rawequal(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let eq = state.raw_equal(1, 2);
    state.push_boolean(eq);
    1
}

unsafe extern "C" fn lua_rawlen(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let n = state.raw_len(1);
    state.push_integer(n as i64);
    1
}

// ---- Helpers --------------------------------------------------------------

fn tv_type_name(state: &LuaState, idx: i32) -> &'static str {
    let tt = state.type_at(idx);
    // Mirrors the base-type tag constants in contract.
    match tt {
        0 => "nil",
        1 => "boolean",
        2 => "userdata",
        3 => "number",
        4 => "string",
        5 => "table",
        6 => "function",
        7 => "userdata",
        8 => "thread",
        _ => "no value",
    }
}

fn tv_to_string(state: &mut LuaState, idx: i32) -> String {
    if let Some(bytes) = state.to_lstring(idx) {
        return String::from_utf8_lossy(bytes).to_string();
    }
    if let Some(i) = state.to_integer_x(idx) {
        return i.to_string();
    }
    if let Some(f) = state.to_number_x(idx) {
        return format!("{}", f);
    }
    // Fallback: type-name + opaque handle.
    let tt = state.type_at(idx);
    match tt {
        0 => "nil".to_string(),
        1 => if state.to_boolean(idx) { "true" } else { "false" }.to_string(),
        _ => format!("{}: ?", tv_type_name(state, idx)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_base_registers_print() {
        let mut state = LuaState::new(0);
        let globals = open_base(&mut state);
        let print_name = state.global.new_string(b"print", 0);
        let v = state.global.heap.table_get_shortstr(globals, print_name);
        assert!(matches!(v, Some(TValue::LightCFunction(_))));
    }

    #[test]
    fn print_buffers_output_when_buffer_enabled() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        reset_print_buffer();
        unsafe {
            state.push_light_cfunction(lua_print);
            state.push_integer(42);
            state.call_value(0, 1, 0).unwrap();
        }
        let buf = take_print_buffer();
        assert_eq!(buf, vec!["42".to_string()]);
    }

    #[test]
    fn tostring_integer() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(lua_tostring);
            state.push_integer(123);
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"123".as_slice()));
        }
    }
}
