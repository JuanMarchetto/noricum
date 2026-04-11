//! Proto / Closure / UpVal lifecycle helpers.
//!
//! Stage 2 / commit 5. Ports the allocation and linkage primitives
//! from `lfunc.c`. Most of `lfunc.c` — to-be-closed upvalue tracking
//! (`luaF_newtbcupval`, `luaF_close`), TM_CLOSE dispatch
//! (`callclosemethod`, `checkclosemth`, `prepcallclosemth`), GC
//! coordination (`luaF_protosize`, `luaF_freeproto`) — is deferred.
//! That code depends on metamethod dispatch, the GC, and the closing
//! stack machinery, all of which live in Stages 3-5.
//!
//! What lands in Stage 2:
//!
//! * [`Heap::new_proto`] — fresh empty prototype (`luaF_newproto`).
//! * [`Heap::new_lclosure`] — Lua closure with a proto and `n`
//!   pre-initialized Closed(Nil) upvals (`luaF_newLclosure` +
//!   `luaF_initupvals` fused; the C code splits them because of GC
//!   barriers, but with manual arenas we don't need that separation
//!   yet).
//! * [`Heap::new_cclosure`] — C closure with `n` Nil upvalues
//!   (`luaF_newCclosure` + the unused-upvalues init).
//! * [`Heap::find_or_create_open_upval`] — find-or-create matching
//!   `luaF_findupval`'s behavior of searching the owning thread's
//!   open-upval list and reusing any existing upvalue at the same
//!   stack level.
//! * [`Heap::close_open_upvals`] — snapshot the stack values of all
//!   open upvalues at or above a level, transition them to
//!   `UpValState::Closed`, and drop them from the thread's open list
//!   (matches `luaF_closeupval`).
//! * [`Proto::get_local_name`] — 1-indexed local-variable lookup at a
//!   given bytecode position, direct port of `luaF_getlocalname`.
//!
//! Ground truth: `lfunc.c`, `lfunc.h`, `lstate.h`.

#![allow(dead_code)]

use crate::contract::{
    CClosure, CClosureHandle, Heap, LClosure, LClosureHandle, Proto, ProtoHandle, RawCFunction,
    StringHandle, TValue, ThreadHandle, UpVal, UpValHandle, UpValState,
};

impl Heap {
    /// Allocate a fresh empty [`Proto`]. Matches `luaF_newproto`.
    pub fn new_proto(&mut self) -> ProtoHandle {
        self.alloc_proto(Proto::default())
    }

    /// Allocate an [`LClosure`] with `nupvals` pre-initialized
    /// closed-nil upvals. Matches the fused behavior of
    /// `luaF_newLclosure` + `luaF_initupvals`.
    pub fn new_lclosure(&mut self, proto: ProtoHandle, nupvals: usize) -> LClosureHandle {
        // Create the upval handles eagerly so the LClosure owns them
        // from the moment it is allocated — this avoids any window
        // where the upvalues Vec holds placeholder/invalid handles.
        let mut upvalues = Vec::with_capacity(nupvals);
        for _ in 0..nupvals {
            let handle = self.alloc_upval(UpVal {
                state: UpValState::Closed(TValue::Nil),
            });
            upvalues.push(handle);
        }
        self.alloc_lclosure(LClosure { proto, upvalues })
    }

    /// Allocate a [`CClosure`] wrapping a raw C function. `nupvals`
    /// slots are pre-filled with `TValue::Nil`. Matches
    /// `luaF_newCclosure` + the no-op upvalue init.
    pub fn new_cclosure(&mut self, f: RawCFunction, nupvals: usize) -> CClosureHandle {
        self.alloc_cclosure(CClosure {
            f,
            upvalues: vec![TValue::Nil; nupvals],
        })
    }

    /// Find the open upvalue on `thread` pointing at `stack_index`,
    /// or create one if none exists. Matches `luaF_findupval`.
    ///
    /// Note: the C implementation keeps the open-upval list sorted
    /// descending by stack level (so that `close_open_upvals(level)`
    /// can walk from the head until `uplevel(p) < level`). Our Rust
    /// port uses an unsorted list for simplicity; Stage 5 will sort
    /// if profiling shows the walk is hot.
    pub fn find_or_create_open_upval(
        &mut self,
        thread: ThreadHandle,
        stack_index: u32,
    ) -> UpValHandle {
        let open_list = self.thread(thread).open_upvals.clone();
        for handle in open_list {
            if let UpValState::Open {
                stack_index: idx, ..
            } = &self.upval(handle).state
            {
                if *idx == stack_index {
                    return handle;
                }
            }
        }
        let handle = self.alloc_upval(UpVal {
            state: UpValState::Open {
                thread,
                stack_index,
            },
        });
        self.thread_mut(thread).open_upvals.push(handle);
        handle
    }

