/*
 * picohttpparser Migrated — idiomatic Rust translation of picohttpparser + tests.
 * Original C by Kazuho Oku et al. (https://github.com/h2o/picohttpparser).
 * Migrated by Noricum (pipeline + human refinement).
 *
 * 0 unsafe blocks, 0 raw pointers, full safe Rust.
 * Compile: rustc picohttpparser_migrated.rs -o test_rs
 */

// ================================================================
// SECTION 1: picotest — minimal TAP test framework
// ================================================================

use std::cell::RefCell;

struct TestState {
    num_tests: i32,
    failed: bool,
}

thread_local! {
    static TEST_STACK: RefCell<Vec<TestState>> = RefCell::new(vec![TestState { num_tests: 0, failed: false }]);
    static TEST_LEVEL: RefCell<i32> = RefCell::new(0);
}

fn indent() {
    TEST_LEVEL.with(|level| {
        let l = *level.borrow();
        for _ in 0..l {
            print!("    ");
        }
    });
}

fn note(msg: &str) {
    indent();
    println!("# {}", msg);
}

fn note_fmt(msg: std::fmt::Arguments) {
    indent();
    println!("# {}", msg);
}

fn ok(cond: bool) {
    TEST_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let state = stack.last_mut().unwrap();
        if !cond {
            state.failed = true;
        }
        state.num_tests += 1;
        indent();
        println!("{} {} - check", if cond { "ok" } else { "not ok" }, state.num_tests);
    });
}

fn done_testing() -> i32 {
    TEST_STACK.with(|stack| {
        let stack = stack.borrow();
        let state = stack.last().unwrap();
        indent();
        println!("1..{}", state.num_tests);
        if state.failed { 1 } else { 0 }
    })
}

fn subtest(name: &str, cb: fn()) {
    TEST_STACK.with(|stack| {
        stack.borrow_mut().push(TestState { num_tests: 0, failed: false });
    });
    TEST_LEVEL.with(|level| *level.borrow_mut() += 1);

    note_fmt(format_args!("Subtest: {}", name));
    cb();
    done_testing();

    TEST_LEVEL.with(|level| *level.borrow_mut() -= 1);

    let child_failed = TEST_STACK.with(|stack| {
        let state = stack.borrow_mut().pop().unwrap();
        state.failed
    });

    TEST_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let parent = stack.last_mut().unwrap();
        if child_failed {
            parent.failed = true;
        }
        parent.num_tests += 1;
        indent();
        println!("{} {} - {}", if !child_failed { "ok" } else { "not ok" }, parent.num_tests, name);
    });
}

// ================================================================
// SECTION 2: picohttpparser — HTTP parser (idiomatic Rust)
// ================================================================

/// A parsed HTTP header (name-value pair).
/// name is None for continuation lines of multiline headers.
#[derive(Debug, Clone)]
struct Header<'a> {
    name: Option<&'a [u8]>,
    value: &'a [u8],
}

/// Parse result: number of bytes consumed, or an error.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ParseResult {
    Complete(usize),
    Incomplete,
    Error,
}

/// Token character map — maps each byte to whether it's a valid HTTP token char.
fn is_token_char(c: u8) -> bool {
    const MAP: [bool; 256] = {
        let mut m = [false; 256];
        // RFC 7230 token chars: !#$%&'*+-.0-9A-Za-z^_`|~
        m[b'!' as usize] = true;
        m[b'#' as usize] = true;
        m[b'$' as usize] = true;
        m[b'%' as usize] = true;
        m[b'&' as usize] = true;
        m[b'\'' as usize] = true;
        m[b'*' as usize] = true;
        m[b'+' as usize] = true;
        m[b'-' as usize] = true;
        m[b'.' as usize] = true;
        let mut i = b'0';
        while i <= b'9' { m[i as usize] = true; i += 1; }
        let mut i = b'A';
        while i <= b'Z' { m[i as usize] = true; i += 1; }
        m[b'^' as usize] = true;
        m[b'_' as usize] = true;
        m[b'`' as usize] = true;
        let mut i = b'a';
        while i <= b'z' { m[i as usize] = true; i += 1; }
        m[b'|' as usize] = true;
        m[b'~' as usize] = true;
        m
    };
    MAP[c as usize]
}

#[inline]
fn is_printable_ascii(c: u8) -> bool {
    c.wrapping_sub(0x20) < 0x5f
}

/// Get token to end of line (CR LF or LF). Returns (token, rest) or error.
fn get_token_to_eol(buf: &[u8]) -> Result<(&[u8], &[u8]), ParseResult> {
    let mut i = 0;
    // scan for control chars
    while i < buf.len() {
        let c = buf[i];
        if !is_printable_ascii(c) {
            if (c < 0x20 && c != b'\t') || c == 0x7f {
                // Found control char
                if c == b'\r' {
                    if i + 1 >= buf.len() {
                        return Err(ParseResult::Incomplete);
                    }
                    if buf[i + 1] != b'\n' {
                        return Err(ParseResult::Error);
                    }
                    let token = &buf[..i];
                    return Ok((token, &buf[i + 2..]));
                } else if c == b'\n' {
                    let token = &buf[..i];
                    return Ok((token, &buf[i + 1..]));
                } else {
                    return Err(ParseResult::Error);
                }
            }
        }
        i += 1;
    }
    Err(ParseResult::Incomplete)
}

/// Check if request/response headers are complete (double CRLF found).
fn is_complete(buf: &[u8], last_len: usize) -> Result<(), ParseResult> {
    let start = if last_len < 3 { 0 } else { last_len - 3 };
    let mut consecutive = 0;
    let mut i = start;
    while i < buf.len() {
        if buf[i] == b'\r' {
            i += 1;
            if i >= buf.len() {
                return Err(ParseResult::Incomplete);
            }
            if buf[i] != b'\n' {
                return Err(ParseResult::Error);
            }
            i += 1;
            consecutive += 1;
        } else if buf[i] == b'\n' {
            i += 1;
            consecutive += 1;
        } else {
            i += 1;
            consecutive = 0;
        }
        if consecutive == 2 {
            return Ok(());
        }
    }
    Err(ParseResult::Incomplete)
}

/// Parse a token until `next_char`. Returns (token, rest after next_char is NOT consumed).
fn parse_token(buf: &[u8], next_char: u8) -> Result<(&[u8], &[u8]), ParseResult> {
    if buf.is_empty() {
        return Err(ParseResult::Incomplete);
    }
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == next_char {
            let token = &buf[..i];
            return Ok((token, &buf[i..]));
        } else if !is_token_char(buf[i]) {
            return Err(ParseResult::Error);
        }
        i += 1;
    }
    Err(ParseResult::Incomplete)
}

