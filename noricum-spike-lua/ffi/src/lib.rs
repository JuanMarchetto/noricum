//! noricum-lua-ffi — drop-in `extern "C"` surface for the safe-Rust
//! Lua runtime in `noricum-spike-lua`.
//!
//! Everything in this crate is a thin trampoline: it takes the
//! C-ABI arguments (`*mut lua_State`, indices, etc.), casts the
//! opaque pointer back into our Rust `LuaState`, and dispatches
//! into the safe Rust API. The only `unsafe` lives here — the
//! 24k+ LOC of runtime beneath it stays pure safe Rust.
//!
//! Safety contract for every function in this file: `L` must be a
//! pointer previously returned by `lua_newstate` / `luaL_newstate`
//! (or clone derived from one), not yet passed to `lua_close`.
//! Same contract as the original C Lua.

#![allow(non_camel_case_types, non_snake_case)]

use spike::contract::{LuaState, TValue, ThreadStatus};
use std::ffi::{c_char, c_int, c_void, CStr};
use std::os::raw::{c_double, c_uint};

// ---- Types -------------------------------------------------------

pub type lua_Integer = i64;
pub type lua_Number = c_double;
pub type lua_Unsigned = u64;
pub type lua_KContext = isize;

/// C-ABI opaque handle. The real type is always a `*mut LuaState`
/// (our safe-Rust state), heap-allocated with `Box` so the
/// address is stable across calls.
#[repr(C)]
pub struct lua_State {
    _opaque: [u8; 0],
}

pub type lua_CFunction = Option<unsafe extern "C" fn(L: *mut lua_State) -> c_int>;
pub type lua_Alloc = Option<
    unsafe extern "C" fn(
        ud: *mut c_void,
        ptr: *mut c_void,
        osize: usize,
        nsize: usize,
    ) -> *mut c_void,
>;
pub type lua_KFunction =
    Option<unsafe extern "C" fn(L: *mut lua_State, status: c_int, ctx: lua_KContext) -> c_int>;
pub type lua_Reader = Option<
    unsafe extern "C" fn(L: *mut lua_State, ud: *mut c_void, sz: *mut usize) -> *const c_char,
>;

// ---- Constants ---------------------------------------------------

pub const LUA_MULTRET: c_int = -1;
pub const LUA_REGISTRYINDEX: c_int = -(i32::MAX / 2 + 1000);

pub const LUA_OK: c_int = 0;
pub const LUA_YIELD: c_int = 1;
pub const LUA_ERRRUN: c_int = 2;
pub const LUA_ERRSYNTAX: c_int = 3;
pub const LUA_ERRMEM: c_int = 4;
pub const LUA_ERRERR: c_int = 5;

pub const LUA_TNONE: c_int = -1;
pub const LUA_TNIL: c_int = 0;
pub const LUA_TBOOLEAN: c_int = 1;
pub const LUA_TLIGHTUSERDATA: c_int = 2;
pub const LUA_TNUMBER: c_int = 3;
pub const LUA_TSTRING: c_int = 4;
pub const LUA_TTABLE: c_int = 5;
pub const LUA_TFUNCTION: c_int = 6;
pub const LUA_TUSERDATA: c_int = 7;
pub const LUA_TTHREAD: c_int = 8;

// Arithmetic/comparison ops.
pub const LUA_OPEQ: c_int = 0;
pub const LUA_OPLT: c_int = 1;
pub const LUA_OPLE: c_int = 2;

// ---- State management --------------------------------------------

#[inline]
unsafe fn state<'a>(L: *mut lua_State) -> &'a mut LuaState {
    unsafe { &mut *(L as *mut LuaState) }
}

