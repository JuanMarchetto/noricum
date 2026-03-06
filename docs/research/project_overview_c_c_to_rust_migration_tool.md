# Noricum
# Project Overview: Automated C/C++ to Rust Migration Tool

## 1. Vision
The project aims to create an industrial-grade tool capable of rewriting very large C and C++ codebases into Rust while preserving semantic behavior. The approach is based on real AST analysis, automatic generation of characterization tests, and incremental transformation of selected components.

## 2. Core Characteristics

### Semantic Equivalence First
- Preservation of observable behavior of the original system.
- Differential execution between C/C++ binaries and generated Rust modules.
- Continuous regression detection using automatically produced tests.

### AST-Guided Transformation
- Use of Clang/LLVM tooling as the source of truth for parsing.
- Extraction of typed AST and control flow information per function.
- Rule-based mapping from source language constructs to Rust representations.

### Automatic Test Generation
- Creation of fuzzing harnesses for individual functions.
- Golden I/O tests to capture real outputs.
- Property-based tests to express invariants.
- Coverage measurement before and after migration.

### Incremental Migration Model
- Mixed builds combining legacy components with Rust bindings.
- Strangler-fig style replacement of modules.
- Progressive reduction of unsafe Rust.

### Idiomaticity Assessment
- Integration with clippy and rustfmt.
- Heuristic analysis of ownership, lifetimes, and cloning.
- AI-assisted refactoring validated by the test suite.

## 3. Architecture Components

1. **AST Extractor** – parses existing code and produces structured intermediate representation.
2. **Test Generator** – builds automatic harnesses and differential tests.
3. **Rust Generator** – emits compilable Rust faithful to layouts.
4. **Ownership Engine** – infers borrowing models.
5. **Assure Module** – verifies behavior and idioms.

## 4. Target Users
- Cloud providers maintaining high-performance native libraries.
- Operating system teams reducing memory CVEs.
- Enterprises with decades of legacy code.

## 5. Technology Stack

### Primary Language
- Rust for the transformation core and CLI.

### Interoperability
- C++ bridges to clang/libTooling.

### AI Layer
- Python or Rust clients interacting with LLMs only as assistants.

### Testing
- cargo-fuzz, proptest, llvm-cov.

## 6. Value Proposition for Microsoft and AWS
- Measurable reduction of memory vulnerabilities.
- Migration without blocking releases.
- Reproducible metrics and CI integration.

## 7. Starting Scope
The initial prototype focuses on:
- C modules without extreme macro metaprogramming.
- Independent functions.
- POD structs and error handling.

---

**Conclusion**
The project combines compiler technology, dynamic testing, and AI assistance to make large-scale migrations credible, safe, and verifiable.

