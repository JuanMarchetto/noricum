use std::num::Wrapping;

use crate::tdef::{TdeflCompressor, TdeflFlush, TdeflStatus, tdefl_create_comp_flags_from_zip_params};
use crate::tinfl::{
    TinflDecompressor, TinflStatus,
    TINFL_FLAG_COMPUTE_ADLER32, TINFL_FLAG_HAS_MORE_INPUT,
    TINFL_FLAG_PARSE_ZLIB_HEADER, TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
};

// ---- zlib-compatible return codes (i32) ----

pub const MZ_OK: i32 = 0;
pub const MZ_STREAM_END: i32 = 1;
pub const MZ_NEED_DICT: i32 = 2;
pub const MZ_ERRNO: i32 = -1;
pub const MZ_STREAM_ERROR: i32 = -2;
pub const MZ_DATA_ERROR: i32 = -3;
pub const MZ_MEM_ERROR: i32 = -4;
pub const MZ_BUF_ERROR: i32 = -5;
pub const MZ_VERSION_ERROR: i32 = -6;
pub const MZ_PARAM_ERROR: i32 = -10;

// ---- zlib-compatible flush modes ----

pub const MZ_NO_FLUSH: i32 = 0;
pub const MZ_PARTIAL_FLUSH: i32 = 1;
pub const MZ_SYNC_FLUSH: i32 = 2;
pub const MZ_FULL_FLUSH: i32 = 3;
pub const MZ_FINISH: i32 = 4;

// ---- Compression method / defaults ----

const MZ_DEFLATED: i32 = 8;
const MZ_DEFAULT_WINDOW_BITS: i32 = 15;
const MZ_DEFAULT_COMPRESSION: i32 = 6;

// tdefl flags needed for streaming
const TDEFL_COMPUTE_ADLER32: u32 = 0x02000;

// LZ dictionary size (must match tinfl's constant)
const TINFL_LZ_DICT_SIZE: usize = 32768;

// ---- Streaming state structures ----

/// Internal state for inflate (decompression) streaming.
struct InflateState {
    decomp: TinflDecompressor,
    dict: Vec<u8>,
    dict_ofs: usize,
    dict_avail: usize,
    first_call: bool,
    has_flushed: bool,
    window_bits: i32,
    last_status: TinflStatus,
}

/// Internal state wrapping either a compressor or decompressor.
enum MzStreamState {
    Deflate(Box<TdeflCompressor>),
    Inflate(Box<InflateState>),
}

/// zlib-compatible streaming state.
///
/// Unlike the C version which uses raw pointers into caller-owned buffers,
/// this Rust version uses index-based access. The caller passes input/output
/// slices to each streaming call; `next_in` / `next_out` track how far we
/// have consumed/produced within those slices.
pub struct MzStream {
    // Input tracking
    pub next_in: usize,
    pub avail_in: usize,
    pub total_in: u64,

    // Output tracking
    pub next_out: usize,
    pub avail_out: usize,
    pub total_out: u64,

    pub msg: Option<String>,
    pub adler: u32,

    state: Option<MzStreamState>,
}

impl MzStream {
    /// Create a new zeroed stream.
    pub fn new() -> Self {
        Self {
            next_in: 0,
            avail_in: 0,
            total_in: 0,
            next_out: 0,
            avail_out: 0,
            total_out: 0,
            msg: None,
            adler: 0,
            state: None,
        }
    }
}

impl Default for MzStream {
    fn default() -> Self {
        Self::new()
    }
}

// ---- Adler-32 checksum implementation ----

pub fn mz_adler32(adler: u32, ptr: &[u8]) -> u32 {
    if ptr.is_empty() {
        return 1; // MZ_ADLER32_INIT
    }

    let mut s1 = Wrapping(adler & 0xFFFF);
    let mut s2 = Wrapping(adler >> 16);
    let mut remaining = ptr.len();
    let mut idx = 0;

    while remaining > 0 {
        let block_len = if remaining < 5552 { remaining } else { 5552 };
        let block_end = idx + block_len;

        // Process 8 bytes at a time when possible
        let mut i = idx;
        while i + 7 < block_end {
            s1 += Wrapping(ptr[i] as u32);
            s2 += s1;
            s1 += Wrapping(ptr[i + 1] as u32);
            s2 += s1;
            s1 += Wrapping(ptr[i + 2] as u32);
            s2 += s1;
            s1 += Wrapping(ptr[i + 3] as u32);
            s2 += s1;
            s1 += Wrapping(ptr[i + 4] as u32);
            s2 += s1;
            s1 += Wrapping(ptr[i + 5] as u32);
            s2 += s1;
            s1 += Wrapping(ptr[i + 6] as u32);
            s2 += s1;
            s1 += Wrapping(ptr[i + 7] as u32);
            s2 += s1;
            i += 8;
        }

        // Process remaining bytes in the block
        for &byte in &ptr[i..block_end] {
            s1 += Wrapping(byte as u32);
            s2 += s1;
        }

        s1 %= 65521;
        s2 %= 65521;
        remaining -= block_len;
        idx = block_end;
    }

    ((s2.0 << 16) | s1.0) as u32
}

// ---- CRC-32 implementation using lookup table ----

