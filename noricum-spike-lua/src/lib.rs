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

use std::os::raw::{c_char, c_int, c_long, c_void};

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
}

// TODO: add `pub mod contract;` once the Rust-side type architecture is
// written (Hour 0-1 of the spike). See docs/methodology/architectural-seeds.md.
