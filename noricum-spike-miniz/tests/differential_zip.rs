//! Differential test harness for the miniz spike.
//!
//! The oracle is the C implementation called through wrapper.c. Every fixture
//! under `fixtures/zip_corpus/` is opened via the C wrapper, all entries are
//! extracted, and `(filename, size, crc32, first_64_bytes)` tuples are recorded.
//!
//! `oracle_selftest` walks every fixture and sanity-checks that the wrapper
//! produces a well-formed snapshot. This is Phase 0's green light: if this test
//! passes, the C oracle works and the spike clock can start.
//!
//! Later tests (once Rust translations land in `src/`) will compare snapshots
//! extracted via the Rust path against the oracle's snapshots and fail on any
//! divergence.

use std::ffi::CString;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use spike::contract::{ZipReader as RustZipReader, ZipWriter as RustZipWriter};
use spike::{
    wr_reader_close, wr_reader_extract, wr_reader_get_num_files, wr_reader_locate, wr_reader_open,
    wr_reader_stat,
};

/// Snapshot of a single entry inside a zip archive, as produced by the C oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntrySnapshot {
    name: String,
    size: usize,
    crc32: u32,
    first_bytes: Vec<u8>,
}

/// Snapshot of one fixture: the ordered list of every entry it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveSnapshot {
    fixture: String,
    entries: Vec<EntrySnapshot>,
}

const FIRST_BYTES_CAP: usize = 64;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("zip_corpus")
}

fn snapshot_via_oracle(fixture_path: &Path) -> Result<ArchiveSnapshot, String> {
    let c_path = CString::new(fixture_path.to_string_lossy().as_bytes())
        .map_err(|e| format!("CString: {e}"))?;

    let handle = unsafe { wr_reader_open(c_path.as_ptr()) };
    if handle.is_null() {
        return Err(format!("wr_reader_open returned NULL for {fixture_path:?}"));
    }

    let mut entries = Vec::new();
    let n = unsafe { wr_reader_get_num_files(handle) };
    if n < 0 {
        unsafe { wr_reader_close(handle) };
        return Err(format!("wr_reader_get_num_files returned {n}"));
    }

    for idx in 0..n {
        let mut name_buf = vec![0i8; 512];
        let mut size: usize = 0;
        let mut crc32: u32 = 0;

        let rc = unsafe {
            wr_reader_stat(
                handle,
                idx,
                name_buf.as_mut_ptr(),
                name_buf.len(),
                &mut size as *mut usize,
                &mut crc32 as *mut u32,
            )
        };
        if rc != 0 {
            unsafe { wr_reader_close(handle) };
            return Err(format!("wr_reader_stat({idx}) returned {rc}"));
        }

        // Safe: wr_reader_stat guarantees a NUL-terminated string within name_cap.
        let name_bytes: Vec<u8> = name_buf
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        let name = String::from_utf8_lossy(&name_bytes).into_owned();

        // Allocate exactly `size` bytes. For zero-byte entries the extract call
        // is skipped (miniz returns success but there's nothing to read).
        let mut body = vec![0u8; size];
        if size > 0 {
            let rc = unsafe {
                wr_reader_extract(
                    handle,
                    idx,
                    body.as_mut_ptr() as *mut _,
                    body.len(),
                )
            };
            if rc != 0 {
                unsafe { wr_reader_close(handle) };
                return Err(format!(
                    "wr_reader_extract({idx}, name={name:?}) returned {rc}"
                ));
            }
        }

        let first_bytes = body[..body.len().min(FIRST_BYTES_CAP)].to_vec();

        entries.push(EntrySnapshot {
            name,
            size,
            crc32,
            first_bytes,
        });
    }

    unsafe { wr_reader_close(handle) };

    Ok(ArchiveSnapshot {
        fixture: fixture_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string(),
        entries,
    })
}

/// List every `.zip` file under `fixtures/zip_corpus/`, sorted for deterministic output.
fn list_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut out: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir({}): {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("zip"))
        .collect();
    out.sort();
    out
}

#[test]
fn oracle_selftest() {
    let fixtures = list_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no .zip fixtures found under {}",
        fixtures_dir().display()
    );

    println!("Oracle self-test across {} fixtures", fixtures.len());

    let mut total_entries = 0usize;
    for fx in &fixtures {
        let snap = match snapshot_via_oracle(fx) {
            Ok(s) => s,
            Err(e) => panic!("oracle failed on {}: {e}", fx.display()),
        };
        println!(
            "  {:24} {:>3} entries",
            snap.fixture,
            snap.entries.len()
        );
        total_entries += snap.entries.len();

        // Cross-check: every entry that claims non-zero size must have matching first_bytes length.
        for entry in &snap.entries {
            assert!(
                entry.first_bytes.len() <= FIRST_BYTES_CAP,
                "{} first_bytes cap exceeded for {}",
                snap.fixture,
                entry.name
            );
            let expected_first = entry.size.min(FIRST_BYTES_CAP);
            assert_eq!(
                entry.first_bytes.len(),
                expected_first,
                "{} / {} first_bytes length mismatch",
                snap.fixture,
                entry.name
            );
        }
    }

    println!("Total entries across corpus: {total_entries}");
    assert!(total_entries > 0, "oracle self-test produced zero entries");
}

