# Pattern: cJSON to serde_json

## C Pattern
Manual JSON manipulation using cJSON library: `cJSON_CreateObject`, `cJSON_AddStringToObject`,
`cJSON_Parse`, `cJSON_GetObjectItem`, `cJSON_Delete`, `free(json_string)`.

## Rust Pattern
Use `serde_json` crate with `serde_json::json!{}` macro for creation,
`serde_json::from_str` for parsing, and automatic memory management.

## C Example
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

## Rust Example
```rust
use serde_json::{json, Value};

let obj = json!({
    "name": "noricum",
    "version": 1
});
println!("{}", obj.to_string());

let parsed: Value = serde_json::from_str(r#"{"key":"value"}"#).unwrap();
let val = parsed["key"].as_str().unwrap();
```

## Key Mappings
| cJSON | serde_json |
|-------|------------|
| `cJSON_CreateObject()` | `json!({})` or `Value::Object(Map::new())` |
| `cJSON_AddStringToObject(obj, k, v)` | `obj[k] = json!(v)` |
| `cJSON_AddNumberToObject(obj, k, n)` | `obj[k] = json!(n)` |
| `cJSON_Parse(s)` | `serde_json::from_str(s)` |
| `cJSON_GetObjectItem(obj, k)` | `obj[k]` or `obj.get(k)` |
| `cJSON_GetArrayItem(arr, i)` | `arr[i]` |
| `cJSON_GetArraySize(arr)` | `arr.as_array().unwrap().len()` |
| `cJSON_PrintUnformatted(obj)` | `obj.to_string()` |
| `cJSON_Delete(obj)` | (automatic — Drop) |
| `free(json_str)` | (automatic — Drop) |

## Notes
- cJSON requires manual `cJSON_Delete()` for every allocated object — Rust handles this via RAII
- cJSON returns NULL on parse failure — use `Result` in Rust
- Array access in cJSON walks a linked list (O(n)) — serde_json uses Vec (O(1) index)
