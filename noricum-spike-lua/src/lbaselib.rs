//! lbaselib — the Lua base library (full port).
//!
//! Port of `lbaselib.c` in Lua 5.4/5.5. Registers every global
//! from the base library into a globals table that the parser
//! binds as the `_ENV` upvalue of compiled scripts.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue};

/// Allocate a globals table, register every base-library function
/// on it, and return the handle so it can be bound as `_ENV` on
/// a compiled closure.
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
    register(state, globals, "rawget", lua_rawget);
    register(state, globals, "rawset", lua_rawset);
    register(state, globals, "setmetatable", lua_setmetatable);
    register(state, globals, "getmetatable", lua_getmetatable);
    register(state, globals, "select", lua_select);
    register(state, globals, "ipairs", lua_ipairs);
    register(state, globals, "pairs", lua_pairs);
    register(state, globals, "next", lua_next);
    register(state, globals, "pcall", lua_pcall);
    register(state, globals, "xpcall", lua_xpcall);
    register(state, globals, "collectgarbage", lua_collectgarbage);
    // Install the globals table as the value of _ENV inside the
    // globals table itself so scripts can do `_G = _ENV` or iterate.
    let env_h = state.global.new_string(b"_G", state.global.hash_seed);
    state.global.table_set_shortstr(globals, env_h, TValue::Table(globals));
    // Store the Lua version.
    let ver_key = state.global.new_string(b"_VERSION", state.global.hash_seed);
    let ver_val = state.global.new_string(b"Lua 5.4", state.global.hash_seed);
    state
        .global
        .table_set_shortstr(globals, ver_key, TValue::ShortString(ver_val));
    globals
}

fn register(
    state: &mut LuaState,
    globals: crate::contract::TableHandle,
    name: &str,
    f: RawCFunction,
) {
    let handle = state.global.new_string(name.as_bytes(), state.global.hash_seed);
    state
        .global
        .table_set_shortstr(globals, handle, TValue::LightCFunction(f));
}

// ---- Print buffering for tests -------------------------------------------

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

// ---- print / tostring / type / tonumber ----------------------------------

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
    // Honour __tostring metamethod: if defined, call it with the
    // value as the only argument and use whatever it returns
    // (which must be a string per Lua spec; we don't enforce that
    // here — bad returns just propagate as-is).
    let v = state.value_at_public(1).unwrap_or(crate::contract::TValue::Nil);
    // __tostring isn't on the fast-method table in C Lua either —
    // it's looked up by name on the metatable. Mirror that.
    let mt = state.global.metatable_for_value(v);
    let mm = if let Some(mt_h) = mt {
        let k = state.global.new_string(b"__tostring", 0);
        state.global.heap.table_get_shortstr(mt_h, k).unwrap_or(crate::contract::TValue::Nil)
    } else {
        crate::contract::TValue::Nil
    };
    if !matches!(mm, crate::contract::TValue::Nil) {
        // Push mm + value, call with 1 arg expecting 1 result.
        let base = state.get_top();
        state.current_thread_mut().push(mm);
        state.current_thread_mut().push(v);
        let func_abs = state.frame_base_index().unwrap_or(0) + base as u32;
        let _ = state.call_value(func_abs, 1, 1);
        // Result is at top of stack — leave it there for the return.
        return 1;
    }
    let s = tv_to_string(state, 1);
    state.push_string(&s);
    1
}

unsafe extern "C" fn lua_type(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if state.get_top() < 1 {
        let h = state.global.new_string(b"bad argument #1 to 'type' (value expected)", 0);
        state.raise_error_value(TValue::ShortString(h));
        return 0;
    }
    let name = tv_type_name(state, 1);
    state.push_string(name);
    1
}

