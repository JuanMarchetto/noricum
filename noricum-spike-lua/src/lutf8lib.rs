//! lutf8lib — the Lua `utf8` library.
//!
//! Port of `lutf8lib.c`. UTF-8 encode/decode helpers.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};

pub fn open_utf8(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, t, "char", utf8_char);
    register(state, t, "codepoint", utf8_codepoint);
    register(state, t, "len", utf8_len);
    register(state, t, "offset", utf8_offset);
    register(state, t, "codes", utf8_codes);

    // utf8.charpattern: matches a single UTF-8 sequence.
    let cp_key = state.global.new_string(b"charpattern", 0);
    let cp_val = state
        .global
        .new_string(b"[\x00-\x7F\xC2-\xFD][\x80-\xBF]*", 0);
    state
        .global
        .table_set_shortstr(t, cp_key, TValue::ShortString(cp_val));

    let name = state.global.new_string(b"utf8", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

fn encode_codepoint(cp: u32, out: &mut Vec<u8>) {
    if cp < 0x80 {
        out.push(cp as u8);
    } else if cp < 0x800 {
        out.push(0xC0 | (cp >> 6) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    } else if cp < 0x10000 {
        out.push(0xE0 | (cp >> 12) as u8);
        out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    } else {
        out.push(0xF0 | (cp >> 18) as u8);
        out.push(0x80 | ((cp >> 12) & 0x3F) as u8);
        out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    }
}

unsafe extern "C" fn utf8_char(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    let mut out = Vec::new();
    for i in 1..=top {
        let cp = state.to_integer_x(i).unwrap_or(0) as u32;
        encode_codepoint(cp, &mut out);
    }
    state.push_string(&String::from_utf8_lossy(&out));
    1
}

unsafe extern "C" fn utf8_codepoint(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let i = state.to_integer_x(2).unwrap_or(1) as usize;
    let j = state.to_integer_x(3).unwrap_or(i as i64) as usize;
    if i == 0 || i > bytes.len() {
        return 0;
    }
    let mut pos = i - 1;
    let end = j.min(bytes.len());
    let mut count = 0;
    while pos < end {
        match decode_codepoint(&bytes, pos) {
            Some((cp, len)) => {
                state.push_integer(cp as i64);
                count += 1;
                pos += len;
            }
            None => break,
        }
    }
    count as i32
}

fn decode_codepoint(bytes: &[u8], pos: usize) -> Option<(u32, usize)> {
    if pos >= bytes.len() {
        return None;
    }
    let b0 = bytes[pos];
    if b0 < 0x80 {
        return Some((b0 as u32, 1));
    }
    let (len, mask) = if b0 & 0xE0 == 0xC0 {
        (2, 0x1F)
    } else if b0 & 0xF0 == 0xE0 {
        (3, 0x0F)
    } else if b0 & 0xF8 == 0xF0 {
        (4, 0x07)
    } else {
        return None;
    };
    if pos + len > bytes.len() {
        return None;
    }
    let mut cp = (b0 & mask) as u32;
    for i in 1..len {
        let b = bytes[pos + i];
        if b & 0xC0 != 0x80 {
            return None;
        }
        cp = (cp << 6) | (b & 0x3F) as u32;
    }
    Some((cp, len))
}

unsafe extern "C" fn utf8_len(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let i = state.to_integer_x(2).unwrap_or(1) as usize;
    let j = state.to_integer_x(3).unwrap_or(bytes.len() as i64) as usize;
    let start = if i == 0 { 0 } else { i - 1 };
    let end = j.min(bytes.len());
    let mut pos = start;
    let mut count: i64 = 0;
    while pos < end {
        match decode_codepoint(&bytes, pos) {
            Some((_, len)) => {
                pos += len;
                count += 1;
            }
            None => {
                state.push_nil();
                state.push_integer((pos + 1) as i64);
                return 2;
            }
        }
    }
    state.push_integer(count);
    1
}

unsafe extern "C" fn utf8_offset(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let n = state.to_integer_x(2).unwrap_or(0);
    let i = state.to_integer_x(3).unwrap_or(if n >= 0 { 1 } else { bytes.len() as i64 + 1 });
    let mut pos = (i - 1) as isize;
    // C Lua's `utf8.offset` returns nil when n cannot be reached in
    // either direction (instead of clamping). Track exhaustion and
    // push nil if the requested character wasn't found.
    if n > 0 {
        let mut remaining = n - 1;
        while remaining > 0 && (pos as usize) < bytes.len() {
            if let Some((_, len)) = decode_codepoint(&bytes, pos as usize) {
                pos += len as isize;
                remaining -= 1;
            } else {
                break;
            }
        }
        if remaining > 0 {
            state.push_nil();
            return 1;
        }
    } else if n < 0 {
        let mut remaining = -n;
        while remaining > 0 && pos > 0 {
            pos -= 1;
            while pos > 0 && (bytes[pos as usize] & 0xC0) == 0x80 {
                pos -= 1;
            }
            remaining -= 1;
        }
        if remaining > 0 {
            state.push_nil();
            return 1;
        }
    }
    state.push_integer((pos + 1) as i64);
    1
}

unsafe extern "C" fn utf8_codes(state: *mut LuaState) -> std::os::raw::c_int {
    // Returns iterator, string, 0. Iterator advances one UTF-8
    // sequence per call.
    let state = unsafe { &mut *state };
    state.push_light_cfunction(utf8_codes_iter);
    state.push_value(1);
    state.push_integer(0);
    3
}

unsafe extern "C" fn utf8_codes_iter(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let pos = state.to_integer_x(2).unwrap_or(0) as usize;
    let next_pos = if pos == 0 {
        0
    } else {
        match decode_codepoint(&bytes, pos - 1) {
            Some((_, len)) => pos - 1 + len,
            None => return 0,
        }
    };
    if next_pos >= bytes.len() {
        return 0;
    }
    match decode_codepoint(&bytes, next_pos) {
        Some((cp, _)) => {
            state.push_integer((next_pos + 1) as i64);
            state.push_integer(cp as i64);
            2
        }
        None => 0,
    }
}
