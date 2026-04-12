//! ltablib — the Lua table library.
//!
//! Port of `ltablib.c`. Registers `table.*` functions.

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};

pub fn open_table(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, t, "insert", tab_insert);
    register(state, t, "remove", tab_remove);
    register(state, t, "concat", tab_concat);
    register(state, t, "unpack", tab_unpack);
    register(state, t, "pack", tab_pack);
    register(state, t, "sort", tab_sort);
    register(state, t, "move", tab_move);

    let name = state.global.new_string(b"table", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(t));
    t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

unsafe extern "C" fn tab_insert(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let table = match get_tv(state, 1) {
        TValue::Table(h) => h,
        _ => return err(state, b"bad argument 1 to insert"),
    };
    let top = state.get_top();
    let len = state.global.heap.table_len(table) as i64;
    match top {
        2 => {
            // table.insert(t, v): append at t[#t + 1]
            let v = get_tv(state, 2);
            state.global.table_set_int(table, len + 1, v);
        }
        3 => {
            // table.insert(t, pos, v): shift elements up.
            let pos = state.to_integer_x(2).unwrap_or(1);
            let v = get_tv(state, 3);
            for i in (pos..=len).rev() {
                let x = state.global.heap.table_get_int(table, i).unwrap_or(TValue::Nil);
                state.global.table_set_int(table, i + 1, x);
            }
            state.global.table_set_int(table, pos, v);
        }
        _ => return err(state, b"wrong number of arguments to insert"),
    }
    0
}

unsafe extern "C" fn tab_remove(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if state.type_at(1) != 5 {
        return err(state, b"bad argument 1 to remove");
    }
    let table = match get_tv(state, 1) {
        TValue::Table(h) => h,
        _ => return err(state, b"bad argument 1 to remove"),
    };
    let len = state.global.heap.table_len(table) as i64;
    let pos = state.to_integer_x(2).unwrap_or(len);
    if len == 0 {
        state.push_nil();
        return 1;
    }
    let v = state
        .global
        .heap
        .table_get_int(table, pos)
        .unwrap_or(TValue::Nil);
    for i in pos..len {
        let x = state.global.heap.table_get_int(table, i + 1).unwrap_or(TValue::Nil);
        state.global.table_set_int(table, i, x);
    }
    state.global.table_set_int(table, len, TValue::Nil);
    state.current_thread_mut().push(v);
    1
}

unsafe extern "C" fn tab_concat(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let table = match get_tv(state, 1) {
        TValue::Table(h) => h,
        _ => return err(state, b"bad argument 1 to concat"),
    };
    let sep_bytes = state.to_lstring(2).unwrap_or(b"").to_vec();
    let len = state.global.heap.table_len(table) as i64;
    let i = state.to_integer_x(3).unwrap_or(1);
    let j = state.to_integer_x(4).unwrap_or(len);
    let mut out: Vec<u8> = Vec::new();
    for idx in i..=j {
        if idx > i {
            out.extend_from_slice(&sep_bytes);
        }
        let v = state.global.heap.table_get_int(table, idx).unwrap_or(TValue::Nil);
        match v {
            TValue::ShortString(h) | TValue::LongString(h) => {
                let b = state.global.heap.string(h).bytes.clone();
                out.extend_from_slice(&b);
            }
            TValue::Integer(n) => out.extend_from_slice(n.to_string().as_bytes()),
            TValue::Number(f) => out.extend_from_slice(format!("{}", f).as_bytes()),
            _ => {
                return err(state, b"invalid value in table.concat");
            }
        }
    }
    state.push_string(&String::from_utf8_lossy(&out));
    1
}

unsafe extern "C" fn tab_unpack(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let table = match get_tv(state, 1) {
        TValue::Table(h) => h,
        _ => return err(state, b"bad argument 1 to unpack"),
    };
    let len = state.global.heap.table_len(table) as i64;
    let i = state.to_integer_x(2).unwrap_or(1);
    let j = state.to_integer_x(3).unwrap_or(len);
    if i > j {
        return 0;
    }
    let mut count = 0;
    for idx in i..=j {
        let v = state.global.heap.table_get_int(table, idx).unwrap_or(TValue::Nil);
        state.current_thread_mut().push(v);
        count += 1;
    }
    count
}

unsafe extern "C" fn tab_pack(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    let t = state.global.heap.alloc_table(crate::contract::Table::default());
    for i in 1..=top {
        let v = get_tv(state, i);
        state.global.table_set_int(t, i as i64, v);
    }
    let n_name = state.global.new_string(b"n", 0);
    state
        .global
        .table_set_shortstr(t, n_name, TValue::Integer(top as i64));
    state.current_thread_mut().push(TValue::Table(t));
    1
}

