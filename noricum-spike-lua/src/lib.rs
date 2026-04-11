//! Noricum interactive spike: lua migration via Claude Code as director.
//!
//! lib.rs is the FFI layer. extern "C" declarations for the wrapper.
//! See docs/methodology/phase-0-oracle-harness.md for the pattern.

use std::os::raw::{c_char, c_int, c_void};

pub type SpikeHandle = *mut c_void;

#[link(name = "lua_wrapper", kind = "static")]
unsafe extern "C" {
    pub fn wr_open(filename: *const c_char) -> SpikeHandle;
    pub fn wr_close(h: SpikeHandle) -> c_int;
    // TODO: add the rest of the wrapper functions
}

// TODO: add pub mod contract; once the type contract is written.
// TODO: add pub mod reader; / pub mod writer; as functions land.