/// `luaL_newstate()` — allocate a fresh state. We ignore the
/// custom allocator (Lua's `lua_Alloc`) because our runtime uses
/// Rust's global allocator; a future iteration can wire it up.
#[unsafe(no_mangle)]
pub extern "C" fn luaL_newstate() -> *mut lua_State {
    let boxed = Box::new(LuaState::new(0));
    Box::into_raw(boxed) as *mut lua_State
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_newstate(
    _f: lua_Alloc,
    _ud: *mut c_void,
    _seed: c_uint,
) -> *mut lua_State {
    luaL_newstate()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_close(L: *mut lua_State) {
    if !L.is_null() {
        // Reconstruct the Box so Drop runs for the LuaState, freeing
        // its heap (strings, tables, threads, GC metadata, etc.).
        drop(unsafe { Box::from_raw(L as *mut LuaState) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_openlibs(L: *mut lua_State) {
    let s = unsafe { state(L) };
    let _ = spike::linit::open_libs(s);
}

// ---- Stack manipulation ------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_gettop(L: *mut lua_State) -> c_int {
    let s = unsafe { state(L) };
    s.get_top() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_settop(L: *mut lua_State, idx: c_int) {
    let s = unsafe { state(L) };
    s.set_top(idx);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushvalue(L: *mut lua_State, idx: c_int) {
    let s = unsafe { state(L) };
    s.push_value(idx);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_absindex(L: *mut lua_State, idx: c_int) -> c_int {
    if idx > 0 || idx <= LUA_REGISTRYINDEX {
        return idx;
    }
    let s = unsafe { state(L) };
    (s.get_top() as c_int) + idx + 1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_checkstack(_L: *mut lua_State, _n: c_int) -> c_int {
    // Our stack grows dynamically, so any request succeeds.
    1
}

// ---- Type inspection ---------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_type(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    let t = s.type_at(idx);
    if t < 0 {
        LUA_TNONE
    } else {
        t as c_int
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_typename(_L: *mut lua_State, tp: c_int) -> *const c_char {
    let name: &'static [u8] = match tp {
        LUA_TNIL => b"nil\0",
        LUA_TBOOLEAN => b"boolean\0",
        LUA_TLIGHTUSERDATA => b"userdata\0",
        LUA_TNUMBER => b"number\0",
        LUA_TSTRING => b"string\0",
        LUA_TTABLE => b"table\0",
        LUA_TFUNCTION => b"function\0",
        LUA_TUSERDATA => b"userdata\0",
        LUA_TTHREAD => b"thread\0",
        _ => b"no value\0",
    };
    name.as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_isnumber(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    s.to_number_x(idx).is_some() as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_isinteger(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    (s.type_at(idx) == LUA_TNUMBER && s.to_integer_x(idx).is_some()) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_isstring(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    let t = s.type_at(idx);
    ((t == LUA_TSTRING) || (t == LUA_TNUMBER)) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_iscfunction(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    matches!(
        s.value_at_public(idx),
        Some(TValue::LightCFunction(_)) | Some(TValue::CClosure(_))
    ) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_isuserdata(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    matches!(
        s.value_at_public(idx),
        Some(TValue::UserData(_)) | Some(TValue::LightUserData(_))
    ) as c_int
}

// ---- Value extraction --------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_tointegerx(
    L: *mut lua_State,
    idx: c_int,
    isnum: *mut c_int,
) -> lua_Integer {
    let s = unsafe { state(L) };
    match s.to_integer_x(idx) {
        Some(n) => {
            if !isnum.is_null() {
                unsafe { *isnum = 1 };
            }
            n
        }
        None => {
            if !isnum.is_null() {
                unsafe { *isnum = 0 };
            }
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_tonumberx(
    L: *mut lua_State,
    idx: c_int,
    isnum: *mut c_int,
) -> lua_Number {
    let s = unsafe { state(L) };
    match s.to_number_x(idx) {
        Some(n) => {
            if !isnum.is_null() {
                unsafe { *isnum = 1 };
            }
            n
        }
        None => {
            if !isnum.is_null() {
                unsafe { *isnum = 0 };
            }
            0.0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_toboolean(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    s.to_boolean(idx) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_tolstring(
    L: *mut lua_State,
    idx: c_int,
    len: *mut usize,
) -> *const c_char {
    let s = unsafe { state(L) };
    match s.to_lstring(idx) {
        Some(bytes) => {
            if !len.is_null() {
                unsafe { *len = bytes.len() };
            }
            // Bytes live inside an interned LuaString on the heap.
            // Lua's contract says the pointer remains valid until the
            // value is removed from the stack; we satisfy that by
            // returning the heap storage directly.
            bytes.as_ptr() as *const c_char
        }
        None => {
            // C Lua coerces numbers to strings here; we fall back to
            // pushing nothing and returning null for any other type.
            if !len.is_null() {
                unsafe { *len = 0 };
            }
            std::ptr::null()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_rawlen(L: *mut lua_State, idx: c_int) -> lua_Unsigned {
    let s = unsafe { state(L) };
    s.raw_len(idx)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_tocfunction(L: *mut lua_State, idx: c_int) -> lua_CFunction {
    let s = unsafe { state(L) };
    match s.value_at_public(idx) {
        Some(TValue::LightCFunction(f)) => {
            // Type alias cast: spike's RawCFunction has the same ABI.
            let ptr: unsafe extern "C" fn(*mut LuaState) -> c_int = f;
            Some(unsafe { std::mem::transmute(ptr) })
        }
        _ => None,
    }
}

// ---- Push ---------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushnil(L: *mut lua_State) {
    let s = unsafe { state(L) };
    s.push_nil();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushboolean(L: *mut lua_State, b: c_int) {
    let s = unsafe { state(L) };
    s.push_boolean(b != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushinteger(L: *mut lua_State, n: lua_Integer) {
    let s = unsafe { state(L) };
    s.push_integer(n);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushnumber(L: *mut lua_State, n: lua_Number) {
    let s = unsafe { state(L) };
    s.push_number(n);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushlstring(
    L: *mut lua_State,
    s_ptr: *const c_char,
    len: usize,
) -> *const c_char {
    let s = unsafe { state(L) };
    let bytes = if s_ptr.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(s_ptr as *const u8, len) }
    };
    let h = s.push_lstring(bytes);
    s.global.heap.string(h).bytes.as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushstring(
    L: *mut lua_State,
    s_ptr: *const c_char,
) -> *const c_char {
    if s_ptr.is_null() {
        let s = unsafe { state(L) };
        s.push_nil();
        return std::ptr::null();
    }
    let cstr = unsafe { CStr::from_ptr(s_ptr) };
    unsafe { lua_pushlstring(L, s_ptr, cstr.to_bytes().len()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushcfunction(L: *mut lua_State, f: lua_CFunction) {
    let s = unsafe { state(L) };
    if let Some(f) = f {
        // Both sides use the same `*mut LuaState` ABI modulo the
        // opaque lua_State newtype: transmute the fn pointer.
        let raw: spike::contract::RawCFunction = unsafe { std::mem::transmute(f) };
        s.current_thread_mut().push(TValue::LightCFunction(raw));
    } else {
        s.push_nil();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushcclosure(L: *mut lua_State, f: lua_CFunction, n: c_int) {
    let s = unsafe { state(L) };
    if let Some(f) = f {
        let raw: spike::contract::RawCFunction = unsafe { std::mem::transmute(f) };
        // Pop `n` upvalues from the top of the stack, bundle them
        // with `raw` into a CClosure, push the closure.
        let top = s.get_top() as usize;
        let n_up = n.max(0) as usize;
        let mut upvalues = Vec::with_capacity(n_up);
        for i in 0..n_up {
            let v = s
                .value_at_public((top - n_up + i + 1) as c_int)
                .unwrap_or(TValue::Nil);
            upvalues.push(v);
        }
        // Pop the upvalues.
        s.set_top((top - n_up) as c_int);
        let cch = s
            .global
            .heap
            .alloc_cclosure(spike::contract::CClosure { f: raw, upvalues });
        s.current_thread_mut().push(TValue::CClosure(cch));
    } else {
        s.push_nil();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pushlightuserdata(L: *mut lua_State, p: *mut c_void) {
    let s = unsafe { state(L) };
    s.current_thread_mut().push(TValue::LightUserData(p));
}

// ---- Tables -------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_createtable(L: *mut lua_State, narr: c_int, nrec: c_int) {
    let s = unsafe { state(L) };
    s.create_table(narr.max(0) as usize, nrec.max(0) as usize);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_rawget(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    s.raw_get(idx);
    unsafe { lua_type(L, -1) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_rawset(L: *mut lua_State, idx: c_int) {
    let s = unsafe { state(L) };
    s.raw_set(idx);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_rawgeti(L: *mut lua_State, idx: c_int, n: lua_Integer) -> c_int {
    let s = unsafe { state(L) };
    s.raw_get_i(idx, n);
    unsafe { lua_type(L, -1) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_rawseti(L: *mut lua_State, idx: c_int, n: lua_Integer) {
    let s = unsafe { state(L) };
    s.raw_set_i(idx, n);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_gettable(L: *mut lua_State, idx: c_int) -> c_int {
    // Metamethod-aware `t[k]` — we don't yet expose the full walker
    // via the public API, so fall back to rawget for now.
    unsafe { lua_rawget(L, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_settable(L: *mut lua_State, idx: c_int) {
    unsafe { lua_rawset(L, idx) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_getfield(
    L: *mut lua_State,
    idx: c_int,
    k: *const c_char,
) -> c_int {
    let s = unsafe { state(L) };
    if k.is_null() {
        s.push_nil();
        return LUA_TNIL;
    }
    let bytes = unsafe { CStr::from_ptr(k) }.to_bytes();
    let key = std::str::from_utf8(bytes).unwrap_or("");
    s.raw_get_field(idx, key);
    unsafe { lua_type(L, -1) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_setfield(L: *mut lua_State, idx: c_int, k: *const c_char) {
    let s = unsafe { state(L) };
    if k.is_null() {
        return;
    }
    let bytes = unsafe { CStr::from_ptr(k) }.to_bytes();
    let key = std::str::from_utf8(bytes).unwrap_or("");
    s.raw_set_field(idx, key);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_geti(L: *mut lua_State, idx: c_int, n: lua_Integer) -> c_int {
    unsafe { lua_rawgeti(L, idx, n) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_seti(L: *mut lua_State, idx: c_int, n: lua_Integer) {
    unsafe { lua_rawseti(L, idx, n) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_next(L: *mut lua_State, idx: c_int) -> c_int {
    // Minimal `next`: look up the table, iterate its hash+array,
    // find the entry strictly after the key at stack top, then
    // push (key, value). Returns 0 when we walked off the end.
    let s = unsafe { state(L) };
    let t_val = s.value_at_public(idx).unwrap_or(TValue::Nil);
    let TValue::Table(th) = t_val else {
        return 0;
    };
    let prev_key = s.value_at_public(-1).unwrap_or(TValue::Nil);
    // Pop the previous key.
    let new_top = s.get_top() - 1;
    s.set_top(new_top as c_int);
    let (next_k, next_v) = {
        let table = s.global.heap.table(th);
        let mut pairs: Vec<(TValue, TValue)> = Vec::new();
        for (i, v) in table.array.iter().enumerate() {
            if !matches!(v, TValue::Nil) {
                pairs.push((TValue::Integer((i + 1) as i64), *v));
            }
        }
        for (k, v) in table.hash.iter() {
            pairs.push((key_to_tvalue(k), *v));
        }
        // Find the position of prev_key.
        let start = if matches!(prev_key, TValue::Nil) {
            0
        } else {
            pairs.iter().position(|(k, _)| *k == prev_key).map(|p| p + 1).unwrap_or(pairs.len())
        };
        if start >= pairs.len() {
            (TValue::Nil, TValue::Nil)
        } else {
            pairs[start]
        }
    };
    if matches!(next_k, TValue::Nil) {
        0
    } else {
        s.current_thread_mut().push(next_k);
        s.current_thread_mut().push(next_v);
        1
    }
}

fn key_to_tvalue(k: &spike::contract::TableKey) -> TValue {
    use spike::contract::TableKey;
    match k {
        TableKey::False => TValue::False,
        TableKey::True => TValue::True,
        TableKey::Integer(i) => TValue::Integer(*i),
        TableKey::Number(n) => TValue::Number(f64::from_bits(*n)),
        TableKey::ShortString(h) => TValue::ShortString(*h),
        TableKey::LongString(h) => TValue::LongString(*h),
        TableKey::Table(h) => TValue::Table(*h),
        TableKey::LuaClosure(h) => TValue::LuaClosure(*h),
        TableKey::LightCFunction(p) => TValue::LightCFunction(unsafe {
            std::mem::transmute::<usize, spike::contract::RawCFunction>(*p)
        }),
        TableKey::CClosure(h) => TValue::CClosure(*h),
        TableKey::UserData(h) => TValue::UserData(*h),
        TableKey::Thread(h) => TValue::Thread(*h),
        TableKey::LightUserData(p) => TValue::LightUserData(*p as *mut c_void),
    }
}

// ---- Globals ------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_getglobal(L: *mut lua_State, name: *const c_char) -> c_int {
    let s = unsafe { state(L) };
    if name.is_null() {
        s.push_nil();
        return LUA_TNIL;
    }
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    let Some(g) = s.global.globals_table() else {
        s.push_nil();
        return LUA_TNIL;
    };
    let key = s.global.new_string(bytes, 0);
    let v = s
        .global
        .heap
        .table_get_shortstr(g, key)
        .unwrap_or(TValue::Nil);
    s.current_thread_mut().push(v);
    unsafe { lua_type(L, -1) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_setglobal(L: *mut lua_State, name: *const c_char) {
    let s = unsafe { state(L) };
    if name.is_null() {
        return;
    }
    let bytes = unsafe { CStr::from_ptr(name) }.to_bytes();
    let Some(g) = s.global.globals_table() else {
        return;
    };
    let key = s.global.new_string(bytes, 0);
    let v = s.value_at_public(-1).unwrap_or(TValue::Nil);
    s.global.table_set_shortstr(g, key, v);
    let new_top = s.get_top() - 1;
    s.set_top(new_top as c_int);
}

// ---- Function calls ----------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_callk(
    L: *mut lua_State,
    nargs: c_int,
    nresults: c_int,
    _ctx: lua_KContext,
    _k: lua_KFunction,
) {
    let s = unsafe { state(L) };
    let top = s.get_top() as u32;
    let func_slot = top - (nargs as u32) - 1;
    match s.call_value(func_slot, nargs as u32, nresults as i16) {
        Ok(()) => {}
        Err(e) => {
            // Stash the error and let the next pcall catch it.
            let v = match e {
                spike::contract::LuaError::Runtime(v) => v,
                _ => TValue::Nil,
            };
            s.raise_error_value(v);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_call(L: *mut lua_State, nargs: c_int, nresults: c_int) {
    unsafe { lua_callk(L, nargs, nresults, 0, None) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pcallk(
    L: *mut lua_State,
    nargs: c_int,
    nresults: c_int,
    _errfunc: c_int,
    _ctx: lua_KContext,
    _k: lua_KFunction,
) -> c_int {
    let s = unsafe { state(L) };
    let top = s.get_top() as u32;
    let func_slot = top - (nargs as u32) - 1;
    match s.pcall(func_slot, nargs as u32, nresults as i16) {
        ThreadStatus::Ok => LUA_OK,
        ThreadStatus::Yield => LUA_YIELD,
        ThreadStatus::RuntimeError => LUA_ERRRUN,
        ThreadStatus::SyntaxError => LUA_ERRSYNTAX,
        ThreadStatus::MemoryError => LUA_ERRMEM,
        _ => LUA_ERRRUN,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_pcall(
    L: *mut lua_State,
    nargs: c_int,
    nresults: c_int,
    errfunc: c_int,
) -> c_int {
    unsafe { lua_pcallk(L, nargs, nresults, errfunc, 0, None) }
}

// ---- Load / execute strings --------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_loadstring(
    L: *mut lua_State,
    s_ptr: *const c_char,
) -> c_int {
    if s_ptr.is_null() {
        return LUA_ERRSYNTAX;
    }
    let cstr = unsafe { CStr::from_ptr(s_ptr) };
    unsafe { luaL_loadbufferx(L, s_ptr, cstr.to_bytes().len(), s_ptr, std::ptr::null()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_loadbufferx(
    L: *mut lua_State,
    buf: *const c_char,
    sz: usize,
    name: *const c_char,
    _mode: *const c_char,
) -> c_int {
    let s = unsafe { state(L) };
    let bytes = if buf.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(buf as *const u8, sz) }
    };
    let chunkname: Vec<u8> = if name.is_null() {
        b"=(load)".to_vec()
    } else {
        unsafe { CStr::from_ptr(name) }.to_bytes().to_vec()
    };
    // Run the parser under catch_unwind since it panics on syntax
    // errors (pre-Result-threading). Silence the default panic hook
    // so the backtrace doesn't leak to stderr.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Prefer the globals table held by the main thread's _ENV
        // upvalue if we can find it; otherwise use the registry's
        // globals slot.
        let globals = s
            .global
            .globals_table()
            .expect("globals table not yet initialized — call luaL_openlibs first");
        spike::lparser::parse_with_env(s, bytes, &chunkname, globals)
    }));
    std::panic::set_hook(prev_hook);
    match result {
        Ok(cl) => {
            s.current_thread_mut().push(TValue::LuaClosure(cl));
            LUA_OK
        }
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&'static str>() {
                s.to_string()
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "load: parse error".to_string()
            };
            s.push_string(&msg);
            LUA_ERRSYNTAX
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_loadfilex(
    L: *mut lua_State,
    filename: *const c_char,
    _mode: *const c_char,
) -> c_int {
    let s = unsafe { state(L) };
    if filename.is_null() {
        s.push_string("luaL_loadfilex: null filename");
        return LUA_ERRSYNTAX;
    }
    let path = unsafe { CStr::from_ptr(filename) }.to_bytes();
    let p = String::from_utf8_lossy(path).to_string();
    match std::fs::read(&p) {
        Ok(bytes) => {
            let mut name = String::from("@");
            name.push_str(&p);
            let name_c = std::ffi::CString::new(name).unwrap_or_default();
            unsafe {
                luaL_loadbufferx(
                    L,
                    bytes.as_ptr() as *const c_char,
                    bytes.len(),
                    name_c.as_ptr(),
                    std::ptr::null(),
                )
            }
        }
        Err(e) => {
            let s = unsafe { state(L) };
            s.push_string(&format!("cannot open {}: {}", p, e));
            LUA_ERRRUN
        }
    }
}

// ---- Comparison / misc -------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_rawequal(L: *mut lua_State, idx1: c_int, idx2: c_int) -> c_int {
    let s = unsafe { state(L) };
    let v1 = s.value_at_public(idx1);
    let v2 = s.value_at_public(idx2);
    (v1 == v2) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_compare(
    L: *mut lua_State,
    idx1: c_int,
    idx2: c_int,
    op: c_int,
) -> c_int {
    let s = unsafe { state(L) };
    let a = s.value_at_public(idx1).unwrap_or(TValue::Nil);
    let b = s.value_at_public(idx2).unwrap_or(TValue::Nil);
    match op {
        LUA_OPEQ => (a == b) as c_int,
        LUA_OPLT | LUA_OPLE => {
            let cmp = match (a, b) {
                (TValue::Integer(x), TValue::Integer(y)) => {
                    if op == LUA_OPLT { x < y } else { x <= y }
                }
                (TValue::Number(x), TValue::Number(y)) => {
                    if op == LUA_OPLT { x < y } else { x <= y }
                }
                (TValue::Integer(x), TValue::Number(y)) => {
                    let xf = x as f64;
                    if op == LUA_OPLT { xf < y } else { xf <= y }
                }
                (TValue::Number(x), TValue::Integer(y)) => {
                    let yf = y as f64;
                    if op == LUA_OPLT { x < yf } else { x <= yf }
                }
                _ => false,
            };
            cmp as c_int
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_error(L: *mut lua_State) -> c_int {
    let s = unsafe { state(L) };
    let v = s.value_at_public(-1).unwrap_or(TValue::Nil);
    s.raise_error_value(v);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_version(_L: *mut lua_State) -> lua_Number {
    504.0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_atpanic(_L: *mut lua_State, _f: lua_CFunction) -> lua_CFunction {
    // Not currently supported — panic handlers aren't wired.
    None
}

// ---- Metatables --------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_getmetatable(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    s.get_metatable(idx) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lua_setmetatable(L: *mut lua_State, idx: c_int) -> c_int {
    let s = unsafe { state(L) };
    s.set_metatable(idx);
    1
}

// ---- luaL_* auxiliary helpers ------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_checkinteger(L: *mut lua_State, arg: c_int) -> lua_Integer {
    let s = unsafe { state(L) };
    match s.to_integer_x(arg) {
        Some(n) => n,
        None => {
            s.push_string(&format!("bad argument #{}: integer expected", arg));
            s.raise_error_value(
                s.value_at_public(-1).unwrap_or(TValue::Nil),
            );
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_checknumber(L: *mut lua_State, arg: c_int) -> lua_Number {
    let s = unsafe { state(L) };
    match s.to_number_x(arg) {
        Some(n) => n,
        None => {
            s.push_string(&format!("bad argument #{}: number expected", arg));
            s.raise_error_value(
                s.value_at_public(-1).unwrap_or(TValue::Nil),
            );
            0.0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_checklstring(
    L: *mut lua_State,
    arg: c_int,
    len: *mut usize,
) -> *const c_char {
    let p = unsafe { lua_tolstring(L, arg, len) };
    if p.is_null() {
        let s = unsafe { state(L) };
        s.push_string(&format!("bad argument #{}: string expected", arg));
        s.raise_error_value(s.value_at_public(-1).unwrap_or(TValue::Nil));
    }
    p
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_checkstring(L: *mut lua_State, arg: c_int) -> *const c_char {
    unsafe { luaL_checklstring(L, arg, std::ptr::null_mut()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_optinteger(
    L: *mut lua_State,
    arg: c_int,
    def: lua_Integer,
) -> lua_Integer {
    let s = unsafe { state(L) };
    if s.type_at(arg) <= 0 {
        def
    } else {
        s.to_integer_x(arg).unwrap_or(def)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn luaL_optnumber(
    L: *mut lua_State,
    arg: c_int,
    def: lua_Number,
) -> lua_Number {
    let s = unsafe { state(L) };
    if s.type_at(arg) <= 0 {
        def
    } else {
        s.to_number_x(arg).unwrap_or(def)
    }
}
