//! lcorolib — the Lua `coroutine` library.
//!
//! Port of `lcorolib.c`. Coroutine implementation itself is
//! deferred (CPS rewrite planned for Stage 5 v2). This module
//! registers function stubs that raise a runtime error when
//! called, so scripts that reference `coroutine.*` don't trip
//! the "global is nil" error at parse time.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};

pub fn open_coroutine(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, t, "create", co_create);
    register(state, t, "resume", co_resume);
    register(state, t, "yield", co_yield_fn);
    register(state, t, "status", co_status);
    register(state, t, "wrap", co_wrap);
    register(state, t, "isyieldable", co_isyieldable);
    register(state, t, "running", co_running);
    register(state, t, "close", co_close);

    let name = state.global.new_string(b"coroutine", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

fn not_yet(state: &mut LuaState) -> std::os::raw::c_int {
    let h = state
        .global
        .new_string(b"coroutines not implemented yet (Stage 5 v2)", 0);
    state.raise_error_value(TValue::ShortString(h));
    0
}

unsafe extern "C" fn co_create(state: *mut LuaState) -> std::os::raw::c_int {
    not_yet(unsafe { &mut *state })
}
unsafe extern "C" fn co_resume(state: *mut LuaState) -> std::os::raw::c_int {
    not_yet(unsafe { &mut *state })
}
unsafe extern "C" fn co_yield_fn(state: *mut LuaState) -> std::os::raw::c_int {
    not_yet(unsafe { &mut *state })
}
unsafe extern "C" fn co_status(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_string("dead");
    1
}
unsafe extern "C" fn co_wrap(state: *mut LuaState) -> std::os::raw::c_int {
    not_yet(unsafe { &mut *state })
}
unsafe extern "C" fn co_isyieldable(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_boolean(false);
    1
}
unsafe extern "C" fn co_running(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    state.push_boolean(true); // true = main thread
    2
}
unsafe extern "C" fn co_close(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_boolean(true);
    1
}
