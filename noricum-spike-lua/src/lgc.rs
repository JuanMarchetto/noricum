//! lgc — incremental tri-color mark-sweep garbage collector.
//!
//! Port of Lua 5.4's `lgc.c` + the GC fields on `global_State` in
//! `lstate.h`. This file lands over seven commits; see
//! `docs/lua-migration/stage-3-gc-design.md` for the full plan.
//!
//! Stage 3 v1 is **incremental mark-sweep only**. Generational mode,
//! weak tables, and finalizers are Stage 3 v2 (separate session).
//!
//! ## This commit (Stage 3 / 3)
//!
//! Lands the state machine vocabulary and a **skeleton dispatcher**:
//!
//! * [`GcState`] — 5-state machine (collapsed from C's 9)
//! * [`HeapKind`] — per-arena cursor for the sweep phase
//! * [`AnyHandle`] — unified enum used by gray lists (replaces C's
//!   intrusive `GCObject*` linked list)
//! * [`SweepCursor`] — `(kind, slot)` position of the incremental sweep
//! * [`GcStepResult`] — what a single [`GlobalState::gc_step`] returns
//! * [`GlobalState::gc_step`] — advances the state machine with stubs
//!   at every phase; no real marking or sweeping yet
//!
//! Commits 4–7 fill in the real work behind `Propagate`, `Atomic`,
//! `Sweep`, and the write barriers. The skeleton here exists so that
//! the next commits can grow the phases one at a time with a pipeline
//! already in place.
//!
//! ## Divergence from `stage-3-gc-design.md` (§9 open questions)
//!
//! * **`AnyHandle` location.** Design note left this open. This commit
//!   parks it in `lgc.rs` — keeps `contract.rs` free of GC concerns,
//!   at the cost of `contract.rs` having to `use crate::lgc::*` for
//!   the four new `GlobalState` fields. Worth the cleaner separation.
//! * **`GcState::End`.** Kept as a distinct state. Collapsing it into
//!   the `Sweep → Pause` transition saves one step per cycle but
//!   costs the "post-sweep cleanup" hook that commit 7 will wire up
//!   (shrink pass, debt reset). Easier to keep it and remove later
//!   than the other way around.

use crate::contract::{
    CClosureHandle, GlobalState, LClosureHandle, ProtoHandle,
    StringHandle, TableHandle, ThreadHandle, UpValHandle, UserDataHandle,
};

// ---------------------------------------------------------------------------
// GcState — 5-state machine. Matches stage-3-gc-design.md §4.1.
// ---------------------------------------------------------------------------

/// The phase of the incremental GC state machine. Collapses Lua 5.4's
/// 9 C states (`GCSpause`, `GCSpropagate`, `GCSenteratomic`,
/// `GCSatomic`, `GCSswpallgc`, `GCSswpfinobj`, `GCSswptobefnz`,
/// `GCSswpend`, `GCScallfin`) into the five phases Stage 3 v1 actually
/// needs. Stage 3 v2 will add `Finalize` when `__gc` metamethods land.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GcState {
    /// Not collecting. Allocation pressure drives the next transition
    /// into `Propagate` via [`GlobalState::gc_step`].
    #[default]
    Pause,
    /// Walking the gray frontier, marking reachables.
    Propagate,
    /// One-shot phase: finalize the root set, flip `currentwhite`.
    Atomic,
    /// Walking every arena, freeing the previous-white objects.
    Sweep,
    /// Post-sweep cleanup (shrink pass, debt reset). Commit 7.
    End,
}

// ---------------------------------------------------------------------------
// HeapKind — per-arena dispatch tag for the sweep cursor.
// ---------------------------------------------------------------------------

/// Discriminator for the 8 object-kind arenas on [`Heap`]. Lets the
/// sweep cursor advance across arenas generically and lets
/// [`AnyHandle::kind`] report which arena a handle belongs to without
/// matching on every variant.
///
/// Variant order is part of the sweep walk order: sweep starts at
/// [`HeapKind::String`] and advances via [`HeapKind::next`] until it
/// runs off the end at [`HeapKind::UserData`]. No external code should
/// depend on the numeric repr — it's `#[repr(u8)]` only so future
/// per-kind array indexing can `as usize` the tag safely.
///
/// [`Heap`]: crate::contract::Heap
#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeapKind {
    #[default]
    String = 0,
    Table,
    Proto,
    LClosure,
    CClosure,
    UpVal,
    Thread,
    UserData,
}

impl HeapKind {
    /// Next arena in sweep order, or `None` if this is the last one.
    /// The sweep phase uses this to advance its cursor; commit 5
    /// wires it into the real sweep loop.
    pub const fn next(self) -> Option<HeapKind> {
        match self {
            HeapKind::String => Some(HeapKind::Table),
            HeapKind::Table => Some(HeapKind::Proto),
            HeapKind::Proto => Some(HeapKind::LClosure),
            HeapKind::LClosure => Some(HeapKind::CClosure),
            HeapKind::CClosure => Some(HeapKind::UpVal),
            HeapKind::UpVal => Some(HeapKind::Thread),
            HeapKind::Thread => Some(HeapKind::UserData),
            HeapKind::UserData => None,
        }
    }
}

