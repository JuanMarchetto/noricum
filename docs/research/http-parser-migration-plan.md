# http-parser Migration Plan

**Target:** nodejs/http-parser (6.4k stars, archived Nov 2022)
**Source:** `http_parser.c` (~2,500 LOC) + `http_parser.h` (~436 LOC) = ~2,936 LOC
**Goal:** 0 unsafe, diff-test byte-exact against C test suite, idiomatic score ≥ 60

## Why This Project

### Impact
- The HTTP parser that powered Node.js for a decade
- 6,400+ GitHub stars, embedded in thousands of projects
- **7+ CVEs** — all memory/parsing safety bugs that Rust prevents by design
- Deprecated in favor of llhttp but still widely used
- **No complete Rust port exists** (httparse is independent, not a behavioral port)

### Known CVEs (Migration Narrative)
| CVE | Severity | Bug Class |
|-----|----------|-----------|
| CVE-2019-15605 | High | HTTP request smuggling (Transfer-Encoding) |
| CVE-2019-15606 | High | Header whitespace bypass |
| CVE-2020-8201 | Medium | CR→hyphen conversion smuggling |
| CVE-2021-22959 | Medium | Space before colon smuggling |
| CVE-2021-22960 | Medium | Chunk extension smuggling |
| CVE-2018-7159 | Medium | Content-Length space parsing (`1 2` → `12`) |

Every one of these is a parsing correctness bug that Rust's strict typing and explicit error handling naturally prevents.

### Technical Fit for Noricum
- **Zero allocations** — no malloc/free to translate (the hardest C→Rust pattern eliminated)
- **Pure state machine** — maps to Rust `enum` + `match`
- **Callback API** — maps to Rust closures/traits
- **Massive test suite** (4,000+ LOC, 500+ effective paths) — excellent diff test material
- **2,936 LOC** — proven range for pipeline (similar to cjson_full at 1,696 LOC)

## Architecture Analysis

### File Structure
```
http_parser.c   (2,500 LOC)  — Implementation
http_parser.h   (436 LOC)    — Public API
test.c          (4,000+ LOC) — Test suite
bench.c         (120 LOC)    — Benchmark
```

### C Source Sections

| Section | Lines | Description |
|---------|-------|-------------|
| Macros + lookup tables | ~310 | Character classification, callback dispatch, string tables |
| `parse_url_char()` | ~90 | URL character state machine |
| `http_parser_execute()` | ~1,125 | **THE CORE** — 58-state switch, 45% of file |
| Utility functions (14) | ~415 | init, keep-alive, method_str, url parsing, pause |

### State Machine (58 states)
```
Dispatch:     s_dead, s_start_req_or_res, s_res_or_resp_H
Response:     s_start_res → s_res_H/HT/HTT/HTTP → s_res_http_major/dot/minor → s_res_status_code → s_res_status
Request:      s_start_req → s_req_method → s_req_spaces_before_url → s_req_schema/.../s_req_fragment → s_req_http_major/dot/minor
Headers:      s_header_field_start → s_header_field → s_header_value → s_headers_done
Body/Chunks:  s_body_identity → s_chunk_size → s_chunk_data → s_message_done
```

### API Surface

**Structs:**
- `http_parser` — bit-packed: type(2), flags(8), state(7), header_state(7), index(5), nread, content_length, http_major/minor, status_code, method, errno, upgrade, `void *data`
- `http_parser_settings` — 10 callback fn pointers
- `http_parser_url` — URL parse result with field_set bitmask

**Enums:**
- `http_parser_type` — REQUEST, RESPONSE, BOTH
- `http_method` — 34 HTTP methods (GET...SOURCE)
- `http_status` — Full HTTP status codes (100-511)
- `http_errno` — ~25 error codes
- `http_parser_url_fields` — SCHEMA, HOST, PORT, PATH, QUERY, FRAGMENT, USERINFO

**Functions (14 public):**
- `http_parser_execute()` — core parse function
- `http_parser_init()`, `http_parser_settings_init()`, `http_parser_url_init()`
- `http_parser_parse_url()`
- `http_should_keep_alive()`, `http_body_is_final()`
- `http_parser_pause()`, `http_parser_version()`
- `http_method_str()`, `http_status_str()`
- `http_errno_name()`, `http_errno_description()`
- `http_parser_set_max_header_size()`

## Rust Design Decisions

