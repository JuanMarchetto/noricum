# best_outputs

Curated archive of the best known-good Rust outputs from Noricum's C-to-Rust migration pipeline.

**What this is.** A reference collection of Rust source files, one per successful migration target, along with per-migration `README.md` files that record the C source path, size ratio, idiomatic score, unsafe count, and test status at the time of archiving. Treat it as a showcase / provenance record.

**Who it is for.** Humans reviewing pipeline quality, demo material, and anyone who wants to compare a freshly-produced migration against a trusted baseline.

**What it is NOT.** This directory is *not* a test target. It is not wired into `cargo test`, it is not a Cargo workspace member, and nothing here is built. The canonical test fixtures still live under `tests/fixtures/` and the canonical pipeline outputs still live under `output/` and `.noricum-artifacts/`. Files in `best_outputs/` are copies, not symlinks, so editing them has no effect on anything that runs.

## Migrations

| Migration | C LOC | Rust LOC | Score | Unsafe | Tests | Notes |
|---|---:|---:|---:|---:|---|---|
| [hash_table](./hash_table/) | 204 | 144 | 91 | 0 | golden diff pass | open-addressing hash table, DeepSeek run |
| [miniz_test](./miniz_test/) | 153 | 154 | 84 | 0 | golden diff pass | Adler-32 + CRC-32 checksum routines |
| [cjson_combined](./cjson_combined/) | 520 | 354 | 100 | 0 | 1 basic + 12 extended pass | hand-combined cJSON subset |
| [expr_eval](./expr_eval/) | 1686 | 1446 | 100 | 0 | golden pass (diff byte-exact at migration) | parser + 25 builtins + variable store |
| [cjson_full](./cjson_full/) | 1696 | 1439 | 100 | 0 | 97 C + 96 Rust, diff byte-exact | flagship: full DaveGamble/cJSON |
| [genann](./genann/) | 642 | 623* | 47 | 0 | 521,556 diff assertions byte-exact | tiny feed-forward neural net |
| [olive](./olive/) | 1443 | 1176 | high | 0 | 23 fns, pixel-buffer checksums byte-exact | tsoding/olive.c 2D graphics |
| [http_parser](./http_parser/) | 3680 | 1492 | high | 0 | 37 conformance tests pass | nodejs/http-parser, manually completed |

\* genann's MEMORY.md-recorded migration size was 721 LOC; the archived file (shared with `tests/fixtures/genann/genann_migrated.rs`) is currently 623 LOC.

## Re-running

To re-run any of these migrations end-to-end, point `noricum-cli` at the matching C fixture, e.g.:

```
cargo run -p noricum-cli -- migrate tests/fixtures/medium/hash_table.c
cargo run -p noricum-cli -- migrate tests/fixtures/cjson/cjson_combined.c
cargo run -p noricum-cli -- migrate tests/fixtures/large/expr_eval.c
```

See each migration's own `README.md` for the exact C source path.
