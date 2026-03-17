use std::cmp::min;

// Constants from miniz.h
const TINFL_FAST_LOOKUP_SIZE: usize = 1 << 10; // 1024
const TINFL_FAST_LOOKUP_BITS: usize = 10;
const TINFL_LZ_DICT_SIZE: usize = 32768;

// Status codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TinflStatus {
    Failed = -1,
    BadParam = -2,
    Adler32Mismatch = -3,
    NeedsMoreInput = -4,
    HasMoreOutput = -5,
    Done = 1,
}

// Decompressor flags
pub const TINFL_FLAG_PARSE_ZLIB_HEADER: u32 = 1;
pub const TINFL_FLAG_HAS_MORE_INPUT: u32 = 2;
pub const TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF: u32 = 4;
pub const TINFL_FLAG_COMPUTE_ADLER32: u32 = 8;

// Decompressor state
#[derive(Debug, Clone)]
pub struct TinflDecompressor {
    // State machine state
    state: usize,

    // Bit buffer state
    num_bits: u32,
    bit_buf: u64,

    // Decoding state
    dist: u32,
    counter: u32,
    num_extra: u32,
    dist_from_out_buf_start: usize,

    // Zlib header
    zhdr0: u8,
    zhdr1: u8,
    z_adler32: u32,
    check_adler32: u32,

    // Block type
    m_type: u32,
    m_final: u32,

    // Raw header for type 0 blocks
    raw_header: [u8; 4],

    // Table sizes
    table_sizes: [u32; 3],

    // Code sizes for each tree
    code_size_0: [u8; 288],
    code_size_1: [u8; 32],
    code_size_2: [u8; 19],

    // Huffman trees
    tree_0: [i16; 576],
    tree_1: [i16; 128],
    tree_2: [i16; 76],

    // Lookup tables
    look_up_0: [i16; TINFL_FAST_LOOKUP_SIZE],
    look_up_1: [i16; TINFL_FAST_LOOKUP_SIZE],
    look_up_2: [i16; TINFL_FAST_LOOKUP_SIZE],

    // Temporary buffer for length codes
    len_codes: [u8; 320],
}

