//! Lua 5.4 type contract — the data-structure vocabulary for the full port.
//!
//! This module defines the shape of every internal Lua type. Behavior comes
//! later, module by module. The contract exists so that downstream modules
//! (`lstate`, `lvm`, `lapi`, …) reference stable type names from the first
//! commit instead of racing to define them.
//!
//! Ground truth is `lobject.h`, `lstate.h`, `lua.h`, and `llimits.h` in this
//! crate's root. Piccolo (`/tmp/piccolo-ref`) was read for inspiration on
//! Rust-side shape only — no code is copied.
//!
//! Design notes:
//!
//! * Heap-allocated Lua objects live in per-type arenas on the [`Heap`] and
//!   are referenced by plain `u32` index handles (see [`StringHandle`],
//!   [`TableHandle`], etc.). This is our zero-dep replacement for
//!   `gc-arena::Gc`. Generation counters will be added in Stage 3 when the
//!   GC module lands — until then, use-after-free is a plan-doc risk (R1).
//! * [`TValue`] is a native Rust enum. The drop-in `extern "C"` ABI never
//!   exposes it — C callers push/pop values via `lua_push*`/`lua_to*`, so
//!   the internal representation is unconstrained.
//! * Every fallible Lua operation returns [`LuaResult<T>`]. This is the
//!   Result-threading error model (lock-in decision 2). No `catch_unwind`,
//!   no panic-as-error. Calls that escape protected mode at the drop-in
//!   ABI boundary translate `Err(..)` into the `lua_pcall` status word.

#![allow(dead_code)]

use std::collections::HashMap;
use std::os::raw::{c_int, c_void};

// ---------------------------------------------------------------------------
// Primitive type aliases — match `luaconf.h` defaults for 64-bit builds.
// ---------------------------------------------------------------------------

/// Signed integer Lua uses for its integer numeric subtype.
/// `luaconf.h` default: `long long` (== `i64`).
pub type LuaInteger = i64;

/// Lua's floating-point numeric subtype.
/// `luaconf.h` default: `double` (== `f64`).
pub type LuaNumber = f64;

/// Unsigned counterpart of [`LuaInteger`], used by bitwise operators and
/// `string.pack` width codes.
pub type LuaUnsigned = u64;

/// A single VM instruction word. `lobject.h`: `typedef l_uint32 Instruction;`.
pub type Instruction = u32;

/// `lu_byte` in `llimits.h`.
pub type LuByte = u8;

/// `ls_byte` in `llimits.h`.
pub type LsByte = i8;

/// Raw C function pointer — the `lua_CFunction` type exported at the drop-in
/// ABI boundary. Signature must match `lua.h`:
/// `typedef int (*lua_CFunction) (lua_State *L);`.
pub type RawCFunction = unsafe extern "C" fn(state: *mut LuaState) -> c_int;

// ---------------------------------------------------------------------------
// Type tags — copied from `lua.h`. `makevariant(t, v)` is `t | (v << 4)`.
// ---------------------------------------------------------------------------

pub const LUA_TNIL: u8 = 0;
pub const LUA_TBOOLEAN: u8 = 1;
pub const LUA_TLIGHTUSERDATA: u8 = 2;
pub const LUA_TNUMBER: u8 = 3;
pub const LUA_TSTRING: u8 = 4;
pub const LUA_TTABLE: u8 = 5;
pub const LUA_TFUNCTION: u8 = 6;
pub const LUA_TUSERDATA: u8 = 7;
pub const LUA_TTHREAD: u8 = 8;
pub const LUA_NUMTYPES: u8 = 9;

pub const LUA_VNIL: u8 = LUA_TNIL;
pub const LUA_VFALSE: u8 = LUA_TBOOLEAN;
pub const LUA_VTRUE: u8 = LUA_TBOOLEAN | (1 << 4);
pub const LUA_VNUMINT: u8 = LUA_TNUMBER;
pub const LUA_VNUMFLT: u8 = LUA_TNUMBER | (1 << 4);
pub const LUA_VSHRSTR: u8 = LUA_TSTRING;
pub const LUA_VLNGSTR: u8 = LUA_TSTRING | (1 << 4);
pub const LUA_VLCL: u8 = LUA_TFUNCTION;
pub const LUA_VLCF: u8 = LUA_TFUNCTION | (1 << 4);
pub const LUA_VCCL: u8 = LUA_TFUNCTION | (2 << 4);

