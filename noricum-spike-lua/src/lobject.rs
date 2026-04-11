//! Lua's `lobject` — value utility functions and numeric helpers.
//!
//! This module hosts the Stage 2 port of the pure, stateless helpers
//! from `lobject.c`. The heavy lifting — TValue-aware arithmetic
//! (`luaO_arith`/`luaO_rawarith`), number-to-string conversion with
//! the `%.14g` roundtrip, `luaO_pushvfstring` and friends — lives in
//! later commits once `LuaString` and the state machinery exist.
//!
//! Ported in the first lobject commit:
//!
//! * [`ceillog2`] — `ceil(log2(x))` via a 256-byte lookup table.
//! * [`hexavalue`] — ASCII hex digit → numeric value (0..=15).
//! * [`code_param`] / [`apply_param`] — Lua's "float byte" encoding
//!   used by GC tuning knobs and some opcodes.
//! * [`utf8esc`] — UTF-8 encode a codepoint (0..=0x7FFFFFFF) into an
//!   8-byte buffer, written backwards.
//!
//! Ground truth: `lobject.c`, `lobject.h`, `llimits.h`.

#![allow(dead_code)]

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
}
