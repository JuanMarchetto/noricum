//! lmathlib — the Lua math library.
//!
//! Port of `lmathlib.c`. Registers `math.*` functions into the
//! globals table.

#![allow(dead_code)]

use crate::contract::{LuaNumber, LuaState, RawCFunction, TValue, TableHandle};

pub fn open_math(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let math_t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());

    register(state, math_t, "abs", math_abs);
    register(state, math_t, "ceil", math_ceil);
    register(state, math_t, "floor", math_floor);
    register(state, math_t, "sqrt", math_sqrt);
    register(state, math_t, "exp", math_exp);
    register(state, math_t, "log", math_log);
    register(state, math_t, "sin", math_sin);
    register(state, math_t, "cos", math_cos);
    register(state, math_t, "tan", math_tan);
    register(state, math_t, "asin", math_asin);
    register(state, math_t, "acos", math_acos);
    register(state, math_t, "atan", math_atan);
    register(state, math_t, "max", math_max);
    register(state, math_t, "min", math_min);
    register(state, math_t, "pow", math_pow);
    register(state, math_t, "fmod", math_fmod);
    register(state, math_t, "modf", math_modf);
    register(state, math_t, "random", math_random);
    register(state, math_t, "randomseed", math_randomseed);
    register(state, math_t, "tointeger", math_tointeger);
    register(state, math_t, "type", math_type);
    register(state, math_t, "deg", math_deg);
    register(state, math_t, "rad", math_rad);

    // Constants
    let pi = state.global.new_string(b"pi", 0);
    state
        .global
        .table_set_shortstr(math_t, pi, TValue::Number(std::f64::consts::PI));
    let huge = state.global.new_string(b"huge", 0);
    state
        .global
        .table_set_shortstr(math_t, huge, TValue::Number(f64::INFINITY));
    let maxi = state.global.new_string(b"maxinteger", 0);
    state
        .global
        .table_set_shortstr(math_t, maxi, TValue::Integer(i64::MAX));
    let mini = state.global.new_string(b"mininteger", 0);
    state
        .global
        .table_set_shortstr(math_t, mini, TValue::Integer(i64::MIN));

    let name = state.global.new_string(b"math", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(math_t));
    math_t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

unsafe extern "C" fn math_abs(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if let Some(i) = state.to_integer_x(1) {
        state.push_integer(i.wrapping_abs());
    } else if let Some(f) = state.to_number_x(1) {
        state.push_number(f.abs());
    } else {
        state.push_nil();
    }
    1
}

unsafe extern "C" fn math_ceil(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if let Some(i) = state.to_integer_x(1) {
        state.push_integer(i);
    } else if let Some(f) = state.to_number_x(1) {
        let c = f.ceil();
        if c >= i64::MIN as f64 && c <= i64::MAX as f64 {
            state.push_integer(c as i64);
        } else {
            state.push_number(c);
        }
    } else {
        state.push_nil();
    }
    1
}

unsafe extern "C" fn math_floor(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if let Some(i) = state.to_integer_x(1) {
        state.push_integer(i);
    } else if let Some(f) = state.to_number_x(1) {
        let c = f.floor();
        if c >= i64::MIN as f64 && c <= i64::MAX as f64 {
            state.push_integer(c as i64);
        } else {
            state.push_number(c);
        }
    } else {
        state.push_nil();
    }
    1
}

fn unary(state: &mut LuaState, f: fn(f64) -> f64) -> i32 {
    let x = state.to_number_x(1).unwrap_or(0.0);
    state.push_number(f(x));
    1
}

unsafe extern "C" fn math_sqrt(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::sqrt)
}

unsafe extern "C" fn math_exp(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::exp)
}

unsafe extern "C" fn math_log(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let x = state.to_number_x(1).unwrap_or(0.0);
    if state.get_top() >= 2 {
        let base = state.to_number_x(2).unwrap_or(std::f64::consts::E);
        state.push_number(x.log(base));
    } else {
        state.push_number(x.ln());
    }
    1
}

unsafe extern "C" fn math_sin(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::sin)
}

unsafe extern "C" fn math_cos(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::cos)
}

unsafe extern "C" fn math_tan(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::tan)
}

unsafe extern "C" fn math_asin(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::asin)
}

unsafe extern "C" fn math_acos(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::acos)
}

unsafe extern "C" fn math_atan(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let y = state.to_number_x(1).unwrap_or(0.0);
    if state.get_top() >= 2 {
        let x = state.to_number_x(2).unwrap_or(1.0);
        state.push_number(y.atan2(x));
    } else {
        state.push_number(y.atan());
    }
    1
}

unsafe extern "C" fn math_deg(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::to_degrees)
}

unsafe extern "C" fn math_rad(state: *mut LuaState) -> std::os::raw::c_int {
    unary(unsafe { &mut *state }, f64::to_radians)
}