// ---------------------------------------------------------------------------
// Arena handles. One `u32` per heap-allocated object.
// ---------------------------------------------------------------------------

/// Handle pointing to a [`LuaString`] slot in [`Heap::strings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StringHandle(pub u32);

/// Handle pointing to a [`Table`] slot in [`Heap::tables`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableHandle(pub u32);

/// Handle pointing to a [`Proto`] slot in [`Heap::protos`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProtoHandle(pub u32);

/// Handle pointing to an [`LClosure`] slot in [`Heap::lclosures`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LClosureHandle(pub u32);

/// Handle pointing to a [`CClosure`] slot in [`Heap::cclosures`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CClosureHandle(pub u32);

/// Handle pointing to an [`UpVal`] slot in [`Heap::upvals`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpValHandle(pub u32);

/// Handle pointing to a [`Thread`] slot in [`Heap::threads`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadHandle(pub u32);

/// Handle pointing to a [`UserData`] slot in [`Heap::userdata`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserDataHandle(pub u32);

// ---------------------------------------------------------------------------
// TValue — the single data type for all Lua values at runtime.
// ---------------------------------------------------------------------------

/// Tagged Lua value. Corresponds to `TValue` in `lobject.h` but uses a
/// Rust enum rather than the C tagged-union trick.
///
/// * `Nil`, `False`, `True` are distinct variants so the interpreter can
///   dispatch on them without wrapping a `bool`.
/// * Short and long strings are distinct variants because Lua uses that
///   distinction for the interning table and the `==` fast path.
/// * `Function` has three flavors matching Lua's `LUA_VLCL`, `LUA_VLCF`,
///   and `LUA_VCCL`: full Lua closure, light C function (bare pointer),
///   and C closure (function + captured upvalues).
#[derive(Debug, Clone, Copy, Default)]
pub enum TValue {
    #[default]
    Nil,
    False,
    True,
    LightUserData(*mut c_void),
    Integer(LuaInteger),
    Number(LuaNumber),
    ShortString(StringHandle),
    LongString(StringHandle),
    Table(TableHandle),
    LuaClosure(LClosureHandle),
    LightCFunction(RawCFunction),
    CClosure(CClosureHandle),
    UserData(UserDataHandle),
    Thread(ThreadHandle),
}

impl TValue {
    /// Lua-visible type name as returned by `type()` and `lua_typename()`.
    pub const fn type_name(&self) -> &'static str {
        match self {
            TValue::Nil => "nil",
            TValue::False | TValue::True => "boolean",
            TValue::LightUserData(_) | TValue::UserData(_) => "userdata",
            TValue::Integer(_) | TValue::Number(_) => "number",
            TValue::ShortString(_) | TValue::LongString(_) => "string",
            TValue::Table(_) => "table",
            TValue::LuaClosure(_) | TValue::LightCFunction(_) | TValue::CClosure(_) => "function",
            TValue::Thread(_) => "thread",
        }
    }

    /// Lua 5.4 variant tag — the `tt_` byte the C VM dispatches on.
    pub const fn variant_tag(&self) -> u8 {
        match self {
            TValue::Nil => LUA_VNIL,
            TValue::False => LUA_VFALSE,
            TValue::True => LUA_VTRUE,
            TValue::LightUserData(_) => LUA_TLIGHTUSERDATA,
            TValue::Integer(_) => LUA_VNUMINT,
            TValue::Number(_) => LUA_VNUMFLT,
            TValue::ShortString(_) => LUA_VSHRSTR,
            TValue::LongString(_) => LUA_VLNGSTR,
            TValue::Table(_) => LUA_TTABLE,
            TValue::LuaClosure(_) => LUA_VLCL,
            TValue::LightCFunction(_) => LUA_VLCF,
            TValue::CClosure(_) => LUA_VCCL,
            TValue::UserData(_) => LUA_TUSERDATA,
            TValue::Thread(_) => LUA_TTHREAD,
        }
    }

    /// Truthiness as Lua sees it: only `nil` and `false` are false.
    pub const fn is_truthy(&self) -> bool {
        !matches!(self, TValue::Nil | TValue::False)
    }
}

