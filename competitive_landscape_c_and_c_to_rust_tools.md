# Competitive Landscape: Tools for C/C++ to Rust Migration

## Overview
This document summarizes the main competing tools and approaches related to automated or assisted translation of large C and C++ codebases into Rust. For each project the existing capabilities are listed along with the features that are currently missing.

---

## 1. C2Rust

### Available Features
- Industrial-strength parsing based on Clang AST and LLVM frontend.
- Automated translation of C99 projects into compilable Rust.
- Preservation of data layouts through repr(C) compatibility.
- Localized emission of unsafe Rust to reflect pointer operations.
- Refactoring utilities to iteratively improve the generated output.
- Basic differential execution plugins.

### Missing Features
- Automatic generation of fully safe Rust.
- Native support for large-scale C++ metaprogramming.
- Ownership and lifetime inference at expert level.
- Production of idiomatic Rust using iterators, traits, and standard types.
- Integrated fuzzing and property-based test creation.

---

## 2. Corrode

### Available Features
- Source-to-source translation from C into Rust syntax.
- Straightforward mapping of control flow and structs.
- ABI-compatible output when the source is well defined.

### Missing Features
- Deep semantic analysis.
- Handling of C++ templates and classes.
- Test generation.
- Equivalence verification.
- Reduction of unsafe code.
- Scalability for millions of lines.

---

## 3. CRUST

### Available Features
- Experimental conversion of simple C and C++ snippets.
- Project skeleton generation.
- Comment preservation.

### Missing Features
- Robust typed parser.
- Pointer and macro handling.
- Ownership modeling.
- Semantic assurance.
- Differential testing.
- Real production usage.

---

## 4. Online AI Converters

### Available Features
- Instant translation of small fragments.
- Interactive suggestions.

### Missing Features
- AST as source of truth.
- Guarantees of behavior.
- Large project output.
- Idiomatic assurance.
- CI integration.

---

## 5. FFI Binding Generators

### Available Features
- bindgen, cxx, autocxx generate Rust interfaces for C/C++.
- Incremental interoperability.

### Missing Features
- Logic translation.
- Architectural migration.
- Behavior comparison.

---

## 6. Academic Prototypes

### Available Features
- LLM-guided transformation research.
- Pointer lifting experiments.
- Symbolic equivalence studies.

### Missing Features
- Cohesive product.
- Build system integration.
- Idiomatic architecture.

---

## Global Gaps in the Market

The following capabilities are not fully covered by any current tool:

1. Comprehensive C++ to Rust migration with templates and OOP.
2. Automatic characterization test generation tightly coupled to the transformation.
3. Ownership and lifetime inference from legacy pointer models.
4. Idiomaticity checking validated against differential behavior.
5. Enterprise-scale processing and incremental strangler replacement.

---

## Conclusion

The ecosystem shows valuable efforts, but existing solutions focus on syntactic fidelity rather than verifiable, safe, and idiomatic Rust generation. A new project addressing these gaps would provide clear strategic value for cloud providers and operating system teams maintaining critical native code.

