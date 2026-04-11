//! Differential test for the ported `lzio::Zio`.
//!
//! The oracle shims in `wrapper.c` invoke the real `luaZ_read`,
//! `luaZ_getaddr`, and `luaZ_fill` against a chunked in-memory reader
//! that hands out fixed-size chunks of a user-provided byte slice.
//! Our Rust `Zio<ChunkedByteReader<'_>>` uses the same chunking
//! policy, so the two should agree for every input and every chunk
//! boundary.

use spike::lzio::{ChunkedByteReader, Zio, EOZ};
use spike::{wr_lzio_fill_first, wr_lzio_getaddr, wr_lzio_read};

fn oracle_read(src: &[u8], chunk_size: usize, n: usize) -> (usize, Vec<u8>) {
    let mut out = vec![0u8; n];
    let missing = unsafe {
        wr_lzio_read(
            src.as_ptr() as *const i8,
            src.len(),
            chunk_size,
            out.as_mut_ptr(),
            n,
        )
    };
    out.truncate(n);
    (missing, out)
}

fn rust_read(src: &[u8], chunk_size: usize, n: usize) -> (usize, Vec<u8>) {
    let mut z = Zio::new(ChunkedByteReader::new(src, chunk_size));
    let mut out = vec![0u8; n];
    let missing = z.read(&mut out);
    (missing, out)
}

fn oracle_getaddr(src: &[u8], chunk_size: usize, n: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; n];
    let ok = unsafe {
        wr_lzio_getaddr(
            src.as_ptr() as *const i8,
            src.len(),
            chunk_size,
            out.as_mut_ptr(),
            n,
        )
    };
    if ok == 1 {
        Some(out)
    } else {
        None
    }
}

fn rust_getaddr(src: &[u8], chunk_size: usize, n: usize) -> Option<Vec<u8>> {
    let mut z = Zio::new(ChunkedByteReader::new(src, chunk_size));
    z.get_addr(n).map(|s| s.to_vec())
}

fn oracle_fill_first(src: &[u8], chunk_size: usize) -> i32 {
    unsafe { wr_lzio_fill_first(src.as_ptr() as *const i8, src.len(), chunk_size) }
}

fn rust_fill_first(src: &[u8], chunk_size: usize) -> i32 {
    let mut z = Zio::new(ChunkedByteReader::new(src, chunk_size));
    z.fill()
}

// -- read() parity --------------------------------------------------------

#[test]
fn read_full_input_in_one_chunk() {
    let src = b"hello, world";
    let (c_missing, c_buf) = oracle_read(src, 64, 12);
    let (r_missing, r_buf) = rust_read(src, 64, 12);
    assert_eq!(c_missing, r_missing);
    assert_eq!(c_buf, r_buf);
    assert_eq!(c_missing, 0);
    assert_eq!(&c_buf, src);
}

#[test]
fn read_across_many_chunks() {
    let src = b"abcdefghijklmnopqrstuvwxyz";
    for chunk_size in [1usize, 2, 3, 5, 7, 11, 13, 26, 100] {
        let (c_m, c_b) = oracle_read(src, chunk_size, src.len());
        let (r_m, r_b) = rust_read(src, chunk_size, src.len());
        assert_eq!(c_m, r_m, "chunk_size = {chunk_size}");
        assert_eq!(c_b, r_b, "chunk_size = {chunk_size}");
    }
}

#[test]
fn read_short_request_returns_missing_count() {
    let src = b"ab";
    for chunk_size in [1usize, 2, 4] {
        let (c_m, c_b) = oracle_read(src, chunk_size, 5);
        let (r_m, r_b) = rust_read(src, chunk_size, 5);
        assert_eq!(c_m, r_m, "chunk_size = {chunk_size}");
        assert_eq!(&c_b[..2], &r_b[..2]);
        assert_eq!(c_m, 3);
    }
}

#[test]
fn read_empty_input_reports_full_miss() {
    let (c_m, _) = oracle_read(b"", 4, 7);
    let (r_m, _) = rust_read(b"", 4, 7);
    assert_eq!(c_m, r_m);
    assert_eq!(c_m, 7);
}

#[test]
fn read_zero_bytes_never_advances() {
    let src = b"abc";
    let (c_m, _) = oracle_read(src, 2, 0);
    let (r_m, _) = rust_read(src, 2, 0);
    assert_eq!(c_m, r_m);
    assert_eq!(c_m, 0);
}

// -- get_addr() parity ----------------------------------------------------

#[test]
fn getaddr_fits_inside_chunk() {
    // Chunk size 8, ask for 5 bytes — all from the first chunk.
    let src = b"0123456789ABCDEF";
    assert_eq!(oracle_getaddr(src, 8, 5), rust_getaddr(src, 8, 5));
}

#[test]
fn getaddr_rejects_when_crossing_chunk_boundary() {
    // Chunk size 4, ask for 5 — first chunk has only 4 bytes.
    let src = b"0123456789";
    assert_eq!(oracle_getaddr(src, 4, 5), None);
    assert_eq!(rust_getaddr(src, 4, 5), None);
}

#[test]
fn getaddr_accepts_exact_chunk_size() {
    let src = b"abcdefgh";
    let c = oracle_getaddr(src, 4, 4);
    let r = rust_getaddr(src, 4, 4);
    assert_eq!(c, r);
    assert_eq!(c, Some(b"abcd".to_vec()));
}

#[test]
fn getaddr_on_empty_input_is_none() {
    assert_eq!(oracle_getaddr(b"", 4, 1), None);
    assert_eq!(rust_getaddr(b"", 4, 1), None);
}

#[test]
fn getaddr_zero_length_always_some() {
    // Edge case: asking for 0 bytes. C returns current position (which
    // is non-NULL after a successful fill), and we match that.
    let src = b"abc";
    assert_eq!(oracle_getaddr(src, 4, 0), rust_getaddr(src, 4, 0));
}

// -- fill() parity --------------------------------------------------------

#[test]
fn fill_first_returns_leading_byte() {
    for src in [&b"h"[..], b"hello", b"\x00\x01"] {
        for chunk_size in [1usize, 2, 16] {
            assert_eq!(
                oracle_fill_first(src, chunk_size),
                rust_fill_first(src, chunk_size),
                "src = {src:?}, chunk_size = {chunk_size}"
            );
        }
    }
}

#[test]
fn fill_on_empty_is_eoz() {
    assert_eq!(oracle_fill_first(b"", 4), EOZ);
    assert_eq!(rust_fill_first(b"", 4), EOZ);
}

#[test]
fn fill_handles_high_byte_values() {
    // Make sure we don't sign-extend 0xff into EOZ.
    let src = b"\xff";
    let c = oracle_fill_first(src, 1);
    let r = rust_fill_first(src, 1);
    assert_eq!(c, r);
    assert_eq!(c, 255);
    assert_ne!(c, EOZ);
}
