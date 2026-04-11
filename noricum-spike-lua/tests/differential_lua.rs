//! Differential test harness for the lua spike.
//!
//! See docs/methodology/phase-0-oracle-harness.md for the pattern.

use std::ffi::CString;
use std::path::{Path, PathBuf};

use spike::{wr_close, wr_open};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("lua_corpus")
}

/// Phase 0 green-light test. This must pass before Hour 0 of the spike.
#[test]
fn oracle_selftest() {
    let fixture = fixtures_dir().join("hello.dat");
    assert!(fixture.exists(), "run fixtures/gen_fixtures.py first");

    let c_path = CString::new(fixture.to_string_lossy().as_bytes()).unwrap();
    let h = unsafe { wr_open(c_path.as_ptr()) };
    assert!(!h.is_null(), "wr_open returned NULL on sanity fixture");

    let rc = unsafe { wr_close(h) };
    assert_eq!(rc, 0, "wr_close failed");

    // TODO: once the wrapper has more functions, iterate every fixture,
    // extract every entry, snapshot (name, size, crc32, first_bytes),
    // assert on well-formedness.
}
