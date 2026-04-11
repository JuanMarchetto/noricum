//! Noricum interactive spike: miniz_zip.c migration via Claude Code as director.
//!
//! `lib.rs` is the FFI layer — it exposes the C miniz wrapper via `extern "C"`
//! so the differential test harness can call miniz through Rust.
//!
//! `contract` holds the Rust-native type contract (the "what the migration is
//! trying to produce" spec). As each C function lands in Rust, it lives in a
//! submodule under `contract`.

pub mod contract;
pub mod reader;
pub mod writer;

use std::os::raw::{c_char, c_int, c_uint, c_void};

pub type ZipHandle = *mut c_void;

#[link(name = "miniz_wrapper", kind = "static")]
unsafe extern "C" {
    pub fn wr_reader_open(filename: *const c_char) -> ZipHandle;
    pub fn wr_reader_get_num_files(h: ZipHandle) -> c_int;
    pub fn wr_reader_locate(h: ZipHandle, name: *const c_char) -> c_int;
    pub fn wr_reader_extract(
        h: ZipHandle,
        file_index: c_int,
        buf: *mut c_void,
        buf_size: usize,
    ) -> c_int;
    pub fn wr_reader_stat(
        h: ZipHandle,
        file_index: c_int,
        name_out: *mut c_char,
        name_cap: usize,
        size_out: *mut usize,
        crc32_out: *mut c_uint,
    ) -> c_int;
    pub fn wr_reader_close(h: ZipHandle);

    pub fn wr_writer_open(filename: *const c_char) -> ZipHandle;
    pub fn wr_writer_add(
        h: ZipHandle,
        name: *const c_char,
        buf: *const c_void,
        buf_size: usize,
    ) -> c_int;
    pub fn wr_writer_finalize(h: ZipHandle) -> c_int;
    pub fn wr_writer_close(h: ZipHandle);
}
