---
name: migration-completeness-check
description: Mandatory completeness verification before claiming any migration is "done". Use EVERY time before saying "complete", "migrated", "done", "terminado", or using celebration language about a C-to-Rust migration. Also trigger proactively when you notice yourself about to declare completion.
---

# Migration Completeness Check

Run this checklist before EVER claiming a migration is complete. If any check fails, list the gaps explicitly instead of using completion language.

## Why This Exists

There is a documented pattern of declaring migrations "done" when they are clearly incomplete — treating "compiles" as "finished", celebrating metrics that mask gaps, and waiting for the user to notice problems. This skill prevents that.

## The Checklist

### 1. Function Coverage
```bash
# Count C functions (from header or source)
grep -cE "^[a-z_].*\(" <c_header_file>
# Count Rust public functions
grep -c "^pub fn" <rust_file>
```
- Report: "X/Y functions implemented (Z%)"
- Threshold: <90% = NOT complete

### 2. Stub/TODO Audit
```bash
grep -cn "TODO\|todo!\|Placeholder\|placeholder\|unimplemented!\|stub\|FIXME" <rust_file>
```
- Report: "N stubs/TODOs remaining" with the list
- Threshold: >0 = NOT complete (list each one)

### 3. LOC Ratio Sanity Check
- If Rust LOC < 30% of C LOC, it's likely a skeleton, not a full translation
- If Rust LOC > 150% of C LOC, something may be wrong (bloat or duplication)
- Expected range: 40-120% of C LOC for a real translation

### 4. Compilation Verification
```bash
rustc --edition=2024 --crate-type=lib <rust_file> 2>&1 | grep "^error" | wc -l
```
- Must be 0 errors
- Also count warnings and unsafe blocks

### 5. Diff Test
- Can the Rust version produce the same output as the C version for real inputs?
- Not just checksums — test the PRIMARY functionality (for ZIP: create archive, add file, extract file)
- If no diff test exists or passes, say so explicitly

### 6. API Parity
- Compare the public API surface: every function declared in the C header should have a Rust equivalent
- List missing functions by name

### 7. Functional Completeness
- Does the Rust code actually DO what the C code does, or does it just have the right types/signatures?
- Check for: empty function bodies, hardcoded return values, placeholder logic
```bash
grep -n "false // TODO\|None // \|0 // \|return Ok(())" <rust_file> | head -20
```

## Output Format

```
## Migration Completeness: [target name]

| Check | Result | Pass? |
|-------|--------|-------|
| Function coverage | X/Y (Z%) | ✅/❌ |
| Stubs/TODOs | N remaining | ✅/❌ |
| LOC ratio | X:Y (Z%) | ✅/❌ |
| Compiles | 0 errors | ✅/❌ |
| Diff test | X/Y tests pass | ✅/❌ |
| API parity | X/Y public functions | ✅/❌ |
| Functional completeness | [assessment] | ✅/❌ |

### Remaining gaps:
1. [specific gap]
2. [specific gap]

### Verdict: [COMPLETE / NOT COMPLETE — N gaps remaining]
```

## Rules

- NEVER skip this checklist when claiming completion
- NEVER use "done", "complete", "migrated", "terminado", "🎉" if any check fails
- Instead say: "Compiles with N gaps remaining: [list]"
- If the user asks "is it done?" — run the checklist, don't guess
- Proactively run this when you notice yourself about to celebrate
