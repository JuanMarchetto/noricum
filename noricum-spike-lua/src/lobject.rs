//! Lua's `lobject` — value utility functions, numeric helpers, and
//! raw arithmetic over `TValue`.
//!
//! Ported progressively in Stage 2:
//!
//! * Commit 1 — stateless helpers: [`ceillog2`], [`hexavalue`],
//!   [`code_param`]/[`apply_param`], [`utf8esc`].
//! * Commit 2 — TValue-aware raw arithmetic: [`ArithOp`],
//!   [`to_integer_ns`], [`to_number_ns`], [`raw_arith`]. Metamethod
//!   fallback (`luaO_arith` proper) is deferred to Stage 5 when
//!   `luaT_trybinTM` exists.
//!
//! Number-to-string conversion with the `%.14g` roundtrip,
//! `luaO_pushvfstring`, and string parsing (`luaO_str2num`) come in
//! later commits once `LuaString` and the state machinery exist.
//!
//! Ground truth: `lobject.c`, `lobject.h`, `llimits.h`, `lvm.c` (for
//! the arithmetic helpers `luaV_idiv` / `luaV_mod` / `luaV_shiftl` and
//! the float-to-integer conversion `luaV_flttointeger`).

#![allow(dead_code)]

use crate::contract::{LuaError, LuaInteger, LuaNumber, LuaResult, TValue};

// ---------------------------------------------------------------------------
// ceillog2
// ---------------------------------------------------------------------------

/// Lookup table `log_2[i - 1] = ceil(log2(i))` for `i in 1..=256`.
/// Transcribed byte-for-byte from `lobject.c` lines 38-47.
#[rustfmt::skip]
const LOG_2: [u8; 256] = [
    0, 1, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
];

/// Compute `ceil(log2(x))`: the smallest integer `n` such that `x <= (1 << n)`.
///
/// Port of `luaO_ceillog2` in `lobject.c`.
///
/// # Precondition
///
/// `x >= 1`. The C version relies on `x - 1` not underflowing; passing
/// `x = 0` in C produces `40` (garbage). We debug-assert the valid
/// domain instead of reproducing the bogus value.
pub const fn ceillog2(x: u32) -> u8 {
    debug_assert!(x >= 1, "ceillog2 is only defined for x >= 1");
    let mut l: u32 = 0;
    let mut x = x - 1;
    while x >= 256 {
        l += 8;
        x >>= 8;
    }
    (l as u8).wrapping_add(LOG_2[x as usize])
}

// ---------------------------------------------------------------------------
// codeparam / applyparam — Lua 5.4's "float byte" encoding.
// ---------------------------------------------------------------------------

/// `MAX_LMEM` = `i64::MAX` on 64-bit systems where `l_mem` is `ptrdiff_t`.
pub const MAX_LMEM: i64 = i64::MAX;

/// Encode a percentage `p` as a 5-bit mantissa + 4-bit excess-7 exponent
/// float byte. Format: `(eeeexxxx)`.
///
/// * If `p` is large enough to overflow the representable range, returns
///   `0xFF` (the sentinel for "saturated").
/// * Otherwise rounds the per-100 fraction into `p * 128 / 100` and
///   encodes either subnormally (`p < 0x10`, exponent bits zero) or
///   normally (exponent bits = `ceillog2(p + 1) - 4`).
///
/// Port of `luaO_codeparam` in `lobject.c`.
pub const fn code_param(p: u32) -> u8 {
    // Overflow check: (0x1F << (0xF - 7 - 1)) * 100 = (0x1F << 7) * 100
    //                = 3968 * 100 = 396800.
    const OVERFLOW_LIMIT: u64 = (0x1F_u64 << (0xF - 7 - 1)) * 100;
    if (p as u64) >= OVERFLOW_LIMIT {
        return 0xFF;
    }

    // Round the percentage into a 128/100 fraction (ceiling division).
    let p = ((p as u64) * 128).div_ceil(100);
    if p < 0x10 {
        // Subnormal: exponent bits are zero, mantissa holds `p` directly.
        p as u8
    } else {
        // Normal: strip top bit, shift mantissa into the low nibble.
        let log = ceillog2((p + 1) as u32) as u32 - 5;
        (((p >> log) - 0x10) as u8) | (((log + 1) as u8) << 4)
    }
}

