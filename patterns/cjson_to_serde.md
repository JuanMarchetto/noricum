# Pattern: cJSON to Idiomatic Rust

## C Pattern
```c
#include "cJSON.h"

cJSON *obj = cJSON_CreateObject();
cJSON_AddStringToObject(obj, "name", "noricum");
cJSON_AddNumberToObject(obj, "version", 1);
char *json = cJSON_PrintUnformatted(obj);
printf("%s\n", json);
free(json);
cJSON_Delete(obj);

cJSON *parsed = cJSON_Parse("{\"key\":\"value\"}");
const char *val = cJSON_GetObjectItem(parsed, "key")->valuestring;
cJSON_Delete(parsed);
```

## Rust Equivalent
```rust
use std::collections::HashMap;

enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

// Creation — no manual free needed (RAII)
let obj = JsonValue::Object(vec![
    ("name".to_string(), JsonValue::Str("noricum".to_string())),
    ("version".to_string(), JsonValue::Number(1.0)),
]);
println!("{}", format_json(&obj));

// Parsing returns Result, not NULL
let parsed = parse_json(r#"{"key":"value"}"#)?;
let val = get_object_item(&parsed, "key")
    .and_then(|v| v.as_str())
    .unwrap_or("");
```

## Key Mappings
| cJSON | Idiomatic Rust |
|-------|----------------|
| `cJSON_CreateObject()` | `JsonValue::Object(Vec::new())` |
| `cJSON_AddStringToObject(obj, k, v)` | `.push((k, JsonValue::Str(v)))` |
| `cJSON_AddNumberToObject(obj, k, n)` | `.push((k, JsonValue::Number(n)))` |
| `cJSON_Parse(s)` | `parse_json(s) -> Result<JsonValue, String>` |
| `cJSON_GetObjectItem(obj, k)` | `get_object_item(&obj, k) -> Option<&JsonValue>` |
| `cJSON_GetArrayItem(arr, i)` | `arr[i]` on `Vec<JsonValue>` |
| `cJSON_GetArraySize(arr)` | `.len()` on `Vec<JsonValue>` |
| `cJSON_PrintUnformatted(obj)` | `format_json(&obj) -> String` |
| `cJSON_Delete(obj)` | (automatic — Drop) |
| `free(json_str)` | (automatic — Drop) |

## Notes
- cJSON requires manual `cJSON_Delete()` for every allocated object — Rust handles this via RAII
- cJSON returns NULL on parse failure — use `Result` in Rust
- Array access in cJSON walks a linked list (O(n)) — Vec gives O(1) index
- Recursive data structures use `enum` with `Box`/`Vec` — no raw pointer graphs
- No external crate needed — hand-rolled `JsonValue` enum suffices for cJSON semantics