/// Advance past spaces to find a token ending at space. Returns (token, rest with space).
fn advance_token(buf: &[u8]) -> Result<(&[u8], &[u8]), ParseResult> {
    if buf.is_empty() {
        return Err(ParseResult::Incomplete);
    }
    let mut i = 0;
    while i < buf.len() {
        let c = buf[i];
        if c == b' ' {
            return Ok((&buf[..i], &buf[i..]));
        } else if !is_printable_ascii(c) {
            if c < 0x20 || c == 0x7f {
                return Err(ParseResult::Error);
            }
            // MSB chars are ok
        }
        i += 1;
    }
    Err(ParseResult::Incomplete)
}

/// Parse HTTP version "HTTP/1.N". Returns (minor_version, rest).
fn parse_http_version(buf: &[u8]) -> Result<(i32, &[u8]), ParseResult> {
    if buf.len() < 9 {
        return Err(ParseResult::Incomplete);
    }
    if &buf[..7] != b"HTTP/1." {
        return Err(ParseResult::Error);
    }
    let digit = buf[7];
    if digit < b'0' || digit > b'9' {
        return Err(ParseResult::Error);
    }
    Ok(((digit - b'0') as i32, &buf[8..]))
}

/// Parse headers. Returns (headers, rest after terminal CRLF, result).
/// Headers are populated even on Incomplete results (matching C behavior).
fn parse_headers_impl<'a>(
    mut buf: &'a [u8],
    max_headers: usize,
) -> (Vec<Header<'a>>, Option<&'a [u8]>, ParseResult) {
    let mut headers = Vec::new();

    loop {
        if buf.is_empty() {
            return (headers, None, ParseResult::Incomplete);
        }

        // Check for end of headers
        if buf[0] == b'\r' {
            if buf.len() < 2 {
                return (headers, None, ParseResult::Incomplete);
            }
            if buf[1] != b'\n' {
                return (headers, None, ParseResult::Error);
            }
            return (headers, Some(&buf[2..]), ParseResult::Complete(0));
        } else if buf[0] == b'\n' {
            return (headers, Some(&buf[1..]), ParseResult::Complete(0));
        }

        if headers.len() == max_headers {
            return (headers, None, ParseResult::Error);
        }

        // Check for continuation line (multiline header)
        if !headers.is_empty() && (buf[0] == b' ' || buf[0] == b'\t') {
            // Continuation line — no name
            match get_token_to_eol(buf) {
                Ok((value, rest)) => {
                    let value = trim_trailing_ws(value);
                    headers.push(Header { name: None, value });
                    buf = rest;
                }
                Err(r) => return (headers, None, r),
            }
        } else {
            // Parse header name
            match parse_token(buf, b':') {
                Ok((name, rest)) => {
                    if name.is_empty() {
                        return (headers, None, ParseResult::Error);
                    }
                    buf = &rest[1..]; // skip ':'

                    // Skip leading whitespace in value
                    while !buf.is_empty() && (buf[0] == b' ' || buf[0] == b'\t') {
                        buf = &buf[1..];
                    }
                    if buf.is_empty() {
                        return (headers, None, ParseResult::Incomplete);
                    }

                    match get_token_to_eol(buf) {
                        Ok((value, rest)) => {
                            let value = trim_trailing_ws(value);
                            headers.push(Header {
                                name: Some(name),
                                value,
                            });
                            buf = rest;
                        }
                        Err(r) => return (headers, None, r),
                    }
                }
                Err(r) => return (headers, None, r),
            }
        }
    }
}

fn trim_trailing_ws(s: &[u8]) -> &[u8] {
    let mut end = s.len();
    while end > 0 && (s[end - 1] == b' ' || s[end - 1] == b'\t') {
        end -= 1;
    }
    &s[..end]
}

/// Parse an HTTP request.
fn phr_parse_request(
    buf: &[u8],
    last_len: usize,
) -> (
    ParseResult,
    Option<&[u8]>,  // method
    Option<&[u8]>,  // path
    i32,            // minor_version
    Vec<Header>,    // headers
) {
    let mut method: Option<&[u8]> = None;
    let mut path: Option<&[u8]> = None;
    let mut minor_version: i32 = -1;
    // Slowloris check
    if last_len != 0 {
        match is_complete(buf, last_len) {
            Ok(()) => {}
            Err(r) => return (r, None, None, -1, Vec::new()),
        }
    }

    let mut rest = buf;

    // Skip leading CRLF (some clients add CRLF after POST body)
    if !rest.is_empty() {
        if rest[0] == b'\r' {
            if rest.len() < 2 {
                return (ParseResult::Incomplete, None, None, -1, Vec::new());
            }
            if rest[1] != b'\n' {
                return (ParseResult::Error, None, None, -1, Vec::new());
            }
            rest = &rest[2..];
        } else if rest[0] == b'\n' {
            rest = &rest[1..];
        }
    } else {
        return (ParseResult::Incomplete, None, None, -1, Vec::new());
    }

    // Parse method
    match parse_token(rest, b' ') {
        Ok((m, r)) => {
            method = Some(m);
            rest = r;
        }
        Err(r) => return (r, None, None, -1, Vec::new()),
    }

    // Skip spaces after method
    loop {
        if rest.is_empty() {
            return (ParseResult::Incomplete, method, None, -1, Vec::new());
        }
        if rest[0] != b' ' {
            break;
        }
        rest = &rest[1..];
    }

    // Parse path (advance token)
    match advance_token(rest) {
        Ok((p, r)) => {
            path = Some(p);
            rest = r;
        }
        Err(r) => return (r, method, None, -1, Vec::new()),
    }

    // Skip spaces after path
    loop {
        if rest.is_empty() {
            return (ParseResult::Incomplete, method, path, -1, Vec::new());
        }
        if rest[0] != b' ' {
            break;
        }
        rest = &rest[1..];
    }

    // Check method/path not empty
    if method.map_or(true, |m| m.is_empty()) || path.map_or(true, |p| p.is_empty()) {
        return (ParseResult::Error, method, path, -1, Vec::new());
    }

    // Parse HTTP version
    match parse_http_version(rest) {
        Ok((v, r)) => {
            minor_version = v;
            rest = r;
        }
        Err(r) => return (r, method, path, minor_version, Vec::new()),
    }

    // Expect CRLF or LF after version
    if rest.is_empty() {
        return (ParseResult::Incomplete, method, path, minor_version, Vec::new());
    }
    if rest[0] == b'\r' {
        if rest.len() < 2 {
            return (ParseResult::Incomplete, method, path, minor_version, Vec::new());
        }
        if rest[1] != b'\n' {
            return (ParseResult::Error, method, path, minor_version, Vec::new());
        }
        rest = &rest[2..];
    } else if rest[0] == b'\n' {
        rest = &rest[1..];
    } else {
        return (ParseResult::Error, method, path, minor_version, Vec::new());
    }

    // Parse headers
    let (h, rest_after, result) = parse_headers_impl(rest, 4);
    match result {
        ParseResult::Complete(_) => {
            let r = rest_after.unwrap();
            let consumed = buf.len() - r.len();
            (ParseResult::Complete(consumed), method, path, minor_version, h)
        }
        r => (r, method, path, minor_version, h),
    }
}