// ---------------------------------------------------------------------------
// TableKey — the subset of TValue that can appear as a table key.
// ---------------------------------------------------------------------------

/// Valid Lua table keys. Matches the runtime check in `ltable.c` that
/// rejects `nil` and `NaN`. Float keys that are exactly-representable
/// integers are normalized to `Integer` when inserted.
///
/// `Hash` + `Eq` are derivable because the `Number` variant stores the
/// raw `u64` bit pattern of the `f64`, bypassing `f64`'s missing trait
/// impls. NaN is rejected at insertion time, so bitwise equality is safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableKey {
    False,
    True,
    Integer(LuaInteger),
    /// Raw `u64` bit pattern of the `f64`. NaN is forbidden upstream.
    Number(u64),
    LightUserData(usize),
    ShortString(StringHandle),
    LongString(StringHandle),
    Table(TableHandle),
    LuaClosure(LClosureHandle),
    LightCFunction(usize),
    CClosure(CClosureHandle),
    UserData(UserDataHandle),
    Thread(ThreadHandle),
}

// ---------------------------------------------------------------------------
// Heap object definitions.
// ---------------------------------------------------------------------------

/// Lua string. Lua strings are arbitrary byte sequences, NOT guaranteed
/// UTF-8 — we carry a `Vec<u8>`, not a `String`.
///
/// Short strings (`shrlen >= 0` in C) live in the interning table and
/// compare by handle identity. Long strings hash lazily and compare
/// by content.
#[derive(Debug, Clone)]
pub struct LuaString {
    pub bytes: Vec<u8>,
    pub hash: u32,
    /// Reserved-word marker for the lexer (`TString::extra` in C).
    pub reserved: u8,
    pub is_long: bool,
    /// `true` once the long string's hash has been computed.
    pub hash_ready: bool,
}

/// Lua table: hybrid array-part + hash-part. Matches `Table` in `lobject.h`.
#[derive(Debug, Clone, Default)]
pub struct Table {
    /// Dense array part. Indexed `1..=array.len()` at the Lua level.
    pub array: Vec<TValue>,
    /// Sparse hash part. Key type rejects nil and NaN at insertion.
    pub hash: HashMap<TableKey, TValue>,
    /// Metatable, if any.
    pub metatable: Option<TableHandle>,
    /// Bitmask caching "metamethod absent" flags so `ltm`'s fast path can
    /// short-circuit. Lua name: `flags`.
    pub meta_cache_flags: u8,
}

/// Description of one upvalue in a [`Proto`] — name, location, kind.
#[derive(Debug, Clone)]
pub struct UpvalDesc {
    pub name: Option<StringHandle>,
    pub in_stack: bool,
    pub idx: u8,
    pub kind: u8,
}

/// Debug-info entry for a local variable in a [`Proto`].
#[derive(Debug, Clone)]
pub struct LocVar {
    pub name: Option<StringHandle>,
    pub start_pc: i32,
    pub end_pc: i32,
}

/// Absolute line-info entry stored alongside relative `lineinfo` deltas.
#[derive(Debug, Clone, Copy)]
pub struct AbsLineInfo {
    pub pc: i32,
    pub line: i32,
}

