/// TINFL — tiny inflate (decompression) library, translated from miniz tinfl.c.
///
/// Pure safe Rust, no external crates, no unsafe, no raw pointers.
/// Implements a streaming DEFLATE decompressor with optional zlib header parsing
/// and Adler-32 checksum verification.

// Constants
const TINFL_FAST_LOOKUP_SIZE: usize = 1 << 10; // 1024
const TINFL_FAST_LOOKUP_BITS: u32 = 10;
const TINFL_MAX_HUFF_SYMBOLS_0: usize = 288;
const TINFL_MAX_HUFF_SYMBOLS_1: usize = 32;
const TINFL_MAX_HUFF_SYMBOLS_2: usize = 19;
const TINFL_LZ_DICT_SIZE: usize = 32768;

/// Status codes returned by the decompressor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TinflStatus {
    /// Decompressor failed (corrupted data or internal error).
    Failed,
    /// Bad parameter passed to decompress.
    BadParam,
    /// Adler-32 checksum mismatch after decompression.
    Adler32Mismatch,
    /// Decompressor needs more input bytes to continue.
    NeedsMoreInput,
    /// Decompressor cannot make forward progress (no more input available).
    FailedCannotMakeProgress,
    /// Decompressor has more output available but the output buffer is full.
    HasMoreOutput,
    /// Decompression completed successfully.
    Done,
}

// Decompressor flags
pub const TINFL_FLAG_PARSE_ZLIB_HEADER: u32 = 1;
pub const TINFL_FLAG_HAS_MORE_INPUT: u32 = 2;
pub const TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF: u32 = 4;
pub const TINFL_FLAG_COMPUTE_ADLER32: u32 = 8;

/// Streaming DEFLATE decompressor state machine.
///
/// Create with `TinflDecompressor::new()`, then call `decompress()` repeatedly
/// with input/output slices until it returns `TinflStatus::Done`.
#[derive(Clone)]
pub struct TinflDecompressor {
    // State machine position (maps to C coroutine state indices)
    state: u32,

    // Bit buffer
    num_bits: u32,
    bit_buf: u64,

    // Decoding state
    dist: u32,
    counter: u32,
    num_extra: u32,
    dist_from_out_buf_start: usize,

    // Zlib header bytes
    zhdr0: u32,
    zhdr1: u32,
    z_adler32: u32,
    check_adler32: u32,

    // Block type and final flag
    m_type: u32,
    m_final: u32,

    // Raw header for type 0 (uncompressed) blocks
    raw_header: [u32; 4],

    // Table sizes for the 3 Huffman tables
    table_sizes: [u32; 3],

    // Code sizes for each Huffman tree
    code_size_0: [u8; TINFL_MAX_HUFF_SYMBOLS_0],
    code_size_1: [u8; TINFL_MAX_HUFF_SYMBOLS_1],
    code_size_2: [u8; TINFL_MAX_HUFF_SYMBOLS_2],

    // Huffman decode trees (stored as i16 for negative-index tree walk)
    tree_0: [i16; TINFL_MAX_HUFF_SYMBOLS_0 * 2],
    tree_1: [i16; TINFL_MAX_HUFF_SYMBOLS_1 * 2],
    tree_2: [i16; TINFL_MAX_HUFF_SYMBOLS_2 * 2],

    // Fast lookup tables for each Huffman tree
    look_up: [[i16; TINFL_FAST_LOOKUP_SIZE]; 3],

    // Temporary storage for length/distance code sizes (dynamic Huffman)
    len_codes: [u8; TINFL_MAX_HUFF_SYMBOLS_0 + TINFL_MAX_HUFF_SYMBOLS_1 + 137],
}

impl TinflDecompressor {
    /// Create a new decompressor in its initial state.
    pub fn new() -> Self {
        Self {
            state: 0,
            num_bits: 0,
            bit_buf: 0,
            dist: 0,
            counter: 0,
            num_extra: 0,
            dist_from_out_buf_start: 0,
            zhdr0: 0,
            zhdr1: 0,
            z_adler32: 1,
            check_adler32: 1,
            m_type: 0,
            m_final: 0,
            raw_header: [0; 4],
            table_sizes: [0; 3],
            code_size_0: [0; TINFL_MAX_HUFF_SYMBOLS_0],
            code_size_1: [0; TINFL_MAX_HUFF_SYMBOLS_1],
            code_size_2: [0; TINFL_MAX_HUFF_SYMBOLS_2],
            tree_0: [0; TINFL_MAX_HUFF_SYMBOLS_0 * 2],
            tree_1: [0; TINFL_MAX_HUFF_SYMBOLS_1 * 2],
            tree_2: [0; TINFL_MAX_HUFF_SYMBOLS_2 * 2],
            look_up: [[0i16; TINFL_FAST_LOOKUP_SIZE]; 3],
            len_codes: [0; TINFL_MAX_HUFF_SYMBOLS_0 + TINFL_MAX_HUFF_SYMBOLS_1 + 137],
        }
    }

    /// Reset the decompressor to its initial state.
    pub fn init(&mut self) {
        self.state = 0;
    }

    /// Return the computed Adler-32 checksum of the decompressed data so far.
    pub fn adler32(&self) -> u32 {
        self.check_adler32
    }

    fn clear_tree(&mut self, tree_type: u32) {
        match tree_type {
            0 => self.tree_0.fill(0),
            1 => self.tree_1.fill(0),
            _ => self.tree_2.fill(0),
        }
    }

    fn get_code_size(&self, table: u32, idx: usize) -> u8 {
        match table {
            0 => self.code_size_0[idx],
            1 => self.code_size_1[idx],
            _ => self.code_size_2[idx],
        }
    }

    fn get_tree(&self, table: u32, idx: usize) -> i16 {
        match table {
            0 => self.tree_0[idx],
            1 => self.tree_1[idx],
            _ => self.tree_2[idx],
        }
    }

    fn set_tree(&mut self, table: u32, idx: usize, val: i16) {
        match table {
            0 => self.tree_0[idx] = val,
            1 => self.tree_1[idx] = val,
            _ => self.tree_2[idx] = val,
        }
    }