/// Parse an HTTP response.
fn phr_parse_response(
    buf: &[u8],
    last_len: usize,
) -> (
    ParseResult,
    i32,            // minor_version
    i32,            // status
    Option<&[u8]>,  // msg
    Vec<Header>,    // headers
) {
    let mut minor_version: i32 = -1;
    let mut status: i32 = 0;
    let mut msg: Option<&[u8]> = None;

    // Slowloris check
    if last_len != 0 {
        match is_complete(buf, last_len) {
            Ok(()) => {}
            Err(r) => return (r, -1, 0, None, Vec::new()),
        }
    }

    let mut rest = buf;

    // Parse HTTP version
    match parse_http_version(rest) {
        Ok((v, r)) => {
            minor_version = v;
            rest = r;
        }
        Err(r) => return (r, minor_version, 0, None, Vec::new()),
    }

    // Expect space after version
    if rest.is_empty() || rest[0] != b' ' {
        return (ParseResult::Error, minor_version, 0, None, Vec::new());
    }

    // Skip spaces
    loop {
        if rest.is_empty() {
            return (ParseResult::Incomplete, minor_version, 0, None, Vec::new());
        }
        if rest[0] != b' ' {
            break;
        }
        rest = &rest[1..];
    }

    // Parse 3-digit status code
    if rest.len() < 4 {
        return (ParseResult::Incomplete, minor_version, 0, None, Vec::new());
    }
    for i in 0..3 {
        if rest[i] < b'0' || rest[i] > b'9' {
            return (ParseResult::Error, minor_version, status, None, Vec::new());
        }
    }
    status = ((rest[0] - b'0') as i32) * 100
        + ((rest[1] - b'0') as i32) * 10
        + ((rest[2] - b'0') as i32);
    rest = &rest[3..];

    // Get status message to EOL
    match get_token_to_eol(rest) {
        Ok((m, r)) => {
            if m.is_empty() {
                msg = Some(m);
            } else if m[0] == b' ' {
                // Strip leading spaces
                let mut start = 0;
                while start < m.len() && m[start] == b' ' {
                    start += 1;
                }
                msg = Some(&m[start..]);
            } else {
                // Garbage after status code
                return (ParseResult::Error, minor_version, status, None, Vec::new());
            }
            rest = r;
        }
        Err(r) => return (r, minor_version, status, None, Vec::new()),
    }

    // Parse headers
    let (h, rest_after, result) = parse_headers_impl(rest, 4);
    match result {
        ParseResult::Complete(_) => {
            let r = rest_after.unwrap();
            let consumed = buf.len() - r.len();
            (ParseResult::Complete(consumed), minor_version, status, msg, h)
        }
        r => (r, minor_version, status, msg, h),
    }
}

/// Parse headers only.
fn phr_parse_headers_only(buf: &[u8], last_len: usize) -> (ParseResult, Vec<Header>) {
    if last_len != 0 {
        match is_complete(buf, last_len) {
            Ok(()) => {}
            Err(r) => return (r, Vec::new()),
        }
    }
    let (h, rest_after, result) = parse_headers_impl(buf, 4);
    match result {
        ParseResult::Complete(_) => {
            let r = rest_after.unwrap();
            let consumed = buf.len() - r.len();
            (ParseResult::Complete(consumed), h)
        }
        r => (r, h),
    }
}

// ================================================================
// SECTION 2b: Chunked transfer encoding decoder
// ================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
enum ChunkedState {
    ChunkSize,
    ChunkExt,
    ChunkHeaderExpectLf,
    ChunkData,
    ChunkDataExpectCr,
    ChunkDataExpectLf,
    TrailersLineHead,
    TrailersLineMiddle,
}

struct ChunkedDecoder {
    bytes_left_in_chunk: usize,
    consume_trailer: bool,
    hex_count: u8,
    state: ChunkedState,
    total_read: u64,
    total_overhead: u64,
}

impl ChunkedDecoder {
    fn new() -> Self {
        Self {
            bytes_left_in_chunk: 0,
            consume_trailer: false,
            hex_count: 0,
            state: ChunkedState::ChunkSize,
            total_read: 0,
            total_overhead: 0,
        }
    }
}

fn decode_hex(ch: u8) -> Option<usize> {
    match ch {
        b'0'..=b'9' => Some((ch - b'0') as usize),
        b'A'..=b'F' => Some((ch - b'A') as usize + 10),
        b'a'..=b'f' => Some((ch - b'a') as usize + 10),
        _ => None,
    }
}