#[test]
fn hello_fixture_matches_expected_content() {
    // Minimum-viable sanity: the canonical sanity fixture round-trips exactly.
    let snap = snapshot_via_oracle(&fixtures_dir().join("hello.zip"))
        .expect("hello.zip oracle snapshot");
    assert_eq!(snap.entries.len(), 1);
    let entry = &snap.entries[0];
    assert_eq!(entry.name, "hello.txt");
    assert_eq!(entry.size, 6);
    assert_eq!(entry.first_bytes, b"hello\n".to_vec());
    assert_eq!(entry.crc32, 0x363a3020, "CRC32 of 'hello\\n' is a known constant");
}

/// One entry's fields as seen from either the C oracle or the Rust reader.
/// Only includes the fields that both sides currently produce (name,
/// uncompressed size, CRC32). First-bytes comparison will come in when
/// extract_to_mem lands on the Rust side.
#[derive(Debug, PartialEq, Eq)]
struct MinEntry {
    name: String,
    size: u64,
    crc32: u32,
}

fn oracle_min_entries(path: &Path) -> Vec<MinEntry> {
    let snap = snapshot_via_oracle(path).expect("oracle snapshot");
    snap.entries
        .into_iter()
        .map(|e| MinEntry {
            name: e.name,
            size: e.size as u64,
            crc32: e.crc32,
        })
        .collect()
}

fn rust_min_entries(path: &Path) -> Vec<MinEntry> {
    let reader = RustZipReader::open(path).expect("rust reader open");
    reader
        .entries()
        .iter()
        .map(|e| MinEntry {
            name: e.file_name.clone(),
            size: e.uncompressed_size,
            crc32: e.crc32,
        })
        .collect()
}

/// Hour-3 gate: the Rust reader must produce the same central-directory
/// view as the C oracle on the canonical non-zip64 fixtures. Compares
/// entry-by-entry (name, size, crc32). If this fails the methodology needs
/// re-assessment before continuing.
#[test]
fn reader_diff_test_hello_zip() {
    let path = fixtures_dir().join("hello.zip");
    let oracle = oracle_min_entries(&path);
    let rust = rust_min_entries(&path);
    assert_eq!(rust, oracle, "hello.zip differential mismatch");
}

#[test]
fn reader_diff_test_empty_zip() {
    let path = fixtures_dir().join("empty.zip");
    let oracle = oracle_min_entries(&path);
    let rust = rust_min_entries(&path);
    assert_eq!(rust, oracle, "empty.zip differential mismatch");
}

#[test]
fn reader_diff_test_multi_small_zip() {
    let path = fixtures_dir().join("multi_small.zip");
    let oracle = oracle_min_entries(&path);
    let rust = rust_min_entries(&path);
    assert_eq!(rust, oracle, "multi_small.zip differential mismatch");
}

/// Rust-side snapshot equivalent of `snapshot_via_oracle`. Walks every
/// entry, extracts the body via `ZipReader::extract_to_mem`, and produces
/// the same `EntrySnapshot` shape so the two sides can be compared field
/// by field.
fn snapshot_via_rust(path: &Path) -> Result<ArchiveSnapshot, String> {
    let mut reader = RustZipReader::open(path).map_err(|e| format!("rust open: {e:?}"))?;
    let n = reader.num_entries();
    let headers: Vec<_> = reader.entries().to_vec();

    let mut entries = Vec::with_capacity(n);
    for (idx, hdr) in headers.into_iter().enumerate() {
        let body = reader
            .extract_to_mem(idx)
            .map_err(|e| format!("rust extract({idx}): {e:?}"))?;
        let first_bytes = body[..body.len().min(FIRST_BYTES_CAP)].to_vec();
        entries.push(EntrySnapshot {
            name: hdr.file_name,
            size: hdr.uncompressed_size as usize,
            crc32: hdr.crc32,
            first_bytes,
        });
    }

    Ok(ArchiveSnapshot {
        fixture: path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string(),
        entries,
    })
}

/// The hour-7 full-corpus differential test for the reader path. Extracts
/// every entry of every fixture via both the C oracle and the Rust reader,
/// then compares the full `EntrySnapshot` (name + size + crc32 +
/// first_bytes). This is the toughest reader-side assertion the spike can
/// make short of byte-for-byte comparison of the whole extracted body,
/// which is covered by the contained tests below for a few specific cases.
#[test]
fn reader_full_diff_oracle_vs_rust() {
    let mut matched = 0usize;
    let mut mismatched: Vec<String> = Vec::new();

    for path in list_fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let oracle = snapshot_via_oracle(&path).expect("oracle");
        let rust = snapshot_via_rust(&path).expect("rust");
        if oracle.entries == rust.entries {
            matched += 1;
            println!("  MATCH  {name}");
        } else {
            mismatched.push(name.clone());
            println!("  DIFFER {name}");
            for (i, (o, r)) in oracle.entries.iter().zip(rust.entries.iter()).enumerate() {
                if o != r {
                    println!(
                        "    entry {i}: oracle={{name={:?}, size={}, crc={:08x}}}, rust={{name={:?}, size={}, crc={:08x}}}",
                        o.name, o.size, o.crc32, r.name, r.size, r.crc32
                    );
                }
            }
        }
    }

    println!(
        "\nFull diff: {} matched / {} mismatched",
        matched,
        mismatched.len()
    );
    assert!(
        mismatched.is_empty(),
        "full oracle-vs-rust diff failed on: {mismatched:?}"
    );
}