    /// Close every open upvalue on `thread` whose stack level is
    /// `>= level`. Each upvalue is snapshotted from the stack into
    /// its `Closed` state. Matches `luaF_closeupval`.
    pub fn close_open_upvals(&mut self, thread: ThreadHandle, level: u32) {
        // Take the current open list; we'll rebuild the ones that
        // survive this close call and put them back.
        let open_list = std::mem::take(&mut self.thread_mut(thread).open_upvals);
        let mut to_close: Vec<(UpValHandle, u32)> = Vec::new();
        let mut keep: Vec<UpValHandle> = Vec::new();
        for handle in open_list {
            match &self.upval(handle).state {
                UpValState::Open {
                    stack_index: idx, ..
                } if *idx >= level => {
                    to_close.push((handle, *idx));
                }
                _ => keep.push(handle),
            }
        }
        self.thread_mut(thread).open_upvals = keep;

        for (handle, idx) in to_close {
            let value = self
                .thread(thread)
                .stack
                .get(idx as usize)
                .copied()
                .unwrap_or(TValue::Nil);
            self.upval_mut(handle).state = UpValState::Closed(value);
        }
    }
}

impl Proto {
    /// Look up the `local_number`-th local variable active at bytecode
    /// position `pc`. Local numbering is 1-indexed (matches the C
    /// convention in `luaF_getlocalname` — `local_number = 1` returns
    /// the first active local, `0` returns `None`).
    ///
    /// Returns the interned name handle, or `None` if no such local
    /// exists.
    pub fn get_local_name(&self, local_number: usize, pc: i32) -> Option<StringHandle> {
        if local_number == 0 {
            return None;
        }
        let mut remaining = local_number;
        for var in &self.local_vars {
            if var.start_pc > pc {
                break;
            }
            if var.end_pc > pc {
                remaining -= 1;
                if remaining == 0 {
                    return var.name;
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CClosureHandle, LocVar, Thread, ThreadStatus, ThreadStatusInner};
    use std::os::raw::c_int;

    fn fresh_thread() -> Thread {
        Thread {
            status: ThreadStatusInner(ThreadStatus::Ok),
            stack: vec![TValue::Nil; 16],
            top: 0,
            frames: Vec::new(),
            open_upvals: Vec::new(),
        }
    }

    unsafe extern "C" fn dummy_c_function(_state: *mut crate::contract::LuaState) -> c_int {
        0
    }

    // ---- Proto ----

    #[test]
    fn new_proto_is_empty_and_has_no_locals() {
        let mut heap = Heap::default();
        let h = heap.new_proto();
        let p = heap.proto(h);
        assert_eq!(p.num_params, 0);
        assert_eq!(p.max_stack_size, 0);
        assert!(p.code.is_empty());
        assert!(p.local_vars.is_empty());
    }

    #[test]
    fn get_local_name_respects_active_range() {
        // These handles are fakes constructed for the test — real
        // callers get their StringHandles from Heap::alloc_string.
        let name10 = StringHandle::new(10, 0);
        let name20 = StringHandle::new(20, 0);
        let name30 = StringHandle::new(30, 0);
        let proto = Proto {
            local_vars: vec![
                LocVar {
                    name: Some(name10),
                    start_pc: 0,
                    end_pc: 5,
                },
                LocVar {
                    name: Some(name20),
                    start_pc: 2,
                    end_pc: 10,
                },
                LocVar {
                    name: Some(name30),
                    start_pc: 6,
                    end_pc: 15,
                },
            ],
            ..Proto::default()
        };

        // At pc = 3 the first two are active. The 1st is var 10, 2nd is var 20.
        assert_eq!(proto.get_local_name(1, 3), Some(name10));
        assert_eq!(proto.get_local_name(2, 3), Some(name20));
        assert_eq!(proto.get_local_name(3, 3), None);

        // At pc = 7 the second and third are active.
        assert_eq!(proto.get_local_name(1, 7), Some(name20));
        assert_eq!(proto.get_local_name(2, 7), Some(name30));
    }

    #[test]
    fn get_local_name_zero_is_none() {
        let proto = Proto {
            local_vars: vec![LocVar {
                name: Some(StringHandle::new(1, 0)),
                start_pc: 0,
                end_pc: 10,
            }],
            ..Proto::default()
        };
        assert_eq!(proto.get_local_name(0, 0), None);
    }

    // ---- LClosure / CClosure ----

    #[test]
    fn new_lclosure_preinitializes_nil_upvals() {
        let mut heap = Heap::default();
        let proto = heap.new_proto();
        let cl = heap.new_lclosure(proto, 3);
        assert_eq!(heap.lclosure(cl).upvalues.len(), 3);
        for &uv_handle in &heap.lclosure(cl).upvalues.clone() {
            match &heap.upval(uv_handle).state {
                UpValState::Closed(TValue::Nil) => {}
                other => panic!("expected Closed(Nil), got {other:?}"),
            }
        }
    }

    #[test]
    fn new_cclosure_has_nil_upvalues() {
        let mut heap = Heap::default();
        let cl: CClosureHandle = heap.new_cclosure(dummy_c_function, 2);
        assert_eq!(heap.cclosure(cl).upvalues.len(), 2);
        for v in &heap.cclosure(cl).upvalues {
            assert!(matches!(v, TValue::Nil));
        }
    }

    // ---- Open/closed upval lifecycle ----

    #[test]
    fn find_or_create_reuses_existing_open_upval() {
        let mut heap = Heap::default();
        let th = heap.alloc_thread(fresh_thread());
        let a = heap.find_or_create_open_upval(th, 5);
        let b = heap.find_or_create_open_upval(th, 5);
        assert_eq!(a, b, "same stack index should reuse handle");
        assert_eq!(heap.thread(th).open_upvals.len(), 1);
    }

    #[test]
    fn find_or_create_allocates_new_for_distinct_levels() {
        let mut heap = Heap::default();
        let th = heap.alloc_thread(fresh_thread());
        let a = heap.find_or_create_open_upval(th, 1);
        let b = heap.find_or_create_open_upval(th, 3);
        assert_ne!(a, b);
        assert_eq!(heap.thread(th).open_upvals.len(), 2);
    }

    #[test]
    fn close_open_upvals_snapshots_stack_into_closed_state() {
        let mut heap = Heap::default();
        let mut t = fresh_thread();
        t.stack[2] = TValue::Integer(42);
        t.stack[5] = TValue::Integer(99);
        let th = heap.alloc_thread(t);

        let uv_low = heap.find_or_create_open_upval(th, 2);
        let uv_high = heap.find_or_create_open_upval(th, 5);
        assert_eq!(heap.thread(th).open_upvals.len(), 2);

        // Close only those at level >= 5 — uv_high goes, uv_low stays.
        heap.close_open_upvals(th, 5);
        assert_eq!(heap.thread(th).open_upvals.len(), 1);
        assert_eq!(heap.thread(th).open_upvals[0], uv_low);

        match &heap.upval(uv_high).state {
            UpValState::Closed(TValue::Integer(99)) => {}
            other => panic!("expected Closed(Integer(99)), got {other:?}"),
        }
        // uv_low is still open.
        match &heap.upval(uv_low).state {
            UpValState::Open { stack_index: 2, .. } => {}
            other => panic!("expected Open at 2, got {other:?}"),
        }
    }

    #[test]
    fn close_open_upvals_below_level_closes_all_affected() {
        let mut heap = Heap::default();
        let mut t = fresh_thread();
        for i in 0..8 {
            t.stack[i] = TValue::Integer(i as i64);
        }
        let th = heap.alloc_thread(t);

        let handles: Vec<_> = (0..8)
            .map(|i| heap.find_or_create_open_upval(th, i))
            .collect();

        // Close at level 3: indices 3..=7 get closed, 0..=2 survive.
        heap.close_open_upvals(th, 3);
        assert_eq!(heap.thread(th).open_upvals.len(), 3);
        for (i, &h) in handles.iter().enumerate() {
            if i >= 3 {
                assert!(matches!(
                    heap.upval(h).state,
                    UpValState::Closed(TValue::Integer(_))
                ));
            } else {
                assert!(matches!(heap.upval(h).state, UpValState::Open { .. }));
            }
        }
    }
}