/// Decode chunked transfer encoding in-place.
/// Returns: (ret, decoded_size).
/// ret: >0 = bytes remaining after chunk end, -2 = incomplete, -1 = error.
/// decoded_size: number of decoded bytes at start of buffer.
/// Undecoded trailing bytes are preserved after decoded_size in the buffer.
fn phr_decode_chunked(decoder: &mut ChunkedDecoder, buf: &mut Vec<u8>) -> (isize, usize) {
    let bufsz = buf.len();
    let mut dst = 0usize;
    let mut src = 0usize;
    let mut ret: isize = -2; // incomplete

    decoder.total_read += bufsz as u64;

    'outer: loop {
        match decoder.state {
            ChunkedState::ChunkSize => {
                loop {
                    if src == bufsz {
                        break 'outer;
                    }
                    match decode_hex(buf[src]) {
                        Some(v) => {
                            if decoder.hex_count == (std::mem::size_of::<usize>() * 2) as u8 {
                                ret = -1;
                                break 'outer;
                            }
                            decoder.bytes_left_in_chunk = decoder.bytes_left_in_chunk * 16 + v;
                            decoder.hex_count += 1;
                            src += 1;
                        }
                        None => {
                            if decoder.hex_count == 0 {
                                ret = -1;
                                break 'outer;
                            }
                            match buf[src] {
                                b' ' | b'\t' | b';' | b'\n' | b'\r' => {}
                                _ => {
                                    ret = -1;
                                    break 'outer;
                                }
                            }
                            break;
                        }
                    }
                }
                decoder.hex_count = 0;
                decoder.state = ChunkedState::ChunkExt;
                // fallthrough
                loop {
                    if src == bufsz {
                        break 'outer;
                    }
                    if buf[src] == b'\r' {
                        src += 1;
                        break;
                    } else if buf[src] == b'\n' {
                        ret = -1;
                        break 'outer;
                    }
                    src += 1;
                }
                decoder.state = ChunkedState::ChunkHeaderExpectLf;
                // fallthrough to expect LF
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                if decoder.bytes_left_in_chunk == 0 {
                    if decoder.consume_trailer {
                        decoder.state = ChunkedState::TrailersLineHead;
                        continue 'outer;
                    } else {
                        ret = (bufsz - src) as isize;
                        break 'outer;
                    }
                }
                decoder.state = ChunkedState::ChunkData;
                // fallthrough to chunk data
                let avail = bufsz - src;
                if avail < decoder.bytes_left_in_chunk {
                    if dst != src {
                        // memmove equivalent
                        for i in 0..avail {
                            buf[dst + i] = buf[src + i];
                        }
                    }
                    src += avail;
                    dst += avail;
                    decoder.bytes_left_in_chunk -= avail;
                    break 'outer;
                }
                if dst != src {
                    for i in 0..decoder.bytes_left_in_chunk {
                        buf[dst + i] = buf[src + i];
                    }
                }
                src += decoder.bytes_left_in_chunk;
                dst += decoder.bytes_left_in_chunk;
                decoder.bytes_left_in_chunk = 0;
                decoder.state = ChunkedState::ChunkDataExpectCr;
                // fallthrough
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\r' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkDataExpectLf;
                // fallthrough
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkSize;
            }
            ChunkedState::ChunkExt => {
                loop {
                    if src == bufsz {
                        break 'outer;
                    }
                    if buf[src] == b'\r' {
                        src += 1;
                        break;
                    } else if buf[src] == b'\n' {
                        ret = -1;
                        break 'outer;
                    }
                    src += 1;
                }
                decoder.state = ChunkedState::ChunkHeaderExpectLf;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                if decoder.bytes_left_in_chunk == 0 {
                    if decoder.consume_trailer {
                        decoder.state = ChunkedState::TrailersLineHead;
                        continue 'outer;
                    } else {
                        ret = (bufsz - src) as isize;
                        break 'outer;
                    }
                }
                decoder.state = ChunkedState::ChunkData;
                let avail = bufsz - src;
                if avail < decoder.bytes_left_in_chunk {
                    if dst != src {
                        for i in 0..avail {
                            buf[dst + i] = buf[src + i];
                        }
                    }
                    src += avail;
                    dst += avail;
                    decoder.bytes_left_in_chunk -= avail;
                    break 'outer;
                }
                if dst != src {
                    for i in 0..decoder.bytes_left_in_chunk {
                        buf[dst + i] = buf[src + i];
                    }
                }
                src += decoder.bytes_left_in_chunk;
                dst += decoder.bytes_left_in_chunk;
                decoder.bytes_left_in_chunk = 0;
                decoder.state = ChunkedState::ChunkDataExpectCr;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\r' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkDataExpectLf;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkSize;
            }
            ChunkedState::ChunkHeaderExpectLf => {
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                if decoder.bytes_left_in_chunk == 0 {
                    if decoder.consume_trailer {
                        decoder.state = ChunkedState::TrailersLineHead;
                        continue 'outer;
                    } else {
                        ret = (bufsz - src) as isize;
                        break 'outer;
                    }
                }
                decoder.state = ChunkedState::ChunkData;
                let avail = bufsz - src;
                if avail < decoder.bytes_left_in_chunk {
                    if dst != src {
                        for i in 0..avail {
                            buf[dst + i] = buf[src + i];
                        }
                    }
                    src += avail;
                    dst += avail;
                    decoder.bytes_left_in_chunk -= avail;
                    break 'outer;
                }
                if dst != src {
                    for i in 0..decoder.bytes_left_in_chunk {
                        buf[dst + i] = buf[src + i];
                    }
                }
                src += decoder.bytes_left_in_chunk;
                dst += decoder.bytes_left_in_chunk;
                decoder.bytes_left_in_chunk = 0;
                decoder.state = ChunkedState::ChunkDataExpectCr;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\r' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkDataExpectLf;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkSize;
            }
            ChunkedState::ChunkData => {
                let avail = bufsz - src;
                if avail < decoder.bytes_left_in_chunk {
                    if dst != src {
                        for i in 0..avail {
                            buf[dst + i] = buf[src + i];
                        }
                    }
                    src += avail;
                    dst += avail;
                    decoder.bytes_left_in_chunk -= avail;
                    break 'outer;
                }
                if dst != src {
                    for i in 0..decoder.bytes_left_in_chunk {
                        buf[dst + i] = buf[src + i];
                    }
                }
                src += decoder.bytes_left_in_chunk;
                dst += decoder.bytes_left_in_chunk;
                decoder.bytes_left_in_chunk = 0;
                decoder.state = ChunkedState::ChunkDataExpectCr;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\r' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkDataExpectLf;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkSize;
            }
            ChunkedState::ChunkDataExpectCr => {
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\r' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkDataExpectLf;
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkSize;
            }
            ChunkedState::ChunkDataExpectLf => {
                if src == bufsz {
                    break 'outer;
                }
                if buf[src] != b'\n' {
                    ret = -1;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::ChunkSize;
            }
            ChunkedState::TrailersLineHead => {
                loop {
                    if src == bufsz {
                        break 'outer;
                    }
                    if buf[src] != b'\r' {
                        break;
                    }
                    src += 1;
                }
                if buf[src] == b'\n' {
                    src += 1;
                    ret = (bufsz - src) as isize;
                    break 'outer;
                }
                src += 1;
                decoder.state = ChunkedState::TrailersLineMiddle;
                // fallthrough
                loop {
                    if src == bufsz {
                        break 'outer;
                    }
                    if buf[src] == b'\n' {
                        break;
                    }
                    src += 1;
                }
                src += 1;
                decoder.state = ChunkedState::TrailersLineHead;
            }
            ChunkedState::TrailersLineMiddle => {
                loop {
                    if src == bufsz {
                        break 'outer;
                    }
                    if buf[src] == b'\n' {
                        break;
                    }
                    src += 1;
                }
                src += 1;
                decoder.state = ChunkedState::TrailersLineHead;
            }
        }
    }

    // Move remaining data
    if dst != src {
        let remaining = bufsz - src;
        for i in 0..remaining {
            buf[dst + i] = buf[src + i];
        }
    }
    // Effective size of decoded data
    let final_bufsz = dst;

    // Overhead check
    if ret == -2 {
        decoder.total_overhead += (bufsz - final_bufsz) as u64;
        if decoder.total_overhead >= 100 * 1024
            && decoder.total_read - decoder.total_overhead < decoder.total_read / 4
        {
            ret = -1;
        }
    }

    (ret, final_bufsz)
}

