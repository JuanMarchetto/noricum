//! Rust type contract for the miniz_zip interactive spike.
//!
//! Hand-written from two references:
//!   1. `miniz_zip.h` in this repo — for the observable field set and wire layout.
//!   2. `zip-rs/zip2/src/types.rs` — for idiomatic Rust shape, especially
//!      the "prefer owned sum types over trait objects" pattern.
//!
//! Constraints enforced here (see `marche-main-design-20260411-095155.md`
//! Hour 0 prompt):
//!   (a) `ZipSource` enum instead of `Box<dyn Read + Write + Seek>` — trait
//!       objects with more than one non-auto trait are invalid Rust, and the
//!       pipeline's R14 tried to encode exactly that and died.
//!   (b) No raw pointers anywhere.
//!   (c) No C-style type aliases (`mz_uint32` etc. → native `u32`).
//!   (d) No forward-declared empty structs.
//!   (e) No field names with `m_` prefix.
//!
//! This file is the CONTRACT. Method bodies come in later commits as each
//! C function migrates bottom-up with differential test validation.

use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

// --------------------------------------------------------------------
// Constants mirroring the ZIP spec. miniz_zip.h hard-codes these values;
// we prefer named constants so the call sites read as spec references.
// --------------------------------------------------------------------

/// Minimum ZIP version (PKWARE spec 4.4.3.2).
pub const MIN_VERSION: u8 = 10;

/// Default "version needed to extract" for entries we write.
pub const DEFAULT_VERSION: u8 = 45;

/// Maximum archive filename length miniz_zip accepts. Matches
/// `MZ_ZIP_MAX_ARCHIVE_FILENAME_SIZE`.
pub const MAX_FILENAME: usize = 512;

/// Maximum per-entry comment length. Matches `MZ_ZIP_MAX_ARCHIVE_FILE_COMMENT_SIZE`.
pub const MAX_COMMENT: usize = 512;

// --------------------------------------------------------------------
// Source abstraction: one concrete enum, two variants, no trait objects.
// Every ZipArchive / ZipReader / ZipWriter owns a ZipSource.
// --------------------------------------------------------------------

/// Byte-backing for a ZIP archive.
///
/// Replaces `Box<dyn Read + Write + Seek>` with a concrete sum type. Both
/// variants natively implement `Read`, `Write`, and `Seek`, so the whole enum
/// can implement those traits by delegation.
pub enum ZipSource {
    /// Backed by an on-disk file. Caller is responsible for opening with
    /// appropriate read/write permissions — the variant itself does not
    /// enforce direction.
    File(File),
    /// Backed by an in-memory buffer that can grow.
    Mem(Cursor<Vec<u8>>),
}

impl Read for ZipSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            ZipSource::File(f) => f.read(buf),
            ZipSource::Mem(c) => c.read(buf),
        }
    }
}

impl Write for ZipSource {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            ZipSource::File(f) => f.write(buf),
            ZipSource::Mem(c) => c.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            ZipSource::File(f) => f.flush(),
            ZipSource::Mem(c) => c.flush(),
        }
    }
}

impl Seek for ZipSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            ZipSource::File(f) => f.seek(pos),
            ZipSource::Mem(c) => c.seek(pos),
        }
    }
}

// --------------------------------------------------------------------
// Enums: compression method, mode, error. Each mirrors its miniz_zip.h
// counterpart but with Rust-native names and values.
// --------------------------------------------------------------------

/// Compression method for a stored entry. Values match the ZIP spec
/// (APPNOTE.TXT 4.4.5), so an `as u16` cast is lossless.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionMethod {
    /// `ZIP_STORED` — the file is stored verbatim with no compression.
    Stored = 0,
    /// `ZIP_DEFLATED` — classic deflate, the workhorse method.
    Deflated = 8,
    /// Any method we recognise but don't implement. The inner value carries
    /// the spec value so errors can report exactly what was in the archive.
    Unsupported(u16),
}

impl CompressionMethod {
    pub fn from_wire(value: u16) -> Self {
        match value {
            0 => CompressionMethod::Stored,
            8 => CompressionMethod::Deflated,
            other => CompressionMethod::Unsupported(other),
        }
    }

    pub fn to_wire(self) -> u16 {
        match self {
            CompressionMethod::Stored => 0,
            CompressionMethod::Deflated => 8,
            CompressionMethod::Unsupported(v) => v,
        }
    }
}

/// Mode the archive is currently in. Mirrors `mz_zip_mode` exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZipMode {
    Invalid,
    Reading,
    Writing,
    WritingFinalized,
}

