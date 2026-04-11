# Architectural Elimination

A pattern for C-to-Rust migrations: **the right Rust architecture can ELIMINATE features from the C original rather than port them**. Validated in the miniz_zip.c spike where 4 separate features collapsed to zero lines of Rust by choosing different data-flow shapes.

## The audit

During Phase 0 / type contract design, BEFORE writing any translation, list every complexity source in the C code and classify each one:

| Category | Example | Action |
|---|---|---|
| **Data-flow driven** | compression algorithm, CRC computation, cryptographic primitives | Must port or delegate to an existing crate |
| **Architecture-driven** | branches that exist because the C chose a particular data-flow shape | Candidate for elimination — choose a different Rust architecture and the feature disappears |
| **Legacy baggage** | 5 wrapper variants of the same API, 32-bit fallback paths, deprecated aliases | Don't port unless a consumer actually needs it |

The key question for each feature: *"does this exist because the C data flow requires it, or because a different data flow would make it irrelevant?"* Features in the second category are free wins — you do NOT port them.

## Four validated eliminations from miniz_zip

### 1. Data descriptors (flag bit 3 / sizes-after-data)

**In C:** ~60 LOC of state-machine branching. When flag bit 3 (`0x0008`) is set, the local file header has zeros for `crc32`, `compressed_size`, `uncompressed_size`. The real values live in a "data descriptor" block after the compressed data. The C reader has code to parse this descriptor, skip it during position calculation, and fall back to the central directory when offsets are ambiguous.

**In Rust:** ZERO new code. The Rust reader reads ALL per-entry metadata from the central directory, never from the local file header. The central directory has the authoritative values regardless of flag bit 3. When `find_data_start(local_header_offset)` computes the data offset, it uses `local_header + 30 + file_name_length + extra_field_length`, which is valid whether bit 3 is set or not. The feature became architectural.

**Verification:** handcrafted a ZIP archive byte-by-byte with flag bit 3 set and zero placeholders in the local header. The Rust reader extracted it correctly on first run, without any data-descriptor-aware code.

### 2. `Box<dyn Read + Write + Seek>` trait-object indirection

**In C:** ~200 LOC of function-pointer callbacks for source abstraction. `mz_zip_archive` has `m_pRead`, `m_pWrite`, `m_pNeeds_keepalive`, `m_pIO_opaque` pointers that let the archive source be a file, memory buffer, or caller-supplied callback. The C code has branches for each variant throughout the reader and writer.

**The pipeline's Run 14 tried to port this to:**

```rust
Box<dyn Read + Write + Seek>
```

This is **invalid Rust** — trait objects can have at most one non-auto trait. It never compiled. This exact error killed Run 14.

**In Rust (interactive spike):**

```rust
pub enum ZipSource {
    File(std::fs::File),
    Mem(std::io::Cursor<Vec<u8>>),
}

impl Read for ZipSource { /* delegate to inner */ }
impl Write for ZipSource { /* delegate to inner */ }
impl Seek for ZipSource { /* delegate to inner */ }
```

Total: 40 LOC of enum + three trait impls. The 200 LOC of C indirection became 40 LOC of Rust by choosing a concrete sum type instead of dynamic dispatch. The failure mode became impossible by construction — this pattern cannot produce the error that killed Run 14.

### 3. 32-bit fallback branches (zip64 transparency)

**In C:** throughout the code, branches like:

```c
if (size <= UINT32_MAX) {
    write_32bit_value(size);
} else {
    enable_zip64();
    write_zip64_extra();
    write_32bit_sentinel();
}
```

There are dozens of these. The C code maintains two parallel code paths for "fits in u32" vs "needs zip64" for sizes, offsets, counts.

**In Rust:** use `u64` for every size/offset/count from the start. The central directory parser reads u32 fields, but immediately widens to u64. The zip64 extra field is consulted when the u32 field is `u32::MAX` (sentinel), but that's one function call, not a parallel code path. ~200 LOC of branching collapsed.

### 4. Five writer init variants → one constructor

**In C:** `mz_zip_writer_init`, `mz_zip_writer_init_v2`, `mz_zip_writer_init_heap`, `mz_zip_writer_init_heap_v2`, `mz_zip_writer_init_file`, `mz_zip_writer_init_file_v2`, `mz_zip_writer_init_cfile`, `mz_zip_writer_init_from_reader`, `mz_zip_writer_init_from_reader_v2`. Nine variants of "start writing a zip". Each dispatches to different internal setup code.

**In Rust:** one `ZipWriter::create(path)` that accepts any `P: AsRef<Path>`. The variations the C API supported (heap vs file vs cfile vs in-place append) are either (a) not needed for 95% of callers, (b) expressible as "construct a `ZipSource` in the mode you want, then pass it in", or (c) edge cases that can be handled by dedicated constructors when someone actually needs them. ~300 LOC of wrapper plumbing collapsed to a single public function.

## Recognition rules

A C feature is a likely elimination candidate if:

- It exists for **performance micro-optimization on constrained systems** (embedded, <64KB stack, no malloc). Modern Rust targets don't care.
- It exists for **legacy file format backward compatibility** (pre-zip64 world, pre-unicode world). If your migration's target environment is modern, you can drop the legacy path.
- It's an **alternate data path for the same logical operation** (reader variants, writer variants). Rust can collapse those via generics or `impl Trait` parameters.
- It uses **function pointers or opaque callbacks to abstract over source/sink**. Rust has trait objects and enums — pick one at design time, not per-call.
- It has a **branch predicated on a mode flag** (`if archive->mode == WRITING then... else...`). Rust can split this into two distinct types where misuse is a compile error.

## Counter-examples (when NOT to eliminate)

- **Compression method field**: miniz_zip.c has branches for `Stored`, `Deflated`, `bzip2`, `lzma`, `AES-extension`. All of these must be preserved in the Rust port because the caller genuinely needs to know which compression the entry used. This is data-flow driven.
- **CRC32 verification**: the C code computes CRC32 on every extract. The Rust code must do the same (via `crc32fast` crate or hand-rolled). Data integrity check, not an architectural artifact.
- **Zip64 extra field parsing**: the presence of the zip64 extra with u64 sizes is a data fact, not an architecture choice. The Rust port needs a parser for it — but it can be one function, not a parallel code path.

## Document eliminations in commit messages

When you eliminate a C feature, call it out in the commit message:

> Added `ZipSource` enum. Replaces the `Box<dyn Read + Write + Seek>` trait object indirection pattern. **ELIMINATED ~200 LOC of callback plumbing via concrete sum type + delegation.** The pipeline's Run 14 died on this exact pattern because trait objects cannot carry more than one non-auto trait.

This preserves the audit trail for readers who wonder why the Rust port is smaller than the C original. It's not that Rust is magic — it's that several features disappeared by choosing a different data-flow shape.