unsafe extern "C" fn lua_tonumber(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    // Two-arg form: tonumber(s, base). When `base` is given, the
    // first arg must be a string and the result is an integer
    // parsed in that base.
    if top >= 2 {
        let base = state.to_integer_x(2).unwrap_or(10) as u32;
        if let Some(bytes) = state.to_lstring(1) {
            let s = std::str::from_utf8(bytes).unwrap_or("").trim();
            if let Ok(n) = i64::from_str_radix(s, base) {
                state.push_integer(n);
                return 1;
            }
        }
        state.push_nil();
        return 1;
    }
    // Single-arg form. First check the underlying TValue: numbers
    // pass through, strings get parsed as int-then-float.
    use crate::contract::TValue;
    match state.value_at_public(1) {
        Some(TValue::Integer(i)) => {
            state.push_integer(i);
            return 1;
        }
        Some(TValue::Number(f)) => {
            state.push_number(f);
            return 1;
        }
        Some(TValue::ShortString(_)) | Some(TValue::LongString(_)) => {
            if let Some(bytes) = state.to_lstring(1) {
                let s = std::str::from_utf8(bytes).unwrap_or("").trim();
                if let Ok(i) = s.parse::<i64>() {
                    state.push_integer(i);
                    return 1;
                }
                if let Ok(f) = s.parse::<f64>() {
                    state.push_number(f);
                    return 1;
                }
                // Hex support. Strip optional sign first, then match
                // '0x'. Accept both integer and float shapes.
                let (neg, rest0) = match s.as_bytes().first() {
                    Some(b'+') => (false, &s[1..]),
                    Some(b'-') => (true, &s[1..]),
                    _ => (false, s),
                };
                let rest0 = rest0.trim_start();
                if let Some(hex) = rest0
                    .strip_prefix("0x")
                    .or_else(|| rest0.strip_prefix("0X"))
                {
                    if !hex.contains('.') && !hex.contains('p') && !hex.contains('P') {
                        if let Ok(u) = u64::from_str_radix(hex, 16) {
                            let signed = if neg { (u as i64).wrapping_neg() } else { u as i64 };
                            state.push_integer(signed);
                            return 1;
                        }
                    }
                    if let Some(mut f) = crate::llex::LexState::parse_hex_float_public(rest0) {
                        if neg { f = -f; }
                        state.push_number(f);
                        return 1;
                    }
                }
            }
        }
        _ => {}
    }
    state.push_nil();
    1
}

// ---- assert / error -------------------------------------------------------

unsafe extern "C" fn lua_assert(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if !state.to_boolean(1) {
        let top = state.get_top();
        if top >= 2 {
            match state.value_at_public(2).unwrap_or(TValue::Nil) {
                TValue::ShortString(_) | TValue::LongString(_) => {
                    let bytes = state
                        .to_lstring(2)
                        .unwrap_or(b"assertion failed!")
                        .to_vec();
                    let final_bytes = if let Some(loc) = source_location_at_level(state, 1) {
                        let mut b = loc.into_bytes();
                        b.extend_from_slice(&bytes);
                        b
                    } else {
                        bytes
                    };
                    let h = state.global.new_string(&final_bytes, 0);
                    state.raise_error_value(TValue::ShortString(h));
                    return 0;
                }
                other => {
                    // Non-string error values pass through unchanged
                    // (numbers, tables, etc.) — matches C Lua.
                    state.raise_error_value(other);
                    return 0;
                }
            }
        }
        let bytes = b"assertion failed!".to_vec();
        let final_bytes = if let Some(loc) = source_location_at_level(state, 1) {
            let mut b = loc.into_bytes();
            b.extend_from_slice(&bytes);
            b
        } else {
            bytes
        };
        let h = state.global.new_string(&final_bytes, 0);
        state.raise_error_value(TValue::ShortString(h));
        return 0;
    }
    state.get_top() as i32
}

unsafe extern "C" fn lua_error_fn(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top();
    // Optional second arg: level (default 1 = caller). Level > 0
    // prepends "source:line: " to a string error using the call
    // frame `level` levels above this `error` call.
    let level = state.to_integer_x(2).unwrap_or(1);
    let v = if top >= 1 {
        // Pass the value through unchanged for non-string types.
        // Strings get prefixed with the source location.
        match state.value_at_public(1).unwrap_or(TValue::Nil) {
            TValue::ShortString(_) | TValue::LongString(_) => {
                let bytes = state.to_lstring(1).unwrap_or(b"error").to_vec();
                if level > 0 {
                    if let Some(loc) = source_location_at_level(state, level as usize) {
                        let mut prefixed = loc.into_bytes();
                        prefixed.extend_from_slice(&bytes);
                        let h = state.global.new_string(&prefixed, 0);
                        TValue::ShortString(h)
                    } else {
                        let h = state.global.new_string(&bytes, 0);
                        TValue::ShortString(h)
                    }
                } else {
                    let h = state.global.new_string(&bytes, 0);
                    TValue::ShortString(h)
                }
            }
            other => other,
        }
    } else {
        TValue::Nil
    };
    state.raise_error_value(v);
    0
}