    /// Main decompression function.
    ///
    /// # Arguments
    /// * `input` - Compressed input bytes.
    /// * `output` - Output buffer. For non-wrapping mode, must be large enough for entire output.
    ///   For wrapping mode, must be a power-of-2 sized buffer (typically 32 KB).
    /// * `out_pos` - Current write position in the output buffer.
    /// * `decomp_flags` - Combination of `TINFL_FLAG_*` constants.
    ///
    /// # Returns
    /// `(in_bytes_consumed, out_bytes_written, status)` on success/suspend,
    /// or `Err(TinflStatus)` on fatal error.
    pub fn decompress(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        out_pos: usize,
        decomp_flags: u32,
    ) -> (usize, usize, TinflStatus) {
        // Static tables
        const LENGTH_BASE: [u16; 31] = [
            3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31,
            35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258, 0, 0,
        ];
        const LENGTH_EXTRA: [u8; 31] = [
            0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2,
            3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0, 0, 0,
        ];
        const DIST_BASE: [u16; 32] = [
            1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193,
            257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289,
            16385, 24577, 0, 0,
        ];
        const DIST_EXTRA: [u8; 32] = [
            0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6,
            7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 0, 0,
        ];
        const LENGTH_DEZIGZAG: [u8; 19] = [
            16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
        ];
        const MIN_TABLE_SIZES: [u32; 3] = [257, 1, 4];
        // Bits to read for dynamic table sizes: HLIT=5, HDIST=5, HCLEN=4
        const TABLE_SIZE_BITS: [u32; 3] = [5, 5, 4];

        let in_buf = input;
        let in_buf_end = in_buf.len();
        let mut in_pos: usize = 0;

        // out_pos is the current write cursor within output[]
        // out_start is where we started this call (for computing bytes written)
        let out_buf_len = output.len();
        let out_start = out_pos;
        let mut out_cur = out_pos;

        let out_buf_size_mask: usize =
            if (decomp_flags & TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) != 0 {
                usize::MAX
            } else {
                out_buf_len.wrapping_sub(1)
            };

        // Validate output buffer size (must be power of 2 or non-wrapping)
        if (decomp_flags & TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) == 0 {
            if out_buf_len == 0 || (out_buf_len & (out_buf_len - 1)) != 0 {
                return (0, 0, TinflStatus::BadParam);
            }
        }

        let mut status;

        // Restore saved local state from decompressor struct
        let mut num_bits = self.num_bits;
        let mut bit_buf = self.bit_buf;
        let mut dist = self.dist;
        let mut counter = self.counter;
        let mut num_extra = self.num_extra;
        let mut dist_from_out_buf_start = self.dist_from_out_buf_start;

        // ---- Macros as closures/inline helpers are not possible due to borrow
        // ---- issues, so we implement the state machine as a big loop with match.

        // The C code uses TINFL_CR_RETURN(state_index, result) which saves state and
        // jumps to common_exit, then on re-entry jumps to `case state_index:`.
        // We implement this with a loop + match on self.state, where each arm does
        // work and either advances self.state or returns (suspend).

        'state_machine: loop {
            match self.state {
                // ---- State 0: Initialization
                0 => {
                    bit_buf = 0;
                    num_bits = 0;
                    dist = 0;
                    counter = 0;
                    num_extra = 0;
                    self.zhdr0 = 0;
                    self.zhdr1 = 0;
                    self.z_adler32 = 1;
                    self.check_adler32 = 1;
                    if (decomp_flags & TINFL_FLAG_PARSE_ZLIB_HEADER) != 0 {
                        self.state = 1;
                    } else {
                        self.state = 3; // skip to block header read
                    }
                }

                // ---- State 1: Read zlib header byte 0
                1 => {
                    if in_pos >= in_buf_end {
                        status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                            TinflStatus::NeedsMoreInput
                        } else {
                            TinflStatus::FailedCannotMakeProgress
                        };
                        break 'state_machine;
                    }
                    self.zhdr0 = in_buf[in_pos] as u32;
                    in_pos += 1;
                    self.state = 2;
                }

                // ---- State 2: Read zlib header byte 1, validate
                2 => {
                    if in_pos >= in_buf_end {
                        status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                            TinflStatus::NeedsMoreInput
                        } else {
                            TinflStatus::FailedCannotMakeProgress
                        };
                        break 'state_machine;
                    }
                    self.zhdr1 = in_buf[in_pos] as u32;
                    in_pos += 1;

                    let mut fail = (self.zhdr0 * 256 + self.zhdr1) % 31 != 0;
                    fail |= (self.zhdr1 & 32) != 0;
                    fail |= (self.zhdr0 & 15) != 8;
                    if (decomp_flags & TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) == 0 {
                        let window_size: usize = 1usize << (8 + (self.zhdr0 >> 4));
                        fail |= window_size > 32768;
                        fail |= (out_buf_size_mask + 1) < window_size;
                    }
                    if fail {
                        status = TinflStatus::Failed;
                        self.state = 36; // forever-fail
                        break 'state_machine;
                    }
                    self.state = 3;
                }

                // ---- State 3: Read block header (3 bits: final + type)
                3 => {
                    // TINFL_GET_BITS(3, r->m_final, 3)
                    while num_bits < 3 {
                        if in_pos >= in_buf_end {
                            self.state = 3;
                            status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                TinflStatus::NeedsMoreInput
                            } else {
                                TinflStatus::FailedCannotMakeProgress
                            };
                            break 'state_machine;
                        }
                        bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                        in_pos += 1;
                        num_bits += 8;
                    }
                    self.m_final = (bit_buf & 7) as u32;
                    bit_buf >>= 3;
                    num_bits -= 3;
                    self.m_type = self.m_final >> 1;

