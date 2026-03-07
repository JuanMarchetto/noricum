# Case Study: Migrating a 1,686-Line Expression Evaluator from C to Safe Rust

**TL;DR:** Noricum migrated a 1,686-line C expression evaluator — with a lexer, recursive descent parser, variable store, 25+ builtins, HashMap, DataSet stats, and string manipulation — to 1,446 lines of safe, idiomatic Rust. Score: 100/100, 0 unsafe blocks, 0 repair iterations, byte-exact differential test passing on first attempt.

---

## The Challenge

`expr_eval.c` is the largest and most complex file in our test suite. At 1,686 lines and 74 functions, it implements a complete expression evaluator with:

- **Lexer**: Tokenizes input into numbers, strings, identifiers, and operators
- **Recursive descent parser**: Full operator precedence (unary, multiplicative, additive, comparison, equality, logical AND/OR)
- **Variable store**: `let x = expr;` with lookup by name
- **25+ built-in functions**: `abs()`, `min()`, `max()`, `sqrt()`, `pow()`, `print()`, `len()`, `substr()`, `upper()`, `lower()`, `trim()`, `reverse()`, `index_of()`, `replace()`, `repeat()`, and more
- **String operations**: Concatenation, repetition, comparison, slicing
- **Conditionals**: `if`/`else` blocks
- **While loops**: `while (cond) { ... }`
- **DataSet**: Statistical operations (mean, variance, stddev, median, min, max) with dynamic arrays
- **HashMap**: String-keyed hash map with separate chaining, insertion, lookup, deletion
- **History buffer**: Tracks evaluation results
- **Comprehensive test suite**: 10 test functions covering arithmetic, comparisons, logical ops, variables, strings, builtins, conditionals, complex expressions, DataSet stats, and HashMap operations

This is 3.2x larger than our previous record (cjson_combined.c at 520 LOC) and represents the kind of real-world C codebase complexity that migration tools must handle.

## What Makes This Hard

| C Pattern | Count | Migration Challenge |
|-----------|-------|-------------------|
| `malloc`/`calloc`/`free` | 20+ | Manual memory management → RAII ownership |
| `char[]` / `char *` / `strdup` | 50+ | C strings → `String`/`&str` |
| Pointer arithmetic | 15+ | Raw pointer math → iterators/slices |
| `void *` / type casting | 5+ | Type erasure → Rust enums |
| Fixed-size arrays | 10+ | Stack arrays → `Vec<T>` |
| Linked structures | 2 | Pointer-based → `Vec`-based |
| `qsort` with function pointer | 1 | C callback → closure/`.sort_by()` |
| Global mutable state (VarStore) | 1 | Global → struct field with `&mut self` |
| Union-like Value type | 1 | Tagged union → Rust `enum` |

The `Value` type alone is a classic C migration challenge — it's a tagged union with `double`, `char[]`, and `int` variants, passed by value throughout the codebase. The `VarStore` uses a fixed-size array of `(name, Value)` pairs with linear scan. The `HashMap` implements separate chaining with `malloc`'d nodes. The `DataSet` uses `realloc` for dynamic growth.

## The Migration

Noricum classified this as **Hard** difficulty and routed it to Claude Opus for translation.

### Key Transformations

| C Construct | Rust Equivalent |
|------------|----------------|
| `typedef enum { ... } TokenType;` | `#[derive(Clone, Copy, PartialEq)] enum TokenType { ... }` |
| `typedef struct { TokenType type; double num_val; char str_val[256]; } Token;` | `struct Token { kind: TokenType, num_val: f64, str_val: String }` |
| `Value` tagged union (type + num/str/bool) | `enum Value { Number(f64), Str(String), Bool(bool), None }` |
| `VarStore` with fixed array + linear scan | `HashMap<String, Value>` |
| `malloc`/`realloc`/`free` for DataSet | `Vec<f64>` with `.push()` |
| `HashMap` with linked-list chaining | `HashMap<String, i32>` from std |
| `qsort(data, n, sizeof(double), cmp_double)` | `data.sort_by(\|a, b\| a.partial_cmp(b).unwrap_or(Ordering::Equal))` |
| `char *strdup(s)` + `free(s)` | `s.to_string()` (owned `String`) |
| `sprintf(buf, fmt, ...)` | `format!(...)` |
| `printf(...)` for output | `print!()` / `println!()` |
| `while (cond) { ... }` with pointer iteration | Iterator chains / `for item in &collection` |
| Global `g_parser`, `g_history` | Struct fields passed as `&mut self` |

