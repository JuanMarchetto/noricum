# Behavioral Equivalence Review Report

**Date:** 2026-03-07
**Tool:** Noricum `review` command with Claude LLM
**Method:** 6-phase behavioral analysis + diff testing
**Total pairs reviewed:** 14

## Aggregate Statistics

| Metric | Value |
|--------|-------|
| EQUIVALENT | 5 (36%) |
| LIKELY EQUIVALENT | 3 (21%) |
| DIVERGENT | 6 (43%) |
| INSUFFICIENT DATA | 0 (0%) |
| **Average Confidence** | **94.4%** |
| Total C LOC | 2858 |
| Total Rust LOC | 2353 |

## Summary Table

| # | Name | Category | C LOC | Rust LOC | Verdict | Confidence |
|---|------|----------|-------|----------|---------|------------|
| 1 | add | Simple | 13 | 9 | EQUIVALENT | 99% |
| 2 | buffer | Simple | 36 | 57 | DIVERGENT | 95% |
| 3 | cjson_combined | Complex | 520 | 320 | EQUIVALENT | 95% |
| 4 | error_codes | Simple | 43 | 33 | DIVERGENT | 95% |
| 5 | expr_eval | Complex | 1686 | 1447 | EQUIVALENT | 95% |
| 6 | factorial | Simple | 18 | 10 | DIVERGENT | 95% |
| 7 | fibonacci | Simple | 30 | 29 | EQUIVALENT | 99% |
| 8 | gcd | Simple | 24 | 20 | EQUIVALENT | 98% |
| 9 | hash_table | Medium | 203 | 173 | LIKELY EQUIVALENT | 85% |
| 10 | linked_list | Simple | 50 | 36 | LIKELY EQUIVALENT | 85% |
| 11 | max_min | Simple | 31 | 21 | LIKELY EQUIVALENT | 95% |
| 12 | miniz_test | Complex | 153 | 155 | DIVERGENT | 95% |
| 13 | power | Simple | 33 | 34 | DIVERGENT | 95% |
| 14 | strlen | Simple | 18 | 9 | DIVERGENT | 95% |

## Simple Files

### add

- **C source:** `tests/fixtures/simple/add.c` (13 LOC)
- **Rust source:** `output/simple/add.rs` (9 LOC)
- **Verdict:** EQUIVALENT
- **Confidence:** 99%

**Summary:** ---

### buffer

- **C source:** `tests/fixtures/simple/buffer.c` (36 LOC)
- **Rust source:** `output/simple/buffer.rs` (57 LOC)
- **Verdict:** DIVERGENT
- **Confidence:** 95%

**Summary:** While the Rust translation produces identical output for the given test case, it contains a **critical behavioral divergence** in error handling that fundamentally changes the program's observable behavior under error conditions. The C version continues execution after buffer overflow (returning -1), while the Rust version panics and terminates the program.

### error_codes

- **C source:** `tests/fixtures/simple/error_codes.c` (43 LOC)
- **Rust source:** `output/simple/error_codes.rs` (33 LOC)
- **Verdict:** DIVERGENT
- **Confidence:** 95%

**Summary:** This is **not** a faithful translation. While the diff test passes for the specific inputs in `main()`, the Rust implementation has fundamentally different semantics that would produce divergent behavior for many other inputs. The migration from C's error-code-with-output-parameter pattern to Rust's `Result` type has introduced several behavioral breaks.

### factorial

- **C source:** `tests/fixtures/simple/factorial.c` (18 LOC)
- **Rust source:** `output/simple/factorial.rs` (10 LOC)
- **Verdict:** DIVERGENT
- **Confidence:** 95%

**Summary:** ---

### fibonacci

- **C source:** `tests/fixtures/simple/fibonacci.c` (30 LOC)
- **Rust source:** `output/simple/fibonacci.rs` (29 LOC)
- **Verdict:** EQUIVALENT
- **Confidence:** 99%

**Summary:** ---

### gcd

- **C source:** `tests/fixtures/simple/gcd.c` (24 LOC)
- **Rust source:** `output/simple/gcd.rs` (20 LOC)
- **Verdict:** EQUIVALENT
- **Confidence:** 98%

**Summary:** ---

### linked_list

- **C source:** `tests/fixtures/simple/linked_list.c` (50 LOC)
- **Rust source:** `output/simple/linked_list.rs` (36 LOC)
- **Verdict:** LIKELY EQUIVALENT
- **Confidence:** 85%

**Summary:** This is a high-quality translation that preserves the core behavioral contract of the C linked list implementation. The Rust version correctly implements the same linked list operations with equivalent observable behavior for the tested scenario. However, there are fundamental differences in error handling and memory management that could lead to behavioral divergence under specific failure conditions not covered by the current test.