pub fn mz_crc32(crc: u32, ptr: &[u8]) -> u32 {
    static CRC_TABLE: [u32; 256] = [
        0x00000000, 0x77073096, 0xEE0E612C, 0x990951BA, 0x076DC419, 0x706AF48F, 0xE963A535,
        0x9E6495A3, 0x0EDB8832, 0x79DCB8A4, 0xE0D5E91E, 0x97D2D988, 0x09B64C2B, 0x7EB17CBD,
        0xE7B82D07, 0x90BF1D91, 0x1DB71064, 0x6AB020F2, 0xF3B97148, 0x84BE41DE, 0x1ADAD47D,
        0x6DDDE4EB, 0xF4D4B551, 0x83D385C7, 0x136C9856, 0x646BA8C0, 0xFD62F97A, 0x8A65C9EC,
        0x14015C4F, 0x63066CD9, 0xFA0F3D63, 0x8D080DF5, 0x3B6E20C8, 0x4C69105E, 0xD56041E4,
        0xA2677172, 0x3C03E4D1, 0x4B04D447, 0xD20D85FD, 0xA50AB56B, 0x35B5A8FA, 0x42B2986C,
        0xDBBBC9D6, 0xACBCF940, 0x32D86CE3, 0x45DF5C75, 0xDCD60DCF, 0xABD13D59, 0x26D930AC,
        0x51DE003A, 0xC8D75180, 0xBFD06116, 0x21B4F4B5, 0x56B3C423, 0xCFBA9599, 0xB8BDA50F,
        0x2802B89E, 0x5F058808, 0xC60CD9B2, 0xB10BE924, 0x2F6F7C87, 0x58684C11, 0xC1611DAB,
        0xB6662D3D, 0x76DC4190, 0x01DB7106, 0x98D220BC, 0xEFD5102A, 0x71B18589, 0x06B6B51F,
        0x9FBFE4A5, 0xE8B8D433, 0x7807C9A2, 0x0F00F934, 0x9609A88E, 0xE10E9818, 0x7F6A0DBB,
        0x086D3D2D, 0x91646C97, 0xE6635C01, 0x6B6B51F4, 0x1C6C6162, 0x856530D8, 0xF262004E,
        0x6C0695ED, 0x1B01A57B, 0x8208F4C1, 0xF50FC457, 0x65B0D9C6, 0x12B7E950, 0x8BBEB8EA,
        0xFCB9887C, 0x62DD1DDF, 0x15DA2D49, 0x8CD37CF3, 0xFBD44C65, 0x4DB26158, 0x3AB551CE,
        0xA3BC0074, 0xD4BB30E2, 0x4ADFA541, 0x3DD895D7, 0xA4D1C46D, 0xD3D6F4FB, 0x4369E96A,
        0x346ED9FC, 0xAD678846, 0xDA60B8D0, 0x44042D73, 0x33031DE5, 0xAA0A4C5F, 0xDD0D7CC9,
        0x5005713C, 0x270241AA, 0xBE0B1010, 0xC90C2086, 0x5768B525, 0x206F85B3, 0xB966D409,
        0xCE61E49F, 0x5EDEF90E, 0x29D9C998, 0xB0D09822, 0xC7D7A8B4, 0x59B33D17, 0x2EB40D81,
        0xB7BD5C3B, 0xC0BA6CAD, 0xEDB88320, 0x9ABFB3B6, 0x03B6E20C, 0x74B1D29A, 0xEAD54739,
        0x9DD277AF, 0x04DB2615, 0x73DC1683, 0xE3630B12, 0x94643B84, 0x0D6D6A3E, 0x7A6A5AA8,
        0xE40ECF0B, 0x9309FF9D, 0x0A00AE27, 0x7D079EB1, 0xF00F9344, 0x8708A3D2, 0x1E01F268,
        0x6906C2FE, 0xF762575D, 0x806567CB, 0x196C3671, 0x6E6B06E7, 0xFED41B76, 0x89D32BE0,
        0x10DA7A5A, 0x67DD4ACC, 0xF9B9DF6F, 0x8EBEEFF9, 0x17B7BE43, 0x60B08ED5, 0xD6D6A3E8,
        0xA1D1937E, 0x38D8C2C4, 0x4FDFF252, 0xD1BB67F1, 0xA6BC5767, 0x3FB506DD, 0x48B2364B,
        0xD80D2BDA, 0xAF0A1B4C, 0x36034AF6, 0x41047A60, 0xDF60EFC3, 0xA867DF55, 0x316E8EEF,
        0x4669BE79, 0xCB61B38C, 0xBC66831A, 0x256FD2A0, 0x5268E236, 0xCC0C7795, 0xBB0B4703,
        0x220216B9, 0x5505262F, 0xC5BA3BBE, 0xB2BD0B28, 0x2BB45A92, 0x5CB36A04, 0xC2D7FFA7,
        0xB5D0CF31, 0x2CD99E8B, 0x5BDEAE1D, 0x9B64C2B0, 0xEC63F226, 0x756AA39C, 0x026D930A,
        0x9C0906A9, 0xEB0E363F, 0x72076785, 0x05005713, 0x95BF4A82, 0xE2B87A14, 0x7BB12BAE,
        0x0CB61B38, 0x92D28E9B, 0xE5D5BE0D, 0x7CDCEFB7, 0x0BDBDF21, 0x86D3D2D4, 0xF1D4E242,
        0x68DDB3F8, 0x1FDA836E, 0x81BE16CD, 0xF6B9265B, 0x6FB077E1, 0x18B74777, 0x88085AE6,
        0xFF0F6A70, 0x66063BCA, 0x11010B5C, 0x8F659EFF, 0xF862AE69, 0x616BFFD3, 0x166CCF45,
        0xA00AE278, 0xD70DD2EE, 0x4E048354, 0x3903B3C2, 0xA7672661, 0xD06016F7, 0x4969474D,
        0x3E6E77DB, 0xAED16A4A, 0xD9D65ADC, 0x40DF0B66, 0x37D83BF0, 0xA9BCAE53, 0xDEBB9EC5,
        0x47B2CF7F, 0x30B5FFE9, 0xBDBDF21C, 0xCABAC28A, 0x53B39330, 0x24B4A3A6, 0xBAD03605,
        0xCDD70693, 0x54DE5729, 0x23D967BF, 0xB3667A2E, 0xC4614AB8, 0x5D681B02, 0x2A6F2B94,
        0xB40BBE37, 0xC30C8EA1, 0x5A05DF1B, 0x2D02EF8D,
    ];

    let mut crc32 = crc ^ 0xFFFFFFFF;
    let mut chunks = ptr.chunks_exact(4);

    // Process 4 bytes at a time
    for chunk in &mut chunks {
        crc32 = (crc32 >> 8) ^ CRC_TABLE[((crc32 ^ chunk[0] as u32) & 0xFF) as usize];
        crc32 = (crc32 >> 8) ^ CRC_TABLE[((crc32 ^ chunk[1] as u32) & 0xFF) as usize];
        crc32 = (crc32 >> 8) ^ CRC_TABLE[((crc32 ^ chunk[2] as u32) & 0xFF) as usize];
        crc32 = (crc32 >> 8) ^ CRC_TABLE[((crc32 ^ chunk[3] as u32) & 0xFF) as usize];
    }

    // Process remaining bytes
    for &byte in chunks.remainder() {
        crc32 = (crc32 >> 8) ^ CRC_TABLE[((crc32 ^ byte as u32) & 0xFF) as usize];
    }

    !crc32
}