### 1. State Machine → Enum + Match
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum ParserState {
    Dead,
    StartReqOrRes,
    // Response states
    StartRes,
    ResH, ResHT, ResHTT, ResHTTP,
    ResHttpMajor, ResHttpDot, ResHttpMinor, ResHttpEnd,
    ResFirstStatusCode, ResStatusCode, ResStatusStart, ResStatus,
    ResLineAlmostDone,
    // Request states
    StartReq, ReqMethod, ReqSpacesBeforeUrl,
    ReqSchema, ReqSchemaSlash, ReqSchemaSlashSlash,
    // ... (58 total)
    MessageDone,
}
```

### 2. Callbacks → Trait or Closures
```rust
/// Option A: Trait-based (most idiomatic)
pub trait HttpParserCallbacks {
    fn on_message_begin(&mut self) -> Result<(), CallbackError> { Ok(()) }
    fn on_url(&mut self, data: &[u8]) -> Result<(), CallbackError> { Ok(()) }
    fn on_status(&mut self, data: &[u8]) -> Result<(), CallbackError> { Ok(()) }
    fn on_header_field(&mut self, data: &[u8]) -> Result<(), CallbackError> { Ok(()) }
    fn on_header_value(&mut self, data: &[u8]) -> Result<(), CallbackError> { Ok(()) }
    fn on_headers_complete(&mut self) -> Result<(), CallbackError> { Ok(()) }
    fn on_body(&mut self, data: &[u8]) -> Result<(), CallbackError> { Ok(()) }
    fn on_message_complete(&mut self) -> Result<(), CallbackError> { Ok(()) }
    fn on_chunk_header(&mut self) -> Result<(), CallbackError> { Ok(()) }
    fn on_chunk_complete(&mut self) -> Result<(), CallbackError> { Ok(()) }
}

/// Option B: Closure-based (closer to C API, better for diff test)
pub struct HttpParserSettings {
    pub on_message_begin: Option<Box<dyn FnMut(&mut HttpParser) -> i32>>,
    pub on_url: Option<Box<dyn FnMut(&mut HttpParser, &[u8]) -> i32>>,
    // ...
}
```

**Decision: Option B (closures)** for initial migration — closer 1:1 mapping to C API makes diff testing easier. Can refactor to trait-based later.

### 3. Bit-Packed Struct → Clean Rust Struct
```rust
pub struct HttpParser {
    pub parser_type: HttpParserType,
    pub state: ParserState,
    pub header_state: HeaderState,
    pub flags: HttpFlags,
    pub index: u8,
    pub nread: u32,
    pub content_length: u64,
    pub http_major: u16,
    pub http_minor: u16,
    pub status_code: u16,
    pub method: HttpMethod,
    pub errno: HttpErrno,
    pub upgrade: bool,
    // void *data → generic or Any
    pub data: Option<Box<dyn std::any::Any>>,
}
```

### 4. Lookup Tables → Const Arrays
```rust
const TOKENS: [u8; 256] = [ /* ... */ ];
const UNHEX: [i8; 256] = [ /* ... */ ];
const NORMAL_URL_CHAR: [u8; 32] = [ /* ... */ ];
```

### 5. Macros → Inline Functions / Methods
```c
// C macro:
#define CALLBACK_DATA(FOR) CALLBACK_DATA_(FOR, p - FOR##_mark, FOR##_mark)

