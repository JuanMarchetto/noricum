# Pattern: Linked List Traversal to Vec/Iterator

## C Pattern
```c
struct node {
    int data;
    struct node *next;
};

struct node *create(int val) {
    struct node *n = (struct node *)malloc(sizeof(struct node));
    n->data = val;
    n->next = NULL;
    return n;
}

void append(struct node **head, int val) {
    struct node *new_node = create(val);
    if (!*head) { *head = new_node; return; }
    struct node *cur = *head;
    while (cur->next) cur = cur->next;
    cur->next = new_node;
}

void print_list(struct node *head) {
    struct node *cur = head;
    while (cur) {
        printf("%d ", cur->data);
        cur = cur->next;
    }
    printf("\n");
}

void free_list(struct node *head) {
    while (head) {
        struct node *tmp = head;
        head = head->next;
        free(tmp);
    }
}
```

## Rust Equivalent
```rust
fn append(list: &mut Vec<i32>, val: i32) {
    list.push(val);
}

fn print_list(list: &[i32]) {
    for val in list {
        print!("{} ", val);
    }
    println!();
}
// No free_list needed — Vec drops automatically
```

## When to Apply
- C code defines a singly-linked list with malloc/free
- List is used for sequential access (no random insert/delete in the middle)
- Traversal patterns like `while (cur) { ... cur = cur->next; }`
- Key insight: most C linked lists are better represented as `Vec<T>` in Rust
