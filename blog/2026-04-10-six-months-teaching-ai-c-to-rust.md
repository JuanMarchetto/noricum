# I Spent Six Months Teaching an AI to Migrate Real C Code to Safe Rust. Here Is Every Way It Failed.

**TL;DR:** Noricum is an autonomous C→Rust migration pipeline I have been building solo. It has successfully migrated five real open-source C libraries to idiomatic, memory-safe Rust with 0 unsafe blocks and byte-exact differential testing. It has also failed, fourteen times in a row, on miniz_zip.c — a 4,895-LOC file that sits exactly at the capability frontier for current LLMs. This post is the complete catalog: five successes with numbers, one failure with fourteen-run taxonomy, thirty-eight documented pipeline improvements, twelve repair rules, and an honest list of what still does not work. If you are working on this problem, please run the pipeline yourself or email me. I want to compare notes.

Noricum repo: [github.com/JuanMarchetto/noricum](https://github.com/JuanMarchetto/noricum)

---

## Why I am publishing this now

Every "we migrated C to Rust" blog post I have read is a success story. The author picks a small, self-contained file. The pipeline works on the first or second try. They publish the numbers and call it a day.

That is not what the frontier looks like.

The frontier looks like fourteen consecutive runs on the same 4,895-LOC C file, each producing a different bouquet of failures. Truncated module boundaries. Warm-start poisoning. Assembly retranslation destroying working code. Cross-module type contract mismatches. Duplicate struct definitions because the LLM reinvents the same type with subtly different field names in seven different modules.

I have been living inside that frontier for six months, and I have built up the most detailed failure taxonomy I know of. None of the DARPA TRACTOR teams have published anything like it. None of the consulting firms have. The LLM benchmarks in the literature stop at 1,000 LOC.

So I am publishing the full record. The wins and the losses. The numbers and the scars. If you are one of the people actually working on this problem, this post is for you.

---

## What Noricum is

Noricum is a deterministic pipeline wrapped around an LLM, not an LLM wrapped around some scripts. The distinction matters because almost every other tool in this space is the opposite.

The pipeline has nine stages:

```
C source
  │
  ├── Extract: parse into semantic code map (noricum-ir)
  ├── Classify: difficulty tier (Easy / Medium / Hard / Massive)
  ├── C2Rust baseline (optional, used only as context or last-resort fallback)
  ├── LLM analysis: generate plan + type contract
  ├── LLM translation: produce idiomatic safe Rust
  ├── Validation: compile + clippy + differential test + idiomatic score
  ├── Repair loop: on failure, feed errors back to LLM (up to 5 iterations)
  ├── Quality floor: reject repairs that increase unsafe count or break substance checks
  └── Final output: Rust crate with diff-test harness
```

Two design choices carry most of the weight:

1. **Differential testing is non-negotiable.** Every migration compiles both the C and Rust versions, runs them with the same inputs, and byte-compares the outputs. A migration that "looks right" but produces different output is a failed migration, period.

2. **The repair loop has a quality floor.** If iteration N introduces more unsafe blocks than iteration N-1 had, the repair is rejected and we back off. This sounds obvious. It is not. I watched repair loops trade compilation correctness for unsafe blocks on a daily basis until I added the floor.

Primary provider is Anthropic Claude. Fallback providers are DeepSeek (used as the primary for most of the miniz experiments because of cost) and Ollama for local development. The pipeline is provider-agnostic via an enum wrapper around `rig-rs`. The whole thing is ~14,100 lines of Rust across seven crates, with 293+ tests.

---

## Five things that worked

These are ordered by size, smallest to largest. All five have 0 unsafe blocks, 0 `unwrap()` calls, and pass differential testing byte-for-byte.

### 1. genann (642 LOC → 721 LOC Rust)

[codeplea/genann](https://github.com/codeplea/genann) is a minimal neural network library in portable C, 2,246 stars on GitHub. The single-file C implementation does backpropagation, stochastic gradient descent, and a handful of activation functions. A textbook case for a clean C library.

Noricum migrated it in one pass with one repair iteration. The differential test runs the C and Rust versions on the same training data and compares the resulting weight matrices after every epoch. 521,556 floating-point assertions, all byte-exact.

The interesting translations:

- **Function pointers → enum dispatch.** genann uses function pointers for activation functions (`genann_act_sigmoid`, `genann_act_tanh`). In Rust, I wanted this idiomatic, not `unsafe { fn() }`. Noricum produced an `ActivationFn` enum with `match` dispatch inside the forward-pass loop. Same performance profile, fully safe.
- **One giant `malloc` → separate `Vec<f64>`.** The C code packs all network state (weights, biases, outputs, deltas) into one contiguous `malloc`. Rust split this into four owned `Vec<f64>` fields on a `struct Genann`. Cleaner, no pointer arithmetic.
- **Global lookup table → struct field.** C used a file-scoped array for activation function names. Rust moved it onto the struct where it belongs.
- **glibc RNG reimplemented.** genann calls `random()` from glibc, which has a specific state-table algorithm (TYPE_3, degree 31) that is NOT portable. To make the differential test pass byte-exact, I reimplemented TYPE_3 in safe Rust with the same state table. This is a real thing you have to do when you care about byte-exact determinism across languages.
- **`FILE *` → `std::io::{Read, Write}`.** genann has serialization functions that take `FILE *`. These became trait-based on `Read` / `Write`, which is more flexible and lets you serialize to a `Vec<u8>` or a file equally well.

Idiomatic score: 47/100 (low because the forward-pass hot loop uses `as f64` casts, which the scorer penalizes as a negative signal — accepted tradeoff for numerical equivalence).

### 2. olive.c (1,022 LOC → 1,176 LOC Rust)

[tsoding/olive.c](https://github.com/tsoding/olive.c) is Alexey Kutepov's single-file 2D graphics library. Triangles, circles, text rendering, subcanvas operations, blending. It is the kind of library that exercises everything nasty about C: raw pointers, type punning, inline arithmetic, manual memory layout.

Noricum shipped a clean migration with byte-exact differential testing via checksum comparison (pixel buffers are binary data, so the driver compares a deterministic checksum of the canvas rather than the raw bytes). 23 test functions covering 24 library functions.

The harder translations:

- **Raw pointer canvas → owned `Canvas` struct.** The C API threads a `uint32_t *pixels`, `size_t width`, `size_t height`, `size_t stride` quadruple through every function. This is the moral equivalent of a fat pointer manually unpacked into four arguments. Rust collapsed it into a `Canvas { pixels: Vec<u32>, width: usize, height: usize, stride: usize }` with methods.
- **`OLIVEC_PIXEL` macro → `pixel()` / `set_pixel()` methods.** The C macro does bounds-checked pointer arithmetic inline. Rust uses typed methods on `Canvas`.
- **Subcanvas aliasing → region-based operations.** olive.c has a `subcanvas` feature where a sub-rectangle of a canvas is treated as its own canvas. In C this is just pointer arithmetic into the parent buffer. In Rust that is shared mutable aliasing, which the borrow checker rejects. I reworked this as region-based direct operations (`fill_rect`, `draw_line_clipped`) that take the rectangle as an argument instead of creating an aliased sub-buffer. No aliasing, no `unsafe`.
- **Type punning `*(uint32_t*)&z` → `f32::to_bits()`.** olive.c uses the standard C trick of bit-casting a float to a `uint32_t` to do interpolation math. Rust has `f32::to_bits()` and `f32::from_bits()` which do exactly this, safely.
- **C macros (`SWAP`, `SIGN`, `ABS`) → `std::mem::swap`, `f32::signum`, `f32::abs`.** Standard library replacements for the standard C preprocessor idioms.

Idiomatic score: 76. 350 lines of the output are a const glyph-data array which is just transcription and does not pattern-match idiomatic Rust.

### 3. expr_eval (1,686 LOC → 1,446 LOC Rust)

A medium-sized expression evaluator written in C. Recursive-descent parser, lexer, 25+ built-in functions, a variable store, a DataSet type for statistics. 74 functions total.

Noricum migrated it in a single pass with zero repair iterations. Idiomatic score 100. Byte-exact diff test.

The interesting work here was the data structure mapping: the C implementation uses linked lists for the variable store and a custom hash table for the function registry. Noricum produced `HashMap<String, f64>` for variables and a `Vec<(String, fn(&[f64]) -> f64)>` for the function registry. The parser translated cleanly from mutually-recursive C functions into mutually-recursive Rust functions using `&mut Parser<'_>` as the state carrier. No lifetimes needed.

### 4. cjson_full (1,696 LOC → 1,439 LOC Rust)

This is the flagship. [DaveGamble/cJSON](https://github.com/DaveGamble/cJSON) is 12,500 stars on GitHub. One of the most widely-used C JSON libraries in the world. Used in hundreds of embedded systems, IoT devices, and memory-constrained applications.

Noricum migrated the full `cJSON.c` file. 97 out of 97 C tests pass. 96 out of 96 Rust tests pass (one test uses a C-specific integer overflow behavior that does not exist in safe Rust, intentionally omitted from the Rust suite). Differential testing is byte-exact across all basic operations plus 12 extended scenarios. 0 unsafe blocks. 0 raw pointers. 0 `unwrap()`. LOC ratio 0.85x — the Rust version is actually shorter than the C version.

This migration went through six phases (A through F) because it is not a single file in spirit, it is a library with multiple logical feature groups. Each phase migrated one group and validated against the full C test suite. The phases were: convenience add functions, references (required a `Clone` implementation because cJSON supports reference counting), case-sensitive operations, float/string array helpers, parse/print variant functions (including error reporting via `ParseError`), and setters/version.

Key translations:

- **Linked list of children → `Vec<JsonValue>`.** cJSON uses a doubly-linked list where each node has `prev` and `next` pointers. This is a classic case where C's "flexible" data structure maps much better to a Rust `Vec` both in ergonomics and performance.
- **`malloc` / `free` → RAII via `Drop`.** The cJSON C code has a dedicated `cJSON_Delete()` that walks the tree and frees every node. Rust gets this for free via `Drop` on the enum variants.
- **Type tags (`cJSON_Number`, `cJSON_String`, etc.) → `enum JsonValue`.** The idiomatic Rust pattern. C used a `union` plus a discriminator `int type`. Rust uses a tagged enum and gets exhaustiveness checking from the compiler.
- **UTF-16 surrogate pair handling.** cJSON's string parser deals with `\uXXXX` escapes including surrogate pairs. Porting this correctly required understanding the encoding rules precisely and writing a clean Rust implementation that produces the exact same bytes.
- **C-compatible `format_g()`.** cJSON uses `sprintf("%g", ...)` for floats, which has specific C89 formatting rules (trailing zero trimming, scientific notation thresholds). Rust's `{}` Display uses different rules. I implemented a `format_g()` helper that matches the C output exactly. This is the kind of thing you have to do when your differential test is byte-exact on JSON serialization.

Idiomatic score: 100.

Phase G (cJSON_Utils, which adds JSON Pointer and JSON Patch support) is deferred — it is another 1,500 LOC and was not in scope for the first ship.

### 5. http-parser (3,680 LOC → 1,492 LOC Rust)

[nodejs/http-parser](https://github.com/nodejs/http-parser) is Joyent's original HTTP parser, 6,400 stars. This is the parser that Node.js used for years before moving to llhttp. It is a famous chunk of C and a famous benchmark for anyone building HTTP infrastructure.

The translation ratio is 0.41x. 3,680 LOC of C became 1,492 lines of Rust. That is not compression magic — it is what happens when you replace a 58-state goto-based switch statement with clean line-based parsing using iterator methods.

0 unsafe blocks. 0 `unwrap()`. 37 test cases, all byte-exact.

One honest note: the pipeline reached "compiles and tests pass" at repair iteration 7 on its first run, but lost that state to a transient 502 error from the Anthropic API during the next repair iteration. The pipeline did not have resilient error handling for transient API failures at that point, so it crashed out and fell back to an earlier state. I completed the last ~200 lines manually, using the pipeline's partial output as the starting point. The translation was the pipeline's work; the finishing touches were mine. I fixed the transient-error handling in the repair loop immediately after (the change is in commit history — `.await?` became a match-based retry for 502s).

The interesting translations:

- **58-state goto-based switch → line-based parsing.** The C parser uses a giant `switch` with `goto` labels for state transitions. This is fine in C but maps horribly to Rust. I let the LLM try a direct translation (it produced ~2,400 lines of nested match statements — technically correct, unreadable). Then I prompted it to reconsider: "What if you parsed this line by line instead of byte by byte?" The result is 1,492 lines of clean state-machine-less parsing using iterator methods. Shorter and more maintainable.
- **C global mutable state → `thread_local!` `RefCell`.** The test harness used global state for callback verification. Rust moved this into `thread_local! { static STATE: RefCell<TestState> }`. Safe, no `Mutex` overhead for the tests.
- **C function pointers for callbacks → direct function calls.** The C API uses a `http_parser_settings` struct full of function pointers. In the Rust version the callback API is preserved via a trait, but the internal calls are direct.
- **Bit-packed struct → clean Rust struct.** The C version uses bit fields to save memory. Rust just uses a struct with named fields. The binary size difference is irrelevant on any machine built after 2005.

### CRUST-Bench 3-project pilot (bonus)

I also ran Noricum against the [CRUST-Bench](https://github.com/anirudhkhatry/CRUST-bench) academic benchmark dataset. On the 3-project pilot: 100% compile rate, 66.7% test pass rate (2 of 3), 0 unsafe blocks. One of the three test failures turned out to be a bug in the CRUST-Bench dataset itself (a `tanh` test with incorrect expected output — I reported it upstream).

Full CRUST-Bench results are in `docs/crust-bench-results.md` in the repo.

---

## The file I could not migrate

This is the centerpiece. The part of the post that, as far as I know, nobody else has published.

**miniz_zip.c**. 4,895 lines of C. Part of [richgel999/miniz](https://github.com/richgel999/miniz), which is a widely-used standalone zlib replacement. miniz_zip is the zip archive handling portion, which I picked deliberately because it combines several of the hardest patterns in C: dense pointer arithmetic, global mutable state, file I/O, a callback-based API for custom IO, manual memory pools, and a state machine for archive parsing that spans several functions.

I have run Noricum against this file **fourteen times**. As of Run 14, the latest assembly has **598 compilation errors**. The best result I have ever achieved was Run 6, which assembled at **1 error** — a truncated function body at a module boundary that cascaded a brace imbalance through the rest of the file.

Here is the timeline. The specific pipeline improvements are visible in the git log as commits P0 through P38.

### Run 1 — DeepSeek R1, no modular split (pre-P12)
The file was treated as two modules based on a naive split. One module compiled (363 LOC, score 78). The other was 4,766 lines of stub functions (score 15) because the LLM could not fit enough context into one prompt to produce real implementations. 85 minutes wall time, 19 LLM calls, cost ~$4. Status: **FallbackUnsafe**.

**Lesson:** Large files need real sub-division, not naive splitting.

### Run 2 — DeepSeek R1, P12 sub-chunking
P12 added `split_into_modules()` that sub-divides prefix groups larger than 1,000 LOC into ~600 LOC sub-modules. miniz_zip became 10 modules (363 to 938 LOC each). Three validated (`if: 100`, `mz_p1: 97`, `mz_p9: 100`). Six FallbackUnsafe. One API error skip. 5.5 hours wall time, 51 LLM calls, hit budget limit. Module `mz_p8` was irrecoverable with 251 errors.

**Lesson:** DeepSeek R1 is too slow for repair loops. Each R1 call averages ~8 minutes vs ~2 minutes for Claude. Modules under 700 LOC succeed, modules over 800 LOC fail with R1.

### Run 3 — Anthropic Claude, full pipeline
17 modules. Average score 85. Three validated. Assembly repair stalled between iterations 7 and 8 at 5-8 errors. I went in manually and discovered the root cause: **`ZipArchive` was defined in 11 out of 17 modules, with different field definitions in each**. Not simple duplicates — actual structural disagreements. I hand-fixed iter-05 to produce a compiling assembly (2,829 LOC, 96 functions, 33 structs, 9 enums) with exactly five mechanical patches. All five were codifiable as repair rules, which became rules R1 through R5 in the next pipeline release.

**Lesson:** Cross-module type disagreement is the dominant failure mode for modular migration of >2,500 LOC files. Assembly dedup alone is insufficient. Prevention via type propagation from a canonical contract is required.

### Runs 4–5 — Warm-start + P30 Hybrid Repair
P30 added a three-phase repair engine: a deterministic rule engine (rules R0 through R4), a surgical repair phase that rewrites individual functions via small LLM calls, and a legacy whole-file repair as a final fallback. Warm-start seeded new runs from prior best-version artifacts.

Run 4 hit 541 errors in assembly due to warm-start manifest mismatch (17-module warm-start used on a 10-module split). Run 5 had 10 modules with 4 validated and a much better assembly starting state (3,543 LOC, 103 functions — best ever for assembled output). Phase 1 rule engine parsed 0 errors (syntax errors not parseable by the regex). Phase 2 surgical repair was skipped. Phase 3 legacy whole-file repair destroyed the code (3,543 LOC → 491 LOC). P1 best-version tracking saved the 3,543 LOC version as the final output.

**Lesson:** Assembly retranslation is destructive for outputs >1,000 LOC. Must be disabled for assembly repair.

### Run 6 — P31 Assembly Cleanup
P31 added three fixes: markdown fence stripping for LLM outputs, syntax-error regex parsing (so rule engine could see syntax errors, not just type errors), and use-import merging. Run 6 reached **1 error** — the best ever. The 1 error was an unclosed delimiter at line 982: a truncated `mz_zip_reader_read_central_dir` function body. Module `mz_p2` output truncated. Function opens brace at depth 0→1 but never closes. Brace imbalance compounds through the rest of the modules.

**Lesson:** P32 — brace-balance validation — became the next priority. You have to detect truncated functions before assembly.

### Runs 7–9 — P32/P32b brace-balance validation
P32 adds a string-and-comment-aware brace-balance checker. If a module has unclosed braces, the pipeline truncates at the last balanced brace point and optionally re-translates. P32b added smart re-translation: if a module has unclosed braces, re-translate once with a truncation hint before smart-truncating.

Run 7 (cold, no warm-start): 8 of 10 modules truncated. P32 correctly detected all of them. Auto-close alone was insufficient — truncated function bodies generate many type/logic errors post-assembly. Assembly ended at 114 errors.

Runs 8–9 (warm-start): re-translate succeeded 3/3 and 3/3. But assembly had 541 errors (Run 8) and 526 errors (Run 9) due to cross-module type incompatibility from seeds that were individually high-scoring but structurally inconsistent across modules.

**Lesson:** Warm-start must match module split exactly. Even with matching split, NearlyCompiles seeds propagate cross-module inconsistencies. Run 6 was anomalously good because its warm-start seeds happened to produce compatible types.

### Runs 10–11 — P33/P33b Type Contract
P33 introduced types-first modular migration: generate a shared type contract before translating any module, compile-validate it, and inject it as a canonical contract into every module's prompt. Assembly then strips duplicates of those canonical types.

Run 10 produced a contract with 4 structs, most of them empty because the source C file had only forward declarations — the actual type definitions lived in .h headers. Assembly had 697 errors. Worse than Run 9.

**Lesson:** For multi-file C projects, type definitions live in .h headers. The contract generator must resolve `#include` directives and read header files. Without this, the contract has empty forward declarations that are useless.

Run 11 (P33b, header resolution + idiomatic prompt): contract had 250 LOC, 10 types, a 15-field `MzZipArchive`. Assembly had only 2 parser-blocking errors! But fixing those syntax errors revealed **878 underlying errors** — 164 "no field," 57 duplicate definitions, 277 type mismatches.

**Lesson:** "2 errors" was misleading. Syntax errors prevent rustc from parsing past the error point. Always fix syntax errors first and re-count.

### Runs 12–14 — Various combinations
Runs 12, 13, 14 explored contract naming consistency, ensemble translation (P38), per-task model routing (P35), and various prompt engineering fixes. Run 14 landed at 598 errors with 1 syntax error masking the rest.

The truncated function at module boundary `mz_p7` → `mz_p8` (`mz_zip_writer_add_cfile`) has truncated in **every single run**. R12 now stubs these automatically, which helps but does not solve the root problem: the LLM runs out of context right where the hardest function lives.

---

## The pipeline improvements timeline

Here is the compressed list of every documented pipeline improvement from P0 to P38. This is useful if you are trying to build something similar — each one came from a specific failure mode.

- **P0 — Repair quality floor.** Reject repairs that increase unsafe count beyond the translation baseline.
- **P1 — Best-version tracking.** Keep the highest-score version across repair iterations. Use it for fallback instead of raw C2Rust output.
- **P2 — Per-function C2Rust context.** Extract only the matching c2rust functions per chunk to save tokens.
- **P3 — Modular migration.** Files >2000 LOC auto-split into semantic modules. Dependency graph with topological ordering. Each module gets independent translate → validate → repair.
- **P4 — Skip-C2Rust flag.** For files where C2Rust adds no value and costs tokens.
- **P6 — Substance gate.** Reject empty stubs, re-translate with higher temperature.
- **P7 — Score penalty for stubs.** Cap score at 15/20 for code that is mostly empty.
- **P8 — Data/code separation.** Detect static data blocks (const arrays, lookup tables). Transcription-only instructions for data chunks.
- **P9 — Foundation context.** Chunk 0's translated Rust types are passed to all subsequent chunks.
- **P10 — Incremental compilation.** Check that accumulated chunks compile together during translation.
- **P11 — Signature agreement pass.** Generate agreed Rust signatures before translating bodies, for files with >2 chunks.
- **P12 — Sub-chunk large modules.** Prefix groups >1000 LOC split into ~600 LOC sub-modules.
- **P13 — Re-translate on >100 errors.** If repair can't make progress, re-translate the module.
- **P14 — API retry with backoff.** Handle transient 502s gracefully instead of crashing.
- **P15 — Adaptive LLM budget.** `max_calls = modules * 7 + 10`.
- **P16 — Faster repair model.** Use DeepSeek-chat (or Claude Sonnet) for repair, even if primary is DeepSeek-R1 or Claude Opus. Repair needs speed more than depth.
- **P17 — Configurable sub-module target LOC.**
- **P18 — Wave-based module iteration.** Kahn's algorithm levels for parallel module translation.
- **P19 — Warm-start from previous artifacts.**
- **P20 — Artifact load/save manifest API.**
- **P26 — Assembly error propagation.** Track which modules compile individually vs only in assembly.
- **P27 — Type deduplication in assembly.** Strip duplicate struct/enum/const definitions (first module wins).
- **P28 — Disable retranslation for assembly repair.** Assembly retranslation is destructive for outputs >1,000 LOC.
- **P29 — Faster repair model for assembly.**
- **P30 — Hybrid Repair Engine.** Three phases: rule engine, surgical repair, legacy whole-file fallback.
- **P31 — Assembly cleanup.** Fence stripping, syntax-error regex, use-import merging.
- **P32 — Brace-balance validation.** Detect truncated functions before assembly.
- **P32b — Smart truncate + re-translate.** Truncate at the last balanced brace point; re-translate once with a hint.
- **P33 — Type Contract.** Types-first modular migration with a canonical contract injected into every module.
- **P33b — Header resolution + idiomatic prompt.** Read `#include`'d .h files to get full type definitions.
- **P34 — Incremental completion.** Re-run only failed modules on retry.
- **P35 — Per-task model routing.** Different models for analysis, translation, and repair.
- **P37 — Behavioral spec mining.** Extract function I/O traces from C programs.
- **P38 — Ensemble translation.** Multi-provider best-of-N candidate selection.

Every one of these came from a real failure on a real file. Nothing in this list was speculative.

---

## The repair rules (R0 – R12)

The repair rule engine is the first phase of the P30 Hybrid Repair Engine. These are deterministic transformations applied before any LLM call, so they cost nothing. Each rule came from a pattern I saw repeatedly in miniz runs and then codified.

- **R0 — Fence strip.** Remove markdown ` ``` ` fences from LLM outputs.
- **R1 — Clone bounds.** Add `where T: Clone` to generics that use `.clone()` in the body.
- **R2 — Dedup functions.** Remove duplicate function definitions (LLMs sometimes emit the same function twice with slightly different bodies).
- **R3 — `mut Option<&mut T>`.** Fix `ref mut` in pattern matches for `Option<&mut T>` parameters.
- **R4 — Brace balance safety net.** Auto-close unclosed braces at EOF.
- **R5 — Inner attributes.** Strip misplaced `#![...]` attributes.
- **R6 — Inner docs.** Strip misplaced `//!` doc comments.
- **R7 — Duplicate imports.** Collapse repeated `use` statements.
- **R8 — Orphaned derives.** Strip `#[derive(...)]` attributes with no following item.
- **R9 — Conflicting impls.** Remove duplicate `impl` blocks for the same type.
- **R10 — External crates.** Strip `extern crate` statements (edition 2024 does not need them).
- **R11 — Windows imports.** Replace `use std::os::windows::*` with cfg-gated equivalents.
- **R12 — Truncated module boundaries.** Stub truncated functions at module boundaries with `todo!()` so the module at least parses.

---

## What still does not work

This is the section I care most about. Nobody else publishes this.

As of Run 14 against miniz_zip.c, the following failure modes are **unsolved**:

1. **Cross-module type contract mismatches when contract fields don't match module usage patterns.** The contract is declarative. The modules are generated separately. If the contract says `MzZipArchive { m_palloc: Option<PAlloc> }` and one module uses `archive.alloc_fn()` because that is what the C code called it, the names disagree. Contract enforcement requires either stronger prompt adherence (which is inconsistent) or post-hoc renaming (which is a real refactor, not a pattern match).

2. **Truncated function bodies at the hardest module boundary, even with P32 re-translate.** The function `mz_zip_writer_add_cfile` truncates in every single run at the `mz_p7` → `mz_p8` boundary. This is a 400+ LOC function in the original C. The LLM's context window runs out right where the hardest function lives. R12 now stubs these automatically, but a stub is not a migration.

3. **Type punning in dense arithmetic kernels.** Specifically, miniz's entropy coding routines use `*(uint32_t*)&z` patterns for bit manipulation on floats. `f32::to_bits()` covers the simple cases. The complex cases involve manipulating multiple registers in a tight loop with specific overflow semantics, and the LLM's output for these is correct-looking but numerically different.

4. **Deep goto state machines where state transitions depend on runtime-computed labels.** miniz's decompressor has several state machines with >30 states. Translating these into Rust match arms works structurally, but the ordering of state transitions does not always preserve in the translated version — the LLM reorders arms for "readability" and breaks the state machine.

5. **Cost at frontier scale.** Each full Noricum run on miniz_zip.c costs ~$3–$5 in LLM API fees. Each DeepSeek R1 call averages ~8 minutes, making repair loops impractical with R1. 14 runs at ~$5 average is ~$70 just for this one file. The hybrid approach (R1 for analysis, faster model for translation + repair) cuts time but not cost by that much.

6. **The assembly repair phase is the structural hard wall.** Ten modules that each compile cleanly in isolation can produce 500+ errors when assembled, because cross-module type disagreements are invisible at the single-module level. I have four separate pipeline phases dedicated to mitigating this (P26, P27, P28, P33) and none of them have fully solved it.

If you have a theory about any of these, I want to hear it.

---

## What I am asking for

Three things. All low-effort on your end.

**1. If you are working on LLM-driven C→Rust at scale, email me.** I am especially interested in hearing from the six DARPA TRACTOR funded teams, Ladybird contributors working on the C++→Rust migration, and anyone at a security-critical C library (libpng, zlib, libsodium, libcurl subsystems) who has tried this on production code. Email me at juanpatriciomarchetto@gmail.com or open an issue at [github.com/JuanMarchetto/noricum](https://github.com/JuanMarchetto/noricum).

**2. Try running Noricum on your own C file.** The repo has a working CLI. `cargo run -p noricum-cli -- migrate path/to/your/file.c --provider deepseek --artifacts-dir .noricum-run`. If it works, I want to know. If it fails, I want to know what failure mode you hit — I may have already seen it and have a pipeline phase for it, or it may be something new that goes into P39.

**3. If you have a theory about any of the six unsolved failure modes above, please share it.** Even if the theory is wrong, hearing it sharpens my thinking. Six months of iteration has given me a deep internal model of these failure modes, and at this stage an external perspective is the single highest-leverage next input I can get.

---

## Honest closing

**This post is an intermediate checkpoint, not a conclusion.** Noricum is a research project in active development. It has five real wins and one stubborn unsolved case. It is not ready for production use on files of miniz_zip's complexity, and I do not want to oversell it. The benchmark numbers in this post are real, reproducible, and verifiable in the repo.

I am publishing this now instead of waiting for a clean miniz_zip success because the failure taxonomy is more valuable than another success story. Every consulting firm in this space will tell you they can migrate your code. Every academic paper reports a high success rate on files under 1,000 LOC. Nobody publishes "here is a file we failed on fourteen times and these are every way it failed."

I think that post is the one the Rust community needs right now, and I am the one who can write it without it costing anyone their contract or their academic credibility.

**What comes next.** Run 15 is scheduled. Pipeline phase P39 is already in draft and targets the specific cross-module type contract mismatch mode that has dominated Runs 11 through 14. I am also planning to run Noricum against MIT Lincoln Laboratory's public DARPA TRACTOR Battery 01 benchmark as an independent baseline. The follow-up post will either be "miniz_zip compiles" or "Run 20 and here is what changed." Whichever comes first.

If you made it this far, thank you. The repo is [here](https://github.com/JuanMarchetto/noricum). All the benchmark sources, diff-test harnesses, and the pipeline itself are MIT-licensed and reproducible.

Juan Marchetto
juanpatriciomarchetto@gmail.com
Argentina, April 2026