/// Return `"source:line: "` for the call frame `level` frames above
/// the running C function. Level 1 = the immediate Lua caller of
/// `error()`, matching C Lua's `lua_error` behavior.
fn source_location_at_level(state: &LuaState, level: usize) -> Option<String> {
    use crate::contract::TValue;
    let thread = state.current_thread();
    // Skip the C error() frame itself, then walk up `level` frames.
    let frame = thread.frames.iter().rev().nth(level)?;
    let func_val = thread.stack.get(frame.func as usize).copied()?;
    let TValue::LuaClosure(h) = func_val else { return None };
    let proto_h = state.global.heap.lclosure(h).proto;
    let proto = state.global.heap.proto(proto_h);
    let source_bytes = proto
        .source
        .map(|sh| state.global.heap.string(sh).bytes.clone())
        .unwrap_or_else(|| b"?".to_vec());
    let mut src = String::from_utf8_lossy(&source_bytes).to_string();
    // Lua source convention: "@filename" → "filename"; "=name" → "name".
    if src.starts_with('@') || src.starts_with('=') {
        src.remove(0);
    }
    let pc = frame.saved_pc.saturating_sub(1) as i32;
    let line = line_for_pc(proto, pc);
    Some(format!("{}:{}: ", src, line))
}

fn line_for_pc(proto: &crate::contract::Proto, pc: i32) -> i32 {
    if pc < 0 || proto.line_info.is_empty() {
        return proto.line_defined;
    }
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
    for i in (base_pc + 1)..=pc {
        if let Some(&d) = proto.line_info.get(i as usize) {
            if d as i8 != i8::MIN {
                line += d as i8 as i32;
            }
        }
    }
    line
}

// ---- raw operations -------------------------------------------------------

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

unsafe extern "C" fn lua_rawget(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Input: t at index 1, key at index 2. raw_get expects the
    // key on top of stack — push_value copies index 2 to top,
    // then raw_get(1) pops the key and pushes t[key].
    state.push_value(2);
    state.raw_get(1);
    1
}

unsafe extern "C" fn lua_rawset(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Input: t at index 1, key at 2, value at 3.
    // raw_set(idx=1) expects key and value on top; pop both and
    // store t[key] := value.
    state.push_value(2);
    state.push_value(3);
    state.raw_set(1);
    state.push_value(1);
    1
}

// ---- metatables -----------------------------------------------------------

