# Stage 3 GC Design Note

**Status:** design-phase (pre-implementation). Produced during the
Stage 2→Stage 3 checkpoint session. Meant to be read once at the
start of Stage 3 work and amended as we discover things we got wrong.

**C reference:** `lgc.c` (1804 LOC) + `lgc.h` + the GC-state fields on
`global_State` in `lstate.h`. All LOC references in this doc use the
line numbers of the unmodified Lua 5.4 sources checked into
`noricum-spike-lua/`.

---

## 1. What Lua's GC actually is

Lua 5.4 ships three collectors under one set of APIs:

1. **Incremental tri-color mark-sweep** (`KGC_INC`). Default. 9 states:
   `GCSpause`, `GCSpropagate`, `GCSenteratomic`, `GCSatomic`,
   `GCSswpallgc`, `GCSswpfinobj`, `GCSswptobefnz`, `GCSswpend`,
   `GCScallfin`. Step-driven by `luaC_step`.
2. **Generational minor** (`KGC_GENMINOR`). Scans young objects only.
   Uses the same barriers as incremental but with 7 object ages
   (`G_NEW`, `G_SURVIVAL`, `G_OLD0..OLD`, `G_TOUCHED1..TOUCHED2`).
3. **Generational major** (`KGC_GENMAJOR`). Same as incremental but
   inside a generational lifetime.

Mode switching happens in `checkmajorminor` (`lgc.c`:1300-ish)
depending on the ratio of bytes that became old last cycle.

The public contract is that any collectable cycle — unreachable
objects pointing only at each other — eventually gets freed, and
`__gc` metamethods run for objects whose metatable defines one.

### 1.1 Object header bits (`marked: lu_byte`)

```
bit 7: TESTBIT   (reserved for ltests)
bit 6: FINALIZEDBIT
bit 5: BLACKBIT
bit 4: WHITE1BIT
bit 3: WHITE0BIT
bit 2..0: AGEBITS  (7 ages for generational)
```

The dual-white trick: `currentwhite` toggles between WHITE0 and WHITE1
each cycle. Objects allocated during a cycle are painted the "new"
white; anything still carrying the "old" white at sweep time is dead.
This is the only way to distinguish "new since mark started" from
"marked as dead".

### 1.2 The main invariant

> A black object can never point to a white one.

Enforced by write barriers (forward + backward). Broken by the sweep
phase deliberately (`keepinvariant(g) == false`), then restored at
the next pause when every surviving object is repainted the new
white.

### 1.3 Gray lists (the mark frontier)

C uses five intrusive lists built out of `GCObject*`:

| list | meaning |
|---|---|
| `gray` | standard gray frontier — walked by `propagatemark` |
| `grayagain` | tables etc. that need revisiting in the atomic phase |
| `weak` | tables with weak values — their white values get cleared |
| `allweak` | tables with weak keys and values |
| `ephemeron` | tables with weak keys — kept iff key is reachable |

Open upvalues and threads are conceptually gray while they live in
`openupval` / `twups`; they're revisited via `remarkupvals`.

---

## 2. Rust adaptation — what survives, what doesn't

### 2.1 Kept (Rust port mirrors C semantics)

* **Tri-color mark-sweep invariant.** Same three colors, same main
  invariant, same sweep semantics.
* **Dual white.** Keeps working as-is — the two white values are
  just bit patterns.
* **Incremental step driver.** `luaC_step` equivalent with a
  `GcState` enum.
* **Forward write barrier.** Required for correctness during
  propagate.
* **Backward write barrier.** Optimization (can be a no-op that
  defaults to "retraverse the parent on atomic"), but we'll port
  the real version eventually for the performance footprint decision 6
  implies.
* **`gcparams[]` step tuning.** Reuses `lobject::code_param` /
  `apply_param` that Stage 2 already ported. The 6 GC tuning knobs
  (pause, stepmul, stepsize, minormul, minormajor, majorminor) each
  become a single byte in a `[u8; 6]`.

### 2.2 Restructured (manual arena → no intrusive lists)

