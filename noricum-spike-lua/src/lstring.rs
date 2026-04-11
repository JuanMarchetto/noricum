//! Lua string hashing and interning.
//!
//! Stage 2 / commit 4. Ports the minimum of `lstring.c` the rest of
//! the stage depends on:
//!
//! * [`hash_bytes`] — byte-exact port of `luaS_hash`. Bytecode-compat
//!   (decision 6) demands this match C for any given seed; the diff
//!   test hammers it against the real C function.
//! * [`LUAI_MAXSHORTLEN`] — short/long string threshold.
//! * [`intern_short`] / [`new_long`] / [`long_string_hash`] —
//!   extensions to [`GlobalState`] that handle interning, long-string
//!   allocation, and lazy long-string hash computation.
//!
//! NOT ported in Stage 2:
//!
//! * `luaS_resize` (string table rehash) — our intern table is a
//!   `HashMap<u32, Vec<StringHandle>>`, which rehashes itself.
//! * `luaS_clearcache` (API string cache) — the API cache lives
//!   on `GlobalState` but is a GC optimization, not correctness.
//!   Stage 3.
//! * `luaS_remove` — string removal on finalization. Stage 3.
//! * External strings (`luaS_newextlstr`, `luaS_normstr`) — a Lua 5.4
//!   extension for zero-copy string data owned outside Lua. Rarely
//!   used and not bytecode-visible. Stage 9 at earliest.

#![allow(dead_code)]

use crate::contract::{GlobalState, LuaString, StringHandle};

/// Short-string length threshold. Strings at most this many bytes are
/// interned in the string table; longer strings are allocated
/// individually and hashed lazily. Matches `LUAI_MAXSHORTLEN` in
/// `lstring.h`.
pub const LUAI_MAXSHORTLEN: usize = 40;

/// Byte-exact port of `luaS_hash` in `lstring.c`:
///
/// ```c
/// unsigned int h = seed ^ cast_uint(l);
/// for (; l > 0; l--)
///   h ^= ((h<<5) + (h>>2) + cast_byte(str[l - 1]));
/// ```
///
/// The result must match the C implementation byte-for-byte for every
/// `(bytes, seed)` pair, because `.luac` chunks encode short-string
/// hashes (decision 6). The differential test invokes `luaS_hash`
/// through the real Lua oracle and compares.
///
/// All arithmetic is `u32` wrapping. Rust's `<<` and `>>` on integer
/// types silently shift out bits beyond the width, matching C's
/// unsigned shifts. Shift counts are constants (`5`, `2`) so the
/// panic-on-overflow-shift rule cannot fire.
pub fn hash_bytes(bytes: &[u8], seed: u32) -> u32 {
    let mut h: u32 = seed ^ (bytes.len() as u32);
    let mut l = bytes.len();
    while l > 0 {
        let term = (h << 5)
            .wrapping_add(h >> 2)
            .wrapping_add(bytes[l - 1] as u32);
        h ^= term;
        l -= 1;
    }
    h
}

impl GlobalState {
    /// Intern a short string. Looks up the string's hash in the
    /// string table; reuses the existing handle on a match, otherwise
    /// allocates a new [`LuaString`] and registers it.
    ///
    /// # Panics
    ///
    /// Panics if `bytes.len() > LUAI_MAXSHORTLEN`. Callers pick the
    /// short/long branch themselves via [`new_string`].
    pub fn intern_short(&mut self, bytes: &[u8], seed: u32) -> StringHandle {
        assert!(
            bytes.len() <= LUAI_MAXSHORTLEN,
            "intern_short called on a {}-byte string; max is {}",
            bytes.len(),
            LUAI_MAXSHORTLEN
        );
        let hash = hash_bytes(bytes, seed);

        // Snapshot the bucket to avoid holding a borrow of `self`
        // while allocating new strings further down. StringHandle is
        // Copy and buckets are short (collisions are rare in a healthy
        // table), so the clone is effectively free.
        let bucket: Vec<StringHandle> = self
            .string_intern
            .get(&hash)
            .cloned()
            .unwrap_or_default();
        for handle in bucket {
            if self.heap.string(handle).bytes == bytes {
                return handle;
            }
        }

        // Not found — allocate a fresh LuaString.
        let s = LuaString {
            bytes: bytes.to_vec(),
            hash,
            reserved: 0,
            is_long: false,
            hash_ready: true,
        };
        let handle = self.heap.alloc_string(s);
        self.string_intern.entry(hash).or_default().push(handle);
        handle
    }

    /// Allocate a new long (non-interned) string. The hash is stored
    /// as `seed` and marked "not ready"; the first call to
    /// [`long_string_hash`] computes and caches the real hash.
    ///
    /// Matches the `luaS_createlngstrobj` / `luaS_hashlongstr` pair in
    /// `lstring.c`.
    pub fn new_long(&mut self, bytes: Vec<u8>, seed: u32) -> StringHandle {
        let s = LuaString {
            bytes,
            hash: seed, // placeholder; replaced on first hash request
            reserved: 0,
            is_long: true,
            hash_ready: false,
        };
        self.heap.alloc_string(s)
    }

