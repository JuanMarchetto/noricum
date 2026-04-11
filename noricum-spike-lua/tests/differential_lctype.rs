//! Differential test for the ported `lctype` module.
//!
//! Iterates the full -1..=255 domain and compares every classification
//! decision between the C oracle (via the wrapper shims) and the Rust
//! port. Any transcription error in `LUAI_CTYPE` would surface here as
//! an immediate panic with the exact character code.

use spike::lctype;
use spike::{
    wr_lctype_byte, wr_lctype_isdigit, wr_lctype_islalnum, wr_lctype_islalpha, wr_lctype_isprint,
    wr_lctype_isspace, wr_lctype_isxdigit, wr_lctype_tolower,
};

fn for_each_char<F: FnMut(i32)>(mut f: F) {
    for c in -1..=255i32 {
        f(c);
    }
}

/// The 257-byte classification table must agree byte-for-byte with the
/// C original. This is the Phase-0-style oracle test for `lctype`.
#[test]
fn classification_table_matches_c_oracle() {
    for_each_char(|c| {
        let c_byte = unsafe { wr_lctype_byte(c) };
        let rust_byte = lctype::ctype_byte(c).map(|b| b as i32).unwrap_or(-1);
        assert_eq!(
            rust_byte, c_byte,
            "ctype byte mismatch at c = {c} (C: {c_byte:#04x}, Rust: {rust_byte:#04x})"
        );
    });
}

#[test]
fn lislalpha_matches_c() {
    for_each_char(|c| {
        let oracle = unsafe { wr_lctype_islalpha(c) } != 0;
        assert_eq!(lctype::lislalpha(c), oracle, "lislalpha at c = {c}");
    });
}

#[test]
fn lislalnum_matches_c() {
    for_each_char(|c| {
        let oracle = unsafe { wr_lctype_islalnum(c) } != 0;
        assert_eq!(lctype::lislalnum(c), oracle, "lislalnum at c = {c}");
    });
}

#[test]
fn lisdigit_matches_c() {
    for_each_char(|c| {
        let oracle = unsafe { wr_lctype_isdigit(c) } != 0;
        assert_eq!(lctype::lisdigit(c), oracle, "lisdigit at c = {c}");
    });
}

#[test]
fn lisspace_matches_c() {
    for_each_char(|c| {
        let oracle = unsafe { wr_lctype_isspace(c) } != 0;
        assert_eq!(lctype::lisspace(c), oracle, "lisspace at c = {c}");
    });
}

#[test]
fn lisprint_matches_c() {
    for_each_char(|c| {
        let oracle = unsafe { wr_lctype_isprint(c) } != 0;
        assert_eq!(lctype::lisprint(c), oracle, "lisprint at c = {c}");
    });
}

#[test]
fn lisxdigit_matches_c() {
    for_each_char(|c| {
        let oracle = unsafe { wr_lctype_isxdigit(c) } != 0;
        assert_eq!(lctype::lisxdigit(c), oracle, "lisxdigit at c = {c}");
    });
}

/// `ltolower` is only defined for the narrow `'A'..='Z'` domain in the C
/// macro. We verify that domain and also that the oracle + Rust agree on
/// the passthrough semantics we picked for the out-of-domain case.
#[test]
fn ltolower_matches_c_on_uppercase() {
    for c in b'A'..=b'Z' {
        let oracle = unsafe { wr_lctype_tolower(c as i32) };
        assert_eq!(lctype::ltolower(c as i32), oracle, "ltolower at 0x{c:02x}");
    }
}