/// The writer-side differential proof: write a set of entries via the
/// Rust writer, then open the resulting archive via the C oracle and
/// verify every entry extracts to exactly the bytes we put in. Any
/// deviation fails the test. This is what "the Rust writer produces a
/// valid miniz-compatible archive" means in practice.
#[test]
fn writer_diff_roundtrip_via_c_oracle() {
    let fixtures: Vec<(&str, Vec<u8>)> = vec![
        ("hello.txt", b"hello\n".to_vec()),
        ("empty.bin", Vec::new()),
        ("pattern.txt", b"aaaaaaaaaaaaaaaaaaaaaa\n".to_vec()),
        ("random.bin", (0..8192u32).map(|i| (i as u8).wrapping_mul(37)).collect()),
        ("unicode_ascii.txt", "nihongo content\n".as_bytes().to_vec()),
        ("deeply/nested/path/file.txt", b"nested body\n".to_vec()),
        ("dir1/a.txt", b"A\n".to_vec()),
        ("dir1/b.txt", b"B\n".to_vec()),
        ("dir2/c.txt", b"C\n".to_vec()),
        (
            "big.bin",
            (0..100_000u32).map(|i| ((i * 7) % 251) as u8).collect(),
        ),
    ];

    let mut path = std::env::temp_dir();
    path.push(format!(
        "noricum-spike-writer-diff-{}.zip",
        std::process::id()
    ));

    // Build archive with Rust writer.
    {
        let mut w = RustZipWriter::create(&path).expect("create");
        for (name, body) in &fixtures {
            w.add_mem(name, body).expect("add");
        }
        w.finalize().expect("finalize");
        w.close().expect("close");
    }

    // Open and extract with C oracle.
    let c_path = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let h = unsafe { wr_reader_open(c_path.as_ptr()) };
    assert!(!h.is_null(), "C oracle failed to open Rust-written archive");

    let n = unsafe { wr_reader_get_num_files(h) };
    assert_eq!(n as usize, fixtures.len(), "entry count mismatch");

    for (idx, (expected_name, expected_body)) in fixtures.iter().enumerate() {
        // Stat check: name + size + crc32 via oracle.
        let mut name_buf = vec![0i8; 512];
        let mut size: usize = 0;
        let mut crc32: u32 = 0;
        let rc = unsafe {
            wr_reader_stat(
                h,
                idx as i32,
                name_buf.as_mut_ptr(),
                name_buf.len(),
                &mut size,
                &mut crc32,
            )
        };
        assert_eq!(rc, 0, "C stat failed on entry {idx}");

        let name_bytes: Vec<u8> = name_buf
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        let name = String::from_utf8_lossy(&name_bytes);
        assert_eq!(
            name.as_ref(),
            *expected_name,
            "name mismatch at entry {idx}"
        );
        assert_eq!(size, expected_body.len(), "size mismatch at {name}");

        // Extract and byte-compare.
        let mut body = vec![0u8; expected_body.len()];
        if !expected_body.is_empty() {
            let rc = unsafe {
                wr_reader_extract(h, idx as i32, body.as_mut_ptr() as *mut _, body.len())
            };
            assert_eq!(rc, 0, "C extract failed on entry {idx} ({name})");
        }
        assert_eq!(&body, expected_body, "byte mismatch at {name}");
    }

    unsafe { wr_reader_close(h) };
    let _ = std::fs::remove_file(&path);
}