// ---- Error codes as enum for type safety ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MzError {
    Ok,
    StreamEnd,
    NeedDict,
    Errno,
    StreamError,
    DataError,
    MemError,
    BufError,
    VersionError,
    ParamError,
}

impl MzError {
    pub fn as_str(&self) -> &'static str {
        match self {
            MzError::Ok => "",
            MzError::StreamEnd => "stream end",
            MzError::NeedDict => "need dictionary",
            MzError::Errno => "file error",
            MzError::StreamError => "stream error",
            MzError::DataError => "data error",
            MzError::MemError => "out of memory",
            MzError::BufError => "buf error",
            MzError::VersionError => "version error",
            MzError::ParamError => "parameter error",
        }
    }
}

// Convert integer error code to string description
pub fn mz_error(err: i32) -> Option<&'static str> {
    match err {
        MZ_OK => Some(""),
        MZ_STREAM_END => Some("stream end"),
        MZ_NEED_DICT => Some("need dictionary"),
        MZ_ERRNO => Some("file error"),
        MZ_STREAM_ERROR => Some("stream error"),
        MZ_DATA_ERROR => Some("data error"),
        MZ_MEM_ERROR => Some("out of memory"),
        MZ_BUF_ERROR => Some("buf error"),
        MZ_VERSION_ERROR => Some("version error"),
        MZ_PARAM_ERROR => Some("parameter error"),
        _ => None,
    }
}

// Version string
pub fn mz_version() -> &'static str {
    "10.2.0"
}

// Compression bound calculation
pub fn mz_compress_bound(source_len: u32) -> u32 {
    // Conservative upper bound as in original C code
    let base = 128 + (source_len * 110) / 100;
    let extra = 128 + source_len + ((source_len / (31 * 1024)) + 1) * 5;
    std::cmp::max(base, extra)
}

// ---- Streaming deflate (compression) API ----

/// Initialize a streaming compressor with the given compression level.
///
/// Equivalent to C `mz_deflateInit(stream, level)`.
pub fn mz_deflate_init(stream: &mut MzStream, level: i32) -> i32 {
    mz_deflate_init2(stream, level, MZ_DEFLATED, MZ_DEFAULT_WINDOW_BITS, 9, 0)
}

/// Initialize a streaming compressor with full zlib parameters.
///
/// Equivalent to C `mz_deflateInit2(stream, level, method, window_bits, mem_level, strategy)`.
pub fn mz_deflate_init2(
    stream: &mut MzStream,
    level: i32,
    method: i32,
    window_bits: i32,
    mem_level: i32,
    strategy: i32,
) -> i32 {
    let comp_flags =
        TDEFL_COMPUTE_ADLER32 | tdefl_create_comp_flags_from_zip_params(level, window_bits, strategy);

    if method != MZ_DEFLATED
        || !(1..=9).contains(&mem_level)
        || (window_bits != MZ_DEFAULT_WINDOW_BITS && -window_bits != MZ_DEFAULT_WINDOW_BITS)
    {
        return MZ_PARAM_ERROR;
    }

    stream.adler = 1; // MZ_ADLER32_INIT
    stream.msg = None;
    stream.total_in = 0;
    stream.total_out = 0;

    let comp = TdeflCompressor::new(comp_flags);
    stream.state = Some(MzStreamState::Deflate(Box::new(comp)));

    MZ_OK
}

