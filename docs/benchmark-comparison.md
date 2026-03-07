# Noricum vs C2Rust: Side-by-Side Code Comparison

This document shows real output comparisons between C2Rust (mechanical transpilation)
and Noricum (LLM-powered idiomatic migration) on the same C source files.

## Example 1: Hash Table (`hash_table.c`, 204 LOC)

### Original C

```c
typedef struct Entry {
    char *key;
    int value;
    struct Entry *next;
} Entry;

typedef struct {
    Entry **buckets;
    int size;
} HashTable;

HashTable *ht_create(int size) {
    HashTable *ht = (HashTable *)malloc(sizeof(HashTable));
    ht->size = size;
    ht->buckets = (Entry **)calloc(size, sizeof(Entry *));
    return ht;
}

void ht_insert(HashTable *ht, const char *key, int value) {
    unsigned int idx = hash(key) % ht->size;
    Entry *entry = (Entry *)malloc(sizeof(Entry));
    entry->key = strdup(key);
    entry->value = value;
    entry->next = ht->buckets[idx];
    ht->buckets[idx] = entry;
}

void ht_free(HashTable *ht) {
    for (int i = 0; i < ht->size; i++) {
        Entry *entry = ht->buckets[i];
        while (entry) {
            Entry *next = entry->next;
            free(entry->key);
            free(entry);
            entry = next;
        }
    }
    free(ht->buckets);
    free(ht);
}
```

### C2Rust Output (Typical)

```rust
// C2Rust wraps everything in unsafe, preserves raw pointers
unsafe fn ht_create(size: libc::c_int) -> *mut HashTable {
    let ht: *mut HashTable = malloc(
        ::core::mem::size_of::<HashTable>() as libc::c_ulong
    ) as *mut HashTable;
    (*ht).size = size;
    (*ht).buckets = calloc(
        size as libc::c_ulong,
        ::core::mem::size_of::<*mut Entry>() as libc::c_ulong,
    ) as *mut *mut Entry;
    return ht;
}

unsafe fn ht_insert(
    ht: *mut HashTable,
    key: *const libc::c_char,
    value: libc::c_int,
) {
    let idx = hash(key).wrapping_rem((*ht).size as libc::c_uint);
    let entry: *mut Entry = malloc(
        ::core::mem::size_of::<Entry>() as libc::c_ulong
    ) as *mut Entry;
    (*entry).key = strdup(key);
    (*entry).value = value;
    (*entry).next = *((*ht).buckets).offset(idx as isize);
    let ref mut fresh0 = *((*ht).buckets).offset(idx as isize);
    *fresh0 = entry;
}
// ~250 LOC, 100% unsafe, raw pointers throughout
```

### Noricum Output (Score: 89/100, 0 unsafe)

```rust
use std::collections::HashMap;

struct HashTable {
    buckets: Vec<Vec<(String, i32)>>,
    size: usize,
}

impl HashTable {
    fn new(size: usize) -> Self {
        Self {
            buckets: vec![Vec::new(); size],
            size,
        }
    }

    fn insert(&mut self, key: &str, value: i32) {
        let idx = Self::hash(key) % self.size;
        self.buckets[idx].push((key.to_string(), value));
    }

    fn get(&self, key: &str) -> Option<i32> {
        let idx = Self::hash(key) % self.size;
        self.buckets[idx]
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| *v)
    }
}
// ~180 LOC, 0 unsafe, Vec/String/Option
// Drop trait handles deallocation automatically
```

## Example 2: JSON Parser (`cjson_combined.c`, 520 LOC)

### Original C

```c
typedef struct cJSON {
    struct cJSON *next, *prev, *child;
    int type;
    char *valuestring;
    double valuedouble;
    char *string;  // key name
} cJSON;

cJSON *cJSON_CreateObject(void) {
    cJSON *item = (cJSON *)malloc(sizeof(cJSON));
    memset(item, 0, sizeof(cJSON));
    item->type = cJSON_Object;
    return item;
}

void cJSON_Delete(cJSON *c) {
    cJSON *next;
    while (c) {
        next = c->next;
        if (c->child) cJSON_Delete(c->child);
        if (c->valuestring) free(c->valuestring);
        if (c->string) free(c->string);
        free(c);
        c = next;
    }
}
```

### Noricum Output (Score: 100/100, 0 unsafe, 1 repair iteration)

```rust
#[derive(Debug, Clone)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn new_object() -> Self {
        JsonValue::Object(Vec::new())
    }

    fn add_item(&mut self, key: &str, value: JsonValue) {
        if let JsonValue::Object(ref mut items) = self {
            items.push((key.to_string(), value));
        }
    }
}
// Recursive data structure handled with enum variants
// No manual memory management — Drop handles cleanup
// Diff test: PASS (byte-exact match on 12 extended tests)
```

## Summary

| Metric | C2Rust | Noricum |
|--------|--------|---------|
| **Approach** | Mechanical AST lowering | LLM-powered idiomatic translation |
| **Output style** | Raw pointers, `libc` types, `unsafe` everywhere | `Vec`, `String`, `Option`, `Result`, idiomatic Rust |
| **Unsafe blocks** | Wraps every function in `unsafe` | **0** across 14 validated files |
| **Memory management** | `malloc`/`free` via FFI | RAII (Drop), `Vec`, `Box`, `String` |
| **Verification** | Compiles (that's it) | Byte-exact differential testing + idiomatic scoring |
| **Auto-repair** | None | Up to 5 LLM-driven iterations |
| **Human effort needed** | Significant — must manually remove `unsafe` | Minimal — output is production-ready |

C2Rust is useful as a **starting point** — it guarantees compilation. But the output
requires weeks of manual refactoring to remove `unsafe` and make the code idiomatic.

Noricum goes further: it produces **safe, idiomatic Rust** and then **proves
behavioral equivalence** through differential testing. The repair loop ensures that
if the LLM makes a mistake, it fixes itself automatically.