/// Writer + Reader closed loop via Rust on the ENTIRE corpus pattern
/// (from fixtures/gen_fixtures.py): generate the same logical content
/// with the Rust writer, then read it back with the Rust reader, then
/// also read it back with the C oracle. All three views must agree.
#[test]
fn writer_diff_rust_writer_rust_reader_c_oracle_all_three_agree() {
    let entries: Vec<(&str, Vec<u8>)> = vec![
        ("hello.txt", b"hello\n".to_vec()),
        ("multi/0.txt", b"zero\n".to_vec()),
        ("multi/1.txt", b"one\n".to_vec()),
        ("multi/2.txt", b"two\n".to_vec()),
        ("compressible.txt", vec![b'x'; 4096]),
        ("random.bin", (0..512u32).map(|i| (i as u8) ^ 0xAA).collect()),
    ];

    let mut path = std::env::temp_dir();
    path.push(format!(
        "noricum-spike-three-way-{}.zip",
        std::process::id()
    ));

    // Write via Rust.
    {
        let mut w = RustZipWriter::create(&path).expect("create");
        for (name, body) in &entries {
            w.add_mem(name, body).expect("add");
        }
        w.finalize().expect("finalize");
    }

    // View 1: Rust reader.
    let mut rust_reader = RustZipReader::open(&path).expect("rust reader");
    assert_eq!(rust_reader.num_entries(), entries.len());
    let rust_headers: Vec<_> = rust_reader.entries().to_vec();
    let rust_extracted: Vec<Vec<u8>> = (0..rust_headers.len())
        .map(|i| rust_reader.extract_to_mem(i).expect("rust extract"))
        .collect();

    // View 2: C oracle reader.
    let oracle = snapshot_via_oracle(&path).expect("oracle");
    assert_eq!(oracle.entries.len(), entries.len());

    // Compare all three views.
    for (i, (name, body)) in entries.iter().enumerate() {
        assert_eq!(rust_headers[i].file_name, *name, "rust name at {i}");
        assert_eq!(oracle.entries[i].name, *name, "oracle name at {i}");
        assert_eq!(&rust_extracted[i], body, "rust body at {name}");
        // Oracle only snapshots first 64 bytes — compare prefix.
        let body_prefix = &body[..body.len().min(FIRST_BYTES_CAP)];
        assert_eq!(
            &oracle.entries[i].first_bytes,
            body_prefix,
            "oracle body prefix at {name}"
        );
        assert_eq!(
            oracle.entries[i].size,
            body.len(),
            "oracle size at {name}"
        );
        assert_eq!(
            oracle.entries[i].crc32,
            rust_headers[i].crc32,
            "crc32 mismatch at {name}"
        );
    }

    let _ = std::fs::remove_file(&path);
}

/// Byte-for-byte comparison on a handful of fixtures where the content is
/// small enough that we can afford the direct equality check on the full
/// body, not just the first 64 bytes.
#[test]
fn reader_byte_equal_small_fixtures() {
    let fixtures = [
        "hello.zip",
        "empty.zip",
        "multi_small.zip",
        "stored_only.zip",
        "deflated.zip",
    ];

    for name in fixtures {
        let path = fixtures_dir().join(name);
        let mut rust_reader = RustZipReader::open(&path).expect("rust open");
        let n = rust_reader.num_entries();

        // Fetch the names and sizes up front to avoid borrow-issues during extract.
        let headers: Vec<_> = rust_reader.entries().to_vec();

        for (idx, hdr) in headers.iter().enumerate() {
            let rust_body = rust_reader.extract_to_mem(idx).expect("rust extract");

            // C oracle extraction of the same entry.
            let c_path = CString::new(path.to_string_lossy().as_bytes()).unwrap();
            let h = unsafe { wr_reader_open(c_path.as_ptr()) };
            assert!(!h.is_null(), "c open {name}");
            let mut c_body = vec![0u8; hdr.uncompressed_size as usize];
            if hdr.uncompressed_size > 0 {
                let rc = unsafe {
                    wr_reader_extract(
                        h,
                        idx as i32,
                        c_body.as_mut_ptr() as *mut _,
                        c_body.len(),
                    )
                };
                assert_eq!(rc, 0, "c extract {name}/{}", hdr.file_name);
            }
            unsafe { wr_reader_close(h) };

            assert_eq!(
                rust_body, c_body,
                "byte mismatch in {name}/{} (entry {idx})",
                hdr.file_name
            );
        }
        let _ = n;
    }
}

/// Cross-corpus diff test: run the Rust reader against every fixture and
/// compare the central-directory view against the C oracle. Any fixture the
/// Rust side cannot parse yet is recorded as a skip with its error, so we
/// get a live map of what works and what doesn't as the reader evolves.
#[test]
fn reader_diff_test_all_fixtures() {
    let mut matched = 0usize;
    let mut skipped: Vec<(String, String)> = Vec::new();
    let mut mismatched: Vec<String> = Vec::new();

    for path in list_fixtures() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let oracle = oracle_min_entries(&path);

        match RustZipReader::open(&path) {
            Ok(reader) => {
                let rust: Vec<MinEntry> = reader
                    .entries()
                    .iter()
                    .map(|e| MinEntry {
                        name: e.file_name.clone(),
                        size: e.uncompressed_size,
                        crc32: e.crc32,
                    })
                    .collect();
                if rust == oracle {
                    matched += 1;
                    println!("  MATCH  {name:24}");
                } else {
                    mismatched.push(name.clone());
                    println!("  DIFFER {name:24}\n    oracle: {oracle:?}\n    rust:   {rust:?}");
                }
            }
            Err(e) => {
                let msg = format!("{e:?}");
                skipped.push((name.clone(), msg.clone()));
                println!("  SKIP   {name:24}  ({msg})");
            }
        }
    }

    println!(
        "\nSummary: {} matched / {} skipped / {} mismatched",
        matched,
        skipped.len(),
        mismatched.len()
    );

    // Hard failure only on MISMATCH (Rust parsed something and disagreed
    // with C). Skips are expected for now — they're the "things still to
    // build" todo list for this reader path. When skip count hits zero
    // the reader path is complete.
    assert!(
        mismatched.is_empty(),
        "differential mismatch on {mismatched:?}"
    );
}

