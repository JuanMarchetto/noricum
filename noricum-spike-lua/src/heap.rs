//! Allocation and accessor methods for [`crate::contract::Heap`].
//!
//! Stage 2 / commit 3 originally, extended by Stage 3 / commit 1 to
//! carry generation counters on every handle. The [`Heap`] struct
//! itself (per-kind slot vecs + free lists + generation vecs) lives
//! in [`crate::contract`]; this module attaches the behavior.
//!
//! For each object kind we expose:
//!
//! | method            | purpose                                   |
//! |-------------------|-------------------------------------------|
//! | `alloc_*(value)`  | pop a slot from the free list (or push a new one), store the value, and return a handle stamped with the current slot generation |
//! | `*(handle)`       | immutable accessor; panics on freed slot OR generation mismatch (always, including release) |
//! | `*_mut(handle)`   | mutable accessor; panics on freed slot OR generation mismatch (always, including release) |
//! | `free_*(handle)`  | validate the handle generation, mark the slot as free, push the index onto the free list; panics always on double-free or stale handle |
//!
//! The panic checks are plain `assert!` (not `debug_assert!`) because
//! double-free corrupts the free list and access-after-free dereferences
//! a stale handle into a recycled object — both are programming errors
//! that are far more costly to debug in production than a single check
//! per operation.
//!
//! # Generation counters (Stage 3 / commit 1 infrastructure)
//!
//! Each arena has a parallel [`u32`] generation Vec. Every `alloc_*`
//! stamps the caller's handle with the current generation for that
//! slot. Every accessor asserts the handle's generation still matches.
//! Stage 3 commit 2 adds the actual bump-on-free that gives this
//! machinery teeth — in commit 1 the generation is always 0, so the
//! check is deterministically a no-op and every Stage 2 test keeps
//! passing through the widening refactor. The infrastructure ships
//! first so the test surface stays stable; the behavior flip lands
//! in a follow-up commit.
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

    /// Allocate a string slot and store `value`, returning a handle
    /// stamped with the current slot generation.
    pub fn alloc_string(&mut self, value: LuaString) -> StringHandle {
        if let Some(slot) = self.free_strings.pop() {
            self.strings[slot as usize] = Some(value);
            let generation = self.generations_strings[slot as usize];
            StringHandle::new(slot, generation)
        } else {
            let slot = self.strings.len() as u32;
            self.strings.push(Some(value));
            self.generations_strings.push(0);
            StringHandle::new(slot, 0)
        }
    }

    /// Borrow the string pointed to by `handle`. Panics if the slot
    /// has been freed or the handle generation is stale.
    pub fn string(&self, handle: StringHandle) -> &LuaString {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_strings[slot], handle.generation,
            "StringHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_strings[slot]
        );
        self.strings[slot]
            .as_ref()
            .expect("StringHandle points to freed slot")
    }

    /// Mutably borrow the string pointed to by `handle`. Panics if
    /// the slot has been freed or the handle generation is stale.
    pub fn string_mut(&mut self, handle: StringHandle) -> &mut LuaString {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_strings[slot], handle.generation,
            "StringHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_strings[slot]
        );
        self.strings[slot]
            .as_mut()
            .expect("StringHandle points to freed slot")
    }

    /// Free the slot pointed to by `handle`. The slot index is pushed
    /// onto the string free list for reuse. Stage 3 commit 2 will add
    /// a generation bump here; for now the counter is untouched.
    pub fn free_string(&mut self, handle: StringHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_strings[slot], handle.generation,
            "free_string on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_strings[slot]
        );
        assert!(
            self.strings[slot].is_some(),
            "double free of StringHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.strings[slot] = None;
        self.free_strings.push(handle.slot);
    }

    // ------------------------------------------------------------------
    // Table
    // ------------------------------------------------------------------

    pub fn alloc_table(&mut self, value: Table) -> TableHandle {
        if let Some(slot) = self.free_tables.pop() {
            self.tables[slot as usize] = Some(value);
            let generation = self.generations_tables[slot as usize];
            TableHandle::new(slot, generation)
        } else {
            let slot = self.tables.len() as u32;
            self.tables.push(Some(value));
            self.generations_tables.push(0);
            TableHandle::new(slot, 0)
        }
    }

    pub fn table(&self, handle: TableHandle) -> &Table {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_tables[slot], handle.generation,
            "TableHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_tables[slot]
        );
        self.tables[slot]
            .as_ref()
            .expect("TableHandle points to freed slot")
    }

    pub fn table_mut(&mut self, handle: TableHandle) -> &mut Table {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_tables[slot], handle.generation,
            "TableHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_tables[slot]
        );
        self.tables[slot]
            .as_mut()
            .expect("TableHandle points to freed slot")
    }

    pub fn free_table(&mut self, handle: TableHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_tables[slot], handle.generation,
            "free_table on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_tables[slot]
        );
        assert!(
            self.tables[slot].is_some(),
            "double free of TableHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.tables[slot] = None;
        self.free_tables.push(handle.slot);
    }

    // ------------------------------------------------------------------
    // Proto
    // ------------------------------------------------------------------

    pub fn alloc_proto(&mut self, value: Proto) -> ProtoHandle {
        if let Some(slot) = self.free_protos.pop() {
            self.protos[slot as usize] = Some(value);
            let generation = self.generations_protos[slot as usize];
            ProtoHandle::new(slot, generation)
        } else {
            let slot = self.protos.len() as u32;
            self.protos.push(Some(value));
            self.generations_protos.push(0);
            ProtoHandle::new(slot, 0)
        }
    }

    pub fn proto(&self, handle: ProtoHandle) -> &Proto {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_protos[slot], handle.generation,
            "ProtoHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_protos[slot]
        );
        self.protos[slot]
            .as_ref()
            .expect("ProtoHandle points to freed slot")
    }

    pub fn proto_mut(&mut self, handle: ProtoHandle) -> &mut Proto {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_protos[slot], handle.generation,
            "ProtoHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_protos[slot]
        );
        self.protos[slot]
            .as_mut()
            .expect("ProtoHandle points to freed slot")
    }

    pub fn free_proto(&mut self, handle: ProtoHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_protos[slot], handle.generation,
            "free_proto on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_protos[slot]
        );
        assert!(
            self.protos[slot].is_some(),
            "double free of ProtoHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.protos[slot] = None;
        self.free_protos.push(handle.slot);
    }

    // ------------------------------------------------------------------
    // LClosure
    // ------------------------------------------------------------------

    pub fn alloc_lclosure(&mut self, value: LClosure) -> LClosureHandle {
        if let Some(slot) = self.free_lclosures.pop() {
            self.lclosures[slot as usize] = Some(value);
            let generation = self.generations_lclosures[slot as usize];
            LClosureHandle::new(slot, generation)
        } else {
            let slot = self.lclosures.len() as u32;
            self.lclosures.push(Some(value));
            self.generations_lclosures.push(0);
            LClosureHandle::new(slot, 0)
        }
    }

    pub fn lclosure(&self, handle: LClosureHandle) -> &LClosure {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_lclosures[slot], handle.generation,
            "LClosureHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_lclosures[slot]
        );
        self.lclosures[slot]
            .as_ref()
            .expect("LClosureHandle points to freed slot")
    }

    pub fn lclosure_mut(&mut self, handle: LClosureHandle) -> &mut LClosure {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_lclosures[slot], handle.generation,
            "LClosureHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_lclosures[slot]
        );
        self.lclosures[slot]
            .as_mut()
            .expect("LClosureHandle points to freed slot")
    }

    pub fn free_lclosure(&mut self, handle: LClosureHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_lclosures[slot], handle.generation,
            "free_lclosure on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_lclosures[slot]
        );
        assert!(
            self.lclosures[slot].is_some(),
            "double free of LClosureHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.lclosures[slot] = None;
        self.free_lclosures.push(handle.slot);
    }

    // ------------------------------------------------------------------
    // CClosure
    // ------------------------------------------------------------------

    pub fn alloc_cclosure(&mut self, value: CClosure) -> CClosureHandle {
        if let Some(slot) = self.free_cclosures.pop() {
            self.cclosures[slot as usize] = Some(value);
            let generation = self.generations_cclosures[slot as usize];
            CClosureHandle::new(slot, generation)
        } else {
            let slot = self.cclosures.len() as u32;
            self.cclosures.push(Some(value));
            self.generations_cclosures.push(0);
            CClosureHandle::new(slot, 0)
        }
    }

    pub fn cclosure(&self, handle: CClosureHandle) -> &CClosure {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_cclosures[slot], handle.generation,
            "CClosureHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_cclosures[slot]
        );
        self.cclosures[slot]
            .as_ref()
            .expect("CClosureHandle points to freed slot")
    }

    pub fn cclosure_mut(&mut self, handle: CClosureHandle) -> &mut CClosure {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_cclosures[slot], handle.generation,
            "CClosureHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_cclosures[slot]
        );
        self.cclosures[slot]
            .as_mut()
            .expect("CClosureHandle points to freed slot")
    }

    pub fn free_cclosure(&mut self, handle: CClosureHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_cclosures[slot], handle.generation,
            "free_cclosure on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_cclosures[slot]
        );
        assert!(
            self.cclosures[slot].is_some(),
            "double free of CClosureHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.cclosures[slot] = None;
        self.free_cclosures.push(handle.slot);
    }

    // ------------------------------------------------------------------
    // UpVal
    // ------------------------------------------------------------------

    pub fn alloc_upval(&mut self, value: UpVal) -> UpValHandle {
        if let Some(slot) = self.free_upvals.pop() {
            self.upvals[slot as usize] = Some(value);
            let generation = self.generations_upvals[slot as usize];
            UpValHandle::new(slot, generation)
        } else {
            let slot = self.upvals.len() as u32;
            self.upvals.push(Some(value));
            self.generations_upvals.push(0);
            UpValHandle::new(slot, 0)
        }
    }

    pub fn upval(&self, handle: UpValHandle) -> &UpVal {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_upvals[slot], handle.generation,
            "UpValHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_upvals[slot]
        );
        self.upvals[slot]
            .as_ref()
            .expect("UpValHandle points to freed slot")
    }

    pub fn upval_mut(&mut self, handle: UpValHandle) -> &mut UpVal {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_upvals[slot], handle.generation,
            "UpValHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_upvals[slot]
        );
        self.upvals[slot]
            .as_mut()
            .expect("UpValHandle points to freed slot")
    }

    pub fn free_upval(&mut self, handle: UpValHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_upvals[slot], handle.generation,
            "free_upval on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_upvals[slot]
        );
        assert!(
            self.upvals[slot].is_some(),
            "double free of UpValHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.upvals[slot] = None;
        self.free_upvals.push(handle.slot);
    }

    // ------------------------------------------------------------------
    // Thread
    // ------------------------------------------------------------------

    pub fn alloc_thread(&mut self, value: Thread) -> ThreadHandle {
        if let Some(slot) = self.free_threads.pop() {
            self.threads[slot as usize] = Some(value);
            let generation = self.generations_threads[slot as usize];
            ThreadHandle::new(slot, generation)
        } else {
            let slot = self.threads.len() as u32;
            self.threads.push(Some(value));
            self.generations_threads.push(0);
            ThreadHandle::new(slot, 0)
        }
    }

    pub fn thread(&self, handle: ThreadHandle) -> &Thread {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_threads[slot], handle.generation,
            "ThreadHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_threads[slot]
        );
        self.threads[slot]
            .as_ref()
            .expect("ThreadHandle points to freed slot")
    }

    pub fn thread_mut(&mut self, handle: ThreadHandle) -> &mut Thread {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_threads[slot], handle.generation,
            "ThreadHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_threads[slot]
        );
        self.threads[slot]
            .as_mut()
            .expect("ThreadHandle points to freed slot")
    }

    pub fn free_thread(&mut self, handle: ThreadHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_threads[slot], handle.generation,
            "free_thread on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_threads[slot]
        );
        assert!(
            self.threads[slot].is_some(),
            "double free of ThreadHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.threads[slot] = None;
        self.free_threads.push(handle.slot);
    }

    // ------------------------------------------------------------------
    // UserData
    // ------------------------------------------------------------------

    pub fn alloc_userdata(&mut self, value: UserData) -> UserDataHandle {
        if let Some(slot) = self.free_userdata.pop() {
            self.userdata[slot as usize] = Some(value);
            let generation = self.generations_userdata[slot as usize];
            UserDataHandle::new(slot, generation)
        } else {
            let slot = self.userdata.len() as u32;
            self.userdata.push(Some(value));
            self.generations_userdata.push(0);
            UserDataHandle::new(slot, 0)
        }
    }

    pub fn userdata_get(&self, handle: UserDataHandle) -> &UserData {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_userdata[slot], handle.generation,
            "UserDataHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_userdata[slot]
        );
        self.userdata[slot]
            .as_ref()
            .expect("UserDataHandle points to freed slot")
    }

    pub fn userdata_mut(&mut self, handle: UserDataHandle) -> &mut UserData {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_userdata[slot], handle.generation,
            "UserDataHandle points to a reused slot (handle generation {}, slot generation {})",
            handle.generation, self.generations_userdata[slot]
        );
        self.userdata[slot]
            .as_mut()
            .expect("UserDataHandle points to freed slot")
    }

    pub fn free_userdata(&mut self, handle: UserDataHandle) {
        let slot = handle.slot as usize;
        assert_eq!(
            self.generations_userdata[slot], handle.generation,
            "free_userdata on stale handle (handle generation {}, slot generation {})",
            handle.generation, self.generations_userdata[slot]
        );
        assert!(
            self.userdata[slot].is_some(),
            "double free of UserDataHandle {{ slot: {}, generation: {} }}",
            handle.slot, handle.generation
        );
        self.userdata[slot] = None;
        self.free_userdata.push(handle.slot);
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
        assert_eq!(h1.slot, 0);
        assert_eq!(h2.slot, 1);

        heap.free_string(h1);
        assert!(heap.free_strings == vec![0]);

        let h3 = heap.alloc_string(fresh_string(b"c"));
        assert_eq!(h3.slot, 0, "expected LIFO slot reuse");
        assert!(heap.free_strings.is_empty());
    }

    #[test]
    fn alloc_after_many_frees_stays_compact() {
        let mut heap = Heap::default();
        let handles: Vec<_> = (0..10)
            .map(|i| heap.alloc_string(fresh_string(&[i as u8])))
            .collect();
        for h in &handles {
            heap.free_string(*h);
        }
        // All 10 slots are now free. A new alloc uses the most recent.
        let h = heap.alloc_string(fresh_string(b"new"));
        assert_eq!(h.slot, 9, "LIFO reuse of slot 9 first");
        assert_eq!(heap.free_strings.len(), 9);
    }

    #[test]
    fn multiple_object_kinds_are_independent() {
        let mut heap = Heap::default();
        let s = heap.alloc_string(fresh_string(b"x"));
        let t = heap.alloc_table(Table::default());
        // Both live at slot 0 of their respective kind.
        assert_eq!(s.slot, 0);
        assert_eq!(t.slot, 0);
        assert_eq!(heap.string(s).bytes, b"x");
        // Free one kind; the other stays reachable.
        heap.free_string(s);
        assert!(heap.free_strings == vec![0]);
        assert!(heap.free_tables.is_empty());
        assert!(heap.table(t).array.is_empty());
    }

    #[test]
    fn generations_vec_grows_in_lockstep_with_slot_vec() {
        let mut heap = Heap::default();
        assert_eq!(heap.generations_strings.len(), 0);
        heap.alloc_string(fresh_string(b"a"));
        assert_eq!(heap.generations_strings.len(), 1);
        heap.alloc_string(fresh_string(b"b"));
        assert_eq!(heap.generations_strings.len(), 2);
        // At Stage 3 commit 1, every generation is still 0.
        assert_eq!(heap.generations_strings, vec![0, 0]);
    }

    #[test]
    fn fresh_handles_have_generation_zero() {
        let mut heap = Heap::default();
        let h = heap.alloc_string(fresh_string(b"x"));
        // Commit 1: generation is always 0. Commit 2 will invalidate
        // this assumption once free_* bumps the counter.
        assert_eq!(h.generation, 0);
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