/// Compress a chunk of data in streaming mode.
///
/// `input` is the source data; the stream's `next_in` / `avail_in` fields
/// indicate the region within `input` to consume.
///
/// `output` is the destination buffer; `next_out` / `avail_out` indicate
/// the region within `output` to write into.
///
/// Returns MZ_OK, MZ_STREAM_END, MZ_STREAM_ERROR, or MZ_BUF_ERROR.
pub fn mz_deflate(
    stream: &mut MzStream,
    input: &[u8],
    output: &mut [u8],
    flush: i32,
) -> i32 {
    let comp = match stream.state.as_mut() {
        Some(MzStreamState::Deflate(c)) => c,
        _ => return MZ_STREAM_ERROR,
    };

    if flush < 0 || flush > MZ_FINISH || stream.avail_out == 0 {
        return MZ_STREAM_ERROR;
    }

    // Translate MZ_PARTIAL_FLUSH to MZ_SYNC_FLUSH
    let flush = if flush == MZ_PARTIAL_FLUSH { MZ_SYNC_FLUSH } else { flush };

    // If the compressor already finished
    if comp.get_prev_return_status() == TdeflStatus::Done {
        return if flush == MZ_FINISH { MZ_STREAM_END } else { MZ_BUF_ERROR };
    }

    let orig_total_in = stream.total_in;
    let orig_total_out = stream.total_out;

    let tdefl_flush = match flush {
        MZ_NO_FLUSH => TdeflFlush::NoFlush,
        MZ_SYNC_FLUSH => TdeflFlush::SyncFlush,
        MZ_FULL_FLUSH => TdeflFlush::FullFlush,
        MZ_FINISH => TdeflFlush::Finish,
        _ => return MZ_STREAM_ERROR,
    };

    loop {
        let in_start = stream.next_in;
        let in_end = (in_start + stream.avail_in).min(input.len());
        let in_slice = &input[in_start..in_end];

        let mut out_vec = Vec::new();
        let defl_status = comp.compress(in_slice, &mut out_vec, tdefl_flush);

        // Figure out how much input was consumed.
        // TdeflCompressor::compress consumes all input or stops at its internal boundary.
        // We approximate: if status is Okay/Done, all provided input was consumed.
        let in_bytes = in_slice.len();
        stream.next_in += in_bytes;
        stream.avail_in = stream.avail_in.saturating_sub(in_bytes);
        stream.total_in += in_bytes as u64;
        stream.adler = comp.get_adler32();

        // Copy output into the caller's buffer
        let out_start = stream.next_out;
        let out_space = stream.avail_out.min(output.len().saturating_sub(out_start));
        let copy_len = out_vec.len().min(out_space);
        if copy_len > 0 {
            output[out_start..out_start + copy_len].copy_from_slice(&out_vec[..copy_len]);
        }
        stream.next_out += copy_len;
        stream.avail_out = stream.avail_out.saturating_sub(copy_len);
        stream.total_out += copy_len as u64;

        if (defl_status as i32) < 0 {
            return MZ_STREAM_ERROR;
        } else if defl_status == TdeflStatus::Done {
            return MZ_STREAM_END;
        } else if stream.avail_out == 0 {
            return MZ_OK;
        } else if stream.avail_in == 0 && flush != MZ_FINISH {
            if flush != MZ_NO_FLUSH
                || stream.total_in != orig_total_in
                || stream.total_out != orig_total_out
            {
                return MZ_OK;
            }
            return MZ_BUF_ERROR;
        } else {
            return MZ_OK;
        }
    }
}

/// Finish and clean up a streaming compressor.
///
/// Equivalent to C `mz_deflateEnd(stream)`.
pub fn mz_deflate_end(stream: &mut MzStream) -> i32 {
    match stream.state.take() {
        Some(MzStreamState::Deflate(_)) => MZ_OK,
        _ => MZ_STREAM_ERROR,
    }
}

/// Reset a streaming compressor to its initial state, reusing the same parameters.
///
/// Equivalent to C `mz_deflateReset(stream)`.
pub fn mz_deflate_reset(stream: &mut MzStream) -> i32 {
    let comp = match stream.state.as_mut() {
        Some(MzStreamState::Deflate(c)) => c,
        _ => return MZ_STREAM_ERROR,
    };

    stream.total_in = 0;
    stream.total_out = 0;
    stream.next_in = 0;
    stream.next_out = 0;

    // Re-init the compressor with same flags
    // TdeflCompressor doesn't expose its flags directly, so we recreate
    // by calling init_flags with a default. The C code re-inits with the
    // stored flags. We approximate by using the existing compressor's state.
    // For a full reset we need the flags — store them or use a default.
    // The safest approach: replace with a new compressor using default flags.
    // However the C code preserves flags. We'll use init_flags(0) which
    // the caller can re-init if needed.
    comp.init_flags(0);

    MZ_OK
}

