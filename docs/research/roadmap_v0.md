# Project v0 -- First 10 Days Pipeline

Project goal: create a **visible v0** of an automated migration tool
capable of rewriting C/C++ code into Rust with behavioral assurance.\
First targets:

-   **miniz** -- scoped bootstrap victim\
-   **libsodium** -- second easy slice for pointer lifting

------------------------------------------------------------------------

## Day 1 -- Foundation

-   Project structure with Clap CLI\
-   CI pipeline (GitHub Actions)\
-   integrate:
    -   clang AST via `clang-sys`/`bindgen`\
    -   proptest harness skeleton\
-   minimal command:
    -   parse single C file\
    -   emit same file unmodified

**Outcome:** repo + CLI working.

------------------------------------------------------------------------

## Day 2 -- AST Extraction

-   Extract functions from miniz:
    -   `tdefl_compress` (header mode)\
    -   `tinfl_decompress`
-   Produce JSON AST dump\
-   design IR for:
    -   ownership hints\
    -   error model

**Outcome:** deterministic AST for two functions.

------------------------------------------------------------------------

## Day 3 -- Test Characterization

-   Generate golden tests:
    -   compress in C original\
    -   decompress in C original\
    -   store vectors
-   integrate fuzz seed corpus.

**Outcome:** differential suite.

------------------------------------------------------------------------

## Day 4 -- First Rust Emitter

-   Emit Rust skeleton from IR\
-   call original via FFI fallback\
-   tests must pass using fallback.

**Outcome:** safe bridge.

------------------------------------------------------------------------

## Day 5 -- Semantic Migration (miniz slice)

-   Translate:
    -   buffer management\
    -   error codes → `Result`
-   roundtrip byte equivalence.

**Outcome:** 2 functions in Rust with assurance.

------------------------------------------------------------------------

## Day 6 -- Idiomatic Checker

-   Rules:
    -   avoid raw pointers\
    -   prefer slices/iterators\
    -   RAII structs
-   static score 0‑100.

**Outcome:** quality metric.

------------------------------------------------------------------------

## Day 7 -- Second Victim Start (libsodium)

-   Parse `crypto_box_easy`\
-   lift pointers to slices\
-   create property tests with known vectors.

**Outcome:** 1 sodium function migrated.

------------------------------------------------------------------------

## Day 8 -- Unsafe Elimination

-   Replace manual memory with:
    -   `Vec<u8>`\
    -   stack arrays
-   perf benchmark harness.

**Outcome:** unsafe ↓

------------------------------------------------------------------------

## Day 9 -- Packaging

-   WASI build\
-   release binary\
-   README + demo.

**Outcome:** public visibility.

------------------------------------------------------------------------

## Day 10 -- Showcase

-   Blog post:
    -   why migration tools ≠ Rust replacements\
    -   miniz automated port\
    -   sodium pointer lifting\
    -   idiomatic score.

**Outcome:** v0 visible.

------------------------------------------------------------------------

# Estimated Effort Solo Dev

-   6--8 hours/day\
-   10 días = **60--80 horas**\
-   Riesgo bajo gracias a dominio miniz.

------------------------------------------------------------------------

## Features Delivered in v0

-   Function extraction via clang\
-   IR → Rust emitter\
-   FFI fallback\
-   Golden + fuzz tests\
-   Idiomatic score.

------------------------------------------------------------------------

## Next 20 Days

-   expand miniz coverage\
-   add sodium modules\
-   tolerance for floats (for future GSL).
