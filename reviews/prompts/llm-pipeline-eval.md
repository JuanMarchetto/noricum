# LLM Pipeline Evaluation Prompt

Evaluates LLM agent quality after prompt or model changes. Combines P5 (AI/ML), P4 (Testing), P3 (Output Quality).

## USAGE

```bash
# Evaluate the full LLM pipeline
claude -p "$(cat reviews/prompts/llm-pipeline-eval.md)" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)' \
    --output-format text

# Evaluate after a specific prompt change
claude -p "$(cat reviews/prompts/llm-pipeline-eval.md) FOCUS=prompts/translation.md" \
    --allowedTools 'Read,Grep,Glob,Bash(read-only)'
```

## PROMPT

You are evaluating the LLM pipeline quality of the Noricum C-to-Rust migration agent. Focus on prompt engineering, output quality, cost efficiency, and reliability.

### Step 1: Review LLM Infrastructure

Read and evaluate:
- `crates/noricum-agents/src/` — all agent implementations
- `crates/noricum-agents/src/providers.rs` — model configuration
- `prompts/*.md` — all prompt templates
- `patterns/*.toml` — RAG pattern store
- `crates/noricum-core/src/orchestrator.rs` — pipeline integration
- `crates/noricum-validation/src/lib.rs` — output validation

### Step 2: Prompt Quality (P5)

For each prompt in `prompts/`:
- [ ] Clear role definition and task description
- [ ] Structured output format specified
- [ ] Examples included (few-shot)
- [ ] Constraints and boundaries defined
- [ ] Edge cases addressed in instructions
- [ ] No prompt injection vulnerabilities from untrusted input
- **Score each prompt 1-10**

### Step 3: Agent Architecture (P5)

- [ ] Agent selection logic is appropriate per task
- [ ] Model routing respects difficulty levels
- [ ] Temperature settings match task requirements
- [ ] Retry/fallback strategy is implemented
- [ ] Token limits are configured
- [ ] Cost tracking is in place

### Step 4: Output Quality (P3)

- [ ] Generated Rust code compiles
- [ ] Generated code passes idiomatic scoring
- [ ] Repair loop improves code quality (not random changes)
- [ ] Test generation produces meaningful tests

### Step 5: Evaluation Readiness (P5)

- [ ] Metrics are defined (compilation rate, idiomatic score, diff-test pass rate)
- [ ] Baselines exist for comparison
- [ ] CRUST-Bench or equivalent harness exists
- [ ] Results are reproducible

### Output Format

```markdown
## LLM Pipeline Evaluation
**Date:** YYYY-MM-DD

### Prompt Scores
| Prompt | Score | Issues |
|--------|-------|--------|
| analysis.md | X/10 | ... |
| translation.md | X/10 | ... |
| repair.md | X/10 | ... |

### Agent Architecture: X/10
**Findings:** ...

### Output Quality: X/10
**Findings:** ...

### Evaluation Readiness: X/10
**Findings:** ...

### Overall LLM Pipeline Score: X/10

### Critical Improvements
1. ...
```