unsafe extern "C" fn lua_setmetatable(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Refuse if the existing metatable defines __metatable — Lua's
    // metatable-protection rule. Raise a typed error so pcall sees
    // false; callers without pcall propagate the error normally.
    let t_val = state
        .value_at_public(1)
        .unwrap_or(crate::contract::TValue::Nil);
    if let crate::contract::TValue::Table(th) = t_val {
        // Intern the lookup key BEFORE fetching the metatable handle.
        // new_string may run a GC step which can shuffle handle slots;
        // doing it in this order means `existing_mt` is read after any
        // such reshuffling and stays valid for the table_get_shortstr
        // call below.
        let key_h = state.global.new_string(b"__metatable", 0);
        if let Some(existing_mt) = state.global.heap.table(th).metatable {
            let field = state
                .global
                .heap
                .table_get_shortstr(existing_mt, key_h)
                .unwrap_or(crate::contract::TValue::Nil);
            if !matches!(field, crate::contract::TValue::Nil) {
                let msg = state
                    .global
                    .new_string(b"cannot change a protected metatable", 0);
                state.raise_error_value(crate::contract::TValue::ShortString(msg));
                return 0;
            }
        }
    }
    // Input: t at 1, mt at 2. set_metatable pops top (mt) and
    // assigns it to t at `idx`. Push mt to top first.
    state.push_value(2);
    state.set_metatable(1);
    // If we just installed a metatable with __mode or __gc, flag
    // the target table so the GC can honor weak-reference or
    // finalization semantics on its next cycle.
    let t_val = state
        .value_at_public(1)
        .unwrap_or(crate::contract::TValue::Nil);
    if let crate::contract::TValue::Table(th) = t_val {
        let mt_opt = state.global.heap.table(th).metatable;
        let (weak, has_gc) = if let Some(mt) = mt_opt {
            let mode_k = state.global.new_string(b"__mode", 0);
            let gc_k = state.global.new_string(b"__gc", 0);
            let mode_v = state
                .global
                .heap
                .table_get_shortstr(mt, mode_k)
                .unwrap_or(crate::contract::TValue::Nil);
            let gc_v = state
                .global
                .heap
                .table_get_shortstr(mt, gc_k)
                .unwrap_or(crate::contract::TValue::Nil);
            let mut flags = 0u8;
            if let crate::contract::TValue::ShortString(h)
            | crate::contract::TValue::LongString(h) = mode_v
            {
                for b in &state.global.heap.string(h).bytes {
                    match *b {
                        b'k' => flags |= 0b01,
                        b'v' => flags |= 0b10,
                        _ => {}
                    }
                }
            }
            (flags, !matches!(gc_v, crate::contract::TValue::Nil))
        } else {
            (0, false)
        };
        let tbl = state.global.heap.table_mut(th);
        tbl.weak_mode = weak;
        tbl.has_finalizer = has_gc;
    }
    state.push_value(1);
    1
}

unsafe extern "C" fn lua_getmetatable(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if !state.get_metatable(1) {
        state.push_nil();
        return 1;
    }
    // Honour __metatable: if the metatable defines a __metatable
    // field, return THAT instead of the real metatable. This is
    // Lua's metatable-protection convention (so a sandbox can hide
    // its real metatable from untrusted code).
    let mt = state.value_at_public(-1);
    if let Some(crate::contract::TValue::Table(mt_h)) = mt {
        let key_h = state.global.new_string(b"__metatable", 0);
        if let Some(field) = state.global.heap.table_get_shortstr(mt_h, key_h) {
            if !matches!(field, crate::contract::TValue::Nil) {
                // Replace the metatable on the stack top with the
                // __metatable field's value: pop the metatable,
                // push the field.
                state.set_top(state.get_top() - 1);
                state.current_thread_mut().push(field);
                return 1;
            }
        }
    }
    1
}

// ---- select ---------------------------------------------------------------

unsafe extern "C" fn lua_select(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top();
    // select("#", ...) returns the count of extra args.
    if let Some(s) = state.to_lstring(1) {
        if s == b"#" {
            state.push_integer((top as i64) - 1);
            return 1;
        }
    }
    let n = state.to_integer_x(1).unwrap_or(0);
    let extra = top as i64 - 1;
    let start = if n < 0 {
        (extra + n + 1).max(1)
    } else {
        n
    };
    if start < 1 || start > extra {
        return 0;
    }
    for i in (start + 1)..=(extra + 1) {
        state.push_value(i as i32);
    }
    (extra - start + 1) as i32
}

// ---- ipairs / pairs / next ------------------------------------------------

unsafe extern "C" fn ipairs_aux(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // args: (t, i). Return (i+1, t[i+1]) while non-nil, else nil.
    let i = state.to_integer_x(2).unwrap_or(0);
    let next_i = i + 1;
    // Push next_i FIRST so it becomes the first return value.
    state.push_integer(next_i);
    // Then push t[next_i] as the second.
    state.raw_get_i(1, next_i);
    // If the value was nil, just return nil (discard next_i).
    // Signal end-of-sequence with a single nil.
    let top_idx = state.get_top() as i32;
    if state.type_at(top_idx) == 0 {
        // Reset: return just one nil. We need to remove next_i
        // and nil from the stack; push_nil after isn't needed
        // because the current top IS the nil value.
        // Actually, return count = 1 and top-1 value is nil.
        // We rely on the caller's finish_c_call to take the
        // last `n_returned` values. We pushed 2 but want 1.
        // Move the nil down over next_i.
        let base = state.frame_base_index().unwrap_or(0);
        // Replace the slot at base (where next_i sits) with nil.
        state
            .current_thread_mut()
            .stack[base as usize] = TValue::Nil;
        // Drop the top (the pushed nil).
        state.set_top(1);
        1
    } else {
        2
    }
}

