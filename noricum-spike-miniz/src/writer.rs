//! ZIP writer implementation — the write half of the reader/writer pair.
//!
//! Mirrors the shape of `src/reader.rs` but for the inverse direction:
//! streaming entries into a freshly-created archive file. Four top-level
//! functions matching miniz_zip.c's public writer API:
//!
//!   - `ZipWriter::create`      — analogue of `mz_zip_writer_init_file`
//!   - `ZipWriter::add_mem`     — analogue of `mz_zip_writer_add_mem`
//!   - `ZipWriter::finalize`    — analogue of `mz_zip_writer_finalize_archive`
//!   - `ZipWriter::close`       — analogue of `mz_zip_writer_end`
//!
//! Deflate compression is delegated to `flate2::write::DeflateEncoder`
//! (raw deflate, no zlib header), which is the same bitstream the C side
//! produces via miniz_tdef. CRC32 via `crc32fast`.
//!
//! Design notes:
//! - Every entry is compressed with deflate unless it is empty (in which
//!   case we use stored to avoid deflate's overhead).
//! - The writer buffers the central directory in memory and writes it on
//!   `finalize`. This matches how miniz_zip does it.
//! - DOS timestamp is fixed to 2026-01-01 00:00:00 for reproducibility.
//!   The differential test only compares name/size/crc32/first_bytes, so
//!   the timestamp is cosmetic.

use std::fs::File;
use std::io::{Seek, Write};
use std::path::Path;

use crate::contract::{
    CentralDirHeader, CompressionMethod, ZipError, ZipMode, ZipResult, ZipSource, ZipWriter,
};
use crate::reader::{CENTRAL_DIR_SIGNATURE, EOCD_SIGNATURE, LOCAL_HEADER_SIGNATURE};

// ---------- write helpers ----------

fn write_u16_le<W: Write>(w: &mut W, v: u16) -> ZipResult<()> {
    w.write_all(&v.to_le_bytes()).map_err(ZipError::Io)
}

fn write_u32_le<W: Write>(w: &mut W, v: u32) -> ZipResult<()> {
    w.write_all(&v.to_le_bytes()).map_err(ZipError::Io)
}

// ---------- fixed DOS date/time ----------

/// DOS-packed date for 2026-01-01 (year-1980=46, month=1, day=1).
/// (46 << 9) | (1 << 5) | 1 = 23585.
const DOS_DATE_2026_01_01: u16 = 23585;
/// DOS-packed time for 00:00:00.
const DOS_TIME_MIDNIGHT: u16 = 0;

// ---------- ZipWriter impl ----------

