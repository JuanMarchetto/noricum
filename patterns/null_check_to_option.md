# Pattern: NULL Check to Option

## C Pattern
```c
typedef struct Node {
    int value;
    struct Node *next;
} Node;

Node *find_node(Node *head, int target) {
    Node *current = head;
    while (current != NULL) {
        if (current->value == target) {
            return current;
        }
        current = current->next;
    }
    return NULL;
}

void use_result(Node *head) {
    Node *found = find_node(head, 42);
    if (found != NULL) {
        printf("Found: %d\n", found->value);
    } else {
        printf("Not found\n");
    }
}
```

## Rust Equivalent
```rust
fn find_node(list: &[i32], target: i32) -> Option<usize> {
    list.iter().position(|&v| v == target)
}

fn use_result(list: &[i32]) {
    match find_node(list, 42) {
        Some(idx) => println!("Found: {}", list[idx]),
        None => println!("Not found"),
    }
}
```

## When to Apply
- C function returns a pointer that may be NULL
- Caller checks `if (ptr != NULL)` or `if (ptr)` before use
- Functions use NULL as a sentinel value for "not found" or "error"
- Key insight: `T *` that can be NULL maps to `Option<T>` or `Option<&T>`