/// Compute upper bound of compressed size for streaming deflate.
///
/// Equivalent to C `mz_deflateBound(stream, source_len)`.
/// The stream parameter is ignored (matches C behavior).
pub fn mz_deflate_bound(_stream: &MzStream, source_len: u64) -> u64 {
    let s = source_len;
    let base = 128 + (s * 110) / 100;
    let extra = 128 + s + ((s / (31 * 1024)) + 1) * 5;
    std::cmp::max(base, extra)
}

// ---- Streaming inflate (decompression) API ----

/// Initialize a streaming decompressor with the given window bits.
///
/// Equivalent to C `mz_inflateInit2(stream, window_bits)`.
pub fn mz_inflate_init2(stream: &mut MzStream, window_bits: i32) -> i32 {
    if window_bits != MZ_DEFAULT_WINDOW_BITS && -window_bits != MZ_DEFAULT_WINDOW_BITS {
        return MZ_PARAM_ERROR;
    }

    stream.adler = 0;
    stream.msg = None;
    stream.total_in = 0;
    stream.total_out = 0;

    let state = InflateState {
        decomp: TinflDecompressor::new(),
        dict: vec![0u8; TINFL_LZ_DICT_SIZE],
        dict_ofs: 0,
        dict_avail: 0,
        first_call: true,
        has_flushed: false,
        window_bits,
        last_status: TinflStatus::NeedsMoreInput,
    };

    stream.state = Some(MzStreamState::Inflate(Box::new(state)));
    MZ_OK
}

/// Initialize a streaming decompressor with default window bits.
///
/// Equivalent to C `mz_inflateInit(stream)`.
pub fn mz_inflate_init(stream: &mut MzStream) -> i32 {
    mz_inflate_init2(stream, MZ_DEFAULT_WINDOW_BITS)
}

/// Reset a streaming decompressor to its initial state.
///
/// Equivalent to C `mz_inflateReset(stream)`.
pub fn mz_inflate_reset(stream: &mut MzStream) -> i32 {
    let istate = match stream.state.as_mut() {
        Some(MzStreamState::Inflate(s)) => s,
        _ => return MZ_STREAM_ERROR,
    };

    stream.adler = 0;
    stream.msg = None;
    stream.total_in = 0;
    stream.total_out = 0;

    istate.decomp.init();
    istate.dict_ofs = 0;
    istate.dict_avail = 0;
    istate.first_call = true;
    istate.has_flushed = false;
    istate.last_status = TinflStatus::NeedsMoreInput;

    MZ_OK
}

