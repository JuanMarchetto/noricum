# Multi-Stakeholder Review Framework for Noricum

> **Purpose:** Comprehensive, repeatable assessment of the Noricum C-to-Rust migration agent from 8 professional perspectives. This document serves as both a human-readable review framework and an executable prompt for Claude Code.

---

## Instructions for Automated Execution

When run as a prompt, Claude Code should:
1. Read the project metrics provided via stdin or environment
2. Read the key source files listed in the Data Sources section
3. Evaluate each of the 8 stakeholder perspectives
4. Produce a scored report with blocking issues, high-priority items, and nice-to-haves
5. Calculate the weighted overall grade

---

## Scoring System

Each perspective produces:
- **Score: 1-10** (10 = world-class, 7 = production-ready, 5 = acceptable, <5 = blocking)
- **Blocking issues** — must fix before any release
- **High-priority** — fix within 1 sprint
- **Nice-to-have** — backlog

### Weighted Overall Grade

| Perspective | Weight |
|-------------|--------|
| P2: Security | 20% |
| P1: Architecture | 15% |
| P3: Rust Quality | 15% |
| P4: Testing | 15% |
| P5: LLM/AI | 15% |
| P6: DevOps | 8% |
| P7: DX | 7% |
| P8: OSS | 5% |

### Grade Scale

| Grade | Range | Meaning |
|-------|-------|---------|
| A+ | 9.0-10.0 | Exceptional, reference implementation |
| A | 8.0-8.9 | Production-ready, minor polish needed |
| B | 7.0-7.9 | Solid, some gaps to address |
| C | 6.0-6.9 | Functional, significant improvements needed |
| D | <6.0 | Not ready for production |

---

## Data Sources

### Files to Read

| File | Used By |
|------|---------|
| `CLAUDE.md` | P1, P7 |
| `README.md` | P7, P8 |
| `SECURITY.md` | P2 |
| `EVALUATION.md` | P1, P5 |
| `Cargo.toml` (workspace) | P1, P3, P6 |
| `crates/*/Cargo.toml` | P3, P6 |
| `.github/workflows/ci.yml` | P4, P6 |
| `Dockerfile` | P6 |
| `crates/noricum-cli/src/main.rs` | P7 |
| `crates/noricum-cli/src/api.rs` | P2, P7 |
| `crates/noricum-tools/src/rig_tools.rs` | P2 |
| `crates/noricum-core/src/orchestrator.rs` | P1, P3 |
| `crates/noricum-validation/src/lib.rs` | P4, P5 |
| `crates/noricum-agents/src/providers.rs` | P5 |
| `crates/noricum-mcp/src/server.rs` | P2, P7 |
| `prompts/*.md` | P5 |

### Automated Metrics (provided via `run-review.sh`)

The following metrics are collected automatically and injected into the review:
- Compilation status (`cargo check`)
- Test results (`cargo test`)
- Clippy warnings (`cargo clippy -- -D warnings`)
- Format check (`cargo fmt --check`)
- Total LOC, test count, dependency count
- `unwrap()` usage in production code
- `unsafe` usage in production code
- Recent git history

---

## P1: CTO / VP Engineering

**Role context:** You are evaluating whether to adopt this project for your engineering org. You care about architecture coherence, maintainability, team velocity, and technical debt.

### Checklist

- [ ] Architecture is documented and matches implementation
- [ ] Crate boundaries are well-defined with minimal circular dependencies
- [ ] State machine is clearly defined with valid transitions
- [ ] Error handling is consistent across crates
- [ ] Dependency versions are current and actively maintained
- [ ] No deprecated or abandoned dependencies
- [ ] License compatibility across all dependencies
- [ ] Configuration management is centralized and documented
- [ ] Build times are reasonable (<2 min clean, <30s incremental)
- [ ] Code follows consistent naming and style conventions
- [ ] Onboarding documentation exists (CLAUDE.md, README, inline docs)
- [ ] Technical debt is tracked and manageable

### Rubric

> Would you approve this for production use at a Series B startup?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## P2: Principal Security Engineer

