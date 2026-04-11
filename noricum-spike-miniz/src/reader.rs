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
/// Signature for the Zip64 End Of Central Directory Locator.
pub const ZIP64_EOCD_LOCATOR_SIGNATURE: u32 = 0x0706_4b50;
/// Signature for the Zip64 End Of Central Directory record.
pub const ZIP64_EOCD_SIGNATURE: u32 = 0x0606_4b50;
/// Fixed size of the EOCD record excluding the variable-length comment.
pub const EOCD_FIXED_SIZE: usize = 22;
/// Fixed size of a central directory entry excluding name/extra/comment.
pub const CENTRAL_ENTRY_FIXED_SIZE: usize = 46;
/// Fixed size of the Zip64 EOCD Locator record.
pub const ZIP64_EOCD_LOCATOR_SIZE: usize = 20;
/// Minimum fixed size of the Zip64 EOCD record (APPNOTE 4.3.14).
pub const ZIP64_EOCD_MIN_SIZE: usize = 56;
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

fn read_u64_le(buf: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
        buf[offset + 4],
        buf[offset + 5],
        buf[offset + 6],
        buf[offset + 7],
    ])
}

// ---- EOCD discovery ------------------------------------------------

/// Parsed EOCD record, widened to u64 so the zip64 path can fill it in
/// without changing the downstream code path.
#[derive(Debug, Clone, Copy)]
struct EocdInfo {
    /// Number of central directory entries.
    num_entries: u64,
    /// Size of the central directory in bytes.
    central_dir_size: u64,
    /// Absolute offset of the central directory in the archive.
    central_dir_offset: u64,
}

/// Locate the EOCD by scanning backwards from end of file. Returns the
/// parsed EOCD plus the byte offset where it starts. If the standard EOCD
/// uses zip64 sentinels, this function transparently consults the
/// Zip64 EOCD Locator + Zip64 EOCD records and returns the 64-bit values.
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
                let mut info = EocdInfo {
                    num_entries: read_u16_le(&buf, idx + 10) as u64,
                    central_dir_size: read_u32_le(&buf, idx + 12) as u64,
                    central_dir_offset: read_u32_le(&buf, idx + 16) as u64,
                };
                let eocd_absolute_offset = search_start + idx as u64;

                let needs_zip64 = info.num_entries == u16::MAX as u64
                    || info.central_dir_size == u32::MAX as u64
                    || info.central_dir_offset == u32::MAX as u64;
                if needs_zip64 {
                    upgrade_eocd_to_zip64(source, eocd_absolute_offset, &mut info)?;
                }

                return Ok((info, eocd_absolute_offset));
            }
        }
        scan -= 1;
    }

    Err(ZipError::FailedFindingCentralDir)
}

/// Given a standard EOCD that uses zip64 sentinels, read the Zip64 EOCD
/// Locator immediately preceding it, follow the pointer to the Zip64 EOCD,
/// and overwrite the u64 fields of `info` with the authoritative values.
fn upgrade_eocd_to_zip64(
    source: &mut ZipSource,
    std_eocd_offset: u64,
    info: &mut EocdInfo,
) -> ZipResult<()> {
    if std_eocd_offset < ZIP64_EOCD_LOCATOR_SIZE as u64 {
        return Err(ZipError::FailedFindingCentralDir);
    }
    let locator_offset = std_eocd_offset - ZIP64_EOCD_LOCATOR_SIZE as u64;

    source.seek(SeekFrom::Start(locator_offset))?;
    let mut locator = [0u8; ZIP64_EOCD_LOCATOR_SIZE];
    source.read_exact(&mut locator)?;
    if read_u32_le(&locator, 0) != ZIP64_EOCD_LOCATOR_SIGNATURE {
        return Err(ZipError::FailedFindingCentralDir);
    }
    let zip64_eocd_offset = read_u64_le(&locator, 8);

    source.seek(SeekFrom::Start(zip64_eocd_offset))?;
    let mut z64 = [0u8; ZIP64_EOCD_MIN_SIZE];
    source.read_exact(&mut z64)?;
    if read_u32_le(&z64, 0) != ZIP64_EOCD_SIGNATURE {
        return Err(ZipError::InvalidHeaderOrCorrupted);
    }
    // size_of_zip64_end_of_central_directory_record is at offset 4 (u64);
    // we ignore it because we only care about the core fields below.
    info.num_entries = read_u64_le(&z64, 32);
    info.central_dir_size = read_u64_le(&z64, 40);
    info.central_dir_offset = read_u64_le(&z64, 48);
    Ok(())
}

// ---- Central directory entry parsing -------------------------------

/// Zip64 extended information extra field header ID (APPNOTE.TXT 4.5.3).
pub const ZIP64_EXTRA_HEADER_ID: u16 = 0x0001;

