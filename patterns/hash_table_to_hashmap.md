# Pattern: Hash Table to HashMap

Manual linked-list hash table implementation → Rust `HashMap`.

## C Pattern

```c
typedef struct Entry {
    char *key;
    int value;
    struct Entry *next;
} Entry;

typedef struct {
    Entry **buckets;
    int capacity;
    int size;
} HashTable;

HashTable *ht_create(void) {
    HashTable *ht = (HashTable *)malloc(sizeof(HashTable));
    ht->buckets = (Entry **)calloc(capacity, sizeof(Entry *));
    return ht;
}

int ht_set(HashTable *ht, const char *key, int value) {
    unsigned long idx = hash(key) % ht->capacity;
    Entry *entry = ht->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            entry->value = value;
            return 0;
        }
        entry = entry->next;
    }
    // ... insert new entry with malloc + strdup
}

int ht_get(const HashTable *ht, const char *key, int *out) {
    // ... linear search in bucket chain
}

void ht_free(HashTable *ht) {
    // ... free all entries, keys, buckets, then ht
}
```

## Rust Equivalent

```rust
use std::collections::HashMap;

fn main() {
    let mut map: HashMap<String, i32> = HashMap::new();

    // Insert
    map.insert("key".to_string(), 42);

    // Get
    if let Some(&value) = map.get("key") {
        println!("found: {}", value);
    }

    // Update
    map.insert("key".to_string(), 99);

    // Remove
    map.remove("key");

    // Size
    println!("size: {}", map.len());

    // Drop is automatic - no manual free needed
}
```

## When to Apply

- C code defines a struct with `key`, `value`, and `next` pointer (linked list chaining)
- Uses `malloc`/`calloc` for bucket arrays and entries
- Implements hash, set, get, delete, free functions manually
- Has collision handling via chaining (linked list per bucket)
- Key insight: the entire data structure maps to a single `HashMap<K, V>`
