//! ldblib — the Lua `debug` library.
//!
//! Port of `ldblib.c`. Minimal stubs for introspection.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};

pub fn open_debug(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, t, "getinfo", debug_getinfo);
    register(state, t, "traceback", debug_traceback);
    register(state, t, "sethook", debug_sethook);
    register(state, t, "gethook", debug_gethook);
    register(state, t, "getlocal", debug_getlocal);
    register(state, t, "setlocal", debug_setlocal);
    register(state, t, "getupvalue", debug_getupvalue);
    register(state, t, "setupvalue", debug_setupvalue);
    register(state, t, "getregistry", debug_getregistry);

    let name = state.global.new_string(b"debug", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

unsafe extern "C" fn debug_getinfo(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let t = state.global.heap.alloc_table(crate::contract::Table::default());
    state.current_thread_mut().push(TValue::Table(t));
    1
}

unsafe extern "C" fn debug_traceback(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let msg = state.to_lstring(1).map(|s| s.to_vec());
    let out = match msg {
        Some(m) => String::from_utf8_lossy(&m).to_string(),
        None => String::new(),
    };
    state.push_string(&out);
    1
}

unsafe extern "C" fn debug_sethook(state: *mut LuaState) -> std::os::raw::c_int {
    let _ = state;
    0
}

unsafe extern "C" fn debug_gethook(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    state.push_string("");
    state.push_integer(0);
    3
}

unsafe extern "C" fn debug_getlocal(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    1
}

unsafe extern "C" fn debug_setlocal(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    1
}

unsafe extern "C" fn debug_getupvalue(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    1
}

unsafe extern "C" fn debug_setupvalue(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    1
}

unsafe extern "C" fn debug_getregistry(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    match state.global.registry {
        Some(t) => state.current_thread_mut().push(TValue::Table(t)),
        None => state.push_nil(),
    }
    1
}
