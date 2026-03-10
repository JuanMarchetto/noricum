# miniz_zip.c Run 5: Hybrid Repair Validation Plan

> **For agentic workers:** REQUIRED: Use superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run miniz_zip.c (4895 LOC) migration with DeepSeek + warm-start to validate that P30 hybrid repair (rule engine → surgical repair → legacy fallback) reduces assembly errors more effectively than whole-file repair.

**Architecture:** Warm-start seeds 17 modules from best previous results (5 Skip, 11 SeedRepair, 1 Retranslate). After module migration, assembly combines all outputs. P30 hybrid repair then applies: Phase 1 (mechanical rules, 0 LLM cost) → Phase 2 (surgical per-function LLM, ~100 LOC per call) → Phase 3 (legacy whole-file, max 3 iters). Success = fewer final errors than Run 4's 4 errors, ideally 0.

**Working directory:** All commands assume `cd /home/marche/noricum` (the project root).

**Tech Stack:** Rust 2024, DeepSeek API (deepseek-chat + deepseek-reasoner), noricum CLI

**Previous runs for comparison:**
| Run | Provider | Cost | Module avg_score | Assembly errors start | Assembly errors final | Outcome |
|-----|----------|------|-----------------|----------------------|----------------------|---------|
| 3 | Claude | $7.53 | 85 | 95 | 4 (iter-07) | FallbackUnsafe |
| 4 | DeepSeek+warmstart | ~$1 | 84 | 10 | 4→3 (retrans destroyed) | FallbackUnsafe |

**Warm-start manifest** (`.noricum-artifacts/miniz_zip-warmstart/manifest.json`):
- 5 Skip: `if`(100), `mz_p1`(97), `mz_p3`(100), `mz_p9`(100), `mz_p16`(100)
- 11 SeedRepair: `mz_p2`(100), `mz_p4`(100), `mz_p5`(68), `mz_p6`(100), `mz_p7`(100), `mz_p8`(57), `mz_p10`(69), `mz_p11`(93), `mz_p12`(74), `mz_p13`(73), `mz_p15`(100)
- 1 Retranslate: `mz_p14`(26)

---

## Chunk 1: Launch, Monitor, and Analyze

### Task 1: Pre-flight checks

**Files:**
- Read: `.env` (DeepSeek API key)
- Read: `.noricum-artifacts/miniz_zip-warmstart/manifest.json`
- Read: `tests/fixtures/miniz/miniz_zip.c`

- [ ] **Step 1: Verify DeepSeek API key is available**

```bash
grep -c "DEEPSEEK_API_KEY" .env && echo "Key present"
```

Expected: `1` and `Key present`

- [ ] **Step 2: Verify warm-start manifest has 17 modules**

```bash
python3 -c "
import json
d = json.load(open('.noricum-artifacts/miniz_zip-warmstart/manifest.json'))
modules = d['modules']
print(f'Modules: {len(modules)}')
skip = [m['name'] for m in modules if m['state'] == 'Validated']
seed = [m['name'] for m in modules if m['state'] == 'NearlyCompiles']
retrans = [m['name'] for m in modules if m['state'] == 'FallbackUnsafe']
print(f'Skip ({len(skip)}): {skip}')
print(f'SeedRepair ({len(seed)}): {seed}')
print(f'Retranslate ({len(retrans)}): {retrans}')
"
```

Expected: 5 Skip, 11 SeedRepair, 1 Retranslate (total 17)

- [ ] **Step 3: Verify warm-start module files exist**

```bash
ls .noricum-artifacts/miniz_zip-warmstart/03-translation/ | wc -l
```

Expected: 17 files

- [ ] **Step 4: Verify miniz_zip.c fixture exists and is 4895 lines**

```bash
wc -l tests/fixtures/miniz/miniz_zip.c
```

Expected: `4895 tests/fixtures/miniz/miniz_zip.c`

- [ ] **Step 5: Quick compile check — P30 code is in place**

```bash
cargo check --workspace 2>&1 | tail -3
```

Expected: `Finished` with no errors

---

### Task 2: Launch Run 5

- [ ] **Step 1: Export API keys and launch migration**

