// Hash table implementation - complex C patterns:
// - Dynamic memory allocation (malloc/realloc/free)
// - Struct with nested pointers
// - Hash function with bit manipulation
// - Collision handling with chaining
// - Error handling via return codes
// - Callback pattern (foreach)
#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define INITIAL_CAPACITY 8
#define LOAD_FACTOR_THRESHOLD 75  // percent

typedef struct Entry {
    char *key;
    int value;
    struct Entry *next;  // collision chain
} Entry;

typedef struct {
    Entry **buckets;
    int capacity;
    int size;
} HashTable;

// DJB2 hash function
unsigned long hash_key(const char *str) {
    unsigned long hash = 5381;
    int c;
    while ((c = *str++))
        hash = ((hash << 5) + hash) + c;
    return hash;
}

HashTable *ht_create(void) {
    HashTable *ht = (HashTable *)malloc(sizeof(HashTable));
    if (!ht) return NULL;
    ht->capacity = INITIAL_CAPACITY;
    ht->size = 0;
    ht->buckets = (Entry **)calloc(ht->capacity, sizeof(Entry *));
    if (!ht->buckets) {
        free(ht);
        return NULL;
    }
    return ht;
}

static int ht_resize(HashTable *ht, int new_capacity) {
    Entry **new_buckets = (Entry **)calloc(new_capacity, sizeof(Entry *));
    if (!new_buckets) return -1;

    // Rehash all entries
    for (int i = 0; i < ht->capacity; i++) {
        Entry *entry = ht->buckets[i];
        while (entry) {
            Entry *next = entry->next;
            unsigned long idx = hash_key(entry->key) % new_capacity;
            entry->next = new_buckets[idx];
            new_buckets[idx] = entry;
            entry = next;
        }
    }
    free(ht->buckets);
    ht->buckets = new_buckets;
    ht->capacity = new_capacity;
    return 0;
}

int ht_set(HashTable *ht, const char *key, int value) {
    if (!ht || !key) return -1;

    // Check load factor, resize if needed
    if (ht->size * 100 / ht->capacity >= LOAD_FACTOR_THRESHOLD) {
        if (ht_resize(ht, ht->capacity * 2) != 0)
            return -1;
    }

    unsigned long idx = hash_key(key) % ht->capacity;

    // Check if key already exists
    Entry *entry = ht->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            entry->value = value;  // update
            return 0;
        }
        entry = entry->next;
    }

    // Insert new entry
    Entry *new_entry = (Entry *)malloc(sizeof(Entry));
    if (!new_entry) return -1;
    new_entry->key = strdup(key);
    if (!new_entry->key) {
        free(new_entry);
        return -1;
    }
    new_entry->value = value;
    new_entry->next = ht->buckets[idx];
    ht->buckets[idx] = new_entry;
    ht->size++;
    return 0;
}

int ht_get(const HashTable *ht, const char *key, int *out_value) {
    if (!ht || !key || !out_value) return -1;

    unsigned long idx = hash_key(key) % ht->capacity;
    Entry *entry = ht->buckets[idx];
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            *out_value = entry->value;
            return 0;
        }
        entry = entry->next;
    }
    return -1;  // not found
}

int ht_delete(HashTable *ht, const char *key) {
    if (!ht || !key) return -1;

    unsigned long idx = hash_key(key) % ht->capacity;
    Entry *entry = ht->buckets[idx];
    Entry *prev = NULL;
    while (entry) {
        if (strcmp(entry->key, key) == 0) {
            if (prev)
                prev->next = entry->next;
            else
                ht->buckets[idx] = entry->next;
            free(entry->key);
            free(entry);
            ht->size--;
            return 0;
        }
        prev = entry;
        entry = entry->next;
    }
    return -1;
}

void ht_free(HashTable *ht) {
    if (!ht) return;
    for (int i = 0; i < ht->capacity; i++) {
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

int main(void) {
    HashTable *ht = ht_create();
    int val;

    // Basic insert and lookup
    ht_set(ht, "alpha", 1);
    ht_set(ht, "beta", 2);
    ht_set(ht, "gamma", 3);
    printf("size=%d\n", ht->size);

    ht_get(ht, "beta", &val);
    printf("beta=%d\n", val);

    // Update existing key
    ht_set(ht, "beta", 42);
    ht_get(ht, "beta", &val);
    printf("beta_updated=%d\n", val);

    // Delete
    ht_delete(ht, "alpha");
    printf("after_delete=%d\n", ht->size);

    // Not found
    int found = ht_get(ht, "alpha", &val);
    printf("alpha_found=%d\n", found);

    // Force resize by adding many entries
    for (int i = 0; i < 20; i++) {
        char key[16];
        sprintf(key, "key_%d", i);
        ht_set(ht, key, i * 10);
    }
    printf("after_bulk=%d\n", ht->size);

    // Verify some bulk entries
    ht_get(ht, "key_0", &val);
    printf("key_0=%d\n", val);
    ht_get(ht, "key_19", &val);
    printf("key_19=%d\n", val);

    ht_free(ht);
    printf("done\n");
    return 0;
}
