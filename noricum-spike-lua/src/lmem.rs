//! Lua memory manager — Stage 1 growth-strategy helper.
//!
//! Most of `lmem.c` is deliberately NOT ported:
//!
//! * `luaM_realloc_`, `luaM_malloc_`, `luaM_free_` — our port uses `Vec`
//!   for storage, which owns raw allocation. The user-supplied
//!   `lua_Alloc` callback surface belongs to the drop-in ABI layer
//!   (Stage 4) and lives behind `lua_setallocf` / `lua_getallocf`.
//! * `luaM_saferealloc_`, `luaM_toobig` — produce `LuaError::Memory` at
//!   the allocation boundary; see Stage 3 when the GC lands.
//! * `tryagain` — emergency GC retry; belongs to Stage 3.
//! * `GCdebt` accounting — Stage 3 concern.
//!
//! What IS ported in Stage 1 is the pure, allocator-independent size
//! strategy from `luaM_growaux_`: given a current array size, how much
//! should we grow it to fit one more element without exceeding `limit`?
//! This helper is used by the parser, codegen, and VM during Stage 4/5
//! before the allocator layer exists, and its semantics must match C
//! byte-for-byte — the differential test invokes the real `luaM_growaux_`
//! under `lua_pcall` and compares the new size.
//!
//! Ground truth: `lmem.c` lines 97-119 (`luaM_growaux_`).

#![allow(dead_code)]

/// Minimum array size when growing from zero. Matches `MINSIZEARRAY` in
/// `lmem.c`: avoids the churn of reallocating 1 → 2 → 4 for arrays that
/// routinely reach small-but-nonzero sizes during parsing.
pub const MIN_SIZE_ARRAY: u32 = 4;

/// Compute the next capacity for a growable array.
///
/// Returns `Some(new_size)` if there is room to grow at least one more
/// element, or `None` if `size` has reached `limit` (mirrors the
/// `luaG_runerror("too many %s (limit is %d)")` path in C — callers
/// translate `None` into a `LuaError` with the appropriate "what" name).
///
/// # Preconditions
///
/// `limit >= MIN_SIZE_ARRAY`. Lua only ever calls `luaM_growaux_` with
/// limits like `MAXARG_A (255)`, `MAXARG_B (255)`, `MAX_INT`, etc., so
/// this is always satisfied in practice. The C source asserts the same
/// invariant via `lua_assert(nelems + 1 <= size && size <= limit)` but
/// that assert is a no-op in release builds, making the out-of-domain
/// behavior quietly nonsensical. We enforce the precondition via
/// `debug_assert` instead.
///
/// # Strategy
///
/// 1. If one more element already fits (`nelems + 1 <= size`), no growth
///    is needed; return the current size.
/// 2. If doubling would overshoot `limit` (`size >= limit / 2`):
///    * If we are already at the limit, return `None`.
///    * Otherwise clamp the new size to exactly `limit` (still leaves
///      at least one free slot because the previous branch guaranteed
///      `size < limit`).
/// 3. Otherwise double the size, clamped to at least `MIN_SIZE_ARRAY`.
///
/// Debug assertions verify the invariants `nelems + 1 <= new_size` and
/// `new_size <= limit` on the success path.
pub fn grow_array_size(size: u32, nelems: u32, limit: u32) -> Option<u32> {
    debug_assert!(
        limit >= MIN_SIZE_ARRAY,
        "grow_array_size precondition: limit ({limit}) must be >= MIN_SIZE_ARRAY ({MIN_SIZE_ARRAY})"
    );

    // Does one extra element already fit?
    if nelems.saturating_add(1) <= size {
        return Some(size);
    }

    let new_size = if size >= limit / 2 {
        if size >= limit {
            // Cannot grow even a little — C raises "too many X".
            return None;
        }
        limit
    } else {
        let doubled = size.saturating_mul(2);
        if doubled < MIN_SIZE_ARRAY {
            MIN_SIZE_ARRAY
        } else {
            doubled
        }
    };

    debug_assert!(nelems < new_size, "grow invariant");
    debug_assert!(new_size <= limit, "grow invariant");
    Some(new_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_grows_to_minimum() {
        assert_eq!(grow_array_size(0, 0, 1024), Some(MIN_SIZE_ARRAY));
    }

    #[test]
    fn doubles_when_room_allows() {
        assert_eq!(grow_array_size(4, 4, 1024), Some(8));
        assert_eq!(grow_array_size(8, 8, 1024), Some(16));
        assert_eq!(grow_array_size(16, 16, 1024), Some(32));
        assert_eq!(grow_array_size(100, 100, 1024), Some(200));
    }

    #[test]
    fn no_grow_when_one_more_fits() {
        assert_eq!(grow_array_size(10, 5, 1024), Some(10));
        assert_eq!(grow_array_size(10, 9, 1024), Some(10));
        // But nelems == size does need growth.
        assert_eq!(grow_array_size(10, 10, 1024), Some(20));
    }

    #[test]
    fn clamps_to_limit_when_doubling_overshoots() {
        // size = 600, limit = 1000, limit/2 = 500, size >= 500, so clamp to 1000.
        assert_eq!(grow_array_size(600, 600, 1000), Some(1000));
    }

    #[test]
    fn none_at_limit() {
        assert_eq!(grow_array_size(1024, 1024, 1024), None);
    }

    #[test]
    fn minimum_kicks_in_for_tiny_arrays() {
        // size = 1 → double is 2 but below minimum → bumped to 4.
        assert_eq!(grow_array_size(1, 1, 1024), Some(MIN_SIZE_ARRAY));
        // size = 2 → double is 4 = minimum.
        assert_eq!(grow_array_size(2, 2, 1024), Some(MIN_SIZE_ARRAY));
    }

    #[test]
    fn no_overflow_on_saturating_double() {
        // size that would overflow on naive *2. Large limit lets us
        // exercise the saturating_mul path.
        let big = u32::MAX / 2 + 1;
        let result = grow_array_size(big, big, u32::MAX);
        assert!(result.is_some());
        assert!(result.unwrap() > big);
    }
}
