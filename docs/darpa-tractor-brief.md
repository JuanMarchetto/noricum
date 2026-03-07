# Noricum: Autonomous C-to-Rust Migration with Verified Behavioral Equivalence

**Technical Brief for DARPA TRACTOR Program Alignment**

---

## Problem Statement

Memory safety vulnerabilities account for approximately 70% of all CVEs in systems software (Microsoft, Google Chrome security data). The U.S. government — through the NSA, White House ONCD, and DARPA's TRACTOR program — has identified automated C/C++ to memory-safe language migration as a national security priority.

Existing tools present a false choice:

- **Mechanical transpilers** (C2Rust) produce syntactically valid Rust wrapped entirely in `unsafe` blocks, preserving all original memory safety risks
- **Manual rewriting** is prohibitively expensive at scale (estimated $1-10 per LOC for security-critical code)
- **LLM-based translation** without verification produces plausible-looking code with no behavioral guarantees

None of these approaches satisfy TRACTOR's core requirement: **automated translation with verified behavioral equivalence**.

## Solution: Noricum

Noricum is an autonomous agent that migrates C source code to safe, idiomatic Rust through a 9-stage pipeline combining LLM intelligence with deterministic verification.

### Architecture

```
C Source → Extraction → Difficulty Classification → C2Rust Baseline
    → LLM Analysis → LLM Translation → Validation → Repair Loop → Verified Rust
```

**Key technical components:**

1. **Difficulty-aware model routing**: AST-based classifier routes simple functions (arithmetic, string ops) to fast models and complex functions (pointer arithmetic, recursive data structures, manual memory management) to capable models.

2. **RAG pattern store**: A retrieval-augmented generation layer provides the LLM with proven migration patterns (malloc→Vec, linked-list→Vec, error-codes→Result, tagged-union→enum) from previously successful migrations.

3. **Byte-exact differential testing**: Both the original C and generated Rust are compiled and executed with identical inputs. Outputs are compared byte-by-byte, including exit codes. Float-tolerant comparison is available for numerical code.

4. **Automated repair loop**: When differential tests fail, compiler errors and output mismatches are fed back to the LLM for up to 5 repair iterations. Each iteration runs the full validation suite.

5. **Idiomatic scoring**: A 0-100 scoring system evaluates output quality based on unsafe block count, clippy warnings, positive Rust patterns (Option, Result, iterators, impl blocks), and negative patterns (unwrap, raw casts, manual indexing).

### Verification Approach

Noricum's verification directly aligns with TRACTOR's emphasis on behavioral equivalence:

| Verification Layer | Method |
|-------------------|--------|
| **Compilation** | `rustc` type checking — catches type mismatches, borrow violations |
| **Static analysis** | `clippy` — catches common bugs, style issues, potential UB |
| **Behavioral equivalence** | Differential testing — byte-exact output comparison |
| **Safety** | Unsafe block counting — target is 0 |
| **Quality** | Idiomatic scoring — measures Rust-native pattern usage |

This multi-layer approach provides confidence that migrations are not just syntactically valid, but semantically correct and memory-safe.

## Demonstrated Results

Noricum has been validated on 14 C source files of increasing complexity:

| Source | LOC | Functions | Unsafe Blocks | Diff Test | Score |
|--------|-----|-----------|---------------|-----------|-------|
| Simple fixtures (10 files) | 13-50 | 1-4 | 0 | PASS | 92-100 |
| `hash_table.c` | 204 | 8 | 0 | PASS | 89 |
| `cjson_combined.c` | 520 | 12 | 0 | PASS | 100 |
| `expr_eval.c` | 1,686 | 74 | 0 | PASS | 100 |

**Aggregate metrics:**
- 14/14 files successfully migrated (100% success rate)
- 0 unsafe blocks across all outputs
- 100% differential test pass rate
- Average idiomatic score: 96/100
- Average repair iterations: <0.1 (most files correct on first attempt)

### Notable Migration: expr_eval.c

The largest validated migration is a 1,686-line expression evaluator featuring:
- Recursive descent parser with full operator precedence
- 25+ built-in functions
- Tagged union value types (C struct → Rust enum)
- Manual memory management (malloc/realloc/free → Vec/String/RAII)
- Hash map with separate chaining (manual → std::collections::HashMap)
- Statistical dataset operations (mean, variance, stddev, median)

Result: 1,686 LOC C → 1,446 LOC Rust, score 100/100, 0 unsafe, 0 repairs, byte-exact diff test pass.

## Technical Differentiation

| Capability | C2Rust | Academic LLM Tools | Noricum |
|------------|--------|-------------------|---------|
| Translation method | AST lowering | LLM (no verification) | LLM + verification pipeline |
| Output safety | All unsafe | Variable | 0 unsafe (verified) |
| Behavioral verification | None | Paper only | Byte-exact differential testing |
| Automated repair | None | None | 5-iteration repair loop |
| Difficulty routing | N/A | N/A | AST-based classification |
| Pattern learning | N/A | N/A | RAG store from past migrations |
| Production readiness | CLI only | Research prototype | CLI + REST API + MCP server |

## TRACTOR Alignment

Noricum addresses several TRACTOR technical areas:

1. **Automated translation**: Full pipeline from C source to compiled Rust binary
2. **Behavioral equivalence**: Differential testing proves input/output equivalence
3. **Safety guarantees**: Zero unsafe blocks with static verification
4. **Scalability pathway**: Dependency-aware multi-file migration with topological ordering
5. **Human-in-the-loop**: IDE integration via MCP server enables developer oversight

## Current Limitations and Roadmap

- **File size ceiling**: Validated up to 1,686 LOC; multi-file projects require further work
- **Preprocessor macros**: Complex `#ifdef` chains and macro-heavy code not yet handled
- **External dependencies**: Self-contained files only; linking against C libraries requires manual intervention
- **C++ support**: Current focus is C; C++ class hierarchies and templates are future work

## Team and Contact

**Juan Patricio Marchetto** — Creator and lead developer. Senior software developer, creator of SODA (Solana code generation tool, 118 GitHub stars), Web3 Builders Alliance member.

- GitHub: github.com/JuanMarchetto/noricum
- License: MIT (open source)

---

*Noricum is an open-source project available for evaluation, collaboration, and integration with TRACTOR performer teams.*
