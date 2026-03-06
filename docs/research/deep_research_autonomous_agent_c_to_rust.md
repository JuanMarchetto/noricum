# Deep Research: Autonomous Agent for C/C++ to Rust Migration

**Date:** March 2026
**Status:** Feasibility Analysis

---

## 1. Market Context: Why Now

The landscape in March 2026 is extraordinarily favorable for this project:

- **DARPA TRACTOR** (Translating All C to Rust): $5M+ program with teams from Illinois, Wisconsin-Madison, UC Berkeley, and Edinburgh. Demonstrates that the US government considers this a national security priority.
- **Microsoft** has announced the goal of eliminating all C/C++ by 2030, with their internal "Rustify" engine and the target of "1 engineer, 1 month, 1 million lines." Over 1.2 billion lines already converted to Rust prototypes internally.
- **Anthropic** demonstrated that 16 Claude Opus 4 agents built a complete C compiler in Rust (100,000 lines) for ~$20,000 in API costs.
- **CRUST-Bench** (2025) shows that the best LLMs achieve 32-48% successful transpilation with repair loops, confirming that AI alone is not enough — a hybrid pipeline is needed.

**Conclusion:** There is massive demand, available funding, and the technology is mature but incomplete. A specialized autonomous agent has a clear gap to fill.

---

## 2. Analysis of Suggested Resources

### 2.1 Noricum (this project)

**Status:** Design phase, 5 architecture documents, no code yet.

**Value for the autonomous agent:**
- Well-defined pipeline architecture: Parser -> Analyzer -> Transformer -> Generator -> Validator
- Language-agnostic IR (Intermediate Representation) already designed
- Plugin system for transformation rules
- Initial targets defined (miniz, libsodium)

**Recommendation:** Noricum should be the **deterministic core** of the agent. AI doesn't replace the pipeline — it enhances it in steps where static analysis is insufficient (ownership inference, idiomaticity, edge cases).

### 2.2 C2Rust

**Status:** Mature tool by Immunant/Galois. Generates compilable `unsafe` Rust from C99.

**Value for the agent:**
- Use as the **first step** of the pipeline: C -> unsafe Rust (mechanical, deterministic)
- Its output is the ideal input for the agent: unsafe Rust that must be refined to safe, idiomatic Rust
- Recent research (2025-2026) confirms the hybrid approach: C2Rust generates, LLM refines

**Recommendation:** Integrate C2Rust as the "step zero" of the pipeline. Don't reinvent mechanical translation.

### 2.3 Ollama (Local LLMs)

**Status:** Mature platform for running LLMs locally. Supports function calling, tool use, and OpenAI-compatible API.

**Relevant models for code (runnable locally):**

| Model | Parameters | VRAM Required | Strength |
|-------|-----------|---------------|----------|
| Qwen3-Coder | 480B (35B active, MoE) | ~24GB Q4 | Code agents, 256K context, 100+ languages |
| DeepSeek-V3 | 671B (37B active, MoE) | ~24GB Q4 | Code reasoning, tool calling |
| DeepSeek-R1 | 671B (37B active) | ~24GB Q4 | Long reasoning, 93% on SACTOR |
| Codestral | 22B | ~14GB | Fast code generation, Mistral |
| Qwen3-32B | 32B | ~20GB | Performance/resource balance |