/// Apply a float-byte percentage to a magnitude. Multiplies `x` by the
/// 5-bit mantissa and shifts by the 4-bit exponent; on overflow returns
/// [`MAX_LMEM`]. Matches `luaO_applyparam` in `lobject.c`.
///
/// The C comment explains the precision trade-off: for positive
/// exponents the multiply-then-shift order is commutative, but for
/// negative exponents multiplying first preserves more precision as
/// long as it doesn't overflow.
pub const fn apply_param(p: u8, x: i64) -> i64 {
    let m = (p & 0x0F) as i64;
    let mut e = (p >> 4) as i32;
    let m = if e > 0 {
        // Normalized representation: bias the exponent and add the
        // implicit leading 1.
        e -= 1;
        m + 0x10
    } else {
        m
    };
    let e = e - 7; // excess-7 correction
    if e >= 0 {
        // Non-negative exponent: check (x * m) << e for overflow.
        let limit = (MAX_LMEM / 0x1F) >> (e as u32);
        if x < limit {
            (x * m) << (e as u32)
        } else {
            MAX_LMEM
        }
    } else {
        let e_abs = (-e) as u32;
        if x < MAX_LMEM / 0x1F {
            // Multiplying first preserves precision.
            (x * m) >> e_abs
        } else if (x >> e_abs) < MAX_LMEM / 0x1F {
            // Shift first to avoid overflow.
            (x >> e_abs) * m
        } else {
            MAX_LMEM
        }
    }
}

// ---------------------------------------------------------------------------
// hexavalue
// ---------------------------------------------------------------------------

/// Convert an ASCII hex digit to its numeric value (`0..=15`).
///
/// Matches `luaO_hexavalue` in `lobject.c` byte-for-byte for the
/// documented domain (digit or letter), and produces garbage for any
/// other input — the C version does not validate, and we match it in
/// [`hexavalue_raw`]. Prefer [`hexavalue`] which returns `Option<u8>`.
pub const fn hexavalue_raw(c: i32) -> u8 {
    // `lisdigit(c)` is true only for b'0'..=b'9'; we inline the check
    // here so this helper doesn't depend on the `lctype` module.
    if c >= b'0' as i32 && c <= b'9' as i32 {
        (c - b'0' as i32) as u8
    } else {
        // ltolower: narrow-domain lowercase. We already know it's not a
        // digit, so it must be an ASCII letter in the caller's contract.
        let lower = if c >= b'A' as i32 && c <= b'Z' as i32 {
            c | 0x20
        } else {
            c
        };
        ((lower - b'a' as i32) + 10) as u8
    }
}

