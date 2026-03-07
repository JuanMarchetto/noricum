#!/usr/bin/env bash
# Generate consolidated markdown report from behavioral review JSON results.
# Usage: bash reviews/gen-behavioral-report.sh
set -euo pipefail
cd "$(dirname "$0")/.."

INDIR="reviews/behavioral"
DATE=$(date +%Y-%m-%d)
REPORT="reviews/reports/behavioral-review-${DATE}.md"

# Collect all JSON files
JSONS=("$INDIR"/*.json)
if [ ${#JSONS[@]} -eq 0 ]; then
    echo "ERROR: No JSON files found in $INDIR/"
    exit 1
fi

COUNT=${#JSONS[@]}
echo "Generating report from $COUNT review results..."

# Use python3 to parse and generate the report
python3 - "$INDIR" "$REPORT" <<'PYEOF'
import json, sys, os, glob
from datetime import date
from pathlib import Path
from collections import Counter

indir = sys.argv[1]
outpath = sys.argv[2]

# Pair metadata for LOC counts
pair_meta = {
    "add": ("tests/fixtures/simple/add.c", "output/simple/add.rs", "Simple"),
    "buffer": ("tests/fixtures/simple/buffer.c", "output/simple/buffer.rs", "Simple"),
    "error_codes": ("tests/fixtures/simple/error_codes.c", "output/simple/error_codes.rs", "Simple"),
    "factorial": ("tests/fixtures/simple/factorial.c", "output/simple/factorial.rs", "Simple"),
    "fibonacci": ("tests/fixtures/simple/fibonacci.c", "output/simple/fibonacci.rs", "Simple"),
    "gcd": ("tests/fixtures/simple/gcd.c", "output/simple/gcd.rs", "Simple"),
    "linked_list": ("tests/fixtures/simple/linked_list.c", "output/simple/linked_list.rs", "Simple"),
    "max_min": ("tests/fixtures/simple/max_min.c", "output/simple/max_min.rs", "Simple"),
    "power": ("tests/fixtures/simple/power.c", "output/simple/power.rs", "Simple"),
    "strlen": ("tests/fixtures/simple/strlen.c", "output/simple/strlen.rs", "Simple"),
    "hash_table": ("tests/fixtures/medium/hash_table.c", "output/medium/hash_table.rs", "Medium"),
    "miniz_test": ("tests/fixtures/miniz/miniz_test.c", "output/miniz/miniz_test.rs", "Complex"),
    "cjson_combined": ("tests/fixtures/cjson/cjson_combined.c", "output/cjson/cjson_combined.rs", "Complex"),
    "expr_eval": ("tests/fixtures/large/expr_eval.c", "output/large/expr_eval.rs", "Complex"),
}

def count_lines(filepath):
    try:
        with open(filepath) as f:
            return sum(1 for _ in f)
    except FileNotFoundError:
        return 0

# Load all results
results = []
for jpath in sorted(glob.glob(os.path.join(indir, "*.json"))):
    name = Path(jpath).stem
    if name.endswith("-stderr"):
        continue
    try:
        with open(jpath) as f:
            data = json.load(f)
        data["_name"] = name
        meta = pair_meta.get(name)
        if meta:
            data["_c_path"] = meta[0]
            data["_rs_path"] = meta[1]
            data["_category"] = meta[2]
            data["_c_loc"] = count_lines(meta[0])
            data["_rs_loc"] = count_lines(meta[1])
        else:
            data["_c_path"] = "?"
            data["_rs_path"] = "?"
            data["_category"] = "?"
            data["_c_loc"] = 0
            data["_rs_loc"] = 0
        results.append(data)
    except (json.JSONDecodeError, KeyError) as e:
        print(f"WARNING: skipping {jpath}: {e}", file=sys.stderr)

if not results:
    print("ERROR: No valid results to report.", file=sys.stderr)
    sys.exit(1)

# Compute stats
verdicts = Counter(r.get("verdict", "UNKNOWN") for r in results)
confidences = [r.get("confidence", 0) for r in results]
avg_confidence = sum(confidences) / len(confidences) if confidences else 0

lines = []
w = lines.append

w(f"# Behavioral Equivalence Review Report")
w(f"")
w(f"**Date:** {date.today().isoformat()}")
w(f"**Tool:** Noricum `review` command with Claude LLM")
w(f"**Method:** 6-phase behavioral analysis + diff testing")
w(f"**Total pairs reviewed:** {len(results)}")
w(f"")

# Aggregate stats
w(f"## Aggregate Statistics")
w(f"")
w(f"| Metric | Value |")
w(f"|--------|-------|")
for v in ["EQUIVALENT", "LIKELY EQUIVALENT", "DIVERGENT", "INSUFFICIENT DATA"]:
    cnt = verdicts.get(v, 0)
    pct = cnt / len(results) * 100 if results else 0
    w(f"| {v} | {cnt} ({pct:.0f}%) |")
w(f"| **Average Confidence** | **{avg_confidence:.1f}%** |")
w(f"| Total C LOC | {sum(r['_c_loc'] for r in results)} |")
w(f"| Total Rust LOC | {sum(r['_rs_loc'] for r in results)} |")
w(f"")

# Summary table
w(f"## Summary Table")
w(f"")
w(f"| # | Name | Category | C LOC | Rust LOC | Verdict | Confidence |")
w(f"|---|------|----------|-------|----------|---------|------------|")
for i, r in enumerate(results, 1):
    name = r["_name"]
    cat = r["_category"]
    cloc = r["_c_loc"]
    rsloc = r["_rs_loc"]
    verdict = r.get("verdict", "?")
    conf = r.get("confidence", 0)
    w(f"| {i} | {name} | {cat} | {cloc} | {rsloc} | {verdict} | {conf}% |")
w(f"")

# Group by category
for cat in ["Simple", "Medium", "Complex"]:
    cat_results = [r for r in results if r["_category"] == cat]
    if not cat_results:
        continue
    w(f"## {cat} Files")
    w(f"")
    for r in cat_results:
        name = r["_name"]
        w(f"### {name}")
        w(f"")
        w(f"- **C source:** `{r['_c_path']}` ({r['_c_loc']} LOC)")
        w(f"- **Rust source:** `{r['_rs_path']}` ({r['_rs_loc']} LOC)")
        w(f"- **Verdict:** {r.get('verdict', '?')}")
        w(f"- **Confidence:** {r.get('confidence', 0)}%")
        w(f"")
        summary = r.get("summary", "No summary available.")
        w(f"**Summary:** {summary}")
        w(f"")

# Confidence distribution
w(f"## Confidence Distribution")
w(f"")
brackets = {"90-100%": 0, "80-89%": 0, "70-79%": 0, "60-69%": 0, "<60%": 0}
for c in confidences:
    if c >= 90:
        brackets["90-100%"] += 1
    elif c >= 80:
        brackets["80-89%"] += 1
    elif c >= 70:
        brackets["70-79%"] += 1
    elif c >= 60:
        brackets["60-69%"] += 1
    else:
        brackets["<60%"] += 1

w(f"| Range | Count |")
w(f"|-------|-------|")
for rng, cnt in brackets.items():
    bar = "#" * cnt
    w(f"| {rng} | {cnt} {bar} |")
w(f"")

# Recommendations
w(f"## Methodology Notes")
w(f"")
w(f"Each pair was reviewed using Noricum's behavioral equivalence agent which performs:")
w(f"1. **Structural mapping** - function-by-function correspondence")
w(f"2. **Type semantics** - integer width, signedness, bool-as-int checks")
w(f"3. **Control flow equivalence** - branching, loops, error paths")
w(f"4. **Standard library mapping** - printf formats, malloc/free, string ops")
w(f"5. **Edge cases & UB** - overflow, null pointers, uninitialized memory")
w(f"6. **Output equivalence** - byte-for-byte stdout, exit codes")
w(f"")
w(f"Diff tests were run for each pair, providing empirical validation alongside the LLM analysis.")
w(f"")

os.makedirs(os.path.dirname(outpath), exist_ok=True)
with open(outpath, "w") as f:
    f.write("\n".join(lines) + "\n")

print(f"Report written to {outpath}")
print(f"  {len(results)} pairs, avg confidence {avg_confidence:.1f}%")
for v in sorted(verdicts):
    print(f"  {v}: {verdicts[v]}")
PYEOF

echo "Done: $REPORT"
