#!/usr/bin/env bash
# spike-scaffold — generate a Phase 0 oracle harness for a new interactive spike.
#
# Usage:
#   tools/spike/scaffold.sh <spike-name> <c-lib-dir> [wrapper-func-list.txt]
#
# Example:
#   tools/spike/scaffold.sh libpng /home/marche/src/libpng-1.6 funcs.txt
#
# Produces noricum-spike-<name>/ at the repo root with:
#   - Cargo.toml (with [workspace] to break inheritance)
#   - build.rs (cc-rs compiling the C sources)
#   - src/lib.rs (extern "C" block stubs for the wrapper)
#   - wrapper.h / wrapper.c (templates with TODOs)
#   - wrapper_smoke.c (template)
#   - fixtures/gen_fixtures.py (template)
#   - tests/differential_<name>.rs (template)
#   - SPIKE.md (Phase 0 checklist)
#
# You still have to fill in:
#   - wrapper.c: wire up the C library's actual functions
#   - fixtures/gen_fixtures.py: generate a deterministic fixture corpus
#   - tests/differential_<name>.rs: the snapshot struct for your format
#
# But you DO NOT have to write the Cargo.toml, build.rs, or the base
# structure from scratch. That alone saves ~30 minutes per spike.

set -euo pipefail

NAME="${1:-}"
C_DIR="${2:-}"
FUNC_LIST="${3:-}"

if [ -z "$NAME" ] || [ -z "$C_DIR" ]; then
    echo "Usage: $0 <spike-name> <c-lib-dir> [wrapper-func-list.txt]" >&2
    exit 1
fi

if [ ! -d "$C_DIR" ]; then
    echo "ERROR: C library directory not found: $C_DIR" >&2
    exit 1
fi

REPO_ROOT="$(git rev-parse --show-toplevel)"
SPIKE_DIR="$REPO_ROOT/noricum-spike-$NAME"

if [ -e "$SPIKE_DIR" ]; then
    echo "ERROR: $SPIKE_DIR already exists. Remove it first or use a different name." >&2
    exit 1
fi

mkdir -p "$SPIKE_DIR/src" "$SPIKE_DIR/src/bin" "$SPIKE_DIR/fixtures" "$SPIKE_DIR/tests"

# Copy all .c and .h files from the source tree (top-level only; if the
# target has subdirs, adjust manually).
copied=0
for f in "$C_DIR"/*.c "$C_DIR"/*.h; do
    [ -e "$f" ] || continue
    cp "$f" "$SPIKE_DIR/"
    copied=$((copied + 1))
done
echo "Copied $copied .c/.h files from $C_DIR"

# Find the .c files for the cc-rs file list (excluding wrapper.c which we
# will create, and *_test.c which are usually test harnesses).
CC_FILES=""
for f in "$SPIKE_DIR"/*.c; do
    base=$(basename "$f")
    case "$base" in
        wrapper.c|wrapper_smoke.c|*_test.c) continue ;;
    esac
    CC_FILES+="        .file(\"$base\")\n"
done

# Cargo.toml — break workspace inheritance, add cc build dep.
cat > "$SPIKE_DIR/Cargo.toml" <<EOF
[workspace]

[package]
name = "noricum-spike-$NAME"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
name = "spike"
path = "src/lib.rs"

[build-dependencies]
cc = "1"

[dependencies]
# Runtime deps go here. Default is Mode-1 FREE (delegate to crates).
# For Mode-2 MATCHING or Mode-3 ZERO, keep this empty and vendor primitives.

[dev-dependencies]
# Optional: use an idiomatic Rust crate for fixture generation at test time.
# See docs/methodology/architectural-seeds.md for the recommended crate
# for your target's library category.

[profile.release-min]
inherits = "release"
lto = true
codegen-units = 1
panic = "abort"
strip = true
opt-level = "z"
EOF

# build.rs
{
    echo "fn main() {"
    echo "    // Cargo rerun triggers — add every .c and .h we depend on."
    for f in "$SPIKE_DIR"/*.c "$SPIKE_DIR"/*.h; do
        [ -e "$f" ] || continue
        base=$(basename "$f")
        echo "    println!(\"cargo:rerun-if-changed=$base\");"
    done
    echo "    println!(\"cargo:rerun-if-changed=wrapper.c\");"
    echo "    println!(\"cargo:rerun-if-changed=wrapper.h\");"
    echo ""
    echo "    cc::Build::new()"
    echo -ne "$CC_FILES"
    echo "        .file(\"wrapper.c\")"
    echo "        .include(\".\")"
    echo "        .flag_if_supported(\"-Wno-unused-parameter\")"
    echo "        .flag_if_supported(\"-Wno-unused-function\")"
    echo "        .compile(\"${NAME}_wrapper\");"
    echo "}"
} > "$SPIKE_DIR/build.rs"

# wrapper.h template
cat > "$SPIKE_DIR/wrapper.h" <<EOF
#ifndef NORICUM_SPIKE_${NAME^^}_WRAPPER_H
#define NORICUM_SPIKE_${NAME^^}_WRAPPER_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Opaque handle. Caller never peeks inside. Fill in the functions below
 * to match the target library's public API. See
 * docs/methodology/phase-0-oracle-harness.md for the naming convention.
 */
