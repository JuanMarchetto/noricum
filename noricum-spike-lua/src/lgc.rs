//! lgc — incremental tri-color mark-sweep garbage collector.
//!
//! Port of Lua 5.4's `lgc.c` + the GC fields on `global_State` in
//! `lstate.h`. This file lands over seven commits; see
//! `docs/lua-migration/stage-3-gc-design.md` for the full plan.
//!
//! Stage 3 v1 is **incremental mark-sweep only**. Generational mode,
//! weak tables, and finalizers are Stage 3 v2 (separate session).
//!
//! ## What's implemented now
//!
//! Commit 3 (skeleton):
//! * [`GcState`] — 5-state machine (collapsed from C's 9)
//! * [`HeapKind`] — per-arena cursor for the sweep phase
//! * [`AnyHandle`] — unified enum used by gray lists
//! * [`SweepCursor`] — `(kind, slot)` position of the incremental sweep
//! * [`GcStepResult`] — what a single [`GlobalState::gc_step`] returns
//! * [`GlobalState::gc_step`] — dispatcher scaffold
//!
//! Commit 4a (storage retrofit):
//! * Parallel `marks_*: Vec<u8>` per arena on `Heap`, initialized to
//!   `WHITE = 0` at every `alloc_*` site. Written in commit 4a, read
//!   for the first time below.
//!
//! Commit 4b (mark phase, this file):
//! * `WHITE` / `GRAY` / `BLACK` color constants.
//! * Mark-byte helpers dispatched through [`AnyHandle::kind`].
//! * [`GlobalState::start_collection`] — clears all marks to white
//!   and enqueues the root set (main thread, registry, tmnames,
//!   every interned string in the cache).
//! * [`GlobalState::propagate_one`] — pop a gray handle, paint it
//!   black, enqueue its children. Called by `gc_step` in the
//!   `Propagate` state.
//! * Per-kind child walkers (`mark_table_children`, etc.) that
//!   traverse every Lua-visible reference from a heap object.
//! * Strings are leaves — no children to walk.
//!
//! Still stubbed until commits 5-7:
//! * `Atomic` phase — in 4b just transitions to `Sweep`. No
//!   weak-table drain, no `grayagain` handling.
//! * `Sweep` phase — in 4b still walks the cursor through every
//!   arena without freeing anything.
//! * Write barriers — come with commit 6.
//! * `gc_debt` / `full_gc` — come with commit 7.
//!
//! ## Scope notes on the root set (Stage 3 v1)
//!
//! The string interning cache (`GlobalState::string_intern`) is
//! treated as a **strong root** here. In C Lua 5.4 it's effectively
//! weak — dead interned strings are cleared during sweep. Since
//! Stage 3 v1 has no weak tables yet, making it strong is
//! over-conservative (interned strings never collect) but
//! correct (no premature freeing). Stage 3 v2 will add the "clear
//! during sweep" semantics when weak-table support lands.
//!
//! ## Divergence from `stage-3-gc-design.md` (§9 open questions)
//!
//! * **`AnyHandle` location.** Parked in `lgc.rs` rather than
//!   `contract.rs` to keep the type contract free of GC concerns.
//! * **`GcState::End`.** Kept as a distinct state. Commit 7 will
//!   wire post-sweep cleanup into it.
//! * **Mark-byte encoding.** Uses plain `u8` constants rather than a
//!   `GcColor` enum. The tri-color state machine is simple enough
//!   that an enum's only payoff would be exhaustiveness checking in
//!   the mark-byte `match`, which we don't have today because the
//!   byte is read as a numeric comparison (`== WHITE`).

use crate::contract::{
    CClosureHandle, GlobalState, LClosureHandle, ProtoHandle, StringHandle, TValue, TableHandle,
    TableKey, ThreadHandle, UpValHandle, UpValState, UserDataHandle,
};

// ---------------------------------------------------------------------------
// Color constants — plain `u8` values stored in the per-kind mark Vecs
// on `Heap`. Tri-color invariant: a BLACK object never points to a
// WHITE one (enforced at propagate time by `mark_object` + child
// walkers, and by the write barriers in commit 6).
// ---------------------------------------------------------------------------

pub const WHITE: u8 = 0;
pub const GRAY: u8 = 1;
pub const BLACK: u8 = 2;

/// Maximum number of slots the incremental sweep processes in one
/// [`GlobalState::gc_step`] call. Matches `GCSWEEPMAX` in Lua 5.4's
/// `lgc.c`. Keeps a single step bounded so long sweeps don't
/// dominate a VM timeslice.
pub const GC_SWEEP_MAX: u32 = 20;