/// Compiled Lua function prototype. Matches `Proto` in `lobject.h`.
#[derive(Debug, Clone, Default)]
pub struct Proto {
    pub num_params: u8,
    pub is_vararg: bool,
    pub max_stack_size: u8,
    pub code: Vec<Instruction>,
    pub constants: Vec<TValue>,
    pub inner_protos: Vec<ProtoHandle>,
    pub upvalues: Vec<UpvalDesc>,
    /// Relative line-info: `i8` deltas stored per instruction. `i8::MIN`
    /// means "see `abs_line_info`". Matches C's `ls_byte *lineinfo`.
    pub line_info: Vec<LsByte>,
    pub abs_line_info: Vec<AbsLineInfo>,
    pub local_vars: Vec<LocVar>,
    pub source: Option<StringHandle>,
    pub line_defined: i32,
    pub last_line_defined: i32,
}

/// Lua-level closure: prototype + captured upvalues.
#[derive(Debug, Clone)]
pub struct LClosure {
    pub proto: ProtoHandle,
    pub upvalues: Vec<UpValHandle>,
}

/// C closure: raw function pointer + captured Lua values.
#[derive(Debug, Clone)]
pub struct CClosure {
    pub f: RawCFunction,
    pub upvalues: Vec<TValue>,
}

/// Upvalue state: either **open** (points to a slot on the owning thread's
/// stack) or **closed** (owns the value directly).
#[derive(Debug, Clone)]
pub enum UpValState {
    Open {
        thread: ThreadHandle,
        stack_index: u32,
    },
    Closed(TValue),
}

/// Captured upvalue of a Lua closure. See `UpVal` in `lobject.h`.
#[derive(Debug, Clone)]
pub struct UpVal {
    pub state: UpValState,
}

/// Full userdata — a blob of user-controlled bytes plus metatable + uservalues.
#[derive(Debug, Clone, Default)]
pub struct UserData {
    pub data: Vec<u8>,
    pub metatable: Option<TableHandle>,
    pub user_values: Vec<TValue>,
}

// ---------------------------------------------------------------------------
// Thread / CallFrame / Status.
// ---------------------------------------------------------------------------

/// Thread status word (matches `lua.h` `LUA_OK`/`LUA_YIELD`/`LUA_ERR*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStatus {
    Ok,
    Yield,
    RuntimeError,
    SyntaxError,
    MemoryError,
    GcError,
    HandlerError,
}

impl ThreadStatus {
    pub const fn as_lua_status(self) -> c_int {
        match self {
            ThreadStatus::Ok => 0,
            ThreadStatus::Yield => 1,
            ThreadStatus::RuntimeError => 2,
            ThreadStatus::SyntaxError => 3,
            ThreadStatus::MemoryError => 4,
            ThreadStatus::GcError => 5,
            ThreadStatus::HandlerError => 6,
        }
    }
}

/// One frame on a thread's call stack. Matches `CallInfo` in `lstate.h` at
/// a conceptual level (fewer fields for now; added as the VM grows).
#[derive(Debug, Clone)]
pub struct CallFrame {
    /// Stack index of the function being called (absolute, not relative).
    pub func: u32,
    /// Top of this frame's register window.
    pub top: u32,
    /// Saved program counter for Lua frames; `0` for C frames.
    pub saved_pc: u32,
    /// Expected result count; `-1` for `LUA_MULTRET`.
    pub n_results: i16,
    /// Packed status/callstatus bits. Named after C's `CallInfo::callstatus`.
    pub call_status: u16,
}

/// A Lua coroutine. Each Lua execution — including the main thread — lives
/// inside a `Thread`. CPS state is stored here when the thread yields.
#[derive(Debug, Clone, Default)]
pub struct Thread {
    pub status: ThreadStatusInner,
    pub stack: Vec<TValue>,
    /// Register index of the current top of stack (next-free slot).
    pub top: u32,
    pub frames: Vec<CallFrame>,
    /// Open upvalue list. Each `UpValHandle` points to a slot in `stack`.
    pub open_upvals: Vec<UpValHandle>,
}

/// Thread-local wrapper so [`Thread`] can `#[derive(Default)]`. Defaults
/// to [`ThreadStatus::Ok`].
#[derive(Debug, Clone, Copy)]
pub struct ThreadStatusInner(pub ThreadStatus);

impl Default for ThreadStatusInner {
    fn default() -> Self {
        ThreadStatusInner(ThreadStatus::Ok)
    }
}