    /// Lazily compute and cache a long string's hash. Matches
    /// `luaS_hashlongstr`.
    ///
    /// # Panics
    ///
    /// Panics in debug if `handle` points to a short string — C's
    /// `lua_assert(ts->tt == LUA_VLNGSTR)` documents the same domain.
    pub fn long_string_hash(&mut self, handle: StringHandle, seed: u32) -> u32 {
        let s = self.heap.string_mut(handle);
        debug_assert!(s.is_long, "long_string_hash called on a short string");
        if !s.hash_ready {
            // Recompute the hash against `seed`. We re-borrow because
            // hash_bytes takes a &[u8] and we need to release the mut
            // borrow on `s` first.
            let bytes_copy = s.bytes.clone();
            let h = hash_bytes(&bytes_copy, seed);
            let s = self.heap.string_mut(handle);
            s.hash = h;
            s.hash_ready = true;
            h
        } else {
            s.hash
        }
    }

    /// Pick the right constructor automatically based on length.
    /// Short strings are interned; long strings are allocated fresh.
    ///
    /// Matches `luaS_newlstr` in `lstring.c`.
    pub fn new_string(&mut self, bytes: &[u8], seed: u32) -> StringHandle {
        if bytes.len() <= LUAI_MAXSHORTLEN {
            self.intern_short(bytes, seed)
        } else {
            self.new_long(bytes.to_vec(), seed)
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_hashes_to_seed() {
        // For the empty string l = 0, the loop body never runs, so
        // h = seed ^ 0 = seed.
        assert_eq!(hash_bytes(b"", 0), 0);
        assert_eq!(hash_bytes(b"", 0xDEADBEEF), 0xDEADBEEF);
    }

    #[test]
    fn hash_is_deterministic_for_same_seed() {
        let s = b"hello, world";
        assert_eq!(hash_bytes(s, 42), hash_bytes(s, 42));
    }

    #[test]
    fn hash_changes_with_seed() {
        let s = b"hello";
        assert_ne!(hash_bytes(s, 0), hash_bytes(s, 1));
    }

    #[test]
    fn hash_changes_with_content() {
        assert_ne!(hash_bytes(b"hello", 42), hash_bytes(b"hEllo", 42));
    }

    #[test]
    fn intern_short_returns_same_handle_on_duplicate() {
        let mut g = GlobalState::default();
        let h1 = g.intern_short(b"print", 0);
        let h2 = g.intern_short(b"print", 0);
        assert_eq!(h1, h2);
        // And one intern bucket holds one entry.
        let hash = hash_bytes(b"print", 0);
        assert_eq!(g.string_intern.get(&hash).unwrap().len(), 1);
    }

    #[test]
    fn intern_short_distinguishes_different_strings() {
        let mut g = GlobalState::default();
        let a = g.intern_short(b"foo", 0);
        let b = g.intern_short(b"bar", 0);
        assert_ne!(a, b);
        assert_eq!(g.heap.string(a).bytes, b"foo");
        assert_eq!(g.heap.string(b).bytes, b"bar");
    }

    #[test]
    fn new_string_picks_long_path_above_threshold() {
        let mut g = GlobalState::default();
        let long = vec![b'a'; LUAI_MAXSHORTLEN + 1];
        let h = g.new_string(&long, 0);
        assert!(g.heap.string(h).is_long);
        assert!(!g.heap.string(h).hash_ready);
    }

    #[test]
    fn new_string_picks_short_path_at_or_below_threshold() {
        let mut g = GlobalState::default();
        let short = vec![b'a'; LUAI_MAXSHORTLEN];
        let h = g.new_string(&short, 0);
        assert!(!g.heap.string(h).is_long);
        assert!(g.heap.string(h).hash_ready);
    }

    #[test]
    fn long_string_hash_is_cached_after_first_call() {
        let mut g = GlobalState::default();
        let h = g.new_long(vec![b'a'; 100], 0x1234_5678);
        let first = g.long_string_hash(h, 0x1234_5678);
        // The hash is now cached; the seed passed to subsequent calls
        // does not matter because we won't recompute.
        assert!(g.heap.string(h).hash_ready);
        let second = g.long_string_hash(h, 0xDEAD_BEEF);
        assert_eq!(first, second);
    }

    #[test]
    fn intern_short_preserves_byte_content_across_reuse() {
        // Non-UTF-8 bytes must round-trip correctly.
        let mut g = GlobalState::default();
        let bytes = b"\xC3\x28\xFF\x00\x01\x02";
        let h1 = g.intern_short(bytes, 0);
        let h2 = g.intern_short(bytes, 0);
        assert_eq!(h1, h2);
        assert_eq!(g.heap.string(h1).bytes, bytes);
    }
}