```bash
set -a && source .env && set +a && \
cargo run -p noricum-cli -- migrate tests/fixtures/miniz/miniz_zip.c \
  --provider deepseek \
  --warm-start .noricum-artifacts/miniz_zip-warmstart \
  --skip-c2rust \
  --max-llm-calls 80 \
  -vv \
  2>&1 | tee run5-output.log
```

NOTE: `--max-llm-calls 80` is set higher than the default (50) because Run 4 hit the limit at 56 calls.
The warm-start reduces module LLM calls (5 Skip = 5 fewer translations), but surgical repair in
Phase 2 adds new LLM calls. 80 should be sufficient. If the run aborts with a budget error,
re-run with `--max-llm-calls 120`.

This will take 30-90 minutes. Monitor the output for these key log lines:

**Module phase** (first ~20 min):
- `warm-start: skip` — modules being reused (expect 5: if, mz_p1, mz_p3, mz_p9, mz_p16)
- `warm-start: seed-repair` — modules starting from saved code (expect 11)
- `warm-start: retranslate` — modules being re-translated (expect 1: mz_p14)
- `repair succeeded, validated` — modules that pass after repair
- `FallbackUnsafe` — modules that fail repair

**Assembly phase** (next ~10-60 min):
- `P30 Phase 1: applying mechanical repair rules` — rule engine starting
- `P30 Phase 1 complete` — shows `errors_before` and `errors_after` (key metric!)
- `P30 Phase 2: surgical per-function repair` — surgical repair starting
- `P30 Phase 2: sending surgical repair request` — each surgical fix attempt
- `P30 Phase 2: surgical repair resolved all errors` — best case: done here
- `P30 Phases 1-2 complete, falling back to Phase 3` — if surgical didn't resolve all
- `P30 Phase 3: entering legacy repair` — fallback with max 3 iters

- [ ] **Step 2: Wait for completion**

The migration will print a final summary. Wait for it to finish or hit a budget limit.

---

### Task 3: Analyze results

- [ ] **Step 1: Find the artifact directory for this run**

```bash
ls -lt .noricum-artifacts/ | head -5
```

The newest `miniz_zip-*` directory is Run 5. Save its name:

```bash
RUN5_DIR=$(ls -td .noricum-artifacts/miniz_zip-2026* | head -1)
echo "Run 5 artifacts: $RUN5_DIR"
```

- [ ] **Step 2: Check final result**

```bash
cat "$RUN5_DIR/manifest.json" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(f'Final state: {d.get(\"final_state\", \"unknown\")}')
print(f'Score: {d.get(\"idiomatic_score\", d.get(\"score\", \"unknown\"))}')
metrics = d.get('metrics', {})
print(f'LLM calls: {metrics.get(\"llm_calls\", d.get(\"llm_calls\", \"unknown\"))}')
print(f'Unsafe: {d.get(\"unsafe_count\", \"unknown\")}')
# Also check module-level results
modules = d.get('modules', [])
if modules:
    scores = [m['score'] for m in modules if 'score' in m]
    states = [m.get('state','?') for m in modules]
    validated = sum(1 for s in states if s == 'Validated')
    print(f'Modules: {len(modules)}, Validated: {validated}/{len(modules)}')
    print(f'Avg module score: {sum(scores)/len(scores):.0f}')
"
```

- [ ] **Step 3: Check the assembly repair artifacts for P30 phases**

```bash
ls "$RUN5_DIR/05-repair/"
```

Look for:
- `iter-00.rs` — Phase 1 rule engine output (only present if rules fired and modified the source)
- `iter-01.rs` through `iter-05.rs` — Phase 2 surgical repair outputs (one per cycle)
- Any legacy `iter-*` files with higher numbers — Phase 3 fallback iterations
- Any `iter-*-rejected.rs` — repairs that were rejected by quality floor

- [ ] **Step 4: Measure final output size and function count**

NOTE: Standalone `rustc` won't work on the assembled file (it uses external crates and merged
module imports). Use the log grep in Step 5 for error counts instead.

```bash
echo "=== Final output ==="
if [ -f "$RUN5_DIR/06-final.rs" ]; then
  wc -l "$RUN5_DIR/06-final.rs"
  grep -c "^fn \|^pub fn " "$RUN5_DIR/06-final.rs"
  echo "functions in final output"
  grep -c "unsafe" "$RUN5_DIR/06-final.rs" || echo "0 unsafe blocks"
else
  echo "No final output file — check manifest for outcome"
fi

# Check Phase 1 rule engine output (only exists if rules fired)
if [ -f "$RUN5_DIR/05-repair/iter-00.rs" ]; then
  echo ""
  echo "=== Phase 1 rule engine output exists ==="
  wc -l "$RUN5_DIR/05-repair/iter-00.rs"
else
  echo ""
  echo "=== No Phase 1 output (rules may not have fired) ==="
fi
```