### max_min

- **C source:** `tests/fixtures/simple/max_min.c` (31 LOC)
- **Rust source:** `output/simple/max_min.rs` (21 LOC)
- **Verdict:** LIKELY EQUIVALENT
- **Confidence:** 95%

**Summary:** This is a high-quality faithful translation. The Rust code uses idiomatic standard library methods (`i32::max`, `i32::min`, `i32::clamp`) that implement the exact same mathematical operations as the C ternary and if/else logic. The type mapping is correct, format strings are equivalent, and the diff test confirms identical output. The only minor uncertainty comes from relying on Rust standard library implementations rather than explicit logic translation, but these are well-defined mathematical operations with no edge cases in the tested domain.

### power

- **C source:** `tests/fixtures/simple/power.c` (33 LOC)
- **Rust source:** `output/simple/power.rs` (34 LOC)
- **Verdict:** DIVERGENT
- **Confidence:** 95%

**Summary:** The Rust translation contains a **critical behavioral divergence** in the `power` function. While the diff test passes for the specific inputs tested, the Rust version adds negative exponent handling that does not exist in the C version, fundamentally changing the function's behavior for a significant class of inputs.

### strlen

- **C source:** `tests/fixtures/simple/strlen.c` (18 LOC)
- **Rust source:** `output/simple/strlen.rs` (9 LOC)
- **Verdict:** DIVERGENT
- **Confidence:** 95%

**Summary:** This is **not** a faithful translation. While the diff test passes for the specific string literals in `main()`, the Rust implementation fundamentally changes the function's contract and type semantics, creating behavioral divergence in multiple dimensions.

## Medium Files

### hash_table

- **C source:** `tests/fixtures/medium/hash_table.c` (203 LOC)
- **Rust source:** `output/medium/hash_table.rs` (173 LOC)
- **Verdict:** LIKELY EQUIVALENT
- **Confidence:** 85%

**Summary:** The Rust translation is a largely faithful implementation of the C hash table, producing identical output for the test cases. However, there are subtle behavioral differences in error handling, memory allocation failure paths, and null pointer handling that could cause divergence under specific conditions not covered by the test harness.

## Complex Files

### cjson_combined

- **C source:** `tests/fixtures/cjson/cjson_combined.c` (520 LOC)
- **Rust source:** `output/cjson/cjson_combined.rs` (320 LOC)
- **Verdict:** EQUIVALENT
- **Confidence:** 95%

**Summary:** The Rust implementation is a behaviorally faithful translation of the C cJSON library subset. The code correctly preserves all observable behavior including JSON creation, parsing, printing, and object/array manipulation. The diff test confirms byte-for-byte identical output, and my analysis finds no behavioral divergence in the covered functionality.

### expr_eval

- **C source:** `tests/fixtures/large/expr_eval.c` (1686 LOC)
- **Rust source:** `output/large/expr_eval.rs` (1447 LOC)
- **Verdict:** EQUIVALENT
- **Confidence:** 95%

**Summary:** This is a highly faithful translation of a complex expression evaluator from C to Rust. The Rust implementation preserves all observable behavior including lexing, parsing, evaluation semantics, operator precedence, type coercion, string operations, and output formatting. The byte-for-byte identical output across all test cases, combined with careful preservation of C's semantic quirks (like integer modulo, boolean representation, and floating-point formatting), demonstrates exceptional behavioral equivalence.

### miniz_test

- **C source:** `tests/fixtures/miniz/miniz_test.c` (153 LOC)
- **Rust source:** `output/miniz/miniz_test.rs` (155 LOC)
- **Verdict:** DIVERGENT
- **Confidence:** 95%

**Summary:** While the diff test passes, the Rust translation contains a critical algorithmic error in the `mz_adler32` function that produces incorrect results for inputs larger than 5552 bytes. The block size calculation logic differs from the C implementation, causing behavioral divergence on larger inputs not covered by the test harness.

## Confidence Distribution

| Range | Count |
|-------|-------|
| 90-100% | 12 ############ |
| 80-89% | 2 ## |
| 70-79% | 0  |
| 60-69% | 0  |
| <60% | 0  |

## Methodology Notes

Each pair was reviewed using Noricum's behavioral equivalence agent which performs:
1. **Structural mapping** - function-by-function correspondence
2. **Type semantics** - integer width, signedness, bool-as-int checks
3. **Control flow equivalence** - branching, loops, error paths
4. **Standard library mapping** - printf formats, malloc/free, string ops
5. **Edge cases & UB** - overflow, null pointers, uninitialized memory
6. **Output equivalence** - byte-for-byte stdout, exit codes

Diff tests were run for each pair, providing empirical validation alongside the LLM analysis.

