//! ltable — hybrid array + hash table, the foundation of every Lua
//! data structure from globals to modules to every `{1, 2, 3}` literal.
//!
//! Port of Lua 5.4's `ltable.c` (1355 LOC C) split across three
//! Stage 4.1 commits:
//!
//! * **4.1.1 (this commit)** — read side. `table_get`,
//!   `table_get_int`, `table_get_shortstr`, `table_get_by_tvalue`,
//!   plus the [`table_key_from_tvalue`] normalizer that converts
//!   any `TValue` into a [`TableKey`] (or rejects it for nil/NaN).
//! * **4.1.2** — write side, with write barriers.
//! * **4.1.3** — length (`luaH_getn` border search) and `next`.
//!
//! ## Rust adaptation
//!
//! C's hybrid table stores integer keys densely in an `array` part
//! and everything else in a collision-chained `Node` hash part.
//! Our port uses a flat [`Vec<TValue>`] for the array part and
//! [`HashMap<TableKey, TValue>`] for the hash part — the HashMap
//! gets us robust hashing and collision resolution without porting
//! 600 lines of open-addressing bookkeeping.
//!
//! Semantics still match C Lua:
//!
//! * Integer keys in the range `1..=array.len()` live in the array
//!   part. Reading a slot whose value is `TValue::Nil` is
//!   indistinguishable from "key absent" — Lua has no way to
//!   observe the difference.
//! * Integer keys outside that range go to the hash part.
//! * Non-integer numeric keys that are exactly-representable
//!   integers (e.g., `2.0`) are normalized to `Integer(2)` and
//!   looked up in the array part when in range.
//! * Float keys that are `NaN` are rejected (panic or `None`).
//! * `nil` is never a valid key.
//!
//! Stage 4.1.1 is strictly read-only; none of these calls mutate
//! the table. Writes land in 4.1.2 and also fire the write barrier
//! from `lgc.rs` so the GC invariant is preserved under mutation.

#![allow(dead_code)]

use crate::contract::{Heap, LuaInteger, StringHandle, TValue, TableHandle, TableKey};
use crate::lobject::to_integer_ns;

// ---------------------------------------------------------------------------
// TValue → TableKey conversion.
// ---------------------------------------------------------------------------

/// Normalize a `TValue` into the matching [`TableKey`] variant, or
/// return `None` if the value isn't a valid Lua table key.
///
/// Rejections (return `None`):
/// * `TValue::Nil` — Lua raises "table index is nil"; we leave the
///   decision of whether to raise to the caller and just report
///   "can't use this as a key".
/// * `TValue::Number(f)` where `f.is_nan()` — Lua raises "table
///   index is NaN" for the same reason (NaN != NaN makes it
///   impossible to look up the value ever again).
///
/// Float keys that are exactly-representable as an `i64` are
/// normalized to `TableKey::Integer(i)` via
/// [`crate::lobject::to_integer_ns`]. This matches C's
/// `luaV_flttointeger` path in `lvm.c` and `rawsetfield` in
/// `ltable.c` — floats like `2.0` and `2` must hash to the same
/// slot so `t[2.0] = 1; print(t[2])` prints `1`.
pub fn table_key_from_tvalue(v: TValue) -> Option<TableKey> {
    match v {
        TValue::Nil => None,
        TValue::False => Some(TableKey::False),
        TValue::True => Some(TableKey::True),
        TValue::Integer(i) => Some(TableKey::Integer(i)),
        TValue::Number(f) => {
            if f.is_nan() {
                None
            } else if let Some(i) = to_integer_ns(&TValue::Number(f)) {
                Some(TableKey::Integer(i))
            } else {
                Some(TableKey::Number(f.to_bits()))
            }
        }
        TValue::LightUserData(p) => Some(TableKey::LightUserData(p as usize)),
        TValue::ShortString(h) => Some(TableKey::ShortString(h)),
        TValue::LongString(h) => Some(TableKey::LongString(h)),
        TValue::Table(h) => Some(TableKey::Table(h)),
        TValue::LuaClosure(h) => Some(TableKey::LuaClosure(h)),
        TValue::LightCFunction(f) => Some(TableKey::LightCFunction(f as usize)),
        TValue::CClosure(h) => Some(TableKey::CClosure(h)),
        TValue::UserData(h) => Some(TableKey::UserData(h)),
        TValue::Thread(h) => Some(TableKey::Thread(h)),
    }
}