unsafe extern "C" fn lua_ipairs(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Return (ipairs_aux, t, 0).
    state.push_light_cfunction(ipairs_aux);
    state.push_value(1);
    state.push_integer(0);
    3
}

unsafe extern "C" fn lua_next(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Argument 1 must be a table — raise a typed error instead of
    // panicking, otherwise hosts that call `next` with a function or
    // other non-table (e.g. fennel's `utils.hook` chain) crash the
    // whole interpreter instead of seeing a catchable Lua error.
    if !matches!(state.value_at_public(1), Some(crate::contract::TValue::Table(_))) {
        let msg = state
            .global
            .new_string(b"bad argument #1 to 'next' (table expected)", 0);
        state.raise_error_value(crate::contract::TValue::ShortString(msg));
        return 0;
    }
    if state.get_top() < 2 {
        state.push_nil();
    }
    if state.next_key(1) {
        2
    } else {
        state.push_nil();
        1
    }
}

unsafe extern "C" fn lua_pairs(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // Return (next, t, nil).
    state.push_light_cfunction(lua_next);
    state.push_value(1);
    state.push_nil();
    3
}

// ---- pcall / xpcall -------------------------------------------------------

unsafe extern "C" fn lua_pcall(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top();
    if top < 1 {
        state.push_boolean(false);
        let h = state.global.new_string(b"bad argument to pcall", 0);
        state.current_thread_mut().push(TValue::ShortString(h));
        return 2;
    }
    let n_args = (top as i32) - 1;
    // Compute the absolute func_slot for pcall: the function is
    // at lapi index 1, which maps to frame_base + 0.
    let frame_base = state.frame_base_index().unwrap_or(0);
    let func_abs = frame_base;
    let status = state.pcall(func_abs, n_args as u32, -1);
    match status {
        crate::contract::ThreadStatus::Ok => {
            // On success, stash the results, reset top to before
            // them, push true, then re-push the results.
            let frame_base_now = state.frame_base_index().unwrap_or(0);
            let top_abs = state.current_thread().top;
            let n_returns = top_abs.saturating_sub(frame_base_now) as i32;
            let saved: Vec<TValue> = (0..n_returns)
                .map(|i| state.current_thread().stack[(frame_base_now + i as u32) as usize])
                .collect();
            state.set_top(0);
            state.push_boolean(true);
            for v in &saved {
                state.current_thread_mut().push(*v);
            }
            (n_returns + 1) as i32
        }
        _ => {
            // On error, the error value sits at the func slot.
            // Replace with (false, error).
            let err_val = state.current_thread().stack[func_abs as usize];
            state.set_top(0);
            state.push_boolean(false);
            state.current_thread_mut().push(err_val);
            2
        }
    }
}

