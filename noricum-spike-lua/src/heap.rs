//! Allocation and accessor methods for [`crate::contract::Heap`].
//!
//! Stage 2 / commit 3. The [`Heap`] struct itself (per-kind slot vecs +
//! free lists) lives in [`crate::contract`]; this module attaches the
//! behavior. For each object kind we expose:
//!
//! | method            | purpose                                   |
//! |-------------------|-------------------------------------------|
//! | `alloc_*(value)`  | pop a slot from the free list (or push a new one) and store the value; returns a handle |
//! | `*(handle)`       | immutable accessor; panics on freed slot (always, including release) |
//! | `*_mut(handle)`   | mutable accessor; panics on freed slot (always, including release) |
//! | `free_*(handle)`  | mark the slot as free and push the index onto the free list; panics always on double-free |
//!
//! The panic checks are plain `assert!` (not `debug_assert!`) because
//! double-free corrupts the free list and access-after-free dereferences
//! a stale handle into a recycled object — both are programming errors
//! that are far more costly to debug in production than a single `is_some`
//! check per operation. Stage 3's generation counters will reduce the
//! access-after-free surface but not eliminate the need for this rail.
//!
//! # Generation counters
//!
//! Handles are bare `u32` slot indices. A freed-then-reallocated slot
//! gets the same handle value as the original, so a stale handle could
//! dereference the new occupant silently. This is **risk R1** in
//! `docs/lua-migration/plan.md`. Stage 3 retrofits 8-bit generation
//! counters onto every handle and compares at deref time; until then,
//! Stage 2 relies on test coverage and the "don't keep handles to
//! freed objects" discipline enforced by every caller.
//!
//! # Handle reuse order
//!
//! Free lists are LIFO (`Vec::pop`), matching piccolo and the general
//! arena convention. This maximizes cache locality for workloads that
//! churn small numbers of short-lived objects.

#![allow(dead_code)]

use crate::contract::{
    CClosure, CClosureHandle, Heap, LClosure, LClosureHandle, LuaString, Proto, ProtoHandle,
    StringHandle, Table, TableHandle, Thread, ThreadHandle, UpVal, UpValHandle, UserData,
    UserDataHandle,
};

impl Heap {
    // ------------------------------------------------------------------
    // LuaString
    // ------------------------------------------------------------------

    /// Allocate a string slot and store `value`, returning a handle.
    pub fn alloc_string(&mut self, value: LuaString) -> StringHandle {
        if let Some(slot) = self.free_strings.pop() {
            self.strings[slot as usize] = Some(value);
            StringHandle(slot)
        } else {
            let slot = self.strings.len() as u32;
            self.strings.push(Some(value));
            StringHandle(slot)
        }
    }

    /// Borrow the string pointed to by `handle`. Panics if the slot
    /// has been freed.
    pub fn string(&self, handle: StringHandle) -> &LuaString {
        self.strings[handle.0 as usize]
            .as_ref()
            .expect("StringHandle points to freed slot")
    }

    /// Mutably borrow the string pointed to by `handle`. Panics if
    /// the slot has been freed.
    pub fn string_mut(&mut self, handle: StringHandle) -> &mut LuaString {
        self.strings[handle.0 as usize]
            .as_mut()
            .expect("StringHandle points to freed slot")
    }

