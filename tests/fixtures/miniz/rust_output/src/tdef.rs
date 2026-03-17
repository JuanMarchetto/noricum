//! Low-level DEFLATE compression implementation.
//! This is a direct translation of the miniz tdefl (Tiny DEFLATE) compressor.

#![allow(dead_code)]

// ---- Constants ----

const TDEFL_MAX_HUFF_TABLES: usize = 3;
const TDEFL_MAX_HUFF_SYMBOLS_0: usize = 288;
const TDEFL_MAX_HUFF_SYMBOLS_1: usize = 32;
const TDEFL_MAX_HUFF_SYMBOLS_2: usize = 19;
const TDEFL_MAX_HUFF_SYMBOLS: usize = 288;
const TDEFL_MAX_SUPPORTED_HUFF_CODESIZE: usize = 32;
const TDEFL_LZ_DICT_SIZE: usize = 32768;
const TDEFL_LZ_DICT_SIZE_MASK: usize = TDEFL_LZ_DICT_SIZE - 1;
const TDEFL_MIN_MATCH_LEN: usize = 3;
const TDEFL_MAX_MATCH_LEN: usize = 258;
const TDEFL_LZ_CODE_BUF_SIZE: usize = 64 * 1024;
const TDEFL_OUT_BUF_SIZE: usize = (TDEFL_LZ_CODE_BUF_SIZE * 13) / 10;
const TDEFL_LZ_HASH_BITS: usize = 15;
const TDEFL_LZ_HASH_SIZE: usize = 1 << TDEFL_LZ_HASH_BITS;
const TDEFL_LZ_HASH_SHIFT: usize = (TDEFL_LZ_HASH_BITS + 2) / 3;
const TDEFL_LEVEL1_HASH_SIZE_MASK: usize = 4095;

// Compression flags
const TDEFL_WRITE_ZLIB_HEADER: u32 = 0x01000;
const TDEFL_COMPUTE_ADLER32: u32 = 0x02000;
const TDEFL_GREEDY_PARSING_FLAG: u32 = 0x04000;
const TDEFL_NONDETERMINISTIC_PARSING_FLAG: u32 = 0x08000;
const TDEFL_RLE_MATCHES: u32 = 0x10000;
const TDEFL_FILTER_MATCHES: u32 = 0x20000;
const TDEFL_FORCE_ALL_STATIC_BLOCKS: u32 = 0x40000;
const TDEFL_FORCE_ALL_RAW_BLOCKS: u32 = 0x80000;
const TDEFL_MAX_PROBES_MASK: u32 = 0xFFF;

// Strategy constants (from miniz.h)
const MZ_DEFAULT_STRATEGY: i32 = 0;
const MZ_FILTERED: i32 = 1;
const MZ_HUFFMAN_ONLY: i32 = 2;
const MZ_RLE: i32 = 3;
const MZ_FIXED: i32 = 4;
const MZ_DEFAULT_LEVEL: i32 = 6;

// ---- Enums ----

/// Flush modes for the compressor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TdeflFlush {
    NoFlush = 0,
    SyncFlush = 2,
    FullFlush = 3,
    Finish = 4,
}

/// Status codes returned by compression functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TdeflStatus {
    BadParam = -2,
    PutBufFailed = -1,
    Okay = 0,
    Done = 1,
}

// ---- Lookup tables ----

static S_TDEFL_LEN_SYM: [u16; 256] = [
    257, 258, 259, 260, 261, 262, 263, 264, 265, 265, 266, 266, 267, 267, 268, 268,
    269, 269, 269, 269, 270, 270, 270, 270, 271, 271, 271, 271, 272, 272, 272, 272,
    273, 273, 273, 273, 273, 273, 273, 273, 274, 274, 274, 274, 274, 274, 274, 274,
    275, 275, 275, 275, 275, 275, 275, 275, 276, 276, 276, 276, 276, 276, 276, 276,
    277, 277, 277, 277, 277, 277, 277, 277, 277, 277, 277, 277, 277, 277, 277, 277,
    278, 278, 278, 278, 278, 278, 278, 278, 278, 278, 278, 278, 278, 278, 278, 278,
    279, 279, 279, 279, 279, 279, 279, 279, 279, 279, 279, 279, 279, 279, 279, 279,
    280, 280, 280, 280, 280, 280, 280, 280, 280, 280, 280, 280, 280, 280, 280, 280,
    281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281,
    281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281, 281,
    282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282,
    282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282, 282,
    283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283,
    283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283, 283,
    284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284,
    284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 284, 285,
];

static S_TDEFL_LEN_EXTRA: [u8; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1,
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 0,
];

static S_TDEFL_SMALL_DIST_SYM: [u8; 512] = [
    0, 1, 2, 3, 4, 4, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7,
    8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 9,
    10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
    11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
    17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
];

static S_TDEFL_SMALL_DIST_EXTRA: [u8; 512] = [
    0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
];

static S_TDEFL_LARGE_DIST_SYM: [u8; 128] = [
    0, 0, 18, 19, 20, 20, 21, 21, 22, 22, 22, 22, 23, 23, 23, 23,
    24, 24, 24, 24, 24, 24, 24, 24, 25, 25, 25, 25, 25, 25, 25, 25,
    26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
    27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27,
    28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28,
    29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
    29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29, 29,
];

static S_TDEFL_LARGE_DIST_EXTRA: [u8; 128] = [
    0, 0, 8, 8, 9, 9, 9, 9, 10, 10, 10, 10, 10, 10, 10, 10,
    11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11, 11,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
];

static S_TDEFL_NUM_PROBES: [u32; 11] = [0, 1, 6, 32, 16, 32, 128, 256, 512, 768, 1500];

static MZ_BITMASKS: [u32; 17] = [
    0x0000, 0x0001, 0x0003, 0x0007, 0x000F, 0x001F, 0x003F, 0x007F,
    0x00FF, 0x01FF, 0x03FF, 0x07FF, 0x0FFF, 0x1FFF, 0x3FFF, 0x7FFF, 0xFFFF,
];