// ---------------------------------------------------------------------------
// AnyHandle — unified handle enum for gray lists.
// ---------------------------------------------------------------------------

/// A type-erased handle to any GC-managed object. The gray list uses
/// this as its element type because C's intrusive `GCObject*` linked
/// list has no direct equivalent in our typed manual arena.
///
/// Cost of the erasure: one branch per mark dispatch (`match self`).
/// Benefit: gray lists are homogeneous `Vec<AnyHandle>` and don't
/// fragment per arena. Stage 3 design note §2.2 + §9 covers the
/// tradeoff; v2 can flatten to per-kind arrays if profiling warrants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyHandle {
    String(StringHandle),
    Table(TableHandle),
    Proto(ProtoHandle),
    LClosure(LClosureHandle),
    CClosure(CClosureHandle),
    UpVal(UpValHandle),
    Thread(ThreadHandle),
    UserData(UserDataHandle),
}

impl AnyHandle {
    /// Which arena this handle refers to. Constant-time match.
    pub const fn kind(self) -> HeapKind {
        match self {
            AnyHandle::String(_) => HeapKind::String,
            AnyHandle::Table(_) => HeapKind::Table,
            AnyHandle::Proto(_) => HeapKind::Proto,
            AnyHandle::LClosure(_) => HeapKind::LClosure,
            AnyHandle::CClosure(_) => HeapKind::CClosure,
            AnyHandle::UpVal(_) => HeapKind::UpVal,
            AnyHandle::Thread(_) => HeapKind::Thread,
            AnyHandle::UserData(_) => HeapKind::UserData,
        }
    }
}

// ---------------------------------------------------------------------------
// SweepCursor — where the incremental sweep is up to.
// ---------------------------------------------------------------------------

/// Position of the incremental sweep across the arenas. Replaces
/// C's `sweepgc` `GCObject**` pointer since our arena has no linked
/// list to chase: `kind` identifies the arena, `slot` the index
/// inside that arena's slot `Vec`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepCursor {
    pub kind: HeapKind,
    pub slot: u32,
}

// ---------------------------------------------------------------------------
// GcStepResult — what a single gc_step returns.
// ---------------------------------------------------------------------------

/// Outcome of a single incremental GC step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcStepResult {
    /// `n` work units consumed. Collector is still mid-cycle.
    Progressed(i64),
    /// Cycle finished, collector is back in [`GcState::Pause`].
    /// Commits 4+ will also reset debt counters here.
    FinishedCycle,
}

// ---------------------------------------------------------------------------
// gc_step — the dispatcher.
// ---------------------------------------------------------------------------

impl GlobalState {
    /// Advance the GC one incremental step.
    ///
    /// **Skeleton (Stage 3 / commit 3).** Every phase has a stub that
    /// advances the state machine with empty data: no root
    /// enumeration, no marking, no freeing. A full cycle from
    /// `Pause → Propagate → Atomic → Sweep → End → Pause` completes
    /// in exactly 12 step calls (1 pause, 1 propagate, 1 atomic, 8
    /// sweep steps — one per [`HeapKind`] — and 1 end). Commits 4–7
    /// fill in the real work and the step count becomes bounded by
    /// `GC_SWEEP_MAX` and the gray list length instead.
    pub fn gc_step(&mut self) -> GcStepResult {
        match self.gc_state {
            GcState::Pause => {
                self.gc_start_cycle();
                GcStepResult::Progressed(1)
            }
            GcState::Propagate => {
                // Skeleton: the gray list is never populated because
                // there's no root enumeration yet. When commit 4
                // lands this branch will pop one handle per call and
                // traverse its children. Until then, an empty gray
                // list means "propagation done, advance to atomic".
                if self.gc_gray.is_empty() {
                    self.gc_state = GcState::Atomic;
                }
                GcStepResult::Progressed(1)
            }
            GcState::Atomic => {
                // Skeleton: no atomic work (no grayagain drain, no
                // weak-table cleanup, no white flip — commits 4+5
                // add those). Position the sweep cursor at the first
                // arena and advance.
                self.gc_sweep_cursor = SweepCursor::default();
                self.gc_state = GcState::Sweep;
                GcStepResult::Progressed(1)
            }
            GcState::Sweep => {
                // Skeleton: treat every arena as "already swept"
                // (slot count is irrelevant because there are no
                // mark bytes yet). Advance the cursor one arena per
                // call; when we fall off the end, transition to End.
                match self.gc_sweep_cursor.kind.next() {
                    Some(next_kind) => {
                        self.gc_sweep_cursor = SweepCursor {
                            kind: next_kind,
                            slot: 0,
                        };
                    }
                    None => {
                        self.gc_state = GcState::End;
                    }
                }
                GcStepResult::Progressed(1)
            }
            GcState::End => {
                // Skeleton: no post-sweep cleanup yet. Commit 7 adds
                // debt reset and (if we keep it) the shrink pass.
                self.gc_state = GcState::Pause;
                GcStepResult::FinishedCycle
            }
        }
    }

