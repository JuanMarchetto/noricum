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

use crate::contract::{
    GlobalState, Heap, LuaInteger, StringHandle, TValue, TableHandle, TableKey,
};
use crate::lgc::{any_handle_from_tvalue, AnyHandle};
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

    // --- Raw write-side helpers (no barrier) -----------------------
    //
    // These are pure Heap mutations. Public table setters live on
    // `GlobalState` so they can also fire the forward write barrier
    // when the table is black and the new value is a collectable
    // white object. Callers that don't have a live GC cycle (tests,
    // bootstrap) can use these directly.

    /// Raw integer-keyed write. No barrier. Panics on invalid
    /// handle. Nil-value semantics match Lua: in the array range,
    /// a nil stores a hole; outside the array range, a nil removes
    /// the hash entry. The "grow array by one" path extends the
    /// array when `key == array.len() + 1` and the value isn't nil.
    pub fn raw_table_set_int(
        &mut self,
        handle: TableHandle,
        key: LuaInteger,
        value: TValue,
    ) {
        let t = self.table_mut(handle);
        if key >= 1 && (key as u64) <= t.array.len() as u64 {
            t.array[(key - 1) as usize] = value;
            return;
        }
        if key == (t.array.len() as LuaInteger) + 1 && !matches!(value, TValue::Nil) {
            t.array.push(value);
            return;
        }
        // Hash part.
        let k = TableKey::Integer(key);
        if matches!(value, TValue::Nil) {
            t.hash.remove(&k);
        } else {
            t.hash.insert(k, value);
        }
    }

    /// Raw short-string-keyed write. No barrier. String keys never
    /// live in the array part.
    pub fn raw_table_set_shortstr(
        &mut self,
        handle: TableHandle,
        key: StringHandle,
        value: TValue,
    ) {
        self.raw_table_hash_set(handle, TableKey::ShortString(key), value);
    }

    /// Raw long-string-keyed write. No barrier.
    pub fn raw_table_set_longstr(
        &mut self,
        handle: TableHandle,
        key: StringHandle,
        value: TValue,
    ) {
        self.raw_table_hash_set(handle, TableKey::LongString(key), value);
    }

    /// Raw generic write. No barrier. Rejects invalid keys (nil,
    /// NaN) by silently dropping the write — callers at the
    /// higher layer should raise the "table index is nil/NaN"
    /// error before reaching this point.
    pub fn raw_table_set(&mut self, handle: TableHandle, key: TValue, value: TValue) {
        let Some(k) = table_key_from_tvalue(key) else {
            return;
        };
        if let TableKey::Integer(i) = k {
            self.raw_table_set_int(handle, i, value);
            return;
        }
        self.raw_table_hash_set(handle, k, value);
    }

    fn raw_table_hash_set(&mut self, handle: TableHandle, key: TableKey, value: TValue) {
        let t = self.table_mut(handle);
        if matches!(value, TValue::Nil) {
            t.hash.remove(&key);
        } else {
            t.hash.insert(key, value);
        }
    }

    // --- Length (`#t`) and iteration (`next(t)`) -------------------
    //
    // Lua's `#` operator returns a *border*: an index `n` such
    // that `t[n] != nil` and `t[n+1] == nil` (or `n == 0` and
    // `t[1] == nil`). For tables without holes this is the
    // sequence length; for tables with holes it's any border (the
    // choice is implementation-defined). We pick the one the C
    // `luaH_getn` would pick, which keeps Stage 4 compatible with
    // user code that relies on the exact hole semantics.

    /// Return a border for `#handle`. Matches `luaH_getn` in
    /// `ltable.c`. O(log n) in the common no-holes case; O(n)
    /// worst case when the border lives in the hash part.
    pub fn table_len(&self, handle: TableHandle) -> u64 {
        let t = self.table(handle);
        let limit = t.array.len() as u64;
        // Case (1): the array ends in a hole — binary search
        // the array for a border.
        if limit > 0 && matches!(t.array[(limit - 1) as usize], TValue::Nil) {
            let mut lo: u64 = 0;
            let mut hi: u64 = limit;
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if matches!(t.array[(mid - 1) as usize], TValue::Nil) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            return lo;
        }
        // Case (2): the array is either empty or its last slot
        // is non-nil. If the hash doesn't extend the sequence,
        // the array length itself is a border.
        let next_key = TableKey::Integer((limit + 1) as LuaInteger);
        if !t.hash.contains_key(&next_key) {
            return limit;
        }
        // Case (3): the hash extends the sequence. Linear scan.
        // The C version uses a smarter binary search based on
        // `hash_search`; Stage 4.1.3 keeps it simple and Stage 4
        // v2 can optimize if profiling warrants.
        let mut n = limit + 1;
        loop {
            let candidate = TableKey::Integer((n + 1) as LuaInteger);
            if !t.hash.contains_key(&candidate) {
                return n;
            }
            n += 1;
        }
    }

    /// Return the next `(key, value)` pair after `key`, or `None`
    /// at the end of the traversal. `None` as the input key means
    /// "give me the first entry". Matches `luaH_next` in
    /// `ltable.c` — specifically the public part, not the hidden
    /// `findindex` step, since our manual arena doesn't need the
    /// C version's slot-index reuse trick.
    ///
    /// Walking order: array part first (indices 1..=array.len()),
    /// skipping holes, then the hash part in HashMap iteration
    /// order (which is unspecified but stable within a single
    /// traversal as long as the table isn't modified).
    pub fn table_next(
        &self,
        handle: TableHandle,
        key: Option<TableKey>,
    ) -> Option<(TableKey, TValue)> {
        let t = self.table(handle);

        // Decide the array starting index. A None key or a
        // non-integer key (meaning we're already in the hash
        // part) means "scan the whole array from slot 0".
        let array_start = match key {
            None => 0usize,
            Some(TableKey::Integer(i)) if i >= 1 && (i as u64) <= t.array.len() as u64 => {
                // i is an array-part index. Continue from i (slot i in 1-based = array[i]).
                i as usize
            }
            _ => t.array.len(), // Skip array — we're already in the hash part.
        };

        // Scan forward in the array for the next non-nil slot.
        for idx in array_start..t.array.len() {
            let v = t.array[idx];
            if !matches!(v, TValue::Nil) {
                return Some((TableKey::Integer((idx + 1) as LuaInteger), v));
            }
        }

        // Array exhausted. Now walk the hash part.
        match key {
            None => {
                // Return the first hash entry, if any.
                t.hash.iter().next().map(|(k, v)| (*k, *v))
            }
            Some(TableKey::Integer(i)) if i >= 1 && (i as u64) <= t.array.len() as u64 => {
                // We were walking the array and just ran off the
                // end. Start the hash iteration from its beginning.
                t.hash.iter().next().map(|(k, v)| (*k, *v))
            }
            Some(hash_key) => {
                // We were already in the hash part. Find `hash_key`,
                // then return the entry that comes after it in
                // HashMap iteration order. If `hash_key` isn't
                // in the hash at all (caller passed a stale key),
                // return None — Lua's reference manual calls this
                // undefined, and None is the safer choice.
                let mut seen_current = false;
                for (k, v) in t.hash.iter() {
                    if seen_current {
                        return Some((*k, *v));
                    }
                    if *k == hash_key {
                        seen_current = true;
                    }
                }
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Write-side accessors on GlobalState — these wrap the raw Heap
// setters above and fire the forward write barrier when the table is
// black and the new value is a white collectable. Callers should
// prefer these over the raw versions whenever the GC might be mid
// cycle (i.e., always, in production code).
// ---------------------------------------------------------------------------

impl GlobalState {
    /// Set `handle[key] = value`. Fires [`GlobalState::barrier_forward`]
    /// on `(table, value)` afterwards if the value is a collectable
    /// handle, preserving the tri-color invariant under mutation.
    pub fn table_set(&mut self, handle: TableHandle, key: TValue, value: TValue) {
        self.heap.raw_table_set(handle, key, value);
        self.maybe_forward_barrier(handle, value);
    }

    /// Set `handle[key] = value` with a known integer key. Same
    /// semantics as [`GlobalState::table_set`] but skips the
    /// generic-key normalization.
    pub fn table_set_int(
        &mut self,
        handle: TableHandle,
        key: LuaInteger,
        value: TValue,
    ) {
        self.heap.raw_table_set_int(handle, key, value);
        self.maybe_forward_barrier(handle, value);
    }

    /// Set `handle[key] = value` with a known short-string key.
    pub fn table_set_shortstr(
        &mut self,
        handle: TableHandle,
        key: StringHandle,
        value: TValue,
    ) {
        self.heap.raw_table_set_shortstr(handle, key, value);
        self.maybe_forward_barrier(handle, value);
    }

    /// Set `handle[key] = value` with a known long-string key.
    pub fn table_set_longstr(
        &mut self,
        handle: TableHandle,
        key: StringHandle,
        value: TValue,
    ) {
        self.heap.raw_table_set_longstr(handle, key, value);
        self.maybe_forward_barrier(handle, value);
    }

    /// If `value` holds a collectable handle, fire the forward
    /// barrier against the parent table. The barrier itself
    /// short-circuits when the parent isn't black or the child
    /// isn't white, so this is cheap to call unconditionally.
    fn maybe_forward_barrier(&mut self, parent: TableHandle, value: TValue) {
        if let Some(child) = any_handle_from_tvalue(value) {
            self.barrier_forward(AnyHandle::Table(parent), child);
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
    use crate::lgc::{AnyHandle, BLACK, GcState, GRAY, WHITE};

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

    // --- raw_table_set_int (Heap) ---------------------------------

    #[test]
    fn set_int_writes_array_slot_in_range() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(0),
            TValue::Integer(0),
        ]));
        heap.raw_table_set_int(h, 1, TValue::Integer(10));
        heap.raw_table_set_int(h, 2, TValue::Integer(20));
        assert_eq!(heap.table_get_int(h, 1), Some(TValue::Integer(10)));
        assert_eq!(heap.table_get_int(h, 2), Some(TValue::Integer(20)));
    }

    #[test]
    fn set_int_extends_array_when_key_is_one_past_length() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(Table::default());
        heap.raw_table_set_int(h, 1, TValue::Integer(100));
        heap.raw_table_set_int(h, 2, TValue::Integer(200));
        heap.raw_table_set_int(h, 3, TValue::Integer(300));
        assert_eq!(heap.table(h).array.len(), 3);
        assert_eq!(heap.table_get_int(h, 3), Some(TValue::Integer(300)));
    }

    #[test]
    fn set_int_writes_hash_for_out_of_range_key() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(Table::default());
        heap.raw_table_set_int(h, 1000, TValue::Integer(42));
        assert!(heap.table(h).array.is_empty());
        assert_eq!(heap.table_get_int(h, 1000), Some(TValue::Integer(42)));
    }

    #[test]
    fn set_int_with_nil_stores_hole_in_array_but_removes_from_hash() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Integer(20),
        ]));
        heap.raw_table_set_int(h, 2, TValue::Nil);
        // Array slot is now a hole — len unchanged, get returns None.
        assert_eq!(heap.table(h).array.len(), 2);
        assert_eq!(heap.table_get_int(h, 2), None);

        // Hash: set, then nil-set removes.
        heap.raw_table_set_int(h, 1000, TValue::Integer(99));
        assert!(heap.table(h).hash.contains_key(&TableKey::Integer(1000)));
        heap.raw_table_set_int(h, 1000, TValue::Nil);
        assert!(!heap.table(h).hash.contains_key(&TableKey::Integer(1000)));
    }

    #[test]
    fn set_shortstr_writes_hash() {
        let mut heap = Heap::default();
        let key = heap.alloc_string(fresh_string(b"k"));
        let h = heap.alloc_table(Table::default());
        heap.raw_table_set_shortstr(h, key, TValue::Integer(7));
        assert_eq!(
            heap.table_get_shortstr(h, key),
            Some(TValue::Integer(7))
        );
    }

    #[test]
    fn generic_set_rejects_nil_key_silently() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(Table::default());
        heap.raw_table_set(h, TValue::Nil, TValue::Integer(99));
        // Nothing stored.
        assert!(heap.table(h).hash.is_empty());
        assert!(heap.table(h).array.is_empty());
    }

    // --- GlobalState::table_set (barrier-aware) -------------------

    #[test]
    fn barrier_fires_when_black_table_mutates_to_hold_white_child_in_propagate() {
        // Put the GC in Propagate with the parent table painted
        // black and the to-be-stored string painted white. Then
        // set the field. The barrier must mark the string gray
        // and enqueue it on gc_gray.
        let mut g = GlobalState::default();
        let parent = g.heap.alloc_table(Table::default());
        let child = g.heap.alloc_string(fresh_string(b"x"));
        g.heap.marks_tables[parent.slot as usize] = BLACK;
        g.heap.marks_strings[child.slot as usize] = WHITE;
        g.gc_state = GcState::Propagate;

        g.table_set_int(parent, 1, TValue::ShortString(child));

        // Mutation landed.
        assert_eq!(
            g.heap.table_get_int(parent, 1),
            Some(TValue::ShortString(child))
        );
        // Barrier fired.
        assert_eq!(g.heap.marks_strings[child.slot as usize], GRAY);
        assert!(g.gc_gray.contains(&AnyHandle::String(child)));
    }

    #[test]
    fn barrier_is_noop_when_value_is_a_leaf_integer() {
        // Leaf values (integer, bool, nil) don't need the
        // barrier because they're not collectable.
        let mut g = GlobalState::default();
        let parent = g.heap.alloc_table(Table::default());
        g.heap.marks_tables[parent.slot as usize] = BLACK;
        g.gc_state = GcState::Propagate;

        g.table_set_int(parent, 1, TValue::Integer(42));

        // Parent mark byte untouched, no gray queue growth.
        assert_eq!(g.heap.marks_tables[parent.slot as usize], BLACK);
        assert!(g.gc_gray.is_empty());
    }

    #[test]
    fn barrier_is_noop_when_parent_is_not_black() {
        // Gray/white parents don't trip the barrier's fast path.
        let mut g = GlobalState::default();
        let parent = g.heap.alloc_table(Table::default());
        let child = g.heap.alloc_string(fresh_string(b"x"));
        g.heap.marks_tables[parent.slot as usize] = WHITE;
        g.heap.marks_strings[child.slot as usize] = WHITE;
        g.gc_state = GcState::Propagate;

        g.table_set_int(parent, 1, TValue::ShortString(child));

        assert_eq!(g.heap.marks_strings[child.slot as usize], WHITE);
        assert!(g.gc_gray.is_empty());
    }

    // --- table_len (#t) --------------------------------------------

    #[test]
    fn len_empty_table_is_zero() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(Table::default());
        assert_eq!(heap.table_len(h), 0);
    }

    #[test]
    fn len_array_without_holes_returns_array_size() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Integer(20),
            TValue::Integer(30),
        ]));
        assert_eq!(heap.table_len(h), 3);
    }

    #[test]
    fn len_array_with_trailing_hole_returns_border_before_the_hole() {
        // `[10, 20, nil]` — the binary search must find index 2
        // as a border.
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Integer(20),
            TValue::Nil,
        ]));
        assert_eq!(heap.table_len(h), 2);
    }

    #[test]
    fn len_array_of_only_holes_returns_zero() {
        // `[nil, nil, nil]` — only 0 is a valid border per Lua's
        // border definition ("0 is a border if t[1] is nil"),
        // and the binary search correctly lands on it.
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Nil,
            TValue::Nil,
            TValue::Nil,
        ]));
        assert_eq!(heap.table_len(h), 0);
    }

    #[test]
    fn len_extends_through_hash_part_contiguously() {
        // Array fills [1..=2], hash has 3 and 4 but not 5.
        // Border should be 4.
        let mut heap = Heap::default();
        let mut t = fresh_table_with_array(vec![
            TValue::Integer(1),
            TValue::Integer(2),
        ]);
        t.hash.insert(TableKey::Integer(3), TValue::Integer(3));
        t.hash.insert(TableKey::Integer(4), TValue::Integer(4));
        let h = heap.alloc_table(t);
        assert_eq!(heap.table_len(h), 4);
    }

    #[test]
    fn len_full_array_with_gap_to_hash_stops_at_array_length() {
        // Array fills [1..=3], hash has 5 (not 4). Border is 3
        // because slot 4 is missing.
        let mut heap = Heap::default();
        let mut t = fresh_table_with_array(vec![
            TValue::Integer(1),
            TValue::Integer(2),
            TValue::Integer(3),
        ]);
        t.hash.insert(TableKey::Integer(5), TValue::Integer(5));
        let h = heap.alloc_table(t);
        assert_eq!(heap.table_len(h), 3);
    }

    // --- table_next (iteration) ------------------------------------

    #[test]
    fn next_on_empty_table_returns_none() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(Table::default());
        assert_eq!(heap.table_next(h, None), None);
    }

    #[test]
    fn next_with_none_returns_first_array_entry() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Integer(20),
        ]));
        assert_eq!(
            heap.table_next(h, None),
            Some((TableKey::Integer(1), TValue::Integer(10)))
        );
    }

    #[test]
    fn next_walks_array_part_sequentially_skipping_holes() {
        let mut heap = Heap::default();
        let h = heap.alloc_table(fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Nil,
            TValue::Integer(30),
        ]));
        // After key=1, the next non-nil slot is index 3.
        assert_eq!(
            heap.table_next(h, Some(TableKey::Integer(1))),
            Some((TableKey::Integer(3), TValue::Integer(30)))
        );
    }

    #[test]
    fn next_transitions_from_array_part_to_hash_part() {
        let mut heap = Heap::default();
        let str_key = heap.alloc_string(fresh_string(b"k"));
        let mut t = fresh_table_with_array(vec![TValue::Integer(10)]);
        t.hash.insert(
            TableKey::ShortString(str_key),
            TValue::Integer(99),
        );
        let h = heap.alloc_table(t);
        // Starting at last array key → first hash entry.
        assert_eq!(
            heap.table_next(h, Some(TableKey::Integer(1))),
            Some((TableKey::ShortString(str_key), TValue::Integer(99)))
        );
    }

    #[test]
    fn next_exhausts_all_entries_in_full_traversal() {
        // A traversal via table_next must visit every live entry
        // exactly once and then return None.
        let mut heap = Heap::default();
        let mut t = fresh_table_with_array(vec![
            TValue::Integer(10),
            TValue::Integer(20),
        ]);
        t.hash.insert(TableKey::Integer(100), TValue::Integer(9999));
        let h = heap.alloc_table(t);

        let mut seen = Vec::new();
        let mut cursor: Option<TableKey> = None;
        while let Some((k, v)) = heap.table_next(h, cursor) {
            seen.push((k, v));
            cursor = Some(k);
        }
        assert_eq!(seen.len(), 3);
        // Array part in order.
        assert_eq!(seen[0], (TableKey::Integer(1), TValue::Integer(10)));
        assert_eq!(seen[1], (TableKey::Integer(2), TValue::Integer(20)));
        // Hash part last.
        assert_eq!(
            seen[2],
            (TableKey::Integer(100), TValue::Integer(9999))
        );
    }
}
