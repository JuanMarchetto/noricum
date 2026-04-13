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
    // Format: default "%c"; leading "!" means UTC.
    let format_bytes = state
        .to_lstring(1)
        .map(|s| s.to_vec())
        .unwrap_or_else(|| b"%c".to_vec());
    let top = state.get_top() as i32;
    let now_secs = if top >= 2 {
        state.to_integer_x(2).unwrap_or(0)
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    };

    let (fmt, utc) = if format_bytes.first() == Some(&b'!') {
        (&format_bytes[1..], true)
    } else {
        (&format_bytes[..], false)
    };

    // Apply local offset (best-effort, TZ env only).
    let offset = if utc { 0 } else { local_tz_offset_secs() };
    let tm = secs_to_broken_down(now_secs + offset as i64);

    if fmt == b"*t" || fmt == b"!*t" {
        let t = state
            .global
            .heap
            .alloc_table(crate::contract::Table::default());
        set_i(state, t, "year", tm.year as i64);
        set_i(state, t, "month", tm.mon as i64);
        set_i(state, t, "day", tm.mday as i64);
        set_i(state, t, "hour", tm.hour as i64);
        set_i(state, t, "min", tm.min as i64);
        set_i(state, t, "sec", tm.sec as i64);
        set_i(state, t, "wday", tm.wday as i64);
        set_i(state, t, "yday", tm.yday as i64);
        set_b(state, t, "isdst", false);
        state.current_thread_mut().push(TValue::Table(t));
        return 1;
    }

    state.push_string(&strftime_like(fmt, &tm));
    1
}

fn set_i(state: &mut LuaState, t: TableHandle, k: &str, v: i64) {
    let h = state.global.new_string(k.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::Integer(v));
}

fn set_b(state: &mut LuaState, t: TableHandle, k: &str, v: bool) {
    let h = state.global.new_string(k.as_bytes(), 0);
    let val = if v { TValue::True } else { TValue::False };
    state.global.table_set_shortstr(t, h, val);
}

struct BrokenDown {
    year: i32,
    mon: u32,
    mday: u32,
    hour: u32,
    min: u32,
    sec: u32,
    wday: u32, // 1 = Sunday
    yday: u32, // 1 = Jan 1
}

/// Howard Hinnant's `civil_from_days`. Returns (y, m, d) from days
/// since 1970-01-01.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

fn secs_to_broken_down(t: i64) -> BrokenDown {
    let days = t.div_euclid(86_400);
    let secs_today = t.rem_euclid(86_400) as u32;
    let (year, mon, mday) = civil_from_days(days);
    // wday: 1970-01-01 was a Thursday (wday=5 in Lua's Sunday=1).
    let wday = (((days % 7 + 7) % 7) as u32 + 4) % 7 + 1;
    // yday: days since Jan 1 of current year + 1.
    let yday = (days - days_from_civil(year, 1, 1) + 1) as u32;
    BrokenDown {
        year,
        mon,
        mday,
        hour: secs_today / 3600,
        min: (secs_today / 60) % 60,
        sec: secs_today % 60,
        wday,
        yday,
    }
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u64 + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// Best-effort local timezone offset in seconds. Reads the `TZ`
/// environment variable when it's a numeric offset like `-03:00` or
/// `+0530`; otherwise returns 0 (UTC). Full tzdata handling is a
/// libc concern and is out of scope for ZERO-dep mode.
fn local_tz_offset_secs() -> i32 {
    if let Ok(tz) = std::env::var("TZ") {
        if let Some(off) = parse_tz_offset(&tz) {
            return off;
        }
    }
    0
}

fn parse_tz_offset(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let (sign, rest) = match bytes[0] {
        b'+' => (1, &bytes[1..]),
        b'-' => (-1, &bytes[1..]),
        _ => return None,
    };
    let mut digits = Vec::new();
    for &b in rest {
        if b.is_ascii_digit() {
            digits.push(b);
        }
    }
    let n: i32 = std::str::from_utf8(&digits).ok()?.parse().ok()?;
    let (h, m) = match digits.len() {
        2 => (n, 0),
        4 => (n / 100, n % 100),
        _ => return None,
    };
    Some(sign * (h * 3600 + m * 60))
}

fn strftime_like(fmt: &[u8], tm: &BrokenDown) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < fmt.len() {
        if fmt[i] == b'%' && i + 1 < fmt.len() {
            let c = fmt[i + 1];
            match c {
                b'Y' => out.push_str(&format!("{:04}", tm.year)),
                b'y' => out.push_str(&format!("{:02}", tm.year % 100)),
                b'm' => out.push_str(&format!("{:02}", tm.mon)),
                b'd' => out.push_str(&format!("{:02}", tm.mday)),
                b'H' => out.push_str(&format!("{:02}", tm.hour)),
                b'M' => out.push_str(&format!("{:02}", tm.min)),
                b'S' => out.push_str(&format!("{:02}", tm.sec)),
                b'p' => out.push_str(if tm.hour < 12 { "AM" } else { "PM" }),
                b'A' => out.push_str(weekday_name(tm.wday, false)),
                b'a' => out.push_str(weekday_name(tm.wday, true)),
                b'B' => out.push_str(month_name(tm.mon, false)),
                b'b' => out.push_str(month_name(tm.mon, true)),
                b'j' => out.push_str(&format!("{:03}", tm.yday)),
                b'w' => out.push_str(&format!("{}", tm.wday - 1)),
                b'c' => out.push_str(&format!(
                    "{} {} {:2} {:02}:{:02}:{:02} {:04}",
                    weekday_name(tm.wday, true),
                    month_name(tm.mon, true),
                    tm.mday,
                    tm.hour,
                    tm.min,
                    tm.sec,
                    tm.year
                )),
                b'x' => out.push_str(&format!("{:02}/{:02}/{:02}", tm.mon, tm.mday, tm.year % 100)),
                b'X' => out.push_str(&format!("{:02}:{:02}:{:02}", tm.hour, tm.min, tm.sec)),
                b'%' => out.push('%'),
                _ => {
                    out.push('%');
                    out.push(c as char);
                }
            }
            i += 2;
        } else {
            out.push(fmt[i] as char);
            i += 1;
        }
    }
    out
}

fn weekday_name(w: u32, short: bool) -> &'static str {
    // w: 1 = Sunday
    let long = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let sh = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let idx = ((w as usize).saturating_sub(1)).min(6);
    if short { sh[idx] } else { long[idx] }
}

fn month_name(m: u32, short: bool) -> &'static str {
    let long = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ];
    let sh = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let idx = ((m as usize).saturating_sub(1)).min(11);
    if short { sh[idx] } else { long[idx] }
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