/// Parse the zip64 extended information extra field inside a central dir
/// entry's extra field blob. The zip64 extra only contains values for
/// fields that have the 0xFFFFFFFF sentinel in the main record (or 0xFFFF
/// for the disk number). Order is fixed: uncompressed_size, compressed_size,
/// local_header_offset, disk_number_start — but each is PRESENT only if the
/// corresponding main field is a sentinel.
fn apply_zip64_extra(
    extra: &[u8],
    compressed_size: &mut u64,
    uncompressed_size: &mut u64,
    local_header_offset: &mut u64,
    disk_number_start: &mut u16,
) {
    // Walk all extra field blocks looking for header ID 0x0001.
    let mut i = 0usize;
    while i + 4 <= extra.len() {
        let header_id = read_u16_le(extra, i);
        let data_size = read_u16_le(extra, i + 2) as usize;
        if i + 4 + data_size > extra.len() {
            return; // malformed, stop walking
        }
        if header_id == ZIP64_EXTRA_HEADER_ID {
            let mut off = i + 4;
            let end = i + 4 + data_size;

            // Per APPNOTE 4.5.3, values appear only if their main field
            // is sentinel, in this exact order.
            if *uncompressed_size == u32::MAX as u64 && off + 8 <= end {
                *uncompressed_size = u64::from_le_bytes([
                    extra[off], extra[off + 1], extra[off + 2], extra[off + 3],
                    extra[off + 4], extra[off + 5], extra[off + 6], extra[off + 7],
                ]);
                off += 8;
            }
            if *compressed_size == u32::MAX as u64 && off + 8 <= end {
                *compressed_size = u64::from_le_bytes([
                    extra[off], extra[off + 1], extra[off + 2], extra[off + 3],
                    extra[off + 4], extra[off + 5], extra[off + 6], extra[off + 7],
                ]);
                off += 8;
            }
            if *local_header_offset == u32::MAX as u64 && off + 8 <= end {
                *local_header_offset = u64::from_le_bytes([
                    extra[off], extra[off + 1], extra[off + 2], extra[off + 3],
                    extra[off + 4], extra[off + 5], extra[off + 6], extra[off + 7],
                ]);
                off += 8;
            }
            if *disk_number_start == u16::MAX && off + 4 <= end {
                *disk_number_start = read_u16_le(extra, off) as u16;
                // (ignore upper half of the u32)
            }
            return;
        }
        i += 4 + data_size;
    }
}

/// Parse one central directory entry starting at `cursor` inside `buf`.
/// Returns the populated header and the total bytes consumed (fixed size
/// plus variable fields). Applies any zip64 extra field overrides before
/// returning.
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
    let mut compressed_size = read_u32_le(b, 20) as u64;
    let mut uncompressed_size = read_u32_le(b, 24) as u64;
    let file_name_length = read_u16_le(b, 28) as usize;
    let extra_field_length = read_u16_le(b, 30) as usize;
    let file_comment_length = read_u16_le(b, 32) as usize;
    let mut disk_number_start = read_u16_le(b, 34);
    let internal_attributes = read_u16_le(b, 36);
    let external_attributes = read_u32_le(b, 38);
    let mut local_header_offset = read_u32_le(b, 42) as u64;

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

    // Apply zip64 extra field overrides for any sentinel fields.
    apply_zip64_extra(
        &extra_field,
        &mut compressed_size,
        &mut uncompressed_size,
        &mut local_header_offset,
        &mut disk_number_start,
    );

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
    // After `find_eocd` promotes to zip64 as needed, these are the real
    // 64-bit values. We still cap at a sane limit to avoid allocating
    // absurd Vecs on corrupt inputs.
    if eocd.central_dir_size > usize::MAX as u64 {
        return Err(ZipError::ArchiveTooLarge);
    }
    if eocd.num_entries > usize::MAX as u64 {
        return Err(ZipError::TooManyFiles);
    }

    source.seek(SeekFrom::Start(eocd.central_dir_offset))?;
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

// ---- Local file header parsing (needed for extract_to_mem) --------

/// Signature for a local file header.
pub const LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
/// Fixed size of the local file header block (excluding name/extra).
pub const LOCAL_HEADER_FIXED_SIZE: usize = 30;

