# Case Study: Migrating a C Hash Table to Safe Rust

**TL;DR:** Noricum migrated a 204-line C hash table with `malloc`/`free`, linked-list collision chaining, and manual memory management to safe, idiomatic Rust — scoring 89/100, with 0 unsafe blocks, 0 repairs needed, and byte-exact differential test passing on first attempt.

---

## The Source: `hash_table.c` (204 LOC, 8 functions)

This is a real-world-style hash table implementation featuring patterns that are notoriously difficult to migrate from C to Rust:

| Pattern | C Implementation | Why It's Hard |
|---------|-----------------|---------------|
| Dynamic allocation | `malloc`/`calloc`/`free` | Must map to ownership model |
| Nested pointers | `Entry **buckets` | Double indirection |
| Linked-list chaining | `struct Entry *next` | Self-referential structs |
| String ownership | `strdup(key)` + `free(key)` | Ownership transfer |
| Pointer mutation | Modify `*next` pointers in-place | Borrow checker conflicts |
| Load factor resize | Rehash all entries to new array | Complex ownership transfer |
| Error codes | Return `-1` on failure | Should be `Result`/`Option` |
| NULL checks | `if (!ht) return -1` | Should be type-level guarantees |

### Key Functions

```c
HashTable *ht_create(void);                           // malloc + calloc
int ht_set(HashTable *ht, const char *key, int value); // insert with resize
int ht_get(const HashTable *ht, const char *key, int *out_value); // lookup
int ht_delete(HashTable *ht, const char *key);         // unlink + free
void ht_free(HashTable *ht);                           // walk + free all
static int ht_resize(HashTable *ht, int new_capacity); // rehash
unsigned long hash_key(const char *str);               // DJB2 hash
```

## The Migration

Noricum classified this as **Hard** difficulty (linked-list pointers, manual memory, resize logic) and routed it to Claude Opus for translation.

### What Changed

| C Construct | Rust Equivalent |
|------------|----------------|
| `Entry **buckets` (heap array of pointers) | `Vec<Vec<(String, i32)>>` |
| `malloc`/`calloc`/`free` | Automatic via `Vec`, `String`, RAII |
| `strdup(key)` + later `free(key)` | `key.to_string()` (owned `String`) |
| `struct Entry *next` (linked list) | `Vec<(String, i32)>` per bucket |
| `return -1` / `return 0` error codes | `Option<i32>` for lookups |
| `if (!ht) return -1` NULL guards | Eliminated — `&self` is always valid |
| `ht_free()` manual cleanup | Automatic `Drop` |

### The Core Insight

The entire linked-list collision chain (`Entry *next`) was replaced by `Vec<(String, i32)>` per bucket. This is actually how a `HashMap` works internally, but the Rust version:

- Has **zero raw pointers**
- Has **zero manual memory management**
- Is **memory-safe by construction** (no use-after-free, no double-free, no dangling pointers)
- Preserves the **exact same behavior** (verified by differential testing)

## Results

| Metric | Value |
|--------|-------|
| Idiomatic Score | 89/100 |
| Unsafe Blocks | 0 |
| Repair Iterations | 0 |
| Diff Test | PASS (byte-exact) |
| C Lines | 204 |
| Rust Lines | ~180 |

### What "89/100" Means

The score breakdown:
- **Base 100** (0 unsafe blocks, 0 clippy warnings)
- **Positive signals detected**: `Vec`, `String`, `Option`, iterator patterns
- **Minor deductions**: some raw `as` casts for hash computation, manual indexing in resize

A score of 89 indicates highly idiomatic Rust that a human reviewer would accept without major changes.

## Differential Testing

Noricum compiled both the C original and the generated Rust, ran them with the same test driver (`main()`), and compared outputs byte-by-byte:

```
C output:    size=3 | beta=2 | beta_updated=42 | after_delete=2 | ...
Rust output: size=3 | beta=2 | beta_updated=42 | after_delete=2 | ...
Result: MATCH (byte-exact)
```

This isn't just "it compiles" — it's proof that the Rust code behaves identically to the C code for the same inputs.

## What This Demonstrates

1. **Complex pointer patterns can be migrated safely.** Linked lists, double pointers, and manual memory management all have clean Rust equivalents.

2. **The LLM doesn't need help.** Zero repair iterations means the translation was correct on the first attempt — the analysis agent correctly identified the patterns and the translation agent applied them.

3. **Verification is automatic.** The differential test caught nothing because there was nothing to catch — but if there had been a subtle behavioral difference (common with LLM-generated code), it would have been detected and repaired.

4. **C2Rust alone can't do this.** C2Rust would produce ~250 lines of `unsafe` Rust with raw pointers everywhere. Noricum produces ~180 lines of safe, idiomatic Rust with standard library types.

## Try It Yourself

```bash
git clone https://github.com/JuanMarchetto/noricum
cd noricum
export ANTHROPIC_API_KEY=sk-ant-...
cargo run -p noricum-cli -- migrate tests/fixtures/medium/hash_table.c --diff-test
```