/// Error surface for the spike. Each variant corresponds to an `mz_zip_error`
/// case. `Io` bubbles up I/O failures from the underlying `ZipSource`.
#[derive(Debug)]
pub enum ZipError {
    UndefinedError,
    TooManyFiles,
    FileTooLarge,
    UnsupportedMethod,
    UnsupportedEncryption,
    UnsupportedFeature,
    FailedFindingCentralDir,
    NotAnArchive,
    InvalidHeaderOrCorrupted,
    UnsupportedMultidisk,
    DecompressionFailed,
    CompressionFailed,
    UnexpectedDecompressedSize,
    CrcCheckFailed,
    UnsupportedCdirSize,
    AllocFailed,
    FileOpenFailed,
    FileCreateFailed,
    FileWriteFailed,
    FileReadFailed,
    FileCloseFailed,
    FileSeekFailed,
    FileStatFailed,
    InvalidParameter,
    InvalidFilename,
    BufTooSmall,
    InternalError,
    FileNotFound,
    ArchiveTooLarge,
    ValidationFailed,
    WriteCallbackFailed,
    /// Wraps a lower-level `std::io::Error` (io::Error does not implement
    /// Clone or Eq, so neither does `ZipError`).
    Io(io::Error),
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ZipError::Io(e) => write!(f, "io: {e}"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl std::error::Error for ZipError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ZipError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ZipError {
    fn from(e: io::Error) -> Self {
        ZipError::Io(e)
    }
}

/// Result alias used throughout the spike.
pub type ZipResult<T> = Result<T, ZipError>;

// --------------------------------------------------------------------
// Spec-level headers: CentralDirHeader and LocalFileHeader.
// Field names follow APPNOTE.TXT 4.3.7 / 4.3.12 exactly (no `m_*` prefix).
// Both are plain data — parsing/serializing lives in the impl, not the type.
// --------------------------------------------------------------------

/// One central directory entry (APPNOTE.TXT 4.3.12).
///
/// Widened to u64 for sizes and offsets to cover zip64 transparently.
#[derive(Clone, Debug)]
pub struct CentralDirHeader {
    pub version_made_by: u16,
    pub version_needed: u16,
    pub flags: u16,
    pub compression_method: CompressionMethod,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub disk_number_start: u16,
    pub internal_attributes: u16,
    pub external_attributes: u32,
    pub local_header_offset: u64,
    pub file_name: String,
    pub extra_field: Vec<u8>,
    pub comment: String,
}

/// One local file header (APPNOTE.TXT 4.3.7), preceding each entry's
/// compressed data on the wire.
#[derive(Clone, Debug)]
pub struct LocalFileHeader {
    pub version_needed: u16,
    pub flags: u16,
    pub compression_method: CompressionMethod,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub file_name: String,
    pub extra_field: Vec<u8>,
}

// --------------------------------------------------------------------
// Archive, Reader, Writer: the three top-level handles.
// --------------------------------------------------------------------

/// The parsed central directory + bookkeeping. One `ZipArchive` owns one
/// `ZipSource` (no borrowing across threads, no shared ownership).
pub struct ZipArchive {
    pub source: ZipSource,
    pub archive_size: u64,
    pub central_directory_offset: u64,
    pub total_files: u32,
    pub mode: ZipMode,
    pub last_error: Option<ZipError>,
    pub entries: Vec<CentralDirHeader>,
}

/// Reader handle for extracting entries from an existing archive.
///
/// This is the Rust analogue of `mz_zip_archive` in `MZ_ZIP_MODE_READING`.
/// It composes `ZipArchive` rather than re-declaring the fields so parsing
/// code can return a `ZipArchive` and the reader wraps it at the API edge.
pub struct ZipReader {
    pub archive: ZipArchive,
}

/// Writer handle for building a new archive. Distinct from `ZipReader` so
/// that misuse (`locate_file` on a writer, `add_mem` on a reader) is a
/// compile-time error rather than an `MZ_ZIP_INVALID_PARAMETER` at runtime.
pub struct ZipWriter {
    pub sink: ZipSource,
    pub entries: Vec<CentralDirHeader>,
    pub archive_size: u64,
    pub file_offset_alignment: u64,
    pub mode: ZipMode,
}

// --------------------------------------------------------------------
// Sanity tests that exercise nothing but the contract's compilation.
// --------------------------------------------------------------------

#[cfg(test)]
mod contract_compiles {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn zip_source_mem_is_read_write_seek() {
        let mut s = ZipSource::Mem(Cursor::new(Vec::new()));
        s.write_all(b"hello").unwrap();
        s.seek(SeekFrom::Start(0)).unwrap();
        let mut buf = [0u8; 5];
        s.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn compression_method_round_trips() {
        for wire in [0u16, 8, 12, 99] {
            assert_eq!(CompressionMethod::from_wire(wire).to_wire(), wire);
        }
    }

    #[test]
    fn zip_error_display_includes_io_cause() {
        let ioerr = io::Error::other("boom");
        let wrapped = ZipError::from(ioerr);
        let msg = format!("{wrapped}");
        assert!(msg.contains("io:"));
        assert!(msg.contains("boom"));
    }

    #[test]
    fn archive_holds_entries() {
        let archive = ZipArchive {
            source: ZipSource::Mem(Cursor::new(Vec::new())),
            archive_size: 0,
            central_directory_offset: 0,
            total_files: 0,
            mode: ZipMode::Invalid,
            last_error: None,
            entries: Vec::new(),
        };
        assert!(archive.entries.is_empty());
    }
}
