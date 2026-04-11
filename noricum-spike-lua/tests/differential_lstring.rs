//! Differential test for `lstring::hash_bytes`.
//!
//! Bytecode-compat (decision 6) demands byte-exact hash output. This
//! test invokes the real `luaS_hash` through a custom oracle shim —
//! `luaS_hash` is static in `lstring.c`, so the shim instead creates
//! an ephemeral `lua_State` pinned to the requested seed, allocates
//! a string through `luaS_newlstr`, and returns its `ts->hash` (for
//! short strings) or the result of `luaS_hashlongstr` (for long).
//! Both paths route through the same static `luaS_hash` internally.

use spike::lstring::{hash_bytes, LUAI_MAXSHORTLEN};
use spike::wr_lstring_hash;

fn oracle(bytes: &[u8], seed: u32) -> u32 {
    unsafe { wr_lstring_hash(bytes.as_ptr() as *const i8, bytes.len(), seed) }
}

fn assert_match(bytes: &[u8], seed: u32) {
    let c = oracle(bytes, seed);
    let r = hash_bytes(bytes, seed);
    assert_eq!(
        r, c,
        "hash_bytes({bytes:?}, seed={seed}) rust={r:#010x} c={c:#010x}"
    );
}

#[test]
fn empty_string_matches_c() {
    // Seed 0 makes this interesting — C returns 0, which is the
    // one case where an empty string is identical to a "hash failed"
    // sentinel for our oracle. Use a non-zero seed too.
    assert_match(b"", 0);
    assert_match(b"", 1);
    assert_match(b"", 0xDEAD_BEEF);
}

#[test]
fn short_ascii_strings_match_c() {
    let samples: &[&[u8]] = &[
        b"a",
        b"ab",
        b"abc",
        b"hello",
        b"print",
        b"function",
        b"local",
        b"return",
        b"the quick brown fox",
    ];
    for seed in [0u32, 1, 42, 0xDEAD_BEEF, 0xFFFF_FFFF] {
        for &s in samples {
            assert_match(s, seed);
        }
    }
}

#[test]
fn hash_is_stable_across_every_byte_value() {
    // Walk single-byte strings for every u8 value; catches any
    // sign-extension bug in the `cast_byte(str[l - 1])` step.
    for b in 0u8..=255 {
        let s = [b];
        assert_match(&s, 0);
        assert_match(&s, 0xA5A5_A5A5);
    }
}

#[test]
fn threshold_length_short_string_matches_c() {
    // Exactly at the short/long boundary.
    let short = vec![b'x'; LUAI_MAXSHORTLEN];
    assert_match(&short, 12345);
}

#[test]
fn long_strings_match_c() {
    // One byte past the threshold, a realistic length, and a big one.
    for len in [LUAI_MAXSHORTLEN + 1, 100, 1000, 4096] {
        let s: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(31)).collect();
        assert_match(&s, 0);
        assert_match(&s, 0x1234_5678);
    }
}

#[test]
fn hash_changes_with_content_at_same_seed() {
    let a = oracle(b"foo", 0);
    let b = oracle(b"bar", 0);
    assert_ne!(a, b, "different content should collide only by coincidence");
    assert_eq!(a, hash_bytes(b"foo", 0));
    assert_eq!(b, hash_bytes(b"bar", 0));
}

#[test]
fn hash_changes_with_seed_at_same_content() {
    let a = oracle(b"hello", 0);
    let b = oracle(b"hello", 1);
    assert_ne!(a, b);
    assert_eq!(a, hash_bytes(b"hello", 0));
    assert_eq!(b, hash_bytes(b"hello", 1));
}

#[test]
fn non_utf8_bytes_match_c() {
    let weird: &[u8] = &[0xFF, 0xC3, 0x28, 0x00, 0x01, 0x80, 0x7F];
    assert_match(weird, 0);
    assert_match(weird, 0xBEEF);
}
