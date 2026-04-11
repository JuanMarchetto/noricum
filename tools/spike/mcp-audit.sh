#!/usr/bin/env bash
# spike-mcp-audit — 5-minute sanity check of the Noricum MCP tools.
#
# Because the MCP server runs inside Claude Code (not as a standalone
# process), this script CANNOT drive the tools directly. Instead, it
# prints a checklist of calls the operator should make in a Claude Code
# session, with expected outputs, and writes the results to a report file.
#
# The operator copies the tool calls into Claude Code, records the
# results, and reports back. Any tool that returns empty / null fields is
# flagged as broken in docs/methodology/mcp-tool-limits.md.
#
# Usage:
#   tools/spike/mcp-audit.sh [report-file]
#
# Output:
#   Prints instructions to stdout. The operator follows them in a
#   Claude Code session. Results go into $report_file (default:
#   /tmp/noricum-mcp-audit-{date}.md).

set -euo pipefail

REPORT="${1:-/tmp/noricum-mcp-audit-$(date +%Y%m%d).md}"

cat > "$REPORT" <<'EOF'
# Noricum MCP Tool Audit

Run this audit from a Claude Code session with the noricum-mcp server
loaded. For each tool below, issue the call, check the return, and fill
in the "actual" line. Flag any tool whose output is empty / null / partial.

## 1. `mcp__noricum__migrate_function`

```
input: source = "int add(int a, int b) { return a + b; }"
```

**Expected:** non-empty `rust_source` field, `state: "Validated"` or similar terminal state.
**Actual:** _____________________________________________

## 2. `mcp__noricum__analyze_function`

```
input: source = "int add(int a, int b) { return a + b; }"
```

**Expected:** non-null difficulty / patterns / analysis fields.
**Actual:** _____________________________________________

## 3. `mcp__noricum__check_compilation`

```
input: source = "fn main() { println!(\"hello\"); }"
```

**Expected:** success, no errors.
**Actual:** _____________________________________________

## 4. `mcp__noricum__get_idiomatic_score`

```
input: source = "fn main() { println!(\"hello\"); }"
```

**Expected:** non-null score in the 70-100 range.
**Actual:** _____________________________________________

## 5. `mcp__noricum__diff_test`

```
input: c_source = "#include <stdio.h>\nint main() { printf(\"hello\\n\"); return 0; }"
input: rust_source = "fn main() { println!(\"hello\"); }"
```

**Expected:** both compile, both print "hello", diff test passes.
**Actual:** _____________________________________________

## 6. `mcp__noricum__repair`

```
input: source = "fn main() { let x: i32 = \"not a number\"; }"
```

**Expected:** repaired source with the type error fixed.
**Actual:** _____________________________________________

## Summary

- Working tools: _______
- Partial tools (returns some fields but not all): _______
- Broken tools (empty / null output): _______

## Action items

- [ ] For each broken tool, file an issue against the noricum-mcp crate
- [ ] Update docs/methodology/mcp-tool-limits.md with any new findings
- [ ] Add audit results to the spike's SPIKE.md before relying on any tool
EOF

echo "Audit template written to: $REPORT"
echo ""
echo "Next steps:"
echo "  1. Open a Claude Code session with noricum-mcp loaded"
echo "  2. Issue each of the 6 tool calls above"
echo "  3. Record the actual output in $REPORT"
echo "  4. Update docs/methodology/mcp-tool-limits.md with findings"
echo ""
echo "Run this audit before any spike that depends on MCP tools being functional."
