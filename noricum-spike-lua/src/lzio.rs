//! Lua buffered input stream (`ZIO`).
//!
//! Port of `lzio.{c,h}`. Provides a chunk-oriented byte reader that
//! the lexer and loader consume one byte at a time. The reader side
//! is expressed as a trait so the lexer doesn't care whether the
//! bytes come from a file, a string, or an FFI callback.
//!
//! # Semantics preserved from C
//!
//! * [`Zio::read`] copies up to `n` bytes into an output slice and
//!   returns the number of bytes that were **missing** (0 on success,
//!   `n - bytes_written` on short read). This matches
//!   `luaZ_read (ZIO*, void*, size_t)` in `lzio.c`.
//! * [`Zio::get_addr`] returns a borrow of `n` consecutive bytes if and
//!   only if they all fit within the current chunk. Matches
//!   `luaZ_getaddr`.
//! * [`Zio::get_byte`] returns the next byte as a non-negative integer,
//!   or [`EOZ`] (= -1) on end-of-stream. Matches `zgetc`.
//! * EOF is sticky: once the reader has returned an empty chunk, the
//!   Zio stays empty.
//!
//! # Deviations from C
//!
//! * The reader trait returns owned `Vec<u8>` chunks instead of the C
//!   `const char* + size_t*` pattern. This is purely because Rust's
//!   borrow checker wants the chunk's lifetime to be tied to the reader
//!   and the chunk bytes must not move under us. Zio owns the current
//!   chunk internally; the reader yields ownership at each fill.
//! * `Mbuffer` (the lexer's growable byte scratch buffer) is NOT ported.
//!   Its consumers will use `Vec<u8>` directly in Stage 6 — every macro
//!   in the `Mbuffer` family is trivially expressed as a `Vec` method.
//!
//! Ground truth: `lzio.c` + `lzio.h` in this crate's root.

#![allow(dead_code)]

/// End-of-stream sentinel. Mirrors `#define EOZ (-1)` in `lzio.h`.
pub const EOZ: i32 = -1;

/// Chunk-producing reader. Return an empty `Vec` to signal end-of-stream.
///
/// Each call MUST return either a non-empty chunk or the final empty
/// chunk. The Zio calls `next_chunk` only when it has exhausted the
/// previous chunk, so spurious empty returns before EOF would be seen
/// as EOF.
pub trait ZioReader {
    fn next_chunk(&mut self) -> Vec<u8>;
}

/// Buffered byte stream. Wraps a [`ZioReader`] and hands out bytes
/// chunk-by-chunk to the lexer/loader.
pub struct Zio<R: ZioReader> {
    reader: R,
    /// Current chunk bytes owned by the Zio. Replaced wholesale on
    /// each refill.
    chunk: Vec<u8>,
    /// Byte offset into `chunk` of the next unread byte.
    pos: usize,
    /// Set once the reader has returned an empty chunk; after that the
    /// Zio refuses to call the reader again.
    eof: bool,
}

impl<R: ZioReader> Zio<R> {
    /// Initialize a new stream over `reader`. Matches `luaZ_init` (minus
    /// the `lua_State*` and `void* data` arguments — those get bundled
    /// into the reader's own state in the Rust design).
    pub fn new(reader: R) -> Self {
        Zio {
            reader,
            chunk: Vec::new(),
            pos: 0,
            eof: false,
        }
    }

    /// Drain the stream into `out`. Returns the number of bytes that
    /// were requested but not delivered (0 on a fully-filled buffer,
    /// the full `out.len()` when nothing at all was available).
    ///
    /// Direct port of `luaZ_read` in `lzio.c`.
    pub fn read(&mut self, out: &mut [u8]) -> usize {
        let mut needed = out.len();
        let mut written = 0;
        while needed > 0 {
            if !self.ensure_chunk() {
                return needed;
            }
            let available = self.chunk.len() - self.pos;
            let take = needed.min(available);
            out[written..written + take]
                .copy_from_slice(&self.chunk[self.pos..self.pos + take]);
            self.pos += take;
            written += take;
            needed -= take;
        }
        0
    }

    /// Return a borrow of `n` consecutive bytes iff they all fit inside
    /// the current chunk. Advances the read cursor on success.
    /// Returns `None` if the stream is at EOF or the remaining chunk is
    /// smaller than `n`. Matches `luaZ_getaddr`.
    pub fn get_addr(&mut self, n: usize) -> Option<&[u8]> {
        if !self.ensure_chunk() {
            return None;
        }
        let remaining = self.chunk.len() - self.pos;
        if remaining < n {
            return None;
        }
        let start = self.pos;
        self.pos += n;
        Some(&self.chunk[start..start + n])
    }

    /// Read the next byte, or [`EOZ`] on end-of-stream.
    ///
    /// Matches the `zgetc` macro in `lzio.h`, which on the fast path
    /// decrements the remaining-bytes counter and on the slow path
    /// calls `luaZ_fill`.
    pub fn get_byte(&mut self) -> i32 {
        if self.pos < self.chunk.len() {
            let b = self.chunk[self.pos];
            self.pos += 1;
            return b as i32;
        }
        self.fill()
    }