/// Safe variant of [`hexavalue_raw`]. Returns `None` for non-hex input.
pub const fn hexavalue(c: i32) -> Option<u8> {
    match c {
        c if c >= b'0' as i32 && c <= b'9' as i32 => Some((c - b'0' as i32) as u8),
        c if c >= b'a' as i32 && c <= b'f' as i32 => Some((c - b'a' as i32 + 10) as u8),
        c if c >= b'A' as i32 && c <= b'F' as i32 => Some((c - b'A' as i32 + 10) as u8),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// utf8esc
// ---------------------------------------------------------------------------

/// Size of the buffer `utf8esc` writes into. Matches `UTF8BUFFSZ` in
/// `lobject.h`.
pub const UTF8_BUFF_SZ: usize = 8;

/// UTF-8 encode a codepoint `x` into `buff`, writing backwards from the
/// end of the buffer. Returns the number of bytes written. The encoded
/// bytes occupy `buff[UTF8_BUFF_SZ - n .. UTF8_BUFF_SZ]` where `n` is
/// the returned length.
///
/// Port of `luaO_utf8esc` in `lobject.c`. Handles the full Lua range
/// `0..=0x7FFFFFFF` (Lua extends standard UTF-8 beyond the Unicode
/// range up to 6-byte sequences).
///
/// # Precondition
///
/// `x <= 0x7FFFFFFF` (C has `lua_assert(x <= 0x7FFFFFFFu)`; we mirror).
pub fn utf8esc(buff: &mut [u8; UTF8_BUFF_SZ], x: u32) -> usize {
    debug_assert!(x <= 0x7FFF_FFFF, "utf8esc codepoint out of range");

    let mut n: usize = 1;
    if x < 0x80 {
        // ASCII fast path — one byte, at the end of the buffer.
        buff[UTF8_BUFF_SZ - 1] = x as u8;
    } else {
        let mut mfb: u32 = 0x3F; // max bits in the first byte
        let mut x = x;
        // Add continuation bytes right-to-left.
        loop {
            n += 1;
            buff[UTF8_BUFF_SZ - (n - 1)] = 0x80 | ((x & 0x3F) as u8);
            x >>= 6;
            mfb >>= 1;
            if x <= mfb {
                break;
            }
        }
        // Finalize the leading byte. `!mfb << 1` encodes the length in
        // the high bits (110xxxxx for 2 bytes, 1110xxxx for 3, etc.).
        buff[UTF8_BUFF_SZ - n] = (((!mfb) << 1) as u8) | (x as u8);
    }
    n
}

// ---------------------------------------------------------------------------
// Raw arithmetic — commit 2.
// ---------------------------------------------------------------------------

/// Lua arithmetic operators. Discriminants match the `LUA_OPADD..LUA_OPBNOT`
/// constants in `lua.h` so the opcode identifier can be passed across
/// the FFI boundary unchanged.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add = 0,
    Sub = 1,
    Mul = 2,
    Mod = 3,
    Pow = 4,
    Div = 5,
    IDiv = 6,
    BAnd = 7,
    BOr = 8,
    BXor = 9,
    Shl = 10,
    Shr = 11,
    Unm = 12,
    BNot = 13,
}

impl ArithOp {
    /// Decode a raw opcode byte into an `ArithOp`, returning `None`
    /// for invalid opcodes.
    pub const fn from_raw(op: u8) -> Option<ArithOp> {
        match op {
            0 => Some(ArithOp::Add),
            1 => Some(ArithOp::Sub),
            2 => Some(ArithOp::Mul),
            3 => Some(ArithOp::Mod),
            4 => Some(ArithOp::Pow),
            5 => Some(ArithOp::Div),
            6 => Some(ArithOp::IDiv),
            7 => Some(ArithOp::BAnd),
            8 => Some(ArithOp::BOr),
            9 => Some(ArithOp::BXor),
            10 => Some(ArithOp::Shl),
            11 => Some(ArithOp::Shr),
            12 => Some(ArithOp::Unm),
            13 => Some(ArithOp::BNot),
            _ => None,
        }
    }
}

/// Convert a `TValue` to `LuaInteger` without string coercion and
/// without float truncation: a float must be exactly integral and in
/// `[i64::MIN, 2^63)` to convert successfully.
///
/// Matches `luaV_tointegerns` with the default mode
/// `LUA_FLOORN2I == F2Ieq` — which in Lua 5.4 is the "reject
/// non-integral floats" mode, not a floor-then-round mode. Strings
/// are never coerced (the `ns` suffix = "no string").
pub fn to_integer_ns(v: &TValue) -> Option<LuaInteger> {
    match v {
        TValue::Integer(i) => Some(*i),
        TValue::Number(n) => number_to_integer_exact(*n),
        _ => None,
    }
}

/// Convert a `TValue` to `LuaNumber` without string coercion. Integers
/// are promoted to `f64`.
pub fn to_number_ns(v: &TValue) -> Option<LuaNumber> {
    match v {
        TValue::Integer(i) => Some(*i as f64),
        TValue::Number(n) => Some(*n),
        _ => None,
    }
}

/// Convert a float to an integer requiring an exactly-integral value
/// and the `[i64::MIN, 2^63)` range. Matches `luaV_flttointeger` with
/// `F2Ieq` + `lua_numbertointeger` in `llimits.h`.
///
/// Note: `-(i64::MIN as f64)` is `2^63`, which is exactly representable
/// in `f64`, so the range check is clean.
fn number_to_integer_exact(n: f64) -> Option<LuaInteger> {
    if !n.is_finite() {
        return None;
    }
    if n != n.floor() {
        return None;
    }
    let min_f = i64::MIN as f64;
    let one_past_max = -min_f; // exactly 2^63
    if n >= min_f && n < one_past_max {
        Some(n as i64)
    } else {
        None
    }
}

/// Perform a raw arithmetic operation without metamethod fallback.
///
/// Port of `luaO_rawarith` in `lobject.c`. Return codes:
///
/// * `Ok(Some(result))` — the op succeeded.
/// * `Ok(None)` — one or both operands could not be converted to a
///   number of the required kind (matches `return 0` in C, which tells
///   the caller to try the matching `__add` / `__sub` / etc. metamethod
///   instead).
/// * `Err(LuaError::Runtime(..))` — the op raised a runtime error, for
///   instance integer division by zero. The error object is currently
///   a `TValue::Nil` placeholder because string interning lands in a
///   later Stage 2 commit; once it does, the message ("attempt to
///   divide by zero", etc.) will move into the `Runtime` variant.
///
/// For unary operations (`Unm`, `BNot`), callers pass the same value
/// twice — the C code does the same and the shift/bitwise logic
/// ignores `p2`'s numeric value but still requires it to convert.
pub fn raw_arith(op: ArithOp, p1: &TValue, p2: &TValue) -> LuaResult<Option<TValue>> {
    use ArithOp::*;

    match op {
        // Integer-only operations. If either operand is a FLOAT with
        // non-integer value, raise "number has no integer
        // representation" (matches C Lua's luaG_tointerror). This
        // differs from "not a number" — the operand IS numeric, just
        // not convertible losslessly.
        BAnd | BOr | BXor | Shl | Shr | BNot => {
            let i1 = match to_integer_ns(p1) {
                Some(v) => v,
                None if matches!(p1, TValue::Number(_)) => {
                    // VM layer catches Runtime(True) as "no int repr"
                    // (True picked as a cheap sentinel — see
                    // try_arith_with_string_coercion in lvm.rs).
                    return Err(LuaError::Runtime(TValue::True));
                }
                None => return Ok(None),
            };
            let i2 = match to_integer_ns(p2) {
                Some(v) => v,
                None if matches!(p2, TValue::Number(_)) => {
                    return Err(LuaError::Runtime(TValue::True));
                }
                None => return Ok(None),
            };
            int_arith(op, i1, i2).map(|i| Some(TValue::Integer(i)))
        }

        // Float-only operations.
        Div | Pow => {
            let Some(n1) = to_number_ns(p1) else { return Ok(None) };
            let Some(n2) = to_number_ns(p2) else { return Ok(None) };
            Ok(Some(TValue::Number(num_arith(op, n1, n2))))
        }

        // Promotional operations: if both operands are integers, stay
        // in integer domain; otherwise coerce both to float.
        Add | Sub | Mul | Mod | IDiv | Unm => {
            if let (TValue::Integer(i1), TValue::Integer(i2)) = (p1, p2) {
                return int_arith(op, *i1, *i2).map(|i| Some(TValue::Integer(i)));
            }
            let Some(n1) = to_number_ns(p1) else { return Ok(None) };
            let Some(n2) = to_number_ns(p2) else { return Ok(None) };
            Ok(Some(TValue::Number(num_arith(op, n1, n2))))
        }
    }
}

/// Integer branch of `intarith` in `lobject.c`. All additive ops use
/// wrapping arithmetic (matching Lua's `intop` macro, which casts
/// through unsigned); `Mod`/`IDiv` apply floor-division correction;
/// `Shl`/`Shr` use logical shifts with out-of-range semantics matching
/// `luaV_shiftl`.
fn int_arith(op: ArithOp, v1: LuaInteger, v2: LuaInteger) -> LuaResult<LuaInteger> {
    use ArithOp::*;
    match op {
        Add => Ok(v1.wrapping_add(v2)),
        Sub => Ok(v1.wrapping_sub(v2)),
        Mul => Ok(v1.wrapping_mul(v2)),
        Mod => int_mod(v1, v2),
        IDiv => int_idiv(v1, v2),
        BAnd => Ok(v1 & v2),
        BOr => Ok(v1 | v2),
        BXor => Ok(v1 ^ v2),
        Shl => Ok(int_shiftl(v1, v2)),
        Shr => Ok(int_shiftl(v1, v2.wrapping_neg())),
        Unm => Ok(v1.wrapping_neg()),
        BNot => Ok(!v1),
        Div | Pow => unreachable!("float-only op dispatched to int_arith"),
    }
}

/// Float branch of `numarith` in `lobject.c`. Uses the Lua-defined
/// modulo adjustment (`luai_nummod`) and the `b == 2 -> a*a` shortcut
/// for `Pow` (`luai_numpow`). All other ops are direct `f64` operators.
fn num_arith(op: ArithOp, a: LuaNumber, b: LuaNumber) -> LuaNumber {
    use ArithOp::*;
    match op {
        Add => a + b,
        Sub => a - b,
        Mul => a * b,
        Div => a / b,
        Pow => {
            if b == 2.0 {
                a * a
            } else {
                a.powf(b)
            }
        }
        IDiv => (a / b).floor(),
        Mod => num_mod(a, b),
        Unm => -a,
        BAnd | BOr | BXor | Shl | Shr | BNot => {
            unreachable!("int-only op dispatched to num_arith")
        }
    }
}

/// Floor-based integer division. Matches `luaV_idiv` in `lvm.c`:
///
/// * Raises on `n == 0`.
/// * Returns `-m` (wrapping) when `n == -1` to avoid the
///   `i64::MIN / -1` overflow.
/// * Otherwise performs C's truncating division and adjusts toward
///   negative infinity when the signs differ and the remainder is
///   non-zero.
fn int_idiv(m: LuaInteger, n: LuaInteger) -> LuaResult<LuaInteger> {
    if n == 0 {
        // Nil-payload sentinel; VM dispatcher rewrites it into a
        // proper "attempt to perform 'n//0'" string with source:line.
        return Err(LuaError::Runtime(TValue::Nil));
    }
    if n == -1 {
        return Ok(m.wrapping_neg());
    }
    let q = m / n;
    if (m ^ n) < 0 && m % n != 0 {
        Ok(q - 1)
    } else {
        Ok(q)
    }
}

/// Floor-based integer modulo. Matches `luaV_mod` in `lvm.c`.
fn int_mod(m: LuaInteger, n: LuaInteger) -> LuaResult<LuaInteger> {
    if n == 0 {
        return Err(LuaError::Runtime(TValue::Nil));
    }
    if n == -1 {
        return Ok(0);
    }
    let r = m % n;
    if r != 0 && (r ^ n) < 0 {
        Ok(r + n)
    } else {
        Ok(r)
    }
}

/// Logical shift-left. Port of `luaV_shiftl` in `lvm.c`. Negative `y`
/// is a shift-right; `|y| >= 64` collapses to zero; otherwise the shift
/// is logical (operates on the `u64` bit pattern).
fn int_shiftl(x: LuaInteger, y: LuaInteger) -> LuaInteger {
    const NBITS: i64 = 64;
    if y >= 0 {
        if y >= NBITS {
            0
        } else {
            ((x as u64) << (y as u32)) as i64
        }
    } else if y <= -NBITS {
        0
    } else {
        ((x as u64) >> ((-y) as u32)) as i64
    }
}

/// Port of `luai_nummod`: `fmod(a, b)` adjusted to follow Lua's
/// floor-division semantics, matching the C macro in `llimits.h`.
fn num_mod(a: LuaNumber, b: LuaNumber) -> LuaNumber {
    let m = a % b;
    if (m > 0.0 && b < 0.0) || (m < 0.0 && b > 0.0) {
        m + b
    } else {
        m
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceillog2_basics() {
        assert_eq!(ceillog2(1), 0);
        assert_eq!(ceillog2(2), 1);
        assert_eq!(ceillog2(3), 2);
        assert_eq!(ceillog2(4), 2);
        assert_eq!(ceillog2(5), 3);
        assert_eq!(ceillog2(8), 3);
        assert_eq!(ceillog2(9), 4);
        assert_eq!(ceillog2(256), 8);
        assert_eq!(ceillog2(257), 9);
        assert_eq!(ceillog2(65536), 16);
        assert_eq!(ceillog2(65537), 17);
    }

    #[test]
    fn hexavalue_is_total_over_hex_chars() {
        for (c, expected) in [
            (b'0' as i32, 0),
            (b'9' as i32, 9),
            (b'a' as i32, 10),
            (b'f' as i32, 15),
            (b'A' as i32, 10),
            (b'F' as i32, 15),
        ] {
            assert_eq!(hexavalue(c), Some(expected));
            assert_eq!(hexavalue_raw(c), expected);
        }
    }

    #[test]
    fn hexavalue_rejects_non_hex() {
        assert_eq!(hexavalue(b'g' as i32), None);
        assert_eq!(hexavalue(b'/' as i32), None);
        assert_eq!(hexavalue(-1), None);
    }

    #[test]
    fn code_param_saturates_beyond_range() {
        // Exactly at the overflow limit returns 0xFF.
        const LIMIT: u32 = (0x1F_u32 << 7) * 100;
        assert_eq!(code_param(LIMIT), 0xFF);
        assert_eq!(code_param(LIMIT + 1), 0xFF);
    }

    #[test]
    fn code_param_apply_param_roundtrip_shape() {
        // Not a strict identity — code_param is lossy — but the shape
        // is: apply_param(code_param(100), 100) should be close to 100.
        let p = code_param(100);
        let applied = apply_param(p, 100);
        assert!((95..=105).contains(&applied), "{applied}");
    }

    #[test]
    fn utf8esc_ascii_path() {
        let mut buff = [0u8; UTF8_BUFF_SZ];
        let n = utf8esc(&mut buff, b'A' as u32);
        assert_eq!(n, 1);
        assert_eq!(buff[UTF8_BUFF_SZ - 1], b'A');
    }

    #[test]
    fn utf8esc_two_byte_path() {
        // U+00A9 (copyright sign) = 0xC2 0xA9
        let mut buff = [0u8; UTF8_BUFF_SZ];
        let n = utf8esc(&mut buff, 0x00A9);
        assert_eq!(n, 2);
        assert_eq!(&buff[UTF8_BUFF_SZ - 2..], &[0xC2, 0xA9]);
    }

    #[test]
    fn utf8esc_three_byte_path() {
        // U+20AC (euro sign) = 0xE2 0x82 0xAC
        let mut buff = [0u8; UTF8_BUFF_SZ];
        let n = utf8esc(&mut buff, 0x20AC);
        assert_eq!(n, 3);
        assert_eq!(&buff[UTF8_BUFF_SZ - 3..], &[0xE2, 0x82, 0xAC]);
    }

    #[test]
    fn utf8esc_four_byte_path() {
        // U+1F600 (grinning face emoji) = 0xF0 0x9F 0x98 0x80
        let mut buff = [0u8; UTF8_BUFF_SZ];
        let n = utf8esc(&mut buff, 0x1F600);
        assert_eq!(n, 4);
        assert_eq!(&buff[UTF8_BUFF_SZ - 4..], &[0xF0, 0x9F, 0x98, 0x80]);
    }

    // -- raw_arith invariants --------------------------------------------

    #[test]
    fn integer_add_stays_integer() {
        let r = raw_arith(ArithOp::Add, &TValue::Integer(5), &TValue::Integer(3)).unwrap();
        assert!(matches!(r, Some(TValue::Integer(8))));
    }

    #[test]
    fn mixed_int_float_add_promotes_to_float() {
        let r = raw_arith(ArithOp::Add, &TValue::Integer(5), &TValue::Number(2.5)).unwrap();
        assert!(matches!(r, Some(TValue::Number(n)) if n == 7.5));
    }

    #[test]
    fn div_always_produces_float() {
        let r = raw_arith(ArithOp::Div, &TValue::Integer(10), &TValue::Integer(4)).unwrap();
        assert!(matches!(r, Some(TValue::Number(n)) if n == 2.5));
    }

    #[test]
    fn idiv_of_negatives_is_floor() {
        // -5 // 3 = -2 (floor of -5/3 = -1.67)
        let r = raw_arith(ArithOp::IDiv, &TValue::Integer(-5), &TValue::Integer(3)).unwrap();
        assert!(matches!(r, Some(TValue::Integer(-2))));
    }

    #[test]
    fn mod_of_negatives_follows_floor_division() {
        // -5 % 3 = 1 (not -2 like C)
        let r = raw_arith(ArithOp::Mod, &TValue::Integer(-5), &TValue::Integer(3)).unwrap();
        assert!(matches!(r, Some(TValue::Integer(1))));
    }

    #[test]
    fn idiv_by_zero_is_runtime_error() {
        let r = raw_arith(ArithOp::IDiv, &TValue::Integer(7), &TValue::Integer(0));
        assert!(matches!(r, Err(LuaError::Runtime(_))));
    }

    #[test]
    fn idiv_int_min_by_minus_one_does_not_overflow() {
        let r = raw_arith(
            ArithOp::IDiv,
            &TValue::Integer(i64::MIN),
            &TValue::Integer(-1),
        )
        .unwrap();
        // wrapping_neg(i64::MIN) == i64::MIN
        assert!(matches!(r, Some(TValue::Integer(i)) if i == i64::MIN));
    }

    #[test]
    fn shift_by_huge_amount_yields_zero() {
        let r = raw_arith(ArithOp::Shl, &TValue::Integer(1), &TValue::Integer(64)).unwrap();
        assert!(matches!(r, Some(TValue::Integer(0))));
        let r = raw_arith(ArithOp::Shr, &TValue::Integer(1), &TValue::Integer(64)).unwrap();
        assert!(matches!(r, Some(TValue::Integer(0))));
    }

    #[test]
    fn shift_is_logical_not_arithmetic() {
        // -1 >> 1 should be a large positive number (u64 shift), not -1.
        let r = raw_arith(ArithOp::Shr, &TValue::Integer(-1), &TValue::Integer(1)).unwrap();
        assert!(matches!(r, Some(TValue::Integer(i)) if i == (i64::MAX)));
    }

    #[test]
    fn pow_uses_square_shortcut_for_exponent_two() {
        let r = raw_arith(ArithOp::Pow, &TValue::Number(3.0), &TValue::Number(2.0)).unwrap();
        assert!(matches!(r, Some(TValue::Number(n)) if n == 9.0));
    }

    #[test]
    fn band_on_float_with_integral_value_succeeds() {
        let r = raw_arith(ArithOp::BAnd, &TValue::Number(12.0), &TValue::Number(10.0)).unwrap();
        assert!(matches!(r, Some(TValue::Integer(8))));
    }

    #[test]
    fn band_on_non_integral_float_raises_integer_representation_error() {
        // Mirrors C Lua's luaG_tointerror: bitwise op on a non-integer
        // float is now a hard runtime error (signaled via Runtime(True)
        // sentinel) rather than returning None for metamethod fallback.
        let r = raw_arith(ArithOp::BAnd, &TValue::Number(1.5), &TValue::Integer(3));
        assert!(matches!(r, Err(LuaError::Runtime(TValue::True))));
    }

    #[test]
    fn string_operands_fail_to_convert() {
        // TValue::Nil stands in for any non-number value here. Real
        // string operands will trigger the same path once lstring
        // lands in a later Stage 2 commit.
        let r = raw_arith(ArithOp::Add, &TValue::Nil, &TValue::Integer(1)).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn number_to_integer_exact_rejects_fraction() {
        assert_eq!(number_to_integer_exact(1.5), None);
        assert_eq!(number_to_integer_exact(-0.1), None);
    }

    #[test]
    fn number_to_integer_exact_rejects_nan_and_inf() {
        assert_eq!(number_to_integer_exact(f64::NAN), None);
        assert_eq!(number_to_integer_exact(f64::INFINITY), None);
        assert_eq!(number_to_integer_exact(f64::NEG_INFINITY), None);
    }

    #[test]
    fn number_to_integer_exact_boundary_values() {
        assert_eq!(number_to_integer_exact(i64::MIN as f64), Some(i64::MIN));
        // 2^63 is exactly representable but out of range.
        assert_eq!(number_to_integer_exact(-(i64::MIN as f64)), None);
        // One below: 2^63 - 2^11 (next representable f64 below 2^63).
        let just_under = (-(i64::MIN as f64)) - (1u64 << 11) as f64;
        assert!(number_to_integer_exact(just_under).is_some());
    }
}