// ---------------------------------------------------------------------------
// LuaError / LuaResult — the Result-threading error model.
// ---------------------------------------------------------------------------

/// Every fallible Lua operation returns a [`LuaResult`]. `Err(LuaError::..)`
/// propagates up through the VM and C-API boundary; `lua_pcall` catches
/// it and writes the matching status code back into the C caller.
#[derive(Debug, Clone)]
pub enum LuaError {
    /// A runtime error carrying any Lua value (Lua's error-object model).
    Runtime(TValue),
    /// Syntax error caught by the parser.
    Syntax(String),
    /// Allocation failure.
    Memory,
    /// GC emergency finalization failure.
    Gc,
    /// Error raised while running an error handler.
    Handler,
}

/// Convenience alias used throughout the crate.
pub type LuaResult<T> = Result<T, LuaError>;

impl LuaError {
    pub const fn matching_status(&self) -> ThreadStatus {
        match self {
            LuaError::Runtime(_) => ThreadStatus::RuntimeError,
            LuaError::Syntax(_) => ThreadStatus::SyntaxError,
            LuaError::Memory => ThreadStatus::MemoryError,
            LuaError::Gc => ThreadStatus::GcError,
            LuaError::Handler => ThreadStatus::HandlerError,
        }
    }
}

// ---------------------------------------------------------------------------
// Heap — the zero-dep arena. One `Vec<Option<T>>` per object kind, plus
// per-kind free lists. Generation counters come in Stage 3 with the GC.
// ---------------------------------------------------------------------------

/// Backing storage for every GC-managed Lua object. Every heap slot is
/// `Option<T>`: `None` while the slot is free, `Some(obj)` while live.
/// Freed slots are pushed onto the matching free list for O(1) reuse.
#[derive(Debug, Default)]
pub struct Heap {
    pub strings: Vec<Option<LuaString>>,
    pub tables: Vec<Option<Table>>,
    pub protos: Vec<Option<Proto>>,
    pub lclosures: Vec<Option<LClosure>>,
    pub cclosures: Vec<Option<CClosure>>,
    pub upvals: Vec<Option<UpVal>>,
    pub threads: Vec<Option<Thread>>,
    pub userdata: Vec<Option<UserData>>,

    pub free_strings: Vec<u32>,
    pub free_tables: Vec<u32>,
    pub free_protos: Vec<u32>,
    pub free_lclosures: Vec<u32>,
    pub free_cclosures: Vec<u32>,
    pub free_upvals: Vec<u32>,
    pub free_threads: Vec<u32>,
    pub free_userdata: Vec<u32>,
}

// ---------------------------------------------------------------------------
// GlobalState / LuaState — VM-wide shared state and the public handle.
// ---------------------------------------------------------------------------

/// Global state shared by every thread. Matches `global_State` in `lstate.h`.
/// Placeholder — grows as the VM lands.
#[derive(Debug, Default)]
pub struct GlobalState {
    pub heap: Heap,
    /// Registry table (`LUA_REGISTRYINDEX`).
    pub registry: Option<TableHandle>,
    /// Main thread.
    pub main_thread: Option<ThreadHandle>,
    /// String interning table: hash → list of short-string handles.
    pub string_intern: HashMap<u32, Vec<StringHandle>>,
    /// Hash seed used by `lstring::hash_bytes` for every string intern.
    /// Matches `global_State::seed` in `lstate.h`. Set at bootstrap
    /// time by `LuaState::new`.
    pub hash_seed: u32,
    /// Interned metamethod name handles indexed by `ltm::TagMethod`.
    /// Populated by `init_metamethod_names` during state bootstrap;
    /// stays `None` on an uninitialized `GlobalState::default()`.
    /// Matches the `G(L)->tmname[TM_N]` array in `lstate.h`.
    pub tm_names: Vec<StringHandle>,
}