                    if self.m_type == 0 {
                        self.state = 5; // uncompressed block
                    } else if self.m_type == 3 {
                        self.state = 36; // invalid block type → fail forever
                        status = TinflStatus::Failed;
                        break 'state_machine;
                    } else {
                        // type 1 (static) or type 2 (dynamic)
                        self.state = 10; // go build Huffman tables
                    }
                }

                // ---- State 5: Skip remaining bits to byte-align, then read 4-byte raw header
                5 => {
                    let skip = num_bits & 7;
                    if skip > 0 {
                        if num_bits < skip {
                            // need_bits for skip — shouldn't happen since skip <= num_bits
                        }
                        bit_buf >>= skip;
                        num_bits -= skip;
                    }
                    self.counter = 0; // reuse as header-byte counter
                    self.state = 6;
                }

                // ---- State 6: Read raw header bytes (4 bytes: LEN, NLEN)
                6 => {
                    while (self.counter as usize) < 4 {
                        let i = self.counter as usize;
                        if num_bits > 0 {
                            // get_bits(6, 8)
                            while num_bits < 8 {
                                if in_pos >= in_buf_end {
                                    self.state = 6;
                                    status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                        TinflStatus::NeedsMoreInput
                                    } else {
                                        TinflStatus::FailedCannotMakeProgress
                                    };
                                    break 'state_machine;
                                }
                                bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                                in_pos += 1;
                                num_bits += 8;
                            }
                            self.raw_header[i] = (bit_buf & 0xFF) as u32;
                            bit_buf >>= 8;
                            num_bits -= 8;
                        } else {
                            // get_byte(7)
                            if in_pos >= in_buf_end {
                                self.state = 6;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            self.raw_header[i] = in_buf[in_pos] as u32;
                            in_pos += 1;
                        }
                        self.counter += 1;
                    }
                    // Validate LEN vs NLEN
                    let len = self.raw_header[0] | (self.raw_header[1] << 8);
                    let nlen = self.raw_header[2] | (self.raw_header[3] << 8);
                    if len != (0xFFFF ^ nlen) {
                        status = TinflStatus::Failed;
                        self.state = 36;
                        break 'state_machine;
                    }
                    counter = len;
                    self.counter = counter;
                    self.state = 51; // copy from bit buffer first
                }

                // ---- State 51: Copy raw bytes from bit buffer
                51 => {
                    while counter > 0 && num_bits > 0 {
                        // get_bits(51, 8)
                        while num_bits < 8 {
                            if in_pos >= in_buf_end {
                                self.state = 51;
                                self.counter = counter;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            in_pos += 1;
                            num_bits += 8;
                        }
                        let byte_val = (bit_buf & 0xFF) as u8;
                        bit_buf >>= 8;
                        num_bits -= 8;

                        if out_cur >= out_buf_len {
                            // TINFL_CR_RETURN(52, HAS_MORE_OUTPUT)
                            self.state = 52;
                            self.counter = counter;
                            dist = byte_val as u32; // stash the byte we already read
                            status = TinflStatus::HasMoreOutput;
                            break 'state_machine;
                        }
                        output[out_cur] = byte_val;
                        out_cur += 1;
                        counter -= 1;
                    }
                    self.counter = counter;
                    self.state = 9; // move to raw-input copy
                }

                // ---- State 52: Resume after output-full during bit-buffer raw copy
                52 => {
                    // The byte was already decoded; we need to write it
                    if out_cur >= out_buf_len {
                        status = TinflStatus::HasMoreOutput;
                        break 'state_machine;
                    }
                    output[out_cur] = dist as u8;
                    out_cur += 1;
                    counter = self.counter;
                    counter -= 1;
                    self.counter = counter;
                    self.state = 51; // continue bit-buffer copy
                }

                // ---- State 9: Copy raw bytes directly from input to output
                9 | 38 => {
                    counter = self.counter;
                    while counter > 0 {
                        if out_cur >= out_buf_len {
                            self.state = 9;
                            self.counter = counter;
                            status = TinflStatus::HasMoreOutput;
                            break 'state_machine;
                        }
                        if in_pos >= in_buf_end {
                            self.state = 38;
                            self.counter = counter;
                            status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                TinflStatus::NeedsMoreInput
                            } else {
                                TinflStatus::FailedCannotMakeProgress
                            };
                            break 'state_machine;
                        }
                        let n = core::cmp::min(
                            core::cmp::min(out_buf_len - out_cur, in_buf_end - in_pos),
                            counter as usize,
                        );
                        output[out_cur..out_cur + n].copy_from_slice(&in_buf[in_pos..in_pos + n]);
                        in_pos += n;
                        out_cur += n;
                        counter -= n as u32;
                    }
                    self.counter = counter;
                    // Block done. Check if final block.
                    if (self.m_final & 1) != 0 {
                        self.state = 32; // go to post-block cleanup
                    } else {
                        self.state = 3; // read next block header
                    }
                }

                // ---- State 10: Build Huffman tables (static or dynamic)
                10 => {
                    if self.m_type == 1 {
                        // Static Huffman: build fixed tables
                        self.table_sizes[0] = 288;
                        self.table_sizes[1] = 32;
                        self.code_size_1.fill(5);
                        for i in 0..=143 {
                            self.code_size_0[i] = 8;
                        }
                        for i in 144..=255 {
                            self.code_size_0[i] = 9;
                        }
                        for i in 256..=279 {
                            self.code_size_0[i] = 7;
                        }
                        for i in 280..=287 {
                            self.code_size_0[i] = 8;
                        }
                        self.state = 15; // skip to tree building
                    } else {
                        // Dynamic Huffman: read table sizes
                        counter = 0;
                        self.counter = 0;
                        self.state = 11;
                    }
                }

                // ---- State 11: Read dynamic Huffman table sizes (3 values)
                11 => {
                    counter = self.counter;
                    while (counter as usize) < 3 {
                        let bits_needed = TABLE_SIZE_BITS[counter as usize];
                        while num_bits < bits_needed {
                            if in_pos >= in_buf_end {
                                self.state = 11;
                                self.counter = counter;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            in_pos += 1;
                            num_bits += 8;
                        }
                        let val = (bit_buf as u32) & ((1u32 << bits_needed) - 1);
                        bit_buf >>= bits_needed;
                        num_bits -= bits_needed;
                        self.table_sizes[counter as usize] = val + MIN_TABLE_SIZES[counter as usize];
                        counter += 1;
                    }
                    // Clear code_size_2 and read code-length code sizes
                    self.code_size_2.fill(0);
                    counter = 0;
                    self.counter = 0;
                    self.state = 14;
                }

                // ---- State 14: Read code-length code sizes (for table 2)
                14 => {
                    counter = self.counter;
                    while counter < self.table_sizes[2] {
                        while num_bits < 3 {
                            if in_pos >= in_buf_end {
                                self.state = 14;
                                self.counter = counter;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            in_pos += 1;
                            num_bits += 8;
                        }
                        let s = (bit_buf & 7) as u8;
                        bit_buf >>= 3;
                        num_bits -= 3;
                        self.code_size_2[LENGTH_DEZIGZAG[counter as usize] as usize] = s;
                        counter += 1;
                    }
                    self.table_sizes[2] = 19;
                    // Now build all 3 trees (starting from table 2, then 1, then 0)
                    // C iterates: for (; (int)r->m_type >= 0; r->m_type--)
                    // On entry m_type is 2 for dynamic. We process tables 2, 1, 0.
                    // For static, m_type is 1, so we process tables 1, 0.
                    self.state = 15;
                }

                // ---- State 15: Build Huffman tree for current m_type, iterate downward
                15 => {
                    // Build tree for self.m_type (iterating from m_type down to 0)
                    let t = self.m_type as usize;
                    let table_size = self.table_sizes[t] as usize;

                    // Count symbols per code length
                    let mut total_syms = [0u32; 16];
                    for i in 0..table_size {
                        let cs = self.get_code_size(t as u32, i) as usize;
                        if cs < 16 {
                            total_syms[cs] += 1;
                        }
                    }

                    let mut used_syms = 0u32;
                    let mut total = 0u32;
                    let mut next_code = [0u32; 17];
                    for i in 1..=15 {
                        used_syms += total_syms[i];
                        total = (total + total_syms[i]) << 1;
                        next_code[i + 1] = total;
                    }

                    if total != 65536 && used_syms > 1 {
                        status = TinflStatus::Failed;
                        self.state = 36;
                        break 'state_machine;
                    }

                    // Clear lookup and tree
                    self.look_up[t].fill(0);
                    self.clear_tree(t as u32);

                    let mut tree_next: i32 = -1;
                    for sym_index in 0..table_size {
                        let code_size = self.get_code_size(t as u32, sym_index) as u32;
                        if code_size == 0 {
                            continue;
                        }
                        let cur_code = next_code[code_size as usize];
                        next_code[code_size as usize] += 1;

                        // Reverse bits
                        let mut rev_code = 0u32;
                        {
                            let mut tmp = cur_code;
                            for _ in 0..code_size {
                                rev_code = (rev_code << 1) | (tmp & 1);
                                tmp >>= 1;
                            }
                        }

                        if code_size <= TINFL_FAST_LOOKUP_BITS {
                            let k = ((code_size << 9) | (sym_index as u32)) as i16;
                            let mut rc = rev_code as usize;
                            while rc < TINFL_FAST_LOOKUP_SIZE {
                                self.look_up[t][rc] = k;
                                rc += 1 << code_size;
                            }
                            continue;
                        }

                        let mut tree_cur =
                            self.look_up[t][(rev_code as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)]
                                as i32;
                        if tree_cur == 0 {
                            self.look_up[t]
                                [(rev_code as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)] =
                                tree_next as i16;
                            tree_cur = tree_next;
                            tree_next -= 2;
                        }

                        let mut rv = rev_code >> (TINFL_FAST_LOOKUP_BITS - 1);
                        let mut j = code_size;
                        while j > TINFL_FAST_LOOKUP_BITS + 1 {
                            rv >>= 1;
                            tree_cur -= (rv & 1) as i32;
                            let idx = (-tree_cur - 1) as usize;
                            if self.get_tree(t as u32, idx) == 0 {
                                self.set_tree(t as u32, idx, tree_next as i16);
                                tree_cur = tree_next;
                                tree_next -= 2;
                            } else {
                                tree_cur = self.get_tree(t as u32, idx) as i32;
                            }
                            j -= 1;
                        }
                        rv >>= 1;
                        tree_cur -= (rv & 1) as i32;
                        let idx = (-tree_cur - 1) as usize;
                        self.set_tree(t as u32, idx, sym_index as i16);
                    }

                    // If we just built tree 2 (code-length tree), decode length codes
                    if t == 2 {
                        self.state = 16; // decode length codes
                    } else if self.m_type > 0 {
                        self.m_type -= 1;
                        self.state = 15; // build next tree
                    } else {
                        // All trees built, start decoding symbols
                        self.state = 23;
                    }
                }

                // ---- State 16: Decode length/distance code sizes using tree 2
                16 => {
                    counter = self.counter;
                    let total_codes = self.table_sizes[0] + self.table_sizes[1];
                    while counter < total_codes {
                        // TINFL_HUFF_DECODE(16, dist, look_up[2], tree_2)
                        let sym = match self.huff_decode_inline(
                            2,
                            in_buf,
                            &mut in_pos,
                            in_buf_end,
                            &mut bit_buf,
                            &mut num_bits,
                            decomp_flags,
                        ) {
                            Ok(s) => s,
                            Err(st) => {
                                self.counter = counter;
                                self.state = 16;
                                status = st;
                                break 'state_machine;
                            }
                        };

                        if sym < 16 {
                            self.len_codes[counter as usize] = sym as u8;
                            counter += 1;
                            continue;
                        }
                        if sym == 16 && counter == 0 {
                            status = TinflStatus::Failed;
                            self.state = 36;
                            break 'state_machine;
                        }

                        // num_extra bits and base repeat count
                        let (extra_bits, base_count): (u32, u32) = match sym {
                            16 => (2, 3),
                            17 => (3, 3),
                            _ /* 18 */ => (7, 11),
                        };

                        // TINFL_GET_BITS(18, s, num_extra)
                        while num_bits < extra_bits {
                            if in_pos >= in_buf_end {
                                // Can't suspend mid-decode easily; save and retry
                                self.counter = counter;
                                self.state = 16;
                                dist = sym;
                                num_extra = extra_bits;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            in_pos += 1;
                            num_bits += 8;
                        }
                        let s = ((bit_buf as u32) & ((1u32 << extra_bits) - 1)) + base_count;
                        bit_buf >>= extra_bits;
                        num_bits -= extra_bits;

                        let fill_val = if sym == 16 {
                            self.len_codes[(counter - 1) as usize]
                        } else {
                            0
                        };
                        let end = core::cmp::min((counter + s) as usize, self.len_codes.len());
                        for idx in (counter as usize)..end {
                            self.len_codes[idx] = fill_val;
                        }
                        counter += s;
                    }
                    self.counter = counter;

                    if total_codes != counter {
                        status = TinflStatus::Failed;
                        self.state = 36;
                        break 'state_machine;
                    }

                    // Copy decoded code sizes into code_size_0 and code_size_1
                    let ts0 = self.table_sizes[0] as usize;
                    let ts1 = self.table_sizes[1] as usize;
                    self.code_size_0[..ts0].copy_from_slice(&self.len_codes[..ts0]);
                    self.code_size_1[..ts1].copy_from_slice(&self.len_codes[ts0..ts0 + ts1]);

                    // Now build trees 1 and 0 (m_type was 2, tree 2 already built)
                    self.m_type = 1;
                    self.state = 15;
                }

                // ---- State 23: Decode literals/lengths from Huffman stream (slow path)
                23 => {
                    // Check if we have enough input/output for fast path
                    if (in_buf_end - in_pos) >= 4 && (out_buf_len - out_cur) >= 2 {
                        self.state = 25; // use fast decode path
                    } else {
                        // Slow path: decode one symbol
                        let sym = match self.huff_decode_inline(
                            0,
                            in_buf,
                            &mut in_pos,
                            in_buf_end,
                            &mut bit_buf,
                            &mut num_bits,
                            decomp_flags,
                        ) {
                            Ok(s) => s,
                            Err(st) => {
                                self.state = 23;
                                status = st;
                                break 'state_machine;
                            }
                        };

                        if sym >= 256 {
                            counter = sym;
                            self.state = 30; // length/distance decode
                        } else {
                            // Literal byte
                            if out_cur >= out_buf_len {
                                // Need to output this byte but buffer is full
                                counter = sym;
                                self.state = 24;
                                status = TinflStatus::HasMoreOutput;
                                break 'state_machine;
                            }
                            output[out_cur] = sym as u8;
                            out_cur += 1;
                            // Stay in state 23 for next symbol
                        }
                    }
                }

                // ---- State 24: Resume after output full during literal write
                24 => {
                    if out_cur >= out_buf_len {
                        status = TinflStatus::HasMoreOutput;
                        break 'state_machine;
                    }
                    output[out_cur] = counter as u8;
                    out_cur += 1;
                    self.state = 23;
                }

                // ---- State 25: Fast decode path (enough input + output)
                25 => {
                    // Ensure we have >=30 bits (we use 64-bit bit buffer)
                    if num_bits < 30 {
                        if in_pos + 3 < in_buf_end {
                            bit_buf |= (read_le32(in_buf, in_pos) as u64) << num_bits;
                            in_pos += 4;
                            num_bits += 32;
                        } else {
                            // Fall back to slow path
                            self.state = 23;
                            continue 'state_machine;
                        }
                    }

                    // Decode first symbol using table 0
                    let mut temp =
                        self.look_up[0][(bit_buf as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)] as i32;
                    let code_len;
                    if temp >= 0 {
                        code_len = (temp >> 9) as u32;
                        temp &= 511;
                    } else {
                        let mut cl = TINFL_FAST_LOOKUP_BITS;
                        loop {
                            temp = self.tree_0[(!temp as usize) + (((bit_buf >> cl) & 1) as usize)]
                                as i32;
                            cl += 1;
                            if temp >= 0 {
                                break;
                            }
                        }
                        code_len = cl;
                    }
                    counter = temp as u32;
                    bit_buf >>= code_len;
                    num_bits -= code_len;

                    if code_len == 0 {
                        status = TinflStatus::Failed;
                        self.state = 36;
                        break 'state_machine;
                    }

                    if (counter & 256) != 0 {
                        // End of block or length code
                        self.state = 30;
                        continue 'state_machine;
                    }

                    // It's a literal. Try to decode a second symbol.
                    // Refill bits if needed (for 32-bit path, always refill; for 64-bit, skip)
                    if num_bits < 15 {
                        if in_pos + 1 < in_buf_end {
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            bit_buf |= (in_buf[in_pos + 1] as u64) << (num_bits + 8);
                            in_pos += 2;
                            num_bits += 16;
                        } else {
                            // Output first literal, fall to slow path
                            if out_cur >= out_buf_len {
                                self.state = 24;
                                status = TinflStatus::HasMoreOutput;
                                break 'state_machine;
                            }
                            output[out_cur] = counter as u8;
                            out_cur += 1;
                            self.state = 23;
                            continue 'state_machine;
                        }
                    }

                    let mut temp2 =
                        self.look_up[0][(bit_buf as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)] as i32;
                    let code_len2;
                    if temp2 >= 0 {
                        code_len2 = (temp2 >> 9) as u32;
                        temp2 &= 511;
                    } else {
                        let mut cl = TINFL_FAST_LOOKUP_BITS;
                        loop {
                            temp2 = self.tree_0
                                [(!temp2 as usize) + (((bit_buf >> cl) & 1) as usize)]
                                as i32;
                            cl += 1;
                            if temp2 >= 0 {
                                break;
                            }
                        }
                        code_len2 = cl;
                    }
                    bit_buf >>= code_len2;
                    num_bits -= code_len2;

                    if code_len2 == 0 {
                        status = TinflStatus::Failed;
                        self.state = 36;
                        break 'state_machine;
                    }

                    // Write first literal
                    if out_cur < out_buf_len {
                        output[out_cur] = counter as u8;
                    }

                    if (temp2 & 256) != 0 {
                        // Second symbol is end-of-block or length code
                        out_cur += 1;
                        counter = temp2 as u32;
                        self.state = 30;
                        continue 'state_machine;
                    }

                    // Both are literals
                    if out_cur + 1 < out_buf_len {
                        output[out_cur + 1] = temp2 as u8;
                    }
                    out_cur += 2;

                    // Check if we can continue fast path
                    if (in_buf_end - in_pos) >= 4 && (out_buf_len - out_cur) >= 2 {
                        self.state = 25;
                    } else {
                        self.state = 23;
                    }
                }

                // ---- State 30: Process length code (counter has the symbol, >= 256)
                30 => {
                    counter &= 511;
                    if counter == 256 {
                        // End of block
                        if (self.m_final & 1) != 0 {
                            self.state = 32; // done with all blocks
                        } else {
                            self.state = 3; // next block
                        }
                        continue 'state_machine;
                    }

                    // Length code: counter is 257..285
                    let idx = (counter - 257) as usize;
                    if idx >= LENGTH_EXTRA.len() {
                        status = TinflStatus::Failed;
                        self.state = 36;
                        break 'state_machine;
                    }
                    num_extra = LENGTH_EXTRA[idx] as u32;
                    counter = LENGTH_BASE[idx] as u32;

                    if num_extra > 0 {
                        // TINFL_GET_BITS(25, extra_bits, num_extra)
                        while num_bits < num_extra {
                            if in_pos >= in_buf_end {
                                self.state = 30;
                                // Save partial state: counter has base length, num_extra has bits needed
                                // We use state 31 as the resume point for extra length bits
                                self.state = 31;
                                self.counter = counter;
                                self.num_extra = num_extra;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            in_pos += 1;
                            num_bits += 8;
                        }
                        let extra = (bit_buf as u32) & ((1u32 << num_extra) - 1);
                        bit_buf >>= num_extra;
                        num_bits -= num_extra;
                        counter += extra;
                    }
                    self.state = 26; // decode distance
                }

                // ---- State 31: Resume getting extra length bits
                31 => {
                    counter = self.counter;
                    num_extra = self.num_extra;
                    while num_bits < num_extra {
                        if in_pos >= in_buf_end {
                            self.state = 31;
                            self.counter = counter;
                            self.num_extra = num_extra;
                            status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                TinflStatus::NeedsMoreInput
                            } else {
                                TinflStatus::FailedCannotMakeProgress
                            };
                            break 'state_machine;
                        }
                        bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                        in_pos += 1;
                        num_bits += 8;
                    }
                    let extra = (bit_buf as u32) & ((1u32 << num_extra) - 1);
                    bit_buf >>= num_extra;
                    num_bits -= num_extra;
                    counter += extra;
                    self.state = 26;
                }

                // ---- State 26: Decode distance code
                26 => {
                    // TINFL_HUFF_DECODE(26, dist, look_up[1], tree_1)
                    let d_sym = match self.huff_decode_inline(
                        1,
                        in_buf,
                        &mut in_pos,
                        in_buf_end,
                        &mut bit_buf,
                        &mut num_bits,
                        decomp_flags,
                    ) {
                        Ok(s) => s,
                        Err(st) => {
                            self.state = 26;
                            self.counter = counter;
                            status = st;
                            break 'state_machine;
                        }
                    };

                    if d_sym as usize >= DIST_EXTRA.len() {
                        status = TinflStatus::Failed;
                        self.state = 36;
                        break 'state_machine;
                    }

                    num_extra = DIST_EXTRA[d_sym as usize] as u32;
                    dist = DIST_BASE[d_sym as usize] as u32;

                    if num_extra > 0 {
                        // TINFL_GET_BITS(27, extra_bits, num_extra)
                        while num_bits < num_extra {
                            if in_pos >= in_buf_end {
                                self.state = 27;
                                self.counter = counter;
                                self.dist = dist;
                                self.num_extra = num_extra;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            in_pos += 1;
                            num_bits += 8;
                        }
                        let extra = (bit_buf as u32) & ((1u32 << num_extra) - 1);
                        bit_buf >>= num_extra;
                        num_bits -= num_extra;
                        dist += extra;
                    }

                    self.state = 37; // validate and copy match
                }

                // ---- State 27: Resume getting extra distance bits
                27 => {
                    counter = self.counter;
                    dist = self.dist;
                    num_extra = self.num_extra;
                    while num_bits < num_extra {
                        if in_pos >= in_buf_end {
                            self.state = 27;
                            self.counter = counter;
                            self.dist = dist;
                            self.num_extra = num_extra;
                            status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                TinflStatus::NeedsMoreInput
                            } else {
                                TinflStatus::FailedCannotMakeProgress
                            };
                            break 'state_machine;
                        }
                        bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                        in_pos += 1;
                        num_bits += 8;
                    }
                    let extra = (bit_buf as u32) & ((1u32 << num_extra) - 1);
                    bit_buf >>= num_extra;
                    num_bits -= num_extra;
                    dist += extra;
                    self.state = 37;
                }

                // ---- State 37: Validate distance and copy match bytes
                37 => {
                    dist_from_out_buf_start = out_cur;

                    if (decomp_flags & TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) != 0 {
                        if dist == 0
                            || dist as usize > dist_from_out_buf_start
                            || dist_from_out_buf_start == 0
                        {
                            status = TinflStatus::Failed;
                            self.state = 36;
                            break 'state_machine;
                        }
                    }

                    let src_pos =
                        (dist_from_out_buf_start.wrapping_sub(dist as usize)) & out_buf_size_mask;

                    // Check if we might exceed output buffer
                    let max_pos = core::cmp::max(out_cur, src_pos);
                    if max_pos + (counter as usize) > out_buf_len {
                        // Slow byte-by-byte copy with output-full handling
                        self.state = 53;
                    } else {
                        // Fast copy (all fits in output buffer)
                        self.state = 40; // fast LZ copy
                    }
                }

                // ---- State 40: Fast LZ77 match copy
                40 => {
                    let mut src_idx =
                        (out_cur.wrapping_sub(dist as usize)) & out_buf_size_mask;

                    // Copy counter bytes from src_idx to out_cur, one byte at a time
                    // (must handle overlapping copies for RLE patterns)
                    let mut remaining = counter as usize;
                    while remaining > 0 {
                        output[out_cur] = output[src_idx];
                        out_cur += 1;
                        src_idx = (src_idx + 1) & out_buf_size_mask;
                        remaining -= 1;
                    }

                    // Back to decode next symbol
                    if (in_buf_end - in_pos) >= 4 && (out_buf_len - out_cur) >= 2 {
                        self.state = 25;
                    } else {
                        self.state = 23;
                    }
                }

                // ---- State 53: Slow LZ77 match copy (may need to suspend for output)
                53 => {
                    while counter > 0 {
                        if out_cur >= out_buf_len {
                            self.state = 53;
                            self.counter = counter;
                            self.dist = dist;
                            self.dist_from_out_buf_start = dist_from_out_buf_start;
                            status = TinflStatus::HasMoreOutput;
                            break 'state_machine;
                        }
                        let src_idx =
                            (dist_from_out_buf_start.wrapping_sub(dist as usize)) & out_buf_size_mask;
                        output[out_cur] = output[src_idx];
                        out_cur += 1;
                        dist_from_out_buf_start += 1;
                        counter -= 1;
                    }

                    // Back to decode next symbol
                    if (in_buf_end - in_pos) >= 4 && (out_buf_len - out_cur) >= 2 {
                        self.state = 25;
                    } else {
                        self.state = 23;
                    }
                }

                // ---- State 32: Post-block cleanup — byte-align and put back bytes
                32 => {
                    // TINFL_SKIP_BITS(32, num_bits & 7)
                    let skip = num_bits & 7;
                    if skip > 0 {
                        while num_bits < skip {
                            if in_pos >= in_buf_end {
                                self.state = 32;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                            in_pos += 1;
                            num_bits += 8;
                        }
                        bit_buf >>= skip;
                        num_bits -= skip;
                    }

                    // Put back bytes we've read ahead
                    while in_pos > 0 && num_bits >= 8 {
                        in_pos -= 1;
                        num_bits -= 8;
                    }
                    bit_buf &= if num_bits > 0 {
                        (1u64 << num_bits) - 1
                    } else {
                        0
                    };

                    if (decomp_flags & TINFL_FLAG_PARSE_ZLIB_HEADER) != 0 {
                        self.z_adler32 = 0;
                        counter = 0;
                        self.counter = 0;
                        self.state = 41;
                    } else {
                        self.state = 34; // done forever
                        status = TinflStatus::Done;
                        break 'state_machine;
                    }
                }

                // ---- State 41: Read zlib adler32 trailer (4 bytes)
                41 => {
                    counter = self.counter;
                    while counter < 4 {
                        if num_bits > 0 {
                            while num_bits < 8 {
                                if in_pos >= in_buf_end {
                                    self.state = 41;
                                    self.counter = counter;
                                    status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                        TinflStatus::NeedsMoreInput
                                    } else {
                                        TinflStatus::FailedCannotMakeProgress
                                    };
                                    break 'state_machine;
                                }
                                bit_buf |= (in_buf[in_pos] as u64) << num_bits;
                                in_pos += 1;
                                num_bits += 8;
                            }
                            let s = (bit_buf & 0xFF) as u32;
                            bit_buf >>= 8;
                            num_bits -= 8;
                            self.z_adler32 = (self.z_adler32 << 8) | s;
                        } else {
                            if in_pos >= in_buf_end {
                                self.state = 41;
                                self.counter = counter;
                                status = if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                    TinflStatus::NeedsMoreInput
                                } else {
                                    TinflStatus::FailedCannotMakeProgress
                                };
                                break 'state_machine;
                            }
                            let s = in_buf[in_pos] as u32;
                            in_pos += 1;
                            self.z_adler32 = (self.z_adler32 << 8) | s;
                        }
                        counter += 1;
                    }
                    self.counter = counter;
                    self.state = 34;
                    status = TinflStatus::Done;
                    break 'state_machine;
                }

                // ---- State 34: Done forever
                34 => {
                    status = TinflStatus::Done;
                    break 'state_machine;
                }

                // ---- State 36: Failed forever
                36 => {
                    status = TinflStatus::Failed;
                    break 'state_machine;
                }

                // Unknown state
                _ => {
                    status = TinflStatus::Failed;
                    break 'state_machine;
                }
            }
        } // end 'state_machine loop

        // ---- common_exit: save state, compute adler32 if needed ----

        // Put back bytes from bit buffer (if not needing more input)
        if status != TinflStatus::NeedsMoreInput
            && status != TinflStatus::FailedCannotMakeProgress
        {
            while in_pos > 0 && num_bits >= 8 {
                in_pos -= 1;
                num_bits -= 8;
            }
        }

        self.num_bits = num_bits;
        self.bit_buf = if num_bits > 0 {
            bit_buf & ((1u64 << num_bits) - 1)
        } else {
            0
        };
        self.dist = dist;
        self.counter = counter;
        self.num_extra = num_extra;
        self.dist_from_out_buf_start = dist_from_out_buf_start;

        let in_consumed = in_pos;
        let out_written = out_cur - out_start;

        // Compute Adler-32 if requested
        if (decomp_flags & (TINFL_FLAG_PARSE_ZLIB_HEADER | TINFL_FLAG_COMPUTE_ADLER32)) != 0
            && (status == TinflStatus::Done
                || status == TinflStatus::HasMoreOutput
                || status == TinflStatus::NeedsMoreInput)
        {
            let data = &output[out_start..out_start + out_written];
            let mut s1 = self.check_adler32 & 0xFFFF;
            let mut s2 = self.check_adler32 >> 16;
            let mut remaining = data.len();
            let mut offset = 0;
            while remaining > 0 {
                let block_len = core::cmp::min(remaining, 5552);
                let mut i = 0;
                while i + 7 < block_len {
                    s1 = s1.wrapping_add(data[offset + i] as u32);
                    s2 = s2.wrapping_add(s1);
                    s1 = s1.wrapping_add(data[offset + i + 1] as u32);
                    s2 = s2.wrapping_add(s1);
                    s1 = s1.wrapping_add(data[offset + i + 2] as u32);
                    s2 = s2.wrapping_add(s1);
                    s1 = s1.wrapping_add(data[offset + i + 3] as u32);
                    s2 = s2.wrapping_add(s1);
                    s1 = s1.wrapping_add(data[offset + i + 4] as u32);
                    s2 = s2.wrapping_add(s1);
                    s1 = s1.wrapping_add(data[offset + i + 5] as u32);
                    s2 = s2.wrapping_add(s1);
                    s1 = s1.wrapping_add(data[offset + i + 6] as u32);
                    s2 = s2.wrapping_add(s1);
                    s1 = s1.wrapping_add(data[offset + i + 7] as u32);
                    s2 = s2.wrapping_add(s1);
                    i += 8;
                }
                while i < block_len {
                    s1 = s1.wrapping_add(data[offset + i] as u32);
                    s2 = s2.wrapping_add(s1);
                    i += 1;
                }
                s1 %= 65521;
                s2 %= 65521;
                remaining -= block_len;
                offset += block_len;
            }
            self.check_adler32 = (s2 << 16) + s1;
            if status == TinflStatus::Done
                && (decomp_flags & TINFL_FLAG_PARSE_ZLIB_HEADER) != 0
                && self.check_adler32 != self.z_adler32
            {
                status = TinflStatus::Adler32Mismatch;
            }
        }

        (in_consumed, out_written, status)
    }

    /// Inline Huffman decode for a given table index (0, 1, or 2).
    /// Returns the decoded symbol, or an error status if input is exhausted.
    fn huff_decode_inline(
        &mut self,
        table: usize,
        in_buf: &[u8],
        in_pos: &mut usize,
        in_buf_end: usize,
        bit_buf: &mut u64,
        num_bits: &mut u32,
        decomp_flags: u32,
    ) -> Result<u32, TinflStatus> {
        // Ensure we have at least 15 bits
        if *num_bits < 15 {
            if in_buf_end.saturating_sub(*in_pos) < 2 {
                // Slow fill path
                self.huff_bitbuf_fill_inline(
                    table, in_buf, in_pos, in_buf_end, bit_buf, num_bits, decomp_flags,
                )?;
            } else {
                *bit_buf |= (in_buf[*in_pos] as u64) << *num_bits;
                *bit_buf |= (in_buf[*in_pos + 1] as u64) << (*num_bits + 8);
                *in_pos += 2;
                *num_bits += 16;
            }
        }

        let mut temp =
            self.look_up[table][(*bit_buf as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)] as i32;
        let code_len;
        let sym;
        if temp >= 0 {
            code_len = (temp >> 9) as u32;
            sym = (temp & 511) as u32;
        } else {
            let mut cl = TINFL_FAST_LOOKUP_BITS;
            loop {
                let tree_val = match table {
                    0 => self.tree_0[(!temp as usize) + (((*bit_buf >> cl) & 1) as usize)],
                    1 => self.tree_1[(!temp as usize) + (((*bit_buf >> cl) & 1) as usize)],
                    _ => self.tree_2[(!temp as usize) + (((*bit_buf >> cl) & 1) as usize)],
                };
                temp = tree_val as i32;
                cl += 1;
                if temp >= 0 {
                    break;
                }
            }
            code_len = cl;
            sym = temp as u32;
        }

        *bit_buf >>= code_len;
        *num_bits -= code_len;
        Ok(sym)
    }

    /// Slow Huffman bit-buffer fill (used when < 2 bytes remain in input).
    fn huff_bitbuf_fill_inline(
        &self,
        table: usize,
        in_buf: &[u8],
        in_pos: &mut usize,
        in_buf_end: usize,
        bit_buf: &mut u64,
        num_bits: &mut u32,
        decomp_flags: u32,
    ) -> Result<(), TinflStatus> {
        loop {
            let temp =
                self.look_up[table][(*bit_buf as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)] as i32;
            if temp >= 0 {
                let code_len = (temp >> 9) as u32;
                if code_len > 0 && *num_bits >= code_len {
                    break;
                }
            } else if *num_bits > TINFL_FAST_LOOKUP_BITS {
                let mut cl = TINFL_FAST_LOOKUP_BITS;
                let mut t = temp;
                loop {
                    let tree_val = match table {
                        0 => self.tree_0[(!t as usize) + (((*bit_buf >> cl) & 1) as usize)],
                        1 => self.tree_1[(!t as usize) + (((*bit_buf >> cl) & 1) as usize)],
                        _ => self.tree_2[(!t as usize) + (((*bit_buf >> cl) & 1) as usize)],
                    };
                    t = tree_val as i32;
                    cl += 1;
                    if t >= 0 || *num_bits < cl + 1 {
                        break;
                    }
                }
                if t >= 0 {
                    break;
                }
            }

            if *in_pos >= in_buf_end {
                return if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                    Err(TinflStatus::NeedsMoreInput)
                } else {
                    Err(TinflStatus::FailedCannotMakeProgress)
                };
            }
            *bit_buf |= (in_buf[*in_pos] as u64) << *num_bits;
            *in_pos += 1;
            *num_bits += 8;

            if *num_bits >= 15 {
                break;
            }
        }
        Ok(())
    }
}

