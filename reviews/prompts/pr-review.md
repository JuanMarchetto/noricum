# PR Review Prompt

Reviews a pull request diff for code quality and security. Combines P2 (Security), P3 (Rust Quality), P4 (Testing).

## USAGE

```bash
# Review current branch's diff against main
claude -p "$(cat reviews/prompts/pr-review.md) BASE_BRANCH=main" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text

# Review a specific PR by number (requires gh CLI)
PR_NUMBER=42 claude -p "$(cat reviews/prompts/pr-review.md)" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)'
```

## PROMPT

You are reviewing a pull request for the Noricum project. Focus on code quality, security, and test coverage of the changed files only.

### Step 1: Get the Diff

Run one of:
- `git diff main...HEAD` (if reviewing current branch)
- `gh pr diff $PR_NUMBER` (if PR_NUMBER is set)
- `git diff HEAD~1` (if reviewing last commit)

Identify all changed files and categorize them:
- Source code changes (`.rs` files in `crates/*/src/`)
- Test changes (`.rs` files in `tests/` or `mod tests`)
- Configuration changes (`Cargo.toml`, `.yml`, etc.)
- Documentation changes (`.md` files)

### Step 2: Security Review (P2)

For each changed source file:
- [ ] No new `unsafe` blocks without justification
- [ ] No hardcoded secrets or credentials
- [ ] External command execution uses safe argument passing
- [ ] User/file inputs are validated
- [ ] No new `unwrap()` in library code
- [ ] No path traversal vulnerabilities

### Step 3: Code Quality Review (P3)

For each changed source file:
- [ ] Follows project conventions in CLAUDE.md
- [ ] Error handling uses `thiserror`/`anyhow` correctly
- [ ] No unnecessary `.clone()` or allocations
- [ ] New public APIs have doc comments
- [ ] Naming is consistent with codebase
- [ ] No dead code or unused imports

### Step 4: Test Coverage Review (P4)

- [ ] New functionality has corresponding tests
- [ ] Edge cases are tested
- [ ] Existing tests still pass (no regressions)
- [ ] Test names are descriptive
- [ ] No `#[ignore]` without justification

### Output Format

```markdown
## PR Review: {branch_name or PR#}
**Files changed:** N
**Lines added/removed:** +X / -Y

### Security: PASS / WARN / FAIL
- ...

### Code Quality: PASS / WARN / FAIL
- ...

### Test Coverage: PASS / WARN / FAIL
- ...

### Verdict: APPROVE / REQUEST CHANGES / COMMENT

### Required Changes
1. ...

### Suggestions (non-blocking)
1. ...
```