unsafe extern "C" fn lua_xpcall(state: *mut LuaState) -> std::os::raw::c_int {
    // xpcall(f, handler, ...) — real implementation runs the
    // handler on error. Minimal drop-in: if the first protected
    // call fails, invoke the handler with the error value as its
    // only argument and substitute its return value as the error.
    let state = unsafe { &mut *state };
    let top = state.get_top();
    if top < 2 {
        state.push_boolean(false);
        let h = state.global.new_string(b"bad argument to xpcall", 0);
        state.current_thread_mut().push(TValue::ShortString(h));
        return 2;
    }
    // Extract handler (at lapi index 2) out of the arg list.
    let frame_base = state.frame_base_index().unwrap_or(0);
    let handler = state.current_thread().stack[(frame_base + 1) as usize];
    // Shift args: move f to index 1 (already there), drop handler
    // at index 2 by compacting.
    let n_args = top as i32 - 2;
    let thread = state.current_thread_mut();
    for i in 1..=n_args {
        thread.stack[(frame_base + i as u32) as usize] =
            thread.stack[(frame_base + 1 + i as u32) as usize];
    }
    thread.top = frame_base + 1 + n_args as u32;
    let status = state.pcall(frame_base, n_args as u32, -1);
    match status {
        crate::contract::ThreadStatus::Ok => {
            let frame_base_now = state.frame_base_index().unwrap_or(0);
            let top_abs = state.current_thread().top;
            let n_returns = top_abs.saturating_sub(frame_base_now) as i32;
            let saved: Vec<TValue> = (0..n_returns)
                .map(|i| state.current_thread().stack[(frame_base_now + i as u32) as usize])
                .collect();
            state.set_top(0);
            state.push_boolean(true);
            for v in &saved {
                state.current_thread_mut().push(*v);
            }
            (n_returns + 1) as i32
        }
        _ => {
            // Call the handler with the error value.
            let err_val = state.current_thread().stack[frame_base as usize];
            state.set_top(0);
            let frame_base_now = state.frame_base_index().unwrap_or(0);
            state.current_thread_mut().push(handler);
            state.current_thread_mut().push(err_val);
            let _ = state.pcall(frame_base_now, 1, 1);
            // The handler's return value (or error) is now at
            // frame_base_now. Wrap in (false, result).
            let result = state.current_thread().stack[frame_base_now as usize];
            state.set_top(0);
            state.push_boolean(false);
            state.current_thread_mut().push(result);
            2
        }
    }
}

// ---- collectgarbage -------------------------------------------------------

unsafe extern "C" fn lua_collectgarbage(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let opt = state
        .to_lstring(1)
        .map(|s| s.to_vec())
        .unwrap_or_else(|| b"collect".to_vec());
    match opt.as_slice() {
        b"collect" => {
            // Run mark + sweep, then invoke any pending __gc
            // metamethods and drop weak-table entries whose
            // keys/values point to dead handles.
            state.global.full_gc();
            run_finalizers(state);
            drain_weak_tables(state);
            state.push_integer(0);
        }
        b"count" => {
            // Approximate: no accurate byte count yet.
            state.push_number(0.0);
        }
        b"stop" | b"restart" | b"step" | b"isrunning" => {
            state.push_integer(0);
        }
        _ => state.push_integer(0),
    }
    1
}

/// Invoke `__gc` on every table whose `has_finalizer` flag is set.
/// In a full GC implementation this would only run for objects
/// that just became unreachable; here we treat it as a best-effort
/// "run finalizers now" trigger so programs that rely on explicit
/// collectgarbage cycles behave sensibly.
fn run_finalizers(state: &mut LuaState) {
    use crate::contract::TValue;
    let candidates: Vec<crate::contract::TableHandle> = state
        .global
        .heap
        .all_table_handles()
        .into_iter()
        .filter(|h| state.global.heap.table(*h).has_finalizer)
        .collect();
    for h in candidates {
        // Clear the flag before invoking so we don't re-enter on
        // recursive collectgarbage calls from inside __gc itself.
        state.global.heap.table_mut(h).has_finalizer = false;
        let mm = state
            .global
            .get_metamethod(TValue::Table(h), crate::ltm::TagMethod::Gc);
        if matches!(mm, TValue::Nil) {
            continue;
        }
        let call_top = state.current_thread().top;
        {
            let thread = state.current_thread_mut();
            thread.push(mm);
            thread.push(TValue::Table(h));
        }
        let _ = state.call_value(call_top, 1, 0);
    }
}

