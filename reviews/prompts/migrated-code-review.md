# Migrated Code Review Prompt

Evaluates a single migrated Rust file for safety, idiom, and correctness.
Combines perspectives: P2 (Security), P3 (Rust Quality), P4 (Test Coverage).

## USAGE

```bash
# Review a specific migrated file
claude -p "$(cat reviews/prompts/migrated-code-review.md)" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text \
    -- "Review file: output/migrated.rs against source: tests/fixtures/simple/example.c"

# Or with environment variables
REVIEW_FILE=output/migrated.rs SOURCE_FILE=tests/fixtures/simple/power.c \
    claude -p "$(cat reviews/prompts/migrated-code-review.md)" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)'
```

## PROMPT

You are reviewing a Rust file that was migrated from C source code by the Noricum migration agent. Evaluate it from three expert perspectives.

### Input

Read the migrated Rust file and its original C source. If environment variables `REVIEW_FILE` and `SOURCE_FILE` are set, use those paths. Otherwise, the file paths will be provided in the user message.

### P2: Security Review

- [ ] No `unsafe` blocks (or each one is justified)
- [ ] No raw pointer dereference
- [ ] No unchecked array indexing (use `.get()` or bounds checks)
- [ ] No integer overflow potential (use `checked_*` or `wrapping_*` where needed)
- [ ] No path traversal or injection from C string handling
- [ ] Buffer operations use safe Rust abstractions (Vec, slice, String)
- [ ] Memory management uses ownership (no manual alloc/dealloc)

### P3: Rust Quality Review

- [ ] Uses `Result`/`Option` instead of error codes or null pointers
- [ ] No `unwrap()` (use `?`, `unwrap_or`, `expect` with context)
- [ ] Proper ownership — no unnecessary `.clone()`
- [ ] Idiomatic constructs: iterators, pattern matching, `if let`
- [ ] Types are appropriate (`usize` for indices, `i32`/`i64` matching C types)
- [ ] Functions are well-scoped (not a monolithic translation)
- [ ] Naming follows Rust conventions (snake_case functions, CamelCase types)
- [ ] No C-isms (`while true`, manual index loops, sentinel values)

### P4: Correctness & Testing Review

- [ ] Logic matches the original C behavior
- [ ] Edge cases handled (null/empty input, overflow, zero-length)
- [ ] Return values match C semantics (or improved with Result)
- [ ] Printf format strings correctly translated
- [ ] Main function entry point preserved (if applicable)
- [ ] Would pass differential testing (same output as C for same input)

### Output Format

```markdown
## Migrated Code Review: {filename}

### Security Score: X/10
**Findings:**
- ...

### Rust Quality Score: X/10
**Findings:**
- ...

### Correctness Score: X/10
**Findings:**
- ...

### Overall Score: X/10

### Blocking Issues
1. ...

### Recommendations
1. ...
```