// ---------------------------------------------------------------------------
// Read-side accessors on Heap. These match the `luaH_get*` family in
// `ltable.h`. All of them return `Option<TValue>` where `None` means
// "absent from the table", including the Lua-visible case where an
// explicit `TValue::Nil` lives in the array part (slotted nil is
// indistinguishable from absence at the language level).
// ---------------------------------------------------------------------------

impl Heap {
    /// Look up `key` in the table. Returns `None` for keys that
    /// aren't set, aren't valid (nil, NaN), or store `TValue::Nil`
    /// in the array part. Integer keys in the array range prefer
    /// the array part; everything else hits the hash.
    pub fn table_get(&self, handle: TableHandle, key: TValue) -> Option<TValue> {
        // Fast path: normalize once, then dispatch.
        let normalized = table_key_from_tvalue(key)?;
        self.table_get_by_key(handle, &normalized)
    }

    /// Look up `key` (already normalized to a [`TableKey`]) in the
    /// table. The normalized variant lets callers that already
    /// know the key's shape skip the [`table_key_from_tvalue`]
    /// call. Integer keys still check the array part first.
    pub fn table_get_by_key(&self, handle: TableHandle, key: &TableKey) -> Option<TValue> {
        if let TableKey::Integer(i) = key {
            return self.table_get_int(handle, *i);
        }
        self.table_hash_get(handle, key)
    }

    /// Integer-keyed lookup. Checks the array part first if the
    /// index is in range, falls through to the hash otherwise.
    /// Matches `luaH_getint` in `ltable.c`.
    pub fn table_get_int(&self, handle: TableHandle, key: LuaInteger) -> Option<TValue> {
        let t = self.table(handle);
        // Array part range: Lua indices are 1-based, so index `i`
        // maps to array slot `i - 1`. Only positive indices up to
        // the current array length live in the array.
        if key >= 1 && (key as u64) <= t.array.len() as u64 {
            let slot = (key - 1) as usize;
            let value = t.array[slot];
            if matches!(value, TValue::Nil) {
                return None;
            }
            return Some(value);
        }
        // Everything else goes to the hash part.
        self.table_hash_get(handle, &TableKey::Integer(key))
    }

    /// Short-string keyed lookup. Bypasses the array check since
    /// strings can never live in the array part. Matches
    /// `luaH_getshortstr` in `ltable.c`.
    pub fn table_get_shortstr(&self, handle: TableHandle, key: StringHandle) -> Option<TValue> {
        self.table_hash_get(handle, &TableKey::ShortString(key))
    }

    /// Long-string keyed lookup. Same semantics as
    /// [`Heap::table_get_shortstr`] but for long strings.
    pub fn table_get_longstr(&self, handle: TableHandle, key: StringHandle) -> Option<TValue> {
        self.table_hash_get(handle, &TableKey::LongString(key))
    }