unsafe extern "C" fn tab_sort(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let table = match get_tv(state, 1) {
        TValue::Table(h) => h,
        _ => return err(state, b"bad argument 1 to sort"),
    };
    let len = state.global.heap.table_len(table) as i64;
    // Collect values.
    let mut values: Vec<TValue> = (1..=len)
        .map(|i| state.global.heap.table_get_int(table, i).unwrap_or(TValue::Nil))
        .collect();
    // Default comparator: ascending numeric/string.
    values.sort_by(|a, b| compare_tv(a, b, state).unwrap_or(std::cmp::Ordering::Equal));
    for (i, v) in values.into_iter().enumerate() {
        state.global.table_set_int(table, (i as i64) + 1, v);
    }
    0
}

fn compare_tv(a: &TValue, b: &TValue, state: &LuaState) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (TValue::Integer(x), TValue::Integer(y)) => Some(x.cmp(y)),
        (TValue::Number(x), TValue::Number(y)) => x.partial_cmp(y),
        (TValue::Integer(x), TValue::Number(y)) => (*x as f64).partial_cmp(y),
        (TValue::Number(x), TValue::Integer(y)) => x.partial_cmp(&(*y as f64)),
        (TValue::ShortString(h1) | TValue::LongString(h1),
         TValue::ShortString(h2) | TValue::LongString(h2)) => {
            let b1 = &state.global.heap.string(*h1).bytes;
            let b2 = &state.global.heap.string(*h2).bytes;
            Some(b1.cmp(b2))
        }
        _ => None,
    }
}

unsafe extern "C" fn tab_move(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // table.move(a1, f, e, t[, a2])
    let a1 = match get_tv(state, 1) {
        TValue::Table(h) => h,
        _ => return err(state, b"bad argument 1"),
    };
    let f = state.to_integer_x(2).unwrap_or(1);
    let e = state.to_integer_x(3).unwrap_or(0);
    let t = state.to_integer_x(4).unwrap_or(1);
    let a2 = if state.get_top() >= 5 {
        match get_tv(state, 5) {
            TValue::Table(h) => h,
            _ => a1,
        }
    } else {
        a1
    };
    if e >= f {
        let count = e - f + 1;
        if t > f {
            // Copy backwards to avoid overlap corruption.
            for i in (0..count).rev() {
                let v = state
                    .global
                    .heap
                    .table_get_int(a1, f + i)
                    .unwrap_or(TValue::Nil);
                state.global.table_set_int(a2, t + i, v);
            }
        } else {
            for i in 0..count {
                let v = state
                    .global
                    .heap
                    .table_get_int(a1, f + i)
                    .unwrap_or(TValue::Nil);
                state.global.table_set_int(a2, t + i, v);
            }
        }
    }
    state.current_thread_mut().push(TValue::Table(a2));
    1
}

fn get_tv(state: &LuaState, idx: i32) -> TValue {
    let base = state.frame_base_index().unwrap_or(0);
    let abs = base as i32 + idx - 1;
    if abs >= 0 && (abs as usize) < state.current_thread().stack.len() {
        state.current_thread().stack[abs as usize]
    } else {
        TValue::Nil
    }
}

fn err(state: &mut LuaState, msg: &[u8]) -> std::os::raw::c_int {
    let h = state.global.new_string(msg, 0);
    state.raise_error_value(TValue::ShortString(h));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lbaselib::open_base;

    #[test]
    fn open_table_registers_table() {
        let mut state = LuaState::new(0);
        let g = open_base(&mut state);
        open_table(&mut state, g);
        let name = state.global.new_string(b"table", 0);
        let v = state.global.heap.table_get_shortstr(g, name);
        assert!(matches!(v, Some(TValue::Table(_))));
    }

    #[test]
    fn table_concat_with_separator() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        let t = state.global.heap.alloc_table(crate::contract::Table::default());
        state.global.table_set_int(t, 1, TValue::Integer(10));
        state.global.table_set_int(t, 2, TValue::Integer(20));
        state.global.table_set_int(t, 3, TValue::Integer(30));
        unsafe {
            state.push_light_cfunction(tab_concat);
            state.current_thread_mut().push(TValue::Table(t));
            state.push_string(",");
            state.call_value(0, 2, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"10,20,30".as_slice()));
        }
    }
}
