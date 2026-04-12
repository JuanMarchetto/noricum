//! loslib — the Lua `os` library.
//!
//! Port of `loslib.c`. Registers `os.*` functions.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open_os(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, t, "time", os_time);
    register(state, t, "clock", os_clock);
    register(state, t, "date", os_date);
    register(state, t, "difftime", os_difftime);
    register(state, t, "getenv", os_getenv);
    register(state, t, "remove", os_remove);
    register(state, t, "rename", os_rename);
    register(state, t, "tmpname", os_tmpname);
    register(state, t, "exit", os_exit);
    register(state, t, "execute", os_execute);
    register(state, t, "setlocale", os_setlocale);

    let name = state.global.new_string(b"os", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

unsafe extern "C" fn os_time(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    state.push_integer(t);
    1
}

unsafe extern "C" fn os_clock(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Approximate CPU time: use process start monotonic.
    use std::time::Instant;
    static mut START: Option<Instant> = None;
    let elapsed = unsafe {
        #[allow(static_mut_refs)]
        if START.is_none() {
            START = Some(Instant::now());
        }
        START.unwrap().elapsed().as_secs_f64()
    };
    state.push_number(elapsed);
    1
}

unsafe extern "C" fn os_date(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Minimal: format "*t" returns a table (unimplemented);
    // default format returns ISO-ish string.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    state.push_string(&format!("{}", now));
    1
}

unsafe extern "C" fn os_difftime(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let t2 = state.to_number_x(1).unwrap_or(0.0);
    let t1 = state.to_number_x(2).unwrap_or(0.0);
    state.push_number(t2 - t1);
    1
}

unsafe extern "C" fn os_getenv(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let name = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let key = String::from_utf8_lossy(&name).to_string();
    match std::env::var(&key) {
        Ok(v) => {
            state.push_string(&v);
        }
        Err(_) => state.push_nil(),
    }
    1
}

unsafe extern "C" fn os_remove(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let path = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let p = String::from_utf8_lossy(&path).to_string();
    match std::fs::remove_file(&p) {
        Ok(()) => {
            state.push_boolean(true);
            1
        }
        Err(e) => {
            state.push_nil();
            state.push_string(&format!("{}", e));
            2
        }
    }
}

unsafe extern "C" fn os_rename(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let from = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let to = state.to_lstring(2).map(|s| s.to_vec()).unwrap_or_default();
    let f = String::from_utf8_lossy(&from).to_string();
    let t = String::from_utf8_lossy(&to).to_string();
    match std::fs::rename(&f, &t) {
        Ok(()) => {
            state.push_boolean(true);
            1
        }
        Err(e) => {
            state.push_nil();
            state.push_string(&format!("{}", e));
            2
        }
    }
}

unsafe extern "C" fn os_tmpname(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let tmp = std::env::temp_dir().join(format!("lua_{}", std::process::id()));
    state.push_string(&tmp.to_string_lossy());
    1
}

unsafe extern "C" fn os_exit(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let code = state.to_integer_x(1).unwrap_or(0) as i32;
    std::process::exit(code);
}

unsafe extern "C" fn os_execute(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let cmd = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    if cmd.is_empty() {
        state.push_boolean(true); // "shell available"
        return 1;
    }
    let c = String::from_utf8_lossy(&cmd).to_string();
    match std::process::Command::new("sh").arg("-c").arg(&c).status() {
        Ok(status) => {
            state.push_boolean(status.success());
            state.push_string("exit");
            state
                .push_integer(status.code().unwrap_or(-1) as i64);
            3
        }
        Err(_) => {
            state.push_nil();
            state.push_string("exit");
            state.push_integer(-1);
            3
        }
    }
}

unsafe extern "C" fn os_setlocale(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Locale manipulation not supported — always return "C".
    state.push_string("C");
    1
}
