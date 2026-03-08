# miniz Migration Attempt — Lessons Learned

**Date:** 2026-03-08
**Input:** `tests/fixtures/miniz/miniz_core_standalone.c` (4429 LOC)
**Result:** FallbackUnsafe (compiles=true, diff_test=false, unsafe=9, score=0)

## Timeline

| Stage | Duration | Notes |
|-------|----------|-------|
| C2Rust transpilation | 1.5s | 9506 lines output |
| Analysis (Opus 4.6) | 38s | 20 patterns detected (coroutine_via_switch, ptr_arithmetic, etc.) |
| Chunked translation (8 chunks) | ~15 min | Structural chunking, data model first |
| Quality gate re-translate | ~2 min | Detected 10 unsafe → re-translated single-pass → 0 unsafe |
| Repair loop (8 iterations) | ~10 min | Oscillated between compile fix and quality degradation |
| **Total** | **~28 min** | 11 LLM calls |

## What Worked

### 1. C2Rust Output Truncation (NEW FIX)
- Original c2rust output: 9506 lines → exceeded 200K token API limit
- Fixed by truncating to 2000 lines for chunked translation, 3000 for single-pass
- First run failed at 204K tokens; second run succeeded

### 2. Quality Gate
- Chunked translation produced 10 unsafe blocks
- Quality gate re-translated entire file single-pass at temperature 0.5
- Result: 0 unsafe blocks, score 76/100 — excellent for compression code
- Shows that full-file context produces better results than chunked for this case

### 3. Analysis Agent
- Correctly identified all 20 C patterns including `coroutine_via_switch` (miniz's key difficulty)
- Produced excellent migration strategy (phase-by-phase approach)
- 38s is reasonable for 4429 LOC

## What Failed

### 1. Repair Loop Degradation
The repair agent oscillated destructively:

| Iter | Errors | Unsafe | Score | Compiles |
|------|--------|--------|-------|----------|
| Pre-repair | 16 | 0 | 76 | No |
| 1 | 10 | 2 | 59 | No |
| 2 | 26 | 0 | 55 | No |
| 3 | 2 | 10 | 0 | No |
| 4 | 6 | 10 | 0 | No |
| 5 | 0 | 8 | 0 | **Yes** |
| 6 | 0 | 9 | 0 | **Yes** (diff fail) |
| 7 | 6 | 11 | 0 | No |
| 8 | 0 | 9 | 0 | **Yes** (diff fail) |

**Key insight:** The repair agent trades quality for compilation. When it can't fix errors idiomatically, it falls back to unsafe patterns (raw pointers, transmute, etc.). This is the wrong tradeoff.

### 2. Context Loss in Repair
- The repair agent sees abbreviated C source (500 lines) + full Rust code + compiler errors
- For 4429 LOC, the abbreviated C source loses critical context about data structures and algorithms
- The repair agent doesn't understand the compression algorithm well enough to fix semantic issues

### 3. Diff Test Runtime Crash
- Rust output compiled but crashed at runtime (exit code 1)
- The compression/decompression algorithms require exact bit-level behavior
- Safe Rust translation of coroutine state machines is fundamentally hard

## Root Causes

1. **Coroutine via switch/case (Duff's device)**: miniz uses `TINFL_CR_BEGIN`/`TINFL_CR_RETURN` macros for coroutine-like control flow. This pattern has NO safe Rust equivalent — it requires reconstructing as an explicit state machine with a match loop.

2. **Large structs with internal pointers**: `tdefl_compressor` (~300KB struct) and `tinfl_decompressor` use internal buffer pointers that can't be modeled safely in Rust without significant redesign.

3. **Bit-level manipulation**: DEFLATE/INFLATE require exact bit packing/unpacking. Any semantic divergence in arithmetic causes completely wrong decompressed output.

4. **File too large for repair context**: At 4429 LOC, the repair agent's abbreviated C context (500 lines) doesn't capture enough of the algorithm for meaningful fixes.

## Pipeline Improvements Needed

### P0: Repair Quality Floor
- Never allow repair to increase unsafe count above translation baseline
- If repair introduces unsafe, reject the repair output and try with higher temperature
- Track "best quality" version separately from "best compiles" version

### P1: Best-Version Tracking
- Keep the highest-scoring version even if it doesn't compile
- Final output should be the best version, not the last version
- Especially important when repair oscillates

### P2: Per-Function C2Rust Context
- Instead of full c2rust output, extract only the c2rust functions matching the current chunk
- This gives the LLM targeted reference without noise

### P3: Incremental Migration
- Split large files into modules (checksum, inflate, deflate, zlib API)
- Migrate each module separately with its own test harness
- Combine at the end
- This is how miniz_oxide was built (it's organized by module)

### P4: Skip C2Rust for LLM-Only Path
- For files where the LLM produces good translations directly from C, skip c2rust entirely
- Save tokens and time
- Use c2rust only as a fallback, not as context

## Comparison with miniz_oxide

miniz_oxide (the human-written Rust port) exists and is battle-tested. Our attempt was for learning purposes. Key differences:
- miniz_oxide uses unsafe where necessary (14 unsafe blocks for performance-critical inner loops)
- miniz_oxide was written module by module over months by experienced Rust developers
- Our pipeline attempted a single-file translation in 28 minutes — unrealistic for this complexity level

## Conclusion

miniz represents the **ceiling of difficulty** for single-file automated migration. The pipeline demonstrated that it CAN produce high-quality idiomatic code (score 76, 0 unsafe) from 4429 LOC of compression C, but the repair loop fails to bring it to compilability without sacrificing quality. The fundamental issue is that compression algorithms require bit-exact semantic preservation, which the repair agent can't achieve through local fixes.

**Recommendation:** For files of this complexity, noricum should offer a "best-effort" mode that outputs the highest-quality version (even if it doesn't compile) alongside the compilable version, letting the developer finish the last mile manually.