- [ ] **Step 5: Extract P30 metrics from the log**

```bash
echo "=== P30 Phase 1 ==="
grep "P30 Phase 1 complete" run5-output.log

echo ""
echo "=== P30 Phase 2 cycles ==="
grep "P30 Phase 2" run5-output.log

echo ""
echo "=== P30 Phase 3 ==="
grep "P30 Phase 3" run5-output.log

echo ""
echo "=== Total LLM calls ==="
grep -c "sending.*request\|repair call\|translate" run5-output.log
echo "approximate LLM calls"
```

- [ ] **Step 6: Compare with previous runs**

Fill in this comparison table from the results:

```
| Metric                    | Run 3 (Claude) | Run 4 (DS+warm) | Run 5 (DS+warm+P30) |
|---------------------------|----------------|-----------------|---------------------|
| Provider                  | Claude         | DeepSeek        | DeepSeek            |
| Cost (est.)               | $7.53          | ~$1             |                     |
| Module avg_score          | 85             | 84              |                     |
| Assembly errors (start)   | 95             | 10              |                     |
| P30 Phase 1 reduction     | N/A            | N/A             |                     |
| P30 Phase 2 reduction     | N/A            | N/A             |                     |
| Assembly errors (final)   | 4              | 4→3             |                     |
| Final functions           | 96 (manual)    | 4 (destroyed)   |                     |
| Final LOC                 | 2829 (manual)  | 1081            |                     |
| Final state               | FallbackUnsafe | FallbackUnsafe  |                     |
| Total LLM calls           | ~50            | ~56             |                     |
```

---

### Task 4: Post-run decisions

Based on the results, decide next steps:

- [ ] **Step 1: Evaluate P30 effectiveness**

**If P30 resolved ALL errors (0 final errors):**
- This is a major milestone — first autonomous compilation of miniz_zip.c
- Update MEMORY.md with Run 5 results
- Commit the run log and results summary
- Consider running diff test validation

**If P30 reduced errors but didn't reach 0:**
- Document which error codes remain (E0499? E0308? E0382?)
- Check if remaining errors are in the same functions as Run 3/4
- Decide if new rules should be added to the rule engine
- Check if surgical repair prompts need refinement

**If P30 made no improvement over Run 4:**
- Check Phase 1 log — did rules even fire?
- Check Phase 2 log — did surgical repair produce valid fixes?
- Examine the surgical repair inputs/outputs for quality
- Consider if the warm-start modules changed the assembly enough to invalidate the rules

- [ ] **Step 2: Update MEMORY.md**

Add Run 5 results to the Implementation Status section in:
`/home/marche/.claude/projects/-home-marche-noricum/memory/MEMORY.md`

Include: final state, score, error count at each phase, cost, LLM calls, comparison with Run 4.

- [ ] **Step 3: Curate warm-start for potential Run 6**

If Run 5 produced better module outputs than Run 4, update the warm-start:

```bash
# Only if modules improved — compare manifests first
python3 -c "
import json
run5 = json.load(open('$RUN5_DIR/manifest.json'))
warm = json.load(open('.noricum-artifacts/miniz_zip-warmstart/manifest.json'))
# Compare each module's score
for r5m in run5.get('modules', []):
    wm = next((w for w in warm['modules'] if w['name'] == r5m['name']), None)
    if wm:
        if r5m['score'] > wm['score']:
            print(f'{r5m[\"name\"]}: {wm[\"score\"]} -> {r5m[\"score\"]} (IMPROVED)')
        elif r5m['score'] < wm['score']:
            print(f'{r5m[\"name\"]}: {wm[\"score\"]} -> {r5m[\"score\"]} (REGRESSED)')
"
```

If improvements found, copy improved module files to warm-start and update manifest.

- [ ] **Step 4: Commit results**

```bash
git add run5-output.log
git commit -m "docs: miniz_zip.c Run 5 results — P30 hybrid repair validation"
```
