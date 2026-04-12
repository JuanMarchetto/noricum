//! liolib — the Lua `io` library (minimal file I/O).
//!
//! Port of `liolib.c`. Registers `io.*` functions. File handles
//! are stored as `UserData` whose raw bytes contain the
//! serialized path; we re-open on each read/write.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};
use std::io::{Read, Write};

pub fn open_io(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, t, "read", io_read);
    register(state, t, "write", io_write);
    register(state, t, "open", io_open);
    register(state, t, "close", io_close);
    register(state, t, "lines", io_lines);
    register(state, t, "stdout", io_stdout_stub);
    register(state, t, "stderr", io_stderr_stub);
    register(state, t, "stdin", io_stdin_stub);

    let name = state.global.new_string(b"io", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

unsafe extern "C" fn io_read(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => {
            state.push_nil();
            1
        }
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
            }
            if line.ends_with('\r') {
                line.pop();
            }
            state.push_string(&line);
            1
        }
        Err(_) => {
            state.push_nil();
            1
        }
    }
}

unsafe extern "C" fn io_write(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    for i in 1..=top {
        if let Some(s) = state.to_lstring(i) {
            let _ = std::io::stdout().write_all(s);
        } else if let Some(n) = state.to_integer_x(i) {
            let _ = std::io::stdout().write_all(n.to_string().as_bytes());
        } else if let Some(f) = state.to_number_x(i) {
            let _ = std::io::stdout().write_all(format!("{}", f).as_bytes());
        }
    }
    0
}

unsafe extern "C" fn io_open(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let path = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let mode = state.to_lstring(2).map(|s| s.to_vec()).unwrap_or_else(|| b"r".to_vec());
    let p = String::from_utf8_lossy(&path).to_string();
    let m = String::from_utf8_lossy(&mode).to_string();
    // We don't keep the File handle alive; just stash the path +
    // mode as userdata bytes. Read/write helpers re-open.
    let tag = format!("{}:{}", m, p);
    let ud = state.global.heap.alloc_userdata(
        crate::contract::UserData {
            data: tag.into_bytes(),
            metatable: None,
            user_values: vec![],
        },
    );
    // Quick existence check for 'r'.
    if m.starts_with('r') && !std::path::Path::new(&p).exists() {
        state.push_nil();
        state.push_string(&format!("cannot open {}", p));
        return 2;
    }
    state.current_thread_mut().push(TValue::UserData(ud));
    1
}

unsafe extern "C" fn io_close(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Close is a no-op in this minimal impl — files are opened
    // per-operation.
    state.push_boolean(true);
    1
}

unsafe extern "C" fn io_lines(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Minimal: read whole file into memory and iterate lines by
    // index stored in a userdata state. Real impl uses BufReader.
    let _path = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    // TODO: return iterator
    state.push_nil();
    1
}

unsafe extern "C" fn io_stdout_stub(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    1
}
unsafe extern "C" fn io_stderr_stub(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    1
}
unsafe extern "C" fn io_stdin_stub(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    state.push_nil();
    1
}
