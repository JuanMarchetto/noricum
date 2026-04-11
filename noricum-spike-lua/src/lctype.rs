//! Lua's `ctype` — ASCII character classification.
//!
//! Ported from `lctype.{h,c}` byte-for-byte. This module is a leaf: it
//! depends only on `core::primitive` types and knows nothing about the
//! rest of the Lua runtime. First victim of Stage 1 of the full port.
//!
//! # Layout
//!
//! The classification table [`LUAI_CTYPE`] has 257 entries. Index 0 is
//! the `EOZ = -1` sentinel (all bits clear); indices `1..=256` correspond
//! to unsigned-byte values `0..=255`. Predicates accept `i32` so the
//! `EOZ` sentinel can be passed through unchanged by the lexer.
//!
//! Each entry is a bitfield:
//!
//! | Bit | Mask | Meaning              |
//! |-----|------|----------------------|
//! | 0   | 0x01 | `ALPHABIT`  — Lua alphabetic (letters + `_`) |
//! | 1   | 0x02 | `DIGITBIT`  — decimal digit |
//! | 2   | 0x04 | `PRINTBIT`  — printable |
//! | 3   | 0x08 | `SPACEBIT`  — whitespace |
//! | 4   | 0x10 | `XDIGITBIT` — hexadecimal digit |
//!
//! `NONA` is `0x00` in the default configuration (matches `lctype.c`
//! when `LUA_UCID` is not defined). We do not support `LUA_UCID`.
//!
//! # Ground truth
//!
//! `lctype.c` and `lctype.h` in this crate's root. The differential test
//! in `tests/differential_lctype.rs` asserts byte-equal tables and
//! predicate parity across every `-1..=255` input.

#![allow(dead_code)]

// -- Bit positions --------------------------------------------------------

pub const ALPHABIT: u8 = 0;
pub const DIGITBIT: u8 = 1;
pub const PRINTBIT: u8 = 2;
pub const SPACEBIT: u8 = 3;
pub const XDIGITBIT: u8 = 4;

// -- Masks ---------------------------------------------------------------

const fn mask(bit: u8) -> u8 {
    1u8 << bit
}

pub const MASK_ALPHA: u8 = mask(ALPHABIT);
pub const MASK_DIGIT: u8 = mask(DIGITBIT);
pub const MASK_PRINT: u8 = mask(PRINTBIT);
pub const MASK_SPACE: u8 = mask(SPACEBIT);
pub const MASK_XDIGIT: u8 = mask(XDIGITBIT);

/// `NONA` entry value. `0x00` in the default non-`LUA_UCID` config.
const NONA: u8 = 0x00;

// -- The table ------------------------------------------------------------