/// Parse a local file header at the given offset in the archive. Returns
/// the absolute byte offset of the compressed data that follows the
/// header (fixed size + file_name_length + extra_field_length).
fn find_data_start(source: &mut ZipSource, local_header_offset: u64) -> ZipResult<u64> {
    source.seek(SeekFrom::Start(local_header_offset))?;
    let mut hdr = [0u8; LOCAL_HEADER_FIXED_SIZE];
    source.read_exact(&mut hdr)?;

    if read_u32_le(&hdr, 0) != LOCAL_HEADER_SIGNATURE {
        return Err(ZipError::InvalidHeaderOrCorrupted);
    }

    let file_name_length = read_u16_le(&hdr, 26) as u64;
    let extra_field_length = read_u16_le(&hdr, 28) as u64;

    Ok(local_header_offset + LOCAL_HEADER_FIXED_SIZE as u64 + file_name_length + extra_field_length)
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

        let total_files = u32::try_from(eocd.num_entries).unwrap_or(u32::MAX);
        Ok(ZipReader {
            archive: ZipArchive {
                source,
                archive_size: file_size,
                central_directory_offset: eocd.central_dir_offset,
                total_files,
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

    /// Rust analogue of `mz_zip_reader_locate_file`. Linear scan over the
    /// central directory for an entry whose filename matches `name` byte
    /// for byte. Returns the entry's index, or `FileNotFound`.
    pub fn locate_file(&self, name: &str) -> ZipResult<usize> {
        self.archive
            .entries
            .iter()
            .position(|e| e.file_name == name)
            .ok_or(ZipError::FileNotFound)
    }

    /// Rust analogue of `mz_zip_reader_file_stat`. Returns the central
    /// directory header at the given index.
    pub fn stat(&self, file_index: usize) -> ZipResult<&CentralDirHeader> {
        self.archive
            .entries
            .get(file_index)
            .ok_or(ZipError::InvalidParameter)
    }

    /// Rust analogue of `mz_zip_reader_extract_to_mem`. Decompresses the
    /// entry at `file_index` into a freshly allocated `Vec<u8>` whose
    /// length equals `uncompressed_size`. CRC32 is verified against the
    /// central directory record.
    pub fn extract_to_mem(&mut self, file_index: usize) -> ZipResult<Vec<u8>> {
        // Clone the header fields we need before re-borrowing self.archive.source mutably.
        let entry = self
            .archive
            .entries
            .get(file_index)
            .ok_or(ZipError::InvalidParameter)?;
        let method = entry.compression_method;
        let compressed_size = entry.compressed_size;
        let uncompressed_size = entry.uncompressed_size;
        let expected_crc32 = entry.crc32;
        let local_header_offset = entry.local_header_offset;

        if uncompressed_size == 0 {
            return Ok(Vec::new());
        }

        let data_start = find_data_start(&mut self.archive.source, local_header_offset)?;
        self.archive.source.seek(SeekFrom::Start(data_start))?;

        let mut out = Vec::with_capacity(uncompressed_size as usize);

        match method {
            CompressionMethod::Stored => {
                let mut taken = (&mut self.archive.source).take(compressed_size);
                taken
                    .read_to_end(&mut out)
                    .map_err(|_| ZipError::FileReadFailed)?;
            }
            CompressionMethod::Deflated => {
                use flate2::read::DeflateDecoder;
                let taken = (&mut self.archive.source).take(compressed_size);
                let mut decoder = DeflateDecoder::new(taken);
                decoder
                    .read_to_end(&mut out)
                    .map_err(|_| ZipError::DecompressionFailed)?;
            }
            CompressionMethod::Unsupported(_) => return Err(ZipError::UnsupportedMethod),
        }

        if out.len() as u64 != uncompressed_size {
            return Err(ZipError::UnexpectedDecompressedSize);
        }

        let actual_crc32 = crc32fast::hash(&out);
        if actual_crc32 != expected_crc32 {
            return Err(ZipError::CrcCheckFailed);
        }

        Ok(out)
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
    fn locate_and_stat_round_trip() {
        let reader = ZipReader::open(fixture("multi_small.zip")).expect("open");
        let idx = reader.locate_file("file3.txt").expect("locate");
        let stat = reader.stat(idx).expect("stat");
        assert_eq!(stat.file_name, "file3.txt");
        assert_eq!(stat.uncompressed_size, "content of file 3\n".len() as u64);
    }

    #[test]
    fn locate_missing_returns_file_not_found() {
        let reader = ZipReader::open(fixture("hello.zip")).expect("open");
        match reader.locate_file("nope.txt") {
            Err(ZipError::FileNotFound) => {}
            other => panic!("expected FileNotFound, got {other:?}"),
        }
    }

    #[test]
    fn extract_hello_txt_from_hello_zip() {
        let mut reader = ZipReader::open(fixture("hello.zip")).expect("open");
        let idx = reader.locate_file("hello.txt").expect("locate");
        let body = reader.extract_to_mem(idx).expect("extract");
        assert_eq!(body, b"hello\n");
    }

    #[test]
    fn extract_deflated_compresses_and_decompresses_back() {
        let mut reader = ZipReader::open(fixture("deflated.zip")).expect("open");
        let idx = reader.locate_file("deflated.txt").expect("locate");
        let body = reader.extract_to_mem(idx).expect("extract");
        let expected: Vec<u8> = b"this should compress well "
            .iter()
            .cycle()
            .take(26 * 256)
            .copied()
            .collect();
        assert_eq!(body, expected);
    }

    #[test]
    fn extract_stored_passes_through() {
        let mut reader = ZipReader::open(fixture("stored_only.zip")).expect("open");
        let idx = reader.locate_file("stored.txt").expect("locate");
        let body = reader.extract_to_mem(idx).expect("extract");
        assert_eq!(body, b"no compression here\n");
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