typedef void* spike_handle_t;

/* TODO: replace these stubs with actual wrapper functions for $NAME. */
spike_handle_t wr_open(const char* filename);
int            wr_close(spike_handle_t h);

#ifdef __cplusplus
}
#endif

#endif
EOF

# wrapper.c template
cat > "$SPIKE_DIR/wrapper.c" <<EOF
#include "wrapper.h"
/* TODO: include the target library's public header */
/* #include "${NAME}.h" */
#include <stdlib.h>

/*
 * TODO: allocate an internal struct on the heap, call the target
 * library's init function, return the opaque pointer. See
 * docs/methodology/phase-0-oracle-harness.md for the pattern.
 */
spike_handle_t wr_open(const char* filename) {
    (void)filename;
    /* TODO */
    return NULL;
}

int wr_close(spike_handle_t h) {
    if (!h) return -1;
    /* TODO: call target library's end/close, free the wrapper struct */
    free(h);
    return 0;
}
EOF

# wrapper_smoke.c template
cat > "$SPIKE_DIR/wrapper_smoke.c" <<EOF
/*
 * Pure-C smoke test. Compile and run BEFORE writing any Rust:
 *   gcc wrapper_smoke.c wrapper.c <target>.c -o wrapper_smoke -I.
 *   ./wrapper_smoke
 * Must exit 0. If this fails, wrapper.c has a bug and Rust is not
 * the problem.
 */
#include "wrapper.h"
#include <stdio.h>

int main(void) {
    /* TODO: open a known-good sanity fixture, verify content, exit 0 */
    spike_handle_t h = wr_open("fixtures/sanity_corpus/hello.dat");
    if (!h) {
        fprintf(stderr, "FAIL: wr_open returned NULL\n");
        return 1;
    }
    wr_close(h);
    printf("smoke: OK\n");
    return 0;
}
EOF

# src/lib.rs
cat > "$SPIKE_DIR/src/lib.rs" <<EOF
//! Noricum interactive spike: $NAME migration via Claude Code as director.
//!
//! lib.rs is the FFI layer. extern "C" declarations for the wrapper.
//! See docs/methodology/phase-0-oracle-harness.md for the pattern.

use std::os::raw::{c_char, c_int, c_void};

pub type SpikeHandle = *mut c_void;

#[link(name = "${NAME}_wrapper", kind = "static")]
unsafe extern "C" {
    pub fn wr_open(filename: *const c_char) -> SpikeHandle;
    pub fn wr_close(h: SpikeHandle) -> c_int;
    // TODO: add the rest of the wrapper functions
}

// TODO: add pub mod contract; once the type contract is written.
// TODO: add pub mod reader; / pub mod writer; as functions land.
EOF