impl TinflDecompressor {
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
            code_size_0: [0; 288],
            code_size_1: [0; 32],
            code_size_2: [0; 19],
            tree_0: [0; 576],
            tree_1: [0; 128],
            tree_2: [0; 76],
            look_up_0: [0; TINFL_FAST_LOOKUP_SIZE],
            look_up_1: [0; TINFL_FAST_LOOKUP_SIZE],
            look_up_2: [0; TINFL_FAST_LOOKUP_SIZE],
            len_codes: [0; 320],
        }
    }

    fn clear_tree(&mut self, tree_type: u32) {
        match tree_type {
            0 => self.tree_0.fill(0),
            1 => self.tree_1.fill(0),
            2 => self.tree_2.fill(0),
            _ => (),
        }
    }

    // Fix: added `input: &[u8]` parameter; replaced pointer arithmetic with slice index
    fn get_byte(&mut self, state_index: usize, input: &[u8], p_in_buf_cur: &mut usize, p_in_buf_end: usize,
                decomp_flags: u32) -> Result<u8, TinflStatus> {
        if *p_in_buf_cur >= p_in_buf_end {
            self.state = state_index;
            return if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                Err(TinflStatus::NeedsMoreInput)
            } else {
                Err(TinflStatus::Failed)
            };
        }

        let c = input[*p_in_buf_cur];
        *p_in_buf_cur += 1;
        Ok(c)
    }

    // Fix: added `input: &[u8]` parameter to thread through to get_byte
    fn need_bits(&mut self, state_index: usize, n: u32, input: &[u8], p_in_buf_cur: &mut usize,
                 p_in_buf_end: usize, decomp_flags: u32) -> Result<(), TinflStatus> {
        while self.num_bits < n {
            let c = self.get_byte(state_index, input, p_in_buf_cur, p_in_buf_end, decomp_flags)?;
            self.bit_buf |= (c as u64) << self.num_bits;
            self.num_bits += 8;
        }
        Ok(())
    }

    // Fix: added `input: &[u8]` parameter to thread through to need_bits
    fn skip_bits(&mut self, state_index: usize, n: u32, input: &[u8], p_in_buf_cur: &mut usize,
                 p_in_buf_end: usize, decomp_flags: u32) -> Result<(), TinflStatus> {
        if self.num_bits < n {
            self.need_bits(state_index, n, input, p_in_buf_cur, p_in_buf_end, decomp_flags)?;
        }
        self.bit_buf >>= n;
        self.num_bits -= n;
        Ok(())
    }

    // Fix: added `input: &[u8]` parameter to thread through to need_bits
    fn get_bits(&mut self, state_index: usize, n: u32, input: &[u8], p_in_buf_cur: &mut usize,
                p_in_buf_end: usize, decomp_flags: u32) -> Result<u32, TinflStatus> {
        if self.num_bits < n {
            self.need_bits(state_index, n, input, p_in_buf_cur, p_in_buf_end, decomp_flags)?;
        }
        let b = (self.bit_buf as u32) & ((1 << n) - 1);
        self.bit_buf >>= n;
        self.num_bits -= n;
        Ok(b)
    }

    // Fix: added `input: &[u8]` parameter to thread through to get_byte
    fn huff_bitbuf_fill(&mut self, state_index: usize, p_look_up: &[i16], p_tree: &[i16],
                        input: &[u8], p_in_buf_cur: &mut usize, p_in_buf_end: usize,
                        decomp_flags: u32) -> Result<(), TinflStatus> {
        loop {
            let temp = p_look_up[(self.bit_buf as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)];
            if temp >= 0 {
                let code_len = (temp >> 9) as u32;
                if code_len > 0 && self.num_bits >= code_len {
                    break;
                }
            } else if self.num_bits > TINFL_FAST_LOOKUP_BITS as u32 {
                let mut code_len = TINFL_FAST_LOOKUP_BITS as u32;
                let mut temp = temp;
                while temp < 0 && self.num_bits >= code_len + 1 {
                    temp = p_tree[(!temp as usize) + (((self.bit_buf >> code_len) & 1) as usize)];
                    code_len += 1;
                }
                if temp >= 0 {
                    break;
                }
            }

            let c = self.get_byte(state_index, input, p_in_buf_cur, p_in_buf_end, decomp_flags)?;
            self.bit_buf |= (c as u64) << self.num_bits;
            self.num_bits += 8;
        }
        Ok(())
    }

    // Fix: added `input: &[u8]` parameter; replaced pointer arithmetic with slice indexing
    fn huff_decode(&mut self, state_index: usize, p_look_up: &[i16], p_tree: &[i16],
                   input: &[u8], p_in_buf_cur: &mut usize, p_in_buf_end: usize,
                   decomp_flags: u32) -> Result<u32, TinflStatus> {
        if self.num_bits < 15 {
            if p_in_buf_end - *p_in_buf_cur < 2 {
                self.huff_bitbuf_fill(state_index, p_look_up, p_tree, input, p_in_buf_cur,
                                      p_in_buf_end, decomp_flags)?;
            } else {
                // Fix: replaced p_in_buf_cur.as_ptr().wrapping_add(*p_in_buf_cur) with slice indexing
                let b0 = input[*p_in_buf_cur] as u64;
                let b1 = input[*p_in_buf_cur + 1] as u64;
                self.bit_buf |= (b0 << self.num_bits) | (b1 << (self.num_bits + 8));
                *p_in_buf_cur += 2;
                self.num_bits += 16;
            }
        }

        let mut temp = p_look_up[(self.bit_buf as usize) & (TINFL_FAST_LOOKUP_SIZE - 1)];
        let (code_len, sym) = if temp >= 0 {
            ((temp >> 9) as u32, (temp & 511) as u32)
        } else {
            let mut code_len = TINFL_FAST_LOOKUP_BITS as u32;
            while temp < 0 {
                temp = p_tree[(!temp as usize) + (((self.bit_buf >> code_len) & 1) as usize)];
                code_len += 1;
            }
            (code_len, temp as u32)
        };

        self.bit_buf >>= code_len;
        self.num_bits -= code_len;
        Ok(sym)
    }

    pub fn decompress(&mut self, p_in_buf_next: &[u8], p_out_buf_start: &mut [u8],
                      decomp_flags: u32) -> Result<(usize, usize, TinflStatus), TinflStatus> {
        // Static tables
        static LENGTH_BASE: [u16; 31] = [
            3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59,
            67, 83, 99, 115, 131, 163, 195, 227, 258, 0, 0
        ];
        static LENGTH_EXTRA: [u8; 31] = [
            0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4,
            5, 5, 5, 5, 0, 0, 0
        ];
        static DIST_BASE: [u16; 32] = [
            1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385,
            513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
            0, 0
        ];
        static DIST_EXTRA: [u8; 32] = [
            0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10,
            10, 11, 11, 12, 12, 13, 13, 0, 0
        ];
        static LENGTH_DEZIGZAG: [u8; 19] = [
            16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15
        ];
        static MIN_TABLE_SIZES: [u16; 3] = [257, 1, 4];

        let mut p_in_buf_cur = 0;
        let p_in_buf_end = p_in_buf_next.len();
        let mut p_out_buf_cur = 0;
        let p_out_buf_end = p_out_buf_start.len();

        let out_buf_size_mask = if (decomp_flags & TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) != 0 {
            usize::MAX
        } else {
            p_out_buf_end - 1
        };

        // Check output buffer size is power of 2
        if ((out_buf_size_mask + 1) & out_buf_size_mask) != 0 || p_out_buf_end == 0 {
            return Err(TinflStatus::BadParam);
        }

        // State machine
        match self.state {
            0 => {
                self.bit_buf = 0;
                self.num_bits = 0;
                self.dist = 0;
                self.counter = 0;
                self.num_extra = 0;
                self.z_adler32 = 1;
                self.check_adler32 = 1;

                if (decomp_flags & TINFL_FLAG_PARSE_ZLIB_HEADER) != 0 {
                    self.state = 1;
                } else {
                    self.state = 3;
                }
            }
            1 => {
                self.zhdr0 = self.get_byte(1, p_in_buf_next, &mut p_in_buf_cur, p_in_buf_end, decomp_flags)?;
                self.state = 2;
            }
            2 => {
                self.zhdr1 = self.get_byte(2, p_in_buf_next, &mut p_in_buf_cur, p_in_buf_end, decomp_flags)?;
                let mut counter = ((self.zhdr0 as u32 * 256 + self.zhdr1 as u32) % 31 != 0) ||
                                 (self.zhdr1 & 32 != 0) ||
                                 ((self.zhdr0 & 15) != 8);
                if (decomp_flags & TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF) == 0 {
                    let window_size = 1 << (8 + (self.zhdr0 >> 4));
                    counter |= (window_size > 32768) || (out_buf_size_mask + 1 < window_size as usize);
                }
                if counter {
                    return Err(TinflStatus::Failed);
                }
                self.state = 3;
            }
            _ => {}
        }

        // Main decompression loop
        loop {
            if self.state == 3 {
                self.m_final = self.get_bits(3, 3, p_in_buf_next, &mut p_in_buf_cur, p_in_buf_end, decomp_flags)?;
                self.m_type = self.m_final >> 1;

                if self.m_type == 0 {
                    // Uncompressed block
                    self.skip_bits(5, self.num_bits & 7, p_in_buf_next, &mut p_in_buf_cur, p_in_buf_end, decomp_flags)?;

                    for i in 0..4 {
                        if self.num_bits > 0 {
                            self.raw_header[i] = self.get_bits(6, 8, p_in_buf_next, &mut p_in_buf_cur, p_in_buf_end, decomp_flags)? as u8;
                        } else {
                            self.raw_header[i] = self.get_byte(7, p_in_buf_next, &mut p_in_buf_cur, p_in_buf_end, decomp_flags)?;
                        }
                    }

                    let len = (self.raw_header[0] as u16) | ((self.raw_header[1] as u16) << 8);
                    let nlen = (self.raw_header[2] as u16) | ((self.raw_header[3] as u16) << 8);

                    if len != !nlen {
                        return Err(TinflStatus::Failed);
                    }

                    self.counter = len as u32;

                    while self.counter > 0 && self.num_bits > 0 {
                        let dist = self.get_bits(51, 8, p_in_buf_next, &mut p_in_buf_cur, p_in_buf_end, decomp_flags)? as u8;
                        if p_out_buf_cur >= p_out_buf_end {
                            self.state = 52;
                            return Ok((p_in_buf_cur, p_out_buf_cur, TinflStatus::HasMoreOutput));
                        }
                        p_out_buf_start[p_out_buf_cur] = dist;
                        p_out_buf_cur += 1;
                        self.counter -= 1;
                    }

                    while self.counter > 0 {
                        if p_out_buf_cur >= p_out_buf_end {
                            self.state = 9;
                            return Ok((p_in_buf_cur, p_out_buf_cur, TinflStatus::HasMoreOutput));
                        }
                        if p_in_buf_cur >= p_in_buf_end {
                            self.state = 38;
                            return if (decomp_flags & TINFL_FLAG_HAS_MORE_INPUT) != 0 {
                                Err(TinflStatus::NeedsMoreInput)
                            } else {
                                Err(TinflStatus::Failed)
                            };
                        }
                        // TODO: complete uncompressed block copy loop
                        p_out_buf_start[p_out_buf_cur] = p_in_buf_next[p_in_buf_cur];
                        p_out_buf_cur += 1;
                        p_in_buf_cur += 1;
                        self.counter -= 1;
                    }
                } // end if m_type == 0
            } // end if state == 3

            // TODO: implement remaining decompression states (compressed blocks, Huffman decode, LZ77 copy)
            break;
        } // end loop

        Ok((p_in_buf_cur, p_out_buf_cur, TinflStatus::Done))
    }
}