/// Debt threshold at which [`GlobalState::record_allocation`]
/// triggers an incremental step. Stage 3 v1 uses a flat constant;
/// commit 7 / v2 will wire up the step-mul / step-size tuning
/// knobs from `gc_params` so the threshold adapts to the live
/// heap size. The value (1024 bytes) is a placeholder chosen to
/// keep unit tests deterministic and runnable.
pub const GC_DEBT_THRESHOLD: i64 = 1024;

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

    /// Slot index inside the matching arena's slot Vec. Constant-time.
    pub const fn slot(self) -> u32 {
        match self {
            AnyHandle::String(h) => h.slot,
            AnyHandle::Table(h) => h.slot,
            AnyHandle::Proto(h) => h.slot,
            AnyHandle::LClosure(h) => h.slot,
            AnyHandle::CClosure(h) => h.slot,
            AnyHandle::UpVal(h) => h.slot,
            AnyHandle::Thread(h) => h.slot,
            AnyHandle::UserData(h) => h.slot,
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
// TValue / TableKey → AnyHandle conversion.
// ---------------------------------------------------------------------------

/// Extract the collectable handle from a tagged Lua value, or
/// `None` for leaf values (nil, bools, numbers, light userdata,
/// light C function pointer). Used by the child walkers to avoid
/// re-matching `TValue` at every call site.
const fn any_handle_from_tvalue(v: TValue) -> Option<AnyHandle> {
    match v {
        TValue::ShortString(h) | TValue::LongString(h) => Some(AnyHandle::String(h)),
        TValue::Table(h) => Some(AnyHandle::Table(h)),
        TValue::LuaClosure(h) => Some(AnyHandle::LClosure(h)),
        TValue::CClosure(h) => Some(AnyHandle::CClosure(h)),
        TValue::UserData(h) => Some(AnyHandle::UserData(h)),
        TValue::Thread(h) => Some(AnyHandle::Thread(h)),
        TValue::Nil
        | TValue::False
        | TValue::True
        | TValue::Integer(_)
        | TValue::Number(_)
        | TValue::LightUserData(_)
        | TValue::LightCFunction(_) => None,
    }
}

/// Same conversion for table keys. The variants that can hold a
/// collectable handle are a strict subset of [`TValue`]'s; the rest
/// (bool, integer, number-bitpattern, light userdata as usize,
/// light C function as usize) are leaves.
const fn any_handle_from_key(k: TableKey) -> Option<AnyHandle> {
    match k {
        TableKey::ShortString(h) | TableKey::LongString(h) => Some(AnyHandle::String(h)),
        TableKey::Table(h) => Some(AnyHandle::Table(h)),
        TableKey::LuaClosure(h) => Some(AnyHandle::LClosure(h)),
        TableKey::CClosure(h) => Some(AnyHandle::CClosure(h)),
        TableKey::UserData(h) => Some(AnyHandle::UserData(h)),
        TableKey::Thread(h) => Some(AnyHandle::Thread(h)),
        TableKey::False
        | TableKey::True
        | TableKey::Integer(_)
        | TableKey::Number(_)
        | TableKey::LightUserData(_)
        | TableKey::LightCFunction(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Mark phase helpers and the main dispatcher.
// ---------------------------------------------------------------------------

impl GlobalState {
    /// Advance the GC one incremental step.
    ///
    /// Post commit 4b: `Pause` and `Propagate` do real work — clearing
    /// marks, enqueuing roots, walking the gray list with child
    /// traversal. `Atomic`, `Sweep`, and `End` are still skeleton
    /// stubs (commits 5–7).
    pub fn gc_step(&mut self) -> GcStepResult {
        match self.gc_state {
            GcState::Pause => {
                self.start_collection();
                GcStepResult::Progressed(1)
            }
            GcState::Propagate => {
                if self.propagate_one() {
                    // Made progress; remain in Propagate.
                    GcStepResult::Progressed(1)
                } else {
                    // Gray list drained → advance to the atomic phase.
                    self.gc_state = GcState::Atomic;
                    GcStepResult::Progressed(1)
                }
            }
            GcState::Atomic => {
                // Skeleton: no atomic work yet. Commit 5 drains
                // `grayagain`, commit 6 flips the current-white bit,
                // Stage 3 v2 handles weak tables.
                self.gc_sweep_cursor = SweepCursor::default();
                self.gc_state = GcState::Sweep;
                GcStepResult::Progressed(1)
            }
            GcState::Sweep => {
                let swept = self.sweep_step(GC_SWEEP_MAX);
                GcStepResult::Progressed(swept.max(1) as i64)
            }
            GcState::End => {
                self.gc_state = GcState::Pause;
                GcStepResult::FinishedCycle
            }
        }
    }

    /// Start a new GC cycle: reset all marks, clear gray lists,
    /// enqueue every root, and transition to `Propagate`.
    ///
    /// Root set for Stage 3 v1:
    /// * the main thread
    /// * the registry table
    /// * every interned metamethod-name string (`tm_names`)
    /// * every interned string in the short-string cache
    ///   (over-conservative pending weak-table support in v2)
    fn start_collection(&mut self) {
        self.reset_all_marks();
        self.gc_gray.clear();
        self.gc_grayagain.clear();

        // Main thread.
        if let Some(main) = self.main_thread {
            self.enqueue_root(AnyHandle::Thread(main));
        }
        // Registry table.
        if let Some(reg) = self.registry {
            self.enqueue_root(AnyHandle::Table(reg));
        }
        // Interned metamethod names.
        let tm_names: Vec<StringHandle> = self.tm_names.clone();
        for name in tm_names {
            self.enqueue_root(AnyHandle::String(name));
        }
        // Every short string that lives in the intern cache.
        // Over-conservative: Stage 3 v2 will treat these as weak.
        let interned: Vec<StringHandle> =
            self.string_intern.values().flatten().copied().collect();
        for s in interned {
            self.enqueue_root(AnyHandle::String(s));
        }

        self.gc_state = GcState::Propagate;
    }

    /// Pop one handle off the gray frontier, paint it black, and
    /// enqueue every collectable it references. Returns `true` if
    /// work was done, `false` if the gray list was already empty
    /// (so `gc_step` can transition to the atomic phase).
    fn propagate_one(&mut self) -> bool {
        let Some(handle) = self.gc_gray.pop() else {
            return false;
        };
        self.set_mark(handle, BLACK);
        self.visit_children(handle);
        true
    }

    /// Mark a single handle. If it's already gray or black, this is
    /// a no-op — idempotent is load-bearing because child walkers
    /// don't de-duplicate before calling.
    fn mark_object(&mut self, handle: AnyHandle) {
        if self.mark_of(handle) == WHITE {
            self.set_mark(handle, GRAY);
            self.gc_gray.push(handle);
        }
    }

    /// Shorthand that also paints the root gray so an early query
    /// sees a consistent state before propagate visits it.
    fn enqueue_root(&mut self, handle: AnyHandle) {
        self.mark_object(handle);
    }

    /// Current mark byte for `handle`. Panics on a stale handle
    /// (out-of-bounds slot index) — the caller should have
    /// enqueued a live handle.
    fn mark_of(&self, handle: AnyHandle) -> u8 {
        let slot = handle.slot() as usize;
        match handle {
            AnyHandle::String(_) => self.heap.marks_strings[slot],
            AnyHandle::Table(_) => self.heap.marks_tables[slot],
            AnyHandle::Proto(_) => self.heap.marks_protos[slot],
            AnyHandle::LClosure(_) => self.heap.marks_lclosures[slot],
            AnyHandle::CClosure(_) => self.heap.marks_cclosures[slot],
            AnyHandle::UpVal(_) => self.heap.marks_upvals[slot],
            AnyHandle::Thread(_) => self.heap.marks_threads[slot],
            AnyHandle::UserData(_) => self.heap.marks_userdata[slot],
        }
    }

    /// Write the mark byte for `handle`. Same panic semantics as
    /// [`GlobalState::mark_of`].
    fn set_mark(&mut self, handle: AnyHandle, color: u8) {
        let slot = handle.slot() as usize;
        match handle {
            AnyHandle::String(_) => self.heap.marks_strings[slot] = color,
            AnyHandle::Table(_) => self.heap.marks_tables[slot] = color,
            AnyHandle::Proto(_) => self.heap.marks_protos[slot] = color,
            AnyHandle::LClosure(_) => self.heap.marks_lclosures[slot] = color,
            AnyHandle::CClosure(_) => self.heap.marks_cclosures[slot] = color,
            AnyHandle::UpVal(_) => self.heap.marks_upvals[slot] = color,
            AnyHandle::Thread(_) => self.heap.marks_threads[slot] = color,
            AnyHandle::UserData(_) => self.heap.marks_userdata[slot] = color,
        }
    }

    /// Paint every slot in every arena white. Called at the start
    /// of each collection cycle. Uses `fill` rather than iterating
    /// so the walk stays cache-friendly.
    fn reset_all_marks(&mut self) {
        self.heap.marks_strings.fill(WHITE);
        self.heap.marks_tables.fill(WHITE);
        self.heap.marks_protos.fill(WHITE);
        self.heap.marks_lclosures.fill(WHITE);
        self.heap.marks_cclosures.fill(WHITE);
        self.heap.marks_upvals.fill(WHITE);
        self.heap.marks_threads.fill(WHITE);
        self.heap.marks_userdata.fill(WHITE);
    }

    /// Dispatch to the per-kind child walker. Called after
    /// `set_mark(handle, BLACK)` in [`GlobalState::propagate_one`].
    /// Strings are leaves so their branch is empty.
    fn visit_children(&mut self, handle: AnyHandle) {
        match handle {
            AnyHandle::String(_) => {}
            AnyHandle::Table(h) => self.mark_table_children(h),
            AnyHandle::Proto(h) => self.mark_proto_children(h),
            AnyHandle::LClosure(h) => self.mark_lclosure_children(h),
            AnyHandle::CClosure(h) => self.mark_cclosure_children(h),
            AnyHandle::UpVal(h) => self.mark_upval_children(h),
            AnyHandle::Thread(h) => self.mark_thread_children(h),
            AnyHandle::UserData(h) => self.mark_userdata_children(h),
        }
    }

    // --- Per-kind child walkers ------------------------------------
    //
    // All walkers follow the same pattern:
    //   1. Collect every child `AnyHandle` into a local `Vec`,
    //      holding only an immutable borrow of `self.heap`.
    //   2. Drop the borrow (the local scope ends).
    //   3. Iterate the local Vec, calling `mark_object` on each.
    //
    // Step 2 is non-negotiable because `mark_object` takes
    // `&mut self` to push onto `gc_gray` and update mark bytes,
    // which conflicts with any live shared borrow of `self.heap`.
    // "Collect first, mutate later" is cleaner than splitting the
    // borrow manually and avoids lifetime gymnastics.

    fn mark_table_children(&mut self, handle: TableHandle) {
        let children: Vec<AnyHandle> = {
            let t = self.heap.tables[handle.slot as usize]
                .as_ref()
                .expect("marking freed table");
            let mut out =
                Vec::with_capacity(t.array.len() + (t.hash.len() * 2) + 1);
            for value in &t.array {
                if let Some(c) = any_handle_from_tvalue(*value) {
                    out.push(c);
                }
            }
            for (key, value) in &t.hash {
                if let Some(c) = any_handle_from_key(*key) {
                    out.push(c);
                }
                if let Some(c) = any_handle_from_tvalue(*value) {
                    out.push(c);
                }
            }
            if let Some(mt) = t.metatable {
                out.push(AnyHandle::Table(mt));
            }
            out
        };
        for child in children {
            self.mark_object(child);
        }
    }

    fn mark_proto_children(&mut self, handle: ProtoHandle) {
        let children: Vec<AnyHandle> = {
            let p = self.heap.protos[handle.slot as usize]
                .as_ref()
                .expect("marking freed proto");
            let mut out = Vec::new();
            for value in &p.constants {
                if let Some(c) = any_handle_from_tvalue(*value) {
                    out.push(c);
                }
            }
            for inner in &p.inner_protos {
                out.push(AnyHandle::Proto(*inner));
            }
            for upv in &p.upvalues {
                if let Some(name) = upv.name {
                    out.push(AnyHandle::String(name));
                }
            }
            for lv in &p.local_vars {
                if let Some(name) = lv.name {
                    out.push(AnyHandle::String(name));
                }
            }
            if let Some(src) = p.source {
                out.push(AnyHandle::String(src));
            }
            out
        };
        for child in children {
            self.mark_object(child);
        }
    }

    fn mark_lclosure_children(&mut self, handle: LClosureHandle) {
        let children: Vec<AnyHandle> = {
            let l = self.heap.lclosures[handle.slot as usize]
                .as_ref()
                .expect("marking freed lclosure");
            let mut out = Vec::with_capacity(l.upvalues.len() + 1);
            out.push(AnyHandle::Proto(l.proto));
            for uv in &l.upvalues {
                out.push(AnyHandle::UpVal(*uv));
            }
            out
        };
        for child in children {
            self.mark_object(child);
        }
    }

    fn mark_cclosure_children(&mut self, handle: CClosureHandle) {
        let children: Vec<AnyHandle> = {
            let c = self.heap.cclosures[handle.slot as usize]
                .as_ref()
                .expect("marking freed cclosure");
            let mut out = Vec::new();
            for value in &c.upvalues {
                if let Some(child) = any_handle_from_tvalue(*value) {
                    out.push(child);
                }
            }
            out
        };
        for child in children {
            self.mark_object(child);
        }
    }

    fn mark_upval_children(&mut self, handle: UpValHandle) {
        let children: Vec<AnyHandle> = {
            let u = self.heap.upvals[handle.slot as usize]
                .as_ref()
                .expect("marking freed upval");
            let mut out = Vec::with_capacity(1);
            match &u.state {
                UpValState::Open { thread, .. } => {
                    out.push(AnyHandle::Thread(*thread));
                }
                UpValState::Closed(value) => {
                    if let Some(c) = any_handle_from_tvalue(*value) {
                        out.push(c);
                    }
                }
            }
            out
        };
        for child in children {
            self.mark_object(child);
        }
    }

    fn mark_thread_children(&mut self, handle: ThreadHandle) {
        let children: Vec<AnyHandle> = {
            let t = self.heap.threads[handle.slot as usize]
                .as_ref()
                .expect("marking freed thread");
            let mut out = Vec::with_capacity(t.stack.len() + t.open_upvals.len());
            for value in &t.stack {
                if let Some(c) = any_handle_from_tvalue(*value) {
                    out.push(c);
                }
            }
            for uv in &t.open_upvals {
                out.push(AnyHandle::UpVal(*uv));
            }
            out
        };
        for child in children {
            self.mark_object(child);
        }
    }

    fn mark_userdata_children(&mut self, handle: UserDataHandle) {
        let children: Vec<AnyHandle> = {
            let u = self.heap.userdata[handle.slot as usize]
                .as_ref()
                .expect("marking freed userdata");
            let mut out = Vec::with_capacity(u.user_values.len() + 1);
            if let Some(mt) = u.metatable {
                out.push(AnyHandle::Table(mt));
            }
            for value in &u.user_values {
                if let Some(c) = any_handle_from_tvalue(*value) {
                    out.push(c);
                }
            }
            out
        };
        for child in children {
            self.mark_object(child);
        }
    }

    // --- Sweep phase (commit 5) ------------------------------------
    //
    // Incremental sweep: walk at most `max_slots` slots per call,
    // starting from `gc_sweep_cursor`. For each live slot whose
    // mark byte is `WHITE`, call the matching `free_*` method on
    // `Heap` — which bumps the generation counter so any stale
    // handle becomes a loud panic on next access.
    //
    // When the cursor falls off the end of an arena, advance to
    // the next `HeapKind` with slot reset to 0. When the cursor
    // falls off `HeapKind::UserData`, transition to `GcState::End`.
    //
    // Black slots are left in place. The next cycle's
    // `start_collection` clears them back to `WHITE`, so no
    // explicit "paint black to white" pass is needed here. This is
    // the single-white simplification; the dual-white refinement
    // (needed for correct handling of objects allocated during
    // sweep) lands alongside the write barriers in commit 6.

    /// Sweep up to `max_slots` cursor positions. Returns how many
    /// slots were visited (not how many were freed). Updates
    /// `gc_sweep_cursor` as it walks and transitions to
    /// [`GcState::End`] when every arena has been exhausted.
    fn sweep_step(&mut self, max_slots: u32) -> u32 {
        let mut swept: u32 = 0;
        while swept < max_slots {
            let cursor = self.gc_sweep_cursor;
            let arena_len = self.arena_len(cursor.kind) as u32;
            if cursor.slot >= arena_len {
                // Done with this arena — advance to the next kind.
                match cursor.kind.next() {
                    Some(next_kind) => {
                        self.gc_sweep_cursor = SweepCursor {
                            kind: next_kind,
                            slot: 0,
                        };
                        continue;
                    }
                    None => {
                        // Every arena exhausted — cycle is done.
                        self.gc_state = GcState::End;
                        return swept;
                    }
                }
            }
            self.sweep_one_slot(cursor.kind, cursor.slot);
            self.gc_sweep_cursor.slot += 1;
            swept += 1;
        }
        swept
    }

    /// Free the slot at `(kind, slot)` if it holds a live object
    /// whose mark byte is `WHITE`. No-op for freed slots and for
    /// reachable (BLACK) objects.
    fn sweep_one_slot(&mut self, kind: HeapKind, slot: u32) {
        let slot_usize = slot as usize;
        match kind {
            HeapKind::String => {
                if self.heap.strings[slot_usize].is_some()
                    && self.heap.marks_strings[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_strings[slot_usize];
                    self.heap.free_string(StringHandle::new(slot, gen));
                }
            }
            HeapKind::Table => {
                if self.heap.tables[slot_usize].is_some()
                    && self.heap.marks_tables[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_tables[slot_usize];
                    self.heap.free_table(TableHandle::new(slot, gen));
                }
            }
            HeapKind::Proto => {
                if self.heap.protos[slot_usize].is_some()
                    && self.heap.marks_protos[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_protos[slot_usize];
                    self.heap.free_proto(ProtoHandle::new(slot, gen));
                }
            }
            HeapKind::LClosure => {
                if self.heap.lclosures[slot_usize].is_some()
                    && self.heap.marks_lclosures[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_lclosures[slot_usize];
                    self.heap.free_lclosure(LClosureHandle::new(slot, gen));
                }
            }
            HeapKind::CClosure => {
                if self.heap.cclosures[slot_usize].is_some()
                    && self.heap.marks_cclosures[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_cclosures[slot_usize];
                    self.heap.free_cclosure(CClosureHandle::new(slot, gen));
                }
            }
            HeapKind::UpVal => {
                if self.heap.upvals[slot_usize].is_some()
                    && self.heap.marks_upvals[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_upvals[slot_usize];
                    self.heap.free_upval(UpValHandle::new(slot, gen));
                }
            }
            HeapKind::Thread => {
                if self.heap.threads[slot_usize].is_some()
                    && self.heap.marks_threads[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_threads[slot_usize];
                    self.heap.free_thread(ThreadHandle::new(slot, gen));
                }
            }
            HeapKind::UserData => {
                if self.heap.userdata[slot_usize].is_some()
                    && self.heap.marks_userdata[slot_usize] == WHITE
                {
                    let gen = self.heap.generations_userdata[slot_usize];
                    self.heap.free_userdata(UserDataHandle::new(slot, gen));
                }
            }
        }
    }

    /// How many slots currently exist in the arena for `kind`.
    /// Used by [`GlobalState::sweep_step`] to decide when the
    /// cursor has fallen off the end of an arena.
    fn arena_len(&self, kind: HeapKind) -> usize {
        match kind {
            HeapKind::String => self.heap.strings.len(),
            HeapKind::Table => self.heap.tables.len(),
            HeapKind::Proto => self.heap.protos.len(),
            HeapKind::LClosure => self.heap.lclosures.len(),
            HeapKind::CClosure => self.heap.cclosures.len(),
            HeapKind::UpVal => self.heap.upvals.len(),
            HeapKind::Thread => self.heap.threads.len(),
            HeapKind::UserData => self.heap.userdata.len(),
        }
    }

    // --- Write barriers (commit 6) ---------------------------------
    //
    // The main tri-color invariant is: a BLACK object never points
    // to a WHITE one. Sweep-incrementality breaks this invariant as
    // soon as the program mutates a black object to reference a
    // freshly allocated white one, so every mutation site must
    // restore the invariant by calling one of these barriers.
    //
    // `barrier_forward` — called when a black parent gains a new
    // white child. During propagate, mark the child so the
    // invariant holds. During sweep, repaint the parent white so
    // the next cycle reconsiders it (and so no other barrier fires
    // for it until then).
    //
    // `barrier_backward` — called when a black container object
    // (typically a table) undergoes a mutation we'd rather
    // retraverse than walk eagerly. Repaint gray and push onto
    // `gc_grayagain`, which the atomic phase (commit 5 / 6) will
    // drain.
    //
    // Stage 3 v1 notes:
    // * No dual-white yet. Single WHITE means objects allocated
    //   during sweep and left ahead of the cursor get freed. Callers
    //   avoid this by not mutating during sweep; a future commit
    //   (Stage 3 v2 or when the VM forces it) replaces `WHITE` with
    //   `WHITE_OLD` / `WHITE_NEW` and flips `gc_current_white` in
    //   the atomic phase.
    // * No generational age bits. Just the tri-color.
    // * Barriers are no-ops when there's no invariant to maintain
    //   (parent isn't black, or child isn't white). C Lua's
    //   `luaC_barrier_` has the same shape.

    /// True when the mark-phase invariant (BLACK never points to
    /// WHITE) still needs to hold at this point in the cycle.
    /// Matches C's `keepinvariant(g)` macro. Pause counts as
    /// "invariant holds" because nothing changed since the last
    /// sweep left everything correctly painted.
    pub const fn keep_invariant(&self) -> bool {
        matches!(
            self.gc_state,
            GcState::Pause | GcState::Propagate | GcState::Atomic
        )
    }

    /// Is this handle currently painted black?
    pub fn is_black(&self, handle: AnyHandle) -> bool {
        self.mark_of(handle) == BLACK
    }

    /// Is this handle currently painted white?
    pub fn is_white(&self, handle: AnyHandle) -> bool {
        self.mark_of(handle) == WHITE
    }

    /// Forward write barrier. Call when a black parent acquires a
    /// reference to a white child. Restores the tri-color invariant
    /// by either marking the child (during propagate) or repainting
    /// the parent white (during sweep, so the next cycle revisits
    /// it).
    ///
    /// No-op if `parent` isn't currently black or `child` isn't
    /// currently white — barriers are cheap to call unconditionally
    /// from every mutation site because most calls fall through
    /// this short-circuit.
    pub fn barrier_forward(&mut self, parent: AnyHandle, child: AnyHandle) {
        if !self.is_black(parent) || !self.is_white(child) {
            return;
        }
        if self.keep_invariant() {
            // Mark phase is still responsible for reachability.
            // Mark the child so the invariant is preserved.
            self.mark_object(child);
        } else {
            // Sweep phase. The mark set is already final for this
            // cycle; repaint the parent white so sweep will
            // reconsider it, and so subsequent mutations on the
            // same parent don't re-fire this barrier.
            self.set_mark(parent, WHITE);
        }
    }

    /// Backward write barrier. Call when a black container-shaped
    /// object (typically a table) mutates and you'd rather defer
    /// the re-traversal of its children to the atomic phase than
    /// mark them eagerly. Repaints `parent` gray and pushes it
    /// onto `gc_grayagain` where the atomic phase (commit 5/6)
    /// will drain it.
    ///
    /// No-op if `parent` isn't currently black. Idempotent in the
    /// sense that repeated calls on the same parent push multiple
    /// entries onto `gc_grayagain`, but the atomic drain
    /// de-duplicates via the mark byte (a GRAY handle popped off
    /// grayagain won't be re-enqueued).
    pub fn barrier_backward(&mut self, parent: AnyHandle) {
        if !self.is_black(parent) {
            return;
        }
        self.set_mark(parent, GRAY);
        self.gc_grayagain.push(parent);
    }

    // --- full_gc driver + allocation debt (commit 7) ---------------
    //
    // The incremental step driver runs gc_step in a loop until the
    // cycle finishes. For Stage 3 v1 tests this is the primary
    // driver since we don't yet have a VM running allocations on a
    // main loop — drive_full_cycle (test helper) wraps this with a
    // safety limit.
    //
    // The allocation-debt accounting layer lets callers (Stage 4
    // ltable, Stage 5 VM, Stage 4 lauxlib) trigger an incremental
    // step from inside their alloc paths without thinking about
    // the state machine. Each call to `record_allocation` bumps
    // `gc_debt` by the allocation's byte cost; once the debt
    // crosses `GC_DEBT_THRESHOLD`, the next call runs one step and
    // resets the debt.

    /// Run the incremental collector to completion. Calls
    /// [`GlobalState::gc_step`] in a tight loop until the cycle
    /// finishes and the state machine returns to
    /// [`GcState::Pause`]. Safe to call from anywhere except
    /// inside another `full_gc` — the state machine is single-
    /// threaded and re-entry would duplicate work.
    pub fn full_gc(&mut self) {
        loop {
            if matches!(self.gc_step(), GcStepResult::FinishedCycle) {
                return;
            }
        }
    }

    /// Record that `bytes` bytes have just been allocated. Bumps
    /// [`GlobalState::gc_debt`]; when the debt crosses
    /// [`GC_DEBT_THRESHOLD`] the collector runs one incremental
    /// step and the debt resets.
    ///
    /// Stage 3 v1 uses a single-step trigger (one `gc_step` per
    /// threshold crossing). Commit 7 / v2 will integrate the
    /// full `stepmul`/`stepsize` logic from `gc_params` so the
    /// step size scales with the live heap.
    pub fn record_allocation(&mut self, bytes: usize) {
        self.gc_debt = self.gc_debt.saturating_add(bytes as i64);
        if self.gc_debt >= GC_DEBT_THRESHOLD {
            self.gc_debt = 0;
            let _ = self.gc_step();
        }
    }
}

// ---------------------------------------------------------------------------
// Approximate object size estimator — used by allocation-debt callers
// that don't have an exact byte count for the object they just created.
// ---------------------------------------------------------------------------

/// Rough byte cost of a freshly-allocated object in arena `kind`.
/// Uses `mem::size_of` on the matching heap type, which covers the
/// stack-side struct but ignores any heap allocations the type
/// owns (e.g., a `Table`'s `Vec<TValue>` array part). That's
/// intentional for Stage 3 v1: the estimator is meant to drive the
/// collector's step cadence, not to be a true live-bytes
/// accountant. Stage 3 v2 or later can add per-object precise
/// accounting if the step cadence becomes a tuning concern.
pub const fn approx_object_size(kind: HeapKind) -> usize {
    match kind {
        HeapKind::String => std::mem::size_of::<crate::contract::LuaString>(),
        HeapKind::Table => std::mem::size_of::<crate::contract::Table>(),
        HeapKind::Proto => std::mem::size_of::<crate::contract::Proto>(),
        HeapKind::LClosure => std::mem::size_of::<crate::contract::LClosure>(),
        HeapKind::CClosure => std::mem::size_of::<crate::contract::CClosure>(),
        HeapKind::UpVal => std::mem::size_of::<crate::contract::UpVal>(),
        HeapKind::Thread => std::mem::size_of::<crate::contract::Thread>(),
        HeapKind::UserData => std::mem::size_of::<crate::contract::UserData>(),
    }
}

// ---------------------------------------------------------------------------
// Tests — smoke-test the skeleton + the new mark phase.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        LClosure, LuaString, Proto, Table, TableKey, Thread, UpVal, UpValState,
    };

    fn fresh_string(bytes: &[u8]) -> LuaString {
        LuaString {
            bytes: bytes.to_vec(),
            hash: 0,
            reserved: 0,
            is_long: false,
            hash_ready: false,
        }
    }

    /// Drive the state machine from Pause all the way back to Pause.
    /// Used by several tests that want to observe "everything
    /// reachable is BLACK after a full cycle".
    fn drive_full_cycle(g: &mut GlobalState) {
        for _ in 0..1024 {
            if matches!(g.gc_step(), GcStepResult::FinishedCycle) {
                return;
            }
        }
        panic!("gc cycle did not finish within 1024 steps");
    }

    // ---- skeleton tests from commit 3 (still required) -------------

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
        assert!(HeapKind::UserData.next().is_none());
    }

    #[test]
    fn any_handle_kind_round_trips_for_every_variant() {
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
    fn empty_state_cycle_walks_every_state_and_returns_to_pause() {
        // With no roots and no objects, start_collection enqueues
        // nothing, propagate_one immediately returns false, and the
        // rest of the cycle walks the skeleton sweep cursor.
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
        assert!(finished, "empty cycle never finished within 64 steps");
        assert_eq!(g.gc_state, GcState::Pause);
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
    fn sweep_phase_completes_empty_heap_in_a_single_step() {
        // With the real sweep (commit 5), arenas with zero slots
        // advance the cursor without visiting any slot. Since
        // GC_SWEEP_MAX (20) is larger than the number of
        // HeapKind variants (8), a single gc_step in the Sweep
        // state walks the cursor through every arena and
        // transitions to End.
        let mut g = GlobalState::default();
        while g.gc_state != GcState::Sweep {
            g.gc_step();
        }
        assert_eq!(g.gc_state, GcState::Sweep);
        g.gc_step();
        assert_eq!(g.gc_state, GcState::End);
    }

    // ---- commit 4b: mark-phase tests -------------------------------

    #[test]
    fn start_collection_resets_every_mark_to_white() {
        // Poison a mark byte in each arena, then run start_collection
        // (via the Pause → Propagate transition). Every byte must
        // come back to WHITE regardless of which arena it's in.
        let mut g = GlobalState::default();
        let s = g.heap.alloc_string(fresh_string(b"x"));
        let t = g.heap.alloc_table(Table::default());
        g.heap.marks_strings[s.slot as usize] = BLACK;
        g.heap.marks_tables[t.slot as usize] = GRAY;

        // Drive through Pause so start_collection runs.
        let _ = g.gc_step();
        assert_eq!(g.gc_state, GcState::Propagate);

        // Poisoned bytes must be white now. (Nothing in the root
        // set references them, so they stay white through the rest
        // of the cycle too — but we only assert the reset here.)
        assert_eq!(g.heap.marks_strings[s.slot as usize], WHITE);
        assert_eq!(g.heap.marks_tables[t.slot as usize], WHITE);
    }

    #[test]
    fn start_collection_enqueues_main_thread_registry_and_tm_names() {
        // Plant each root type and verify it lands in the gray
        // frontier. `start_collection` runs `mark_object` on each
        // root, which paints it GRAY and pushes it to gc_gray.
        let mut g = GlobalState::default();

        let thread_handle = g.heap.alloc_thread(Thread::default());
        g.main_thread = Some(thread_handle);

        let reg_handle = g.heap.alloc_table(Table::default());
        g.registry = Some(reg_handle);

        let name_a = g.heap.alloc_string(fresh_string(b"__index"));
        let name_b = g.heap.alloc_string(fresh_string(b"__gc"));
        g.tm_names = vec![name_a, name_b];

        let _ = g.gc_step(); // Pause → Propagate (runs start_collection)

        // Every root must have been painted GRAY and queued.
        assert_eq!(g.heap.marks_threads[thread_handle.slot as usize], GRAY);
        assert_eq!(g.heap.marks_tables[reg_handle.slot as usize], GRAY);
        assert_eq!(g.heap.marks_strings[name_a.slot as usize], GRAY);
        assert_eq!(g.heap.marks_strings[name_b.slot as usize], GRAY);

        // And it should be a proper gray list, not empty.
        assert!(!g.gc_gray.is_empty());
        assert!(g.gc_gray.contains(&AnyHandle::Thread(thread_handle)));
        assert!(g.gc_gray.contains(&AnyHandle::Table(reg_handle)));
        assert!(g.gc_gray.contains(&AnyHandle::String(name_a)));
        assert!(g.gc_gray.contains(&AnyHandle::String(name_b)));
    }

    #[test]
    fn mark_phase_paints_reachable_table_black_through_registry() {
        // Registry holds a table → run a full cycle → the table
        // must end up BLACK. This is the simplest positive case.
        let mut g = GlobalState::default();
        let reg = g.heap.alloc_table(Table::default());
        g.registry = Some(reg);
        drive_full_cycle(&mut g);
        assert_eq!(g.heap.marks_tables[reg.slot as usize], BLACK);
    }

    #[test]
    fn mark_phase_follows_table_values_to_strings_transitively() {
        // Registry → Table → String. After the cycle, the string
        // reached via the table's hash map must be BLACK.
        let mut g = GlobalState::default();
        let msg = g.heap.alloc_string(fresh_string(b"hello"));
        let mut tbl = Table::default();
        tbl.hash
            .insert(TableKey::Integer(1), TValue::ShortString(msg));
        let reg = g.heap.alloc_table(tbl);
        g.registry = Some(reg);
        drive_full_cycle(&mut g);
        assert_eq!(g.heap.marks_strings[msg.slot as usize], BLACK);
        assert_eq!(g.heap.marks_tables[reg.slot as usize], BLACK);
    }

    #[test]
    fn mark_phase_reaches_array_part_of_table() {
        // Same as above but via the array part, not the hash part.
        let mut g = GlobalState::default();
        let arr_entry = g.heap.alloc_table(Table::default());
        let mut reg_table = Table::default();
        reg_table.array.push(TValue::Table(arr_entry));
        let reg = g.heap.alloc_table(reg_table);
        g.registry = Some(reg);
        drive_full_cycle(&mut g);
        assert_eq!(g.heap.marks_tables[arr_entry.slot as usize], BLACK);
    }

    #[test]
    fn mark_phase_follows_lclosure_to_proto_and_upvalues() {
        // main_thread stack holds an LClosure. The closure's Proto
        // and every UpVal it captures must end up BLACK.
        let mut g = GlobalState::default();
        let proto = g.heap.alloc_proto(Proto::default());
        let uv1 = g.heap.alloc_upval(UpVal {
            state: UpValState::Closed(TValue::Integer(42)),
        });
        let uv2 = g.heap.alloc_upval(UpVal {
            state: UpValState::Closed(TValue::Integer(7)),
        });
        let closure = g.heap.alloc_lclosure(LClosure {
            proto,
            upvalues: vec![uv1, uv2],
        });
        let mut main_thread = Thread::default();
        main_thread.stack.push(TValue::LuaClosure(closure));
        let th = g.heap.alloc_thread(main_thread);
        g.main_thread = Some(th);

        drive_full_cycle(&mut g);

        assert_eq!(g.heap.marks_lclosures[closure.slot as usize], BLACK);
        assert_eq!(g.heap.marks_protos[proto.slot as usize], BLACK);
        assert_eq!(g.heap.marks_upvals[uv1.slot as usize], BLACK);
        assert_eq!(g.heap.marks_upvals[uv2.slot as usize], BLACK);
    }

    #[test]
    fn mark_phase_follows_proto_source_and_constants() {
        // Proto references several strings: source filename plus
        // constants. All must end up BLACK.
        let mut g = GlobalState::default();
        let src = g.heap.alloc_string(fresh_string(b"@main.lua"));
        let konst = g.heap.alloc_string(fresh_string(b"answer"));
        let p = Proto {
            source: Some(src),
            constants: vec![TValue::ShortString(konst)],
            ..Proto::default()
        };
        let proto = g.heap.alloc_proto(p);

        // Register the proto via the registry.
        let mut reg_table = Table::default();
        reg_table.array.push(TValue::Nil); // placeholder
        let reg = g.heap.alloc_table(reg_table);
        g.registry = Some(reg);
        // Attach the proto through an LClosure sitting in the registry.
        let closure = g.heap.alloc_lclosure(LClosure {
            proto,
            upvalues: vec![],
        });
        g.heap
            .table_mut(reg)
            .array
            .push(TValue::LuaClosure(closure));

        drive_full_cycle(&mut g);
        assert_eq!(g.heap.marks_protos[proto.slot as usize], BLACK);
        assert_eq!(g.heap.marks_strings[src.slot as usize], BLACK);
        assert_eq!(g.heap.marks_strings[konst.slot as usize], BLACK);
    }

    #[test]
    fn unreachable_object_gets_freed_by_full_cycle() {
        // Allocate a table that nothing references, then run the
        // full cycle. Commit 5 sweep must free the slot and bump
        // its generation counter, while the registry-rooted table
        // survives as BLACK.
        let mut g = GlobalState::default();
        let orphan = g.heap.alloc_table(Table::default());
        let reg = g.heap.alloc_table(Table::default());
        g.registry = Some(reg);
        drive_full_cycle(&mut g);
        assert_eq!(g.heap.marks_tables[reg.slot as usize], BLACK);
        // Orphan slot has been freed and its generation bumped.
        assert!(
            g.heap.tables[orphan.slot as usize].is_none(),
            "orphan must be freed by sweep"
        );
        assert_eq!(
            g.heap.generations_tables[orphan.slot as usize],
            orphan.generation + 1,
            "free_table must bump the generation counter"
        );
        assert!(
            g.heap.free_tables.contains(&orphan.slot),
            "swept slot must be on the free list"
        );
    }

    #[test]
    fn mark_phase_terminates_on_self_referential_table() {
        // A table whose hash part points back at itself. Without
        // the is-already-marked short-circuit in `mark_object`,
        // the propagate loop would infinite-loop pushing the table
        // onto gc_gray forever. With the short-circuit, the cycle
        // terminates and the table is BLACK.
        let mut g = GlobalState::default();
        let reg = g.heap.alloc_table(Table::default());
        g.heap
            .table_mut(reg)
            .hash
            .insert(TableKey::Integer(1), TValue::Table(reg));
        g.registry = Some(reg);
        drive_full_cycle(&mut g);
        assert_eq!(g.heap.marks_tables[reg.slot as usize], BLACK);
    }

    #[test]
    fn cycle_with_roots_drains_the_gray_list_before_atomic() {
        // After propagate is done, `gc_gray` must be empty — every
        // gray handle has been drained and painted black. Tests a
        // real invariant of the mark phase.
        let mut g = GlobalState::default();
        let reg = g.heap.alloc_table(Table::default());
        g.registry = Some(reg);
        let name = g.heap.alloc_string(fresh_string(b"__index"));
        g.tm_names = vec![name];

        // Step until we're out of Propagate.
        while g.gc_state != GcState::Atomic {
            g.gc_step();
            if g.gc_state == GcState::Pause {
                panic!("never reached Atomic before wrapping back to Pause");
            }
        }
        assert!(
            g.gc_gray.is_empty(),
            "gray list must be empty when Propagate ends"
        );
        // And the reachable objects must all be BLACK by now.
        assert_eq!(g.heap.marks_tables[reg.slot as usize], BLACK);
        assert_eq!(g.heap.marks_strings[name.slot as usize], BLACK);
    }

    // ---- commit 5: sweep-phase tests -------------------------------

    #[test]
    fn sweep_frees_unreachable_string_and_pushes_slot_to_free_list() {
        let mut g = GlobalState::default();
        let ghost = g.heap.alloc_string(fresh_string(b"doomed"));
        drive_full_cycle(&mut g);
        assert!(
            g.heap.strings[ghost.slot as usize].is_none(),
            "unreachable string should be freed"
        );
        assert_eq!(
            g.heap.generations_strings[ghost.slot as usize],
            ghost.generation + 1
        );
        assert!(g.heap.free_strings.contains(&ghost.slot));
    }

    #[test]
    fn sweep_preserves_reachable_string_referenced_through_registry() {
        // A string referenced via the registry's hash part must
        // survive a full cycle — the mark phase reaches it via the
        // table walker, and sweep leaves it alone.
        let mut g = GlobalState::default();
        let alive = g.heap.alloc_string(fresh_string(b"keep-me"));
        let mut reg_table = Table::default();
        reg_table
            .hash
            .insert(TableKey::Integer(1), TValue::ShortString(alive));
        let reg = g.heap.alloc_table(reg_table);
        g.registry = Some(reg);
        drive_full_cycle(&mut g);
        assert!(
            g.heap.strings[alive.slot as usize].is_some(),
            "reachable string must survive sweep"
        );
        assert_eq!(g.heap.string(alive).bytes, b"keep-me");
    }

    #[test]
    fn sweep_frees_every_kind_of_unreachable_object() {
        // One unreachable object per arena kind. Sweep must walk
        // every arena and free each of them. Verifies that
        // `sweep_one_slot` has a branch for every HeapKind.
        let mut g = GlobalState::default();
        let s = g.heap.alloc_string(fresh_string(b"s"));
        let t = g.heap.alloc_table(Table::default());
        let p = g.heap.alloc_proto(Proto::default());
        let l = g.heap.alloc_lclosure(LClosure {
            proto: p,
            upvalues: vec![],
        });
        let uv = g.heap.alloc_upval(UpVal {
            state: UpValState::Closed(TValue::Integer(0)),
        });
        let th = g.heap.alloc_thread(Thread::default());

        drive_full_cycle(&mut g);

        assert!(g.heap.strings[s.slot as usize].is_none());
        assert!(g.heap.tables[t.slot as usize].is_none());
        assert!(g.heap.protos[p.slot as usize].is_none());
        assert!(g.heap.lclosures[l.slot as usize].is_none());
        assert!(g.heap.upvals[uv.slot as usize].is_none());
        assert!(g.heap.threads[th.slot as usize].is_none());
    }

    #[test]
    #[should_panic(expected = "points to a reused slot")]
    fn stale_handle_from_swept_slot_panics_on_access_after_realloc() {
        // Sweep frees a slot (bumping its generation). A new alloc
        // reuses that slot with a new generation. The old handle
        // must now panic — the R1 guarantee layered on top of sweep:
        // use-after-free is deterministic, not silent.
        let mut g = GlobalState::default();
        let stale = g.heap.alloc_string(fresh_string(b"stale"));
        drive_full_cycle(&mut g); // sweep frees `stale`
        let _fresh = g.heap.alloc_string(fresh_string(b"fresh"));
        let _ = g.heap.string(stale); // must panic
    }

    #[test]
    fn sweep_preserves_deeply_nested_reachable_closure_tree() {
        // main_thread -> LClosure -> Proto -> source + const
        //              ^          -> UpVal (closed) -> captured str
        // Every object in the tree must survive.
        let mut g = GlobalState::default();
        let src = g.heap.alloc_string(fresh_string(b"@main.lua"));
        let konst = g.heap.alloc_string(fresh_string(b"answer"));
        let uv_str = g.heap.alloc_string(fresh_string(b"captured"));
        let p = Proto {
            source: Some(src),
            constants: vec![TValue::ShortString(konst)],
            ..Proto::default()
        };
        let proto = g.heap.alloc_proto(p);
        let uv = g.heap.alloc_upval(UpVal {
            state: UpValState::Closed(TValue::ShortString(uv_str)),
        });
        let closure = g.heap.alloc_lclosure(LClosure {
            proto,
            upvalues: vec![uv],
        });
        let mut main = Thread::default();
        main.stack.push(TValue::LuaClosure(closure));
        let th = g.heap.alloc_thread(main);
        g.main_thread = Some(th);

        drive_full_cycle(&mut g);

        // Every handle along the tree must still resolve without
        // panicking — the generation checks in the accessors are
        // the proof of survival.
        assert_eq!(g.heap.string(src).bytes, b"@main.lua");
        assert_eq!(g.heap.string(konst).bytes, b"answer");
        assert_eq!(g.heap.string(uv_str).bytes, b"captured");
        assert!(g.heap.protos[proto.slot as usize].is_some());
        assert!(g.heap.upvals[uv.slot as usize].is_some());
        assert!(g.heap.lclosures[closure.slot as usize].is_some());
        assert!(g.heap.threads[th.slot as usize].is_some());
    }

    // ---- commit 6: write-barrier tests -----------------------------

    #[test]
    fn forward_barrier_during_propagate_marks_white_child() {
        // Black parent, white child, mid-mark phase. Forward
        // barrier must paint the child gray and push it onto the
        // main gray frontier so propagate_one picks it up.
        let mut g = GlobalState::default();
        let parent = g.heap.alloc_table(Table::default());
        let child = g.heap.alloc_string(fresh_string(b"new"));
        g.heap.marks_tables[parent.slot as usize] = BLACK;
        g.heap.marks_strings[child.slot as usize] = WHITE;
        g.gc_state = GcState::Propagate;

        g.barrier_forward(AnyHandle::Table(parent), AnyHandle::String(child));

        assert_eq!(g.heap.marks_strings[child.slot as usize], GRAY);
        assert!(g.gc_gray.contains(&AnyHandle::String(child)));
        // Parent untouched.
        assert_eq!(g.heap.marks_tables[parent.slot as usize], BLACK);
    }

    #[test]
    fn forward_barrier_during_sweep_repaints_black_parent_to_white() {
        // Same setup, but gc_state == Sweep. Forward barrier
        // must leave the child alone and repaint the parent
        // white so the next cycle will reconsider it.
        let mut g = GlobalState::default();
        let parent = g.heap.alloc_table(Table::default());
        let child = g.heap.alloc_string(fresh_string(b"new"));
        g.heap.marks_tables[parent.slot as usize] = BLACK;
        g.heap.marks_strings[child.slot as usize] = WHITE;
        g.gc_state = GcState::Sweep;

        g.barrier_forward(AnyHandle::Table(parent), AnyHandle::String(child));

        assert_eq!(g.heap.marks_tables[parent.slot as usize], WHITE);
        // Child is still white; not queued.
        assert_eq!(g.heap.marks_strings[child.slot as usize], WHITE);
        assert!(!g.gc_gray.contains(&AnyHandle::String(child)));
    }

    #[test]
    fn forward_barrier_ignores_non_black_parent() {
        // Parent is gray (still propagating), not black. Barrier
        // is a no-op because there's no invariant to restore yet.
        let mut g = GlobalState::default();
        let parent = g.heap.alloc_table(Table::default());
        let child = g.heap.alloc_string(fresh_string(b"new"));
        g.heap.marks_tables[parent.slot as usize] = GRAY;
        g.heap.marks_strings[child.slot as usize] = WHITE;
        g.gc_state = GcState::Propagate;

        g.barrier_forward(AnyHandle::Table(parent), AnyHandle::String(child));

        assert_eq!(g.heap.marks_tables[parent.slot as usize], GRAY);
        assert_eq!(g.heap.marks_strings[child.slot as usize], WHITE);
        assert!(g.gc_gray.is_empty());
    }

    #[test]
    fn backward_barrier_repaints_black_parent_gray_and_pushes_to_grayagain() {
        // Black table mutates. Backward barrier paints it gray
        // and queues it on grayagain so the atomic phase
        // re-traverses it.
        let mut g = GlobalState::default();
        let parent = g.heap.alloc_table(Table::default());
        g.heap.marks_tables[parent.slot as usize] = BLACK;
        g.gc_state = GcState::Propagate;

        g.barrier_backward(AnyHandle::Table(parent));

        assert_eq!(g.heap.marks_tables[parent.slot as usize], GRAY);
        assert_eq!(g.gc_grayagain, vec![AnyHandle::Table(parent)]);
        // Not on the main gray list — that's what grayagain exists for.
        assert!(g.gc_gray.is_empty());
    }

    #[test]
    fn backward_barrier_ignores_non_black_parent() {
        // Gray and white parents don't need the backward barrier
        // because they're either being walked already or weren't
        // visited at all.
        let mut g = GlobalState::default();
        let gray_parent = g.heap.alloc_table(Table::default());
        let white_parent = g.heap.alloc_table(Table::default());
        g.heap.marks_tables[gray_parent.slot as usize] = GRAY;
        g.heap.marks_tables[white_parent.slot as usize] = WHITE;
        g.gc_state = GcState::Propagate;

        g.barrier_backward(AnyHandle::Table(gray_parent));
        g.barrier_backward(AnyHandle::Table(white_parent));

        assert_eq!(g.heap.marks_tables[gray_parent.slot as usize], GRAY);
        assert_eq!(g.heap.marks_tables[white_parent.slot as usize], WHITE);
        assert!(g.gc_grayagain.is_empty());
    }

    // ---- commit 5 continued: cross-cycle sweep behavior ------------

    // ---- commit 7: full_gc driver + allocation debt ---------------

    #[test]
    fn full_gc_completes_a_cycle_in_one_call_and_returns_to_pause() {
        // full_gc must drive the state machine from Pause back to
        // Pause without the caller needing to manage gc_step.
        let mut g = GlobalState::default();
        let reg = g.heap.alloc_table(Table::default());
        g.registry = Some(reg);
        g.full_gc();
        assert_eq!(g.gc_state, GcState::Pause);
        assert_eq!(g.heap.marks_tables[reg.slot as usize], BLACK);
    }

    #[test]
    fn full_gc_frees_unreachable_and_preserves_reachable() {
        // End-to-end integration: unreachable object freed,
        // reachable object preserved, cycle returns to Pause.
        let mut g = GlobalState::default();
        let ghost = g.heap.alloc_table(Table::default());
        let alive = g.heap.alloc_table(Table::default());
        g.registry = Some(alive);
        g.full_gc();
        assert!(
            g.heap.tables[ghost.slot as usize].is_none(),
            "ghost must be freed"
        );
        assert!(
            g.heap.tables[alive.slot as usize].is_some(),
            "alive must survive"
        );
        assert_eq!(g.gc_state, GcState::Pause);
    }

    #[test]
    fn record_allocation_bumps_debt_without_triggering_below_threshold() {
        let mut g = GlobalState::default();
        // A single small allocation must not cross the threshold.
        g.record_allocation(64);
        assert_eq!(g.gc_debt, 64);
        assert_eq!(g.gc_state, GcState::Pause, "no step triggered yet");
        g.record_allocation(128);
        assert_eq!(g.gc_debt, 192);
        assert_eq!(g.gc_state, GcState::Pause);
    }

    #[test]
    fn record_allocation_triggers_gc_step_at_threshold_and_resets_debt() {
        // Drop a single huge allocation that crosses the
        // threshold. record_allocation must reset the debt and
        // run one gc_step. Since the state machine was in Pause,
        // the step transitions into Propagate.
        let mut g = GlobalState::default();
        g.record_allocation((GC_DEBT_THRESHOLD + 1) as usize);
        assert_eq!(g.gc_debt, 0, "debt must reset after triggering");
        assert_eq!(
            g.gc_state,
            GcState::Propagate,
            "Pause + step = Propagate"
        );
    }

    #[test]
    fn approx_object_size_is_non_zero_for_every_kind() {
        // The size estimator must report a positive byte count
        // for every arena kind, otherwise record_allocation based
        // on it would never accumulate debt for that kind.
        for kind in [
            HeapKind::String,
            HeapKind::Table,
            HeapKind::Proto,
            HeapKind::LClosure,
            HeapKind::CClosure,
            HeapKind::UpVal,
            HeapKind::Thread,
            HeapKind::UserData,
        ] {
            assert!(
                approx_object_size(kind) > 0,
                "approx_object_size({:?}) must be > 0",
                kind
            );
        }
    }

    // ---- commit 5 continued: cross-cycle sweep behavior ------------

    #[test]
    fn two_sequential_cycles_collect_the_changing_dead_set() {
        // Cycle 1: registry holds `reg`; `orphan` is unreachable
        // and gets collected. Cycle 2: swap registry to `new_reg`
        // so `reg` becomes unreachable. The start_collection
        // BLACK -> WHITE reset is what lets `reg` become collectable
        // in cycle 2; without it, it would stay BLACK forever.
        let mut g = GlobalState::default();
        let orphan = g.heap.alloc_table(Table::default());
        let reg = g.heap.alloc_table(Table::default());
        g.registry = Some(reg);

        drive_full_cycle(&mut g);
        assert!(g.heap.tables[orphan.slot as usize].is_none());
        assert!(g.heap.tables[reg.slot as usize].is_some());
        assert_eq!(g.heap.marks_tables[reg.slot as usize], BLACK);

        // Swap the registry. `reg` is now unreachable.
        let new_reg = g.heap.alloc_table(Table::default());
        g.registry = Some(new_reg);

        drive_full_cycle(&mut g);
        assert!(
            g.heap.tables[reg.slot as usize].is_none(),
            "`reg` must be collected once detached from the registry"
        );
        assert!(
            g.heap.tables[new_reg.slot as usize].is_some(),
            "new registry table must survive the second cycle"
        );
        assert_eq!(g.heap.marks_tables[new_reg.slot as usize], BLACK);
    }
}
