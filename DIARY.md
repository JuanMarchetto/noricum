# Noricum Project Diary

A chronicle of building an autonomous C/C++ to Rust migration agent.

---

## Day 1 — From Zero to Pipeline (2026-03-05)

### The Vision
Started with 6 design documents and zero code. The goal: build an autonomous agent that takes C code and produces safe, idiomatic Rust — not just mechanical translation, but genuinely good Rust.

### What Got Built
In a single session: 7 crates, 60 files, ~4800 lines of Rust, 128 tests, 0 clippy warnings. The entire architecture from the plan documents came to life.

### The Sync Pipeline
Before touching any LLM, we built a deterministic fallback: rule-based translation that handles simple C patterns (if/else, while, for loops, printf, return statements). This was critical — it means Noricum works without any API keys, just slower and for simpler code.

### First LLM Call — Analysis Works
Wired up rig-rs 0.31 to Claude Sonnet. The analysis agent parsed C code and identified patterns like "simple_loop", "integer_arithmetic", "pure_function". This structured JSON output feeds directly into the translation prompt. Clean separation of concerns.

### The Bool-vs-Int Problem — A Turning Point
The translation agent converted `power.c` with `is_even(int n)` returning `int` (0 or 1 in C) into `bool` (true/false in Rust). The code was 100% correct Rust. Score 100/100. Zero unsafe blocks. But the output was wrong: `"true"` instead of `"1"`.

This is the fundamental tension in C-to-Rust migration: **idiomatically correct Rust can be semantically incorrect**. The LLM made the "right" choice (booleans are more idiomatic) but broke behavioral equivalence.

### Diff Test Integration — The Fix
We integrated differential testing directly into the validation pipeline. Now when code compiles, we automatically:
1. Compile both C and Rust as standalone executables
2. Run both
3. Compare stdout byte-for-byte

If there's a mismatch, the feedback goes to the repair agent: "Your Rust output was `true\nfalse\n` but the C output was `1\n0\n`."

The repair agent understood immediately and changed `is_even() -> bool` back to `is_even() -> i32` with `if n % 2 == 0 { 1 } else { 0 }`. One iteration. Problem solved.

**Insight**: The repair loop was designed for compilation errors, but behavioral mismatches are just as important. A program that compiles but produces wrong output is worse than one that doesn't compile at all — at least compilation errors are visible.

### Linked List — The Real Test
`linked_list.c` with `malloc`, `free`, `Node*`, and `**head` (double pointer) migrated to `Option<Box<Node>>` with zero unsafe blocks. The LLM:
- Replaced `malloc/free` with Box (automatic cleanup)
- Replaced `**head` with `&mut Option<Box<Node>>`
- Used `head.take()` for the ownership transfer
- Added `// list is automatically freed when it goes out of scope` comment

Diff test PASSED on first attempt. This is what good migration looks like.

### Buffer.c — Structural Transformation
The C `Buffer` struct with `char data[256]` + `int len` + `memcpy` became a Rust struct with `String` and `Result<(), BufferError>`. The LLM didn't just translate syntax — it understood that a bounded char array in C maps to a capacity-checked String in Rust.

### Error Codes — Paradigm Shift
C's `ErrorCode` enum + out-params (`int *result`) became `Result<i32, MathError>`. The manual overflow check with `__INT_MAX__` became `a.checked_add(b).ok_or(MathError::Overflow)`. The LLM used Rust's type system instead of C's runtime checks.

### RAG Pattern Injection
We built a PatternStore with seed patterns (ptr_to_slice, malloc_to_vec, error_to_result) and inject the 3 most relevant ones as few-shot examples into every translation prompt. We can see in the logs which patterns were used. This will scale as we add more successful translations.

### Key Numbers
| File | Difficulty | Score | Unsafe | Diff Test | Repair Iterations |
|------|-----------|-------|--------|-----------|-------------------|
| add.c | Easy | 100 | 0 | PASS | 0 |
| power.c | Medium | 100 | 0 | PASS | 1 (bool->int) |
| gcd.c | Easy | 100 | 0 | PASS | 0 |
| linked_list.c | Hard | 100 | 0 | PASS | 0 |
| buffer.c | Medium | 100 | 0 | PASS | 0 |
| error_codes.c | Medium | 92 | 0 | PASS | 0 |

### Architectural Decisions That Paid Off
1. **C2Rust as "step zero"**: We don't have c2rust installed, and everything still works. The graceful fallback chain (LLM -> sync -> rule-translate) means no hard dependency.
2. **Diff testing as validation**: Not just a CLI flag but built into the core pipeline. Every migration is automatically verified.
3. **Repair loop with behavioral feedback**: The key innovation. Compiler errors tell you what's syntactically wrong. Diff tests tell you what's semantically wrong. The repair agent needs both.

### Cost Observation
Each function costs ~4 API calls (analysis + translation + validation/repair + test gen). At Sonnet rates, that's roughly $0.10-0.30 per function depending on complexity and repair iterations. A full miniz migration (~100 functions) would cost approximately $15-30.

---

## Insights for Future Reference

### On LLM Translation Quality
- Simple arithmetic functions (add, gcd, fibonacci): ~100% first-pass success
- Functions with type semantics (int-as-bool): Need diff test to catch
- Pointer-heavy code (linked lists): Surprisingly good — LLMs understand the `Box<>` / `Option<>` mapping well
- Error code patterns: LLMs produce genuinely better Rust than the C original

### On Differential Testing
- It's the single most important verification mechanism
- "Compiles and looks good" is not enough — output must match byte-for-byte
- The bool-vs-int case would have been invisible without diff testing
- Must have `main()` in both C and Rust for diff testing to work

### On the Repair Loop
- Most repairs are 0-1 iterations (the LLM gets it right first time)
- Behavioral mismatches are harder to repair than compilation errors
- The repair prompt must explicitly say "output must match byte-for-byte"
- Temperature 0.2 for repair (precision) vs 0.3 for translation (creativity)

### On Architecture
- The 9-stage pipeline (Extracted -> Classified -> C2Rust -> Analyzed -> Refined -> Validated -> Repairing -> FallbackUnsafe) with TestGen as an optional final stage works well
- Keeping sync and async paths separate avoids complexity
- PatternStore (RAG) provides context without fine-tuning