static S_TDEFL_PACKED_CODE_SIZE_SYMS_SWIZZLE: [u8; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

// ---- Helper types ----

#[derive(Clone, Copy, Default)]
struct SymFreq {
    key: u16,
    sym_index: u16,
}

// ---- Output callback type ----

/// Callback type for streaming compressed output.
/// Returns true on success, false on failure.
pub type PutBufFunc = fn(buf: &[u8], user: &mut Vec<u8>) -> bool;

// ---- Main compressor struct ----

/// The DEFLATE compressor state.
pub struct TdeflCompressor {
    // Callback-based output (optional)
    put_buf_func: Option<PutBufFunc>,
    put_buf_user: Vec<u8>,

    flags: u32,
    max_probes: [u32; 2],
    greedy_parsing: bool,

    adler32: u32,
    lookahead_pos: u32,
    lookahead_size: u32,
    dict_size: u32,

    // LZ code buffer: index-based instead of pointer-based
    lz_code_buf: Vec<u8>,
    lz_code_buf_cursor: usize,  // replaces m_pLZ_code_buf
    lz_flags_cursor: usize,     // replaces m_pLZ_flags

    // Output buffer: index-based
    output_buf: Vec<u8>,
    output_cursor: usize,       // replaces m_pOutput_buf
    output_end: usize,          // replaces m_pOutput_buf_end

    num_flags_left: u32,
    total_lz_bytes: u32,
    lz_code_buf_dict_pos: u32,
    bits_in: u32,
    bit_buffer: u32,

    saved_match_dist: u32,
    saved_match_len: u32,
    saved_lit: u32,
    output_flush_ofs: u32,
    output_flush_remaining: u32,
    finished: bool,
    block_index: u32,
    wants_to_finish: bool,

    prev_return_status: TdeflStatus,

    // Input tracking
    src_data: Vec<u8>,       // copy of input data for current call
    src_pos: usize,          // current read position in src_data
    src_buf_left: usize,
    out_buf_ofs: usize,
    flush: TdeflFlush,

    // Dictionary and hash tables
    dict: Vec<u8>,                                              // TDEFL_LZ_DICT_SIZE + TDEFL_MAX_MATCH_LEN - 1
    huff_count: [[u16; TDEFL_MAX_HUFF_SYMBOLS]; TDEFL_MAX_HUFF_TABLES],
    huff_codes: [[u16; TDEFL_MAX_HUFF_SYMBOLS]; TDEFL_MAX_HUFF_TABLES],
    huff_code_sizes: [[u8; TDEFL_MAX_HUFF_SYMBOLS]; TDEFL_MAX_HUFF_TABLES],
    next: Vec<u16>,                                             // TDEFL_LZ_DICT_SIZE
    hash: Vec<u16>,                                             // TDEFL_LZ_HASH_SIZE
}

impl TdeflCompressor {
    /// Create a new compressor with given flags.
    pub fn new(flags: u32) -> Self {
        let mut c = Self {
            put_buf_func: None,
            put_buf_user: Vec::new(),
            flags,
            max_probes: [0; 2],
            greedy_parsing: false,
            adler32: 1,
            lookahead_pos: 0,
            lookahead_size: 0,
            dict_size: 0,
            lz_code_buf: vec![0u8; TDEFL_LZ_CODE_BUF_SIZE],
            lz_code_buf_cursor: 1,
            lz_flags_cursor: 0,
            output_buf: vec![0u8; TDEFL_OUT_BUF_SIZE],
            output_cursor: 0,
            output_end: 0,
            num_flags_left: 8,
            total_lz_bytes: 0,
            lz_code_buf_dict_pos: 0,
            bits_in: 0,
            bit_buffer: 0,
            saved_match_dist: 0,
            saved_match_len: 0,
            saved_lit: 0,
            output_flush_ofs: 0,
            output_flush_remaining: 0,
            finished: false,
            block_index: 0,
            wants_to_finish: false,
            prev_return_status: TdeflStatus::Okay,
            src_data: Vec::new(),
            src_pos: 0,
            src_buf_left: 0,
            out_buf_ofs: 0,
            flush: TdeflFlush::NoFlush,
            dict: vec![0u8; TDEFL_LZ_DICT_SIZE + TDEFL_MAX_MATCH_LEN - 1],
            huff_count: [[0u16; TDEFL_MAX_HUFF_SYMBOLS]; TDEFL_MAX_HUFF_TABLES],
            huff_codes: [[0u16; TDEFL_MAX_HUFF_SYMBOLS]; TDEFL_MAX_HUFF_TABLES],
            huff_code_sizes: [[0u8; TDEFL_MAX_HUFF_SYMBOLS]; TDEFL_MAX_HUFF_TABLES],
            next: vec![0u16; TDEFL_LZ_DICT_SIZE],
            hash: vec![0u16; TDEFL_LZ_HASH_SIZE],
        };
        c.init_flags(flags);
        c
    }

    /// Re-initialize with new flags (equivalent to tdefl_init).
    pub fn init_flags(&mut self, flags: u32) {
        self.flags = flags;
        self.max_probes[0] = 1 + ((flags & 0xFFF) + 2) / 3;
        self.greedy_parsing = (flags & TDEFL_GREEDY_PARSING_FLAG) != 0;
        self.max_probes[1] = 1 + (((flags & 0xFFF) >> 2) + 2) / 3;

        if (flags & TDEFL_NONDETERMINISTIC_PARSING_FLAG) == 0 {
            for h in self.hash.iter_mut() {
                *h = 0;
            }
        }

        self.lookahead_pos = 0;
        self.lookahead_size = 0;
        self.dict_size = 0;
        self.total_lz_bytes = 0;
        self.lz_code_buf_dict_pos = 0;
        self.bits_in = 0;
        self.output_flush_ofs = 0;
        self.output_flush_remaining = 0;
        self.finished = false;
        self.block_index = 0;
        self.bit_buffer = 0;
        self.wants_to_finish = false;

        self.lz_code_buf_cursor = 1;
        self.lz_flags_cursor = 0;
        self.lz_code_buf[0] = 0;
        self.num_flags_left = 8;
        self.output_cursor = 0;
        self.output_end = 0;
        self.prev_return_status = TdeflStatus::Okay;
        self.saved_match_dist = 0;
        self.saved_match_len = 0;
        self.saved_lit = 0;
        self.adler32 = 1;
        self.src_data.clear();
        self.src_pos = 0;
        self.src_buf_left = 0;
        self.out_buf_ofs = 0;
        self.flush = TdeflFlush::NoFlush;

        if (flags & TDEFL_NONDETERMINISTIC_PARSING_FLAG) == 0 {
            for b in self.dict.iter_mut() {
                *b = 0;
            }
        }

        for i in 0..TDEFL_MAX_HUFF_SYMBOLS_0 {
            self.huff_count[0][i] = 0;
        }
        for i in 0..TDEFL_MAX_HUFF_SYMBOLS_1 {
            self.huff_count[1][i] = 0;
        }
    }

    // ---- Bitstream writer ----

    /// Write bits to the output bitstream (equivalent to TDEFL_PUT_BITS macro).
    fn put_bits(&mut self, bits: u32, len: u32) {
        self.bit_buffer |= bits << self.bits_in;
        self.bits_in += len;
        while self.bits_in >= 8 {
            if self.output_cursor < self.output_end {
                self.output_buf[self.output_cursor] = (self.bit_buffer & 0xFF) as u8;
                self.output_cursor += 1;
            }
            self.bit_buffer >>= 8;
            self.bits_in -= 8;
        }
    }

    // ---- Huffman table optimization ----

    /// Optimize a Huffman table. If `static_table` is true, use existing code sizes.
    fn optimize_huffman_table(&mut self, table_num: usize, table_len: usize, code_size_limit: usize, static_table: bool) {
        let mut num_codes = [0i32; 1 + TDEFL_MAX_SUPPORTED_HUFF_CODESIZE];

        if static_table {
            for i in 0..table_len {
                num_codes[self.huff_code_sizes[table_num][i] as usize] += 1;
            }
        } else {
            let mut syms0 = [SymFreq::default(); TDEFL_MAX_HUFF_SYMBOLS];
            let mut syms1 = [SymFreq::default(); TDEFL_MAX_HUFF_SYMBOLS];
            let mut num_used_syms = 0usize;

            for i in 0..table_len {
                if self.huff_count[table_num][i] != 0 {
                    syms0[num_used_syms].key = self.huff_count[table_num][i];
                    syms0[num_used_syms].sym_index = i as u16;
                    num_used_syms += 1;
                }
            }

            let sorted = radix_sort_syms(num_used_syms, &mut syms0, &mut syms1);
            calculate_minimum_redundancy(sorted);

            for i in 0..num_used_syms {
                num_codes[sorted[i].key as usize] += 1;
            }

            huffman_enforce_max_code_size(&mut num_codes, num_used_syms as i32, code_size_limit as i32);

            for i in 0..TDEFL_MAX_HUFF_SYMBOLS {
                self.huff_code_sizes[table_num][i] = 0;
            }
            for i in 0..TDEFL_MAX_HUFF_SYMBOLS {
                self.huff_codes[table_num][i] = 0;
            }

            let mut j = num_used_syms;
            for i in 1..=code_size_limit {
                let mut l = num_codes[i];
                while l > 0 {
                    j -= 1;
                    self.huff_code_sizes[table_num][sorted[j].sym_index as usize] = i as u8;
                    l -= 1;
                }
            }
        }

        let mut next_code = [0u32; TDEFL_MAX_SUPPORTED_HUFF_CODESIZE + 1];
        next_code[1] = 0;
        {
            let mut j: u32 = 0;
            for i in 2..=code_size_limit {
                j = (j + num_codes[i - 1] as u32) << 1;
                next_code[i] = j;
            }
        }

        for i in 0..table_len {
            let code_size = self.huff_code_sizes[table_num][i] as usize;
            if code_size == 0 {
                continue;
            }
            let mut code = next_code[code_size];
            next_code[code_size] += 1;
            let mut rev_code: u32 = 0;
            for _ in 0..code_size {
                rev_code = (rev_code << 1) | (code & 1);
                code >>= 1;
            }
            self.huff_codes[table_num][i] = rev_code as u16;
        }
    }

    // ---- RLE helpers for dynamic block header ----

    fn rle_prev_code_size(
        huff_count_2: &mut [u16; TDEFL_MAX_HUFF_SYMBOLS],
        packed_code_sizes: &mut [u8],
        num_packed: &mut usize,
        rle_repeat_count: &mut u32,
        prev_code_size: u8,
    ) {
        if *rle_repeat_count > 0 {
            if *rle_repeat_count < 3 {
                huff_count_2[prev_code_size as usize] += *rle_repeat_count as u16;
                for _ in 0..*rle_repeat_count {
                    packed_code_sizes[*num_packed] = prev_code_size;
                    *num_packed += 1;
                }
            } else {
                huff_count_2[16] += 1;
                packed_code_sizes[*num_packed] = 16;
                *num_packed += 1;
                packed_code_sizes[*num_packed] = (*rle_repeat_count - 3) as u8;
                *num_packed += 1;
            }
            *rle_repeat_count = 0;
        }
    }

    fn rle_zero_code_size(
        huff_count_2: &mut [u16; TDEFL_MAX_HUFF_SYMBOLS],
        packed_code_sizes: &mut [u8],
        num_packed: &mut usize,
        rle_z_count: &mut u32,
    ) {
        if *rle_z_count > 0 {
            if *rle_z_count < 3 {
                huff_count_2[0] += *rle_z_count as u16;
                for _ in 0..*rle_z_count {
                    packed_code_sizes[*num_packed] = 0;
                    *num_packed += 1;
                }
            } else if *rle_z_count <= 10 {
                huff_count_2[17] += 1;
                packed_code_sizes[*num_packed] = 17;
                *num_packed += 1;
                packed_code_sizes[*num_packed] = (*rle_z_count - 3) as u8;
                *num_packed += 1;
            } else {
                huff_count_2[18] += 1;
                packed_code_sizes[*num_packed] = 18;
                *num_packed += 1;
                packed_code_sizes[*num_packed] = (*rle_z_count - 11) as u8;
                *num_packed += 1;
            }
            *rle_z_count = 0;
        }
    }

    // ---- Block start functions ----

    /// Emit a dynamic Huffman block header.
    fn start_dynamic_block(&mut self) {
        self.huff_count[0][256] = 1;

        self.optimize_huffman_table(0, TDEFL_MAX_HUFF_SYMBOLS_0, 15, false);
        self.optimize_huffman_table(1, TDEFL_MAX_HUFF_SYMBOLS_1, 15, false);

        let mut num_lit_codes = 286i32;
        while num_lit_codes > 257 {
            if self.huff_code_sizes[0][(num_lit_codes - 1) as usize] != 0 {
                break;
            }
            num_lit_codes -= 1;
        }

        let mut num_dist_codes = 30i32;
        while num_dist_codes > 1 {
            if self.huff_code_sizes[1][(num_dist_codes - 1) as usize] != 0 {
                break;
            }
            num_dist_codes -= 1;
        }

        let mut code_sizes_to_pack = [0u8; TDEFL_MAX_HUFF_SYMBOLS_0 + TDEFL_MAX_HUFF_SYMBOLS_1];
        for i in 0..num_lit_codes as usize {
            code_sizes_to_pack[i] = self.huff_code_sizes[0][i];
        }
        for i in 0..num_dist_codes as usize {
            code_sizes_to_pack[num_lit_codes as usize + i] = self.huff_code_sizes[1][i];
        }
        let total_code_sizes_to_pack = (num_lit_codes + num_dist_codes) as usize;

        let mut packed_code_sizes = [0u8; TDEFL_MAX_HUFF_SYMBOLS_0 + TDEFL_MAX_HUFF_SYMBOLS_1];
        let mut num_packed_code_sizes: usize = 0;
        let mut rle_z_count: u32 = 0;
        let mut rle_repeat_count: u32 = 0;
        let mut prev_code_size: u8 = 0xFF;

        for i in 0..TDEFL_MAX_HUFF_SYMBOLS_2 {
            self.huff_count[2][i] = 0;
        }

        for i in 0..total_code_sizes_to_pack {
            let code_size = code_sizes_to_pack[i];
            if code_size == 0 {
                Self::rle_prev_code_size(
                    &mut self.huff_count[2],
                    &mut packed_code_sizes,
                    &mut num_packed_code_sizes,
                    &mut rle_repeat_count,
                    prev_code_size,
                );
                rle_z_count += 1;
                if rle_z_count == 138 {
                    Self::rle_zero_code_size(
                        &mut self.huff_count[2],
                        &mut packed_code_sizes,
                        &mut num_packed_code_sizes,
                        &mut rle_z_count,
                    );
                }
            } else {
                Self::rle_zero_code_size(
                    &mut self.huff_count[2],
                    &mut packed_code_sizes,
                    &mut num_packed_code_sizes,
                    &mut rle_z_count,
                );
                if code_size != prev_code_size {
                    Self::rle_prev_code_size(
                        &mut self.huff_count[2],
                        &mut packed_code_sizes,
                        &mut num_packed_code_sizes,
                        &mut rle_repeat_count,
                        prev_code_size,
                    );
                    self.huff_count[2][code_size as usize] += 1;
                    packed_code_sizes[num_packed_code_sizes] = code_size;
                    num_packed_code_sizes += 1;
                } else {
                    rle_repeat_count += 1;
                    if rle_repeat_count == 6 {
                        Self::rle_prev_code_size(
                            &mut self.huff_count[2],
                            &mut packed_code_sizes,
                            &mut num_packed_code_sizes,
                            &mut rle_repeat_count,
                            prev_code_size,
                        );
                    }
                }
            }
            prev_code_size = code_size;
        }

        if rle_repeat_count > 0 {
            Self::rle_prev_code_size(
                &mut self.huff_count[2],
                &mut packed_code_sizes,
                &mut num_packed_code_sizes,
                &mut rle_repeat_count,
                prev_code_size,
            );
        } else {
            Self::rle_zero_code_size(
                &mut self.huff_count[2],
                &mut packed_code_sizes,
                &mut num_packed_code_sizes,
                &mut rle_z_count,
            );
        }

        self.optimize_huffman_table(2, TDEFL_MAX_HUFF_SYMBOLS_2, 7, false);

        self.put_bits(2, 2);
        self.put_bits((num_lit_codes - 257) as u32, 5);
        self.put_bits((num_dist_codes - 1) as u32, 5);

        let mut num_bit_lengths: i32 = 18;
        while num_bit_lengths >= 0 {
            if self.huff_code_sizes[2][S_TDEFL_PACKED_CODE_SIZE_SYMS_SWIZZLE[num_bit_lengths as usize] as usize] != 0 {
                break;
            }
            num_bit_lengths -= 1;
        }
        num_bit_lengths = 4i32.max(num_bit_lengths + 1);
        self.put_bits((num_bit_lengths - 4) as u32, 4);

        for i in 0..num_bit_lengths as usize {
            self.put_bits(
                self.huff_code_sizes[2][S_TDEFL_PACKED_CODE_SIZE_SYMS_SWIZZLE[i] as usize] as u32,
                3,
            );
        }

        let mut packed_code_sizes_index: usize = 0;
        while packed_code_sizes_index < num_packed_code_sizes {
            let code = packed_code_sizes[packed_code_sizes_index] as usize;
            packed_code_sizes_index += 1;
            let hc = self.huff_codes[2][code] as u32;
            let hs = self.huff_code_sizes[2][code] as u32;
            self.put_bits(hc, hs);
            if code >= 16 {
                // "\02\03\07"[code - 16] => extra bits: 2, 3, 7 for codes 16, 17, 18
                let extra_bits: u32 = match code {
                    16 => 2,
                    17 => 3,
                    18 => 7,
                    _ => 0,
                };
                self.put_bits(
                    packed_code_sizes[packed_code_sizes_index] as u32,
                    extra_bits,
                );
                packed_code_sizes_index += 1;
            }
        }
    }

    /// Emit a static Huffman block header.
    fn start_static_block(&mut self) {
        for i in 0..=143 {
            self.huff_code_sizes[0][i] = 8;
        }
        for i in 144..=255 {
            self.huff_code_sizes[0][i] = 9;
        }
        for i in 256..=279 {
            self.huff_code_sizes[0][i] = 7;
        }
        for i in 280..=287 {
            self.huff_code_sizes[0][i] = 8;
        }

        for i in 0..32 {
            self.huff_code_sizes[1][i] = 5;
        }

        self.optimize_huffman_table(0, 288, 15, true);
        self.optimize_huffman_table(1, 32, 15, true);

        self.put_bits(1, 2);
    }

    // ---- LZ code compression ----

    /// Compress the LZ codes using the current Huffman tables.
    fn compress_lz_codes(&mut self) -> bool {
        let mut flags: u32 = 1;
        let mut lz_pos: usize = 0;
        let lz_end = self.lz_code_buf_cursor;

        // Use the lz_code_buf data from index 0 up to lz_code_buf_cursor
        // The flags byte is at index 0, data starts at 1
        // But the format is: first byte of each group of 8 is a flags byte.
        // Replicate the pointer-based logic using indices into lz_code_buf.

        let lz_buf: Vec<u8> = self.lz_code_buf[..lz_end].to_vec();

        while lz_pos < lz_buf.len() {
            if flags == 1 {
                if lz_pos >= lz_buf.len() {
                    break;
                }
                flags = lz_buf[lz_pos] as u32 | 0x100;
                lz_pos += 1;
            }

            if (flags & 1) != 0 {
                // Match
                if lz_pos + 2 >= lz_buf.len() {
                    break;
                }
                let match_len = lz_buf[lz_pos] as usize;
                let match_dist = lz_buf[lz_pos + 1] as usize | ((lz_buf[lz_pos + 2] as usize) << 8);
                lz_pos += 3;

                let len_sym = S_TDEFL_LEN_SYM[match_len] as usize;
                self.put_bits(
                    self.huff_codes[0][len_sym] as u32,
                    self.huff_code_sizes[0][len_sym] as u32,
                );
                let len_extra = S_TDEFL_LEN_EXTRA[match_len] as u32;
                self.put_bits(
                    match_len as u32 & MZ_BITMASKS[len_extra as usize],
                    len_extra,
                );

                let (sym, num_extra_bits) = if match_dist < 512 {
                    (
                        S_TDEFL_SMALL_DIST_SYM[match_dist] as usize,
                        S_TDEFL_SMALL_DIST_EXTRA[match_dist] as u32,
                    )
                } else {
                    (
                        S_TDEFL_LARGE_DIST_SYM[(match_dist >> 8) & 127] as usize,
                        S_TDEFL_LARGE_DIST_EXTRA[(match_dist >> 8) & 127] as u32,
                    )
                };

                self.put_bits(
                    self.huff_codes[1][sym] as u32,
                    self.huff_code_sizes[1][sym] as u32,
                );
                self.put_bits(
                    match_dist as u32 & MZ_BITMASKS[num_extra_bits as usize],
                    num_extra_bits,
                );
            } else {
                // Literal
                if lz_pos >= lz_buf.len() {
                    break;
                }
                let lit = lz_buf[lz_pos] as usize;
                lz_pos += 1;
                self.put_bits(
                    self.huff_codes[0][lit] as u32,
                    self.huff_code_sizes[0][lit] as u32,
                );
            }
            flags >>= 1;
        }

        // End of block symbol
        self.put_bits(
            self.huff_codes[0][256] as u32,
            self.huff_code_sizes[0][256] as u32,
        );

        self.output_cursor < self.output_end
    }

    /// Compress a block (static or dynamic).
    fn compress_block(&mut self, static_block: bool) -> bool {
        if static_block {
            self.start_static_block();
        } else {
            self.start_dynamic_block();
        }
        self.compress_lz_codes()
    }

    // ---- Match recording ----

    /// Record a literal byte in the LZ code buffer.
    fn record_literal(&mut self, lit: u8) {
        self.total_lz_bytes += 1;
        self.lz_code_buf[self.lz_code_buf_cursor] = lit;
        self.lz_code_buf_cursor += 1;
        self.lz_code_buf[self.lz_flags_cursor] = self.lz_code_buf[self.lz_flags_cursor] >> 1;
        self.num_flags_left -= 1;
        if self.num_flags_left == 0 {
            self.num_flags_left = 8;
            self.lz_flags_cursor = self.lz_code_buf_cursor;
            self.lz_code_buf_cursor += 1;
        }
        self.huff_count[0][lit as usize] += 1;
    }

    /// Record a match (length, distance) in the LZ code buffer.
    fn record_match(&mut self, match_len: u32, mut match_dist: u32) {
        self.total_lz_bytes += match_len;

        self.lz_code_buf[self.lz_code_buf_cursor] = (match_len - TDEFL_MIN_MATCH_LEN as u32) as u8;

        match_dist -= 1;
        self.lz_code_buf[self.lz_code_buf_cursor + 1] = (match_dist & 0xFF) as u8;
        self.lz_code_buf[self.lz_code_buf_cursor + 2] = (match_dist >> 8) as u8;
        self.lz_code_buf_cursor += 3;

        self.lz_code_buf[self.lz_flags_cursor] = (self.lz_code_buf[self.lz_flags_cursor] >> 1) | 0x80;
        self.num_flags_left -= 1;
        if self.num_flags_left == 0 {
            self.num_flags_left = 8;
            self.lz_flags_cursor = self.lz_code_buf_cursor;
            self.lz_code_buf_cursor += 1;
        }

        let s0 = S_TDEFL_SMALL_DIST_SYM[(match_dist & 511) as usize] as usize;
        let s1 = S_TDEFL_LARGE_DIST_SYM[((match_dist >> 8) & 127) as usize] as usize;
        self.huff_count[1][if match_dist < 512 { s0 } else { s1 }] += 1;
        self.huff_count[0][S_TDEFL_LEN_SYM[(match_len - TDEFL_MIN_MATCH_LEN as u32) as usize] as usize] += 1;
    }

    // ---- Match finder ----

    /// Find the best match at the current position.
    fn find_match(
        &self,
        lookahead_pos: u32,
        max_dist: u32,
        max_match_len: u32,
        match_dist: &mut u32,
        match_len: &mut u32,
    ) {
        let pos = (lookahead_pos as usize) & TDEFL_LZ_DICT_SIZE_MASK;
        let mut cur_match_len = *match_len;
        let mut probe_pos = pos;
        let mut num_probes_left = self.max_probes[if cur_match_len >= 32 { 1 } else { 0 }];
        let mut c0 = self.dict[pos + cur_match_len as usize];
        let mut c1 = if cur_match_len > 0 {
            self.dict[pos + cur_match_len as usize - 1]
        } else {
            self.dict[pos]
        };

        if max_match_len <= cur_match_len {
            return;
        }

        loop {
            // Probe loop
            loop {
                num_probes_left -= 1;
                if num_probes_left == 0 {
                    return;
                }

                let next_probe_pos = self.next[probe_pos] as u32;
                if next_probe_pos == 0 {
                    return;
                }
                let dist = lookahead_pos.wrapping_sub(next_probe_pos) & 0xFFFF;
                if dist > max_dist {
                    return;
                }
                probe_pos = (next_probe_pos as usize) & TDEFL_LZ_DICT_SIZE_MASK;
                if self.dict[probe_pos + cur_match_len as usize] == c0
                    && self.dict[probe_pos + cur_match_len as usize - if cur_match_len > 0 { 1 } else { 0 }] == c1
                {
                    *match_dist = dist;
                    break;
                }
            }

            if *match_dist == 0 {
                break;
            }

            let s = pos;
            let q_start = probe_pos;
            let mut probe_len: u32 = 0;
            while probe_len < max_match_len {
                if self.dict[s + probe_len as usize] != self.dict[q_start + probe_len as usize] {
                    break;
                }
                probe_len += 1;
            }

            if probe_len > cur_match_len {
                *match_dist = lookahead_pos.wrapping_sub(self.next[probe_pos & TDEFL_LZ_DICT_SIZE_MASK] as u32) & 0xFFFF;
                // Recalculate dist properly from the probe
                // Actually we need the dist we already computed above
                // The match_dist was already set in the inner loop. Just update match_len.
                cur_match_len = probe_len;
                *match_len = cur_match_len;
                if cur_match_len == max_match_len {
                    return;
                }
                c0 = self.dict[pos + cur_match_len as usize];
                c1 = self.dict[pos + cur_match_len as usize - 1];
            }
        }
    }

    // ---- Flush block ----

    /// Flush the current block to the output buffer.
    /// Returns 0 on success with no pending output, positive if there's pending output,
    /// negative on error (mapped to i32 for C compatibility).
    fn flush_block(&mut self, flush: i32) -> i32 {
        let use_raw_block = ((self.flags & TDEFL_FORCE_ALL_RAW_BLOCKS) != 0)
            && (self.lookahead_pos.wrapping_sub(self.lz_code_buf_dict_pos) <= self.dict_size);

        // Decide output target: use a separate region of output_buf
        let use_internal_buf = self.put_buf_func.is_some() || (self.out_buf_ofs + TDEFL_OUT_BUF_SIZE > self.output_buf.len());
        let output_buf_start = 0usize;

        self.output_cursor = output_buf_start;
        self.output_end = output_buf_start + TDEFL_OUT_BUF_SIZE - 16;

        self.output_flush_ofs = 0;
        self.output_flush_remaining = 0;

        // Finalize LZ flags
        if self.num_flags_left < 8 {
            self.lz_code_buf[self.lz_flags_cursor] = self.lz_code_buf[self.lz_flags_cursor] >> self.num_flags_left;
        }
        if self.num_flags_left == 8 {
            self.lz_code_buf_cursor -= 1;
        }

        // Write zlib header if needed
        if (self.flags & TDEFL_WRITE_ZLIB_HEADER) != 0 && self.block_index == 0 {
            let cmf: u32 = 0x78;
            let mut flevel: u32 = 3;

            // Determine compression level
            let mut found_i = S_TDEFL_NUM_PROBES.len();
            for (i, &probe) in S_TDEFL_NUM_PROBES.iter().enumerate() {
                if probe == (self.flags & 0xFFF) {
                    found_i = i;
                    break;
                }
            }

            if found_i < 2 {
                flevel = 0;
            } else if found_i < 6 {
                flevel = 1;
            } else if found_i == 6 {
                flevel = 2;
            }

            let mut header = (cmf << 8) | (flevel << 6);
            header += 31 - (header % 31);
            let flg = header & 0xFF;

            self.put_bits(cmf, 8);
            self.put_bits(flg, 8);
        }

        // Final block bit
        self.put_bits(if flush == TdeflFlush::Finish as i32 { 1 } else { 0 }, 1);

        let saved_output_cursor = self.output_cursor;
        let saved_bit_buf = self.bit_buffer;
        let saved_bits_in = self.bits_in;

        let mut comp_block_succeeded = false;
        if !use_raw_block {
            comp_block_succeeded = self.compress_block(
                (self.flags & TDEFL_FORCE_ALL_STATIC_BLOCKS) != 0 || self.total_lz_bytes < 48,
            );
        }

        // Check if raw block is needed
        let output_expanded = self.total_lz_bytes > 0
            && (self.output_cursor - saved_output_cursor + 1) as u32 >= self.total_lz_bytes;
        let dict_has_data = self.lookahead_pos.wrapping_sub(self.lz_code_buf_dict_pos) <= self.dict_size;

        if (use_raw_block || (output_expanded && dict_has_data)) && dict_has_data {
            self.output_cursor = saved_output_cursor;
            self.bit_buffer = saved_bit_buf;
            self.bits_in = saved_bits_in;
            self.put_bits(0, 2);
            if self.bits_in > 0 {
                self.put_bits(0, 8 - self.bits_in);
            }
            // Write total_lz_bytes and its complement
            let tlz = self.total_lz_bytes;
            self.put_bits(tlz & 0xFFFF, 16);
            self.put_bits((tlz ^ 0xFFFF) & 0xFFFF, 16);
            // Write raw bytes from dictionary
            for i in 0..tlz {
                let idx = (self.lz_code_buf_dict_pos + i) as usize & TDEFL_LZ_DICT_SIZE_MASK;
                self.put_bits(self.dict[idx] as u32, 8);
            }
        } else if !comp_block_succeeded {
            self.output_cursor = saved_output_cursor;
            self.bit_buffer = saved_bit_buf;
            self.bits_in = saved_bits_in;
            self.compress_block(true);
        }

        // Flush handling
        if flush != 0 {
            if flush == TdeflFlush::Finish as i32 {
                if self.bits_in > 0 {
                    self.put_bits(0, 8 - self.bits_in);
                }
                if (self.flags & TDEFL_WRITE_ZLIB_HEADER) != 0 {
                    let mut a = self.adler32;
                    for _ in 0..4 {
                        self.put_bits((a >> 24) & 0xFF, 8);
                        a <<= 8;
                    }
                }
            } else {
                self.put_bits(0, 3);
                if self.bits_in > 0 {
                    self.put_bits(0, 8 - self.bits_in);
                }
                let mut z: u32 = 0;
                for _ in 0..2 {
                    self.put_bits(z & 0xFFFF, 16);
                    z ^= 0xFFFF;
                }
            }
        }

        // Clear huff counts for next block
        for i in 0..TDEFL_MAX_HUFF_SYMBOLS_0 {
            self.huff_count[0][i] = 0;
        }
        for i in 0..TDEFL_MAX_HUFF_SYMBOLS_1 {
            self.huff_count[1][i] = 0;
        }

        // Reset LZ code buffer
        self.lz_code_buf_cursor = 1;
        self.lz_flags_cursor = 0;
        self.num_flags_left = 8;
        self.lz_code_buf_dict_pos += self.total_lz_bytes;
        self.total_lz_bytes = 0;
        self.block_index += 1;

        let n = self.output_cursor - output_buf_start;
        if n != 0 {
            if self.put_buf_func.is_some() {
                // Callback mode: not supported in safe Rust without raw pointers
                // Store output in put_buf_user
                let data = self.output_buf[output_buf_start..self.output_cursor].to_vec();
                self.put_buf_user.extend_from_slice(&data);
            } else if use_internal_buf {
                let bytes_to_copy = n.min(self.output_buf.len() - self.out_buf_ofs);
                // In the safe Rust model, output is accumulated in output_buf
                // and we track out_buf_ofs.
                self.out_buf_ofs += bytes_to_copy;
                let remaining = n - bytes_to_copy;
                if remaining != 0 {
                    self.output_flush_ofs = bytes_to_copy as u32;
                    self.output_flush_remaining = remaining as u32;
                }
            } else {
                self.out_buf_ofs += n;
            }
        }

        self.output_flush_remaining as i32
    }

    // ---- Normal compression (with lazy matching) ----

    /// Main compression loop with lazy matching.
    fn compress_normal(&mut self) -> bool {
        while self.src_buf_left > 0 || (self.flush != TdeflFlush::NoFlush && self.lookahead_size > 0) {
            // Update dictionary and hash chains
            if (self.lookahead_size + self.dict_size) >= (TDEFL_MIN_MATCH_LEN as u32 - 1) {
                let dst_pos_init = (self.lookahead_pos + self.lookahead_size) as usize & TDEFL_LZ_DICT_SIZE_MASK;
                let ins_pos_init = (self.lookahead_pos + self.lookahead_size).wrapping_sub(2);
                let mut hash_val = ((self.dict[(ins_pos_init as usize) & TDEFL_LZ_DICT_SIZE_MASK] as u32) << TDEFL_LZ_HASH_SHIFT)
                    ^ (self.dict[((ins_pos_init + 1) as usize) & TDEFL_LZ_DICT_SIZE_MASK] as u32);
                let num_bytes_to_process = self.src_buf_left.min(TDEFL_MAX_MATCH_LEN - self.lookahead_size as usize);
                self.src_buf_left -= num_bytes_to_process;
                self.lookahead_size += num_bytes_to_process as u32;

                let mut dst_pos = dst_pos_init;
                let mut ins_pos = ins_pos_init;
                for _ in 0..num_bytes_to_process {
                    if self.src_pos >= self.src_data.len() {
                        break;
                    }
                    let c = self.src_data[self.src_pos];
                    self.src_pos += 1;
                    self.dict[dst_pos] = c;
                    if dst_pos < TDEFL_MAX_MATCH_LEN - 1 {
                        self.dict[TDEFL_LZ_DICT_SIZE + dst_pos] = c;
                    }
                    hash_val = ((hash_val << TDEFL_LZ_HASH_SHIFT) ^ (c as u32)) & (TDEFL_LZ_HASH_SIZE as u32 - 1);
                    self.next[(ins_pos as usize) & TDEFL_LZ_DICT_SIZE_MASK] = self.hash[hash_val as usize];
                    self.hash[hash_val as usize] = ins_pos as u16;
                    dst_pos = (dst_pos + 1) & TDEFL_LZ_DICT_SIZE_MASK;
                    ins_pos = ins_pos.wrapping_add(1);
                }
            } else {
                while self.src_buf_left > 0 && (self.lookahead_size < TDEFL_MAX_MATCH_LEN as u32) {
                    if self.src_pos >= self.src_data.len() {
                        break;
                    }
                    let c = self.src_data[self.src_pos];
                    self.src_pos += 1;
                    let dst_pos = (self.lookahead_pos + self.lookahead_size) as usize & TDEFL_LZ_DICT_SIZE_MASK;
                    self.src_buf_left -= 1;
                    self.dict[dst_pos] = c;
                    if dst_pos < TDEFL_MAX_MATCH_LEN - 1 {
                        self.dict[TDEFL_LZ_DICT_SIZE + dst_pos] = c;
                    }
                    self.lookahead_size += 1;
                    if (self.lookahead_size + self.dict_size) >= TDEFL_MIN_MATCH_LEN as u32 {
                        let ins_pos = self.lookahead_pos + self.lookahead_size - 1 - 2;
                        let hash_val = (((self.dict[(ins_pos as usize) & TDEFL_LZ_DICT_SIZE_MASK] as u32) << (TDEFL_LZ_HASH_SHIFT * 2))
                            ^ ((self.dict[((ins_pos + 1) as usize) & TDEFL_LZ_DICT_SIZE_MASK] as u32) << TDEFL_LZ_HASH_SHIFT)
                            ^ (c as u32))
                            & (TDEFL_LZ_HASH_SIZE as u32 - 1);
                        self.next[(ins_pos as usize) & TDEFL_LZ_DICT_SIZE_MASK] = self.hash[hash_val as usize];
                        self.hash[hash_val as usize] = ins_pos as u16;
                    }
                }
            }

            self.dict_size = self.dict_size.min(TDEFL_LZ_DICT_SIZE as u32 - self.lookahead_size);
            if self.flush == TdeflFlush::NoFlush && self.lookahead_size < TDEFL_MAX_MATCH_LEN as u32 {
                break;
            }

            // Lazy/greedy parsing state machine
            let mut len_to_move: u32 = 1;
            let mut cur_match_dist: u32 = 0;
            let mut cur_match_len: u32 = if self.saved_match_len > 0 {
                self.saved_match_len
            } else {
                TDEFL_MIN_MATCH_LEN as u32 - 1
            };
            let cur_pos = self.lookahead_pos as usize & TDEFL_LZ_DICT_SIZE_MASK;

            if (self.flags & (TDEFL_RLE_MATCHES | TDEFL_FORCE_ALL_RAW_BLOCKS)) != 0 {
                if self.dict_size > 0 && (self.flags & TDEFL_FORCE_ALL_RAW_BLOCKS) == 0 {
                    let c = self.dict[(cur_pos.wrapping_sub(1)) & TDEFL_LZ_DICT_SIZE_MASK];
                    cur_match_len = 0;
                    while cur_match_len < self.lookahead_size {
                        if self.dict[cur_pos + cur_match_len as usize] != c {
                            break;
                        }
                        cur_match_len += 1;
                    }
                    if cur_match_len < TDEFL_MIN_MATCH_LEN as u32 {
                        cur_match_len = 0;
                    } else {
                        cur_match_dist = 1;
                    }
                }
            } else {
                self.find_match(
                    self.lookahead_pos,
                    self.dict_size,
                    self.lookahead_size,
                    &mut cur_match_dist,
                    &mut cur_match_len,
                );
            }

            if (cur_match_len == TDEFL_MIN_MATCH_LEN as u32 && cur_match_dist >= 8 * 1024)
                || cur_pos as u32 == cur_match_dist
                || ((self.flags & TDEFL_FILTER_MATCHES) != 0 && cur_match_len <= 5)
            {
                cur_match_dist = 0;
                cur_match_len = 0;
            }

            if self.saved_match_len > 0 {
                if cur_match_len > self.saved_match_len {
                    self.record_literal(self.saved_lit as u8);
                    if cur_match_len >= 128 {
                        self.record_match(cur_match_len, cur_match_dist);
                        self.saved_match_len = 0;
                        len_to_move = cur_match_len;
                    } else {
                        self.saved_lit = self.dict[cur_pos.min(self.dict.len() - 1)] as u32;
                        self.saved_match_dist = cur_match_dist;
                        self.saved_match_len = cur_match_len;
                    }
                } else {
                    self.record_match(self.saved_match_len, self.saved_match_dist);
                    len_to_move = self.saved_match_len - 1;
                    self.saved_match_len = 0;
                }
            } else if cur_match_dist == 0 {
                self.record_literal(self.dict[cur_pos.min(self.dict.len() - 1)]);
            } else if self.greedy_parsing || (self.flags & TDEFL_RLE_MATCHES) != 0 || cur_match_len >= 128 {
                self.record_match(cur_match_len, cur_match_dist);
                len_to_move = cur_match_len;
            } else {
                self.saved_lit = self.dict[cur_pos.min(self.dict.len() - 1)] as u32;
                self.saved_match_dist = cur_match_dist;
                self.saved_match_len = cur_match_len;
            }

            self.lookahead_pos += len_to_move;
            self.lookahead_size -= len_to_move;
            self.dict_size = (self.dict_size + len_to_move).min(TDEFL_LZ_DICT_SIZE as u32);

            // Check if it's time to flush
            let lz_buf_almost_full = self.lz_code_buf_cursor > TDEFL_LZ_CODE_BUF_SIZE - 8;
            let ratio_check = self.total_lz_bytes > 31 * 1024
                && ((((self.lz_code_buf_cursor as u32) * 115) >> 7) >= self.total_lz_bytes
                    || (self.flags & TDEFL_FORCE_ALL_RAW_BLOCKS) != 0);
            if lz_buf_almost_full || ratio_check {
                let n = self.flush_block(0);
                if n != 0 {
                    return n >= 0;
                }
            }
        }

        true
    }

    // ---- Flush output buffer ----

    /// Flush remaining output data.
    fn flush_output_buffer(&mut self) -> TdeflStatus {
        if self.output_flush_remaining > 0 {
            let n = self.output_flush_remaining.min(
                (self.output_buf.len() - self.out_buf_ofs) as u32,
            );
            self.output_flush_ofs += n;
            self.output_flush_remaining -= n;
            self.out_buf_ofs += n as usize;
        }

        if self.finished && self.output_flush_remaining == 0 {
            TdeflStatus::Done
        } else {
            TdeflStatus::Okay
        }
    }

    // ---- Top-level compress API ----

    /// Compress input data. This is the main entry point.
    ///
    /// `input` is the data to compress. `flush` controls flushing behavior.
    /// Returns (status, bytes_consumed).
    pub fn compress(&mut self, input: &[u8], output: &mut Vec<u8>, flush: TdeflFlush) -> TdeflStatus {
        if self.prev_return_status != TdeflStatus::Okay {
            return TdeflStatus::BadParam;
        }
        if self.wants_to_finish && flush != TdeflFlush::Finish {
            return TdeflStatus::BadParam;
        }

        self.wants_to_finish = self.wants_to_finish || flush == TdeflFlush::Finish;

        // Store input
        self.src_data = input.to_vec();
        self.src_pos = 0;
        self.src_buf_left = input.len();
        self.out_buf_ofs = 0;
        self.flush = flush;

        if self.output_flush_remaining > 0 || self.finished {
            self.prev_return_status = self.flush_output_buffer();
            return self.prev_return_status;
        }

        if !self.compress_normal() {
            return self.prev_return_status;
        }

        if (self.flags & (TDEFL_WRITE_ZLIB_HEADER | TDEFL_COMPUTE_ADLER32)) != 0 && !input.is_empty() {
            let consumed = self.src_pos;
            self.adler32 = mz_adler32(self.adler32, &input[..consumed]);
        }

        if flush != TdeflFlush::NoFlush
            && self.lookahead_size == 0
            && self.src_buf_left == 0
            && self.output_flush_remaining == 0
        {
            if self.flush_block(flush as i32) < 0 {
                return self.prev_return_status;
            }
            self.finished = flush == TdeflFlush::Finish;
            if flush == TdeflFlush::FullFlush {
                for h in self.hash.iter_mut() {
                    *h = 0;
                }
                for n in self.next.iter_mut() {
                    *n = 0;
                }
                self.dict_size = 0;
            }
        }

        self.prev_return_status = self.flush_output_buffer();

        // Copy output
        let end = self.out_buf_ofs.min(self.output_buf.len());
        output.extend_from_slice(&self.output_buf[..end]);

        self.prev_return_status
    }

    /// Get the previous return status.
    pub fn get_prev_return_status(&self) -> TdeflStatus {
        self.prev_return_status
    }

    /// Get the current Adler-32 checksum.
    pub fn get_adler32(&self) -> u32 {
        self.adler32
    }
}

// ---- Free functions (Huffman helpers) ----

/// Radix sort symbol frequencies by 16-bit key. Returns a reference to the sorted slice.
fn radix_sort_syms<'a>(
    num_syms: usize,
    syms0: &'a mut [SymFreq; TDEFL_MAX_HUFF_SYMBOLS],
    syms1: &'a mut [SymFreq; TDEFL_MAX_HUFF_SYMBOLS],
) -> &'a mut [SymFreq] {
    let mut hist = [0u32; 512];

    for i in 0..num_syms {
        let freq = syms0[i].key as u32;
        hist[(freq & 0xFF) as usize] += 1;
        hist[256 + ((freq >> 8) & 0xFF) as usize] += 1;
    }

    let mut total_passes: u32 = 2;
    while total_passes > 1 && num_syms as u32 == hist[((total_passes - 1) * 256) as usize] {
        total_passes -= 1;
    }

    // We need to swap between syms0 and syms1. We'll use a flag.
    // Pass 0: read from syms0, write to syms1
    // Pass 1: read from syms1, write to syms0
    // Result is whichever was last written to.

    let mut cur_is_0 = true; // current source is syms0

    for pass in 0..total_passes {
        let pass_shift = pass * 8;
        let hist_offset = (pass * 256) as usize;

        let mut offsets = [0u32; 256];
        let mut cur_ofs: u32 = 0;
        for i in 0..256 {
            offsets[i] = cur_ofs;
            cur_ofs += hist[hist_offset + i];
        }

        if cur_is_0 {
            for i in 0..num_syms {
                let idx = ((syms0[i].key >> pass_shift) & 0xFF) as usize;
                let dst = offsets[idx] as usize;
                offsets[idx] += 1;
                syms1[dst] = syms0[i];
            }
        } else {
            for i in 0..num_syms {
                let idx = ((syms1[i].key >> pass_shift) & 0xFF) as usize;
                let dst = offsets[idx] as usize;
                offsets[idx] += 1;
                syms0[dst] = syms1[i];
            }
        }
        cur_is_0 = !cur_is_0;
    }

    if cur_is_0 {
        &mut syms0[..num_syms]
    } else {
        &mut syms1[..num_syms]
    }
}

