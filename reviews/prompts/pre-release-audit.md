# Pre-Release Audit Prompt

Full gate check before git tag or crate publish. Combines P1 (Architecture), P2 (Security), P4 (QA), P6 (DevOps).

## USAGE

```bash
# Run pre-release audit for a specific version
claude -p "$(cat reviews/prompts/pre-release-audit.md) VERSION=0.2.0" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text

# Minimal invocation
claude -p "$(cat reviews/prompts/pre-release-audit.md)" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)'
```

## PROMPT

You are performing a pre-release audit of the Noricum project. This is a go/no-go gate check before tagging a release. Be strict — a false pass here means shipping bugs to users.

### Step 1: Collect Metrics

Run these checks and record results:
- `cargo check --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cargo fmt --all -- --check`
- Search for `unwrap()` in library code
- Search for `unsafe` blocks
- Search for `TODO`, `FIXME`, `HACK` in source
- Verify `Cargo.lock` is committed
- Check all `version` fields in `crates/*/Cargo.toml` are consistent

### Step 2: Architecture Gate (P1)

- [ ] All crates compile independently
- [ ] No circular dependencies
- [ ] State machine transitions are tested
- [ ] Public API surface is intentional (no accidental `pub`)
- [ ] Breaking changes are documented

### Step 3: Security Gate (P2)

- [ ] No secrets in source code or git history
- [ ] `.env` in `.gitignore`
- [ ] All external process execution uses safe argument passing
- [ ] LLM API calls have timeout and budget limits
- [ ] MCP server validates inputs
- [ ] `cargo audit` passes (if installed)

### Step 4: QA Gate (P4)

- [ ] All tests pass
- [ ] No `#[ignore]` tests without documented reason
- [ ] Integration tests cover primary use cases
- [ ] Diff test passes on at least 3 fixture files
- [ ] Error paths have test coverage

### Step 5: DevOps Gate (P6)

- [ ] CI passes on main branch
- [ ] Docker build succeeds
- [ ] Version numbers are consistent across Cargo.toml files
- [ ] CHANGELOG is updated (if exists)
- [ ] README reflects current functionality

### Output Format

```markdown
## Pre-Release Audit: Noricum vX.Y.Z
**Date:** YYYY-MM-DD
**Verdict:** PASS / FAIL / CONDITIONAL PASS

### Gate Results
| Gate | Status | Blocking Issues |
|------|--------|-----------------|
| Architecture | PASS/FAIL | ... |
| Security | PASS/FAIL | ... |
| QA | PASS/FAIL | ... |
| DevOps | PASS/FAIL | ... |

### Blocking Issues (must fix before release)
1. ...

### Conditional Items (acceptable with documented risk)
1. ...

### Release Recommendation
SHIP / DO NOT SHIP / SHIP WITH CONDITIONS
```
