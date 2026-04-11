//! ZIP reader implementation — function-by-function bottom-up migration.
//!
//! First function: `ZipReader::open`, the Rust analogue of
//! `mz_zip_reader_init_file` in `miniz_zip.c`. It opens a file, walks back
//! from the end looking for the End Of Central Directory (EOCD) record,
//! parses the EOCD, then parses every central directory entry into
//! `ZipArchive::entries`.
//!
//! Byte layouts are straight from APPNOTE.TXT (ZIP spec) and were
//! cross-checked against `zip-rs/zip2/src/spec.rs` as an architectural
//! reference.
//!
//! Scope of this commit:
//!   - non-zip64 archives only
//!   - reader path only (writer comes later)
//!   - `open`, `num_entries`, `entries`
//!
//! zip64 support will land in a follow-up commit once hour-3 gate passes.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::contract::{
    CentralDirHeader, CompressionMethod, ZipArchive, ZipError, ZipMode, ZipReader, ZipResult,
    ZipSource,
};

/// Signature for the End Of Central Directory (EOCD) record.
pub const EOCD_SIGNATURE: u32 = 0x0605_4b50;
/// Signature for a central directory file header.
pub const CENTRAL_DIR_SIGNATURE: u32 = 0x0201_4b50;
/// Fixed size of the EOCD record excluding the variable-length comment.
pub const EOCD_FIXED_SIZE: usize = 22;
/// Fixed size of a central directory entry excluding name/extra/comment.
pub const CENTRAL_ENTRY_FIXED_SIZE: usize = 46;
/// Maximum comment length the EOCD can describe (u16 limit).
pub const MAX_EOCD_COMMENT: usize = u16::MAX as usize;

// ---- Little-endian read helpers ------------------------------------

fn read_u16_le(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

// ---- EOCD discovery ------------------------------------------------

/// Parsed EOCD record — only the fields we care about during the
/// reader init path.
#[derive(Debug, Clone, Copy)]
struct EocdInfo {
    /// Number of central directory entries.
    num_entries: u32,
    /// Size of the central directory in bytes.
    central_dir_size: u32,
    /// Absolute offset of the central directory in the archive.
    central_dir_offset: u32,
}

/// Locate the EOCD by scanning backwards from end of file. Returns the
/// parsed EOCD plus the byte offset where it starts.
///
/// Scanning strategy matches the standard: read the last
/// `EOCD_FIXED_SIZE + MAX_EOCD_COMMENT` bytes (or the whole file if
/// smaller), find the last occurrence of the magic, validate the
/// declared comment length fits exactly to EOF.
fn find_eocd(source: &mut ZipSource, file_size: u64) -> ZipResult<(EocdInfo, u64)> {
    if file_size < EOCD_FIXED_SIZE as u64 {
        return Err(ZipError::NotAnArchive);
    }

    let max_search = (EOCD_FIXED_SIZE + MAX_EOCD_COMMENT) as u64;
    let search_start = file_size.saturating_sub(max_search);
    let search_len = (file_size - search_start) as usize;

    let mut buf = vec![0u8; search_len];
    source.seek(SeekFrom::Start(search_start))?;
    source.read_exact(&mut buf)?;

    // Walk backwards looking for the magic. The signature must sit at a
    // position where the 22 fixed bytes + declared comment length lines
    // up exactly with end-of-file.
    let mut scan: isize = search_len as isize - EOCD_FIXED_SIZE as isize;
    while scan >= 0 {
        let idx = scan as usize;
        if read_u32_le(&buf, idx) == EOCD_SIGNATURE {
            let comment_len = read_u16_le(&buf, idx + 20) as usize;
            if idx + EOCD_FIXED_SIZE + comment_len == search_len {
                let info = EocdInfo {
                    num_entries: read_u16_le(&buf, idx + 10) as u32,
                    central_dir_size: read_u32_le(&buf, idx + 12),
                    central_dir_offset: read_u32_le(&buf, idx + 16),
                };
                return Ok((info, search_start + idx as u64));
            }
        }
        scan -= 1;
    }

    Err(ZipError::FailedFindingCentralDir)
}

// ---- Central directory entry parsing -------------------------------

/// Parse one central directory entry starting at `cursor` inside `buf`.
/// Returns the populated header and the total bytes consumed (fixed size
/// plus variable fields).
fn parse_central_entry(buf: &[u8], cursor: usize) -> ZipResult<(CentralDirHeader, usize)> {
    if buf.len() < cursor + CENTRAL_ENTRY_FIXED_SIZE {
        return Err(ZipError::InvalidHeaderOrCorrupted);
    }
    let b = &buf[cursor..];

    if read_u32_le(b, 0) != CENTRAL_DIR_SIGNATURE {
        return Err(ZipError::InvalidHeaderOrCorrupted);
    }

    let version_made_by = read_u16_le(b, 4);
    let version_needed = read_u16_le(b, 6);
    let flags = read_u16_le(b, 8);
    let compression_method = CompressionMethod::from_wire(read_u16_le(b, 10));
    let last_mod_time = read_u16_le(b, 12);
    let last_mod_date = read_u16_le(b, 14);
    let crc32 = read_u32_le(b, 16);
    let compressed_size = read_u32_le(b, 20) as u64;
    let uncompressed_size = read_u32_le(b, 24) as u64;
    let file_name_length = read_u16_le(b, 28) as usize;
    let extra_field_length = read_u16_le(b, 30) as usize;
    let file_comment_length = read_u16_le(b, 32) as usize;
    let disk_number_start = read_u16_le(b, 34);
    let internal_attributes = read_u16_le(b, 36);
    let external_attributes = read_u32_le(b, 38);
    let local_header_offset = read_u32_le(b, 42) as u64;

    let total = CENTRAL_ENTRY_FIXED_SIZE + file_name_length + extra_field_length + file_comment_length;
    if buf.len() < cursor + total {
        return Err(ZipError::InvalidHeaderOrCorrupted);
    }

    let name_start = cursor + CENTRAL_ENTRY_FIXED_SIZE;
    let extra_start = name_start + file_name_length;
    let comment_start = extra_start + extra_field_length;
    let comment_end = comment_start + file_comment_length;

    // ZIP spec allows UTF-8 filenames when flag bit 11 is set; otherwise
    // CP437. For the spike we accept both by using from_utf8_lossy — the
    // differential test compares byte-level content via the wrapper, so
    // filename encoding is cosmetic here.
    let file_name = String::from_utf8_lossy(&buf[name_start..extra_start]).into_owned();
    let extra_field = buf[extra_start..comment_start].to_vec();
    let comment = String::from_utf8_lossy(&buf[comment_start..comment_end]).into_owned();

    Ok((
        CentralDirHeader {
            version_made_by,
            version_needed,
            flags,
            compression_method,
            last_mod_time,
            last_mod_date,
            crc32,
            compressed_size,
            uncompressed_size,
            disk_number_start,
            internal_attributes,
            external_attributes,
            local_header_offset,
            file_name,
            extra_field,
            comment,
        },
        total,
    ))
}

fn parse_central_directory(
    source: &mut ZipSource,
    eocd: EocdInfo,
) -> ZipResult<Vec<CentralDirHeader>> {
    if eocd.central_dir_offset == u32::MAX || eocd.num_entries == u16::MAX as u32 {
        // zip64 territory — not supported yet in this commit. hello/empty/
        // multi_small are non-zip64, which is what the hour-3 gate needs.
        return Err(ZipError::UnsupportedFeature);
    }

    source.seek(SeekFrom::Start(eocd.central_dir_offset as u64))?;
    let mut buf = vec![0u8; eocd.central_dir_size as usize];
    source.read_exact(&mut buf)?;

    let mut entries = Vec::with_capacity(eocd.num_entries as usize);
    let mut cursor = 0usize;
    for _ in 0..eocd.num_entries {
        let (hdr, consumed) = parse_central_entry(&buf, cursor)?;
        cursor += consumed;
        entries.push(hdr);
    }

    Ok(entries)
}

// ---- Public ZipReader API ------------------------------------------

impl ZipReader {
    /// Open a ZIP archive from a filesystem path. This is the Rust
    /// analogue of `mz_zip_reader_init_file` in `miniz_zip.c`.
    pub fn open<P: AsRef<Path>>(path: P) -> ZipResult<Self> {
        let file = File::open(path.as_ref()).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ZipError::FileNotFound,
            _ => ZipError::FileOpenFailed,
        })?;
        let file_size = file.metadata().map(|m| m.len()).map_err(ZipError::Io)?;

        let mut source = ZipSource::File(file);
        let (eocd, _eocd_offset) = find_eocd(&mut source, file_size)?;
        let entries = parse_central_directory(&mut source, eocd)?;

        Ok(ZipReader {
            archive: ZipArchive {
                source,
                archive_size: file_size,
                central_directory_offset: eocd.central_dir_offset as u64,
                total_files: eocd.num_entries,
                mode: ZipMode::Reading,
                last_error: None,
                entries,
            },
        })
    }

    /// Number of entries in the central directory.
    pub fn num_entries(&self) -> usize {
        self.archive.entries.len()
    }

    /// Slice of all parsed entries.
    pub fn entries(&self) -> &[CentralDirHeader] {
        &self.archive.entries
    }
}