/// Calculate minimum redundancy (optimal Huffman code lengths).
/// Moffat-Katajainen algorithm.
fn calculate_minimum_redundancy(a: &mut [SymFreq]) {
    let n = a.len() as i32;
    if n == 0 {
        return;
    }
    if n == 1 {
        a[0].key = 1;
        return;
    }

    a[0].key = a[0].key.wrapping_add(a[1].key);
    let mut root: i32 = 0;
    let mut leaf: i32 = 2;

    for next in 1..(n - 1) {
        if leaf >= n || a[root as usize].key < a[leaf as usize].key {
            a[next as usize].key = a[root as usize].key;
            a[root as usize].key = next as u16;
            root += 1;
        } else {
            a[next as usize].key = a[leaf as usize].key;
            leaf += 1;
        }

        if leaf >= n || (root < next && a[root as usize].key < a[leaf as usize].key) {
            a[next as usize].key = a[next as usize].key.wrapping_add(a[root as usize].key);
            a[root as usize].key = next as u16;
            root += 1;
        } else {
            a[next as usize].key = a[next as usize].key.wrapping_add(a[leaf as usize].key);
            leaf += 1;
        }
    }

    a[(n - 2) as usize].key = 0;
    for next in (0..=(n - 3)).rev() {
        a[next as usize].key = a[a[next as usize].key as usize].key + 1;
    }

    let mut avbl: i32 = 1;
    let mut used: i32 = 0;
    let mut dpth: i32 = 0;
    let mut root = n - 2;
    let mut next = n - 1;

    while avbl > 0 {
        while root >= 0 && a[root as usize].key as i32 == dpth {
            used += 1;
            root -= 1;
        }
        while avbl > used {
            a[next as usize].key = dpth as u16;
            next -= 1;
            avbl -= 1;
        }
        avbl = 2 * used;
        dpth += 1;
        used = 0;
    }
}

