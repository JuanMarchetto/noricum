# Developer Experience & Onboarding Evaluation Prompt

Evaluates the new contributor experience. Combines P7 (DX) and P8 (OSS).

## USAGE

```bash
# Full DX evaluation
claude -p "$(cat reviews/prompts/dx-onboarding.md)" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text

# Focus on CLI experience
claude -p "$(cat reviews/prompts/dx-onboarding.md) FOCUS=cli" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)'
```

## PROMPT

You are a new developer encountering the Noricum project for the first time. Simulate the onboarding experience and evaluate documentation, tooling, and developer ergonomics.

### Step 1: First Impressions (README)

Read `README.md` and evaluate:
- [ ] Project purpose is clear within 10 seconds
- [ ] Value proposition is compelling
- [ ] Quick start section exists and is <5 steps
- [ ] Prerequisites are listed
- [ ] Architecture overview is visual or clearly structured
- [ ] Badges show project health (CI, license, version)
- [ ] Screenshots or demo output included

### Step 2: Setup Experience

Simulate the setup process:
- [ ] Clone instructions work
- [ ] Dependencies are documented
- [ ] `cargo build` succeeds without undocumented steps
- [ ] `.env` setup is documented
- [ ] `cargo test` passes out of the box (or failures are explained)
- [ ] Docker alternative exists for complex setup

### Step 3: CLI Experience (P7)

- Read CLI source (`crates/noricum-cli/src/`)
- [ ] `--help` is comprehensive and well-organized
- [ ] Subcommands are intuitive (migrate, analyze, doctor, bench)
- [ ] Error messages tell you what to do, not just what failed
- [ ] `doctor` command validates setup
- [ ] Output formats are useful (JSON, human-readable, HTML)
- [ ] Progressive disclosure (simple defaults, advanced flags available)

### Step 4: Contributor Experience (P8)

- [ ] CONTRIBUTING.md exists with clear guidelines
- [ ] CODE_OF_CONDUCT.md exists
- [ ] Issue templates guide bug reports and feature requests
- [ ] CLAUDE.md provides developer conventions
- [ ] Code structure is navigable (clear crate boundaries)
- [ ] Tests are easy to run and understand
- [ ] License is clear and permissive

### Step 5: Documentation Quality

- [ ] Inline doc comments on public APIs
- [ ] `cargo doc` produces clean output
- [ ] Architecture decisions are documented
- [ ] Example usage for common tasks
- [ ] Troubleshooting section exists

### Step 6: Time to First Migration

Estimate: Starting from `git clone`, how long to complete first migration?
- <5 min: Excellent
- 5-15 min: Good
- 15-30 min: Acceptable
- >30 min: Needs improvement

### Output Format

```markdown
## Developer Experience Evaluation
**Date:** YYYY-MM-DD

### First Impressions: X/10
**Findings:** ...

### Setup Experience: X/10
**Findings:** ...

### CLI Experience: X/10
**Findings:** ...

### Contributor Experience: X/10
**Findings:** ...

### Documentation Quality: X/10
**Findings:** ...

### Time to First Migration: ~X minutes
**Bottlenecks:** ...

### Overall DX Score: X/10

### Quick Wins (improve DX with minimal effort)
1. ...

### Strategic Improvements
1. ...
```