    /// Hash-part lookup. Returns `None` for absent keys and for
    /// slots that happen to hold `TValue::Nil` (Lua-visible
    /// absence).
    fn table_hash_get(&self, handle: TableHandle, key: &TableKey) -> Option<TValue> {
        let t = self.table(handle);
        match t.hash.get(key).copied() {
            Some(TValue::Nil) | None => None,
            Some(v) => Some(v),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{LuaString, Table};

    fn fresh_string(bytes: &[u8]) -> LuaString {
        LuaString {
            bytes: bytes.to_vec(),
            hash: 0,
            reserved: 0,
            is_long: false,
            hash_ready: false,
        }
    }

    fn fresh_table_with_array(entries: Vec<TValue>) -> Table {
        Table {
            array: entries,
            ..Table::default()
        }
    }

    // --- table_key_from_tvalue ------------------------------------

    #[test]
    fn table_key_rejects_nil() {
        assert!(table_key_from_tvalue(TValue::Nil).is_none());
    }

    #[test]
    fn table_key_rejects_nan() {
        assert!(table_key_from_tvalue(TValue::Number(f64::NAN)).is_none());
    }

    #[test]
    fn table_key_normalizes_integral_float_to_integer() {
        // 2.0 must hash to the same slot as 2 — otherwise
        // `t[2.0] = 1; print(t[2])` would print nil.
        let k = table_key_from_tvalue(TValue::Number(2.0)).unwrap();
        assert_eq!(k, TableKey::Integer(2));
    }

    #[test]
    fn table_key_keeps_non_integral_float_as_number() {
        let k = table_key_from_tvalue(TValue::Number(2.5)).unwrap();
        match k {
            TableKey::Number(bits) => assert_eq!(bits, 2.5f64.to_bits()),
            _ => panic!("expected Number variant"),
        }
    }

    // --- table_get_int --------------------------------------------

    #[test]
    fn get_int_reads_array_slot_when_in_range() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Integer(20),
            TValue::Integer(30),
        ]));
        assert_eq!(heap.table_get_int(h, 1), Some(TValue::Integer(10)));
        assert_eq!(heap.table_get_int(h, 2), Some(TValue::Integer(20)));
        assert_eq!(heap.table_get_int(h, 3), Some(TValue::Integer(30)));
    }

    #[test]
    fn get_int_returns_none_for_nil_in_array_slot() {
        // A nil in the array is indistinguishable from absence.
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Nil,
            TValue::Integer(30),
        ]));
        assert_eq!(heap.table_get_int(h, 2), None);
    }

    #[test]
    fn get_int_falls_through_to_hash_for_out_of_range_key() {
        let mut heap = Heap::default();
        let mut t = Table::default();
        // Sparse entry at index 1000 — lands in the hash.
        t.hash
            .insert(TableKey::Integer(1000), TValue::Integer(42));
        let h = heap.alloc_table(t);
        assert_eq!(heap.table_get_int(h, 1000), Some(TValue::Integer(42)));
    }

    #[test]
    fn get_int_returns_none_for_missing_key() {
        let heap_with_h = {
            let mut heap = Heap::default();
            let h = heap.alloc_table(Table::default());
            (heap, h)
        };
        let (heap, h) = heap_with_h;
        assert_eq!(heap.table_get_int(h, 1), None);
        assert_eq!(heap.table_get_int(h, -5), None);
    }

    // --- table_get_shortstr ---------------------------------------

    #[test]
    fn get_shortstr_reads_hash_part() {
        let mut heap = Heap::default();
        let key = heap.alloc_string(fresh_string(b"name"));
        let mut t = Table::default();
        t.hash.insert(
            TableKey::ShortString(key),
            TValue::ShortString(key),
        );
        let h = heap.alloc_table(t);
        assert_eq!(
            heap.table_get_shortstr(h, key),
            Some(TValue::ShortString(key))
        );
    }

    // --- table_get (generic) --------------------------------------

    #[test]
    fn generic_get_dispatches_integer_to_array_range() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![TValue::Integer(99)]));
        assert_eq!(
            heap.table_get(h, TValue::Integer(1)),
            Some(TValue::Integer(99))
        );
    }

    #[test]
    fn generic_get_normalizes_float_key_to_integer_lookup() {
        // t[1] = "x"; reading t[1.0] must return the same slot.
        let mut heap = Heap::default();
        let s = heap.alloc_string(fresh_string(b"x"));
        let h = heap.alloc_table(fresh_table_with_array(vec![TValue::ShortString(s)]));
        assert_eq!(
            heap.table_get(h, TValue::Number(1.0)),
            Some(TValue::ShortString(s))
        );
    }

    #[test]
    fn generic_get_rejects_nil_key_with_none() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(Table::default());
        assert_eq!(heap.table_get(h, TValue::Nil), None);
    }

    #[test]
    fn generic_get_rejects_nan_key_with_none() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(Table::default());
        assert_eq!(heap.table_get(h, TValue::Number(f64::NAN)), None);
    }
}