* **`allgc` is gone.** C uses an intrusive `GCObject*` linked list
  rooted at `g->allgc` to enumerate every live object. Our manual
  arena already enumerates live objects via `Heap::strings`,
  `Heap::tables`, etc. — iterating each arena's slot Vec is the
  equivalent of walking `allgc`. No list, no pointers to patch on
  free.
* **`finobj` is gone, replaced by a `HashSet<AnyHandle>` on
  `GlobalState`.** Same semantics, simpler structure.
* **`fixedgc` is gone.** Objects that should never collect (the
  metamethod names, error message strings) get a `pinned: bool`
  flag in their header byte (or live in a `HashSet<StringHandle>`
  — we'll pick at implementation time).
* **`gray` / `grayagain` / etc. become `Vec<AnyHandle>`**, where
  `AnyHandle` is a small enum over every handle kind. This is a new
  type we'll introduce in `lgc.rs`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
  ```
* **`sweepgc` cursor** is replaced by a `(kind, slot)` pair on
  `GlobalState` that remembers how far the incremental sweep has
  walked. Equivalent semantics, different shape.

### 2.3 Deferred (not in Stage 3 v1)

These land in Stage 3 v2 (or later) as follow-up sessions:

* **Generational mode.** `KGC_GENMINOR` and `KGC_GENMAJOR`. Requires
  the 7-age machinery and mode-switch logic in `checkmajorminor`.
  Decision: incremental-only for Stage 3 v1. We can always layer
  generational on top once Stage 5 exposes a mutation-heavy workload
  worth optimizing for.
* **Weak tables** (`weak`, `ephemeron`, `allweak`). Require cooperation
  from `ltable` which is Stage 4. Defer to Stage 3 v2, or to Stage 4
  when ltable lands.
* **Finalizers** (`__gc` metamethod dispatch via `GCTM` and
  `callallpendingfinalizers`). Require the VM's call machinery
  (`ldo_call`) which is Stage 5. Defer to Stage 3 v2 or later.
* **Emergency GC retry** from `lmem::tryagain`. Requires the
  allocator integration layer from Stage 4. Defer.
* **Userdata `__gc`.** Same reason as general finalizers. Defer.

---

## 3. `contract.rs` changes required

### 3.1 Per-object mark byte

**Chosen approach:** add a parallel `Vec<u8>` per object kind to the
`Heap` struct. Every slot `Vec<Option<T>>` gets a sibling
`Vec<u8>` of the same length holding the `marked` byte.

```rust
pub struct Heap {
    pub strings: Vec<Option<LuaString>>,
    pub string_marks: Vec<u8>,
    // ... same pattern for the other 7 kinds
}
```

**Why not embed in `LuaString { marked: u8, ... }`?** Two reasons:
1. Keeps `LuaString` (etc.) pristine at the type-contract level — no
   GC metadata leaking into value types that are conceptually
   arena-agnostic.
2. Cache-friendly marking: walking a flat `Vec<u8>` to mark every live
   object's color is dramatically faster than chasing a pointer per
   object into the value itself.

**Why not a `Vec<(u8, T)>` slot?** Same reason as above — breaks the
cache locality of the mark walk and doesn't gain anything.

### 3.2 Generation counters — R1 finally resolved

Handles widen from `StringHandle(pub u32)` to:

```rust
pub struct StringHandle {
    pub slot: u32,
    pub gen: u32,
}
```

8 bytes per handle instead of 4. Every arena grows a parallel
`Vec<u32>` of generation counters:

```rust
pub struct Heap {
    pub string_gens: Vec<u32>,
    // ...
}
```

`alloc_*` reads the current generation from the slot's `string_gens[i]`
and stamps it on the returned handle. `free_*` bumps
`string_gens[i] += 1` so the next `alloc_*` at the same slot index
hands out a handle with a different generation.

Every accessor method (`string`, `string_mut`, `free_string`) verifies
`handle.gen == string_gens[handle.slot as usize]` before returning and
panics on mismatch with `"StringHandle points to a reused slot
(generation {handle.gen} vs {current})"`.

**Cost:** 8-byte handles instead of 4. TValue becomes slightly larger
(the integer-valued enum has 16-byte variants regardless). Table
hashing gets marginally slower (hash 8 bytes instead of 4). These are
flat-out acceptable.

**Counter wrap:** u32 wraps after 4 billion frees of the same slot.
For a long-lived Lua interpreter that's feasible in a single session
(e.g., a web server that allocates 1000 short-lived strings/sec runs
out in 46 days). Not a blocker for Stage 3 v1; if it becomes real
we bump to `u64` generations (16-byte handles) in Stage 3 v2. Noted
as a new risk **R11** in plan.md.

### 3.3 New `GlobalState` fields

```rust
pub struct GlobalState {
    // ... existing fields
    pub gc_state: GcState,              // NEW — see lgc.rs
    pub gc_kind: GcKind,                // NEW — Inc / GenMinor / GenMajor (only Inc used in v1)
    pub gc_stopped: u8,                 // NEW — GCSTPUSR|GCSTPGC|GCSTPCLS bitmask
    pub gc_debt: i64,                   // NEW — bytes allocated since last step
    pub gc_total_bytes: u64,            // NEW — approx live bytes
    pub gc_marked_bytes: u64,           // NEW — marked during current cycle
    pub gc_params: [u8; 6],             // NEW — code_param-encoded pause/stepmul/etc.
    pub gc_current_white: u8,           // NEW — WHITE0 or WHITE1
    pub gc_gray: Vec<AnyHandle>,        // NEW — mark frontier
    pub gc_grayagain: Vec<AnyHandle>,   // NEW — defer-to-atomic list
    // Weak-table lists deferred to Stage 3 v2:
    // pub gc_weak: Vec<TableHandle>,
    // pub gc_ephemeron: Vec<TableHandle>,
    // pub gc_allweak: Vec<TableHandle>,
    pub gc_sweep_cursor: (HeapKind, u32),  // NEW — (arena, slot)
    pub gc_finobj: HashSet<AnyHandle>,  // Deferred to v2
    pub gc_fixed: HashSet<AnyHandle>,   // NEW — metamethod names, memerr message
    pub twups: Vec<ThreadHandle>,       // NEW — threads with open upvalues
}
```

### 3.4 `HeapKind` enum

New in `lgc.rs`:

```rust
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapKind {
    String = 0,
    Table,
    Proto,
    LClosure,
    CClosure,
    UpVal,
    Thread,
    UserData,
}
```

Lets sweep step dispatch over arenas generically.

---

## 4. Algorithm — Stage 3 v1 (incremental mark-sweep only)

### 4.1 GC state machine (5 states, down from 9)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcState {
    Pause,          // not collecting
    Propagate,      // walking gray list, marking reachables
    Atomic,         // one-shot: finalize root set, flip currentwhite
    Sweep,          // walking arenas, freeing whites
    End,            // post-sweep cleanup
}
```

Compared to C's 9 states:

| C state | Rust state | reason collapsed |
|---|---|---|
| `GCSpause` | `Pause` | 1:1 |
| `GCSpropagate` | `Propagate` | 1:1 |
| `GCSenteratomic` + `GCSatomic` | `Atomic` | no need for a "prepare" step when we don't have generational |
| `GCSswpallgc` + `GCSswpfinobj` + `GCSswptobefnz` | `Sweep` | we don't have separate finobj/tobefnz lists in v1 |
| `GCSswpend` + `GCScallfin` | `End` | no finalizers in v1 |

Stage 3 v2 adds back `GcState::Finalize` when finalizers land.

### 4.2 `Heap::gc_step` dispatcher

```rust
pub enum GcStepResult {
    Progressed(i64),  // work units consumed
    FinishedCycle,    // returned to Pause
}

