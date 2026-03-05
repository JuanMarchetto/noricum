---
name: c-cpp-analyzer
description: Analyze C/C++ code for migration planning. Use when examining C source code to assess migration difficulty, identify patterns, and plan Rust conversion strategy.
---

# C/C++ Analysis for Migration

## Difficulty Classification
- **Easy**: Pure functions, simple arithmetic, no pointers, < 30 lines
- **Medium**: Pointer parameters, simple structs, bounded arrays, malloc with clear ownership
- **Hard**: void*, function pointers, unions, goto, complex lifetimes, macro-heavy code

## Pattern Detection
Look for these C patterns and their Rust equivalents:
| C Pattern | Rust Equivalent |
|-----------|----------------|
| `ptr + len` parameter pairs | `&[T]` slices |
| Error code returns | `Result<T, E>` |
| NULL checks | `Option<T>` |
| `malloc`/`free` | `Vec<T>`, `Box<T>` |
| `char*` strings | `&str`, `String` |
| Manual iteration with pointers | Iterators |
| `goto` cleanup | `Drop` trait, `?` operator |
| Global mutable state | Function parameters, `OnceLock` |
| Bit manipulation | Same in Rust (with explicit types) |
| `union` | `enum` with variants |
| Function pointers / callbacks | `Fn` traits, generics |

## Risk Flags
- Platform-specific code (`#ifdef _WIN32`, inline assembly)
- Undefined behavior (signed overflow, use-after-free patterns)
- Macro-heavy code (requires expansion before analysis)
- Variadic functions (`...` parameters)
- setjmp/longjmp
- Thread-unsafe global state