/// Walk every live table with `weak_mode != 0` and remove entries
/// whose key (weak-k) or value (weak-v) is a handle pointing at a
/// freed slot. This runs after the GC's mark+sweep so freed slots
/// are already visible via the handle's generation-mismatch check.
///
/// Currently this drains entries whose value is a dead table/
/// userdata/thread/closure handle. Extending to weak keys requires
/// a second pass over the hash part — left for when the GC starts
/// reporting its freed-handles list.
fn drain_weak_tables(state: &mut LuaState) {
    let handles: Vec<crate::contract::TableHandle> = state
        .global
        .heap
        .all_table_handles()
        .into_iter()
        .filter(|h| state.global.heap.table(*h).weak_mode != 0)
        .collect();
    for h in handles {
        let (mode, array_dead): (u8, Vec<usize>) = {
            let t = state.global.heap.table(h);
            let mut dead = Vec::new();
            let mode = t.weak_mode;
            if mode & 0b10 != 0 {
                for (i, v) in t.array.iter().enumerate() {
                    if is_dead_tvalue(&state.global.heap, v) {
                        dead.push(i);
                    }
                }
            }
            (mode, dead)
        };
        {
            let t = state.global.heap.table_mut(h);
            for i in array_dead {
                t.array[i] = crate::contract::TValue::Nil;
            }
        }
        // Hash part drain: remove entries whose value (weak-v) or
        // key (weak-k) is a dead handle.
        let hash_dead: Vec<crate::contract::TableKey> = {
            let t = state.global.heap.table(h);
            t.hash
                .iter()
                .filter(|(k, v)| {
                    let val_dead = (mode & 0b10) != 0 && is_dead_tvalue(&state.global.heap, v);
                    let key_dead = (mode & 0b01) != 0 && is_dead_key(&state.global.heap, k);
                    val_dead || key_dead
                })
                .map(|(k, _)| (*k).clone())
                .collect()
        };
        let t = state.global.heap.table_mut(h);
        for k in hash_dead {
            t.hash.remove(&k);
        }
    }
}

fn is_dead_tvalue(heap: &crate::contract::Heap, v: &crate::contract::TValue) -> bool {
    use crate::contract::TValue;
    match v {
        TValue::Table(h) => !heap.table_handle_alive(*h),
        TValue::UserData(h) => !heap.userdata_handle_alive(*h),
        TValue::Thread(h) => !heap.thread_handle_alive(*h),
        TValue::LuaClosure(h) => !heap.lclosure_handle_alive(*h),
        TValue::CClosure(h) => !heap.cclosure_handle_alive(*h),
        _ => false,
    }
}

fn is_dead_key(heap: &crate::contract::Heap, k: &crate::contract::TableKey) -> bool {
    use crate::contract::TableKey;
    match k {
        TableKey::Table(h) => !heap.table_handle_alive(*h),
        TableKey::UserData(h) => !heap.userdata_handle_alive(*h),
        TableKey::Thread(h) => !heap.thread_handle_alive(*h),
        TableKey::LuaClosure(h) => !heap.lclosure_handle_alive(*h),
        TableKey::CClosure(h) => !heap.cclosure_handle_alive(*h),
        _ => false,
    }
}

// ---- Helpers --------------------------------------------------------------

fn tv_type_name(state: &LuaState, idx: i32) -> &'static str {
    let tt = state.type_at(idx);
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
    // Check type before coercing — floats and integers must format
    // differently (`1` vs `1.0`). to_integer_x would happily convert
    // a float with integer value and lose the decimal point.
    let tt = state.type_at(idx);
    if tt == 4 {
        if let Some(bytes) = state.to_lstring(idx) {
            return String::from_utf8_lossy(bytes).to_string();
        }
    }
    if tt == 3 {
        // Introspect the underlying TValue to distinguish integer/float.
        let v = {
            let base = state.frame_base_index().unwrap_or(0) as usize;
            let slot = (base as i32 + idx - 1).max(0) as usize;
            state.current_thread().stack.get(slot).copied().unwrap_or(crate::contract::TValue::Nil)
        };
        match v {
            crate::contract::TValue::Integer(i) => return i.to_string(),
            crate::contract::TValue::Number(f) => return format_lua_number(f),
            _ => {}
        }
    }
    match tt {
        0 => "nil".to_string(),
        1 => if state.to_boolean(idx) { "true" } else { "false" }.to_string(),
        5 => "table: ?".to_string(),
        6 => "function: ?".to_string(),
        _ => format!("{}: ?", tv_type_name(state, idx)),
    }
}