impl GlobalState {
    pub fn gc_step(&mut self) -> GcStepResult {
        match self.gc_state {
            GcState::Pause => { self.start_collection(); GcStepResult::Progressed(1) }
            GcState::Propagate => self.propagate_one(),
            GcState::Atomic => { self.atomic_phase(); GcStepResult::Progressed(1) }
            GcState::Sweep => self.sweep_step(GC_SWEEP_MAX),
            GcState::End => { self.gc_state = GcState::Pause; GcStepResult::FinishedCycle }
        }
    }
}
```

### 4.3 Mark phase — `start_collection` + `propagate_one`

`start_collection` (equivalent of `restartcollection`):
1. Clear every arena's mark byte to the "new white" (the current
   white bit).
2. Clear `gc_gray` and `gc_grayagain`.
3. Reset `gc_marked_bytes` to 0.
4. Enqueue roots:
   - Main thread (from `main_thread`)
   - Registry (from `registry`)
   - Per-base-type metatables (when Stage 4 adds them)
   - Every handle in `gc_fixed` (metamethod names + tm_names,
     memerrmsg once it exists)
5. Transition to `GcState::Propagate`.

`propagate_one`:
1. Pop one handle from `gc_gray`.
2. Mark it black (set BLACKBIT in its mark byte).
3. For each reachable child:
   - If white, mark gray and push onto `gc_gray`.
4. Return work units consumed (≈ field count).

When `gc_gray` is empty, transition to `GcState::Atomic`.

### 4.4 Atomic phase

1. Mark the currently running thread (for reentrancy safety).
2. Walk any `grayagain` list and drain it.
3. [v2: weak-table cleanup]
4. [v2: separate finobj / mark to-be-finalized]
5. Flip `gc_current_white` to the other white bit.
6. Transition to `GcState::Sweep`. Record `(HeapKind::String, 0)` as
   the sweep cursor.

### 4.5 Sweep phase

Per step: walk at most `GC_SWEEP_MAX = 20` slots starting from
`gc_sweep_cursor`. For each slot:
- If the slot is `None`, skip.
- If the mark byte has the OTHER white (not the current), the
  object is dead: `free_*(handle)`.
- Otherwise, repaint to the current white (clear BLACK + set current
  white) and leave in place.

When the cursor reaches the end of an arena, advance `HeapKind` and
reset slot to 0. When all arenas are done, transition to `End`.

### 4.6 Write barriers

Forward barrier (used when a black parent gains a new white child):

```rust
impl GlobalState {
    pub fn barrier_forward(&mut self, parent: AnyHandle, child: AnyHandle) {
        if !self.is_black(parent) || !self.is_white(child) {
            return;
        }
        if self.keep_invariant() {
            // Restore the invariant: mark the child.
            self.mark_object(child);
        } else {
            // Sweep phase: repaint the parent to the current white so
            // the sweep will reconsider it next cycle.
            self.paint_white(parent);
        }
    }
}
```

Backward barrier (used when a black table mutates):

```rust
pub fn barrier_backward(&mut self, parent: AnyHandle) {
    if !self.is_black(parent) { return; }
    // Paint parent gray and push to grayagain.
    self.paint_gray(parent);
    self.gc_grayagain.push(parent);
}
```

**Every mutation point must call one of these.** Candidates:
* `Table::set(key, value)` → forward barrier if value is collectable,
  backward barrier on the table.
* `LClosure::upvalues[i] = uv` → forward barrier.
* `UpVal::state = Closed(v)` → forward barrier on the upvalue when v
  is collectable.
* `GlobalState::registry` assignment → forward barrier.
* `Thread::stack[i] = v` — actually NO, stack slots aren't tracked
  individually (they're a root, rescanned each cycle). Noop.

Stage 4 wires these into every `ltable::set` and the Stage 5 VM
instruction handlers.

### 4.7 `full_gc()` — run to completion

```rust
pub fn full_gc(&mut self) {
    loop {
        let result = self.gc_step();
        if matches!(result, GcStepResult::FinishedCycle) {
            return;
        }
    }
}
```

For Stage 3 v1 tests this is the primary driver. `luaC_step`'s
byte-debt accounting comes in Stage 3 v1.5 when we wire it into
allocation paths.

---

## 5. Commit breakdown for Stage 3

**Stage 3 v1 (this design targets):**

| # | Commit | What lands | Est LOC | Tests |
|---|---|---|---:|---|
| 1 | `contract: widen handles with generation counters` | Handle struct change, Heap gen vecs, accessor retrofits, test updates across every existing module | +800 -400 | all existing tests still pass |
| 2 | `heap: bump generation on free and validate on access` | `free_*` bumps gen, accessors compare, new unit tests for stale-handle detection | +200 | 8 new unit tests |
| 3 | `lgc: GcState + HeapKind + AnyHandle + gc_step skeleton` | New `lgc.rs` with the state machine dispatcher, all stubs | +400 | 3 unit tests |
| 4 | `lgc: mark phase with root enumeration` | `start_collection`, `propagate_one`, `mark_object` dispatching per type, per-kind child walkers | +600 | 8 unit tests (mark only) |
| 5 | `lgc: sweep phase across all arenas` | Incremental sweep cursor, paint-to-current-white, white-object freeing | +300 | 6 unit tests |
| 6 | `lgc: forward + backward write barriers` | `barrier_forward`, `barrier_backward`, unit tests hitting both paths | +200 | 5 unit tests |
| 7 | `lgc: full_gc driver + allocation debt` | `full_gc`, `gc_debt` accounting in alloc_*, luaC_checkGC equivalent | +200 | 4 integration tests |

**Stage 3 v1 budget: ~2700 net LOC, ~34 new tests. One session.**

**Stage 3 v2 (separate session, maybe skipped until Stage 4/5 force it):**

* Weak tables (weak, ephemeron, allweak)
* Finalizers (__gc metamethod dispatch)
* Generational mode (KGC_GENMINOR / KGC_GENMAJOR)

---

## 6. Test strategy

Differential testing a GC is hard because behavior isn't observable
through the public C API except via `collectgarbage("count")` (total
live bytes) and side effects of running `__gc` metamethods. Stage 3
v1 uses **unit tests only**:

* **Mark invariant tests.** Allocate 5 tables, keep references to 3,
  run `full_gc`, verify the 3 are still reachable (non-null accessors
  don't panic) and the 2 are freed (generation bumped, accessor
  panics). Implicitly verifies the root set is complete.

* **Sweep incremental correctness.** Allocate 100 strings, run a full
  cycle, verify `free_strings.len() == 100 - live_count`.

* **Write barrier tests.** Manually walk the state through a
  `Propagate` state, mutate a table, verify `gc_grayagain` grows
  (backward barrier) or the value gets marked (forward barrier).

* **Stale handle detection.** Allocate + free + alloc → verify the
  old handle panics on access (generation mismatch). This is the R1
  resolution validation.

The real oracle is **the full Lua test suite running against our
port at Stage 10** — which will hit millions of allocations through
scripts and will uncover any GC bug that unit tests missed. Stage 3
v1 only needs to be correct enough that Stage 4/5/6 can build on top
of it without getting use-after-free bugs during normal VM
operation. Stage 10 will tighten the screws.

Additionally: every Stage 2 test must still pass after the handle
widening in commit 1. Any failure there means the generation-counter
retrofit broke a stage 2 invariant.

---

## 7. Risks specific to Stage 3

| # | Risk | P | I | Mitigation |
|---|---|---|---|---|
| **R11** | u32 generation counter wraps (4 billion frees of the same slot) → ABA bug | Low | High | Panic on wrap in Stage 3 v1 (noisy), switch to u64 in v2 if real-world profiling shows it's close. |
| **R12** | Missing root set entry (e.g., we forget metamethod names) → premature free of a pinned object | Medium | High | Every fixed/pinned handle goes into `gc_fixed`; unit test walks the set post-gc to verify every entry is still alive. |
| **R13** | Missing barrier at a mutation site → invariant violation → dangling black-to-white pointer | High | High | Centralize every mutation through `Heap::{table_set, lclosure_set_upval, ...}` helpers that call the barriers; any direct arena access is a lint warning. |
| **R14** | Sweep cursor races with alloc of the same kind → new object gets freed before first mark | Medium | Medium | New objects always stamp the current white (`gc_current_white`), and the sweep frees only the OTHER white. Mirrors the C dual-white invariant — just need to be careful `alloc_*` writes the mark byte. |
| **R15** | Generation counter arena grows without bound even after GCs (we never shrink `Vec<u32>`) | Low | Low | Memory overhead is 4 bytes per ever-allocated slot. Ignore for v1; Stage 3 v2 adds a slot-compaction pass. |
| **R16** | Incremental step latency balloons on huge arenas (10M-slot walk per sweep step) | Low | Medium | `GC_SWEEP_MAX = 20` bounds it per step; full sweep takes multiple steps. Matches C behavior. |
| **R17** | `AnyHandle` enum dispatch adds per-mark overhead vs C's tagged pointer | Medium | Low | Cost is ~1 extra branch per mark. Negligible vs the memory-bound work. Stage 3 v2 can flatten to per-kind arrays if profiling shows it matters. |

The full plan.md risk register gets R1 retired (generation counters
resolve it) and picks up R11-R17 when Stage 3 work begins.

---

## 8. Migration plan doc impact

On Stage 3 v1 landing, `plan.md` needs:

1. Stage 3 status tracker row: **lgc 1804 LOC → ~2700 LOC Rust, ~34 tests**.
2. Risk register: retire R1, add R11-R17.
3. Decision log entries for:
   * Incremental-only (defer generational)
   * Deferred weak tables and finalizers
   * Per-kind mark Vec vs embedded mark byte
   * u32 generation counters with wrap panic
4. Session log entry for whichever session lands Stage 3 v1.

---

## 9. Open questions I'm punting on until implementation time

* **Should `GcState::End` actually exist,** or can we fold it into
  `Sweep` → `Pause` directly? Depends on whether the post-sweep
  shrink pass in `checkSizes` has an analog in our port. Probably
  collapse, but I'll decide when writing the commit.

* **Should `AnyHandle` live in `lgc.rs` or `contract.rs`?** Leaning
  contract.rs since it references every handle type that already
  lives there, but that pollutes the type contract with a GC concept.
  Leaving for implementation time.

* **Do we need a `GcColor` enum or is the raw `marked: u8` enough?**
  The C version uses raw bits with macros. Rust could wrap them as
  a typed enum. If it makes the mark walk fatter, keep it raw. If
  the compiler can eliminate the tag, use the enum for readability.

* **How should `gc_total_bytes` be computed?** C tracks it via every
  `luaM_realloc_` call, which we don't have in the same form. Our
  options are (a) approximate via slot counts × average size, or
  (b) add an `object_size()` method per type that sums field sizes.
  (b) is more accurate; (a) is cheaper. Start with (b) and see.

These are all implementation-time decisions that won't affect the
shape of the above design. Noted here so the next-session reader
doesn't think I missed them.

---

## 10. Checkpoint-related changes already in

The Stage 3 checkpoint session landed one fix that matters for
Stage 3 planning: `heap.rs` promoted `debug_assert!` to `assert!` for
double-free and access-after-free. This rail now runs in release
builds, and the test names lost the `_in_debug` suffix. Stage 3's
generation-counter work will EXTEND (not replace) these checks.

## 11. What we DON'T know until we start writing code

* How much the handle widening (u32 → `{slot, gen}`) actually
  complicates the `TableKey` enum. It's a `#[derive(Hash, Eq)]` type,
  so 8-byte handles should just work, but the hash quality of the
  generation counter might need review.
* Whether the existing `Heap::alloc_*` code paths need splitting to
  accept the "stamp this mark byte" step. Likely yes — we need to
  record the current white at allocation time.
* Whether `AnyHandle` as an enum is fast enough. Could matter for the
  mark loop. Alternative: one `Vec<Handle>` per object kind in the
  gray list, iterated in a switch. Stage 3 v1 starts with the enum
  and benchmarks at the end.

These are the things we'll learn in session 1 of Stage 3. The design
above is the best guess going in; every commit message in Stage 3
should call out where it diverged from this document.