/// Read a little-endian u32 from a byte slice at the given offset.
fn read_le32(buf: &[u8], pos: usize) -> u32 {
    (buf[pos] as u32)
        | ((buf[pos + 1] as u32) << 8)
        | ((buf[pos + 2] as u32) << 16)
        | ((buf[pos + 3] as u32) << 24)
}

// ---- High-level helper functions ----

/// Decompress a deflate/zlib buffer entirely in memory.
///
/// Returns the decompressed bytes, or `None` on failure.
pub fn decompress_to_vec(src: &[u8], flags: u32) -> Option<Vec<u8>> {
    let mut decomp = TinflDecompressor::new();
    let mut out = Vec::new();
    let mut src_ofs = 0;
    let actual_flags =
        (flags & !TINFL_FLAG_HAS_MORE_INPUT) | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;

    loop {
        let remaining_in = &src[src_ofs..];
        // Grow output buffer
        let old_len = out.len();
        let new_capacity = if out.capacity() < 128 { 128 } else { out.capacity() * 2 };
        out.reserve(new_capacity - old_len);
        // Extend the vec so we can write into it
        let write_start = old_len;
        out.resize(new_capacity, 0);

        let (in_used, out_used, status) =
            decomp.decompress(remaining_in, &mut out, write_start, actual_flags);

        src_ofs += in_used;
        let actual_end = write_start + out_used;
        out.truncate(actual_end);

        match status {
            TinflStatus::Done => return Some(out),
            TinflStatus::HasMoreOutput => {
                // Need more output space, loop will grow buffer
                continue;
            }
            _ => return None,
        }
    }
}