/// Enforce maximum Huffman code size by redistributing code lengths.
fn huffman_enforce_max_code_size(num_codes: &mut [i32], code_list_len: i32, max_code_size: i32) {
    if code_list_len <= 1 {
        return;
    }
    for i in (max_code_size + 1) as usize..=TDEFL_MAX_SUPPORTED_HUFF_CODESIZE {
        num_codes[max_code_size as usize] += num_codes[i];
    }
    let mut total: u32 = 0;
    for i in (1..=max_code_size as usize).rev() {
        total += (num_codes[i] as u32) << (max_code_size as usize - i);
    }
    while total != 1u32 << max_code_size {
        num_codes[max_code_size as usize] -= 1;
        for i in (1..max_code_size as usize).rev() {
            if num_codes[i] > 0 {
                num_codes[i] -= 1;
                num_codes[i + 1] += 2;
                break;
            }
        }
        total -= 1;
    }
}

// ---- Adler-32 (needed for zlib header) ----

/// Compute Adler-32 checksum.
pub fn mz_adler32(adler: u32, data: &[u8]) -> u32 {
    if data.is_empty() {
        return 1;
    }
    let mut s1 = adler & 0xFFFF;
    let mut s2 = adler >> 16;
    let mut remaining = data.len();
    let mut idx = 0;

    while remaining > 0 {
        let block_len = remaining.min(5552);
        let block_end = idx + block_len;

        for &byte in &data[idx..block_end] {
            s1 = s1.wrapping_add(byte as u32);
            s2 = s2.wrapping_add(s1);
        }

        s1 %= 65521;
        s2 %= 65521;
        remaining -= block_len;
        idx = block_end;
    }

    (s2 << 16) | s1
}