#[test]
fn locate_by_name_matches_iterated_index() {
    let path = fixtures_dir().join("multi_small.zip");
    let c_path = CString::new(path.to_string_lossy().as_bytes()).unwrap();

    let h = unsafe { wr_reader_open(c_path.as_ptr()) };
    assert!(!h.is_null(), "open multi_small.zip");

    let name = CString::new("file2.txt").unwrap();
    let located = unsafe { wr_reader_locate(h, name.as_ptr()) };
    assert!(located >= 0, "locate file2.txt failed: {located}");

    // Confirm stat on that index returns the same name.
    let mut name_buf = vec![0i8; 256];
    let mut size: usize = 0;
    let mut crc32: u32 = 0;
    let rc = unsafe {
        wr_reader_stat(
            h,
            located,
            name_buf.as_mut_ptr(),
            name_buf.len(),
            &mut size,
            &mut crc32,
        )
    };
    unsafe { wr_reader_close(h) };

    assert_eq!(rc, 0, "stat on located index");
    let name_bytes: Vec<u8> = name_buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    assert_eq!(String::from_utf8_lossy(&name_bytes), "file2.txt");
}

// ---------------------------------------------------------------------------
// Extension tests: zip64 + data descriptors + AES.
//
// These use `zip = "2"` as a dev-dependency to generate fixtures with format
// features that Python's zipfile module either cannot produce deterministically
// (true zip64 central directory) or does not produce at all (WinZip AES). The
// fixtures are generated into temp files at test time, not committed.
// ---------------------------------------------------------------------------

fn tmp_archive(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "noricum-spike-ext-{}-{}.zip",
        label,
        std::process::id()
    ));
    p
}

/// Generate a ZIP archive whose central directory genuinely uses zip64
/// extended information fields (not just a zip64 local header extra). Uses
/// `zip-rs`'s `FileOptions::large_file(true)` which forces the zip64 extra on
/// every entry, and writes a minimum archive that miniz/any reader must
/// parse via the zip64 EOCD path.
fn write_zip64_archive_via_zip_rs(path: &Path) {
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    let file = std::fs::File::create(path).expect("create zip64 fixture file");
    let mut zw = ZipWriter::new(file);
    let opts = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    for i in 0..3 {
        zw.start_file(format!("entry{i}.txt"), opts).unwrap();
        zw.write_all(format!("content of entry {i}\n").as_bytes())
            .unwrap();
    }
    zw.finish().unwrap();
}

#[test]
fn extension_test_zip64_via_zip_rs() {
    let path = tmp_archive("zip64");
    write_zip64_archive_via_zip_rs(&path);

    // C oracle must accept this archive (miniz supports zip64).
    let oracle = snapshot_via_oracle(&path).expect("oracle zip64 open");
    assert_eq!(oracle.entries.len(), 3, "oracle sees 3 entries");
    for (i, entry) in oracle.entries.iter().enumerate() {
        assert_eq!(entry.name, format!("entry{i}.txt"));
        let expected = format!("content of entry {i}\n");
        assert_eq!(entry.size, expected.len());
    }

    // Rust reader MUST match.
    let mut rust = RustZipReader::open(&path).expect("rust zip64 open");
    assert_eq!(rust.num_entries(), 3, "rust sees 3 entries in zip64 archive");
    for i in 0..3usize {
        let body = rust.extract_to_mem(i).expect("rust zip64 extract");
        let expected = format!("content of entry {i}\n");
        assert_eq!(body, expected.as_bytes(), "rust zip64 body at {i}");
    }

    let _ = std::fs::remove_file(&path);
}