/// 257-byte classification table. `LUAI_CTYPE[0]` is the `EOZ` slot;
/// `LUAI_CTYPE[(c as u8 + 1) as usize]` for byte values.
///
/// Transcribed byte-for-byte from `lctype.c`. The diff test verifies
/// every entry against the C oracle so transcription errors are caught
/// immediately.
#[rustfmt::skip]
pub const LUAI_CTYPE: [u8; 257] = [
    0x00, // EOZ
    // 0.
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00,
    // 1.
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 2.
    0x0c, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    // 3.
    0x16, 0x16, 0x16, 0x16, 0x16, 0x16, 0x16, 0x16,
    0x16, 0x16, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04,
    // 4.
    0x04, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x05,
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    // 5.
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    0x05, 0x05, 0x05, 0x04, 0x04, 0x04, 0x04, 0x05,
    // 6.
    0x04, 0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x05,
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    // 7.
    0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05, 0x05,
    0x05, 0x05, 0x05, 0x04, 0x04, 0x04, 0x04, 0x00,
    // 8.
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    // 9.
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    // a.
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    // b.
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    // c.
    0x00, 0x00, NONA, NONA, NONA, NONA, NONA, NONA,
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    // d.
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    // e.
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    NONA, NONA, NONA, NONA, NONA, NONA, NONA, NONA,
    // f.
    NONA, NONA, NONA, NONA, NONA, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// -- Predicates -----------------------------------------------------------

/// Read the classification byte for character `c`. Returns `None` if
/// `c` is outside the valid `-1..=255` range (EOZ..=u8::MAX).
#[inline]
pub const fn ctype_byte(c: i32) -> Option<u8> {
    if c < -1 || c > 255 {
        None
    } else {
        // `c + 1` is in 0..=256, which fits LUAI_CTYPE's 257 entries.
        Some(LUAI_CTYPE[(c + 1) as usize])
    }
}

/// Internal helper mirroring `testprop(c, p)` in `lctype.h`.
#[inline]
const fn test_prop(c: i32, prop: u8) -> bool {
    match ctype_byte(c) {
        Some(b) => (b & prop) != 0,
        None => false,
    }
}

/// `lislalpha` — Lua alphabetic: ASCII letter or `_`.
#[inline]
pub const fn lislalpha(c: i32) -> bool {
    test_prop(c, MASK_ALPHA)
}

/// `lislalnum` — Lua alphanumeric: letter, digit, or `_`.
#[inline]
pub const fn lislalnum(c: i32) -> bool {
    test_prop(c, MASK_ALPHA | MASK_DIGIT)
}

/// `lisdigit` — decimal digit `'0'..='9'`.
#[inline]
pub const fn lisdigit(c: i32) -> bool {
    test_prop(c, MASK_DIGIT)
}

/// `lisspace` — ASCII whitespace as Lua defines it (`\t \n \v \f \r ' '`).
#[inline]
pub const fn lisspace(c: i32) -> bool {
    test_prop(c, MASK_SPACE)
}

/// `lisprint` — printable (includes space).
#[inline]
pub const fn lisprint(c: i32) -> bool {
    test_prop(c, MASK_PRINT)
}

/// `lisxdigit` — hexadecimal digit `'0'..='9' | 'a'..='f' | 'A'..='F'`.
#[inline]
pub const fn lisxdigit(c: i32) -> bool {
    test_prop(c, MASK_XDIGIT)
}

/// `ltolower` — ASCII lowercase conversion with Lua's narrow domain.
/// Only defined for `'A'..='Z'`; every other input is returned unchanged.
///
/// This matches the C macro's `check_exp` precondition: the caller
/// guarantees `c` is an upper-case letter, a lower-case letter, or a
/// character unchanged by the transform. Outside that domain the result
/// is deliberately unspecified — we pass through untouched.
#[inline]
pub const fn ltolower(c: i32) -> i32 {
    if c >= b'A' as i32 && c <= b'Z' as i32 {
        c | 0x20
    } else {
        c
    }
}

// -- Unit tests -----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eoz_is_false_for_every_predicate() {
        assert!(!lislalpha(-1));
        assert!(!lislalnum(-1));
        assert!(!lisdigit(-1));
        assert!(!lisspace(-1));
        assert!(!lisprint(-1));
        assert!(!lisxdigit(-1));
    }

    #[test]
    fn digit_range_is_tight() {
        for c in b'0'..=b'9' {
            assert!(lisdigit(c as i32));
            assert!(lislalnum(c as i32));
            assert!(lisxdigit(c as i32));
            assert!(lisprint(c as i32));
            assert!(!lislalpha(c as i32));
        }
    }

    #[test]
    fn letters_and_underscore_are_lualpha() {
        for c in b'a'..=b'z' {
            assert!(lislalpha(c as i32), "{}", c as char);
        }
        for c in b'A'..=b'Z' {
            assert!(lislalpha(c as i32), "{}", c as char);
        }
        assert!(lislalpha(b'_' as i32));
    }

    #[test]
    fn whitespace_matches_c_lua() {
        for c in [b'\t', b'\n', 0x0b, 0x0c, b'\r', b' '] {
            assert!(lisspace(c as i32), "char 0x{c:02x}");
        }
        assert!(!lisspace(b'a' as i32));
        assert!(!lisspace(b'0' as i32));
    }

    #[test]
    fn ltolower_only_touches_uppercase() {
        for c in b'A'..=b'Z' {
            assert_eq!(ltolower(c as i32), (c + 32) as i32);
        }
        // Unchanged outside 'A'..='Z' (that's the macro's contract).
        assert_eq!(ltolower(b'a' as i32), b'a' as i32);
        assert_eq!(ltolower(b'0' as i32), b'0' as i32);
        assert_eq!(ltolower(-1), -1);
    }

    #[test]
    fn ctype_byte_out_of_range_is_none() {
        assert!(ctype_byte(-2).is_none());
        assert!(ctype_byte(256).is_none());
        assert!(ctype_byte(-1).is_some()); // EOZ is valid
        assert!(ctype_byte(0).is_some());
        assert!(ctype_byte(255).is_some());
    }
}