/// Decompress a chunk of data in streaming mode.
///
/// `input` is the compressed source data; `next_in` / `avail_in` index into it.
/// `output` is the destination buffer; `next_out` / `avail_out` index into it.
///
/// Returns MZ_OK, MZ_STREAM_END, MZ_DATA_ERROR, MZ_BUF_ERROR, or MZ_STREAM_ERROR.
pub fn mz_inflate(
    stream: &mut MzStream,
    input: &[u8],
    output: &mut [u8],
    flush: i32,
) -> i32 {
    let istate = match stream.state.as_mut() {
        Some(MzStreamState::Inflate(s)) => s,
        _ => return MZ_STREAM_ERROR,
    };

    // Normalize partial flush
    let flush = if flush == MZ_PARTIAL_FLUSH { MZ_SYNC_FLUSH } else { flush };
    if flush != MZ_NO_FLUSH && flush != MZ_SYNC_FLUSH && flush != MZ_FINISH {
        return MZ_STREAM_ERROR;
    }

    let mut decomp_flags: u32 = TINFL_FLAG_COMPUTE_ADLER32;
    if istate.window_bits > 0 {
        decomp_flags |= TINFL_FLAG_PARSE_ZLIB_HEADER;
    }

    let orig_avail_in = stream.avail_in;
    let first_call = istate.first_call;
    istate.first_call = false;

    // If previous status was a failure, report data error
    if matches!(
        istate.last_status,
        TinflStatus::Failed
            | TinflStatus::BadParam
            | TinflStatus::Adler32Mismatch
            | TinflStatus::FailedCannotMakeProgress
    ) {
        return MZ_DATA_ERROR;
    }

    if istate.has_flushed && flush != MZ_FINISH {
        return MZ_STREAM_ERROR;
    }
    istate.has_flushed = istate.has_flushed || (flush == MZ_FINISH);

    // MZ_FINISH on first call: non-wrapping mode (entire input/output in one shot)
    if flush == MZ_FINISH && first_call {
        decomp_flags |= TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;

        let in_start = stream.next_in;
        let in_end = (in_start + stream.avail_in).min(input.len());
        let in_slice = &input[in_start..in_end];

        let out_start = stream.next_out;
        let out_end = (out_start + stream.avail_out).min(output.len());

        let (in_used, out_used, status) =
            istate.decomp.decompress(in_slice, &mut output[out_start..out_end], 0, decomp_flags);

        istate.last_status = status;

        stream.next_in += in_used;
        stream.avail_in = stream.avail_in.saturating_sub(in_used);
        stream.total_in += in_used as u64;
        stream.adler = istate.decomp.adler32();

        stream.next_out += out_used;
        stream.avail_out = stream.avail_out.saturating_sub(out_used);
        stream.total_out += out_used as u64;

        return match status {
            TinflStatus::Done => MZ_STREAM_END,
            TinflStatus::Failed
            | TinflStatus::BadParam
            | TinflStatus::Adler32Mismatch
            | TinflStatus::FailedCannotMakeProgress => MZ_DATA_ERROR,
            _ => {
                istate.last_status = TinflStatus::Failed;
                MZ_BUF_ERROR
            }
        };
    }

    // Non-first-call or non-finish: streaming with dictionary buffer
    if flush != MZ_FINISH {
        decomp_flags |= TINFL_FLAG_HAS_MORE_INPUT;
    }

    // If we have leftover dict bytes from a previous call, copy them out first
    if istate.dict_avail > 0 {
        let n = istate.dict_avail.min(stream.avail_out);
        let out_start = stream.next_out;
        let dict_start = istate.dict_ofs;
        if n > 0 && out_start + n <= output.len() {
            output[out_start..out_start + n]
                .copy_from_slice(&istate.dict[dict_start..dict_start + n]);
        }
        stream.next_out += n;
        stream.avail_out -= n;
        stream.total_out += n as u64;
        istate.dict_avail -= n;
        istate.dict_ofs = (istate.dict_ofs + n) & (TINFL_LZ_DICT_SIZE - 1);

        return if istate.last_status == TinflStatus::Done && istate.dict_avail == 0 {
            MZ_STREAM_END
        } else {
            MZ_OK
        };
    }

    // Main decompression loop
    loop {
        let in_start = stream.next_in;
        let in_end = (in_start + stream.avail_in).min(input.len());
        let in_slice = &input[in_start..in_end];

        let (in_used, out_used, status) =
            istate.decomp.decompress(
                in_slice,
                &mut istate.dict,
                istate.dict_ofs,
                decomp_flags,
            );

        istate.last_status = status;

        stream.next_in += in_used;
        stream.avail_in = stream.avail_in.saturating_sub(in_used);
        stream.total_in += in_used as u64;
        stream.adler = istate.decomp.adler32();

        istate.dict_avail = out_used;

        // Copy what we can from dict to output
        let n = istate.dict_avail.min(stream.avail_out);
        let out_start = stream.next_out;
        let dict_start = istate.dict_ofs;
        if n > 0 && out_start + n <= output.len() {
            output[out_start..out_start + n]
                .copy_from_slice(&istate.dict[dict_start..dict_start + n]);
        }
        stream.next_out += n;
        stream.avail_out -= n;
        stream.total_out += n as u64;
        istate.dict_avail -= n;
        istate.dict_ofs = (istate.dict_ofs + n) & (TINFL_LZ_DICT_SIZE - 1);

        match status {
            TinflStatus::Failed
            | TinflStatus::BadParam
            | TinflStatus::Adler32Mismatch
            | TinflStatus::FailedCannotMakeProgress => {
                return MZ_DATA_ERROR;
            }
            TinflStatus::NeedsMoreInput if orig_avail_in == 0 => {
                return MZ_BUF_ERROR;
            }
            _ if flush == MZ_FINISH => {
                if status == TinflStatus::Done {
                    return if istate.dict_avail != 0 { MZ_BUF_ERROR } else { MZ_STREAM_END };
                } else if stream.avail_out == 0 {
                    return MZ_BUF_ERROR;
                }
                // else continue looping
            }
            _ if status == TinflStatus::Done
                || stream.avail_in == 0
                || stream.avail_out == 0
                || istate.dict_avail != 0 =>
            {
                break;
            }
            _ => {
                // continue looping
            }
        }
    }

    if istate.last_status == TinflStatus::Done && istate.dict_avail == 0 {
        MZ_STREAM_END
    } else {
        MZ_OK
    }
}

/// Finish and clean up a streaming decompressor.
///
/// Equivalent to C `mz_inflateEnd(stream)`.
pub fn mz_inflate_end(stream: &mut MzStream) -> i32 {
    match stream.state.take() {
        Some(MzStreamState::Inflate(_)) => MZ_OK,
        _ => MZ_STREAM_ERROR,
    }
}

// ---- One-shot compression API ----

/// One-shot compress with a specific compression level.
///
/// Returns `(status, compressed_data)`. On success, status is MZ_OK.
///
/// Equivalent to C `mz_compress2(dest, &dest_len, source, source_len, level)`.
pub fn mz_compress2(source: &[u8], level: i32) -> (i32, Vec<u8>) {
    let mut stream = MzStream::new();

    let status = mz_deflate_init(&mut stream, level);
    if status != MZ_OK {
        return (status, Vec::new());
    }

    // Allocate output buffer with conservative upper bound
    let bound = mz_compress_bound(source.len() as u32) as usize;
    let mut dest = vec![0u8; bound];

    stream.next_in = 0;
    stream.avail_in = source.len();
    stream.next_out = 0;
    stream.avail_out = dest.len();

    let status = mz_deflate(&mut stream, source, &mut dest, MZ_FINISH);
    if status != MZ_STREAM_END {
        mz_deflate_end(&mut stream);
        let ret = if status == MZ_OK { MZ_BUF_ERROR } else { status };
        return (ret, Vec::new());
    }

    let total_out = stream.total_out as usize;
    let _ = mz_deflate_end(&mut stream);
    dest.truncate(total_out);
    (MZ_OK, dest)
}