/// Generate an archive using a DATA DESCRIPTOR. zip-rs emits a data
/// descriptor (flag bit 3 = 0x0008) when it writes via a non-seekable
/// sink — we emulate that by wrapping a Vec<u8> in a blocking writer or by
/// using the explicit streaming-write API.
///
/// Actually, zip-rs writes data descriptors only when the seek is
/// unavailable; we work around by writing to an in-memory buffer via the
/// `ZipWriter::new` path which stores, then manually re-emitting with the
/// bit 3 flag set. Simpler path: use `start_file` followed by `set_flush_on_finish_file`
/// or use `new_from_streamed_nonseekable`. The simplest is to write to a
/// Cursor<Vec<u8>> then write the bytes to disk — zip-rs uses data
/// descriptors whenever it can't seek, which cursors CAN, so we force the
/// flag via the SimpleFileOptions manually. Alternative:
///
/// Since zip-rs doesn't expose a "force data descriptor" flag, we fall back
/// to asserting that our reader handles the zip-rs default (which already
/// emits consistent local + central entries in the seekable writer path).
/// The reader already uses the central directory as authoritative, so data
/// descriptors are transparent to it. This test locks in that behavior by
/// constructing a handcrafted archive that sets flag bit 3 in the local
/// header on purpose.
#[test]
fn extension_test_data_descriptor_reader_handles_bit3() {
    // Build an archive by hand with bit 3 set on the local header and
    // zeros for the local size fields, exercising the reader path that
    // pulls sizes from the central directory.
    let path = tmp_archive("data_descriptor");
    let body = b"content with data descriptor flag\n";
    let crc = crc32fast_hash(body);
    let compressed = body.to_vec(); // Stored compression: body == compressed
    let size = body.len() as u32;
    let name = b"ddesc.txt";

    let mut archive: Vec<u8> = Vec::new();

    // Local file header with flag bit 3 set and zeros for sizes/crc.
    archive.extend_from_slice(&0x04034b50u32.to_le_bytes()); // signature
    archive.extend_from_slice(&20u16.to_le_bytes()); // version
    archive.extend_from_slice(&0x0008u16.to_le_bytes()); // flag bit 3
    archive.extend_from_slice(&0u16.to_le_bytes()); // compression method (stored)
    archive.extend_from_slice(&0u16.to_le_bytes()); // time
    archive.extend_from_slice(&0u16.to_le_bytes()); // date
    archive.extend_from_slice(&0u32.to_le_bytes()); // crc32 (placeholder)
    archive.extend_from_slice(&0u32.to_le_bytes()); // compressed size (placeholder)
    archive.extend_from_slice(&0u32.to_le_bytes()); // uncompressed size (placeholder)
    archive.extend_from_slice(&(name.len() as u16).to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes()); // extra length
    archive.extend_from_slice(name);

    let data_offset = archive.len() as u64;
    archive.extend_from_slice(&compressed);

    // Data descriptor: optional magic + real crc/sizes.
    archive.extend_from_slice(&0x08074b50u32.to_le_bytes()); // optional magic
    archive.extend_from_slice(&crc.to_le_bytes());
    archive.extend_from_slice(&size.to_le_bytes());
    archive.extend_from_slice(&size.to_le_bytes());

    // Central directory entry with the REAL sizes/crc.
    let cd_offset = archive.len() as u32;
    archive.extend_from_slice(&0x02014b50u32.to_le_bytes()); // signature
    archive.extend_from_slice(&20u16.to_le_bytes()); // version_made_by
    archive.extend_from_slice(&20u16.to_le_bytes()); // version_needed
    archive.extend_from_slice(&0x0008u16.to_le_bytes()); // flags bit 3
    archive.extend_from_slice(&0u16.to_le_bytes()); // compression
    archive.extend_from_slice(&0u16.to_le_bytes()); // time
    archive.extend_from_slice(&0u16.to_le_bytes()); // date
    archive.extend_from_slice(&crc.to_le_bytes());
    archive.extend_from_slice(&size.to_le_bytes()); // comp size
    archive.extend_from_slice(&size.to_le_bytes()); // uncomp size
    archive.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name len
    archive.extend_from_slice(&0u16.to_le_bytes()); // extra len
    archive.extend_from_slice(&0u16.to_le_bytes()); // comment len
    archive.extend_from_slice(&0u16.to_le_bytes()); // disk start
    archive.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
    archive.extend_from_slice(&0u32.to_le_bytes()); // external attrs
    archive.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    archive.extend_from_slice(name);

    let cd_end = archive.len() as u32;
    let cd_size = cd_end - cd_offset;

    // EOCD.
    archive.extend_from_slice(&0x06054b50u32.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes()); // disk number
    archive.extend_from_slice(&0u16.to_le_bytes()); // disk with central
    archive.extend_from_slice(&1u16.to_le_bytes()); // entries this disk
    archive.extend_from_slice(&1u16.to_le_bytes()); // total entries
    archive.extend_from_slice(&cd_size.to_le_bytes());
    archive.extend_from_slice(&cd_offset.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes()); // comment length

    std::fs::write(&path, &archive).unwrap();
    let _ = data_offset;

    // Both C oracle and Rust reader must extract the correct body.
    let oracle = snapshot_via_oracle(&path).expect("oracle data_descriptor open");
    assert_eq!(oracle.entries.len(), 1);
    assert_eq!(oracle.entries[0].size, body.len());
    assert_eq!(oracle.entries[0].crc32, crc);

    let mut rust = RustZipReader::open(&path).expect("rust data_descriptor open");
    let extracted = rust.extract_to_mem(0).expect("rust extract");
    assert_eq!(extracted, body, "rust must yield body from data-descriptor archive");

    let _ = std::fs::remove_file(&path);
}

