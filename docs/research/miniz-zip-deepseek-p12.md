# miniz_zip.c Migration with DeepSeek R1 + P12 Sub-Chunking

**Date:** 2026-03-10
**File:** `tests/fixtures/miniz/miniz_zip.c` (4895 LOC, 123 functions)
**Provider:** DeepSeek R1 (`deepseek-reasoner`)
**Duration:** ~5.5 hours (23:42 - 05:29 UTC)
**Total LLM calls:** 51 (exceeded limit of 50)
**Result:** Assembly never ran (hit call limit after module 9/10 completed)

## Module Breakdown (P12 Split)

| Module | LOC | Funcs | Status | Best Score | Compiles | Repairs | Initial Errors | Notes |
|--------|-----|-------|--------|------------|----------|---------|----------------|-------|
| if | 363 | 3 | Validated | 100 | Yes | 0 | 0 | Clean first pass |
| mz_p1 | 674 | 23 | Validated | 97 | Yes | 3 | 7 | P0 rejected iter 1,2 (unsafe↑) |
| mz_p2 | 919 | 14 | SKIPPED | - | - | 0 | - | API decode error on chunk 2/2 |
| mz_p3 | 846 | 14 | FallbackUnsafe | 64 | Yes | 5 | 108 | Compiled at iter 4, score↑ iter 5 |
| mz_p4 | 923 | 12 | FallbackUnsafe | 42 | No | 5 | 171 | Never compiled |
| mz_p5 | ~700 | ? | FallbackUnsafe | 15 | ? | 5 | ? | Substance penalty |
| mz_p6 | ~900 | ? | FallbackUnsafe | 87 | Yes | 5 | ? | Almost validated |
| mz_p7 | 831 | 5 | FallbackUnsafe | 48 | Yes | 5 | 25 | Compiled iter 4, P1 saved best=48 |
| mz_p8 | 920 | 6 | FallbackUnsafe | 5 | No | 5 | 251 | Irrecoverable — chunked garbage |
| mz_p9 | 590 | 19 | Validated | 100 | Yes | 3 | 118 | Score 100 throughout |

**Totals: 3 Validated, 6 FallbackUnsafe, 1 Skipped**

## Key Findings

### 1. Module Size → Success Rate (DeepSeek R1)

| LOC Range | Modules | Validated | Rate |
|-----------|---------|-----------|------|
| <400 | 1 (if) | 1 | 100% |
| 400-700 | 2 (mz_p1, mz_p9) | 2 | 100% |
| 700-850 | 2 (mz_p3, mz_p7) | 0 | 0% (but both compiled) |
| 850-930 | 4 (mz_p2, mz_p4, mz_p6, mz_p8) | 0 | 0% |

**Sweet spot for DeepSeek R1: <700 LOC modules.**

### 2. DeepSeek R1 Repair Performance

- ~8 minutes per repair call (vs ~2 min with Claude)
- 10 modules × up to 6 calls each = budget catastrophe
- Total 51 calls over 5.5h = average 6.5 min per call
- Budget of 50 calls insufficient for 10 modules with repair loops

### 3. Translation Quality Indicators

- Modules with <30 initial compilation errors: recoverable (mz_p1: 7 errors, mz_p7: 25 errors)
- Modules with >100 initial errors: unrecoverable in 5 repairs (mz_p3: 108, mz_p4: 171, mz_p8: 251)
- **Proposed heuristic:** If initial errors > 100, re-translate instead of repair

### 4. P0 Quality Floor Working

- mz_p1: P0 correctly rejected 2 repairs where unsafe count increased
- Final repair (iter 3) succeeded without adding unsafe

### 5. P1 Best-Version Tracking Working

- mz_p7: Score peaked at 48 (iter 4, compiled), then repair degraded to 15 (iter 5)
- P1 correctly used the 48-score version as final output

### 6. API Stability

- DeepSeek had 1 "error decoding response body" on mz_p2 chunk 2/2
- Module entirely lost (no retry logic for decode errors)
- Anthropic has never had this failure mode in our testing

## Comparison: Pre-P12 vs Post-P12

| Metric | Run 1 (no P12) | Run 2 (P12) |
|--------|----------------|-------------|
| Modules | 2 (if + mz) | 10 |
| Largest module | 4766 LOC | 938 LOC |
| Validated modules | 1/2 (if only) | 3/10 |
| Best mz_* score | 15 (stubs) | 100 (mz_p9) |
| Duration | 85 min | 330 min |
| LLM calls | 19 | 51 |
| Compiled modules | 1/2 | 5-6/10 |

**P12 is a massive improvement.** The mz module went from a single 4766-LOC stub to 9 sub-modules where several produce real, compiling Rust code.

## Recommended Improvements

### P13: Re-translate on high error count
If initial compilation produces >100 errors, discard and re-translate with different temperature instead of entering repair loop. Would have saved ~40 min on mz_p8.

### P14: API error retry
Add retry (with backoff) for HTTP decode errors. Would have saved mz_p2.

### P15: Adaptive LLM call budget
Set `max_llm_calls` based on module count: `module_count * 7 + 10`. For 10 modules: 80 calls.

### P16: Hybrid provider for repair
Use DeepSeek R1 for analysis + translation (where reasoning depth helps), but switch to `deepseek-chat` or Claude for repair (where speed matters). Each repair call is 8 min with R1 vs potentially 2 min with a faster model.

### P17: Tighter sub-module targeting
For DeepSeek R1, target 500 LOC sub-modules instead of 600. The data shows <700 LOC is the success threshold.

## Artifacts

All intermediate outputs saved to:
```
.noricum-artifacts/miniz_zip-20260309-234234/
├── 00-c-source.c
├── 01-analysis.json
├── 03-translation/
│   ├── module-if.rs (4.7 KB)
│   ├── module-mz_p1.rs (16 KB)
│   ├── module-mz_p3.rs (33 KB)
│   ├── module-mz_p4.rs (28 KB)
│   ├── module-mz_p5.rs (25 KB)
│   ├── module-mz_p6.rs (38 KB)
│   ├── module-mz_p7.rs (29 KB)
│   ├── module-mz_p8.rs (28 KB)
│   └── module-mz_p9.rs (9.5 KB)
└── 05-repair/ (empty — repair artifacts not populated)
```

Total Rust output: ~212 KB across 9 module files.
