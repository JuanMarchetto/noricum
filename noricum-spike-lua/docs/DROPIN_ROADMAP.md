# Drop-In Roadmap

Goal: Rust port of Lua 5.4/5.5 is a drop-in replacement of the C
reference. Definition of done:
1. The official Lua 5.4 test suite passes at least as many files
   as the C 5.5 reference does on the same suite (currently 15/31).
2. Real-world programs (penlight, fennel, dkjson, lpeg-using apps)
   produce byte-exact output vs C ref.
3. C consumers can swap `liblua.so` for `libnoricum_lua.so` without
   recompilation and observe the same behaviour.
4. `.luac` bytecode roundtrips byte-for-byte against C reference.
5. Performance within 3x of C ref on bench/* (today: 2-23x).

---

## Tier 1 — Pure-Lua drop-in

Closes the gap for scripts that don't embed C modules.

### 1A. Compat-suite bugs (5/10 → 10/10)
- **comparison-in-expression-position**: `local x = a == b` and
  `assert(a == b, ...)` hang. `==`/`<`/`<=`/etc. emitted as
  ExprDesc::Jmp aren't being materialized into a register when
  used as a value (only when used as `if` condition).
- **coroutine.wrap end signal**: spurious `[0->0]` after the
  iterator body finishes. Wrap callback should return zero
  values when the coroutine is dead.
- **table.sort with comparator + non-trivial key types**: edge
  case in our insertion-sort yields wrong order.
- **JSON encoder hang (09_json_lua.lua)**: recursive table walk
  hits some pathological path. Likely related to either
  comparison-in-expr-pos (via `if t == "..."`) or the new
  multi-assign path consuming registers wrong.
- **string.format %.17g precision**: C uses `%.17g`, Rust
  default is 16 significant digits. Match the 17-digit format.

### 1B. extern "C" API completeness (~50 → ~145 symbols)
Need to expose the full `lua.h` + `lauxlib.h` API:
- `lua_topointer`, `lua_arith`, `lua_concat`, `lua_len`,
  `lua_xmove`, `lua_dump`, `lua_pushfstring`, `lua_pushvfstring`,
  `lua_resume`, `lua_yield`, `lua_status`, `lua_isyieldable`,
  `lua_setupvalue`, `lua_getupvalue`, `lua_upvalueid`,
  `lua_upvaluejoin`, `lua_setuservalue`, `lua_getuservalue`,
  `lua_warning`, `lua_setwarnf`, debug hooks (`lua_sethook`,
  `lua_gethookmask`, etc.).
- `luaL_*` helpers: `luaL_checkudata`, `luaL_testudata`,
  `luaL_argcheck`, `luaL_typeerror`, `luaL_setmetatable`,
  `luaL_getmetatable`, `luaL_getsubtable`, `luaL_requiref`,
  `luaL_buffinit` (Buffer API), `luaL_traceback`,
  `luaL_loadfile`, `luaL_loadbuffer`, `luaL_tolstring` with
  `__tostring` metamethod, `luaL_where`, `luaL_pushresult`.

### 1C. Bytecode `.luac` byte-exact diff
- Generate `.luac` for each of `tests/lua_compat/*.lua` via our
  ldump and via reference `luac`.
- Implement a roundtrip test that dumps via ours, loads via C
  ref, and runs.
- Fix any structural mismatch (LUAC_VERSION, LUAC_FORMAT,
  endianness markers, instruction encoding, constant pool order).

### 1D. Wider real-world coverage
Add these to `tests/lua_compat/`:
- Penlight stdlib utility functions.
- A small Fennel program (Lisp → Lua).
- Middleclass OOP (popular library).
- A 200-line state machine using ipairs + closures.
- A regex/pattern stress test with all Lua pattern features.

---

## Tier 2 — C-embedded drop-in

For applications that link `liblua.so` and expect full semantics.

### 2A. Coroutines via CPS (or OS threads)
Current Result-threading model can't yield through C frames.
Pick one:
- **CPS rewrite**: VM dispatch loop becomes resumable. Every
  blocking call returns a continuation. Most faithful but
  invasive (~2k LOC churn in lvm.rs).
- **OS threads**: each coroutine = a Rust thread + channel pair.
  Simple, correct, ~1 OS thread per coroutine (cost: a few KB
  per thread).

Recommendation: OS threads first (week 1), CPS later if perf
demands it.

### 2B. GC v2: weak tables + finalizers in sweep
Today `__mode` and `__gc` only fire on explicit
`collectgarbage("collect")`. Need:
- Mark phase honors weak-mode (skip marking weak refs).
- Atomic phase drains weak tables of dead entries.
- Sweep phase routes objects with `__gc` through a
  finalization queue before freeing.
- Generational mode (Lua 5.4 default): nursery + tenured,
  young-collection vs full.

Substantial GC work. ~4-5 sessions.

### 2C. Dynamic C module loading
`require("lpeg")` etc. Need:
- `dlopen`/`dlsym` shim (Linux/macOS) and `LoadLibraryA`/`GetProcAddress` (Windows).
- `package.cpath` honored for `.so`/`.dll` discovery.
- Convention: lookup `luaopen_<modname>` symbol, call with `lua_State*`.
- Edge cases: nested `.` paths, custom searchers, sentinel.

### 2D. Precise int/float comparisons
C Lua 5.4's `LTintfloat` / `LTfloatint` near `i64::MAX` use
careful range checks our naive `as f64` cast misses. Port
`LTintfloat` from `lvm.c` directly.

### 2E. io completeness
- `io.open(path, "rb")` / `"wb"` properly binary-mode.
- `read("L")` keeps the trailing newline.
- `seek("cur", -n)` with negative offsets.
- `popen` for process pipes.
- Proper `__tostring` on file handles ("file (closed)" / "file (0x...)").

### 2F. os.date with full tzdata
Parse `/etc/localtime` (TZif binary format) for proper local-time
support. Without this, `os.date("*t")` reports UTC even when the
user expects local time.

---

## Tier 3 — Performance parity

Target: within 3x of C ref on bench/*.

### 3A. Direct-threaded dispatch
Today the VM is `match opcode { ... }`. Rust 1.x doesn't have
computed-goto, but tail-call dispatch (one fn per opcode that
tail-calls into the next) compiles to similar code on LLVM with
`#[inline(always)]` + explicit `become`. ~3x speedup expected on
fib/sieve.

### 3B. NaN-boxed TValue
Today `TValue` is an enum (16 bytes payload + 8 byte tag). C Lua
uses NaN-boxing to fit type+value in 8 bytes. ~2x speedup on
arithmetic-heavy code.

### 3C. Specialized opcodes
Lua's `OP_ADDII`, `OP_ADDIF`, etc. paths assume integer/float
without re-checking the tag every time. Our paths re-discriminate.
Fast-path the common case.

### 3D. Inline-cache for `__index` chain
Today every `OP_GETTABLE` walks the metatable chain. Cache the
last successful path on the call site. ~30% speedup on OO code.

---

## Sequencing

The plan executes in this order to maximize blast radius per hour:

1. **1A** (4-6 hours, this session) — unblocks 5 more compat tests, surfaces hidden bugs.
2. **1B** (8-12 hours) — every C consumer becomes viable.
3. **1C** (4 hours) — proves bytecode-level fidelity.
4. **1D** (4 hours) — lateral test coverage.
5. **2A** (8-12 hours) — flagship feature parity.
6. **2B** (16-20 hours) — long-running app semantic parity.
7. **2C** (4-6 hours) — popular C-module ecosystem.
8. **2D**/**2E**/**2F** (8 hours total) — corner-case parity.
9. **Tier 3** (parallel optional) — competitive performance.

Estimated total: 60-90 hours of focused work. We're at ~40% drop-in
today (FFI works, simple programs run, but bugs surface in real apps).
After Tier 1: ~80%. After Tier 2: ~98%. Tier 3 is optional.