**Recommendation:** Ollama is ideal for:
- Agent development and testing without API costs
- Offline/private execution (client's proprietary code)
- Rapid iteration with smaller models (Qwen3-32B, Codestral)
- Production with larger models (Qwen3-Coder, DeepSeek-R1)

---

## 3. Paid Models: When They're Worth It

### 3.1 OpenAI

**GPT-5.2-Codex** (via API or Codex CLI):
- Optimized for "agentic coding" including large migrations and refactors
- Native support for worktrees and parallelism
- 1M+ weekly active developers
- **Cost:** ~$15/M input tokens, ~$60/M output tokens (o3-level)
- **When to use:** Production migrations where precision justifies the cost

**o3/o4-mini** (reasoning):
- 22% on CRUST-Bench one-shot, 48% with repair loop
- Excellent for ownership inference and lifetime analysis
- **Cost:** Variable based on thinking tokens

### 3.2 Anthropic Claude

**Claude Opus 4.6:**
- Demonstrated building a C compiler in Rust (100K lines)
- Agent Teams: multiple coordinated agents with mailbox system
- Claude Code as CLI agent with native tool use
- **Cost:** ~$15/M input, ~$75/M output
- **When to use:** Complex reasoning tasks (C++ templates, ownership)

**Claude Sonnet 4.6:**
- Cost/performance balance for rapid iterations
- **Cost:** ~$3/M input, ~$15/M output
- **When to use:** Repair loops, validation, iterative refactoring

### 3.3 Hybrid Model Strategy (Recommended)

```
Phase 1 - Mechanical translation:    C2Rust (free, deterministic)
Phase 2 - Ownership analysis:        Ollama/DeepSeek-R1 or Claude Opus (reasoning)
Phase 3 - Idiomatic refinement:      Ollama/Qwen3-Coder or GPT-5.2-Codex (volume)
Phase 4 - Repair loop:               Economical model (Sonnet, Qwen3-32B)
Phase 5 - Final validation:          Strong model (Opus, o3) only for failures
```

**Estimated cost per 10,000 lines of C:**
- Ollama local only: $0 (own hardware)
- Hybrid (Ollama + API for hard cases): $5-15
- Premium API only: $50-200

---

## 4. Autonomous Agent Architecture

### 4.1 General Design

```
                    +---------------------------+
                    |    Main Orchestrator       |
                    |  (Rust - Noricum core)     |
                    +------+----------+---------+
                           |          |
              +------------+          +-------------+
              |                                     |
    +---------v---------+              +------------v-----------+
    |  Deterministic     |              |  Intelligence Layer    |
    |  Pipeline          |              |  (LLM Agent Layer)     |
    |                    |              |                        |
    | 1. C2Rust          |              | - Ownership Inference  |
    | 2. AST Extractor   |              | - Idiom Refactoring    |
    | 3. IR Builder      |              | - Error Recovery       |
    | 4. Type Mapper     |              | - Test Generation      |
    | 5. Test Runner     |              |                        |
    +---------+----------+              +------------+-----------+
              |                                      |
              +----------------+---------------------+
                               |
                    +----------v-----------+
                    |   Validation          |
                    |                       |
                    | - Compilation         |
                    | - Differential tests  |
                    | - Clippy/rustfmt      |
                    | - Coverage            |
                    | - Idiomatic score     |
                    +-----------------------+
```

### 4.2 Agent Components

#### A. Orchestrator (Rust)
- Coordinates the full pipeline
- Manages migration state per function/module
- Decides when to escalate from local model to paid API
- Implements retry logic and repair loops

#### B. Analysis Agent (LLM)
- Analyzes C code to infer ownership and lifetimes
- Detects patterns: buffer management, error codes, state machines
- Suggests optimal mapping to Rust idioms
- **Recommended model:** DeepSeek-R1 (local) or Claude Opus (API)

#### C. Translation Agent (LLM)
- Refines C2Rust output from unsafe to safe
- Applies idioms: slices, iterators, Result<T,E>, Option<T>
- **Recommended model:** Qwen3-Coder (local) or GPT-5.2-Codex (API)

#### D. Repair Agent (LLM)
- Receives compilation/test errors
- Proposes fixes iteratively (max N attempts)
- **Recommended model:** Qwen3-32B (local, fast) or Sonnet (API, economical)

#### E. Validation Agent (deterministic + LLM)
- Runs differential tests (C vs Rust)
- Measures coverage and idiomatic score
- LLM only for complex failure analysis

### 4.3 Per-Function Workflow

```
For each function F in the C codebase:

1. EXTRACT
   - Clang AST -> IR (deterministic)
   - Dependencies, types, signatures

2. CHARACTERIZE
   - Generate golden tests from C original
   - Generate fuzz seeds
   - Capture I/O vectors

3. TRANSLATE (step 1 - mechanical)
   - C2Rust: C -> unsafe Rust
   - Verify compilation

4. ANALYZE (LLM - analysis agent)
   - Infer ownership model
   - Detect memory patterns
   - Classify: easy/medium/hard

5. REFINE (LLM - translation agent)
   - Unsafe -> Safe Rust
   - Apply idioms
   - If "easy": local model
   - If "hard": premium model

6. VALIDATE
   - Compile
   - Run differential tests
   - Measure: compiles? tests pass? clippy? coverage?

7. REPAIR (if fails, max 5 attempts)
   - LLM receives error + context
   - Proposes fix
   - Return to step 6

8. REPORT
   - Idiomatic score (0-100)
   - Remaining unsafe blocks
   - Clippy warnings
   - Test coverage
```

---

## 5. Rig.rs: Deep Dive — The LLM Agent Framework for Noricum

After evaluating the full documentation at [docs.rig.rs](https://docs.rig.rs/), rig-rs is the **recommended foundation** for Noricum's LLM agent layer. Here is a detailed analysis of why and how.

### 5.1 What Rig Provides

Rig (v0.31.0) is a Rust-native library for building LLM-powered applications. It is NOT a Python wrapper — it's idiomatic Rust with async/await (Tokio), type-safe interactions, and trait-based extensibility.

**Core abstractions:**

| Abstraction | Purpose | Relevance to Noricum |
|-------------|---------|---------------------|
| `CompletionModel` trait | Unified interface to any LLM | Switch between Ollama/OpenAI/Claude seamlessly |
| `Agent` struct | LLM + preamble + tools + context | Each migration sub-agent (analysis, translation, repair) |
| `Tool` trait | Function calling with typed args | Compile checker, test runner, clippy scorer, AST extractor |
| `Pipeline` (Op trait) | DAG of composable operations | The entire migration pipeline as a directed graph |
| `VectorStoreIndex` trait | RAG over document collections | Index of C patterns, Rust idioms, past successful translations |
| `Extractor` | Structured data extraction from LLM output | Parse ownership models, error classifications from LLM responses |

### 5.2 Provider Support (Critical for Hybrid Strategy)

Rig natively supports **20+ providers** through a single API:

- **Ollama** (local) — DeepSeek-R1, Qwen3-Coder, Codestral
- **OpenAI** — GPT-5.2-Codex, o3, o4-mini
- **Anthropic** — Claude Opus 4.6, Sonnet 4.6
- **DeepSeek** (direct API)
- **Groq** (fast inference)
- **Azure OpenAI** (enterprise)
- **Mistral**, **Cohere**, **Gemini**, **xAI**, and more

This means the **hybrid model routing** (local for volume, paid for precision) requires zero code changes — just swap the provider client.

### 5.3 Tool System — Mapping to Noricum

Rig's `Tool` trait maps perfectly to Noricum's pipeline components:

```rust
// Example: Compile-check tool for the repair agent
pub struct CompileChecker;

#[derive(Deserialize, JsonSchema)]
pub struct CompileArgs {
    /// The Rust source code to compile
    pub rust_code: String,
    /// The target crate directory
    pub crate_dir: PathBuf,
}

impl Tool for CompileChecker {
    const NAME: &'static str = "compile_check";
    type Args = CompileArgs;
    type Output = CompileResult;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "compile_check".into(),
            description: "Compile Rust code and return errors if any".into(),
            parameters: json_schema::<CompileArgs>(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Write code to file, run `cargo check`, parse errors
    }
}
```

**Noricum tools to implement:**

| Tool | Agent | Description |
|------|-------|-------------|
| `compile_check` | Repair Agent | Run `cargo check`, return structured errors |
| `run_tests` | Validation Agent | Execute differential tests (C vs Rust) |
| `clippy_score` | Validation Agent | Run clippy, compute idiomatic score |
| `ast_extract` | Analysis Agent | Extract Clang AST for a function |
| `c2rust_translate` | Translation Agent | Run C2Rust on a C file |
| `fuzz_generate` | Test Agent | Generate fuzz harness for a function |
| `read_source` | All Agents | Read C or Rust source file |
| `write_source` | Translation/Repair | Write generated Rust code |
| `git_diff` | Orchestrator | Show changes for review |

### 5.4 Pipeline as DAG — The Migration Flow

Rig's `Pipeline` module implements the `Op` trait with combinators that model the entire migration as a DAG:

```rust
use rig::pipeline::{self, Op};

// Conceptual pipeline for a single function migration
let migration_pipeline = pipeline::new()
    // Step 1: Extract AST (deterministic)
    .chain(|source_path| ast_extractor.extract(source_path))
    // Step 2: Generate golden tests (deterministic)
    .chain(|ir| test_generator.generate_golden(ir))
    // Step 3: C2Rust mechanical translation (deterministic)
    .chain(|ir| c2rust.translate(ir))
    // Step 4: LLM analysis (agent)
    .chain(|unsafe_rust| analysis_agent.prompt(format!(
        "Analyze this unsafe Rust and infer ownership: {unsafe_rust}"
    )))
    // Step 5: LLM refinement (agent)
    .chain(|analysis| translation_agent.prompt(format!(
        "Refine to safe idiomatic Rust: {analysis}"
    )))
    // Step 6: Validate (deterministic + agent for failures)
    .chain(|safe_rust| validator.validate(safe_rust));
```

The `parallel!` macro enables concurrent processing of independent functions:

```rust
// Process multiple functions simultaneously
let results = parallel!(
    migration_pipeline.call("func_a.c"),
    migration_pipeline.call("func_b.c"),
    migration_pipeline.call("func_c.c"),
);
```

### 5.5 Agent Design for Noricum

Each migration sub-agent maps to a Rig `Agent` with specific preamble, tools, and model:

```rust
// Analysis Agent — uses reasoning model
let analysis_agent = ollama_client
    .agent("deepseek-r1:70b")
    .preamble("You are a C-to-Rust migration expert. Analyze the following \
               unsafe Rust code (translated from C by C2Rust) and provide: \
               1. Ownership model for each pointer \
               2. Suggested lifetime annotations \
               3. Difficulty classification (easy/medium/hard) \
               4. Recommended Rust idioms to apply")
    .temperature(0.2)  // Low for deterministic analysis
    .tool(AstExtractor)
    .tool(ReadSource)
    .build();

// Translation Agent — uses code generation model
let translation_agent = ollama_client
    .agent("qwen3-coder:32b")
    .preamble("You are a Rust expert. Transform unsafe C-like Rust into \
               safe, idiomatic Rust. Use slices instead of raw pointers, \
               Result<T,E> instead of error codes, iterators instead of \
               manual loops. Preserve exact semantics.")
    .temperature(0.3)
    .tool(CompileChecker)
    .tool(WriteSource)
    .build();

// Repair Agent — uses fast economical model
let repair_agent = ollama_client
    .agent("qwen3:32b")
    .preamble("You are a Rust compiler error fixer. Given compilation errors \
               and the source code, provide the minimal fix. Do not change \
               the overall structure — only fix the specific error.")
    .temperature(0.1)
    .tool(CompileChecker)
    .tool(ReadSource)
    .tool(WriteSource)
    .multi_turn(5)  // Max 5 repair iterations
    .build();
```

### 5.6 Dynamic Model Routing

Rig's provider abstraction enables intelligent routing without code changes:

```rust
// Route to different models based on difficulty
async fn get_translation_agent(difficulty: Difficulty) -> impl Prompt {
    match difficulty {
        Difficulty::Easy => ollama_client
            .agent("qwen3:32b")
            .preamble(TRANSLATION_PREAMBLE)
            .build(),
        Difficulty::Medium => ollama_client
            .agent("deepseek-r1:70b")
            .preamble(TRANSLATION_PREAMBLE)
            .build(),
        Difficulty::Hard => anthropic_client
            .agent("claude-opus-4-6")
            .preamble(TRANSLATION_PREAMBLE)
            .build(),
    }
}
```

### 5.7 RAG for Pattern Matching

Rig's vector store integration enables learning from successful translations:

```rust
// Build a knowledge base of successful C->Rust patterns
let pattern_index = mongodb_client
    .vector_store::<TranslationPattern>("noricum_patterns")
    .await?;

// Agent with dynamic context from past translations
let contextual_agent = openai_client
    .agent("gpt-5.2-codex")
    .preamble(TRANSLATION_PREAMBLE)
    .dynamic_context(3, pattern_index)  // Retrieve 3 most relevant patterns
    .tool(CompileChecker)
    .build();
```

### 5.8 MCP Integration

Rig supports Model Context Protocol (MCP), enabling Noricum to expose its tools as an MCP server that any MCP-compatible client (Claude Code, Codex CLI, OpenClaw) can use:

```rust
// Expose Noricum tools as MCP server
let mcp_server = Server::builder("noricum".to_string(), "0.1.0".to_string())
    .register_tool(CompileChecker::tool(), CompileChecker::call())
    .register_tool(AstExtractor::tool(), AstExtractor::call())
    .register_tool(RunTests::tool(), RunTests::call())
    .build();
```

### 5.9 Performance Advantage

Benchmarks (2026) show Rig at **24% CPU usage** vs 4.7GB+ memory for Python frameworks. For a tool processing thousands of functions in a large codebase, this efficiency is critical.

### 5.10 Why Rig Over Alternatives

| Criterion | rig-rs | LangGraph (Python) | CrewAI (Python) |
|-----------|--------|-------------------|-----------------|
| Language | Rust (native) | Python | Python |
| Memory usage | <1.1 GB | >4.7 GB | >4.7 GB |
| CPU efficiency | 24% | 60%+ | 55%+ |
| Type safety | Full (compile-time) | Runtime only | Runtime only |
| Ollama support | Native provider | Via wrapper | Via wrapper |
| Tool definition | Typed `Tool` trait | Dict-based | Dict-based |
| Pipeline DAG | Native `Op` trait | Graph nodes | Crew tasks |
| MCP support | Native | Plugin | Plugin |
| Aligns with Noricum | Rust core + Rust agent | Rust core + Python agent | Rust core + Python agent |

**Verdict:** rig-rs keeps the entire project in Rust — no Python bridge, no FFI overhead, no language mismatch. The pipeline, agent, and tool abstractions map directly to Noricum's architecture.

---

## 6. Recommended Technology Stack

### 6.1 Agent Core (Rust)

| Component | Tool | Justification |
|-----------|------|---------------|
| CLI | `clap` | Rust standard |
| AST Parsing | `clang-sys` (v0), libTooling (v2) | Full access to Clang AST |
| Base translation | C2Rust (as dependency/subprocess) | Don't reinvent mechanical translation |
| IR | Custom structs + serde (JSON) | Debugging, caching |
| LLM Client | `rig-rs` | Native Rust LLM agent framework, efficient (24% CPU vs 4.7GB+ in Python) |
| Local LLM | Ollama API (OpenAI-compatible) | Qwen3-Coder, DeepSeek-R1 |
| Remote LLM | OpenAI API / Anthropic API via `rig-rs` | Fallback for hard cases |
| Testing | `cargo-test`, `cargo-fuzz`, `proptest` | Native ecosystem |
| Quality | `clippy`, `rustfmt` | Built-in |
| Tree-sitter | `tree-sitter` + C/Rust grammars | Fast incremental analysis |
| Build | `cargo` | Standard |

### 6.2 Orchestration Frameworks (alternatives)

If Python is preferred for the agent layer:

| Framework | Use Case | Note |
|-----------|----------|------|
| **LangGraph** | Complex state graphs, repair loops | More control, more code |
| **CrewAI** | Agent teams with roles | Fast to prototype, 40% less time to production |
| **AutoGen** | Multi-agent conversations | In maintenance mode (Microsoft) |

**Recommendation:** `rig-rs` if all-Rust. LangGraph if Python agent layer with maximum control is needed.

### 6.3 Infrastructure

| Need | Tool |
|------|------|
| Local LLM | Ollama on GPU (RTX 3090/4090 minimum for large models) |
| CI/CD | GitHub Actions |
| Metrics | Custom dashboard (lines migrated, unsafe%, score) |
| IR versioning | JSON with versioned schema |

---

## 7. Relevant Academic Research (2025-2026)

### 7.1 SACTOR (March 2025)
- **Approach:** 2-step translation: C -> unsafe Rust -> idiomatic Rust
- **Innovation:** Static analysis + FFI for end-to-end verification
- **Results:** DeepSeek-R1 achieves 93% success, 7x fewer Clippy warnings
- **Relevance:** Validates exactly the architecture proposed for Noricum
- [Paper](https://arxiv.org/abs/2503.12511)

### 7.2 RustMap (March 2025)
- **Approach:** Project-scale migration, not isolated functions
- **Innovation:** Decomposes by dependencies, translates small units, recomposes
- **Results:** Successful on bzip2 (7000+ lines) using GPT-4o
- **Relevance:** Decomposition strategy essential for large codebases
- [Paper](https://arxiv.org/abs/2503.17741)

### 7.3 CRUST-Bench (April 2025)
- **Approach:** Benchmark of 100 C repos with manual Rust interfaces
- **Results:** Best model (o3) achieves 48% with repair loop
- **Relevance:** Defines the benchmark against which to measure Noricum
- [Paper](https://arxiv.org/abs/2504.15254)

### 7.4 ForCLift (DARPA TRACTOR)
- **Approach:** Verified Lifting with formal methods + LLMs
- **Innovation:** Formal verification of the translation
- **Relevance:** Future direction for correctness guarantees
- [Info](https://csl.illinois.edu/news-and-media/translating-legacy-code-for-a-safer-future-darpa-backs-effort-to-convert-c-to-rust)

### 7.5 EvoC2Rust (2025)
- **Approach:** Skeleton-guided project-level translation framework
- **Relevance:** Complementary to RustMap
- [Paper](https://arxiv.org/html/2508.04295)

---

## 8. Feasibility Analysis

### 8.1 What Is Realistic TODAY

| Capability | Feasibility | Notes |
|-----------|------------|-------|
| Pure C (independent functions) | HIGH | C2Rust + LLM refinement works well |
| C with complex macros | MEDIUM | Requires pre-translation macro expansion |
| C with complex pointers | MEDIUM-HIGH | SACTOR demonstrates 93% with DeepSeek-R1 |
| Structs/enums in C | HIGH | Direct mapping |
| C with goto/setjmp | LOW-MEDIUM | Requires control flow restructuring |
| Simple C++ classes | MEDIUM | Struct + impl in Rust |
| C++ templates | LOW | Requires libTooling, complex generics |
| C++ STL usage | MEDIUM | Mapping to Rust stdlib |
| C++ metaprogramming | VERY LOW | Research frontier |

### 8.2 Differentiators vs Competition

| Feature | C2Rust | DARPA TRACTOR | AI Converters | Noricum Agent |
|---------|--------|---------------|---------------|---------------|
| Mechanical translation | YES | YES | Partial | YES (via C2Rust) |
| Safe & idiomatic Rust | NO | In development | Partial | YES (LLM layer) |
| Automatic tests | NO | YES | NO | YES |
| Differential verification | Partial | YES (formal) | NO | YES |
| Offline/private | YES | Not public | NO | YES (Ollama) |
| Project-scale | YES | In development | NO | YES (RustMap strategy) |
| Operating cost | Free | Not available | High | Configurable |

### 8.3 Main Risks

1. **LLM hallucinations:** Mitigation with mandatory differential tests
2. **Non-compilable code:** Mitigation with repair loop (max 5 attempts) + fallback to unsafe
3. **Semantics loss:** Mitigation with golden tests captured BEFORE translation
4. **C++ complexity:** Mitigation by starting with C only, C++ as v2
5. **API cost:** Mitigation with local model as default, API only for failures

---

## 9. Proposed Execution Plan

### Phase 0: Prototype (2 weeks)
- [ ] Setup: cargo project with clap CLI
- [ ] Integrate C2Rust as subprocess
- [ ] Connect Ollama via OpenAI-compatible API (rig-rs)
- [ ] Minimal pipeline: 1 miniz function, C -> C2Rust -> LLM refine -> compile check
- [ ] Result: functional proof of concept

### Phase 1: Basic Pipeline (4 weeks)
- [ ] AST extractor with clang-sys
- [ ] Custom IR with serde
- [ ] Test generator: golden tests from C
- [ ] Basic repair loop
- [ ] Idiomatic score (clippy + heuristics)
- [ ] Result: miniz fully migrated

### Phase 2: Intelligent Agent (4 weeks)
- [ ] Ownership inference agent
- [ ] Multi-model routing (local vs API based on difficulty)
- [ ] Dependency-guided decomposition (RustMap-style)
- [ ] Metrics dashboard
- [ ] Result: libsodium migrated

### Phase 3: Scale (4 weeks)
- [ ] Parallelism (multiple functions simultaneously)
- [ ] Translation caching
- [ ] Support for projects with build systems (CMake, Make)
- [ ] CI/CD integration
- [ ] Result: tool ready for medium projects (10K-50K lines)

### Phase 4: C++ and Enterprise (future)
- [ ] libTooling bridge for C++
- [ ] Classes -> structs + traits
- [ ] Basic templates -> generics
- [ ] Integration with DARPA TRACTOR findings

---

## 10. Cost Estimation

### Hardware (one-time)
- GPU: RTX 4090 24GB (~$1,600) or RTX 3090 24GB (~$800 used)
- RAM: 64GB minimum
- SSD: 1TB NVMe

### Monthly Operation
- Ollama local: $0
- Paid API (estimated mixed use): $50-200/month during development
- Production for clients: $5-15 per 10K lines (hybrid)

### Development Time (1 person)
- Phase 0-1: 6 weeks
- Phase 2: 4 weeks
- Phase 3: 4 weeks
- **Total to usable MVP: ~14 weeks**

---

## 11. Conclusions

### Is it viable? **YES, this is the optimal moment.**

**Reasons:**
1. Academic research (SACTOR, RustMap) has validated exactly the architecture Noricum proposes
2. Local LLMs (Qwen3-Coder, DeepSeek-R1) are now good enough for 80% of cases
3. C2Rust solves the mechanical part, freeing the agent to focus on the intelligent part
4. Microsoft, DARPA, and the industry are creating massive demand
5. No integrated product yet combines deterministic pipeline + LLM agent + verification

### Winning strategy:
1. **Don't compete with C2Rust** — use it as a base
2. **Don't depend 100% on LLMs** — deterministic pipeline as skeleton
3. **Obsessive verification** — differential tests are the real differentiator
4. **Hybrid LLM model** — local for volume, paid for precision
5. **Start with pure C** — C++ is v2, not v0

### Most valuable insight:
The Noricum project already has the right architecture. What's missing is:
1. Integrate C2Rust as step zero
2. Add the LLM agent layer (rig-rs + Ollama)
3. Implement the repair loop with differential tests
4. Build intelligent routing between models

---

## Sources

### Tools and Platforms
- [C2Rust - Immunant](https://github.com/immunant/c2rust)
- [Ollama](https://github.com/ollama/ollama)
- [Rig-rs - Rust LLM Framework](https://github.com/0xPlaygrounds/rig)
- [OpenAI Codex](https://openai.com/codex/)
- [Claude Code - Anthropic](https://github.com/anthropics/claude-code)
- [Tree-sitter Rust Grammar](https://github.com/tree-sitter/tree-sitter-rust)
- [OpenClaw - AI Agent](https://open-claw.org/)

### Rig.rs Documentation
- [Rig Official Docs](https://docs.rig.rs/)
- [Rig Architecture](https://docs.rig.rs/docs/architecture)
- [Rig Agents Concepts](https://docs.rig.rs/docs/concepts/agent)
- [Rig Tools Concepts](https://docs.rig.rs/docs/concepts/tools)
- [Rig Pipeline Module](https://docs.rig.rs/docs/concepts/chains)
- [Rig MCP Integration](https://dev.to/joshmo_dev/using-model-context-protocol-with-rig-m7o)
- [Rig Agentic Design Patterns](https://dev.to/joshmo_dev/implementing-design-patterns-for-agentic-ai-with-rig-rust-1o71)
- [Rig API Reference (docs.rs)](https://docs.rs/rig-core/latest/rig/)
- [Rig Flight Assistant Example](https://docs.rig.rs/guides/advanced/flight_assistant)

### Academic Research
- [SACTOR: LLM-Driven C to Rust Translation (2025)](https://arxiv.org/abs/2503.12511)
- [RustMap: Project-Scale C-to-Rust Migration (2025)](https://arxiv.org/abs/2503.17741)
- [CRUST-Bench: C-to-safe-Rust Transpilation Benchmark (2025)](https://arxiv.org/abs/2504.15254)
- [EvoC2Rust: Skeleton-guided C-to-Rust Translation (2025)](https://arxiv.org/html/2508.04295)
- [From C to Rust: Evaluating LLM Capabilities (2025)](https://link.springer.com/chapter/10.1007/978-3-032-07612-0_24)

### Institutional Programs
- [DARPA TRACTOR Program](https://www.darpa.mil/research/programs/translating-all-c-to-rust)
- [ForCLift - DARPA/Illinois](https://csl.illinois.edu/news-and-media/translating-legacy-code-for-a-safer-future-darpa-backs-effort-to-convert-c-to-rust)
- [Microsoft C/C++ to Rust Initiative](https://www.windowscentral.com/microsoft/windows-11/my-goal-is-to-eliminate-every-line-of-c-and-c-from-microsoft-by-2030-microsoft-bets-on-ai-to-finally-modernize-windows)
- [Anthropic - Building C Compiler in Rust](https://www.anthropic.com/engineering/building-c-compiler)
- [IEEE Spectrum - AI Code Transforms C to Rust](https://spectrum.ieee.org/ai-code-rust-great-refactor)

### Model and Framework Comparisons
- [AI Agent Frameworks Comparison 2026](https://calmops.com/ai/ai-agent-frameworks-comparison-2026/)
- [Best Open-Source LLMs for Coding 2026](https://www.siliconflow.com/articles/en/best-open-source-LLMs-for-coding)
- [Rust Libraries for LLM Orchestration 2026](https://dasroot.net/posts/2026/02/rust-libraries-llm-orchestration-2026/)
- [Benchmarking AI Agent Frameworks - AutoAgents Rust](https://dev.to/saivishwak/benchmarking-ai-agent-frameworks-in-2026-autoagents-rust-vs-langchain-langgraph-llamaindex-338f)
- [Legacy Code Migration: C to Rust Tools 2025](https://markaicode.com/legacy-code-migration-c-to-rust-tools-2025/)