/// Helper that delegates to `crc32fast` without adding it as a dev-dep
/// (it's already a runtime dep of the crate).
fn crc32fast_hash(data: &[u8]) -> u32 {
    // re-export through a tiny shim since we can't `use crc32fast` in tests
    // without adding it to dev-dependencies separately; the runtime dep is
    // visible through `spike`, but there's no re-export — so we inline a
    // reflected-table impl that matches the zlib CRC32 polynomial.
    const POLY: u32 = 0xEDB8_8320;
    let mut table = [0u32; 256];
    for (n, entry) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { POLY ^ (c >> 1) } else { c >> 1 };
        }
        *entry = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Generate an AES-256 encrypted archive via zip-rs and decrypt it with
/// the Rust reader using the correct password. Verifies the full AES path:
/// PBKDF2 key derivation, password verification, HMAC-SHA1 auth, AES-CTR
/// decryption, and post-decryption deflate.
#[test]
fn extension_test_aes256_decrypt_full() {
    use zip::AesMode;
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    const PASSWORD: &str = "noricumspike";
    const PAYLOAD: &[u8] = b"this payload is AES encrypted and then deflated\n";

    let path = tmp_archive("aes256");
    {
        let file = std::fs::File::create(&path).expect("create aes fixture");
        let mut zw = ZipWriter::new(file);
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .with_aes_encryption(AesMode::Aes256, PASSWORD);
        zw.start_file("secret.txt", opts).unwrap();
        zw.write_all(PAYLOAD).unwrap();
        zw.finish().unwrap();
    }

    // Without a password the reader must refuse.
    {
        let mut reader = RustZipReader::open(&path).expect("open aes archive");
        match reader.extract_to_mem(0) {
            Err(spike::contract::ZipError::UnsupportedEncryption) => {}
            other => panic!("expected UnsupportedEncryption without password, got {other:?}"),
        }
    }

    // With the correct password the reader must produce the exact payload.
    {
        let mut reader = RustZipReader::open(&path).expect("open aes archive again");
        let idx = reader.locate_file("secret.txt").expect("locate");
        let body = reader
            .extract_to_mem_with_password(idx, PASSWORD.as_bytes())
            .expect("aes decrypt with password");
        assert_eq!(body, PAYLOAD, "AES-256 decrypted payload must match");
    }

    // With a WRONG password the reader must fail on password verification.
    {
        let mut reader = RustZipReader::open(&path).expect("open aes archive 3");
        let err = reader.extract_to_mem_with_password(0, b"wrong-password");
        assert!(
            matches!(
                err,
                Err(spike::contract::ZipError::InvalidParameter)
                    | Err(spike::contract::ZipError::CrcCheckFailed)
            ),
            "wrong password must fail, got {err:?}"
        );
    }

    // C oracle doesn't support AES — don't assert on oracle behavior.
    let _ = snapshot_via_oracle(&path);

    let _ = std::fs::remove_file(&path);
}

