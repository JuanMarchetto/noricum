All marketing materials updated. Here's a summary of changes:

### Blog post
- `13 C files` → `14 C files`, `complex (520 LOC)` → `complex (1,686 LOC)`
- `~13,200 LOC` → `~16,000 LOC`

### README.md
- Tests badge: `340 passing` → `321 passing` (actual passing count, not annotation count)
- LOC badge: `~13,200` → `~16,000`
- Comparison table: `13+ files` → `14 files`

### Social posts (all platforms)
- File counts: `13/13` → `14/14` everywhere
- Biggest migration: cJSON (520 LOC) → expr_eval.c (1,686 LOC) everywhere
- LOC: `~13K` / `~13,200` → `~16K` / `~16,000` everywhere
- Test count: `309` → `321`

### Anthropic Build application
- `13/13` → `14/14`, range `13-520 LOC` → `13-1,686 LOC`
- Largest migration updated to expr_eval.c (1,686 LOC)
- `309 tests` → `321 tests`
- `~13,200 LOC` → `~16,000 LOC`

### CHANGELOG
- Added 8 missing items to `[Unreleased]`: interface-aware CRUST-Bench, error severity classification, `--ollama-model` flag, `compare` subcommand, CI release workflow, fuzz targets, 5th RAG pattern, behavioral review infra