**Role context:** You are performing a security review before deployment. This system executes external commands (compilers, c2rust), handles LLM API keys, and processes untrusted C source code.

### Checklist

- [ ] No hardcoded secrets, API keys, or credentials in source
- [ ] `.env` file is in `.gitignore`
- [ ] API keys are loaded securely (env vars or config, not CLI args visible in `ps`)
- [ ] Command injection prevention: all shell arguments are sanitized/escaped
- [ ] Path traversal prevention: file paths are validated and sandboxed
- [ ] LLM prompt injection: C source code cannot manipulate agent behavior
- [ ] LLM output validation: generated Rust code is compiled, not blindly trusted
- [ ] No SSRF vectors (URL construction from user input)
- [ ] Dependency audit: no known CVEs in dependency tree
- [ ] `Cargo.lock` is committed and integrity is verifiable
- [ ] Docker image runs as non-root user
- [ ] Temporary files are created securely and cleaned up
- [ ] Rate limiting / budget controls on LLM API calls
- [ ] Audit logging for security-relevant operations
- [ ] MCP server validates all inputs

### Rubric

> Would you sign off on a SOC 2 Type II audit for this system?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## P3: Staff Rust Engineer

**Role context:** You are reviewing this as a senior Rust engineer focused on idiomatic code, safety, performance, and API design.

### Checklist

- [ ] Error types use `thiserror` in libraries, `anyhow` in binaries
- [ ] No `unwrap()` in library code (only in tests)
- [ ] Zero `unsafe` blocks (or each one is justified and documented)
- [ ] Ownership and borrowing are used correctly (no unnecessary cloning)
- [ ] `String` vs `&str` usage is appropriate at API boundaries
- [ ] Traits are used for abstraction where appropriate
- [ ] Enums model state correctly (impossible states are unrepresentable)
- [ ] `clippy::pedantic` passes (or violations are justified)
- [ ] Async code avoids blocking the runtime
- [ ] No unnecessary allocations in hot paths
- [ ] Public API surface is minimal and well-documented
- [ ] Type conversions use `From`/`Into` traits
- [ ] Iterator chains preferred over manual loops where clearer
- [ ] `cfg(test)` modules don't leak into production builds

### Rubric

> Would you approve this PR in a Rust-focused team?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## P4: QA / Test Engineering Lead

**Role context:** You are assessing test coverage, test quality, and confidence that the test suite catches regressions.

### Checklist

- [ ] Unit tests exist for each crate's core logic
- [ ] Integration tests cover end-to-end migration flows
- [ ] Golden/snapshot tests for deterministic outputs (rule-based translation)
- [ ] Edge cases covered: empty input, massive files, unicode, malformed C
- [ ] Tests are deterministic (no flaky tests, no shared mutable state)
- [ ] Tests run in CI on every push/PR
- [ ] Test timeout handling exists
- [ ] Differential testing: C output vs Rust output comparison
- [ ] LLM-dependent tests are isolated or mockable
- [ ] Error paths are tested (not just happy paths)
- [ ] Test naming follows conventions and is descriptive
- [ ] No `#[ignore]` tests without justification
- [ ] Property-based or fuzz testing for parsers/validators
- [ ] Coverage metrics are tracked

### Rubric

> Would you ship this with confidence the test suite catches regressions?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## P5: ML/AI Engineering Lead

**Role context:** You are evaluating the LLM integration quality, cost efficiency, and AI pipeline maturity.

### Checklist

- [ ] Prompts are version-controlled and separated from code
- [ ] Prompt engineering follows best practices (clear instructions, examples, constraints)
- [ ] LLM output is parsed and validated before use
- [ ] Fallback strategy exists when LLM fails or returns garbage
- [ ] Token usage is tracked and logged
- [ ] Budget/cost controls are implemented
- [ ] Model routing is configurable (Claude vs Ollama vs others)
- [ ] Temperature and sampling parameters are tuned per task
- [ ] Repair loop has convergence guarantees (max iterations, backoff)
- [ ] RAG pattern store is relevant and retrievable
- [ ] Evaluation framework exists (metrics, baselines, benchmarks)
- [ ] CRUST-Bench or equivalent benchmark readiness
- [ ] LLM responses are not cached without invalidation strategy
- [ ] Prompt injection defenses exist for untrusted input