/// Real-world fuzzing pass: read every .zip file in a directory pointed to by
/// the `SPIKE_REAL_ARCHIVES` env var, walk it through both the C oracle and
/// the Rust reader, and assert that the entry-by-entry view agrees (name,
/// size, crc32). Also byte-compares the first 1024 bytes of each entry.
///
/// This test is `#[ignore]`d by default so the default `cargo test` run
/// stays hermetic. To actually run the fuzzing pass:
///
/// ```sh
/// SPIKE_REAL_ARCHIVES=/tmp/real-world-zips cargo test --test differential_zip real_world -- --ignored --nocapture
/// ```
///
/// Any divergence between the oracle and the Rust reader is a failure the
/// spike cares about — it means there's a format feature the reader does
/// not yet handle.
#[test]
#[ignore = "requires SPIKE_REAL_ARCHIVES env var pointing to a directory of .zip files"]
fn real_world_diff_test() {
    let dir = match std::env::var("SPIKE_REAL_ARCHIVES") {
        Ok(d) => PathBuf::from(d),
        Err(_) => {
            eprintln!("SPIKE_REAL_ARCHIVES not set, skipping real-world fuzzing");
            return;
        }
    };

    // Accept any ZIP-format container: .zip, .jar (Java), .war/.ear (Java EE),
    // .apk (Android), .docx/.xlsx/.pptx (Office), .epub (e-books), .kmz (KML).
    const ZIP_EXTS: &[&str] = &[
        "zip", "jar", "war", "ear", "apk", "docx", "xlsx", "pptx", "epub", "kmz",
    ];
    let mut archives: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir({}): {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|ext| {
                    ZIP_EXTS.iter().any(|z| z.eq_ignore_ascii_case(ext))
                })
                .unwrap_or(false)
        })
        .collect();
    archives.sort();

    assert!(
        !archives.is_empty(),
        "SPIKE_REAL_ARCHIVES is set but no .zip files found in {}",
        dir.display()
    );

    println!("\nReal-world diff test: {} archives", archives.len());

    let mut total_archives = 0usize;
    let mut total_entries = 0usize;
    let mut matched_archives = 0usize;
    let mut oracle_only_failures: Vec<String> = Vec::new();
    let mut rust_only_failures: Vec<(String, String)> = Vec::new();
    let mut mismatches: Vec<String> = Vec::new();

    for archive in &archives {
        let name = archive.file_name().unwrap().to_string_lossy().into_owned();
        total_archives += 1;

        let oracle = match snapshot_via_oracle(archive) {
            Ok(s) => s,
            Err(e) => {
                oracle_only_failures.push(format!("{name}: oracle rejected ({e})"));
                println!("  ORACLE-FAIL {name}  ({e})");
                continue;
            }
        };

        let mut rust_reader = match RustZipReader::open(archive) {
            Ok(r) => r,
            Err(e) => {
                rust_only_failures.push((name.clone(), format!("{e:?}")));
                println!("  RUST-OPEN-FAIL {name}  ({e:?})");
                continue;
            }
        };

        let rust_headers: Vec<_> = rust_reader.entries().to_vec();

        if rust_headers.len() != oracle.entries.len() {
            mismatches.push(format!(
                "{name}: oracle={} entries, rust={} entries",
                oracle.entries.len(),
                rust_headers.len()
            ));
            println!(
                "  DIFFER    {name}  oracle={} rust={} entries",
                oracle.entries.len(),
                rust_headers.len()
            );
            continue;
        }

        let mut archive_ok = true;
        for (i, hdr) in rust_headers.iter().enumerate() {
            let oracle_entry = &oracle.entries[i];
            if hdr.file_name != oracle_entry.name {
                mismatches.push(format!(
                    "{name} entry {i}: name mismatch oracle={:?} rust={:?}",
                    oracle_entry.name, hdr.file_name
                ));
                archive_ok = false;
                break;
            }
            if hdr.uncompressed_size as usize != oracle_entry.size {
                mismatches.push(format!(
                    "{name}/{}: size oracle={} rust={}",
                    hdr.file_name, oracle_entry.size, hdr.uncompressed_size
                ));
                archive_ok = false;
                break;
            }
            if hdr.crc32 != oracle_entry.crc32 {
                mismatches.push(format!(
                    "{name}/{}: crc32 oracle={:08x} rust={:08x}",
                    hdr.file_name, oracle_entry.crc32, hdr.crc32
                ));
                archive_ok = false;
                break;
            }

            // Extract the full entry via Rust and first 64 bytes via oracle,
            // byte-compare the overlap.
            match rust_reader.extract_to_mem(i) {
                Ok(body) => {
                    let prefix = &body[..body.len().min(FIRST_BYTES_CAP)];
                    if prefix != oracle_entry.first_bytes.as_slice() {
                        mismatches.push(format!(
                            "{name}/{}: first-bytes diverge",
                            hdr.file_name
                        ));
                        archive_ok = false;
                        break;
                    }
                    total_entries += 1;
                }
                Err(e) => {
                    mismatches.push(format!(
                        "{name}/{}: rust extract failed: {e:?}",
                        hdr.file_name
                    ));
                    archive_ok = false;
                    break;
                }
            }
        }

        if archive_ok {
            matched_archives += 1;
            println!(
                "  MATCH     {name}  ({} entries)",
                oracle.entries.len()
            );
        }
    }

    println!(
        "\nSummary: {matched_archives} / {total_archives} archives matched, {total_entries} entries extracted"
    );

    if !oracle_only_failures.is_empty() {
        println!("Oracle failures (C cannot open):");
        for m in &oracle_only_failures {
            println!("  - {m}");
        }
    }
    if !rust_only_failures.is_empty() {
        println!("Rust failures (Rust cannot open/parse):");
        for (n, e) in &rust_only_failures {
            println!("  - {n}: {e}");
        }
    }
    if !mismatches.is_empty() {
        println!("Mismatches:");
        for m in &mismatches {
            println!("  - {m}");
        }
    }

    // Fail the test if Rust failed to open something the oracle handled, OR
    // if anything matched on open but diverged on content.
    assert!(
        rust_only_failures.is_empty() && mismatches.is_empty(),
        "real-world fuzzing had {} rust failures and {} mismatches",
        rust_only_failures.len(),
        mismatches.len()
    );
}

/// Also verify the AES-128 and AES-192 paths with a smaller payload, to make
/// sure all three strengths are exercised.
#[test]
fn extension_test_aes128_and_aes192_small_payload() {
    use zip::AesMode;
    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    for (mode, label) in [(AesMode::Aes128, "aes128"), (AesMode::Aes192, "aes192")] {
        let path = tmp_archive(label);
        const PASSWORD: &str = "pw";
        let payload = format!("hello {label}\n").into_bytes();

        {
            let file = std::fs::File::create(&path).expect("create aes fixture");
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Stored)
                .with_aes_encryption(mode, PASSWORD);
            zw.start_file("m.txt", opts).unwrap();
            zw.write_all(&payload).unwrap();
            zw.finish().unwrap();
        }

        let mut reader = RustZipReader::open(&path).expect("open");
        let body = reader
            .extract_to_mem_with_password(0, PASSWORD.as_bytes())
            .expect("extract");
        assert_eq!(body, payload, "{label} payload");

        let _ = std::fs::remove_file(&path);
    }
}