/// Format a Lua float using `%.14g` semantics, adding a `.0` suffix
/// when the output would otherwise look like an integer so the value
/// survives a tostring/tonumber round-trip with type preserved.
/// Format a Lua number using Lua 5.5's two-stage strategy: first
/// try `%.15g` (LUA_NUMBER_FMT). If `tonumber(tostring(n)) == n`
/// we use that — the shortest precise form. Otherwise fall back
/// to `%.17g` (LUA_NUMBER_FMT_N) to guarantee a round-trip.
/// Matches lobject.c::tostringbuffFloat byte-for-byte.
pub fn format_lua_number(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_string();
    }
    if f.is_infinite() {
        return if f < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0".to_string() } else { "0.0".to_string() };
    }
    let mut s = format_g(f, 15);
    // Round-trip test: parse the formatted string back. If it
    // matches bit-exact, we're done. Otherwise widen to %.17g.
    let needs_widen = match s.parse::<f64>() {
        Ok(parsed) => parsed.to_bits() != f.to_bits(),
        Err(_) => true,
    };
    if needs_widen {
        s = format_g(f, 17);
    }
    if !s.contains('.') && !s.contains('e') && !s.contains('E') && !s.contains('n') && !s.contains('i') {
        let mut with_dot = s;
        with_dot.push_str(".0");
        return with_dot;
    }
    s
}

/// `%.{prec}g` printf-style formatter for f64. Picks between %f
/// and %e based on the exponent; strips trailing zeros from the
/// mantissa in either case.
fn format_g(f: f64, prec: usize) -> String {
    if f == 0.0 {
        return "0".to_string();
    }
    let abs = f.abs();
    let exp = abs.log10().floor() as i32;
    if exp < -4 || exp >= prec as i32 {
        // Scientific.
        let raw = format!("{:.*e}", prec - 1, f);
        normalize_exp_e(&raw)
    } else {
        // Fixed.
        let f_prec = (prec as i32 - 1 - exp).max(0) as usize;
        let raw = format!("{:.*}", f_prec, f);
        strip_trailing_zeros(raw)
    }
}

fn strip_trailing_zeros(mut s: String) -> String {
    if s.contains('.') {
        while s.ends_with('0') { s.pop(); }
        if s.ends_with('.') { s.pop(); }
    }
    s
}

fn normalize_exp_e(raw: &str) -> String {
    // Rust's {:e} gives "1.234e5" (no sign, no zero-pad). C %g gives
    // "1.234e+05". Convert.
    let Some(idx) = raw.find(|c| c == 'e' || c == 'E') else {
        return raw.to_string();
    };
    let (mantissa, rest) = raw.split_at(idx);
    let exp = &rest[1..];
    let (sign, digits) = if let Some(d) = exp.strip_prefix('-') {
        ("-", d)
    } else if let Some(d) = exp.strip_prefix('+') {
        ("+", d)
    } else {
        ("+", exp)
    };
    let mant_trim = strip_trailing_zeros(mantissa.to_string());
    let pad: String = if digits.len() < 2 {
        std::iter::repeat('0').take(2 - digits.len()).collect()
    } else {
        String::new()
    };
    format!("{}e{}{}{}", mant_trim, sign, pad, digits)
}

// ---- Tests ----------------------------------------------------------------

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
    fn open_base_registers_g_self_reference() {
        let mut state = LuaState::new(0);
        let globals = open_base(&mut state);
        let g_name = state.global.new_string(b"_G", 0);
        let v = state.global.heap.table_get_shortstr(globals, g_name);
        assert_eq!(v, Some(TValue::Table(globals)));
    }

    #[test]
    fn open_base_registers_version() {
        let mut state = LuaState::new(0);
        let globals = open_base(&mut state);
        let vk = state.global.new_string(b"_VERSION", 0);
        let v = state.global.heap.table_get_shortstr(globals, vk);
        assert!(matches!(v, Some(TValue::ShortString(_))));
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

    #[test]
    fn tostring_string_roundtrip() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(lua_tostring);
            state.push_string("hello");
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"hello".as_slice()));
        }
    }
}