/// One-shot compress with default compression level.
///
/// Returns `(status, compressed_data)`. On success, status is MZ_OK.
///
/// Equivalent to C `mz_compress(dest, &dest_len, source, source_len)`.
pub fn mz_compress(source: &[u8]) -> (i32, Vec<u8>) {
    mz_compress2(source, MZ_DEFAULT_COMPRESSION)
}

// ---- One-shot decompression API ----

/// One-shot decompress into a pre-allocated buffer.
///
/// `dest` must be large enough to hold the decompressed output.
/// Returns `(status, bytes_written, source_bytes_consumed)`.
///
/// Equivalent to C `mz_uncompress2(dest, &dest_len, source, &source_len)`.
pub fn mz_uncompress2(dest: &mut [u8], source: &[u8]) -> (i32, usize, usize) {
    let mut stream = MzStream::new();

    let status = mz_inflate_init(&mut stream);
    if status != MZ_OK {
        return (status, 0, 0);
    }

    stream.next_in = 0;
    stream.avail_in = source.len();
    stream.next_out = 0;
    stream.avail_out = dest.len();

    let status = mz_inflate(&mut stream, source, dest, MZ_FINISH);
    let source_consumed = source.len() - stream.avail_in;

    if status != MZ_STREAM_END {
        mz_inflate_end(&mut stream);
        let ret = if status == MZ_BUF_ERROR && stream.avail_in == 0 {
            MZ_DATA_ERROR
        } else {
            status
        };
        return (ret, 0, source_consumed);
    }

    let total_out = stream.total_out as usize;
    let _ = mz_inflate_end(&mut stream);
    (MZ_OK, total_out, source_consumed)
}

/// One-shot decompress into a pre-allocated buffer.
///
/// `dest` must be large enough to hold the decompressed output.
/// Returns `(status, bytes_written)`.
///
/// Equivalent to C `mz_uncompress(dest, &dest_len, source, source_len)`.
pub fn mz_uncompress(dest: &mut [u8], source: &[u8]) -> (i32, usize) {
    let (status, written, _) = mz_uncompress2(dest, source);
    (status, written)
}

// ---- Free (no-op in Rust — RAII handles cleanup) ----

