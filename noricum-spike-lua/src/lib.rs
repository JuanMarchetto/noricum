//! Noricum Lua 5.4 full port — crate root.
//!
//! This crate hosts two things that are deliberately separate:
//!
//! 1. The **C oracle FFI layer** (this file). `extern "C"` bindings to the
//!    thin C wrapper around Lua's public API (see `wrapper.h`). Used only
//!    by the differential test harness (`tests/differential_lua.rs`) so
//!    the Rust migration has a ground-truth comparator. This is the only
//!    `unsafe` in the crate during the migration era; it goes away once
//!    the port is feature-complete.
//!
//! 2. The **Rust Lua implementation** itself, rooted in [`contract`]. The
//!    module grows as the migration advances. Every module is pure safe
//!    Rust; the drop-in `extern "C"` ABI layer (Stage 4, not yet written)
//!    will eventually re-expose these types as the public `liblua` ABI.
//!
//! Methodology: `docs/methodology/full-port-mode.md`.
//! Plan + status: `docs/lua-migration/plan.md`.
//! Locked decisions: persistent memory `project_lua_full_port.md`.

pub mod contract;
pub mod heap;
pub mod lapi;
pub mod lauxlib;
pub mod lbaselib;
pub mod lcorolib;
pub mod lctype;
pub mod ldblib;
pub mod ldump;
pub mod linit;
pub mod lundump;
pub mod liolib;
pub mod loslib;
pub mod lutf8lib;
pub mod ldo;
pub mod lfunc;
pub mod lgc;
pub mod lmem;
pub mod lcode;
pub mod llex;
pub mod lmathlib;
pub mod lobject;
pub mod lparser;
pub mod lopcodes;
pub mod lstate;
pub mod lstring;
pub mod lstrlib;
pub mod ltable;
pub mod ltablib;
pub mod ltm;
pub mod lvm;
pub mod lzio;

use std::os::raw::{c_char, c_int, c_long, c_uint, c_void};

pub type SpikeHandle = *mut c_void;

#[link(name = "lua_wrapper", kind = "static")]
unsafe extern "C" {
    pub fn wr_newstate() -> SpikeHandle;
    pub fn wr_openlibs(h: SpikeHandle);
    pub fn wr_loadstring(h: SpikeHandle, src: *const c_char) -> c_int;
    pub fn wr_pcall(h: SpikeHandle, nargs: c_int, nresults: c_int, errfunc: c_int) -> c_int;
    pub fn wr_tostring(h: SpikeHandle, idx: c_int, out: *mut c_char, cap: usize) -> c_long;
    pub fn wr_close(h: SpikeHandle);

    pub fn wr_open(filename: *const c_char) -> SpikeHandle;

    // lctype oracle shims (see wrapper.c). Each takes an int to carry
    // Lua's EOZ = -1 sentinel through unchanged.
    pub fn wr_lctype_byte(c: c_int) -> c_int;
    pub fn wr_lctype_islalpha(c: c_int) -> c_int;
    pub fn wr_lctype_islalnum(c: c_int) -> c_int;
    pub fn wr_lctype_isdigit(c: c_int) -> c_int;
    pub fn wr_lctype_isspace(c: c_int) -> c_int;
    pub fn wr_lctype_isprint(c: c_int) -> c_int;
    pub fn wr_lctype_isxdigit(c: c_int) -> c_int;
    pub fn wr_lctype_tolower(c: c_int) -> c_int;

    // lopcodes oracle shims.
    pub fn wr_lopcodes_num_opcodes() -> c_int;
    pub fn wr_lopcodes_opmode(op: c_int) -> c_int;
    pub fn wr_lopcodes_get_opcode(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_a(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_b(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_c(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_k(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_vb(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_vc(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_bx(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_sbx(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_ax(i: c_uint) -> c_int;
    pub fn wr_lopcodes_getarg_sj(i: c_uint) -> c_int;
    pub fn wr_lopcodes_is_ot(i: c_uint) -> c_int;
    pub fn wr_lopcodes_is_it(i: c_uint) -> c_int;
    pub fn wr_lopcodes_op_tailcall() -> c_int;
    pub fn wr_lopcodes_op_setlist() -> c_int;
    pub fn wr_lopcodes_op_call() -> c_int;
    pub fn wr_lopcodes_op_return() -> c_int;
    pub fn wr_lopcodes_op_extraarg() -> c_int;

    // lmem oracle shim. Writes new size via out-param; returns 1 on
    // success, 0 on "too many X" overflow, -1 on state init failure.
    pub fn wr_lmem_grow_array_size(
        size_in: c_int,
        nelems: c_int,
        limit: c_int,
        out_size: *mut c_int,
    ) -> c_int;

    // lzio oracle shims. Each runs the operation through a real ZIO
    // backed by a chunked in-memory reader over (src, src_len).
    pub fn wr_lzio_read(
        src: *const c_char,
        src_len: usize,
        chunk_size: usize,
        out_buf: *mut u8,
        n: usize,
    ) -> usize;
    pub fn wr_lzio_getaddr(
        src: *const c_char,
        src_len: usize,
        chunk_size: usize,
        out_buf: *mut u8,
        n: usize,
    ) -> c_int;
    pub fn wr_lzio_fill_first(
        src: *const c_char,
        src_len: usize,
        chunk_size: usize,
    ) -> c_int;

    // lobject oracle shims — pure helpers, no lua_State required.
    pub fn wr_lobject_ceillog2(x: c_uint) -> c_uint;
    pub fn wr_lobject_hexavalue(c: c_int) -> c_int;
    pub fn wr_lobject_codeparam(p: c_uint) -> c_uint;
    pub fn wr_lobject_applyparam(p: c_uint, x: i64) -> i64;
    pub fn wr_lobject_utf8esc(buff: *mut u8, x: c_uint) -> c_int;

    /// `luaO_rawarith` oracle. See `wrapper.h` for the return-code
    /// contract (1 = ok, 0 = metamethod fallback, -1 = runtime error,
    /// -2 = init failed).
    #[allow(clippy::too_many_arguments)]
    pub fn wr_lobject_rawarith(
        op: c_int,
        t1: c_int,
        i1: i64,
        f1: f64,
        t2: c_int,
        i2: i64,
        f2: f64,
        out_tag: *mut c_int,
        out_int: *mut i64,
        out_float: *mut f64,
    ) -> c_int;

    /// `luaS_hash` oracle. Creates an ephemeral lua_State pinned to
    /// `seed` and returns the hash the C implementation computes for
    /// the given byte sequence. Short and long strings both route
    /// through the same static `luaS_hash` internally.
    pub fn wr_lstring_hash(bytes: *const c_char, len: usize, seed: c_uint) -> c_uint;

    /// Total number of tag methods (TM_N).
    pub fn wr_ltm_tm_n() -> c_int;

    /// Copy the nth interned metamethod event name into `out_buf`.
    /// Returns the byte length on success, -1 for out-of-range `i`,
    /// or 0 on oracle state init failure.
    pub fn wr_ltm_event_name(i: c_int, out_buf: *mut u8, cap: usize) -> c_int;
}

// TODO: add `pub mod contract;` once the Rust-side type architecture is
// written (Hour 0-1 of the spike). See docs/methodology/architectural-seeds.md.