/// The public VM handle. At the drop-in ABI boundary this is passed
/// around as `*mut lua_State`; internally it owns the [`GlobalState`] and
/// remembers which thread is currently executing.
///
/// NOTE: the `extern "C"` API layer ([`crate::lib`]) will expose a
/// `repr(C)` header struct named `lua_State` that forwards to this type
/// via a `Box<LuaStateInner>` stored at offset 0. That layer lands in
/// Stage 4 alongside `lapi`.
#[derive(Debug)]
pub struct LuaState {
    pub global: GlobalState,
    pub current_thread: ThreadHandle,
}

impl Default for LuaState {
    fn default() -> Self {
        LuaState {
            global: GlobalState::default(),
            // Placeholder — real thread allocation lands with `lstate`.
            current_thread: ThreadHandle(0),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — the contract must compile standalone. Smoke-test the few trivial
// semantics already present.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_matches_lua_semantics() {
        assert_eq!(TValue::Nil.type_name(), "nil");
        assert_eq!(TValue::False.type_name(), "boolean");
        assert_eq!(TValue::True.type_name(), "boolean");
        assert_eq!(TValue::Integer(42).type_name(), "number");
        assert_eq!(TValue::Number(1.5).type_name(), "number");
        assert_eq!(TValue::ShortString(StringHandle(0)).type_name(), "string");
        assert_eq!(TValue::LongString(StringHandle(0)).type_name(), "string");
        assert_eq!(TValue::Table(TableHandle(0)).type_name(), "table");
        assert_eq!(TValue::LuaClosure(LClosureHandle(0)).type_name(), "function");
        assert_eq!(TValue::Thread(ThreadHandle(0)).type_name(), "thread");
        assert_eq!(TValue::UserData(UserDataHandle(0)).type_name(), "userdata");
    }

    #[test]
    fn truthiness_matches_lua() {
        assert!(!TValue::Nil.is_truthy());
        assert!(!TValue::False.is_truthy());
        assert!(TValue::True.is_truthy());
        assert!(TValue::Integer(0).is_truthy()); // zero is truthy in Lua
        assert!(TValue::Number(0.0).is_truthy());
        assert!(TValue::Table(TableHandle(0)).is_truthy());
    }

    #[test]
    fn variant_tags_match_c_lua_layout() {
        assert_eq!(TValue::Nil.variant_tag(), LUA_VNIL);
        assert_eq!(TValue::False.variant_tag(), LUA_VFALSE);
        assert_eq!(TValue::True.variant_tag(), LUA_VTRUE);
        assert_eq!(TValue::Integer(0).variant_tag(), LUA_VNUMINT);
        assert_eq!(TValue::Number(0.0).variant_tag(), LUA_VNUMFLT);
        assert_eq!(TValue::ShortString(StringHandle(0)).variant_tag(), LUA_VSHRSTR);
        assert_eq!(TValue::LongString(StringHandle(0)).variant_tag(), LUA_VLNGSTR);
        assert_eq!(TValue::LuaClosure(LClosureHandle(0)).variant_tag(), LUA_VLCL);
        assert_eq!(TValue::CClosure(CClosureHandle(0)).variant_tag(), LUA_VCCL);
    }

    #[test]
    fn error_status_mapping_is_total() {
        // Every LuaError variant must map to a ThreadStatus without panicking.
        let errors = [
            LuaError::Runtime(TValue::Nil),
            LuaError::Syntax("x".into()),
            LuaError::Memory,
            LuaError::Gc,
            LuaError::Handler,
        ];
        for e in errors {
            let _ = e.matching_status();
        }
    }

    #[test]
    fn heap_defaults_are_empty() {
        let h = Heap::default();
        assert!(h.strings.is_empty());
        assert!(h.tables.is_empty());
        assert!(h.free_strings.is_empty());
    }

    #[test]
    fn table_key_hashes_floats_by_bit_pattern() {
        // Two distinct NaN bit patterns are distinct keys (valid because NaN
        // is rejected at insertion; this test exists to document that the
        // Hash derive uses the raw u64 — no float-equality funny business).
        let a = TableKey::Number(1.5f64.to_bits());
        let b = TableKey::Number(1.5f64.to_bits());
        let c = TableKey::Number(2.5f64.to_bits());
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