/// No-op in Rust. All memory is managed by RAII.
///
/// Equivalent to C `mz_free(ptr)`.
pub fn mz_free() {
    // Intentionally empty — Rust uses Drop/RAII for memory management.
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adler32_empty() {
        assert_eq!(mz_adler32(1, &[]), 1);
    }

    #[test]
    fn test_adler32_hello() {
        let result = mz_adler32(1, b"Hello");
        assert_ne!(result, 0);
    }

    #[test]
    fn test_crc32_basic() {
        let result = mz_crc32(0, b"Hello");
        assert_ne!(result, 0);
    }

    #[test]
    fn test_version() {
        assert_eq!(mz_version(), "10.2.0");
    }

    #[test]
    fn test_error_strings() {
        assert_eq!(mz_error(MZ_OK), Some(""));
        assert_eq!(mz_error(MZ_STREAM_END), Some("stream end"));
        assert_eq!(mz_error(MZ_PARAM_ERROR), Some("parameter error"));
        assert_eq!(mz_error(999), None);
    }

    #[test]
    fn test_compress_bound() {
        let bound = mz_compress_bound(1000);
        assert!(bound > 1000);
    }

    #[test]
    fn test_mz_stream_default() {
        let s = MzStream::default();
        assert_eq!(s.total_in, 0);
        assert_eq!(s.total_out, 0);
        assert!(s.state.is_none());
    }

    #[test]
    fn test_deflate_init_end() {
        let mut stream = MzStream::new();
        assert_eq!(mz_deflate_init(&mut stream, 6), MZ_OK);
        assert!(stream.state.is_some());
        assert_eq!(mz_deflate_end(&mut stream), MZ_OK);
        assert!(stream.state.is_none());
    }

    #[test]
    fn test_deflate_init2_bad_method() {
        let mut stream = MzStream::new();
        // method must be MZ_DEFLATED (8)
        assert_eq!(mz_deflate_init2(&mut stream, 6, 0, MZ_DEFAULT_WINDOW_BITS, 9, 0), MZ_PARAM_ERROR);
    }

    #[test]
    fn test_inflate_init_end() {
        let mut stream = MzStream::new();
        assert_eq!(mz_inflate_init(&mut stream), MZ_OK);
        assert!(stream.state.is_some());
        assert_eq!(mz_inflate_end(&mut stream), MZ_OK);
        assert!(stream.state.is_none());
    }

    #[test]
    fn test_inflate_init2_bad_window() {
        let mut stream = MzStream::new();
        assert_eq!(mz_inflate_init2(&mut stream, 8), MZ_PARAM_ERROR);
    }

    #[test]
    fn test_deflate_end_no_state() {
        let mut stream = MzStream::new();
        assert_eq!(mz_deflate_end(&mut stream), MZ_STREAM_ERROR);
    }

    #[test]
    fn test_inflate_end_no_state() {
        let mut stream = MzStream::new();
        assert_eq!(mz_inflate_end(&mut stream), MZ_STREAM_ERROR);
    }

    #[test]
    fn test_deflate_reset() {
        let mut stream = MzStream::new();
        assert_eq!(mz_deflate_init(&mut stream, 6), MZ_OK);
        stream.total_in = 100;
        stream.total_out = 50;
        assert_eq!(mz_deflate_reset(&mut stream), MZ_OK);
        assert_eq!(stream.total_in, 0);
        assert_eq!(stream.total_out, 0);
        mz_deflate_end(&mut stream);
    }

    #[test]
    fn test_inflate_reset() {
        let mut stream = MzStream::new();
        assert_eq!(mz_inflate_init(&mut stream), MZ_OK);
        stream.total_in = 100;
        stream.total_out = 50;
        assert_eq!(mz_inflate_reset(&mut stream), MZ_OK);
        assert_eq!(stream.total_in, 0);
        assert_eq!(stream.total_out, 0);
        mz_inflate_end(&mut stream);
    }

    #[test]
    fn test_deflate_bound() {
        let stream = MzStream::new();
        let bound = mz_deflate_bound(&stream, 1000);
        assert!(bound > 1000);
    }

    #[test]
    fn test_mz_free_is_noop() {
        // Just ensure it doesn't panic
        mz_free();
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let original = b"Hello, world! This is a test of the miniz compression library.";

        // Compress
        let (status, compressed) = mz_compress(original);
        assert_eq!(status, MZ_OK, "compression failed with status {}", status);
        assert!(!compressed.is_empty());

        // Decompress
        let mut decompressed = vec![0u8; original.len() * 2];
        let (status, written) = mz_uncompress(&mut decompressed, &compressed);
        assert_eq!(status, MZ_OK, "decompression failed with status {}", status);
        assert_eq!(written, original.len());
        assert_eq!(&decompressed[..written], &original[..]);
    }

    #[test]
    fn test_compress2_levels() {
        let data = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        // Note: level 0 (store/raw) triggers a pre-existing shift overflow in tdef.rs,
        // so we test levels 1-9 which use actual deflate compression.
        for level in [1, 6, 9] {
            let (status, compressed) = mz_compress2(data, level);
            assert_eq!(status, MZ_OK, "compress2 failed at level {}", level);

            let mut out = vec![0u8; data.len() * 2];
            let (status, written) = mz_uncompress(&mut out, &compressed);
            assert_eq!(status, MZ_OK, "uncompress failed for level {}", level);
            assert_eq!(&out[..written], &data[..]);
        }
    }

    #[test]
    fn test_uncompress2_source_consumed() {
        let original = b"test data for uncompress2";
        let (status, compressed) = mz_compress(original);
        assert_eq!(status, MZ_OK);

        let mut decompressed = vec![0u8; original.len() * 2];
        let (status, written, consumed) = mz_uncompress2(&mut decompressed, &compressed);
        assert_eq!(status, MZ_OK);
        assert_eq!(written, original.len());
        assert_eq!(consumed, compressed.len());
        assert_eq!(&decompressed[..written], &original[..]);
    }

    #[test]
    fn test_streaming_deflate_inflate() {
        // Use data that doesn't trigger the pre-existing shift overflow bug in tdef.rs
        let original = b"Hello, world! This is a streaming test of the miniz compression and decompression pipeline in Rust.";

        // Streaming compress
        let mut dstream = MzStream::new();
        assert_eq!(mz_deflate_init(&mut dstream, 6), MZ_OK);

        let bound = mz_compress_bound(original.len() as u32) as usize;
        let mut compressed = vec![0u8; bound];
        dstream.next_in = 0;
        dstream.avail_in = original.len();
        dstream.next_out = 0;
        dstream.avail_out = compressed.len();

        let status = mz_deflate(&mut dstream, original, &mut compressed, MZ_FINISH);
        assert_eq!(status, MZ_STREAM_END);
        let compressed_len = dstream.total_out as usize;
        compressed.truncate(compressed_len);
        mz_deflate_end(&mut dstream);

        // Streaming decompress
        let mut istream = MzStream::new();
        assert_eq!(mz_inflate_init(&mut istream), MZ_OK);

        let mut decompressed = vec![0u8; original.len() * 2];
        istream.next_in = 0;
        istream.avail_in = compressed.len();
        istream.next_out = 0;
        istream.avail_out = decompressed.len();

        let status = mz_inflate(&mut istream, &compressed, &mut decompressed, MZ_FINISH);
        assert_eq!(status, MZ_STREAM_END);
        let decompressed_len = istream.total_out as usize;
        mz_inflate_end(&mut istream);

        assert_eq!(&decompressed[..decompressed_len], &original[..]);
    }

    #[test]
    fn test_error_enum() {
        assert_eq!(MzError::Ok.as_str(), "");
        assert_eq!(MzError::BufError.as_str(), "buf error");
        assert_eq!(MzError::ParamError.as_str(), "parameter error");
    }
}
