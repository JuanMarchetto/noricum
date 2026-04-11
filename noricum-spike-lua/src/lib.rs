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
pub mod lctype;
pub mod lmem;
pub mod lopcodes;

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
}

// TODO: add `pub mod contract;` once the Rust-side type architecture is
// written (Hour 0-1 of the spike). See docs/methodology/architectural-seeds.md.
