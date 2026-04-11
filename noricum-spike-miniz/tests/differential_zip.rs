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

use spike::contract::ZipReader as RustZipReader;
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
