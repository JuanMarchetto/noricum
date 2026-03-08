---
name: c-cpp-analyzer
description: Analyze C/C++ code for migration planning. Use when examining C source code to assess migration difficulty, identify patterns, and plan Rust conversion strategy.
---

# C/C++ Analysis for Migration

## Difficulty Classification
Uses tree-sitter AST analysis with regex fallback:
- **Easy**: Pure functions, simple arithmetic, no pointers, <30 lines
- **Medium**: Pointer parameters, simple structs, bounded arrays, malloc with clear ownership
- **Hard**: void*, function pointers, unions, goto, complex lifetimes, macro-heavy, coroutines

## Hard signals (any 1 → Hard):
- `goto` statements
- `union` types
- `void*` pointers
- Function pointer callbacks
- Coroutine patterns (Duff's device, `switch`-based state machines)
- setjmp/longjmp

## Medium signals (1+ with >30 lines → Medium):
- Pointer declarations
- Cast expressions
- `malloc`/`calloc` usage

## Pattern Detection
| C Pattern | Rust Equivalent | Difficulty |
|-----------|----------------|------------|
| `ptr + len` pairs | `&[T]` slices | Medium |
| Error code returns | `Result<T, E>` | Easy |
| NULL checks | `Option<T>` | Easy |
| `malloc`/`free` | `Vec<T>`, `Box<T>` | Medium |
| `char*` strings | `&str`, `String` | Medium |
| Manual iteration with pointers | `.iter()`, `.enumerate()` | Medium |
| `goto` cleanup | `Drop` trait, `?` operator | Hard |
| Global mutable state | Function parameters, `OnceLock` | Hard |
| Bit manipulation | Same ops with explicit types | Medium |
| `union` | `enum` with variants | Hard |
| Function pointers / callbacks | Enum dispatch (preferred) or `Fn` traits | Hard |
| Linked lists (next/prev) | `Vec<T>` (flatten) | Medium |
| Type tag + union (tagged union) | `enum` with data variants | Medium-Hard |
| Recursive data structures | `enum` + `Box<T>` | Medium |
| Coroutine via switch/case | Explicit state machine (`match` loop) | Hard |
| Printf format strings | `format!()` / `println!()` macros | Easy-Medium |
| `strdup`/`strcpy` | `.to_string()`, `.clone()` | Easy |

## Size-Based Strategy
| LOC Range | Strategy | Chunking | Repair Iters |
|-----------|----------|----------|--------------|
| <800 | Single-pass translation | No | Full (5) |
| 800-2000 | Chunked (400 LOC target) | Structural (data model first) | Full (8 for chunked) |
| 2000+ | Chunked (500 LOC target) | Structural | Reduced (2) |
| 4000+ | Consider per-module split | split_into_modules() | Reduced (2) |

## Risk Flags
- Platform-specific code (`#ifdef _WIN32`, inline assembly)
- Undefined behavior (signed overflow, use-after-free patterns)
- Macro-heavy code (requires `cc -E` expansion before analysis)
- Variadic functions (`...` parameters)
- setjmp/longjmp (no safe Rust equivalent)
- Thread-unsafe global state
- Bit-exact algorithms (compression, crypto) — very hard to get semantically correct
- Files >4000 LOC — may exceed API token limits even with chunking

## Migration Ceiling (learned from miniz)
Some patterns have NO safe Rust equivalent and require full algorithm redesign:
- Coroutine via switch/case (Duff's device) → explicit state machine
- Large structs with internal pointers (~300KB) → redesign needed
- Bit-level DEFLATE/INFLATE → requires exact semantic preservation
- These files need "best-effort" mode: output highest-quality version for developer to finish