// Rust: method on parser
fn callback_data(&mut self, cb: CallbackType, mark: usize, p: usize) -> Result<(), HttpErrno> { ... }
```

### 6. Error Handling
```rust
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum HttpErrno {
    #[error("success")]
    Ok,
    #[error("invalid EOF state")]
    InvalidEofState,
    #[error("header overflow")]
    HeaderOverflow,
    // ... 25 total
}
```

## Modular Migration Strategy (P3)

The file naturally splits into **4 modules** by function prefix and domain:

### Module 1: `types` (Data Model) — ~200 LOC
- All enums: `HttpParserType`, `HttpMethod`, `HttpStatus`, `HttpErrno`, `ParserState`, `HeaderState`, `HttpFlags`
- All structs: `HttpParser`, `HttpParserSettings`, `HttpParserUrl`, `UrlFieldData`
- All const lookup tables: `TOKENS`, `UNHEX`, `NORMAL_URL_CHAR`, `METHOD_STRINGS`, `STATUS_STRINGS`
- **Difficulty:** EASY — pure data transcription

### Module 2: `url` (URL Parsing) — ~300 LOC
- `parse_url_char()` — URL character state machine
- `http_parse_host_char()` — host character validation
- `http_parse_host()` — host string parser
- `http_parser_parse_url()` — full URL parser
- `http_parser_url_init()` — URL struct initializer
- **Difficulty:** EASY-MEDIUM — small state machines, no callbacks

### Module 3: `parser` (Core State Machine) — ~1,300 LOC
- `http_parser_execute()` — the 1,125-line monster
- `http_message_needs_eof()` — EOF detection
- `http_body_is_final()` — body completion check
- **Difficulty:** HARD — this is the core challenge. The LLM must translate a 1,125-line switch into idiomatic Rust match.
- **Strategy:** Translate as a single function (don't split further — the switch states share mutable state). Let the LLM produce a large match block.

### Module 4: `util` (Utilities) — ~200 LOC
- `http_parser_init()`, `http_parser_settings_init()`
- `http_should_keep_alive()`
- `http_method_str()`, `http_status_str()`
- `http_errno_name()`, `http_errno_description()`
- `http_parser_pause()`, `http_parser_version()`
- `http_parser_set_max_header_size()`
- **Difficulty:** EASY — simple lookup/getter functions

### Migration Order (Dependency-Based)
```
1. types   (no deps)           → compile ✓ → DONE
2. url     (depends on types)  → compile ✓ → DONE
3. util    (depends on types)  → compile ✓ → DONE
4. parser  (depends on all)    → compile ✓ → diff test → DONE
```

## Diff Test Strategy

### Phase 1: Build a C Test Harness
Extract from test.c a subset that produces deterministic output:
```c
// Feed raw HTTP bytes, print parsed method/status/headers/body
int main() {
    // Test requests
    parse_and_print("GET /path HTTP/1.1\r\nHost: example.com\r\n\r\n");
    // Test responses
    parse_and_print("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
    // Test chunked
    parse_and_print("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n");
    // ... 20-30 representative cases
}
```

### Phase 2: Mirror in Rust
Same test cases, same output format → byte-exact comparison.

### Phase 3: Port Full Test Suite
The C test.c has 73+ test cases with byte-boundary splitting. After diff test passes, port the full suite as Rust integration tests.

## Risk Assessment

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| 1,125-line function exceeds LLM context | Medium | P3 modular: translate as single module with full context |
| State machine semantics diverge | Medium | Extensive diff test suite (73+ cases) |
| Callback control flow lost in translation | Low | Closure-based API matches C callbacks closely |
| Performance regression | Low | Not a goal for v1; functional correctness first |
| `HTTP_PARSER_STRICT` mode | Low | Translate strict mode only (non-strict is the CVE source) |

## Success Criteria

- [ ] 0 `unsafe` blocks
- [ ] 0 `unwrap()` in library code
- [ ] Compiles without warnings
- [ ] Diff test byte-exact on ≥ 30 HTTP request/response cases
- [ ] Idiomatic score ≥ 60
- [ ] Handles: GET/POST/PUT/DELETE, chunked encoding, keep-alive, upgrade, status codes 100-511
- [ ] All 7 CVE scenarios handled correctly (strict parsing by default)

## Estimated Pipeline Execution

| Stage | Est. Time | Notes |
|-------|-----------|-------|
| Analysis | ~30s | Pattern detection: state_machine, callback, bit_packed_struct |
| Module 1 (types) | ~1 min | Data transcription |
| Module 2 (url) | ~2 min | Small state machine |
| Module 3 (parser) | ~5 min | The big one — 1,300 LOC module |
| Module 4 (util) | ~1 min | Simple utilities |
| Repair (per module) | ~3 min each | Max 5 iters per module |
| Assembly + validation | ~1 min | Combine + final diff test |
| **Total estimate** | **~20 min** | ~15 LLM calls |

## Preparation Steps

1. Clone http-parser: `git clone https://github.com/nodejs/http-parser.git`
2. Create combined file: `cat http_parser.h http_parser.c > http_parser_combined.c`
3. Build C test: `gcc -std=gnu11 -o test_c test_harness.c http_parser.c -DHTTP_PARSER_STRICT`
4. Write diff test harness (C side)
5. Add fixture: `tests/fixtures/http_parser/`
6. Run: `cargo run -p noricum-cli -- migrate tests/fixtures/http_parser/http_parser_combined.c --diff-test --report`

## Post-Migration Opportunities

- **Blog post:** "The Parser That Powered Node.js, Now in Safe Rust — Automatically"
- **CVE analysis:** Side-by-side showing how each CVE is structurally impossible in the Rust version
- **Performance benchmark:** Compare with httparse on the same HTTP corpus
- **Crate publication:** `http-parser-rs` on crates.io (behavioral port, not just FFI)