/// Decompress from `src` into a fixed-size `dst` buffer.
///
/// Returns the number of decompressed bytes, or `None` on failure.
pub fn decompress_to_slice(src: &[u8], dst: &mut [u8], flags: u32) -> Option<usize> {
    let mut decomp = TinflDecompressor::new();
    let actual_flags =
        (flags & !TINFL_FLAG_HAS_MORE_INPUT) | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    let (_, out_used, status) = decomp.decompress(src, dst, 0, actual_flags);
    if status == TinflStatus::Done {
        Some(out_used)
    } else {
        None
    }
}

/// Decompress using a dictionary-sized (32 KB) buffer, calling `callback` for each
/// chunk of decompressed data.
///
/// Returns `true` on success, `false` on failure.
pub fn decompress_with_callback<F>(src: &[u8], mut callback: F, flags: u32) -> bool
where
    F: FnMut(&[u8]) -> bool,
{
    let mut decomp = TinflDecompressor::new();
    let mut dict = vec![0u8; TINFL_LZ_DICT_SIZE];
    let mut src_ofs = 0;
    let mut dict_ofs = 0;
    let actual_flags = flags & !(TINFL_FLAG_HAS_MORE_INPUT | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF);

    loop {
        let remaining = &src[src_ofs..];
        let (in_used, out_used, status) =
            decomp.decompress(remaining, &mut dict, dict_ofs, actual_flags);
        src_ofs += in_used;
        if out_used > 0 && !callback(&dict[dict_ofs..dict_ofs + out_used]) {
            return false;
        }
        match status {
            TinflStatus::Done => return true,
            TinflStatus::HasMoreOutput => {
                dict_ofs = (dict_ofs + out_used) & (TINFL_LZ_DICT_SIZE - 1);
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_decompressor() {
        let d = TinflDecompressor::new();
        assert_eq!(d.state, 0);
        assert_eq!(d.check_adler32, 1);
        assert_eq!(d.z_adler32, 1);
    }

    #[test]
    fn test_init_resets_state() {
        let mut d = TinflDecompressor::new();
        d.state = 42;
        d.init();
        assert_eq!(d.state, 0);
    }

    #[test]
    fn test_empty_input_needs_more() {
        let mut d = TinflDecompressor::new();
        let input: &[u8] = &[];
        let mut output = [0u8; 1024];
        let (in_used, out_used, status) =
            d.decompress(input, &mut output, 0, TINFL_FLAG_HAS_MORE_INPUT | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF);
        assert_eq!(in_used, 0);
        assert_eq!(out_used, 0);
        assert_eq!(status, TinflStatus::NeedsMoreInput);
    }

    #[test]
    fn test_uncompressed_block() {
        // Build a raw deflate uncompressed block:
        // bfinal=1, btype=00 → byte 0x01
        // LEN=5, NLEN=0xFFFA (complement)
        // Data: "hello"
        let mut input: Vec<u8> = Vec::new();
        input.push(0x01); // final=1, type=0 (bits: 1 00 = 0x01)
        input.push(0x05); // LEN low
        input.push(0x00); // LEN high
        input.push(0xFA); // NLEN low
        input.push(0xFF); // NLEN high
        input.extend_from_slice(b"hello");

        let mut d = TinflDecompressor::new();
        let mut output = [0u8; 1024];
        let (in_used, out_used, status) = d.decompress(
            &input,
            &mut output,
            0,
            TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        assert_eq!(status, TinflStatus::Done);
        assert_eq!(out_used, 5);
        assert_eq!(&output[..5], b"hello");
        assert_eq!(in_used, 10);
    }

    #[test]
    fn test_zlib_wrapped_empty() {
        // A valid zlib stream that decompresses to nothing:
        // CMF=0x78, FLG=0x01 (no dict, compression level 0)
        // then a final empty uncompressed block
        // Adler-32 of empty data = 0x00000001
        let mut input: Vec<u8> = Vec::new();
        input.push(0x78); // CMF: CM=8 (deflate), CINFO=7 (32K window)
        input.push(0x01); // FLG: (0x78*256+0x01) % 31 = 0
        // Final uncompressed block: bfinal=1, btype=00 → 0x01
        input.push(0x01);
        // LEN=0, NLEN=0xFFFF
        input.push(0x00);
        input.push(0x00);
        input.push(0xFF);
        input.push(0xFF);
        // Adler-32 big-endian: 0x00 0x00 0x00 0x01
        input.push(0x00);
        input.push(0x00);
        input.push(0x00);
        input.push(0x01);

        let mut d = TinflDecompressor::new();
        let mut output = [0u8; 1024];
        let (_, out_used, status) = d.decompress(
            &input,
            &mut output,
            0,
            TINFL_FLAG_PARSE_ZLIB_HEADER | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        assert_eq!(status, TinflStatus::Done);
        assert_eq!(out_used, 0);
    }

    #[test]
    fn test_static_huffman_block() {
        // Use a known zlib-compressed "hello" to test static Huffman decode.
        // This is the zlib encoding of "hello" produced by standard zlib at level 1.
        // We can construct the raw deflate stream manually:
        //
        // Actually, let's use a known compressed payload.
        // zlib compress of b"hello": 78 01 cb 48 cd c9 c9 07 00 06 2c 02 15
        // (CMF=0x78, FLG=0x01, deflate data, adler32=0x062c0215)
        let input: &[u8] = &[
            0x78, 0x01, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
        ];

        let mut d = TinflDecompressor::new();
        let mut output = [0u8; 1024];
        let (_, out_used, status) = d.decompress(
            input,
            &mut output,
            0,
            TINFL_FLAG_PARSE_ZLIB_HEADER | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        assert_eq!(status, TinflStatus::Done, "expected Done, got {:?}", status);
        assert_eq!(out_used, 5);
        assert_eq!(&output[..5], b"hello");
    }

    #[test]
    fn test_decompress_to_vec() {
        // zlib of "hello"
        let input: &[u8] = &[
            0x78, 0x01, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
        ];
        let result = decompress_to_vec(input, TINFL_FLAG_PARSE_ZLIB_HEADER);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), b"hello");
    }

    #[test]
    fn test_bad_zlib_header() {
        let input: &[u8] = &[0xFF, 0xFF];
        let mut d = TinflDecompressor::new();
        let mut output = [0u8; 64];
        let (_, _, status) = d.decompress(
            input,
            &mut output,
            0,
            TINFL_FLAG_PARSE_ZLIB_HEADER | TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        assert_eq!(status, TinflStatus::Failed);
    }

    #[test]
    fn test_block_type_3_fails() {
        // bfinal=0, btype=11 → bits 0 11 = 0x06
        let input: &[u8] = &[0x06];
        let mut d = TinflDecompressor::new();
        let mut output = [0u8; 64];
        let (_, _, status) = d.decompress(
            input,
            &mut output,
            0,
            TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );
        assert_eq!(status, TinflStatus::Failed);
    }

    #[test]
    fn test_repeated_data_with_match() {
        // zlib compress of "aaaaaa" (6 bytes of 'a')
        // This will use a length/distance match (LZ77 back-reference)
        // zlib of "aaaaaa": 78 01 4a 4c 44 02 00 08 09 02 01
        let input: &[u8] = &[
            0x78, 0x01, 0x4a, 0x4c, 0x44, 0x02, 0x00, 0x08, 0x09, 0x02, 0x01,
        ];
        let result = decompress_to_vec(input, TINFL_FLAG_PARSE_ZLIB_HEADER);
        assert!(result.is_some());
        let out = result.unwrap();
        assert_eq!(&out, b"aaaaaa");
    }
}
