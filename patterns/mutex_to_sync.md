# Pattern: pthread_mutex to std::sync::Mutex

## C Pattern
```c
#include <pthread.h>

static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static int shared_counter = 0;

void increment(void) {
    pthread_mutex_lock(&lock);
    shared_counter++;
    pthread_mutex_unlock(&lock);
}

int get_counter(void) {
    int val;
    pthread_mutex_lock(&lock);
    val = shared_counter;
    pthread_mutex_unlock(&lock);
    return val;
}
```

## Rust Equivalent
```rust
use std::sync::Mutex;

static COUNTER: Mutex<i32> = Mutex::new(0);

fn increment() {
    let mut val = COUNTER.lock().unwrap();
    *val += 1;
}

fn get_counter() -> i32 {
    *COUNTER.lock().unwrap()
}
```

## When to Apply
- C code uses `pthread_mutex_t` with `pthread_mutex_lock`/`unlock`
- Global mutable state protected by a mutex
- `pthread_rwlock_t` -> `std::sync::RwLock`
- Key insight: Rust's `Mutex<T>` wraps the data it protects, making it impossible to access without locking. The `MutexGuard` auto-unlocks on drop.