// ================================================================
// SECTION 3: Tests
// ================================================================

fn bufis(s: &[u8], t: &[u8]) -> bool {
    s == t
}

fn parse_result_to_int(r: ParseResult, input_len: usize) -> i32 {
    match r {
        ParseResult::Complete(n) => n as i32,
        ParseResult::Incomplete => -2,
        ParseResult::Error => -1,
    }
}

const INPUT_BUF_SIZE: usize = 4096;

fn test_request() {
    // Use a buffer that we copy test input into (mimicking the C test approach)
    let mut input_storage = vec![0u8; INPUT_BUF_SIZE];

    macro_rules! parse {
        ($s:expr, $last_len:expr, $exp:expr, $comment:expr) => {{
            let s: &[u8] = $s;
            let slen = s.len();
            note($comment);
            // Copy into end of buffer
            let start = INPUT_BUF_SIZE - slen;
            input_storage[start..start + slen].copy_from_slice(s);
            let buf = &input_storage[start..start + slen];
            let (result, method, path, minor_version, headers) =
                phr_parse_request(buf, $last_len);
            let r = parse_result_to_int(result, slen);
            let expected: i32 = if $exp == 0 { slen as i32 } else { $exp };
            ok(r == expected);
            (method, path, minor_version, headers)
        }};
    }

    let (method, path, minor_version, headers) =
        parse!(b"GET / HTTP/1.0\r\n\r\n", 0, 0, "simple");
    ok(headers.len() == 0);
    ok(bufis(method.unwrap(), b"GET"));
    ok(bufis(path.unwrap(), b"/"));
    ok(minor_version == 0);

    parse!(b"GET / HTTP/1.0\r\n\r", 0, -2, "partial");

    let (method, path, minor_version, headers) =
        parse!(b"GET /hoge HTTP/1.1\r\nHost: example.com\r\nCookie: \r\n\r\n", 0, 0, "parse headers");
    ok(headers.len() == 2);
    ok(bufis(method.unwrap(), b"GET"));
    ok(bufis(path.unwrap(), b"/hoge"));
    ok(minor_version == 1);
    ok(bufis(headers[0].name.unwrap(), b"Host"));
    ok(bufis(headers[0].value, b"example.com"));
    ok(bufis(headers[1].name.unwrap(), b"Cookie"));
    ok(bufis(headers[1].value, b""));

    let (method, path, minor_version, headers) =
        parse!(b"GET /hoge HTTP/1.1\r\nHost: example.com\r\nUser-Agent: \xe3\x81\xb2\xe3/1.0\r\n\r\n", 0, 0, "multibyte included");
    ok(headers.len() == 2);
    ok(bufis(method.unwrap(), b"GET"));
    ok(bufis(path.unwrap(), b"/hoge"));
    ok(minor_version == 1);
    ok(bufis(headers[0].name.unwrap(), b"Host"));
    ok(bufis(headers[0].value, b"example.com"));
    ok(bufis(headers[1].name.unwrap(), b"User-Agent"));
    ok(bufis(headers[1].value, b"\xe3\x81\xb2\xe3/1.0"));

    let (method, path, minor_version, headers) =
        parse!(b"GET / HTTP/1.0\r\nfoo: \r\nfoo: b\r\n  \tc\r\n\r\n", 0, 0, "parse multiline");
    ok(headers.len() == 3);
    ok(bufis(method.unwrap(), b"GET"));
    ok(bufis(path.unwrap(), b"/"));
    ok(minor_version == 0);
    ok(bufis(headers[0].name.unwrap(), b"foo"));
    ok(bufis(headers[0].value, b""));
    ok(bufis(headers[1].name.unwrap(), b"foo"));
    ok(bufis(headers[1].value, b"b"));
    ok(headers[2].name.is_none());
    ok(bufis(headers[2].value, b"  \tc"));

    parse!(b"GET / HTTP/1.0\r\nfoo : ab\r\n\r\n", 0, -1, "parse header name with trailing space");

    let (method, _, _, _) = parse!(b"GET", 0, -2, "incomplete 1");
    ok(method.is_none());
    let (method, _, _, _) = parse!(b"GET ", 0, -2, "incomplete 2");
    ok(bufis(method.unwrap(), b"GET"));
    let (_, path, _, _) = parse!(b"GET /", 0, -2, "incomplete 3");
    ok(path.is_none());
    let (_, path, _, _) = parse!(b"GET / ", 0, -2, "incomplete 4");
    ok(bufis(path.unwrap(), b"/"));
    parse!(b"GET / H", 0, -2, "incomplete 5");
    parse!(b"GET / HTTP/1.", 0, -2, "incomplete 6");
    let (_, _, minor_version, _) = parse!(b"GET / HTTP/1.0", 0, -2, "incomplete 7");
    ok(minor_version == -1);
    let (_, _, minor_version, _) = parse!(b"GET / HTTP/1.0\r", 0, -2, "incomplete 8");
    ok(minor_version == 0);

    let slen = b"GET /hoge HTTP/1.0\r\n\r".len();
    parse!(b"GET /hoge HTTP/1.0\r\n\r", slen - 1, -2, "slowloris (incomplete)");
    let slen = b"GET /hoge HTTP/1.0\r\n\r\n".len();
    parse!(b"GET /hoge HTTP/1.0\r\n\r\n", slen - 1, 0, "slowloris (complete)");

    parse!(b" / HTTP/1.0\r\n\r\n", 0, -1, "empty method");
    parse!(b"GET  HTTP/1.0\r\n\r\n", 0, -1, "empty request-target");

    parse!(b"GET / HTTP/1.0\r\n:a\r\n\r\n", 0, -1, "empty header name");
    parse!(b"GET / HTTP/1.0\r\n :a\r\n\r\n", 0, -1, "header name (space only)");

    parse!(b"G\0T / HTTP/1.0\r\n\r\n", 0, -1, "NUL in method");
    parse!(b"G\tT / HTTP/1.0\r\n\r\n", 0, -1, "tab in method");
    parse!(b":GET / HTTP/1.0\r\n\r\n", 0, -1, "invalid method");
    parse!(b"GET /\x7fhello HTTP/1.0\r\n\r\n", 0, -1, "DEL in uri-path");
    parse!(b"GET / HTTP/1.0\r\na\0b: c\r\n\r\n", 0, -1, "NUL in header name");
    parse!(b"GET / HTTP/1.0\r\nab: c\0d\r\n\r\n", 0, -1, "NUL in header value");
    parse!(b"GET / HTTP/1.0\r\na\x1bb: c\r\n\r\n", 0, -1, "CTL in header name");
    parse!(b"GET / HTTP/1.0\r\nab: c\x1b\r\n\r\n", 0, -1, "CTL in header value");
    parse!(b"GET / HTTP/1.0\r\n/: 1\r\n\r\n", 0, -1, "invalid char in header value");

    let (method, path, minor_version, headers) =
        parse!(b"GET /\xa0 HTTP/1.0\r\nh: c\xa2y\r\n\r\n", 0, 0, "accept MSB chars");
    ok(headers.len() == 1);
    ok(bufis(method.unwrap(), b"GET"));
    ok(bufis(path.unwrap(), b"/\xa0"));
    ok(minor_version == 0);
    ok(bufis(headers[0].name.unwrap(), b"h"));
    ok(bufis(headers[0].value, b"c\xa2y"));

    let (_, _, _, headers) =
        parse!(b"GET / HTTP/1.0\r\n\x7c\x7e: 1\r\n\r\n", 0, 0, "accept |~ (though forbidden by SSE)");
    ok(headers.len() == 1);
    ok(bufis(headers[0].name.unwrap(), b"\x7c\x7e"));
    ok(bufis(headers[0].value, b"1"));

    parse!(b"GET / HTTP/1.0\r\n\x7b: 1\r\n\r\n", 0, -1, "disallow {");

    let (_, _, _, headers) =
        parse!(b"GET / HTTP/1.0\r\nfoo: a \t \r\n\r\n", 0, 0, "exclude leading and trailing spaces in header value");
    ok(bufis(headers[0].value, b"a"));

    parse!(b"GET   /   HTTP/1.0\r\n\r\n", 0, 0, "accept multiple spaces between tokens");
}

