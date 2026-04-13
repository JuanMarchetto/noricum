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
    // debug.getinfo(level[, what]) — minimal fields: source,
    // currentline, what, short_src. Skip `level` frames up the
    // call stack (innermost first).
    let level = state.to_integer_x(1).unwrap_or(1) as usize;
    let t = state.global.heap.alloc_table(crate::contract::Table::default());
    let frame = {
        let thread = state.current_thread();
        thread.frames.iter().rev().nth(level).cloned()
    };
    if let Some(fr) = frame {
        let fv = state
            .current_thread()
            .stack
            .get(fr.func as usize)
            .copied()
            .unwrap_or(TValue::Nil);
        if let TValue::LuaClosure(h) = fv {
            let proto_h = state.global.heap.lclosure(h).proto;
            let source = {
                let proto = state.global.heap.proto(proto_h);
                proto
                    .source
                    .map(|sh| state.global.heap.string(sh).bytes.clone())
                    .unwrap_or_default()
            };
            let line = line_for_pc(
                state.global.heap.proto(proto_h),
                fr.saved_pc.saturating_sub(1) as i32,
            );
            let src_k = state.global.new_string(b"source", 0);
            let shandle = state.global.new_string(&source, 0);
            state.global.table_set_shortstr(t, src_k, TValue::ShortString(shandle));
            let line_k = state.global.new_string(b"currentline", 0);
            state
                .global
                .table_set_shortstr(t, line_k, TValue::Integer(line as i64));
            let what_k = state.global.new_string(b"what", 0);
            let lua_val = state.global.new_string(b"Lua", 0);
            state
                .global
                .table_set_shortstr(t, what_k, TValue::ShortString(lua_val));
            let short_k = state.global.new_string(b"short_src", 0);
            let short_val = state.global.new_string(&source, 0);
            state
                .global
                .table_set_shortstr(t, short_k, TValue::ShortString(short_val));
        } else {
            let what_k = state.global.new_string(b"what", 0);
            let c_val = state.global.new_string(b"C", 0);
            state
                .global
                .table_set_shortstr(t, what_k, TValue::ShortString(c_val));
        }
    }
    state.current_thread_mut().push(TValue::Table(t));
    1
}

unsafe extern "C" fn debug_traceback(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Accept (msg, level) both optional. level defaults to 1 (skip
    // the traceback call itself).
    let msg = state
        .to_lstring(1)
        .map(|s| String::from_utf8_lossy(s).to_string());
    let level = state.to_integer_x(2).unwrap_or(1) as usize;

    let mut out = String::new();
    if let Some(m) = msg {
        out.push_str(&m);
        out.push('\n');
    }
    out.push_str("stack traceback:");

    // Walk the call frames from innermost (most recent) outward,
    // skipping the first `level` frames so we don't report the
    // traceback call itself.
    let frames: Vec<(u32, u32, u32)> = {
        let thread = state.current_thread();
        thread
            .frames
            .iter()
            .rev()
            .skip(level)
            .map(|f| (f.func, f.saved_pc, f.top))
            .collect()
    };
    for (func_slot, saved_pc, _top) in frames {
        let fv = state
            .current_thread()
            .stack
            .get(func_slot as usize)
            .copied()
            .unwrap_or(TValue::Nil);
        match fv {
            TValue::LuaClosure(h) => {
                let proto_h = state.global.heap.lclosure(h).proto;
                let proto = state.global.heap.proto(proto_h);
                let source = proto
                    .source
                    .map(|sh| {
                        String::from_utf8_lossy(&state.global.heap.string(sh).bytes).to_string()
                    })
                    .unwrap_or_else(|| "?".to_string());
                // Decode line for (saved_pc - 1): the NEXT instruction
                // was the pending one; we want the currently-executing.
                let pc = saved_pc.saturating_sub(1) as i32;
                let line = line_for_pc(proto, pc);
                out.push_str(&format!("\n\t{}:{}: in function", source, line));
            }
            TValue::LightCFunction(_) | TValue::CClosure(_) => {
                out.push_str("\n\t[C]: in function");
            }
            _ => {
                out.push_str("\n\t?: in function");
            }
        }
    }

    state.push_string(&out);
    1
}

/// Resolve the source line for a given instruction PC using a
/// `Proto`'s `line_info` (1-byte deltas) plus `abs_line_info`
/// absolute checkpoints. Matches `luaG_getfuncline`.
fn line_for_pc(proto: &crate::contract::Proto, pc: i32) -> i32 {
    if pc < 0 || proto.line_info.is_empty() {
        return proto.line_defined;
    }
    // Find the latest absolute-line checkpoint <= pc.
    let mut base_pc: i32 = -1;
    let mut line: i32 = proto.line_defined;
    for abs in &proto.abs_line_info {
        if abs.pc <= pc {
            base_pc = abs.pc;
            line = abs.line;
        } else {
            break;
        }
    }
    // Walk deltas from the checkpoint forward.
    for i in (base_pc + 1)..=pc {
        if let Some(&d) = proto.line_info.get(i as usize) {
            if d as i8 != i8::MIN {
                line += d as i8 as i32;
            }
        }
    }
    line
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
