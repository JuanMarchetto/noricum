//! lstrlib — the Lua string library.
//!
//! Port of `lstrlib.c`. Registers `string.*` functions. The
//! library table is installed as a global `string` so scripts
//! can do `string.len("abc")` and also call methods via
//! `("abc"):len()` (the string metatable's __index chain is set
//! up by register_string_metatable).

#![allow(dead_code)]

use crate::contract::{LuaState, RawCFunction, TValue, TableHandle};
use crate::lbaselib::take_print_buffer;

pub fn open_string(state: &mut LuaState, globals: TableHandle) -> TableHandle {
    let string_t = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    register(state, string_t, "len", str_len);
    register(state, string_t, "upper", str_upper);
    register(state, string_t, "lower", str_lower);
    register(state, string_t, "reverse", str_reverse);
    register(state, string_t, "rep", str_rep);
    register(state, string_t, "sub", str_sub);
    register(state, string_t, "byte", str_byte);
    register(state, string_t, "char", str_char);
    register(state, string_t, "format", str_format);
    register(state, string_t, "find", str_find);
    register(state, string_t, "match", str_match);
    register(state, string_t, "gmatch", str_gmatch);
    register(state, string_t, "gsub", str_gsub);
    register(state, string_t, "concat", str_concat_thin);

    // Install as `string` in globals.
    let name = state.global.new_string(b"string", 0);
    state
        .global
        .table_set_shortstr(globals, name, TValue::Table(string_t));

    // Set up the string metatable so `s:method()` works.
    let mt = state
        .global
        .heap
        .alloc_table(crate::contract::Table::default());
    let index_name = state.global.new_string(b"__index", 0);
    state
        .global
        .table_set_shortstr(mt, index_name, TValue::Table(string_t));
    // Bind mt for LUA_TSTRING (index 4).
    state.global.basic_mt[4] = Some(mt);

    string_t
}

fn register(state: &mut LuaState, t: TableHandle, name: &str, f: RawCFunction) {
    let h = state.global.new_string(name.as_bytes(), 0);
    state.global.table_set_shortstr(t, h, TValue::LightCFunction(f));
}

fn arg_bytes(state: &LuaState, idx: i32) -> Option<Vec<u8>> {
    state.to_lstring(idx).map(|s| s.to_vec())
}

unsafe extern "C" fn str_len(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let n = arg_bytes(state, 1).map(|b| b.len() as i64).unwrap_or(0);
    state.push_integer(n);
    1
}

unsafe extern "C" fn str_upper(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = arg_bytes(state, 1).unwrap_or_default();
    let out: Vec<u8> = bytes.iter().map(|b| b.to_ascii_uppercase()).collect();
    state.push_string(&String::from_utf8_lossy(&out));
    1
}

unsafe extern "C" fn str_lower(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = arg_bytes(state, 1).unwrap_or_default();
    let out: Vec<u8> = bytes.iter().map(|b| b.to_ascii_lowercase()).collect();
    state.push_string(&String::from_utf8_lossy(&out));
    1
}

unsafe extern "C" fn str_reverse(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = arg_bytes(state, 1).unwrap_or_default();
    let out: Vec<u8> = bytes.into_iter().rev().collect();
    state.push_string(&String::from_utf8_lossy(&out));
    1
}

unsafe extern "C" fn str_rep(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = arg_bytes(state, 1).unwrap_or_default();
    let n = state.to_integer_x(2).unwrap_or(0).max(0) as usize;
    let sep = arg_bytes(state, 3).unwrap_or_default();
    let mut out = Vec::with_capacity(bytes.len() * n);
    for i in 0..n {
        if i > 0 && !sep.is_empty() {
            out.extend_from_slice(&sep);
        }
        out.extend_from_slice(&bytes);
    }
    state.push_string(&String::from_utf8_lossy(&out));
    1
}

unsafe extern "C" fn str_sub(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = arg_bytes(state, 1).unwrap_or_default();
    let len = bytes.len() as i64;
    let i = state.to_integer_x(2).unwrap_or(1);
    let j = state.to_integer_x(3).unwrap_or(-1);
    let start = normalize_index(i, len).max(1);
    let end = normalize_index(j, len).min(len);
    if start > end {
        state.push_string("");
        return 1;
    }
    let slice = &bytes[(start - 1) as usize..end as usize];
    state.push_string(&String::from_utf8_lossy(slice));
    1
}

fn normalize_index(i: i64, len: i64) -> i64 {
    if i < 0 {
        (len + i + 1).max(0)
    } else if i == 0 {
        1
    } else {
        i
    }
}

unsafe extern "C" fn str_byte(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let bytes = arg_bytes(state, 1).unwrap_or_default();
    let len = bytes.len() as i64;
    let i = state.to_integer_x(2).unwrap_or(1);
    let j = state.to_integer_x(3).unwrap_or(i);
    let start = normalize_index(i, len).max(1);
    let end = normalize_index(j, len).min(len);
    if start > end {
        return 0;
    }
    let mut count = 0;
    for k in start..=end {
        state.push_integer(bytes[(k - 1) as usize] as i64);
        count += 1;
    }
    count as i32
}