// ---- Convenience APIs ----

/// Compress a block of memory, returning the compressed data.
/// `flags` are the compression flags (probes | options).
pub fn tdefl_compress_mem_to_vec(src: &[u8], flags: u32) -> Option<Vec<u8>> {
    let mut comp = TdeflCompressor::new(flags);
    let mut output = Vec::new();
    let status = comp.compress(src, &mut output, TdeflFlush::Finish);
    if status == TdeflStatus::Done {
        // Also collect any callback-buffered data
        if !comp.put_buf_user.is_empty() {
            output.extend_from_slice(&comp.put_buf_user);
        }
        Some(output)
    } else {
        None
    }
}

/// Create compression flags from zlib-style parameters.
/// `level` may range from 0 to 10. `window_bits` > 0 enables zlib header.
/// `strategy` controls the match-finding strategy.
pub fn tdefl_create_comp_flags_from_zip_params(level: i32, window_bits: i32, strategy: i32) -> u32 {
    let effective_level = if level >= 0 { level.min(10) } else { MZ_DEFAULT_LEVEL };
    let mut comp_flags = S_TDEFL_NUM_PROBES[effective_level as usize]
        | if level <= 3 { TDEFL_GREEDY_PARSING_FLAG } else { 0 };

    if window_bits > 0 {
        comp_flags |= TDEFL_WRITE_ZLIB_HEADER;
    }

    if level == 0 {
        comp_flags |= TDEFL_FORCE_ALL_RAW_BLOCKS;
    } else if strategy == MZ_FILTERED {
        comp_flags |= TDEFL_FILTER_MATCHES;
    } else if strategy == MZ_HUFFMAN_ONLY {
        comp_flags &= !TDEFL_MAX_PROBES_MASK;
    } else if strategy == MZ_FIXED {
        comp_flags |= TDEFL_FORCE_ALL_STATIC_BLOCKS;
    } else if strategy == MZ_RLE {
        comp_flags |= TDEFL_RLE_MATCHES;
    }

    comp_flags
}