// ---- Unit tests ----------------------------------------------------

#[cfg(test)]
mod reader_tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("zip_corpus")
            .join(name)
    }

    #[test]
    fn opens_hello_zip_and_sees_one_entry() {
        let reader = ZipReader::open(fixture("hello.zip")).expect("hello.zip open");
        assert_eq!(reader.num_entries(), 1);
        let entry = &reader.entries()[0];
        assert_eq!(entry.file_name, "hello.txt");
        assert_eq!(entry.uncompressed_size, 6);
        assert_eq!(entry.crc32, 0x363a_3020);
    }

    #[test]
    fn opens_empty_zip_and_sees_zero_entries() {
        let reader = ZipReader::open(fixture("empty.zip")).expect("empty.zip open");
        assert_eq!(reader.num_entries(), 0);
    }

    #[test]
    fn opens_multi_small_and_sees_five_entries_in_order() {
        let reader = ZipReader::open(fixture("multi_small.zip")).expect("multi_small.zip open");
        assert_eq!(reader.num_entries(), 5);
        let names: Vec<&str> = reader.entries().iter().map(|e| e.file_name.as_str()).collect();
        assert_eq!(names, vec!["file0.txt", "file1.txt", "file2.txt", "file3.txt", "file4.txt"]);
        // All five should be nonzero-sized with same pattern "content of file N\n".
        for (i, entry) in reader.entries().iter().enumerate() {
            let expected = format!("content of file {i}\n");
            assert_eq!(entry.uncompressed_size, expected.len() as u64);
        }
    }

    #[test]
    fn non_zip_file_returns_not_an_archive_or_failed_finding() {
        // miniz_zip.h itself is decidedly not a zip.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("miniz_zip.h");
        let result = ZipReader::open(&path);
        match result {
            Ok(_) => panic!("expected error, got Ok opening a non-zip file"),
            Err(ZipError::NotAnArchive) | Err(ZipError::FailedFindingCentralDir) => {}
            Err(other) => panic!("expected NotAnArchive or FailedFindingCentralDir, got {other:?}"),
        }
    }
}