impl ZipWriter {
    /// Create a new archive at `path`. Analogue of `mz_zip_writer_init_file`.
    /// The file is truncated if it already exists.
    pub fn create<P: AsRef<Path>>(path: P) -> ZipResult<Self> {
        let file = File::create(path.as_ref()).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => ZipError::FileCreateFailed,
            _ => ZipError::FileCreateFailed,
        })?;
        Ok(ZipWriter {
            sink: ZipSource::File(file),
            entries: Vec::new(),
            archive_size: 0,
            file_offset_alignment: 0,
            mode: ZipMode::Writing,
        })
    }

    /// Add an in-memory buffer as an entry. Analogue of `mz_zip_writer_add_mem`.
    /// Always compresses with deflate (matching miniz's default level 6), unless
    /// the buffer is empty in which case the entry is stored.
    pub fn add_mem(&mut self, name: &str, data: &[u8]) -> ZipResult<()> {
        if self.mode != ZipMode::Writing {
            return Err(ZipError::InvalidParameter);
        }

        let name_bytes = name.as_bytes();
        if name_bytes.len() > u16::MAX as usize {
            return Err(ZipError::InvalidFilename);
        }

        let uncompressed_size = data.len() as u64;
        let crc32 = crc32fast::hash(data);

        // Pick compression method. Empty → stored (deflate of empty input
        // still emits a 2-byte end-of-stream marker which is fine, but stored
        // is cheaper and matches what Python's zipfile defaults to for size=0
        // entries in some fixtures).
        let (method, compressed_bytes): (CompressionMethod, Vec<u8>) = if data.is_empty() {
            (CompressionMethod::Stored, Vec::new())
        } else {
            use flate2::Compression;
            use flate2::write::DeflateEncoder;
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(data).map_err(ZipError::Io)?;
            let compressed = encoder.finish().map_err(ZipError::Io)?;
            // If deflate expanded the data (pathological small inputs), fall
            // back to stored. This matches miniz's heuristic.
            if compressed.len() as u64 >= uncompressed_size {
                (CompressionMethod::Stored, data.to_vec())
            } else {
                (CompressionMethod::Deflated, compressed)
            }
        };

        let compressed_size = compressed_bytes.len() as u64;

        // Local file header offset = current sink position.
        let local_header_offset = self.sink.stream_position().map_err(ZipError::Io)?;

        // Write local file header (30 bytes fixed).
        write_u32_le(&mut self.sink, LOCAL_HEADER_SIGNATURE)?;
        write_u16_le(&mut self.sink, 20)?; // version needed to extract (2.0)
        write_u16_le(&mut self.sink, 0)?; // flags
        write_u16_le(&mut self.sink, method.to_wire())?;
        write_u16_le(&mut self.sink, DOS_TIME_MIDNIGHT)?;
        write_u16_le(&mut self.sink, DOS_DATE_2026_01_01)?;
        write_u32_le(&mut self.sink, crc32)?;
        write_u32_le(&mut self.sink, compressed_size as u32)?;
        write_u32_le(&mut self.sink, uncompressed_size as u32)?;
        write_u16_le(&mut self.sink, name_bytes.len() as u16)?;
        write_u16_le(&mut self.sink, 0)?; // extra field length
        self.sink.write_all(name_bytes).map_err(ZipError::Io)?;
        // No extra field in this commit.
        self.sink
            .write_all(&compressed_bytes)
            .map_err(ZipError::Io)?;

        // Record central directory entry for later finalize.
        self.entries.push(CentralDirHeader {
            version_made_by: 20,
            version_needed: 20,
            flags: 0,
            compression_method: method,
            last_mod_time: DOS_TIME_MIDNIGHT,
            last_mod_date: DOS_DATE_2026_01_01,
            crc32,
            compressed_size,
            uncompressed_size,
            disk_number_start: 0,
            internal_attributes: 0,
            external_attributes: 0,
            local_header_offset,
            file_name: name.to_string(),
            extra_field: Vec::new(),
            comment: String::new(),
        });

        self.archive_size = self.sink.stream_position().map_err(ZipError::Io)?;

        Ok(())
    }

    /// Write the central directory and EOCD. Analogue of
    /// `mz_zip_writer_finalize_archive`. After calling this the writer is
    /// in `WritingFinalized` mode and no more entries can be added.
    pub fn finalize(&mut self) -> ZipResult<()> {
        if self.mode != ZipMode::Writing {
            return Err(ZipError::InvalidParameter);
        }

        let central_dir_offset = self.sink.stream_position().map_err(ZipError::Io)?;

        // Write each central directory entry.
        for entry in &self.entries {
            let name_bytes = entry.file_name.as_bytes();
            if name_bytes.len() > u16::MAX as usize {
                return Err(ZipError::InvalidFilename);
            }
            let extra_len = entry.extra_field.len();
            let comment_bytes = entry.comment.as_bytes();

            write_u32_le(&mut self.sink, CENTRAL_DIR_SIGNATURE)?;
            write_u16_le(&mut self.sink, entry.version_made_by)?;
            write_u16_le(&mut self.sink, entry.version_needed)?;
            write_u16_le(&mut self.sink, entry.flags)?;
            write_u16_le(&mut self.sink, entry.compression_method.to_wire())?;
            write_u16_le(&mut self.sink, entry.last_mod_time)?;
            write_u16_le(&mut self.sink, entry.last_mod_date)?;
            write_u32_le(&mut self.sink, entry.crc32)?;
            write_u32_le(&mut self.sink, entry.compressed_size as u32)?;
            write_u32_le(&mut self.sink, entry.uncompressed_size as u32)?;
            write_u16_le(&mut self.sink, name_bytes.len() as u16)?;
            write_u16_le(&mut self.sink, extra_len as u16)?;
            write_u16_le(&mut self.sink, comment_bytes.len() as u16)?;
            write_u16_le(&mut self.sink, entry.disk_number_start)?;
            write_u16_le(&mut self.sink, entry.internal_attributes)?;
            write_u32_le(&mut self.sink, entry.external_attributes)?;
            write_u32_le(&mut self.sink, entry.local_header_offset as u32)?;
            self.sink.write_all(name_bytes).map_err(ZipError::Io)?;
            self.sink
                .write_all(&entry.extra_field)
                .map_err(ZipError::Io)?;
            self.sink.write_all(comment_bytes).map_err(ZipError::Io)?;
        }

        let central_dir_end = self.sink.stream_position().map_err(ZipError::Io)?;
        let central_dir_size = central_dir_end - central_dir_offset;

        if self.entries.len() > u16::MAX as usize {
            return Err(ZipError::TooManyFiles);
        }
        if central_dir_size > u32::MAX as u64 || central_dir_offset > u32::MAX as u64 {
            return Err(ZipError::ArchiveTooLarge);
        }

        // EOCD record (22 bytes fixed, zero comment).
        write_u32_le(&mut self.sink, EOCD_SIGNATURE)?;
        write_u16_le(&mut self.sink, 0)?; // disk number
        write_u16_le(&mut self.sink, 0)?; // disk with central directory
        write_u16_le(&mut self.sink, self.entries.len() as u16)?; // entries this disk
        write_u16_le(&mut self.sink, self.entries.len() as u16)?; // total entries
        write_u32_le(&mut self.sink, central_dir_size as u32)?;
        write_u32_le(&mut self.sink, central_dir_offset as u32)?;
        write_u16_le(&mut self.sink, 0)?; // comment length

        self.sink.flush().map_err(ZipError::Io)?;
        self.archive_size = self.sink.stream_position().map_err(ZipError::Io)?;
        self.mode = ZipMode::WritingFinalized;
        Ok(())
    }

    /// Release the underlying sink. Analogue of `mz_zip_writer_end`.
    /// After calling this the writer must not be used again.
    pub fn close(self) -> ZipResult<()> {
        // Dropping self will close the File automatically. The Rust analogue
        // of mz_zip_writer_end is mostly a no-op because Rust's ownership
        // model already closes the file on drop; this function exists for
        // API symmetry with miniz.
        drop(self);
        Ok(())
    }
}

