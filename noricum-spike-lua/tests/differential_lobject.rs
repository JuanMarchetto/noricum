//! Differential test for the Stage 2 `lobject` utility helpers.
//!
//! All five helpers (`ceillog2`, `hexavalue`, `code_param`,
//! `apply_param`, `utf8esc`) call into the real `luaO_*` functions via
//! shims defined in `wrapper.c`. None of them need a `lua_State`.

use spike::lobject;
use spike::{
    wr_lobject_applyparam, wr_lobject_ceillog2, wr_lobject_codeparam, wr_lobject_hexavalue,
    wr_lobject_utf8esc,
};

// -- ceillog2 --------------------------------------------------------------

#[test]
fn ceillog2_matches_c_on_small_range() {
    for x in 1u32..=256 {
        let c = unsafe { wr_lobject_ceillog2(x) };
        let r = lobject::ceillog2(x) as u32;
        assert_eq!(r, c, "ceillog2({x}): rust={r}, c={c}");
    }
}

#[test]
fn ceillog2_matches_c_up_to_32_bits() {
    // Walk powers of two and off-by-one neighbours across the full
    // 32-bit range. Exhaustive iteration would be 4 billion calls.
    for shift in 0..32 {
        let p = 1u32 << shift;
        for &x in &[p.saturating_sub(1).max(1), p, p.saturating_add(1)] {
            let c = unsafe { wr_lobject_ceillog2(x) };
            let r = lobject::ceillog2(x) as u32;
            assert_eq!(r, c, "ceillog2({x}): rust={r}, c={c}");
        }
    }
}

// -- hexavalue -------------------------------------------------------------

#[test]
fn hexavalue_matches_c_on_hex_chars() {
    // hexavalue is undefined in C for non-hex input, so we only
    // compare across the valid domain.
    for c in b'0'..=b'9' {
        let oracle = unsafe { wr_lobject_hexavalue(c as i32) } as u8;
        assert_eq!(lobject::hexavalue_raw(c as i32), oracle);
        assert_eq!(lobject::hexavalue(c as i32), Some(oracle));
    }
    for c in b'a'..=b'f' {
        let oracle = unsafe { wr_lobject_hexavalue(c as i32) } as u8;
        assert_eq!(lobject::hexavalue_raw(c as i32), oracle);
        assert_eq!(lobject::hexavalue(c as i32), Some(oracle));
    }
    for c in b'A'..=b'F' {
        let oracle = unsafe { wr_lobject_hexavalue(c as i32) } as u8;
        assert_eq!(lobject::hexavalue_raw(c as i32), oracle);
        assert_eq!(lobject::hexavalue(c as i32), Some(oracle));
    }
}

// -- code_param / apply_param ---------------------------------------------

#[test]
fn code_param_matches_c_across_representative_range() {
    // code_param is used by GC tuning (pause, stepmul, ...) and
    // NEWTABLE hints. Realistic inputs go from 0 up to several thousand
    // percent; we walk more densely at the low end where precision
    // matters and include the overflow saturation point.
    let mut samples: Vec<u32> = (0..=200).collect();
    samples.extend((200..=10_000).step_by(37));
    samples.extend((10_000..=500_000).step_by(1001));
    samples.push(u32::MAX / 8); // far above overflow
    for p in samples {
        let c = unsafe { wr_lobject_codeparam(p) };
        let r = lobject::code_param(p) as u32;
        assert_eq!(r, c, "codeparam({p})");
    }
}

#[test]
fn apply_param_matches_c_cross_product() {
    // Walk every possible float-byte (0..=255) crossed with a handful
    // of magnitudes spanning the integer range.
    let magnitudes = [
        0i64,
        1,
        10,
        100,
        1_000,
        10_000,
        100_000,
        1_000_000,
        1_000_000_000,
        100_000_000_000,
        i64::MAX / 4,
        i64::MAX / 0x20, // just below the overflow threshold
        i64::MAX,
    ];
    for p in 0u32..=255 {
        for &x in &magnitudes {
            let c = unsafe { wr_lobject_applyparam(p, x) };
            let r = lobject::apply_param(p as u8, x);
            assert_eq!(r, c, "applyparam(p={p}, x={x})");
        }
    }
}

// -- utf8esc ---------------------------------------------------------------

fn oracle_utf8(x: u32) -> Vec<u8> {
    let mut buf = [0u8; 8];
    let n = unsafe { wr_lobject_utf8esc(buf.as_mut_ptr(), x) } as usize;
    buf[8 - n..].to_vec()
}

fn rust_utf8(x: u32) -> Vec<u8> {
    let mut buf = [0u8; 8];
    let n = lobject::utf8esc(&mut buf, x);
    buf[8 - n..].to_vec()
}

#[test]
fn utf8esc_matches_c_on_boundary_codepoints() {
    // Boundaries of each UTF-8 length class (Lua extends UTF-8 to 6
    // bytes for codepoints beyond the Unicode range).
    let samples: &[u32] = &[
        0x00,
        0x7F,
        0x80,
        0x7FF,
        0x800,
        0xFFFF,
        0x1_0000,
        0x10_FFFF, // last Unicode
        0x11_0000, // first out-of-Unicode
        0x1F_FFFF,
        0x20_0000,
        0x3FF_FFFF,
        0x400_0000,
        0x7FFF_FFFF,
    ];
    for &x in samples {
        let c = oracle_utf8(x);
        let r = rust_utf8(x);
        assert_eq!(r, c, "utf8esc(U+{x:06X})");
    }
}

#[test]
fn utf8esc_matches_c_on_dense_low_range() {
    // Dense coverage of the ASCII and Latin-1 range.
    for x in 0u32..=0x1000 {
        assert_eq!(rust_utf8(x), oracle_utf8(x), "utf8esc(U+{x:04X})");
    }
}
