//! Differential test harness for the lua spike.
//!
//! The C wrapper is the oracle: `wr_open` reads a fixture, creates a
//! fresh `lua_State`, opens stdlibs, and runs the chunk. Tests here are
//! the Phase-0 green light — they must pass before Hour 0 of the spike.
//!
//! See docs/methodology/phase-0-oracle-harness.md for the pattern.

use std::ffi::{CStr, CString};
use std::path::{Path, PathBuf};

use spike::{wr_close, wr_open, wr_tostring, SpikeHandle};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("lua_corpus")
}

/// Read the string currently sitting at the given stack index out of
/// the oracle and return it as an owned `String`. Panics if the value
/// is not a string — fine for tests, which want the loud failure.
fn tos_string(h: SpikeHandle, idx: i32) -> String {
    let mut buf = [0u8; 256];
    let len = unsafe {
        wr_tostring(
            h,
            idx,
            buf.as_mut_ptr() as *mut i8,
            buf.len(),
        )
    };
    assert!(
        len >= 0,
        "wr_tostring returned {len}; value at idx {idx} is not a string"
    );
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const i8) };
    cstr.to_string_lossy().into_owned()
}

/// Phase 0 green-light test. Must pass before Hour 0 of the spike.
#[test]
fn oracle_selftest() {
    let fixture = fixtures_dir().join("hello.dat");
    assert!(
        fixture.exists(),
        "sanity fixture missing; run `python3 fixtures/gen_fixtures.py`"
    );

    let c_path = CString::new(fixture.to_string_lossy().as_bytes())
        .expect("fixture path contains a NUL byte");

    // Drive the oracle: newstate + openlibs + loadstring + pcall are
    // all bundled inside wr_open.
    let h = unsafe { wr_open(c_path.as_ptr()) };
    assert!(!h.is_null(), "wr_open returned NULL on the sanity fixture");

    // `return "hello, world"` leaves the string on top of the stack.
    let top = tos_string(h, -1);
    assert_eq!(top, "hello, world", "unexpected oracle return value");

    unsafe { wr_close(h) };
}