### Rubric

> Would you present these LLM metrics to investors?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## P6: DevOps / SRE Lead

**Role context:** You are evaluating operational readiness: build reproducibility, observability, deployment, and incident response.

### Checklist

- [ ] Docker build is reproducible and multi-stage
- [ ] Docker image size is reasonable (<500MB)
- [ ] CI pipeline exists with matrix testing
- [ ] CI caching is effective (dependencies, build artifacts)
- [ ] Structured logging with `tracing` (not println/eprintln)
- [ ] Log levels are appropriate (debug vs info vs warn vs error)
- [ ] Configuration is 12-factor compliant (env vars, config files)
- [ ] Secrets are not logged or exposed in error messages
- [ ] Health checks exist for long-running services (MCP server)
- [ ] Graceful shutdown handling
- [ ] Resource limits are defined (memory, CPU, timeout)
- [ ] Error classification enables alerting
- [ ] Rollback strategy exists
- [ ] Artifact versioning (cargo version, git tags)

### Rubric

> Would you run this in production with a pager?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## P7: Developer Experience (DX) Lead

**Role context:** You are evaluating how quickly a new developer can become productive with this project.

### Checklist

- [ ] README has clear quick-start (clone to first run in <5 min)
- [ ] CLI help text is comprehensive and follows conventions
- [ ] Error messages are actionable (tell user what to do, not just what failed)
- [ ] `--help` on all commands and subcommands
- [ ] Progressive disclosure (simple defaults, advanced options available)
- [ ] MCP integration is well-documented with tool descriptions
- [ ] CLAUDE.md provides contributor guidance
- [ ] Inline doc comments on all public APIs
- [ ] Example usage in documentation
- [ ] `cargo doc` generates clean documentation
- [ ] Output formats are useful (JSON, human-readable, HTML reports)
- [ ] Doctor/diagnostic command exists for setup validation
- [ ] Consistent CLI flag naming across subcommands

### Rubric

> Would a new engineer be productive in 1 day?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## P8: Open Source Community Manager

**Role context:** You are evaluating this project's readiness for public launch and community growth.

### Checklist

- [ ] README has badges (CI status, license, crates.io, docs.rs)
- [ ] README has architecture diagram or visual overview
- [ ] Quick start section works for first-time users
- [ ] CONTRIBUTING.md exists with guidelines
- [ ] CODE_OF_CONDUCT.md exists
- [ ] LICENSE file is present and clear (MIT/Apache-2.0)
- [ ] Issue templates exist for bugs, features, questions
- [ ] PR template exists
- [ ] CHANGELOG.md tracks releases
- [ ] Semantic versioning is followed
- [ ] Release process is documented
- [ ] Comparable projects are acknowledged (positioning statement)
- [ ] Project has a clear value proposition in README
- [ ] Social proof / demo / screenshots available

### Rubric

> Would this project attract contributors on day 1 of public launch?

**Score: ___/10**

### Findings

*(Auto-filled during review execution)*

### Verdict

*(Blocking / High-priority / Nice-to-have items)*

---

## Overall Assessment

### Score Summary

| # | Perspective | Score | Weight | Weighted |
|---|-------------|-------|--------|----------|
| P1 | Architecture (CTO) | /10 | 15% | |
| P2 | Security | /10 | 20% | |
| P3 | Rust Quality | /10 | 15% | |
| P4 | Testing | /10 | 15% | |
| P5 | LLM/AI | /10 | 15% | |
| P6 | DevOps | /10 | 8% | |
| P7 | DX | /10 | 7% | |
| P8 | OSS | /10 | 5% | |
| | **Overall** | | | **/10** |

### Grade: ___

### Top 5 Blocking Issues

1.
2.
3.
4.
5.

### Top 5 High-Priority Improvements

1.
2.
3.
4.
5.

### Executive Summary

*(2-3 paragraph overall assessment)*