### The Value Enum — The Core Transformation

The C code uses a tagged union:

```c
typedef struct {
    int type;  // 0=number, 1=string, 2=bool, 3=none
    double num_val;
    char str_val[256];
    int bool_val;
} Value;
```

Noricum transformed this to a proper Rust enum:

```rust
#[derive(Clone, Debug)]
enum Value {
    Number(f64),
    Str(String),
    Bool(bool),
    None,
}
```

This eliminates: reading uninitialized union fields, buffer overflows on `str_val`, type tag mismatches, and the 280+ bytes of wasted space per `Value` instance (Rust's `Value` is 32 bytes).

## Results

| Metric | Value |
|--------|-------|
| Idiomatic Score | **100/100** |
| Unsafe Blocks | **0** |
| Repair Iterations | **0** |
| Diff Test | **PASS** (byte-exact) |
| C Lines | 1,686 |
| Rust Lines | 1,446 |
| LOC Reduction | **14%** |
| Functions | 74 |

### Score Breakdown

- **Base 100**: 0 unsafe blocks, 0 clippy warnings
- **Positive signals**: `enum`, `HashMap`, `Vec`, `String`, `Option`, `Result`, `.iter()`, `impl` blocks, `match` expressions, `format!`
- **LOC ratio bonus**: +5 for producing shorter output than the C source
- **No deductions**: No `.unwrap()` in library logic, no raw `as` casts, no manual indexing

## Differential Testing

Noricum compiled both versions and ran them with identical inputs. The test harness exercises:

1. **Basic arithmetic**: Addition, subtraction, multiplication, division, modulo, negation
2. **Operator precedence**: `2 + 3 * 4` = 14, not 20
3. **Comparisons**: `<`, `>`, `<=`, `>=`, `==`, `!=`
4. **Logical operators**: `&&`, `||`, `!`
5. **Variables**: `let x = 10; x * 2`
6. **String operations**: Concatenation, length, substring, upper/lower, reverse, trim, indexOf, replace, repeat
7. **Built-in functions**: `abs(-5)`, `min(3,7)`, `max(3,7)`, `sqrt(16)`, `pow(2,10)`
8. **Conditionals**: `if (x > 0) { ... } else { ... }`
9. **DataSet statistics**: Mean, variance, stddev, min, max, median
10. **HashMap operations**: Set, get, contains, remove, overwrite

All test outputs matched byte-for-byte between C and Rust.

## What This Demonstrates

1. **Scale is not a barrier.** 1,686 lines with 74 functions, recursive parsing, and multiple data structures — migrated correctly on the first attempt with zero repairs.

2. **Complex data structures translate cleanly.** Tagged unions → enums, malloc'd arrays → Vec, manual hash maps → std::collections::HashMap, pointer-linked structures → Vec-based collections.

3. **The result is genuinely idiomatic.** Score 100/100 means no raw pointers, no unsafe, no .unwrap() abuse, proper use of Rust's type system and ownership model. A Rust developer reviewing this code would not know it was machine-generated.

4. **Behavioral equivalence is proven, not assumed.** The differential test verifies that both implementations produce identical output for identical inputs across 10 distinct test categories.

## Try It Yourself

```bash
git clone https://github.com/JuanMarchetto/noricum
cd noricum
export ANTHROPIC_API_KEY=sk-ant-...
cargo run -p noricum-cli -- migrate tests/fixtures/large/expr_eval.c --diff-test --report expr_eval_report.html
```