    /// Start a new GC cycle: clear gray lists and move to `Propagate`.
    /// Skeleton — commit 4 will also clear mark bytes and enqueue
    /// the root set (main thread, registry, fixed/pinned handles).
    fn gc_start_cycle(&mut self) {
        self.gc_gray.clear();
        self.gc_grayagain.clear();
        self.gc_state = GcState::Propagate;
    }
}

// ---------------------------------------------------------------------------
// Tests — smoke-test the skeleton dispatcher without any real GC work.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_kind_next_covers_every_arena_exactly_once() {
        let mut seen = Vec::new();
        let mut cursor = HeapKind::default();
        seen.push(cursor);
        while let Some(next) = cursor.next() {
            seen.push(next);
            cursor = next;
        }
        assert_eq!(
            seen,
            vec![
                HeapKind::String,
                HeapKind::Table,
                HeapKind::Proto,
                HeapKind::LClosure,
                HeapKind::CClosure,
                HeapKind::UpVal,
                HeapKind::Thread,
                HeapKind::UserData,
            ]
        );
        // Tail of the walk must be a true terminator, not a cycle.
        assert!(HeapKind::UserData.next().is_none());
    }

    #[test]
    fn any_handle_kind_round_trips_for_every_variant() {
        // Every AnyHandle variant must report the matching HeapKind,
        // otherwise the gray-list dispatch in commit 4 will mark the
        // wrong arena.
        assert_eq!(AnyHandle::String(StringHandle::new(0, 0)).kind(), HeapKind::String);
        assert_eq!(AnyHandle::Table(TableHandle::new(0, 0)).kind(), HeapKind::Table);
        assert_eq!(AnyHandle::Proto(ProtoHandle::new(0, 0)).kind(), HeapKind::Proto);
        assert_eq!(AnyHandle::LClosure(LClosureHandle::new(0, 0)).kind(), HeapKind::LClosure);
        assert_eq!(AnyHandle::CClosure(CClosureHandle::new(0, 0)).kind(), HeapKind::CClosure);
        assert_eq!(AnyHandle::UpVal(UpValHandle::new(0, 0)).kind(), HeapKind::UpVal);
        assert_eq!(AnyHandle::Thread(ThreadHandle::new(0, 0)).kind(), HeapKind::Thread);
        assert_eq!(AnyHandle::UserData(UserDataHandle::new(0, 0)).kind(), HeapKind::UserData);
    }

    #[test]
    fn default_global_state_parks_the_gc_in_pause() {
        let g = GlobalState::default();
        assert_eq!(g.gc_state, GcState::Pause);
        assert!(g.gc_gray.is_empty());
        assert!(g.gc_grayagain.is_empty());
        assert_eq!(g.gc_sweep_cursor, SweepCursor::default());
        assert_eq!(g.gc_sweep_cursor.kind, HeapKind::String);
        assert_eq!(g.gc_sweep_cursor.slot, 0);
    }

    #[test]
    fn gc_step_skeleton_walks_every_state_and_returns_to_pause() {
        // Drive the skeleton through one full cycle. The dispatcher
        // must touch every phase and end back in Pause with a
        // FinishedCycle result, inside a bounded number of calls.
        let mut g = GlobalState::default();
        let mut phases_seen: Vec<GcState> = vec![g.gc_state];
        let mut finished = false;
        for _ in 0..64 {
            let result = g.gc_step();
            phases_seen.push(g.gc_state);
            if matches!(result, GcStepResult::FinishedCycle) {
                finished = true;
                break;
            }
        }
        assert!(finished, "skeleton cycle never finished within 64 steps");
        assert_eq!(g.gc_state, GcState::Pause, "cycle must return to Pause");
        for expected in [
            GcState::Pause,
            GcState::Propagate,
            GcState::Atomic,
            GcState::Sweep,
            GcState::End,
        ] {
            assert!(
                phases_seen.contains(&expected),
                "phase {:?} was skipped: trace = {:?}",
                expected,
                phases_seen
            );
        }
    }

    #[test]
    fn sweep_phase_visits_every_heap_kind_before_transitioning_to_end() {
        // Drive the state machine up to Sweep, then collect every
        // HeapKind the cursor touches. Commit 5 will replace this
        // "one step per arena" walk with "GC_SWEEP_MAX slots per step
        // with overflow into the next arena", but the set of
        // arenas visited must stay the same.
        let mut g = GlobalState::default();
        while g.gc_state != GcState::Sweep {
            g.gc_step();
        }
        let mut kinds_touched: Vec<HeapKind> = vec![g.gc_sweep_cursor.kind];
        while g.gc_state == GcState::Sweep {
            g.gc_step();
            if g.gc_state == GcState::Sweep {
                kinds_touched.push(g.gc_sweep_cursor.kind);
            }
        }
        assert_eq!(
            kinds_touched,
            vec![
                HeapKind::String,
                HeapKind::Table,
                HeapKind::Proto,
                HeapKind::LClosure,
                HeapKind::CClosure,
                HeapKind::UpVal,
                HeapKind::Thread,
                HeapKind::UserData,
            ],
            "sweep must visit every HeapKind in declared order"
        );
        assert_eq!(g.gc_state, GcState::End);
    }
}
