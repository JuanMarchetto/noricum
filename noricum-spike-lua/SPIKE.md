# lua Interactive Spike

Scaffolded by `tools/spike/scaffold.sh` on 2026-04-11.

## Phase 0 checklist (pre-clock)

- [ ] Fill in `wrapper.h` with the actual API functions you need (~5-10 fns).
- [ ] Fill in `wrapper.c` with thin delegations to the C library.
- [ ] Fill in `src/lib.rs` extern block to match `wrapper.h`.
- [ ] Write `fixtures/gen_fixtures.py` with a deterministic corpus.
- [ ] Run `python3 fixtures/gen_fixtures.py` and commit the output.
- [ ] Run `gcc wrapper_smoke.c wrapper.c <sources>.c -o wrapper_smoke -I.` and verify exit 0.
- [ ] Run `cargo test --test differential_lua -- oracle_selftest` and verify green.

When all boxes are checked, **Hour 0 of the spike starts.** See
docs/methodology/interactive-spike-mode.md for the flow.

## Dependency budget (pick one at start)

- [ ] **Mode 1 — FREE** — delegate well-solved algorithms to crates
      (flate2, aes, etc.). Default for research spikes.
- [ ] **Mode 2 — MATCHING** — match the C original's dep count (usually 0).
      Vendor primitives as source. Default for drop-in replacements.
- [ ] **Mode 3 — ZERO** — no external runtime deps. Everything vendored.
      Default for embedded / no-std / defense targets.

See docs/methodology/dependency-budget.md.

## Architectural seed

Check docs/methodology/architectural-seeds.md for a reference Rust crate
whose type architecture you should steal. Read it for 30 minutes before
writing `src/contract.rs`. Do NOT write the type contract from the C
code alone.

## Timebox

Phase 0: ~2-3 hours (this checklist). Does NOT count against the spike clock.

Hour 0 — Hour 8: the spike itself. Hard exit at Hour 8 regardless of
state. See docs/methodology/interactive-spike-mode.md.

## Commit cadence

One commit per function that passes its differential test. No batching.
The commit log becomes the blog post narrative.
