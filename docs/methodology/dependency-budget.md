# Dependency Budget Modes

A critical decision at the start of any interactive spike: **how many external dependencies is the Rust port allowed to have?** Different targets have different answers, and the wrong default produces the wrong output.

Set this as part of the initial spike instructions, alongside the 8-hour timebox and success criteria. The choice drives binary size, reproducibility, supply chain surface, and what counts as "done".

## The three modes

### Mode 1 — FREE (default for research spikes)

> "Delegate well-solved algorithms to best-in-class crates. Binary size and dep count are not primary concerns. Optimize for Rust code clarity and time-to-working."

**When to use:** research spikes, demos, reference implementations, learning material, proof-of-concept for a blog post, anything where the output is not expected to ship as a binary replacement for the C original.

**What this looks like:** deflate/inflate → `flate2`, CRC32 → `crc32fast`, AES → `aes` + `hmac` + `pbkdf2` + `sha1` + `constant_time_eq`, SHA → `sha2`, regex → `regex`, JSON → `serde_json`, etc.

**Binary size expectation:** 3-5x the C original for small utilities. LOC expectation: 0.15-0.25x the C source.

**Reference:** the miniz_zip.c interactive spike — 7 runtime deps, 1157 LOC Rust, 457 KB release binary (vs 72 KB for C -Os equivalent).

**Pros:** fastest to write, cleanest code, smallest source, best ergonomics.
**Cons:** 3-5x binary size, transitive supply chain, not a drop-in replacement for embedded or WASM targets.

### Mode 2 — MATCHING (dep count equivalent to C original)

> "Match the dependency profile of the C original. If the C lib has 0 external deps, the Rust port must too. If the C lib depends on zlib, the Rust port can depend on one compression crate. Count cardinality, not functionality."

**When to use:** projects where the target library is specifically chosen for its lean dep profile (embedded SDKs, FFI libraries that ship as artifacts, CLI tools with "zero deps" as a marketing feature).

**What this looks like:** if the C lib was self-contained (like miniz), the Rust port vendors miniz_oxide (or equivalent) as source code, not as a dep. Same for crypto primitives. LOC goes up, crate count goes to zero or near-zero.

**Binary size expectation:** 2-3x the C original (Rust still pays libstd overhead but the crate bloat is gone). LOC expectation: 0.3-0.5x the C source.

**Pros:** reproducibility parity with C, same supply chain surface, drop-in replacement for the downstream consumer, binary size closer to C baseline.
**Cons:** more code to write (the vendored portions), more code to test, more ongoing maintenance.

### Mode 3 — ZERO (no external runtime deps at all)

> "The Rust port has zero external runtime dependencies. Every algorithm is implemented in-tree or vendored as source. The only `[dependencies]` section in `Cargo.toml` is empty."

**When to use:** embedded / no-std targets, WASM with size constraints, defense or regulated environments where supply chain audits matter, any case where the C original was specifically written to have no external deps.

**What this looks like:** everything in Mode 2, plus more. If the target has AES, you write AES (or vendor a pure-Rust single-file AES impl). If it has deflate, you vendor miniz-oxide's source into `src/deflate/`. `cargo tree` shows only the crate itself.

**Binary size expectation:** 2x the C original in release-min mode (Rust libstd is still there but nothing else). LOC expectation: 0.4-0.7x the C source.

**Pros:** maximum reproducibility, minimum supply chain, defensibly similar to the C original for audit purposes.
**Cons:** significant extra work, ongoing burden to maintain the vendored primitives, duplication of well-tested community crates.

## The initial-instructions template

When opening a spike with Claude Code, state the dependency budget in the first prompt. Example:

> "Interactive spike to migrate miniz_zip.c to Rust. Target: 8-hour budget, byte-match with C oracle on 14 fixtures. **Dependency budget: MATCHING.** The C original is self-contained (deflate, CRC32, no external libs). The Rust port should achieve the same profile: vendor `miniz_oxide` source for deflate/inflate instead of depending on `flate2`; hand-write CRC32 inline; if AES is needed, vendor a single-file AES implementation. Do not pull in the RustCrypto crate family. Do not pull in `flate2` or any wrapping crate. `cargo tree` should show one crate (the spike itself)."

The director (Claude Code) uses this budget to reject delegation shortcuts and plan accordingly.

## How to convert between modes

An existing Mode-1 spike can be converted to Mode-2 or Mode-3 by:

1. **Vendor the deflate implementation.** Copy `miniz_oxide/src/` into `src/deflate/` with the upstream license headers. Update `flate2::read::DeflateDecoder` call sites to the vendored path.
2. **Hand-write or vendor CRC32.** 50 LOC reflected-table impl.
3. **Vendor AES primitives.** `aes/src/` or a single-file impl (~700 LOC for AES-128/192/256 + key schedule).
4. **Hand-write HMAC-SHA1 + PBKDF2.** ~150 LOC combined.
5. **Delete runtime deps from `Cargo.toml`.** Verify `cargo tree` shows only the crate.
6. **Re-run all tests.** Everything should still pass if the vendored primitives are correct.
7. **Measure binary size.** Expect 30-50% reduction vs Mode-1.

Expected effort: 4-6 hours of focused work if the Mode-1 spike already has clean module boundaries. Longer if the integration is messy.

## Binary size targets

For the miniz_zip spike with equivalent functionality (stored + deflated reader path, no AES):

| Mode | Rust binary | Source LOC | Ratio vs C-Os |
|---|---:|---:|---:|
| FREE | 457 KB | 1157 | 6.36x |
| FREE + `release-min` | 353 KB | 1157 | 4.92x |
| MATCHING (projected) | ~200 KB | ~2500 | 2.8x |
| ZERO (projected) | ~150 KB | ~3500 | 2.1x |
| C -Os baseline | 72 KB | 8016 | 1.00x |

The "2x floor" is libstd + panic unwinding + format! infrastructure. Going below 2x requires `no_std` + `alloc`, which is a separate methodology.

## Historical note

The miniz_zip.c spike (branch `feat/interactive-spike`) was run in FREE mode, with 7 runtime deps. That was the right default for a research spike where the output was a blog post and a reference implementation. **If the goal had been "drop-in replacement for miniz_zip", MATCHING would have been the right mode** — and the output would have been ~2500 LOC Rust with 0 deps, taking ~10-12 hours instead of 66 minutes.

Both are valid. The wrong answer is picking the mode by default without thinking about what the spike is for.