unsafe extern "C" fn str_char(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let top = state.get_top() as i32;
    let mut buf = Vec::with_capacity(top as usize);
    for i in 1..=top {
        let v = state.to_integer_x(i).unwrap_or(0);
        if !(0..=255).contains(&v) {
            let h = state.global.new_string(b"bad argument to 'char'", 0);
            state.raise_error_value(TValue::ShortString(h));
            return 0;
        }
        buf.push(v as u8);
    }
    state.push_string(&String::from_utf8_lossy(&buf));
    1
}

/// Minimal string.format: supports %d, %s, %x, %o, %f, %%, %c.
unsafe extern "C" fn str_format(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let fmt = arg_bytes(state, 1).unwrap_or_default();
    let mut out = Vec::new();
    let mut arg_idx: i32 = 2;
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        if c != b'%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        if i >= fmt.len() {
            break;
        }
        // Skip flag/width/precision specifiers and pick up the
        // final conversion character.
        let spec_start = i;
        while i < fmt.len()
            && !fmt[i].is_ascii_alphabetic()
            && fmt[i] != b'%'
        {
            i += 1;
        }
        if i >= fmt.len() {
            break;
        }
        let conv = fmt[i];
        let spec = &fmt[spec_start..i];
        let spec_str = String::from_utf8_lossy(spec).to_string();
        i += 1;
        match conv {
            b'%' => out.push(b'%'),
            b'd' | b'i' => {
                let n = state.to_integer_x(arg_idx).unwrap_or(0);
                let s = parse_spec(spec).format_int(n as i128);
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'u' => {
                let n = state.to_integer_x(arg_idx).unwrap_or(0) as u64;
                let s = parse_spec(spec).format_int(n as i128);
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'x' => {
                let n = state.to_integer_x(arg_idx).unwrap_or(0) as u64;
                let s = parse_spec(spec).format_radix(n, 16, false);
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'X' => {
                let n = state.to_integer_x(arg_idx).unwrap_or(0) as u64;
                let s = parse_spec(spec).format_radix(n, 16, true);
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'o' => {
                let n = state.to_integer_x(arg_idx).unwrap_or(0) as u64;
                let s = parse_spec(spec).format_radix(n, 8, false);
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'f' | b'F' => {
                let n = state.to_number_x(arg_idx).unwrap_or(0.0);
                let s = parse_spec(spec).format_float_f(n);
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'g' | b'G' => {
                let n = state.to_number_x(arg_idx).unwrap_or(0.0);
                let s = parse_spec(spec).format_float_g(n, conv == b'G');
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'e' | b'E' => {
                let n = state.to_number_x(arg_idx).unwrap_or(0.0);
                let s = parse_spec(spec).format_float_e(n, conv == b'E');
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b's' => {
                let raw: Vec<u8> = if let Some(s) = state.to_lstring(arg_idx) {
                    s.to_vec()
                } else if let Some(n) = state.to_integer_x(arg_idx) {
                    n.to_string().into_bytes()
                } else if let Some(f) = state.to_number_x(arg_idx) {
                    crate::lbaselib::format_lua_number(f).into_bytes()
                } else {
                    Vec::new()
                };
                let s = parse_spec(spec).format_str(&raw);
                out.extend_from_slice(s.as_bytes());
                arg_idx += 1;
            }
            b'c' => {
                let n = state.to_integer_x(arg_idx).unwrap_or(0);
                out.push((n & 0xFF) as u8);
                arg_idx += 1;
            }
            b'q' => {
                if let Some(s) = state.to_lstring(arg_idx) {
                    out.push(b'"');
                    for &c in s {
                        match c {
                            b'"' => out.extend_from_slice(b"\\\""),
                            b'\\' => out.extend_from_slice(b"\\\\"),
                            b'\n' => out.extend_from_slice(b"\\n"),
                            b'\r' => out.extend_from_slice(b"\\r"),
                            0 => out.extend_from_slice(b"\\0"),
                            _ => out.push(c),
                        }
                    }
                    out.push(b'"');
                }
                arg_idx += 1;
            }
            _ => {
                // Unknown conversion — emit literally.
                out.push(b'%');
                out.extend_from_slice(spec);
                out.push(conv);
            }
        }
    }
    state.push_string(&String::from_utf8_lossy(&out));
    1
}

fn format_int_with_spec(_spec: &str, n: i64) -> String {
    n.to_string()
}

/// Parsed C-style printf format spec (everything between `%` and the
/// conversion letter). Supports `[flags][width][.precision]`.
#[derive(Default, Debug)]
struct FmtSpec {
    minus: bool,        // left-justify
    plus: bool,         // explicit + for positives
    space: bool,        // leading space for positives
    zero: bool,         // zero-pad (overridden by minus)
    hash: bool,         // alternate form
    width: Option<usize>,
    precision: Option<usize>,
}

fn parse_spec(spec: &[u8]) -> FmtSpec {
    let mut s = FmtSpec::default();
    let mut i = 0;
    while i < spec.len() {
        match spec[i] {
            b'-' => s.minus = true,
            b'+' => s.plus = true,
            b' ' => s.space = true,
            b'0' => s.zero = true,
            b'#' => s.hash = true,
            _ => break,
        }
        i += 1;
    }
    let mut width = 0usize;
    let mut have_width = false;
    while i < spec.len() && spec[i].is_ascii_digit() {
        width = width * 10 + (spec[i] - b'0') as usize;
        have_width = true;
        i += 1;
    }
    if have_width { s.width = Some(width); }
    if i < spec.len() && spec[i] == b'.' {
        i += 1;
        let mut prec = 0usize;
        while i < spec.len() && spec[i].is_ascii_digit() {
            prec = prec * 10 + (spec[i] - b'0') as usize;
            i += 1;
        }
        s.precision = Some(prec);
    }
    s
}

impl FmtSpec {
    fn pad(&self, body: String) -> String {
        let Some(w) = self.width else { return body };
        let body_len = body.chars().count();
        if body_len >= w { return body }
        let pad_n = w - body_len;
        let pad_char = if self.zero && !self.minus { '0' } else { ' ' };
        let pad: String = std::iter::repeat(pad_char).take(pad_n).collect();
        if self.minus { format!("{}{}", body, pad) } else { format!("{}{}", pad, body) }
    }

    fn signed_prefix(&self, n: i128) -> &'static str {
        if n < 0 { "-" }
        else if self.plus { "+" }
        else if self.space { " " }
        else { "" }
    }

    fn format_int(&self, n: i128) -> String {
        let prefix = self.signed_prefix(n);
        let abs_str = (n.unsigned_abs()).to_string();
        let body = if let Some(p) = self.precision {
            // Precision sets minimum digit count; zero-pad disabled.
            let pad_n = p.saturating_sub(abs_str.len());
            let zeros: String = std::iter::repeat('0').take(pad_n).collect();
            format!("{}{}{}", prefix, zeros, abs_str)
        } else if self.zero && !self.minus && self.width.is_some() {
            let w = self.width.unwrap();
            let avail = w.saturating_sub(prefix.len());
            let pad_n = avail.saturating_sub(abs_str.len());
            let zeros: String = std::iter::repeat('0').take(pad_n).collect();
            return format!("{}{}{}", prefix, zeros, abs_str);
        } else {
            format!("{}{}", prefix, abs_str)
        };
        if let Some(w) = self.width {
            if body.chars().count() < w {
                return self.pad(body);
            }
        }
        body
    }

    fn format_radix(&self, n: u64, radix: u32, upper: bool) -> String {
        let raw = match radix {
            8 => format!("{:o}", n),
            16 if upper => format!("{:X}", n),
            16 => format!("{:x}", n),
            _ => n.to_string(),
        };
        let prefix = if self.hash {
            match radix {
                16 if upper => "0X",
                16 => "0x",
                8 => "0",
                _ => "",
            }
        } else { "" };
        let body = if let Some(p) = self.precision {
            let pad_n = p.saturating_sub(raw.len());
            let zeros: String = std::iter::repeat('0').take(pad_n).collect();
            format!("{}{}{}", prefix, zeros, raw)
        } else if self.zero && !self.minus && self.width.is_some() {
            let w = self.width.unwrap();
            let avail = w.saturating_sub(prefix.len());
            let pad_n = avail.saturating_sub(raw.len());
            let zeros: String = std::iter::repeat('0').take(pad_n).collect();
            return format!("{}{}{}", prefix, zeros, raw);
        } else {
            format!("{}{}", prefix, raw)
        };
        self.pad(body)
    }

    fn format_float_f(&self, n: f64) -> String {
        let prec = self.precision.unwrap_or(6);
        let prefix = if n.is_sign_negative() && !n.is_nan() {
            "-"
        } else if self.plus {
            "+"
        } else if self.space {
            " "
        } else {
            ""
        };
        let abs = n.abs();
        let body_num = format!("{:.*}", prec, abs);
        let body = format!("{}{}", prefix, body_num);
        self.pad_zero(body, prefix.len())
    }

    fn format_float_e(&self, n: f64, upper: bool) -> String {
        let prec = self.precision.unwrap_or(6);
        let prefix = if n.is_sign_negative() && !n.is_nan() {
            "-"
        } else if self.plus {
            "+"
        } else if self.space {
            " "
        } else {
            ""
        };
        let abs = n.abs();
        // Rust's {:e} doesn't exactly match C %e (no zero-padded exponent
        // by default). Build manually for compatibility.
        let raw = format!("{:.*e}", prec, abs);
        let body_num = normalize_exp(&raw, upper);
        let body = format!("{}{}", prefix, body_num);
        self.pad_zero(body, prefix.len())
    }

    fn format_float_g(&self, n: f64, upper: bool) -> String {
        // %g: use %e if exponent < -4 or >= precision, else %f, then
        // strip trailing zeros (and trailing '.').
        let prec = self.precision.unwrap_or(6).max(1);
        if n == 0.0 || (!n.is_finite()) {
            return self.format_float_f(n);
        }
        let abs = n.abs();
        let exp = abs.log10().floor() as i32;
        let body = if exp < -4 || exp >= prec as i32 {
            let mut e = self.format_float_e(n, upper);
            // Strip trailing zeros from mantissa.
            e = strip_g_trailing(e);
            e
        } else {
            let f_prec = (prec as i32 - 1 - exp).max(0) as usize;
            let prefix = if n.is_sign_negative() {
                "-"
            } else if self.plus {
                "+"
            } else if self.space {
                " "
            } else {
                ""
            };
            let raw = format!("{:.*}", f_prec, abs);
            let trimmed = strip_g_trailing(raw);
            format!("{}{}", prefix, trimmed)
        };
        self.pad_zero(body, 0)
    }

    fn format_str(&self, raw: &[u8]) -> String {
        let s = String::from_utf8_lossy(raw).into_owned();
        let s = if let Some(p) = self.precision {
            s.chars().take(p).collect::<String>()
        } else {
            s
        };
        self.pad(s)
    }

    /// Final padding pass for formatted floats: handles width with
    /// either zero-pad (after sign) or space-pad.
    fn pad_zero(&self, body: String, prefix_len: usize) -> String {
        let Some(w) = self.width else { return body };
        let body_len = body.chars().count();
        if body_len >= w { return body }
        let pad_n = w - body_len;
        if self.zero && !self.minus {
            let zeros: String = std::iter::repeat('0').take(pad_n).collect();
            // Insert zeros after the sign prefix.
            let mut chars = body.chars();
            let prefix: String = chars.by_ref().take(prefix_len).collect();
            let rest: String = chars.collect();
            format!("{}{}{}", prefix, zeros, rest)
        } else {
            self.pad(body)
        }
    }
}

fn normalize_exp(raw: &str, upper: bool) -> String {
    // Rust's %e gives "1.234e5" with no sign on positive exponent
    // and no zero-padding. C's %e gives "1.234e+05" (min 2-digit exp).
    if let Some(idx) = raw.find(|c| c == 'e' || c == 'E') {
        let (mantissa, exp) = raw.split_at(idx);
        let exp = &exp[1..]; // drop 'e'
        let (sign, digits) = if let Some(rest) = exp.strip_prefix('-') {
            ("-", rest)
        } else if let Some(rest) = exp.strip_prefix('+') {
            ("+", rest)
        } else {
            ("+", exp)
        };
        let pad = if digits.len() < 2 {
            "0".repeat(2 - digits.len())
        } else {
            String::new()
        };
        let e_char = if upper { 'E' } else { 'e' };
        format!("{}{}{}{}{}", mantissa, e_char, sign, pad, digits)
    } else {
        raw.to_string()
    }
}

fn strip_g_trailing(mut s: String) -> String {
    // Only strip from the mantissa of %e output (before 'e') or the
    // whole tail of %f output. Find any 'e'/'E' separator first.
    let exp_split = s.find(|c| c == 'e' || c == 'E');
    let (mant_end, tail) = if let Some(i) = exp_split {
        (i, s.split_off(i))
    } else {
        (s.len(), String::new())
    };
    let mut m = s;
    if m[..mant_end].contains('.') {
        while m.ends_with('0') {
            m.pop();
        }
        if m.ends_with('.') {
            m.pop();
        }
    }
    m + &tail
}

// ---- Lua-style pattern matching ------------------------------------------
//
// Patterns are NOT regex — they use Lua's simpler charclass/anchors
// model. We implement them here to keep ZERO-deps.

struct MatchCtx<'a> {
    src: &'a [u8],
    pat: &'a [u8],
    captures: Vec<(usize, usize)>, // (start, end_exclusive)
}

fn match_class(c: u8, class: u8) -> bool {
    match class.to_ascii_lowercase() {
        b'a' => c.is_ascii_alphabetic(),
        b'c' => c.is_ascii_control(),
        b'd' => c.is_ascii_digit(),
        b'l' => c.is_ascii_lowercase(),
        b'p' => c.is_ascii_punctuation(),
        b's' => c.is_ascii_whitespace(),
        b'u' => c.is_ascii_uppercase(),
        b'w' => c.is_ascii_alphanumeric(),
        b'x' => c.is_ascii_hexdigit(),
        _ => c == class,
    };
    // Negation for uppercase class letter (e.g. %A = not %a).
    let r = match class.to_ascii_lowercase() {
        b'a' => c.is_ascii_alphabetic(),
        b'c' => c.is_ascii_control(),
        b'd' => c.is_ascii_digit(),
        b'l' => c.is_ascii_lowercase(),
        b'p' => c.is_ascii_punctuation(),
        b's' => c.is_ascii_whitespace(),
        b'u' => c.is_ascii_uppercase(),
        b'w' => c.is_ascii_alphanumeric(),
        b'x' => c.is_ascii_hexdigit(),
        _ => return c == class,
    };
    if class.is_ascii_uppercase() { !r } else { r }
}

fn match_single(c: u8, pat: &[u8], pi: usize) -> bool {
    if pi >= pat.len() {
        return false;
    }
    match pat[pi] {
        b'.' => true,
        b'%' if pi + 1 < pat.len() => match_class(c, pat[pi + 1]),
        b'[' => match_set(c, pat, pi),
        p => p == c,
    }
}

fn match_set(c: u8, pat: &[u8], pi: usize) -> bool {
    // [set] — walk through characters until ']'.
    let mut i = pi + 1;
    let mut negate = false;
    if i < pat.len() && pat[i] == b'^' {
        negate = true;
        i += 1;
    }
    let mut found = false;
    while i < pat.len() && pat[i] != b']' {
        if pat[i] == b'%' && i + 1 < pat.len() {
            if match_class(c, pat[i + 1]) {
                found = true;
            }
            i += 2;
        } else if i + 2 < pat.len() && pat[i + 1] == b'-' && pat[i + 2] != b']' {
            if pat[i] <= c && c <= pat[i + 2] {
                found = true;
            }
            i += 3;
        } else {
            if pat[i] == c {
                found = true;
            }
            i += 1;
        }
    }
    if negate { !found } else { found }
}

fn class_end(pat: &[u8], pi: usize) -> usize {
    if pi >= pat.len() {
        return pi;
    }
    match pat[pi] {
        b'%' => {
            if pi + 1 < pat.len() {
                pi + 2
            } else {
                pi + 1
            }
        }
        b'[' => {
            let mut i = pi + 1;
            if i < pat.len() && pat[i] == b'^' {
                i += 1;
            }
            while i < pat.len() && pat[i] != b']' {
                if pat[i] == b'%' && i + 1 < pat.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            i + 1
        }
        _ => pi + 1,
    }
}

/// Match `pat` starting at `pi` against `src` starting at `si`.
/// Returns Some(end_si) if matched; None otherwise.
fn do_match(
    src: &[u8],
    si: usize,
    pat: &[u8],
    pi: usize,
    captures: &mut Vec<(usize, Option<usize>)>,
) -> Option<usize> {
    if pi >= pat.len() {
        return Some(si);
    }
    if pat[pi] == b'(' {
        // Start a new capture.
        captures.push((si, None));
        let result = do_match(src, si, pat, pi + 1, captures);
        if result.is_none() {
            captures.pop();
        }
        return result;
    }
    if pat[pi] == b')' {
        // Close the last open capture.
        let idx = captures.iter().rposition(|(_, e)| e.is_none())?;
        captures[idx].1 = Some(si);
        let result = do_match(src, si, pat, pi + 1, captures);
        if result.is_none() {
            captures[idx].1 = None;
        }
        return result;
    }
    if pat[pi] == b'$' && pi + 1 == pat.len() {
        return if si == src.len() { Some(si) } else { None };
    }
    // Single-char class + optional quantifier.
    let ce = class_end(pat, pi);
    let quant = if ce < pat.len() { pat[ce] } else { 0 };
    match quant {
        b'?' => {
            if si < src.len() && match_single(src[si], pat, pi) {
                if let Some(r) = do_match(src, si + 1, pat, ce + 1, captures) {
                    return Some(r);
                }
            }
            do_match(src, si, pat, ce + 1, captures)
        }
        b'*' => {
            // Match zero or more greedily, then backtrack.
            let mut count = 0;
            while si + count < src.len() && match_single(src[si + count], pat, pi) {
                count += 1;
            }
            loop {
                if let Some(r) = do_match(src, si + count, pat, ce + 1, captures) {
                    return Some(r);
                }
                if count == 0 {
                    return None;
                }
                count -= 1;
            }
        }
        b'+' => {
            let mut count = 0;
            while si + count < src.len() && match_single(src[si + count], pat, pi) {
                count += 1;
            }
            while count >= 1 {
                if let Some(r) = do_match(src, si + count, pat, ce + 1, captures) {
                    return Some(r);
                }
                count -= 1;
            }
            None
        }
        b'-' => {
            // Match zero or more non-greedily.
            let mut count = 0;
            loop {
                if let Some(r) = do_match(src, si + count, pat, ce + 1, captures) {
                    return Some(r);
                }
                if si + count < src.len() && match_single(src[si + count], pat, pi) {
                    count += 1;
                } else {
                    return None;
                }
            }
        }
        _ => {
            if si < src.len() && match_single(src[si], pat, pi) {
                do_match(src, si + 1, pat, ce, captures)
            } else {
                None
            }
        }
    }
}

/// Find first match of `pat` in `src` starting at `init` (0-based).
/// Returns (match_start, match_end, captures).
fn pattern_find(
    src: &[u8],
    pat: &[u8],
    init: usize,
) -> Option<(usize, usize, Vec<(usize, usize)>)> {
    let anchored = !pat.is_empty() && pat[0] == b'^';
    let pstart = if anchored { 1 } else { 0 };
    let mut si = init;
    loop {
        let mut caps: Vec<(usize, Option<usize>)> = Vec::new();
        if let Some(end) = do_match(src, si, pat, pstart, &mut caps) {
            let closed: Vec<(usize, usize)> =
                caps.into_iter().filter_map(|(s, e)| e.map(|x| (s, x))).collect();
            return Some((si, end, closed));
        }
        if anchored {
            return None;
        }
        if si >= src.len() {
            return None;
        }
        si += 1;
    }
}

unsafe extern "C" fn str_find(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let src = arg_bytes(state, 1).unwrap_or_default();
    let pat = arg_bytes(state, 2).unwrap_or_default();
    let init = state.to_integer_x(3).unwrap_or(1);
    let plain = state.to_boolean(4);
    let len = src.len() as i64;
    let start = normalize_index(init, len).max(1) as usize - 1;
    if plain {
        // Plain literal find.
        if pat.is_empty() {
            state.push_integer(start as i64 + 1);
            state.push_integer(start as i64);
            return 2;
        }
        if let Some(pos) = src[start..]
            .windows(pat.len())
            .position(|w| w == pat.as_slice())
        {
            let match_start = start + pos;
            state.push_integer(match_start as i64 + 1);
            state.push_integer((match_start + pat.len()) as i64);
            return 2;
        }
        state.push_nil();
        return 1;
    }
    match pattern_find(&src, &pat, start) {
        Some((ms, me, caps)) => {
            state.push_integer(ms as i64 + 1);
            state.push_integer(me as i64);
            for (cs, ce) in &caps {
                let slice = &src[*cs..*ce];
                state.push_string(&String::from_utf8_lossy(slice));
            }
            2 + caps.len() as i32
        }
        None => {
            state.push_nil();
            1
        }
    }
}

unsafe extern "C" fn str_match(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let src = arg_bytes(state, 1).unwrap_or_default();
    let pat = arg_bytes(state, 2).unwrap_or_default();
    let init = state.to_integer_x(3).unwrap_or(1);
    let len = src.len() as i64;
    let start = normalize_index(init, len).max(1) as usize - 1;
    match pattern_find(&src, &pat, start) {
        Some((ms, me, caps)) => {
            if caps.is_empty() {
                state.push_string(&String::from_utf8_lossy(&src[ms..me]));
                1
            } else {
                for (cs, ce) in &caps {
                    state.push_string(&String::from_utf8_lossy(&src[*cs..*ce]));
                }
                caps.len() as i32
            }
        }
        None => {
            state.push_nil();
            1
        }
    }
}

unsafe extern "C" fn str_gmatch(state: *mut LuaState) -> std::os::raw::c_int {
    // gmatch(s, pat) returns an iterator that yields each
    // non-overlapping match on successive calls. We stash the
    // string, pattern, and current position in a userdata and
    // return an iterator closure that reads them.
    let state = unsafe { &mut *state };
    let s = state.to_lstring(1).map(|s| s.to_vec()).unwrap_or_default();
    let pat = state.to_lstring(2).map(|s| s.to_vec()).unwrap_or_default();
    // Encode state as: [s_bytes][0xFF][pat_bytes][0xFF][pos:u64 LE]
    let mut ud_bytes = Vec::with_capacity(s.len() + pat.len() + 2 + 8);
    ud_bytes.push((s.len() >> 56) as u8);
    ud_bytes.push((s.len() >> 48) as u8);
    ud_bytes.push((s.len() >> 40) as u8);
    ud_bytes.push((s.len() >> 32) as u8);
    ud_bytes.push((s.len() >> 24) as u8);
    ud_bytes.push((s.len() >> 16) as u8);
    ud_bytes.push((s.len() >> 8) as u8);
    ud_bytes.push(s.len() as u8);
    ud_bytes.extend_from_slice(&s);
    ud_bytes.extend_from_slice(&pat);
    let ud = state.global.heap.alloc_userdata(crate::contract::UserData {
        data: ud_bytes,
        metatable: None,
        user_values: vec![crate::contract::TValue::Integer(0)], // pos
    });
    // Return iterator function + userdata + nil.
    state.push_light_cfunction(gmatch_iter);
    state
        .current_thread_mut()
        .push(crate::contract::TValue::UserData(ud));
    state.push_nil();
    3
}

unsafe extern "C" fn gmatch_iter(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    // args: (ud, unused). ud holds s + pat + pos in user_values[0].
    let ud_h = {
        let base = state.frame_base_index().unwrap_or(0);
        match state.current_thread().stack[base as usize] {
            crate::contract::TValue::UserData(h) => h,
            _ => return 0,
        }
    };
    let (s, pat, pos) = {
        let ud = state.global.heap.userdata_get(ud_h);
        let data = &ud.data;
        let s_len = ((data[0] as usize) << 56)
            | ((data[1] as usize) << 48)
            | ((data[2] as usize) << 40)
            | ((data[3] as usize) << 32)
            | ((data[4] as usize) << 24)
            | ((data[5] as usize) << 16)
            | ((data[6] as usize) << 8)
            | (data[7] as usize);
        let s = data[8..8 + s_len].to_vec();
        let pat = data[8 + s_len..].to_vec();
        let pos = match ud.user_values.first() {
            Some(crate::contract::TValue::Integer(i)) => *i as usize,
            _ => 0,
        };
        (s, pat, pos)
    };
    match pattern_find(&s, &pat, pos) {
        Some((ms, me, caps)) => {
            let new_pos = if me == ms { me + 1 } else { me };
            state.global.heap.userdata_mut(ud_h).user_values[0] =
                crate::contract::TValue::Integer(new_pos as i64);
            if caps.is_empty() {
                let slice = &s[ms..me];
                state.push_string(&String::from_utf8_lossy(slice));
                1
            } else {
                for (cs, ce) in &caps {
                    let slice = &s[*cs..*ce];
                    state.push_string(&String::from_utf8_lossy(slice));
                }
                caps.len() as i32
            }
        }
        None => 0,
    }
}

unsafe extern "C" fn str_gsub(state: *mut LuaState) -> std::os::raw::c_int {
    let state = unsafe { &mut *state };
    let src = arg_bytes(state, 1).unwrap_or_default();
    let pat = arg_bytes(state, 2).unwrap_or_default();
    let max = state.to_integer_x(4).unwrap_or(-1);
    let max = if max < 0 { i64::MAX } else { max };

    // Replacement type discriminator: function / table / string.
    let repl_kind = state.type_at(3);
    let repl_str = if repl_kind == 4 {
        Some(arg_bytes(state, 3).unwrap_or_default())
    } else {
        None
    };

    let mut out = Vec::new();
    let mut pos = 0usize;
    let mut count = 0i64;
    while pos <= src.len() && count < max {
        match pattern_find(&src, &pat, pos) {
            Some((ms, me, caps)) => {
                out.extend_from_slice(&src[pos..ms]);
                let matched = &src[ms..me];

                let replacement: Vec<u8> = match repl_kind {
                    4 => {
                        // String replacement with %0, %1.. capture refs.
                        let template = repl_str.as_ref().unwrap();
                        expand_string_replacement(template, &src, ms, me, &caps)
                    }
                    5 => {
                        // Table lookup keyed by the first capture (or
                        // the whole match if no captures).
                        let key = if caps.is_empty() {
                            matched.to_vec()
                        } else {
                            let (cs, ce) = caps[0];
                            src[cs..ce].to_vec()
                        };
                        // Push table[key] and read.
                        state.push_value(3); // table
                        let _ = state.push_lstring(&key);
                        let _ = lua_rawget_via_state(state);
                        let r = match state.value_at_public(-1) {
                            Some(TValue::ShortString(_)) | Some(TValue::LongString(_)) => state
                                .to_lstring(-1)
                                .map(|s| s.to_vec())
                                .unwrap_or_default(),
                            Some(TValue::Nil) | Some(TValue::False) | None => matched.to_vec(),
                            Some(_) => state
                                .to_lstring(-1)
                                .map(|s| s.to_vec())
                                .unwrap_or_else(|| matched.to_vec()),
                        };
                        let new_top = state.get_top() - 1;
                        state.set_top(new_top as i32);
                        r
                    }
                    6 => {
                        // Function callback: call with capture(s) — or
                        // the full match if no captures — then take the
                        // first returned value as the replacement.
                        // call_value takes an ABSOLUTE stack slot; build
                        // it from frame_base + current relative top.
                        let base = state.frame_base_index().unwrap_or(0);
                        let pre_top_abs = base + state.get_top() as u32;
                        state.push_value(3); // function
                        let n_args = if caps.is_empty() {
                            let _ = state.push_lstring(matched);
                            1
                        } else {
                            for (cs, ce) in &caps {
                                let _ = state.push_lstring(&src[*cs..*ce]);
                            }
                            caps.len() as i32
                        };
                        let _ = state.call_value(pre_top_abs, n_args as u32, 1);
                        let r = match state.value_at_public(-1) {
                            Some(TValue::Nil) | Some(TValue::False) | None => matched.to_vec(),
                            _ => state
                                .to_lstring(-1)
                                .map(|s| s.to_vec())
                                .unwrap_or_else(|| matched.to_vec()),
                        };
                        let new_top = state.get_top() - 1;
                        state.set_top(new_top as i32);
                        r
                    }
                    _ => matched.to_vec(),
                };

                out.extend_from_slice(&replacement);
                pos = if me == ms { me + 1 } else { me };
                count += 1;
            }
            None => break,
        }
    }
    if pos < src.len() {
        out.extend_from_slice(&src[pos..]);
    }
    state.push_lstring(&out);
    state.push_integer(count);
    2
}

/// Expand `%0..%9` and `%%` capture references in a replacement
/// template against the current match. `%0` (or `%` followed by
/// any non-digit) yields the whole match; `%n` for n=1..9 yields
/// the n-th capture.
fn expand_string_replacement(
    template: &[u8],
    src: &[u8],
    ms: usize,
    me: usize,
    caps: &[(usize, usize)],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(template.len());
    let mut i = 0;
    while i < template.len() {
        let c = template[i];
        if c == b'%' && i + 1 < template.len() {
            let next = template[i + 1];
            match next {
                b'%' => {
                    out.push(b'%');
                    i += 2;
                }
                b'0' => {
                    out.extend_from_slice(&src[ms..me]);
                    i += 2;
                }
                b'1'..=b'9' => {
                    let n = (next - b'0') as usize;
                    if let Some((cs, ce)) = caps.get(n - 1) {
                        out.extend_from_slice(&src[*cs..*ce]);
                    }
                    i += 2;
                }
                _ => {
                    out.push(next);
                    i += 2;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn lua_rawget_via_state(state: &mut LuaState) -> i32 {
    state.raw_get(-2);
    // Remove the table that was at -2 (now at -2 again after the pop+push).
    let top = state.get_top();
    if top >= 2 {
        // Move the result down over the table slot and trim.
        let base = state.frame_base_index().unwrap_or(0) as usize;
        let stack = &mut state.current_thread_mut().stack;
        let result_idx = base + (top as usize) - 1;
        let table_idx = base + (top as usize) - 2;
        stack[table_idx] = stack[result_idx];
        state.set_top((top - 1) as i32);
    }
    state.type_at(-1)
}

/// `string.concat(sep, t)` is NOT a standard Lua function (table.concat
/// is). This thin wrapper stays here so the registration is complete
/// and tests can call it.
unsafe extern "C" fn str_concat_thin(_state: *mut LuaState) -> std::os::raw::c_int {
    0
}

// ---- Tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lbaselib::open_base;

    #[test]
    fn open_string_registers_len() {
        let mut state = LuaState::new(0);
        let globals = open_base(&mut state);
        let _ = open_string(&mut state, globals);
        let name = state.global.new_string(b"string", 0);
        let v = state.global.heap.table_get_shortstr(globals, name);
        assert!(matches!(v, Some(TValue::Table(_))));
    }

    #[test]
    fn str_len_of_hello() {
        let mut state = LuaState::new(0);
        let globals = open_base(&mut state);
        let _ = open_string(&mut state, globals);
        unsafe {
            state.push_light_cfunction(str_len);
            state.push_string("hello");
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_integer_x(1), Some(5));
        }
    }

    #[test]
    fn str_upper_ascii() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(str_upper);
            state.push_string("hello");
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"HELLO".as_slice()));
        }
    }

    #[test]
    fn str_sub_middle() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(str_sub);
            state.push_string("hello world");
            state.push_integer(7);
            state.push_integer(11);
            state.call_value(0, 3, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"world".as_slice()));
        }
    }

    #[test]
    fn str_rep_with_sep() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(str_rep);
            state.push_string("ab");
            state.push_integer(3);
            state.push_string(",");
            state.call_value(0, 3, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"ab,ab,ab".as_slice()));
        }
    }

    #[test]
    fn str_reverse_ascii() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(str_reverse);
            state.push_string("abc");
            state.call_value(0, 1, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"cba".as_slice()));
        }
    }

    #[test]
    fn str_find_plain() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(str_find);
            state.push_string("hello world");
            state.push_string("world");
            state.push_integer(1);
            state.push_boolean(true);
            state.call_value(0, 4, 2).unwrap();
            assert_eq!(state.to_integer_x(1), Some(7));
            assert_eq!(state.to_integer_x(2), Some(11));
        }
    }

    #[test]
    fn str_match_digits() {
        let mut state = LuaState::new(0);
        open_base(&mut state);
        unsafe {
            state.push_light_cfunction(str_match);
            state.push_string("abc123xyz");
            state.push_string("%d+");
            state.call_value(0, 2, 1).unwrap();
            assert_eq!(state.to_lstring(1), Some(b"123".as_slice()));
        }
    }
}