/// Compress memory to memory. Returns the number of bytes written, or 0 on failure.
pub fn tdefl_compress_mem_to_mem(src: &[u8], flags: u32) -> Vec<u8> {
    tdefl_compress_mem_to_vec(src, flags).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radix_sort_basic() {
        let mut syms0 = [SymFreq::default(); TDEFL_MAX_HUFF_SYMBOLS];
        let mut syms1 = [SymFreq::default(); TDEFL_MAX_HUFF_SYMBOLS];

        syms0[0] = SymFreq { key: 5, sym_index: 0 };
        syms0[1] = SymFreq { key: 1, sym_index: 1 };
        syms0[2] = SymFreq { key: 3, sym_index: 2 };

        let sorted = radix_sort_syms(3, &mut syms0, &mut syms1);
        assert_eq!(sorted[0].key, 1);
        assert_eq!(sorted[1].key, 3);
        assert_eq!(sorted[2].key, 5);
    }

    #[test]
    fn test_calculate_minimum_redundancy_single() {
        let mut syms = [SymFreq { key: 10, sym_index: 0 }];
        calculate_minimum_redundancy(&mut syms);
        assert_eq!(syms[0].key, 1);
    }

    #[test]
    fn test_calculate_minimum_redundancy_two() {
        let mut syms = [
            SymFreq { key: 1, sym_index: 0 },
            SymFreq { key: 1, sym_index: 1 },
        ];
        calculate_minimum_redundancy(&mut syms);
        assert_eq!(syms[0].key, 1);
        assert_eq!(syms[1].key, 1);
    }

    #[test]
    fn test_huffman_enforce_max_code_size() {
        let mut codes = [0i32; 1 + TDEFL_MAX_SUPPORTED_HUFF_CODESIZE];
        codes[1] = 1;
        codes[2] = 2;
        huffman_enforce_max_code_size(&mut codes, 3, 15);
        // Should not panic
    }

    #[test]
    fn test_compressor_creation() {
        let comp = TdeflCompressor::new(0);
        assert_eq!(comp.prev_return_status, TdeflStatus::Okay);
        assert_eq!(comp.adler32, 1);
    }

    #[test]
    fn test_put_bits() {
        let mut comp = TdeflCompressor::new(0);
        comp.output_cursor = 0;
        comp.output_end = TDEFL_OUT_BUF_SIZE;
        comp.put_bits(0xFF, 8);
        assert_eq!(comp.output_buf[0], 0xFF);
    }

    #[test]
    fn test_create_comp_flags_from_zip_params() {
        let flags = tdefl_create_comp_flags_from_zip_params(6, 15, MZ_DEFAULT_STRATEGY);
        assert_ne!(flags & TDEFL_WRITE_ZLIB_HEADER, 0);
    }

    #[test]
    fn test_create_comp_flags_raw() {
        let flags = tdefl_create_comp_flags_from_zip_params(0, -15, MZ_DEFAULT_STRATEGY);
        assert_ne!(flags & TDEFL_FORCE_ALL_RAW_BLOCKS, 0);
    }

    #[test]
    fn test_adler32() {
        let result = mz_adler32(1, b"Hello");
        assert_ne!(result, 0);
        assert_ne!(result, 1);
    }

    #[test]
    fn test_compress_empty() {
        let mut comp = TdeflCompressor::new(TDEFL_WRITE_ZLIB_HEADER);
        let mut output = Vec::new();
        let status = comp.compress(&[], &mut output, TdeflFlush::Finish);
        assert!(status == TdeflStatus::Done || status == TdeflStatus::Okay);
    }

    #[test]
    fn test_record_literal() {
        let mut comp = TdeflCompressor::new(0);
        comp.record_literal(b'A');
        assert_eq!(comp.total_lz_bytes, 1);
        assert_eq!(comp.huff_count[0][b'A' as usize], 1);
    }

    #[test]
    fn test_record_match() {
        let mut comp = TdeflCompressor::new(0);
        comp.record_match(3, 1); // minimum match
        assert_eq!(comp.total_lz_bytes, 3);
    }

    #[test]
    fn test_optimize_huffman_static() {
        let mut comp = TdeflCompressor::new(0);
        // Set up static block code sizes
        for i in 0..=143 { comp.huff_code_sizes[0][i] = 8; }
        for i in 144..=255 { comp.huff_code_sizes[0][i] = 9; }
        for i in 256..=279 { comp.huff_code_sizes[0][i] = 7; }
        for i in 280..=287 { comp.huff_code_sizes[0][i] = 8; }
        comp.optimize_huffman_table(0, 288, 15, true);
        // Should produce valid codes
        assert_ne!(comp.huff_codes[0][0], 0xFFFF);
    }

    #[test]
    fn test_compress_small_data() {
        let data = b"Hello, World! This is a test of the DEFLATE compressor.";
        let flags = tdefl_create_comp_flags_from_zip_params(1, -15, MZ_DEFAULT_STRATEGY);
        let mut comp = TdeflCompressor::new(flags);
        let mut output = Vec::new();
        let status = comp.compress(data, &mut output, TdeflFlush::Finish);
        assert!(status == TdeflStatus::Done || status == TdeflStatus::Okay);
    }

    #[test]
    fn test_flush_enum_values() {
        assert_eq!(TdeflFlush::NoFlush as i32, 0);
        assert_eq!(TdeflFlush::SyncFlush as i32, 2);
        assert_eq!(TdeflFlush::FullFlush as i32, 3);
        assert_eq!(TdeflFlush::Finish as i32, 4);
    }

    #[test]
    fn test_status_enum_values() {
        assert_eq!(TdeflStatus::BadParam as i32, -2);
        assert_eq!(TdeflStatus::PutBufFailed as i32, -1);
        assert_eq!(TdeflStatus::Okay as i32, 0);
        assert_eq!(TdeflStatus::Done as i32, 1);
    }

    #[test]
    fn test_bitmasks() {
        assert_eq!(MZ_BITMASKS[0], 0);
        assert_eq!(MZ_BITMASKS[1], 1);
        assert_eq!(MZ_BITMASKS[8], 0xFF);
        assert_eq!(MZ_BITMASKS[16], 0xFFFF);
    }
}
