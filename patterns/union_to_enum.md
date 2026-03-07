# Pattern: Tagged Union to Rust Enum

## C Pattern
```c
typedef enum { TYPE_INT, TYPE_FLOAT, TYPE_STRING } ValueType;

typedef struct {
    ValueType type;
    union {
        int i;
        double f;
        char *s;
    } data;
} Value;

void print_value(const Value *v) {
    switch (v->type) {
        case TYPE_INT:    printf("%d", v->data.i); break;
        case TYPE_FLOAT:  printf("%f", v->data.f); break;
        case TYPE_STRING: printf("%s", v->data.s); break;
    }
}
```

## Rust Equivalent
```rust
enum Value {
    Int(i32),
    Float(f64),
    Str(String),
}

fn print_value(v: &Value) {
    match v {
        Value::Int(i) => print!("{}", i),
        Value::Float(f) => print!("{}", f),
        Value::Str(s) => print!("{}", s),
    }
}
```

## When to Apply
- C code uses a struct with a type tag enum + union
- Switch statements that dispatch on the type tag
- The union fields are accessed only after checking the type tag
- Key insight: Rust enums with data replace the tag+union pattern with compile-time safety