fn test_response() {
    let mut input_storage = vec![0u8; INPUT_BUF_SIZE];

    macro_rules! parse {
        ($s:expr, $last_len:expr, $exp:expr, $comment:expr) => {{
            let s: &[u8] = $s;
            let slen = s.len();
            note($comment);
            let start = INPUT_BUF_SIZE - slen;
            input_storage[start..start + slen].copy_from_slice(s);
            let buf = &input_storage[start..start + slen];
            let (result, minor_version, status, msg, headers) =
                phr_parse_response(buf, $last_len);
            let r = parse_result_to_int(result, slen);
            let expected: i32 = if $exp == 0 { slen as i32 } else { $exp };
            ok(r == expected);
            (minor_version, status, msg, headers)
        }};
    }

    let (minor_version, status, msg, headers) =
        parse!(b"HTTP/1.0 200 OK\r\n\r\n", 0, 0, "simple");
    ok(headers.len() == 0);
    ok(status == 200);
    ok(minor_version == 0);
    ok(bufis(msg.unwrap(), b"OK"));

    parse!(b"HTTP/1.0 200 OK\r\n\r", 0, -2, "partial");

    let (minor_version, status, msg, headers) =
        parse!(b"HTTP/1.1 200 OK\r\nHost: example.com\r\nCookie: \r\n\r\n", 0, 0, "parse headers");
    ok(headers.len() == 2);
    ok(minor_version == 1);
    ok(status == 200);
    ok(bufis(msg.unwrap(), b"OK"));
    ok(bufis(headers[0].name.unwrap(), b"Host"));
    ok(bufis(headers[0].value, b"example.com"));
    ok(bufis(headers[1].name.unwrap(), b"Cookie"));
    ok(bufis(headers[1].value, b""));

    let (minor_version, status, msg, headers) =
        parse!(b"HTTP/1.0 200 OK\r\nfoo: \r\nfoo: b\r\n  \tc\r\n\r\n", 0, 0, "parse multiline");
    ok(headers.len() == 3);
    ok(minor_version == 0);
    ok(status == 200);
    ok(bufis(msg.unwrap(), b"OK"));
    ok(bufis(headers[0].name.unwrap(), b"foo"));
    ok(bufis(headers[0].value, b""));
    ok(bufis(headers[1].name.unwrap(), b"foo"));
    ok(bufis(headers[1].value, b"b"));
    ok(headers[2].name.is_none());
    ok(bufis(headers[2].value, b"  \tc"));

    let (minor_version, status, msg, headers) =
        parse!(b"HTTP/1.0 500 Internal Server Error\r\n\r\n", 0, 0, "internal server error");
    ok(headers.len() == 0);
    ok(minor_version == 0);
    ok(status == 500);
    ok(bufis(msg.unwrap(), b"Internal Server Error"));
    ok(msg.unwrap().len() == "Internal Server Error".len());

    let (minor_version, _, _, _) = parse!(b"H", 0, -2, "incomplete 1");
    parse!(b"HTTP/1.", 0, -2, "incomplete 2");
    let (minor_version, _, _, _) = parse!(b"HTTP/1.1", 0, -2, "incomplete 3");
    ok(minor_version == -1);
    let (minor_version, _, _, _) = parse!(b"HTTP/1.1 ", 0, -2, "incomplete 4");
    ok(minor_version == 1);
    parse!(b"HTTP/1.1 2", 0, -2, "incomplete 5");
    let (_, status, _, _) = parse!(b"HTTP/1.1 200", 0, -2, "incomplete 6");
    ok(status == 0);
    let (_, status, _, _) = parse!(b"HTTP/1.1 200 ", 0, -2, "incomplete 7");
    ok(status == 200);
    parse!(b"HTTP/1.1 200 O", 0, -2, "incomplete 8");
    let (_, _, msg, _) = parse!(b"HTTP/1.1 200 OK\r", 0, -2, "incomplete 9");
    ok(msg.is_none());
    let (_, _, msg, _) = parse!(b"HTTP/1.1 200 OK\r\n", 0, -2, "incomplete 10");
    ok(bufis(msg.unwrap(), b"OK"));
    let (_, _, msg, _) = parse!(b"HTTP/1.1 200 OK\n", 0, -2, "incomplete 11");
    ok(bufis(msg.unwrap(), b"OK"));

    let (_, _, _, headers) = parse!(b"HTTP/1.1 200 OK\r\nA: 1\r", 0, -2, "incomplete 11");
    ok(headers.len() == 0);
    let (_, _, _, headers) = parse!(b"HTTP/1.1 200 OK\r\nA: 1\r\n", 0, -2, "incomplete 12");
    ok(headers.len() == 1);
    ok(bufis(headers[0].name.unwrap(), b"A"));
    ok(bufis(headers[0].value, b"1"));

    let slen = b"HTTP/1.0 200 OK\r\n\r".len();
    parse!(b"HTTP/1.0 200 OK\r\n\r", slen - 1, -2, "slowloris (incomplete)");
    let slen = b"HTTP/1.0 200 OK\r\n\r\n".len();
    parse!(b"HTTP/1.0 200 OK\r\n\r\n", slen - 1, 0, "slowloris (complete)");

    parse!(b"HTTP/1. 200 OK\r\n\r\n", 0, -1, "invalid http version");
    parse!(b"HTTP/1.2z 200 OK\r\n\r\n", 0, -1, "invalid http version 2");
    parse!(b"HTTP/1.1  OK\r\n\r\n", 0, -1, "no status code");

    let (_, _, msg, _) = parse!(b"HTTP/1.1 200\r\n\r\n", 0, 0, "accept missing trailing whitespace in status-line");
    ok(bufis(msg.unwrap(), b""));
    parse!(b"HTTP/1.1 200X\r\n\r\n", 0, -1, "garbage after status 1");
    parse!(b"HTTP/1.1 200X \r\n\r\n", 0, -1, "garbage after status 2");
    parse!(b"HTTP/1.1 200X OK\r\n\r\n", 0, -1, "garbage after status 3");

    let (_, _, _, headers) =
        parse!(b"HTTP/1.1 200 OK\r\nbar: \t b\t \t\r\n\r\n", 0, 0, "exclude leading and trailing spaces in header value");
    ok(bufis(headers[0].value, b"b"));

    parse!(b"HTTP/1.1   200   OK\r\n\r\n", 0, 0, "accept multiple spaces between tokens");
}

