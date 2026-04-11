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