    /// Request a new chunk from the reader. Returns the first byte of
    /// the new chunk, or [`EOZ`] if the reader returned an empty chunk.
    /// Direct port of `luaZ_fill`.
    pub fn fill(&mut self) -> i32 {
        if self.eof {
            return EOZ;
        }
        let chunk = self.reader.next_chunk();
        if chunk.is_empty() {
            self.eof = true;
            self.chunk = Vec::new();
            self.pos = 0;
            return EOZ;
        }
        self.chunk = chunk;
        // Match C's luaZ_fill: return the first byte AND advance past it.
        let first = self.chunk[0];
        self.pos = 1;
        first as i32
    }

    /// Number of unread bytes still in the current chunk. Matches the
    /// `z->n` field in C.
    pub fn remaining_in_chunk(&self) -> usize {
        self.chunk.len() - self.pos
    }

    /// Has the stream reached end-of-file?
    pub fn is_eof(&self) -> bool {
        self.eof && self.pos >= self.chunk.len()
    }

    /// Top-up helper used by [`read`] and [`get_addr`]. Returns `true`
    /// if the current chunk has at least one unread byte after the call
    /// (refilling from the reader if necessary); `false` at EOF.
    fn ensure_chunk(&mut self) -> bool {
        if self.pos < self.chunk.len() {
            return true;
        }
        if self.eof {
            return false;
        }
        let chunk = self.reader.next_chunk();
        if chunk.is_empty() {
            self.eof = true;
            self.chunk = Vec::new();
            self.pos = 0;
            return false;
        }
        self.chunk = chunk;
        self.pos = 0;
        true
    }
}

// ---------------------------------------------------------------------------
// Readers
// ---------------------------------------------------------------------------

/// Reader that hands out an in-memory byte slice in fixed-size chunks.
/// Primarily a test fixture — matches the `wr_zio_chunked_reader` shape
/// in the C oracle.
pub struct ChunkedByteReader<'a> {
    data: &'a [u8],
    pos: usize,
    chunk_size: usize,
}

impl<'a> ChunkedByteReader<'a> {
    pub fn new(data: &'a [u8], chunk_size: usize) -> Self {
        assert!(chunk_size > 0, "chunk_size must be positive");
        ChunkedByteReader {
            data,
            pos: 0,
            chunk_size,
        }
    }
}

impl<'a> ZioReader for ChunkedByteReader<'a> {
    fn next_chunk(&mut self) -> Vec<u8> {
        if self.pos >= self.data.len() {
            return Vec::new();
        }
        let take = (self.data.len() - self.pos).min(self.chunk_size);
        let out = self.data[self.pos..self.pos + take].to_vec();
        self.pos += take;
        out
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_zio(data: &[u8], chunk_size: usize) -> Zio<ChunkedByteReader<'_>> {
        Zio::new(ChunkedByteReader::new(data, chunk_size))
    }

    #[test]
    fn read_single_chunk_exact() {
        let data = b"hello";
        let mut z = fresh_zio(data, 16);
        let mut buf = [0u8; 5];
        let missing = z.read(&mut buf);
        assert_eq!(missing, 0);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn read_across_chunks() {
        let data = b"hello, world";
        let mut z = fresh_zio(data, 3);
        let mut buf = [0u8; 12];
        let missing = z.read(&mut buf);
        assert_eq!(missing, 0);
        assert_eq!(&buf, b"hello, world");
    }

    #[test]
    fn short_read_returns_missing_count() {
        let data = b"hi";
        let mut z = fresh_zio(data, 4);
        let mut buf = [0u8; 5];
        let missing = z.read(&mut buf);
        assert_eq!(missing, 3);
        assert_eq!(&buf[..2], b"hi");
    }

    #[test]
    fn get_byte_walks_and_eof_is_sticky() {
        let data = b"ab";
        let mut z = fresh_zio(data, 1);
        assert_eq!(z.get_byte(), b'a' as i32);
        assert_eq!(z.get_byte(), b'b' as i32);
        assert_eq!(z.get_byte(), EOZ);
        assert_eq!(z.get_byte(), EOZ); // still EOZ
        assert!(z.is_eof());
    }

    #[test]
    fn get_addr_only_returns_within_chunk() {
        // Chunk size 4: "abcd" then "efgh" then "ij".
        let data = b"abcdefghij";
        let mut z = fresh_zio(data, 4);

        // Ask for 4 bytes — exactly the first chunk.
        assert_eq!(z.get_addr(4).map(|s| s.to_vec()), Some(b"abcd".to_vec()));

        // Ask for 5 bytes now — only 4 available in the new chunk, so None.
        assert_eq!(z.get_addr(5), None);

        // We can still read byte-by-byte after that (the Zio pulled the
        // second chunk while trying to satisfy get_addr).
        let mut buf = [0u8; 4];
        assert_eq!(z.read(&mut buf), 0);
        assert_eq!(&buf, b"efgh");
    }

    #[test]
    fn empty_input_produces_immediate_eof() {
        let mut z = fresh_zio(b"", 4);
        assert_eq!(z.get_byte(), EOZ);
        let mut buf = [0u8; 3];
        assert_eq!(z.read(&mut buf), 3);
    }

    #[test]
    fn read_zero_bytes_is_a_noop() {
        let mut z = fresh_zio(b"abc", 2);
        let mut buf: [u8; 0] = [];
        assert_eq!(z.read(&mut buf), 0);
        // Stream position unchanged.
        assert_eq!(z.get_byte(), b'a' as i32);
    }
}