# fixtures/gen_fixtures.py template
cat > "$SPIKE_DIR/fixtures/gen_fixtures.py" <<EOF
#!/usr/bin/env python3
"""Generate deterministic fixtures for the $NAME differential test."""
import os
# TODO: import the Python library that produces your target's format
# (zipfile, tarfile, struct, etc.)

FIXTURE_DIR = os.path.dirname(os.path.abspath(__file__))
CORPUS_DIR = os.path.join(FIXTURE_DIR, "${NAME}_corpus")
os.makedirs(CORPUS_DIR, exist_ok=True)

def write_sanity_fixture():
    """The simplest fixture that exercises the library's basic read path."""
    path = os.path.join(CORPUS_DIR, "hello.dat")
    # TODO: write the sanity fixture for this format
    with open(path, "wb") as f:
        f.write(b"hello\\n")
    return path

def main():
    print(f"Writing fixtures to {CORPUS_DIR}")
    write_sanity_fixture()
    print("TODO: add more fixtures covering edge cases")

if __name__ == "__main__":
    main()
EOF

# tests/differential_<name>.rs template
cat > "$SPIKE_DIR/tests/differential_${NAME}.rs" <<EOF
//! Differential test harness for the $NAME spike.
//!
//! See docs/methodology/phase-0-oracle-harness.md for the pattern.

use std::ffi::CString;
use std::path::{Path, PathBuf};

use spike::{wr_close, wr_open};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("${NAME}_corpus")
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
EOF

# SPIKE.md checklist
cat > "$SPIKE_DIR/SPIKE.md" <<EOF
# $NAME Interactive Spike

Scaffolded by \`tools/spike/scaffold.sh\` on $(date +%Y-%m-%d).

## Phase 0 checklist (pre-clock)

- [ ] Fill in \`wrapper.h\` with the actual API functions you need (~5-10 fns).
- [ ] Fill in \`wrapper.c\` with thin delegations to the C library.
- [ ] Fill in \`src/lib.rs\` extern block to match \`wrapper.h\`.
- [ ] Write \`fixtures/gen_fixtures.py\` with a deterministic corpus.
- [ ] Run \`python3 fixtures/gen_fixtures.py\` and commit the output.
- [ ] Run \`gcc wrapper_smoke.c wrapper.c <sources>.c -o wrapper_smoke -I.\` and verify exit 0.
- [ ] Run \`cargo test --test differential_$NAME -- oracle_selftest\` and verify green.

When all boxes are checked, **Hour 0 of the spike starts.** See
docs/methodology/interactive-spike-mode.md for the flow.

## Dependency budget (pick one at start)

- [ ] **Mode 1 — FREE** — delegate well-solved algorithms to crates
      (flate2, aes, etc.). Default for research spikes.
- [ ] **Mode 2 — MATCHING** — match the C original's dep count (usually 0).
      Vendor primitives as source. Default for drop-in replacements.
- [ ] **Mode 3 — ZERO** — no external runtime deps. Everything vendored.
      Default for embedded / no-std / defense targets.

See docs/methodology/dependency-budget.md.

## Architectural seed

Check docs/methodology/architectural-seeds.md for a reference Rust crate
whose type architecture you should steal. Read it for 30 minutes before
writing \`src/contract.rs\`. Do NOT write the type contract from the C
code alone.

## Timebox

Phase 0: ~2-3 hours (this checklist). Does NOT count against the spike clock.

Hour 0 — Hour 8: the spike itself. Hard exit at Hour 8 regardless of
state. See docs/methodology/interactive-spike-mode.md.

## Commit cadence

One commit per function that passes its differential test. No batching.
The commit log becomes the blog post narrative.
EOF

echo ""
echo "Scaffold complete: $SPIKE_DIR"
echo ""
echo "Next steps:"
echo "  1. cd $SPIKE_DIR"
echo "  2. Edit wrapper.h, wrapper.c, fixtures/gen_fixtures.py"
echo "  3. Walk the SPIKE.md checklist"
echo "  4. Run cargo test -- oracle_selftest to verify Phase 0 is green"
