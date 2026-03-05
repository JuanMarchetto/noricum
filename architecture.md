# Noricum Architecture

**Version:** 1.0  
**Last Updated:** 2024  
**Status:** Design Document

## Table of Contents

1. [Overview](#overview)
2. [Architectural Principles](#architectural-principles)
3. [System Architecture](#system-architecture)
4. [Component Details](#component-details)
5. [Data Flow](#data-flow)
6. [Evolution Path](#evolution-path)
7. [Implementation Guidelines](#implementation-guidelines)
8. [Interfaces & Contracts](#interfaces--contracts)

---

## Overview

Noricum is a modular, extensible system for migrating C/C++ codebases to Rust. The architecture is designed to:

- **Enable rapid v0**: Start with minimal components for C code
- **Scale gracefully**: Add complexity incrementally without breaking existing functionality
- **Maintain quality**: Each component is independently testable and verifiable
- **Support extensibility**: New features integrate via well-defined interfaces

### Core Design Philosophy

1. **Pipeline Architecture**: Data flows through stages, each stage is independently replaceable
2. **Plugin System**: Features are pluggable components, not hardcoded
3. **Progressive Enhancement**: Start simple, add sophistication incrementally
4. **Test-Driven**: Every transformation is validated by generated tests
5. **Language Agnostic IR**: Intermediate representation decouples parsing from generation

---

## Architectural Principles

### 1. Separation of Concerns

Each component has a single, well-defined responsibility:
- **Parsing** → Extract AST from source
- **Analysis** → Understand semantics and structure
- **Transformation** → Convert to Rust idioms
- **Generation** → Emit Rust code
- **Validation** → Verify correctness

### 2. Interface-Based Design

Components communicate through well-defined interfaces (traits in Rust). This allows:
- Swapping implementations (e.g., libclang vs libTooling)
- Parallel development of components
- Easy testing with mocks
- Future AI integration without core changes

### 3. Progressive Complexity

**v0**: Simple, direct transformations
**v1**: Add ownership inference
**v2**: Add C++ support
**v3**: Add AI-assisted refactoring

Each version builds on the previous without breaking changes.

### 4. Fail-Safe Defaults

- Always generate compilable Rust (even if unsafe)
- Provide FFI fallback for untranslatable code
- Generate tests before transformation
- Never lose information (preserve original in comments)

---

## System Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI Interface                            │
│                    (clap-based command parser)                   │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Orchestration Layer                        │
│              (Coordinates pipeline execution)                    │
└────────────────────────────┬────────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│   Parser      │   │   Analyzer    │   │  Transformer  │
│  (Extract)    │──▶│  (Understand) │──▶│  (Convert)    │
└───────────────┘   └───────────────┘   └───────────────┘
        │                    │                    │
        │                    │                    │
        └────────────────────┼────────────────────┘
                             │
                             ▼
                    ┌───────────────┐
                    │   Generator   │
                    │   (Emit Rust) │
                    └───────┬───────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│  Test Gen     │   │   Validator   │   │   Reporter   │
│  (Harnesses)  │   │  (Verify)     │   │  (Metrics)   │
└───────────────┘   └───────────────┘   └───────────────┘
```

### Component Layers

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                         │
│  CLI, Configuration, Project Management, Build Integration   │
└─────────────────────────────────────────────────────────────┘
                            │
┌─────────────────────────────────────────────────────────────┐
│                    Core Pipeline Layer                      │
│  Parser → Analyzer → Transformer → Generator → Validator   │
└─────────────────────────────────────────────────────────────┘
                            │
┌─────────────────────────────────────────────────────────────┐
│                    Infrastructure Layer                     │
│  IR, Type System, Error Handling, Logging, Caching         │
└─────────────────────────────────────────────────────────────┘
                            │
┌─────────────────────────────────────────────────────────────┐
│                    External Dependencies                    │
│  Clang (libclang/libTooling), Rust Toolchain, Test Tools   │
└─────────────────────────────────────────────────────────────┘
```

---

## Component Details

### 1. Parser Component

**Purpose**: Extract AST from C/C++ source code

**v0 Implementation**:
- Use `clang-sys` (libclang C API)
- Extract functions, types, control flow
- Serialize to IR format (JSON/MessagePack)

**Future Enhancements**:
- Add libTooling bridge for C++ templates
- Support for macros and preprocessor
- Incremental parsing for large codebases

**Interface**:
```rust
pub trait Parser {
    fn parse(&self, source: &Path, config: &ParseConfig) -> Result<TranslationUnit>;
    fn extract_functions(&self, tu: &TranslationUnit) -> Vec<Function>;
    fn extract_types(&self, tu: &TranslationUnit) -> Vec<Type>;
}
```

**Output**: `TranslationUnit` (IR representation)

---

### 2. IR (Intermediate Representation)

**Purpose**: Language-agnostic representation of parsed code

**Design Principles**:
- Serializable (JSON/MessagePack) for debugging and caching
- Extensible (versioned schema)
- Preserves all semantic information
- Supports incremental updates

**Core Structures** (v0):
```rust
pub struct TranslationUnit {
    pub functions: Vec<Function>,
    pub types: Vec<Type>,
    pub globals: Vec<Global>,
    pub metadata: Metadata,
}

pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Type,
    pub body: Block,
    pub attributes: FunctionAttributes,
}

pub struct Type {
    pub kind: TypeKind,  // Primitive, Struct, Pointer, Array, etc.
    pub name: Option<String>,
    pub size: Option<usize>,
    pub alignment: Option<usize>,
}
```

**Future Extensions**:
- Ownership hints
- Lifetime annotations
- Template instantiations (C++)
- Macro expansions

---

### 3. Analyzer Component

**Purpose**: Understand semantics, infer patterns, prepare for transformation

**v0 Implementation**:
- Basic control flow analysis
- Type resolution
- Error code pattern detection
- Pointer usage analysis

**Future Enhancements**:
- Ownership inference
- Lifetime analysis
- Dead code detection
- Performance hotspots

**Interface**:
```rust
pub trait Analyzer {
    fn analyze(&self, tu: &TranslationUnit) -> Result<AnalysisResult>;
    fn infer_ownership(&self, func: &Function) -> OwnershipModel;
    fn detect_patterns(&self, tu: &TranslationUnit) -> Vec<Pattern>;
}
```

**Output**: `AnalysisResult` with annotations and hints

---

### 4. Transformer Component

**Purpose**: Convert IR to Rust-idiomatic representation

**Architecture**: Plugin-based transformation pipeline

**v0 Transformations**:
- C types → Rust types
- Error codes → `Result<T, E>`
- Pointers → `*mut T` / `*const T` (unsafe, but correct)
- Control flow → Rust equivalents

**Future Transformations**:
- Pointer lifting → slices/borrows
- Memory management → ownership
- C++ classes → Rust structs + traits
- Templates → generics

**Interface**:
```rust
pub trait Transformer {
    fn transform(&self, ir: &TranslationUnit, analysis: &AnalysisResult) -> Result<RustIR>;
}

pub trait TransformRule {
    fn applies_to(&self, node: &IRNode) -> bool;
    fn transform(&self, node: &IRNode, ctx: &TransformContext) -> Result<RustNode>;
}
```

**Plugin System**:
```rust
// v0: Basic rules
- TypeTransformer
- ErrorCodeTransformer
- ControlFlowTransformer

// v1: Ownership rules
- OwnershipLifter
- LifetimeInferrer

// v2: C++ rules
- ClassTransformer
- TemplateTransformer
```

---

### 5. Generator Component

**Purpose**: Emit compilable Rust code from RustIR

**v0 Implementation**:
- Direct code generation (string templates)
- Basic formatting
- FFI fallback generation

**Future Enhancements**:
- AST-based generation (using `syn`/`quote`)
- Formatting with `rustfmt`
- Documentation generation
- Module organization

**Interface**:
```rust
pub trait Generator {
    fn generate(&self, rust_ir: &RustIR, config: &GenConfig) -> Result<GeneratedCode>;
    fn generate_ffi_bridge(&self, func: &Function) -> Result<String>;
}
```

**Output**: `GeneratedCode` (Rust source files + Cargo.toml)

---

### 6. Test Generator Component

**Purpose**: Create test harnesses for behavior verification

**v0 Implementation**:
- Golden test generation (capture C output)
- Basic fuzzing harness skeleton
- Differential test framework

**Future Enhancements**:
- Property-based tests (proptest)
- Coverage-guided fuzzing (cargo-fuzz)
- Performance benchmarks
- Integration test generation

**Interface**:
```rust
pub trait TestGenerator {
    fn generate_golden_tests(&self, func: &Function) -> Result<TestHarness>;
    fn generate_fuzz_harness(&self, func: &Function) -> Result<FuzzHarness>;
    fn generate_property_tests(&self, func: &Function) -> Result<PropertyTests>;
}
```

**Strategy**: Generate tests BEFORE transformation, run against both C and Rust

---

### 7. Validator Component

**Purpose**: Verify correctness and quality of generated code

**v0 Implementation**:
- Compilation check
- Basic idiomaticity scoring
- Test execution verification

**Future Enhancements**:
- Clippy integration
- Ownership safety analysis
- Performance regression detection
- Coverage comparison

**Interface**:
```rust
pub trait Validator {
    fn validate_compilation(&self, code: &GeneratedCode) -> Result<ValidationResult>;
    fn check_idiomaticity(&self, code: &GeneratedCode) -> IdiomaticityScore;
    fn verify_tests(&self, tests: &TestHarness) -> Result<TestResults>;
}
```

---

### 8. Ownership Engine (Future)

**Purpose**: Infer Rust ownership and lifetimes from C pointer patterns

**When to Add**: v1 (after basic C migration works)

**Approach**:
- Pattern matching on pointer usage
- Data flow analysis
- Conservative inference (default to unsafe, improve incrementally)

**Interface**:
```rust
pub trait OwnershipEngine {
    fn infer_ownership(&self, func: &Function) -> OwnershipModel;
    fn suggest_refactoring(&self, model: &OwnershipModel) -> Vec<Refactoring>;
}
```

---

### 9. AI Assistant (Future)

**Purpose**: Suggest idiomatic improvements, handle edge cases

**When to Add**: v2+ (after core pipeline is stable)

**Architecture**:
- Separate service/plugin
- Operates on RustIR, not source
- Validates suggestions with test suite
- Never modifies code without verification

**Interface**:
```rust
pub trait AIAssistant {
    fn suggest_improvements(&self, code: &RustIR) -> Result<Vec<Suggestion>>;
    fn apply_suggestion(&self, suggestion: &Suggestion) -> Result<RustIR>;
}
```

---

## Data Flow

### Standard Migration Flow

```
1. Source Code (C/C++)
   │
   ▼
2. Parser → TranslationUnit (IR)
   │
   ▼
3. Analyzer → AnalysisResult (annotations)
   │
   ▼
4. Transformer → RustIR (Rust representation)
   │
   ▼
5. Generator → GeneratedCode (Rust source)
   │
   ├─→ 6a. TestGenerator → TestHarness
   │
   └─→ 6b. Validator → ValidationResult
```

### Incremental Migration Flow

```
1. Parse entire codebase → TranslationUnit
2. User selects functions/modules to migrate
3. For each selection:
   a. Generate tests (against C version)
   b. Transform to Rust
   c. Generate FFI bridge
   d. Run tests (both C and Rust)
   e. Report results
4. User reviews and iterates
```

### Test-Driven Flow

```
1. Parse function
2. Generate golden tests (capture C behavior)
3. Transform to Rust (with FFI fallback)
4. Run tests against Rust (should pass via FFI)
5. Gradually improve transformation
6. Remove FFI fallback when tests pass
```

---

## Evolution Path

### v0: Minimal Viable Product (10 days)

**Scope**: C code only, basic transformations

**Components**:
- ✅ Parser (libclang)
- ✅ Basic IR
- ✅ Simple Transformer (types, error codes)
- ✅ Basic Generator
- ✅ Golden test generation
- ✅ Compilation validator

**Deliverables**:
- Migrate 2 miniz functions
- Migrate 1 libsodium function
- Idiomaticity score

**Architecture Decisions**:
- Use libclang (no C++ bridge yet)
- Simple string-based code generation
- JSON IR for debugging
- FFI fallback for all functions

---

### v1: Ownership & Safety (Next 20 days)

**Add**:
- Ownership inference engine
- Pointer lifting transformations
- Property-based test generation
- Clippy integration
- Unsafe code reduction

**Architecture Changes**:
- Extend IR with ownership hints
- Add ownership analysis plugin
- Add pointer lifting transformer

---

### v2: C++ Support (Future)

**Add**:
- libTooling bridge (C++ AST extractor)
- C++ IR extensions (classes, templates)
- Class → struct + trait transformer
- Template → generic transformer

**Architecture Changes**:
- Dual parser support (libclang + libTooling)
- Unified IR (handles both C and C++)
- C++-specific transformers

---

### v3: Enterprise Features (Future)

**Add**:
- Incremental migration support
- Build system integration
- Project-wide analysis
- AI-assisted refactoring
- Performance benchmarking

**Architecture Changes**:
- Project management layer
- Caching and incremental updates
- AI plugin system

---

## Implementation Guidelines

### 1. Module Organization

```
noricum/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs                 # CLI entry point
│   ├── lib.rs                  # Library root
│   │
│   ├── cli/                    # CLI interface
│   │   ├── mod.rs
│   │   ├── commands.rs
│   │   └── config.rs
│   │
│   ├── parser/                 # AST extraction
│   │   ├── mod.rs
│   │   ├── libclang.rs         # v0: libclang implementation
│   │   ├── libtooling.rs       # v2: libTooling bridge
│   │   └── traits.rs
│   │
│   ├── ir/                     # Intermediate representation
│   │   ├── mod.rs
│   │   ├── translation_unit.rs
│   │   ├── function.rs
│   │   ├── types.rs
│   │   └── serialize.rs
│   │
│   ├── analyzer/               # Semantic analysis
│   │   ├── mod.rs
│   │   ├── control_flow.rs
│   │   ├── patterns.rs
│   │   └── ownership.rs        # v1
│   │
│   ├── transformer/            # IR → RustIR
│   │   ├── mod.rs
│   │   ├── pipeline.rs
│   │   ├── rules/              # Transformation rules
│   │   │   ├── mod.rs
│   │   │   ├── types.rs
│   │   │   ├── errors.rs
│   │   │   ├── control_flow.rs
│   │   │   └── ownership.rs    # v1
│   │   └── traits.rs
│   │
│   ├── rust_ir/                # Rust representation
│   │   ├── mod.rs
│   │   ├── function.rs
│   │   ├── types.rs
│   │   └── module.rs
│   │
│   ├── generator/              # Code emission
│   │   ├── mod.rs
│   │   ├── rust.rs
│   │   ├── ffi.rs
│   │   └── cargo.rs
│   │
│   ├── testgen/                # Test generation
│   │   ├── mod.rs
│   │   ├── golden.rs
│   │   ├── fuzz.rs
│   │   └── property.rs         # v1
│   │
│   ├── validator/              # Verification
│   │   ├── mod.rs
│   │   ├── compilation.rs
│   │   ├── idiomaticity.rs
│   │   └── tests.rs
│   │
│   ├── ownership/              # v1: Ownership engine
│   │   ├── mod.rs
│   │   ├── inference.rs
│   │   └── patterns.rs
│   │
│   └── orchestration/          # Pipeline coordination
│       ├── mod.rs
│       ├── pipeline.rs
│       └── project.rs          # v3
│
├── cpp-extractor/              # v2: C++ AST extractor
│   ├── CMakeLists.txt
│   └── src/
│       └── main.cpp
│
└── tests/
    ├── integration/
    └── fixtures/
```

### 2. Error Handling Strategy

**Principle**: Never fail silently, always provide actionable errors

```rust
// Use Result types throughout
pub type Result<T> = std::result::Result<T, NoricumError>;

#[derive(Debug, thiserror::Error)]
pub enum NoricumError {
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Transformation error: {0}")]
    Transform(String),
    
    #[error("Generation error: {0}")]
    Generation(String),
    
    // Always include context
    #[error("Failed to transform function {name}: {reason}")]
    FunctionTransform { name: String, reason: String },
}
```

### 3. Configuration Management

**v0**: Simple CLI flags
**v1+**: Configuration file support

```rust
#[derive(Debug, Deserialize)]
pub struct Config {
    pub parser: ParserConfig,
    pub transformer: TransformerConfig,
    pub generator: GeneratorConfig,
    pub testgen: TestGenConfig,
}

// Allow per-project overrides
// Support for .noricum.toml in project root
```

### 4. Logging & Debugging

**Use structured logging**:
```rust
use tracing::{info, warn, error, debug};

// Levels:
// - ERROR: Failures that prevent migration
// - WARN: Issues that degrade quality but don't fail
// - INFO: Progress and milestones
// - DEBUG: Detailed AST/IR dumps (opt-in)
```

**IR Serialization**: Always allow JSON dump for debugging
```rust
// CLI flag: --dump-ir
// Outputs: translation_unit.json, rust_ir.json
```

### 5. Testing Strategy

**Unit Tests**: Each component independently
**Integration Tests**: End-to-end on real code (miniz, libsodium)
**Property Tests**: IR roundtrip (parse → serialize → parse)

```rust
// Example integration test
#[test]
fn test_miniz_compress() {
    let result = migrate_function("tests/fixtures/miniz.c", "tdefl_compress");
    assert!(result.rust_code.compiles());
    assert!(result.tests.pass());
}
```

---

## Interfaces & Contracts

### Core Traits

```rust
// Parser contract
pub trait Parser: Send + Sync {
    fn parse(&self, source: &Path, config: &ParseConfig) -> Result<TranslationUnit>;
    fn supports_language(&self, lang: Language) -> bool;
}

// Transformer contract
pub trait Transformer: Send + Sync {
    fn transform(&self, ir: &TranslationUnit, analysis: &AnalysisResult) -> Result<RustIR>;
    fn register_rule(&mut self, rule: Box<dyn TransformRule>);
}

// Generator contract
pub trait Generator: Send + Sync {
    fn generate(&self, rust_ir: &RustIR, config: &GenConfig) -> Result<GeneratedCode>;
}
```

### Data Contracts

**IR Versioning**: IR format is versioned, supports migration
```rust
pub struct TranslationUnit {
    pub version: u32,  // IR version
    // ... fields
}
```

**Backward Compatibility**: v0 IR must be readable by v1 (with defaults for new fields)

---

## Extension Points

### Adding a New Transformation Rule

```rust
// 1. Implement TransformRule trait
pub struct MyCustomRule;

impl TransformRule for MyCustomRule {
    fn applies_to(&self, node: &IRNode) -> bool {
        // Check if rule applies
    }
    
    fn transform(&self, node: &IRNode, ctx: &TransformContext) -> Result<RustNode> {
        // Perform transformation
    }
}

// 2. Register in transformer
transformer.register_rule(Box::new(MyCustomRule));
```

### Adding a New Parser

```rust
// 1. Implement Parser trait
pub struct MyCustomParser;

impl Parser for MyCustomParser {
    fn parse(&self, source: &Path, config: &ParseConfig) -> Result<TranslationUnit> {
        // Parse and return IR
    }
}

// 2. Use in pipeline
let parser: Box<dyn Parser> = Box::new(MyCustomParser);
```

### Adding AI Integration

```rust
// AI is a plugin, not core
pub struct AITransformer {
    client: AIClient,
}

impl Transformer for AITransformer {
    // Uses AI to suggest improvements
    // Always validates with tests
}
```

---

## Performance Considerations

### v0: Not a priority
- Focus on correctness
- Acceptable: < 1 second per function

### v1+: Optimize
- Parallel processing of independent functions
- Caching of parsed ASTs
- Incremental updates
- Target: < 100ms per function for large codebases

---

## Security Considerations

- **Sandboxing**: Generated tests run in isolated environment
- **Input Validation**: All user-provided paths validated
- **No Code Execution**: Tool never executes user code directly (only via tests)
- **Audit Trail**: Log all transformations for review

---

## Conclusion

This architecture provides:

1. **Quick v0**: Minimal components, simple implementations
2. **Scalability**: Clear extension points for all features
3. **Maintainability**: Modular design, well-defined interfaces
4. **Testability**: Each component independently testable
5. **Flexibility**: Plugin system allows experimentation

**Key Success Factors**:
- Start simple, add complexity incrementally
- Maintain clear interfaces between components
- Always generate tests before transformation
- Never break backward compatibility of IR format
- Document decisions and rationale

---

## Appendix: Decision Log

| Decision | Rationale | Version |
|----------|-----------|---------|
| Use libclang for v0 | Faster to implement, sufficient for C | v0 |
| JSON IR format | Easy debugging, language-agnostic | v0 |
| Plugin-based transformers | Extensible, testable | v0 |
| FFI fallback always | Never fail to generate compilable code | v0 |
| Separate C++ extractor | Full AST access without complex FFI | v2 |

---

**Next Steps for Developers**:

1. Read this document thoroughly
2. Start with `src/orchestration/pipeline.rs` for v0
3. Implement components in order: Parser → IR → Transformer → Generator
4. Add tests for each component
5. Refer to `roadmap_v0.md` for day-by-day tasks