unsafe extern "C" fn math_max(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    if top < 1 {
        state.push_nil();
        return 1;
    }
    let mut best_i: Option<i64> = None;
    let mut best_f: Option<f64> = None;
    for i in 1..=top {
        if let Some(v) = state.to_integer_x(i) {
            best_i = Some(best_i.map_or(v, |b| b.max(v)));
        } else if let Some(v) = state.to_number_x(i) {
            best_f = Some(best_f.map_or(v, |b| b.max(v)));
        }
    }
    match (best_i, best_f) {
        (Some(i), None) => state.push_integer(i),
        (None, Some(f)) => state.push_number(f),
        (Some(i), Some(f)) => {
            let combined = (i as f64).max(f);
            state.push_number(combined);
        }
        _ => state.push_nil(),
    }
    1
}

unsafe extern "C" fn math_min(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    if top < 1 {
        state.push_nil();
        return 1;
    }
    let mut best_i: Option<i64> = None;
    let mut best_f: Option<f64> = None;
    for i in 1..=top {
        if let Some(v) = state.to_integer_x(i) {
            best_i = Some(best_i.map_or(v, |b| b.min(v)));
        } else if let Some(v) = state.to_number_x(i) {
            best_f = Some(best_f.map_or(v, |b| b.min(v)));
        }
    }
    match (best_i, best_f) {
        (Some(i), None) => state.push_integer(i),
        (None, Some(f)) => state.push_number(f),
        (Some(i), Some(f)) => {
            let combined = (i as f64).min(f);
            state.push_number(combined);
        }
        _ => state.push_nil(),
    }
    1
}

unsafe extern "C" fn math_pow(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let a = state.to_number_x(1).unwrap_or(0.0);
    let b = state.to_number_x(2).unwrap_or(0.0);
    state.push_number(a.powf(b));
    1
}

unsafe extern "C" fn math_fmod(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let a = state.to_number_x(1).unwrap_or(0.0);
    let b = state.to_number_x(2).unwrap_or(1.0);
    state.push_number(a % b);
    1
}

unsafe extern "C" fn math_modf(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let x = state.to_number_x(1).unwrap_or(0.0);
    let int_part = x.trunc();
    let frac_part = x - int_part;
    if int_part >= i64::MIN as f64 && int_part <= i64::MAX as f64 {
        state.push_integer(int_part as i64);
    } else {
        state.push_number(int_part);
    }
    state.push_number(frac_part);
    2
}

// Simple PRNG state (xorshift64).
static mut RNG_STATE: u64 = 0x9E3779B97F4A7C15;

fn rng_next() -> u64 {
    unsafe {
        let mut x = RNG_STATE;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        RNG_STATE = x;
        x
    }
}

unsafe extern "C" fn math_random(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    match top {
        0 => {
            let r = rng_next();
            let v = (r >> 11) as f64 / (1u64 << 53) as f64;
            state.push_number(v);
        }
        1 => {
            let m = state.to_integer_x(1).unwrap_or(1).max(1);
            let r = (rng_next() as i64 & i64::MAX) % m + 1;
            state.push_integer(r);
        }
        _ => {
            let m = state.to_integer_x(1).unwrap_or(1);
            let n = state.to_integer_x(2).unwrap_or(1);
            if m > n {
                state.push_nil();
                return 1;
            }
            let range = (n - m + 1).max(1);
            let r = (rng_next() as i64 & i64::MAX) % range + m;
            state.push_integer(r);
        }
    }
    1
}

unsafe extern "C" fn math_randomseed(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let seed = state.to_integer_x(1).unwrap_or(0) as u64;
    unsafe {
        RNG_STATE = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
    }
    0
}

unsafe extern "C" fn math_tointeger(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    if let Some(i) = state.to_integer_x(1) {
        state.push_integer(i);
    } else {
        state.push_nil();
    }
    1
}

unsafe extern "C" fn math_type(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let tt = state.type_at(1);
    if tt == 3 {
        // Distinguish integer from float.
        if state.to_integer_x(1).is_some() {
            state.push_string("integer");
        } else {
            state.push_string("float");
        }
    } else {
        state.push_nil();
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lbaselib::open_base;

    #[test]
    fn open_math_registers_pi() {
        let mut state = LuaState::new(0);
        let g = open_base(&mut state);
        open_math(&mut state, g);
        let m_name = state.global.new_string(b"math", 0);
        let math = state.global.heap.table_get_shortstr(g, m_name);
        let pi_name = state.global.new_string(b"pi", 0);
        if let Some(TValue::Table(h)) = math {
            let pi = state.global.heap.table_get_shortstr(h, pi_name);
            assert!(matches!(pi, Some(TValue::Number(_))));
        } else {
            panic!("math table not registered");
        }
    }

    #[test]
    fn math_sqrt_of_four() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(math_sqrt);
            state.push_number(4.0);
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_number_x(1), Some(2.0));
        }
    }

    #[test]
    fn math_max_picks_largest() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(math_max);
            state.push_integer(3);
            state.push_integer(10);
            state.push_integer(7);
            state.call_value(0, 3, 1).unwrap();
            assert_eq!(state.to_integer_x(1), Some(10));
        }
    }

    #[test]
    fn math_floor_negative() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(math_floor);
            state.push_number(-3.2);
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_integer_x(1), Some(-4));
        }
    }

    #[test]
    fn math_type_integer() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(math_type);
            state.push_integer(42);
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"integer".as_slice()));
        }
    }
}