// ---------- tests ----------

#[cfg(test)]
mod writer_tests {
    use super::*;
    use crate::contract::ZipReader;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("noricum-spike-writer-{}-{}.zip", std::process::id(), name));
        p
    }

    #[test]
    fn write_single_entry_round_trips_via_rust_reader() {
        let path = tmp_path("hello");
        {
            let mut w = ZipWriter::create(&path).expect("create");
            w.add_mem("hello.txt", b"hello\n").expect("add");
            w.finalize().expect("finalize");
            w.close().expect("close");
        }

        let mut reader = ZipReader::open(&path).expect("read back");
        assert_eq!(reader.num_entries(), 1);
        let idx = reader.locate_file("hello.txt").expect("locate");
        let body = reader.extract_to_mem(idx).expect("extract");
        assert_eq!(body, b"hello\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_multiple_entries_preserves_order() {
        let path = tmp_path("multi");
        {
            let mut w = ZipWriter::create(&path).expect("create");
            for i in 0..5 {
                let name = format!("file{i}.txt");
                let body = format!("content of file {i}\n");
                w.add_mem(&name, body.as_bytes()).expect("add");
            }
            w.finalize().expect("finalize");
        }

        let reader = ZipReader::open(&path).expect("read back");
        assert_eq!(reader.num_entries(), 5);
        for (i, entry) in reader.entries().iter().enumerate() {
            assert_eq!(entry.file_name, format!("file{i}.txt"));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_empty_archive_is_valid() {
        let path = tmp_path("empty");
        {
            let mut w = ZipWriter::create(&path).expect("create");
            w.finalize().expect("finalize");
        }

        let reader = ZipReader::open(&path).expect("read back");
        assert_eq!(reader.num_entries(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_large_entry_round_trips() {
        let path = tmp_path("large");
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        {
            let mut w = ZipWriter::create(&path).expect("create");
            w.add_mem("big.bin", &big).expect("add");
            w.finalize().expect("finalize");
        }

        let mut reader = ZipReader::open(&path).expect("read back");
        assert_eq!(reader.num_entries(), 1);
        let body = reader.extract_to_mem(0).expect("extract");
        assert_eq!(body, big);
        let _ = std::fs::remove_file(&path);
    }
}