    /// Free the slot pointed to by `handle`. The slot index is pushed
    /// onto the string free list for reuse.
    pub fn free_string(&mut self, handle: StringHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.strings[slot].is_some(),
            "double free of StringHandle({})",
            handle.0
        );
        self.strings[slot] = None;
        self.free_strings.push(handle.0);
    }

    // ------------------------------------------------------------------
    // Table
    // ------------------------------------------------------------------

    pub fn alloc_table(&mut self, value: Table) -> TableHandle {
        if let Some(slot) = self.free_tables.pop() {
            self.tables[slot as usize] = Some(value);
            TableHandle(slot)
        } else {
            let slot = self.tables.len() as u32;
            self.tables.push(Some(value));
            TableHandle(slot)
        }
    }

    pub fn table(&self, handle: TableHandle) -> &Table {
        self.tables[handle.0 as usize]
            .as_ref()
            .expect("TableHandle points to freed slot")
    }

    pub fn table_mut(&mut self, handle: TableHandle) -> &mut Table {
        self.tables[handle.0 as usize]
            .as_mut()
            .expect("TableHandle points to freed slot")
    }

    pub fn free_table(&mut self, handle: TableHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.tables[slot].is_some(),
            "double free of TableHandle({})",
            handle.0
        );
        self.tables[slot] = None;
        self.free_tables.push(handle.0);
    }

    // ------------------------------------------------------------------
    // Proto
    // ------------------------------------------------------------------

    pub fn alloc_proto(&mut self, value: Proto) -> ProtoHandle {
        if let Some(slot) = self.free_protos.pop() {
            self.protos[slot as usize] = Some(value);
            ProtoHandle(slot)
        } else {
            let slot = self.protos.len() as u32;
            self.protos.push(Some(value));
            ProtoHandle(slot)
        }
    }

    pub fn proto(&self, handle: ProtoHandle) -> &Proto {
        self.protos[handle.0 as usize]
            .as_ref()
            .expect("ProtoHandle points to freed slot")
    }

    pub fn proto_mut(&mut self, handle: ProtoHandle) -> &mut Proto {
        self.protos[handle.0 as usize]
            .as_mut()
            .expect("ProtoHandle points to freed slot")
    }

    pub fn free_proto(&mut self, handle: ProtoHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.protos[slot].is_some(),
            "double free of ProtoHandle({})",
            handle.0
        );
        self.protos[slot] = None;
        self.free_protos.push(handle.0);
    }

    // ------------------------------------------------------------------
    // LClosure
    // ------------------------------------------------------------------

    pub fn alloc_lclosure(&mut self, value: LClosure) -> LClosureHandle {
        if let Some(slot) = self.free_lclosures.pop() {
            self.lclosures[slot as usize] = Some(value);
            LClosureHandle(slot)
        } else {
            let slot = self.lclosures.len() as u32;
            self.lclosures.push(Some(value));
            LClosureHandle(slot)
        }
    }

    pub fn lclosure(&self, handle: LClosureHandle) -> &LClosure {
        self.lclosures[handle.0 as usize]
            .as_ref()
            .expect("LClosureHandle points to freed slot")
    }

    pub fn lclosure_mut(&mut self, handle: LClosureHandle) -> &mut LClosure {
        self.lclosures[handle.0 as usize]
            .as_mut()
            .expect("LClosureHandle points to freed slot")
    }

    pub fn free_lclosure(&mut self, handle: LClosureHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.lclosures[slot].is_some(),
            "double free of LClosureHandle({})",
            handle.0
        );
        self.lclosures[slot] = None;
        self.free_lclosures.push(handle.0);
    }

    // ------------------------------------------------------------------
    // CClosure
    // ------------------------------------------------------------------

    pub fn alloc_cclosure(&mut self, value: CClosure) -> CClosureHandle {
        if let Some(slot) = self.free_cclosures.pop() {
            self.cclosures[slot as usize] = Some(value);
            CClosureHandle(slot)
        } else {
            let slot = self.cclosures.len() as u32;
            self.cclosures.push(Some(value));
            CClosureHandle(slot)
        }
    }

    pub fn cclosure(&self, handle: CClosureHandle) -> &CClosure {
        self.cclosures[handle.0 as usize]
            .as_ref()
            .expect("CClosureHandle points to freed slot")
    }

    pub fn cclosure_mut(&mut self, handle: CClosureHandle) -> &mut CClosure {
        self.cclosures[handle.0 as usize]
            .as_mut()
            .expect("CClosureHandle points to freed slot")
    }

    pub fn free_cclosure(&mut self, handle: CClosureHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.cclosures[slot].is_some(),
            "double free of CClosureHandle({})",
            handle.0
        );
        self.cclosures[slot] = None;
        self.free_cclosures.push(handle.0);
    }

    // ------------------------------------------------------------------
    // UpVal
    // ------------------------------------------------------------------

    pub fn alloc_upval(&mut self, value: UpVal) -> UpValHandle {
        if let Some(slot) = self.free_upvals.pop() {
            self.upvals[slot as usize] = Some(value);
            UpValHandle(slot)
        } else {
            let slot = self.upvals.len() as u32;
            self.upvals.push(Some(value));
            UpValHandle(slot)
        }
    }

    pub fn upval(&self, handle: UpValHandle) -> &UpVal {
        self.upvals[handle.0 as usize]
            .as_ref()
            .expect("UpValHandle points to freed slot")
    }

    pub fn upval_mut(&mut self, handle: UpValHandle) -> &mut UpVal {
        self.upvals[handle.0 as usize]
            .as_mut()
            .expect("UpValHandle points to freed slot")
    }

    pub fn free_upval(&mut self, handle: UpValHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.upvals[slot].is_some(),
            "double free of UpValHandle({})",
            handle.0
        );
        self.upvals[slot] = None;
        self.free_upvals.push(handle.0);
    }

    // ------------------------------------------------------------------
    // Thread
    // ------------------------------------------------------------------

    pub fn alloc_thread(&mut self, value: Thread) -> ThreadHandle {
        if let Some(slot) = self.free_threads.pop() {
            self.threads[slot as usize] = Some(value);
            ThreadHandle(slot)
        } else {
            let slot = self.threads.len() as u32;
            self.threads.push(Some(value));
            ThreadHandle(slot)
        }
    }

    pub fn thread(&self, handle: ThreadHandle) -> &Thread {
        self.threads[handle.0 as usize]
            .as_ref()
            .expect("ThreadHandle points to freed slot")
    }

    pub fn thread_mut(&mut self, handle: ThreadHandle) -> &mut Thread {
        self.threads[handle.0 as usize]
            .as_mut()
            .expect("ThreadHandle points to freed slot")
    }

    pub fn free_thread(&mut self, handle: ThreadHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.threads[slot].is_some(),
            "double free of ThreadHandle({})",
            handle.0
        );
        self.threads[slot] = None;
        self.free_threads.push(handle.0);
    }

    // ------------------------------------------------------------------
    // UserData
    // ------------------------------------------------------------------

    pub fn alloc_userdata(&mut self, value: UserData) -> UserDataHandle {
        if let Some(slot) = self.free_userdata.pop() {
            self.userdata[slot as usize] = Some(value);
            UserDataHandle(slot)
        } else {
            let slot = self.userdata.len() as u32;
            self.userdata.push(Some(value));
            UserDataHandle(slot)
        }
    }

    pub fn userdata_get(&self, handle: UserDataHandle) -> &UserData {
        self.userdata[handle.0 as usize]
            .as_ref()
            .expect("UserDataHandle points to freed slot")
    }

    pub fn userdata_mut(&mut self, handle: UserDataHandle) -> &mut UserData {
        self.userdata[handle.0 as usize]
            .as_mut()
            .expect("UserDataHandle points to freed slot")
    }

    pub fn free_userdata(&mut self, handle: UserDataHandle) {
        let slot = handle.0 as usize;
        assert!(
            self.userdata[slot].is_some(),
            "double free of UserDataHandle({})",
            handle.0
        );
        self.userdata[slot] = None;
        self.free_userdata.push(handle.0);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
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

    #[test]
    fn alloc_then_access_string() {
        let mut heap = Heap::default();
        let h = heap.alloc_string(fresh_string(b"hello"));
        assert_eq!(heap.string(h).bytes, b"hello");
    }

    #[test]
    fn mutation_through_mut_accessor() {
        let mut heap = Heap::default();
        let h = heap.alloc_string(fresh_string(b"foo"));
        heap.string_mut(h).bytes.extend_from_slice(b"bar");
        assert_eq!(heap.string(h).bytes, b"foobar");
    }

    #[test]
    fn free_list_recycles_slot_index() {
        let mut heap = Heap::default();
        let h1 = heap.alloc_string(fresh_string(b"a"));
        let h2 = heap.alloc_string(fresh_string(b"b"));
        assert_eq!(h1.0, 0);
        assert_eq!(h2.0, 1);

        heap.free_string(h1);
        assert!(heap.free_strings == vec![0]);

        let h3 = heap.alloc_string(fresh_string(b"c"));
        assert_eq!(h3.0, 0, "expected LIFO slot reuse");
        assert!(heap.free_strings.is_empty());
    }

    #[test]
    fn alloc_after_many_frees_stays_compact() {
        let mut heap = Heap::default();
        let handles: Vec<_> = (0..10).map(|i| {
            heap.alloc_string(fresh_string(&[i as u8]))
        }).collect();
        for h in &handles {
            heap.free_string(*h);
        }
        // All 10 slots are now free. A new alloc uses the most recent.
        let h = heap.alloc_string(fresh_string(b"new"));
        assert_eq!(h.0, 9, "LIFO reuse of slot 9 first");
        assert_eq!(heap.free_strings.len(), 9);
    }

    #[test]
    fn multiple_object_kinds_are_independent() {
        let mut heap = Heap::default();
        let s = heap.alloc_string(fresh_string(b"x"));
        let t = heap.alloc_table(Table::default());
        // Both live at slot 0 of their respective kind.
        assert_eq!(s.0, 0);
        assert_eq!(t.0, 0);
        assert_eq!(heap.string(s).bytes, b"x");
        // Free one kind; the other stays reachable.
        heap.free_string(s);
        assert!(heap.free_strings == vec![0]);
        assert!(heap.free_tables.is_empty());
        assert!(heap.table(t).array.is_empty());
    }

    #[test]
    #[should_panic(expected = "StringHandle points to freed slot")]
    fn access_after_free_panics() {
        let mut heap = Heap::default();
        let h = heap.alloc_string(fresh_string(b"doomed"));
        heap.free_string(h);
        let _ = heap.string(h);
    }

    #[test]
    #[should_panic(expected = "double free of StringHandle")]
    fn double_free_panics() {
        let mut heap = Heap::default();
        let h = heap.alloc_string(fresh_string(b"ghost"));
        heap.free_string(h);
        heap.free_string(h);
    }
}