fn test_headers() {
    macro_rules! parse {
        ($s:expr, $last_len:expr, $exp:expr, $comment:expr) => {{
            let s: &[u8] = $s;
            let slen = s.len();
            note($comment);
            let (result, headers) = phr_parse_headers_only(s, $last_len);
            let r = parse_result_to_int(result, slen);
            let expected: i32 = if $exp == 0 { slen as i32 } else { $exp };
            ok(r == expected);
            headers
        }};
    }

    let headers = parse!(b"Host: example.com\r\nCookie: \r\n\r\n", 0, 0, "simple");
    ok(headers.len() == 2);
    ok(bufis(headers[0].name.unwrap(), b"Host"));
    ok(bufis(headers[0].value, b"example.com"));
    ok(bufis(headers[1].name.unwrap(), b"Cookie"));
    ok(bufis(headers[1].value, b""));

    let headers = parse!(b"Host: example.com\r\nCookie: \r\n\r\n", 1, 0, "slowloris");
    ok(headers.len() == 2);
    ok(bufis(headers[0].name.unwrap(), b"Host"));
    ok(bufis(headers[0].value, b"example.com"));
    ok(bufis(headers[1].name.unwrap(), b"Cookie"));
    ok(bufis(headers[1].value, b""));

    parse!(b"Host: example.com\r\nCookie: \r\n\r", 0, -2, "partial");

    parse!(b"Host: e\x7fample.com\r\nCookie: \r\n\r", 0, -1, "error");
}

fn test_chunked_at_once(consume_trailer: bool, encoded: &[u8], decoded: &[u8], expected: isize) {
    let mut dec = ChunkedDecoder::new();
    dec.consume_trailer = consume_trailer;

    note("testing at-once");

    let mut buf = encoded.to_vec();
    let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut buf);

    ok(ret == expected);
    ok(bufsz == decoded.len());
    ok(&buf[..bufsz] == decoded);
    if expected >= 0 {
        if ret == expected {
            let remaining_start = encoded.len() - expected as usize;
            ok(&buf[bufsz..bufsz + expected as usize] == &encoded[remaining_start..]);
        } else {
            ok(false);
        }
    }
}

fn test_chunked_per_byte(consume_trailer: bool, encoded: &[u8], decoded: &[u8], expected: isize) {
    let mut dec = ChunkedDecoder::new();
    dec.consume_trailer = consume_trailer;

    note("testing per-byte");

    let bytes_to_consume = encoded.len() - if expected >= 0 { expected as usize } else { 0 };
    let mut buf: Vec<u8> = Vec::with_capacity(encoded.len() + 1);
    let mut bytes_ready = 0usize;

    for i in 0..bytes_to_consume - 1 {
        if bytes_ready >= buf.len() {
            buf.push(encoded[i]);
        } else {
            buf[bytes_ready] = encoded[i];
            // Ensure length covers what we need
            if buf.len() <= bytes_ready {
                buf.resize(bytes_ready + 1, 0);
            }
        }
        let mut chunk = vec![buf[bytes_ready]];
        let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut chunk);
        if ret != -2 {
            ok(false);
            return;
        }
        if bufsz > 0 {
            buf[bytes_ready] = chunk[0];
        }
        bytes_ready += bufsz;
    }
    // Feed remaining bytes
    let remaining = &encoded[bytes_to_consume - 1..];
    buf.truncate(bytes_ready);
    buf.extend_from_slice(remaining);
    let mut chunk: Vec<u8> = buf[bytes_ready..].to_vec();
    let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut chunk);
    ok(ret == expected);
    // Copy decoded data back
    for i in 0..bufsz {
        buf[bytes_ready + i] = chunk[i];
    }
    bytes_ready += bufsz;
    ok(bytes_ready == decoded.len());
    ok(&buf[..bytes_ready] == decoded);
    if expected >= 0 {
        if ret == expected {
            // Check undecoded trailing data
            let trailing = &chunk[bufsz..bufsz + expected as usize];
            let expected_trailing = &encoded[bytes_to_consume..];
            ok(trailing == expected_trailing);
        } else {
            ok(false);
        }
    }
}

