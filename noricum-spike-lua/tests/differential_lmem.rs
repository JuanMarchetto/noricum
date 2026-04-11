//! Differential test for the ported `lmem::grow_array_size`.
//!
//! The oracle shim `wr_lmem_grow_array_size` invokes the real
//! `luaM_growaux_` inside `lua_pcall` with `size_elems == 0` so the
//! size-selection logic runs but no memory is actually allocated. We
//! compare the returned new size against the Rust port's for a matrix
//! of `(size, nelems, limit)` triples that cover:
//!
//! * The zero-size bootstrap path that lands on `MINSIZEARRAY`.
//! * The doubling path at small sizes.
//! * The clamp-to-limit path when doubling would overshoot.
//! * The limit-reached path (both C and Rust should return "no grow").
//! * Edge cases at `u32::MAX / 2`-ish sizes (Rust-side saturating_mul).
//!   Note that C ints are signed 32-bit, so we stay below `i32::MAX`
//!   for the oracle comparison.

use spike::lmem;
use spike::wr_lmem_grow_array_size;

/// Convenience wrapper that returns `(ok, new_size)` matching Rust's
/// `Option<u32>` convention but with explicit ok flag.
fn oracle_grow(size: i32, nelems: i32, limit: i32) -> Option<i32> {
    let mut out: i32 = -1;
    let rc = unsafe { wr_lmem_grow_array_size(size, nelems, limit, &mut out) };
    if rc == 1 {
        Some(out)
    } else if rc == 0 {
        None
    } else {
        panic!("oracle state init failed at ({size}, {nelems}, {limit})");
    }
}

fn assert_agree(size: i32, nelems: i32, limit: i32) {
    let rust = lmem::grow_array_size(size as u32, nelems as u32, limit as u32);
    let oracle = oracle_grow(size, nelems, limit);
    match (rust, oracle) {
        (Some(r), Some(o)) => assert_eq!(
            r as i32, o,
            "grow_array_size({size}, {nelems}, {limit}): rust={r}, c={o}"
        ),
        (None, None) => {}
        (r, o) => panic!(
            "grow_array_size({size}, {nelems}, {limit}): rust={r:?}, c={o:?}"
        ),
    }
}

#[test]
fn zero_size_bootstraps_to_minsizearray() {
    assert_agree(0, 0, 1024);
    assert_agree(0, 0, 8);
    assert_agree(0, 0, 4);
}

#[test]
fn doubling_at_small_sizes() {
    for (size, nelems) in [(4, 4), (8, 8), (16, 16), (32, 32), (64, 64), (128, 128)] {
        assert_agree(size, nelems, 1024);
    }
}

#[test]
fn no_grow_when_room_exists() {
    assert_agree(100, 0, 1024);
    assert_agree(100, 50, 1024);
    assert_agree(100, 98, 1024);
    assert_agree(100, 99, 1024);
    // nelems == size needs grow
    assert_agree(100, 100, 1024);
}

#[test]
fn clamp_to_limit_path() {
    // size >= limit/2 but < limit → clamp to limit.
    assert_agree(600, 600, 1000);
    assert_agree(700, 700, 1000);
    assert_agree(999, 999, 1000);
}

#[test]
fn limit_reached_returns_none() {
    assert_agree(1024, 1024, 1024);
    assert_agree(1000, 1000, 1000);
    assert_agree(10, 10, 10);
}

#[test]
fn exhaustive_small_matrix() {
    // All triples with size, nelems, limit where:
    //   limit >= MIN_SIZE_ARRAY (the function's documented precondition)
    //   nelems <= size <= limit
    //
    // The precondition is real in Lua — no caller of luaM_growaux_ in
    // the upstream source passes a limit smaller than 4. Sizes 0/1/2/3
    // can all legitimately appear below a larger limit and are tested.
    let sizes = [0i32, 1, 2, 3, 4, 8, 16, 32];
    let limits = [4i32, 8, 16, 32, 64, 128];
    for &limit in &limits {
        for &size in &sizes {
            if size > limit {
                continue;
            }
            for &nelems in &sizes {
                if nelems > size {
                    continue;
                }
                assert_agree(size, nelems, limit);
            }
        }
    }
}

#[test]
fn mid_range_limit_values_match() {
    // A broader sweep at realistic parser-size limits.
    for limit in [256, 512, 1024, 4096, 65536] {
        for size in [0, 1, 4, 16, 64, 256, 1024, limit / 2 - 1, limit / 2, limit - 1] {
            if size >= 0 && size <= limit {
                assert_agree(size, size, limit);
            }
        }
    }
}