fn test_chunked_failure(encoded: &[u8], expected: isize) {
    note("testing failure at-once");
    {
        let mut dec = ChunkedDecoder::new();
        let mut buf = encoded.to_vec();
        let (ret, _) = phr_decode_chunked(&mut dec, &mut buf);
        ok(ret == expected);
    }

    note("testing failure per-byte");
    {
        let mut dec = ChunkedDecoder::new();
        for i in 0..encoded.len() {
            let mut chunk = vec![encoded[i]];
            let (ret, _) = phr_decode_chunked(&mut dec, &mut chunk);
            if ret == -1 {
                ok(ret == expected);
                return;
            } else if ret == -2 {
                // continue
            } else {
                ok(false);
                return;
            }
        }
        ok(-2 == expected);
    }
}

type ChunkedTestRunner = fn(bool, &[u8], &[u8], isize);

fn test_chunked() {
    let runners: &[ChunkedTestRunner] = &[test_chunked_at_once, test_chunked_per_byte];

    for runner in runners {
        runner(false, b"b\r\nhello world\r\n0\r\n", b"hello world", 0);
        runner(false, b"6\r\nhello \r\n5\r\nworld\r\n0\r\n", b"hello world", 0);
        runner(false, b"6;comment=hi\r\nhello \r\n5\r\nworld\r\n0\r\n", b"hello world", 0);
        runner(false, b"6 ; comment\r\nhello \r\n5\r\nworld\r\n0\r\n", b"hello world", 0);
        runner(false, b"6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\r\nc: d\r\n\r\n", b"hello world",
               (b"a: b\r\nc: d\r\n\r\n".len()) as isize);
        runner(false, b"b\r\nhello world\r\n0\r\n", b"hello world", 0);
    }

    note("failures");
    test_chunked_failure(b"z\r\nabcdefg", -1);
    if std::mem::size_of::<usize>() == 8 {
        test_chunked_failure(b"6\r\nhello \r\nffffffffffffffff\r\nabcdefg", -2);
        test_chunked_failure(b"6\r\nhello \r\nfffffffffffffffff\r\nabcdefg", -1);
    }
    test_chunked_failure(b"1x\r\na\r\n0\r\n", -1);

    test_chunked_failure(b"6\nhello \r\n5\r\nworld\r\n0\r\n", -1);
    test_chunked_failure(b"6\r\nhello \n5\r\nworld\r\n0\r\n", -1);
    test_chunked_failure(b"6\r\nhello \r\n5\r\nworld\n0\r\n", -1);
    test_chunked_failure(b"6\r\nhello \r\n5\r\nworld\n0\r\n", -1);
    test_chunked_failure(b"6\r\nhello \r\n5\r\nworld\r\n0\n", -1);
    test_chunked_failure(b"6\rX\nhello \n5\r\nworld\r\n0\r\n", -1);
}

fn test_chunked_consume_trailer() {
    let runners: &[ChunkedTestRunner] = &[test_chunked_at_once, test_chunked_per_byte];

    for runner in runners {
        runner(true, b"b\r\nhello world\r\n0\r\n", b"hello world", -2);
        runner(true, b"6\r\nhello \r\n5\r\nworld\r\n0\r\n", b"hello world", -2);
        runner(true, b"6;comment=hi\r\nhello \r\n5\r\nworld\r\n0\r\n", b"hello world", -2);
        runner(true, b"b\r\nhello world\r\n0\r\n\r\n", b"hello world", 0);
        runner(true, b"6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\r\nc: d\r\n\r\n", b"hello world", 0);
        runner(true, b"b\r\nhello world\r\n0\r\n\n", b"hello world", 0);
        runner(true, b"6\r\nhello \r\n5\r\nworld\r\n0\r\na: b\nc: d\n\n", b"hello world", 0);
    }
}

fn test_chunked_leftdata() {
    const NEXT_REQ: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    let mut dec = ChunkedDecoder::new();
    dec.consume_trailer = true;
    let mut input = b"5\r\nabcde\r\n0\r\n\r\n".to_vec();
    input.extend_from_slice(NEXT_REQ);

    let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut input);
    ok(ret >= 0);
    ok(bufsz == 5);
    ok(&input[..5] == b"abcde");
    ok(ret == NEXT_REQ.len() as isize);
    ok(&input[bufsz..bufsz + ret as usize] == NEXT_REQ);
}

fn do_test_chunked_overhead(chunk_len: usize, chunk_count: usize, extra: &str) -> isize {
    let mut dec = ChunkedDecoder::new();

    for _ in 0..chunk_count {
        // Build chunk header
        let header = format!("{:x}{}\r\n", chunk_len, extra);
        let mut buf = header.into_bytes();
        let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut buf);
        if ret != -2 {
            return ret;
        }
        assert!(bufsz == 0);

        // Build chunk body
        let mut buf = vec![b'A'; chunk_len];
        let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut buf);
        if ret != -2 {
            return ret;
        }
        assert!(bufsz == chunk_len);

        // Build chunk end (CRLF)
        let mut buf = b"\r\n".to_vec();
        let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut buf);
        if ret != -2 {
            return ret;
        }
        assert!(bufsz == 0);
    }

    // End chunk
    let mut buf = b"0\r\n\r\n".to_vec();
    let (ret, bufsz) = phr_decode_chunked(&mut dec, &mut buf);
    assert!(bufsz == 0);
    ret
}

fn test_chunked_overhead() {
    ok(do_test_chunked_overhead(100, 10000, "") == 2);
    ok(do_test_chunked_overhead(10, 100000, "") == 2);
    ok(do_test_chunked_overhead(1, 1000000, "") == -1);
    ok(do_test_chunked_overhead(10, 100000, "; tiny=1") == 2);
    ok(do_test_chunked_overhead(10, 100000, "; large=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa") == -1);
}

fn main() {
    subtest("request", test_request);
    subtest("response", test_response);
    subtest("headers", test_headers);
    subtest("chunked", test_chunked);
    subtest("chunked-consume-trailer", test_chunked_consume_trailer);
    subtest("chunked-leftdata", test_chunked_leftdata);
    subtest("chunked-overhead", test_chunked_overhead);

    let exit_code = done_testing();
    std::process::exit(exit_code);
}
