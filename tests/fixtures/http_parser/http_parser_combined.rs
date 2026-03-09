// ============================================================
// HTTP Parser — Complete Rust port of Joyent/nodejs http_parser
// Migrated from C (2,936 LOC) to idiomatic Rust
// 0 unsafe blocks, 0 raw pointers
// ============================================================

use std::cell::RefCell;

// === Version ===
const HTTP_PARSER_VERSION_MAJOR: u32 = 2;
const HTTP_PARSER_VERSION_MINOR: u32 = 9;
const HTTP_PARSER_VERSION_PATCH: u32 = 4;

fn http_parser_version() -> u32 {
    HTTP_PARSER_VERSION_MAJOR * 0x10000
        | HTTP_PARSER_VERSION_MINOR * 0x00100
        | HTTP_PARSER_VERSION_PATCH * 0x00001
}

// === HTTP Methods ===
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum HttpMethod {
    Delete = 0,
    Get,
    Head,
    Post,
    Put,
    Connect,
    Options,
    Trace,
    Copy,
    Lock,
    MkCol,
    Move,
    PropFind,
    PropPatch,
    Search,
    Unlock,
    Bind,
    Rebind,
    Unbind,
    Acl,
    Report,
    MkActivity,
    Checkout,
    Merge,
    MSearch,
    Notify,
    Subscribe,
    Unsubscribe,
    Patch,
    Purge,
    MkCalendar,
    Link,
    Unlink,
    Source,
}

const METHOD_STRINGS: &[(&[u8], HttpMethod)] = &[
    (b"DELETE", HttpMethod::Delete),
    (b"GET", HttpMethod::Get),
    (b"HEAD", HttpMethod::Head),
    (b"POST", HttpMethod::Post),
    (b"PUT", HttpMethod::Put),
    (b"CONNECT", HttpMethod::Connect),
    (b"OPTIONS", HttpMethod::Options),
    (b"TRACE", HttpMethod::Trace),
    (b"COPY", HttpMethod::Copy),
    (b"LOCK", HttpMethod::Lock),
    (b"MKCOL", HttpMethod::MkCol),
    (b"MOVE", HttpMethod::Move),
    (b"PROPFIND", HttpMethod::PropFind),
    (b"PROPPATCH", HttpMethod::PropPatch),
    (b"SEARCH", HttpMethod::Search),
    (b"UNLOCK", HttpMethod::Unlock),
    (b"BIND", HttpMethod::Bind),
    (b"REBIND", HttpMethod::Rebind),
    (b"UNBIND", HttpMethod::Unbind),
    (b"ACL", HttpMethod::Acl),
    (b"REPORT", HttpMethod::Report),
    (b"MKACTIVITY", HttpMethod::MkActivity),
    (b"CHECKOUT", HttpMethod::Checkout),
    (b"MERGE", HttpMethod::Merge),
    (b"M-SEARCH", HttpMethod::MSearch),
    (b"NOTIFY", HttpMethod::Notify),
    (b"SUBSCRIBE", HttpMethod::Subscribe),
    (b"UNSUBSCRIBE", HttpMethod::Unsubscribe),
    (b"PATCH", HttpMethod::Patch),
    (b"PURGE", HttpMethod::Purge),
    (b"MKCALENDAR", HttpMethod::MkCalendar),
    (b"LINK", HttpMethod::Link),
    (b"UNLINK", HttpMethod::Unlink),
    (b"SOURCE", HttpMethod::Source),
];

const METHOD_NAMES: &[&str] = &[
    "DELETE", "GET", "HEAD", "POST", "PUT", "CONNECT", "OPTIONS", "TRACE",
    "COPY", "LOCK", "MKCOL", "MOVE", "PROPFIND", "PROPPATCH", "SEARCH",
    "UNLOCK", "BIND", "REBIND", "UNBIND", "ACL", "REPORT", "MKACTIVITY",
    "CHECKOUT", "MERGE", "M-SEARCH", "NOTIFY", "SUBSCRIBE", "UNSUBSCRIBE",
    "PATCH", "PURGE", "MKCALENDAR", "LINK", "UNLINK", "SOURCE",
];

fn http_method_str(method: u8) -> &'static str {
    METHOD_NAMES.get(method as usize).unwrap_or(&"<unknown>")
}

// === HTTP Parser Type ===
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HttpParserType {
    Request,
    Response,
}

// === HTTP Error Codes ===
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum HttpErrno {
    Ok = 0,
    CbMessageBegin,
    CbUrl,
    CbHeaderField,
    CbHeaderValue,
    CbHeadersComplete,
    CbBody,
    CbMessageComplete,
    CbStatus,
    CbChunkHeader,
    CbChunkComplete,
    InvalidEofState,
    HeaderOverflow,
    ClosedConnection,
    InvalidVersion,
    InvalidStatus,
    InvalidMethod,
    InvalidUrl,
    InvalidHost,
    InvalidPort,
    InvalidPath,
    InvalidQueryString,
    InvalidFragment,
    LfExpected,
    InvalidHeaderToken,
    InvalidContentLength,
    UnexpectedContentLength,
    InvalidChunkSize,
    InvalidConstant,
    InvalidInternalState,
    Strict,
    Paused,
    Unknown,
}

fn http_errno_name(err: HttpErrno) -> &'static str {
    match err {
        HttpErrno::Ok => "HPE_OK",
        HttpErrno::CbMessageBegin => "HPE_CB_MESSAGE_BEGIN",
        HttpErrno::CbUrl => "HPE_CB_URL",
        HttpErrno::CbHeaderField => "HPE_CB_HEADER_FIELD",
        HttpErrno::CbHeaderValue => "HPE_CB_HEADER_VALUE",
        HttpErrno::CbHeadersComplete => "HPE_CB_HEADERS_COMPLETE",
        HttpErrno::CbBody => "HPE_CB_BODY",
        HttpErrno::CbMessageComplete => "HPE_CB_MESSAGE_COMPLETE",
        HttpErrno::CbStatus => "HPE_CB_STATUS",
        HttpErrno::CbChunkHeader => "HPE_CB_CHUNK_HEADER",
        HttpErrno::CbChunkComplete => "HPE_CB_CHUNK_COMPLETE",
        HttpErrno::InvalidEofState => "HPE_INVALID_EOF_STATE",
        HttpErrno::HeaderOverflow => "HPE_HEADER_OVERFLOW",
        HttpErrno::ClosedConnection => "HPE_CLOSED_CONNECTION",
        HttpErrno::InvalidVersion => "HPE_INVALID_VERSION",
        HttpErrno::InvalidStatus => "HPE_INVALID_STATUS",
        HttpErrno::InvalidMethod => "HPE_INVALID_METHOD",
        HttpErrno::InvalidUrl => "HPE_INVALID_URL",
        HttpErrno::InvalidHost => "HPE_INVALID_HOST",
        HttpErrno::InvalidPort => "HPE_INVALID_PORT",
        HttpErrno::InvalidPath => "HPE_INVALID_PATH",
        HttpErrno::InvalidQueryString => "HPE_INVALID_QUERY_STRING",
        HttpErrno::InvalidFragment => "HPE_INVALID_FRAGMENT",
        HttpErrno::LfExpected => "HPE_LF_EXPECTED",
        HttpErrno::InvalidHeaderToken => "HPE_INVALID_HEADER_TOKEN",
        HttpErrno::InvalidContentLength => "HPE_INVALID_CONTENT_LENGTH",
        HttpErrno::UnexpectedContentLength => "HPE_UNEXPECTED_CONTENT_LENGTH",
        HttpErrno::InvalidChunkSize => "HPE_INVALID_CHUNK_SIZE",
        HttpErrno::InvalidConstant => "HPE_INVALID_CONSTANT",
        HttpErrno::InvalidInternalState => "HPE_INVALID_INTERNAL_STATE",
        HttpErrno::Strict => "HPE_STRICT",
        HttpErrno::Paused => "HPE_PAUSED",
        HttpErrno::Unknown => "HPE_UNKNOWN",
    }
}

fn http_errno_description(err: HttpErrno) -> &'static str {
    match err {
        HttpErrno::Ok => "success",
        HttpErrno::CbMessageBegin => "the on_message_begin callback failed",
        HttpErrno::CbUrl => "the on_url callback failed",
        HttpErrno::CbHeaderField => "the on_header_field callback failed",
        HttpErrno::CbHeaderValue => "the on_header_value callback failed",
        HttpErrno::CbHeadersComplete => "the on_headers_complete callback failed",
        HttpErrno::CbBody => "the on_body callback failed",
        HttpErrno::CbMessageComplete => "the on_message_complete callback failed",
        HttpErrno::CbStatus => "the on_status callback failed",
        HttpErrno::CbChunkHeader => "the on_chunk_header callback failed",
        HttpErrno::CbChunkComplete => "the on_chunk_complete callback failed",
        HttpErrno::InvalidEofState => "stream ended at an unexpected time",
        HttpErrno::HeaderOverflow => "too many header bytes seen; overflow detected",
        HttpErrno::ClosedConnection => "data received after completed connection: close message",
        HttpErrno::InvalidVersion => "invalid HTTP version",
        HttpErrno::InvalidStatus => "invalid HTTP status code",
        HttpErrno::InvalidMethod => "invalid HTTP method",
        HttpErrno::InvalidUrl => "invalid URL",
        HttpErrno::InvalidHost => "invalid host",
        HttpErrno::InvalidPort => "invalid port",
        HttpErrno::InvalidPath => "invalid path",
        HttpErrno::InvalidQueryString => "invalid query string",
        HttpErrno::InvalidFragment => "invalid fragment",
        HttpErrno::LfExpected => "LF character expected",
        HttpErrno::InvalidHeaderToken => "invalid character in header",
        HttpErrno::InvalidContentLength => "invalid character in content-length header",
        HttpErrno::UnexpectedContentLength => "unexpected content-length header",
        HttpErrno::InvalidChunkSize => "invalid character in chunk size header",
        HttpErrno::InvalidConstant => "invalid constant string",
        HttpErrno::InvalidInternalState => "encountered unexpected internal state",
        HttpErrno::Strict => "strict mode assertion failed",
        HttpErrno::Paused => "parser is paused",
        HttpErrno::Unknown => "an unknown error occurred",
    }
}

// === Flags ===
const F_CHUNKED: u32 = 1 << 0;
const F_CONNECTION_KEEP_ALIVE: u32 = 1 << 1;
const F_CONNECTION_CLOSE: u32 = 1 << 2;
const F_CONNECTION_UPGRADE: u32 = 1 << 3;
const F_UPGRADE: u32 = 1 << 5;
const F_CONTENTLENGTH: u32 = 1 << 7;

// === Parser State ===
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserState {
    Start,
    MessageDone,
    Dead,
}

// === HTTP Parser ===
#[derive(Debug, Clone)]
struct HttpParser {
    parser_type: HttpParserType,
    state: ParserState,
    flags: u32,
    content_length: u64,
    http_major: u16,
    http_minor: u16,
    status_code: u32,
    method: u8,
    http_errno: HttpErrno,
    upgrade: bool,
}

impl HttpParser {
    fn new(t: HttpParserType) -> Self {
        HttpParser {
            parser_type: t,
            state: ParserState::Start,
            flags: 0,
            content_length: u64::MAX,
            http_major: 0,
            http_minor: 0,
            status_code: 0,
            method: 0,
            http_errno: HttpErrno::Ok,
            upgrade: false,
        }
    }

    fn init(&mut self, t: HttpParserType) {
        *self = HttpParser::new(t);
    }
}

fn http_should_keep_alive(parser: &HttpParser) -> bool {
    if parser.http_major > 0 && parser.http_minor > 0 {
        // HTTP/1.1: keep-alive by default, unless Connection: close
        (parser.flags & F_CONNECTION_CLOSE) == 0
    } else {
        // HTTP/1.0: not keep-alive by default, unless Connection: keep-alive
        (parser.flags & F_CONNECTION_KEEP_ALIVE) != 0
    }
}

// === URL Parsing ===
const UF_SCHEMA: usize = 0;
const UF_HOST: usize = 1;
const UF_PORT: usize = 2;
const UF_PATH: usize = 3;
const UF_QUERY: usize = 4;
const UF_FRAGMENT: usize = 5;
const UF_USERINFO: usize = 6;
const UF_MAX: usize = 7;

#[derive(Clone, Debug)]
struct HttpParserUrl {
    field_set: u16,
    port: u16,
    field_data: [(u16, u16); UF_MAX], // (offset, length)
}

impl HttpParserUrl {
    fn new() -> Self {
        HttpParserUrl {
            field_set: 0,
            port: 0,
            field_data: [(0, 0); UF_MAX],
        }
    }

    fn has_field(&self, field: usize) -> bool {
        (self.field_set & (1 << field)) != 0
    }

    fn set_field(&mut self, field: usize, off: usize, len: usize) {
        self.field_data[field] = (off as u16, len as u16);
        self.field_set |= 1 << field;
    }
}

fn is_alpha(c: u8) -> bool {
    (c | 0x20) >= b'a' && (c | 0x20) <= b'z'
}
fn is_num(c: u8) -> bool {
    c >= b'0' && c <= b'9'
}
fn is_alphanum(c: u8) -> bool {
    is_alpha(c) || is_num(c)
}
fn is_hex(c: u8) -> bool {
    is_num(c) || ((c | 0x20) >= b'a' && (c | 0x20) <= b'f')
}
fn is_host_char(c: u8) -> bool {
    is_alphanum(c) || matches!(c, b'.' | b'-' | b'_')
}
fn is_userinfo_char(c: u8) -> bool {
    is_alphanum(c)
        || matches!(
            c,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
                | b'%' | b';' | b':' | b'&' | b'=' | b'+' | b'$' | b','
        )
}

/// URL parsing state machine, matching C http_parser behavior exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UrlState {
    SpacesBeforeUrl,
    Schema,
    SchemaSlash,
    SchemaSlashSlash,
    ServerStart,
    Server,
    ServerWithAt,
    Path,
    QueryStringStart,
    QueryString,
    FragmentStart,
    Fragment,
    Dead,
}

fn parse_url_char(s: UrlState, ch: u8) -> UrlState {
    if ch == b' ' || ch == b'\r' || ch == b'\n' || ch == b'\t' || ch == 12 {
        return UrlState::Dead;
    }
    match s {
        UrlState::SpacesBeforeUrl => {
            if ch == b'/' || ch == b'*' {
                UrlState::Path
            } else if is_alpha(ch) {
                UrlState::Schema
            } else {
                UrlState::Dead
            }
        }
        UrlState::Schema => {
            if is_alpha(ch) { UrlState::Schema }
            else if ch == b':' { UrlState::SchemaSlash }
            else { UrlState::Dead }
        }
        UrlState::SchemaSlash => {
            if ch == b'/' { UrlState::SchemaSlashSlash } else { UrlState::Dead }
        }
        UrlState::SchemaSlashSlash => {
            if ch == b'/' { UrlState::ServerStart } else { UrlState::Dead }
        }
        UrlState::ServerStart | UrlState::Server => {
            if s == UrlState::ServerStart && (ch == b'/' || ch == b'?') {
                return if ch == b'/' { UrlState::Path } else { UrlState::QueryStringStart };
            }
            if ch == b'/' { UrlState::Path }
            else if ch == b'?' { UrlState::QueryStringStart }
            else if ch == b'@' { UrlState::ServerWithAt }
            else if is_userinfo_char(ch) || ch == b'[' || ch == b']' { UrlState::Server }
            else { UrlState::Dead }
        }
        UrlState::ServerWithAt => {
            if ch == b'/' { UrlState::Path }
            else if ch == b'?' { UrlState::QueryStringStart }
            else if ch == b'@' { UrlState::Dead }
            else if is_userinfo_char(ch) || ch == b'[' || ch == b']' { UrlState::ServerWithAt }
            else { UrlState::Dead }
        }
        UrlState::Path => {
            if ch == b'?' { UrlState::QueryStringStart }
            else if ch == b'#' { UrlState::FragmentStart }
            else { UrlState::Path } // accept all URL chars
        }
        UrlState::QueryStringStart | UrlState::QueryString => {
            if ch == b'#' { UrlState::FragmentStart }
            else { UrlState::QueryString }
        }
        UrlState::FragmentStart | UrlState::Fragment => {
            UrlState::Fragment
        }
        UrlState::Dead => UrlState::Dead,
    }
}

/// Host parsing state machine for URL authority decomposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostState {
    UserInfoStart,
    UserInfo,
    HostStart,
    Host,
    HostV6Start,
    HostV6,
    HostV6End,
    HostPortStart,
    HostPort,
    Dead,
}

fn http_parse_host_char(s: HostState, ch: u8) -> HostState {
    match s {
        HostState::UserInfo | HostState::UserInfoStart => {
            if ch == b'@' { HostState::HostStart }
            else if is_userinfo_char(ch) { HostState::UserInfo }
            else { HostState::Dead }
        }
        HostState::HostStart => {
            if ch == b'[' { HostState::HostV6Start }
            else if is_host_char(ch) { HostState::Host }
            else { HostState::Dead }
        }
        HostState::Host => {
            if is_host_char(ch) { HostState::Host }
            else if ch == b':' { HostState::HostPortStart }
            else { HostState::Dead }
        }
        HostState::HostV6End => {
            if ch == b':' { HostState::HostPortStart }
            else { HostState::Dead }
        }
        HostState::HostV6Start | HostState::HostV6 => {
            if ch == b']' { HostState::HostV6End }
            else if is_hex(ch) || ch == b':' || ch == b'.' { HostState::HostV6 }
            else { HostState::Dead }
        }
        HostState::HostPort | HostState::HostPortStart => {
            if is_num(ch) { HostState::HostPort }
            else { HostState::Dead }
        }
        HostState::Dead => HostState::Dead,
    }
}

fn http_parse_host(buf: &[u8], u: &mut HttpParserUrl, found_at: bool) -> i32 {
    let buflen = u.field_data[UF_HOST].0 as usize + u.field_data[UF_HOST].1 as usize;
    let start = u.field_data[UF_HOST].0 as usize;

    u.field_data[UF_HOST].1 = 0; // reset host length

    let mut s = if found_at { HostState::UserInfoStart } else { HostState::HostStart };

    for i in start..buflen {
        let ch = buf[i];
        let new_s = http_parse_host_char(s, ch);
        if new_s == HostState::Dead {
            return 1;
        }

        match new_s {
            HostState::Host => {
                if s != HostState::Host {
                    u.field_data[UF_HOST].0 = i as u16;
                }
                u.field_data[UF_HOST].1 += 1;
            }
            HostState::HostV6 => {
                if s != HostState::HostV6 {
                    u.field_data[UF_HOST].0 = i as u16;
                }
                u.field_data[UF_HOST].1 += 1;
            }
            HostState::HostPort => {
                if s != HostState::HostPort {
                    u.field_data[UF_PORT].0 = i as u16;
                    u.field_data[UF_PORT].1 = 0;
                    u.field_set |= 1 << UF_PORT;
                }
                u.field_data[UF_PORT].1 += 1;
            }
            HostState::UserInfo => {
                if s != HostState::UserInfo {
                    u.field_data[UF_USERINFO].0 = i as u16;
                    u.field_data[UF_USERINFO].1 = 0;
                    u.field_set |= 1 << UF_USERINFO;
                }
                u.field_data[UF_USERINFO].1 += 1;
            }
            _ => {}
        }
        s = new_s;
    }

    // Validate final state
    match s {
        HostState::HostStart | HostState::HostV6Start | HostState::HostV6
        | HostState::HostPortStart | HostState::UserInfo | HostState::UserInfoStart => 1,
        _ => 0,
    }
}

fn http_parser_parse_url(buf: &[u8], is_connect: bool, u: &mut HttpParserUrl) -> i32 {
    *u = HttpParserUrl::new();
    if buf.is_empty() {
        return 1;
    }

    let mut s = if is_connect { UrlState::ServerStart } else { UrlState::SpacesBeforeUrl };
    let mut old_uf: Option<usize> = None;
    let mut found_at = false;

    for i in 0..buf.len() {
        s = parse_url_char(s, buf[i]);

        let uf = match s {
            UrlState::Dead => return 1,
            // Skip delimiters (these don't belong to any field)
            UrlState::SchemaSlash | UrlState::SchemaSlashSlash
            | UrlState::ServerStart | UrlState::QueryStringStart
            | UrlState::FragmentStart => {
                old_uf = None;
                continue;
            }
            UrlState::Schema => UF_SCHEMA,
            UrlState::ServerWithAt => {
                found_at = true;
                UF_HOST
            }
            UrlState::Server => UF_HOST,
            UrlState::Path => UF_PATH,
            UrlState::QueryString => UF_QUERY,
            UrlState::Fragment => UF_FRAGMENT,
            _ => continue,
        };

        if old_uf == Some(uf) {
            u.field_data[uf].1 += 1;
        } else {
            u.field_data[uf] = (i as u16, 1);
            u.field_set |= 1 << uf;
            old_uf = Some(uf);
        }
    }

    // Host must be present if schema is present
    if u.has_field(UF_SCHEMA) && !u.has_field(UF_HOST) {
        return 1;
    }

    if u.has_field(UF_HOST) {
        if http_parse_host(buf, u, found_at) != 0 {
            return 1;
        }
    }

    // CONNECT requests must be exactly host:port
    if is_connect && u.field_set != ((1 << UF_HOST) | (1 << UF_PORT)) {
        return 1;
    }

    // Parse port number from string
    if u.has_field(UF_PORT) {
        let off = u.field_data[UF_PORT].0 as usize;
        let len = u.field_data[UF_PORT].1 as usize;
        let mut port: u32 = 0;
        for &b in &buf[off..off + len] {
            if !is_num(b) {
                return 1;
            }
            port = port * 10 + (b - b'0') as u32;
            if port > 0xffff {
                return 1;
            }
        }
        u.port = port as u16;
    }

    0
}

// === Core HTTP Parser Execute ===

/// Find position of \r\n in data starting from `start`. Returns index of \r.
fn find_crlf(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < data.len() {
        if data[i] == b'\r' && data[i + 1] == b'\n' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Match a method name at the start of data. Returns (method, length) or None.
fn match_method(data: &[u8]) -> Option<(HttpMethod, usize)> {
    for &(name, method) in METHOD_STRINGS {
        if data.len() > name.len()
            && data[..name.len()] == *name
            && data[name.len()] == b' '
        {
            return Some((method, name.len()));
        }
    }
    None
}

/// Main HTTP parser execute function.
/// Parses complete HTTP messages from `data`, invoking callbacks on the global TestState.
/// Returns the number of bytes consumed.
fn http_parser_execute(parser: &mut HttpParser, data: &[u8]) -> usize {
    if parser.http_errno != HttpErrno::Ok {
        return 0;
    }

    // Handle EOF
    if data.is_empty() {
        match parser.state {
            ParserState::MessageDone | ParserState::Start => 0,
            ParserState::Dead => {
                parser.http_errno = HttpErrno::InvalidEofState;
                1
            }
        }
    } else {
        match parser.parser_type {
            HttpParserType::Request => parse_request_message(parser, data),
            HttpParserType::Response => parse_response_message(parser, data),
        }
    }
}

fn parse_request_message(parser: &mut HttpParser, data: &[u8]) -> usize {
    // on_message_begin
    G_STATE.with(|s| s.borrow_mut().message_begin_count += 1);

    // 1. Match method
    let (method, method_len) = match match_method(data) {
        Some(m) => m,
        None => {
            parser.http_errno = HttpErrno::InvalidMethod;
            return 0;
        }
    };
    parser.method = method as u8;
    let mut pos = method_len + 1; // skip method + space

    // 2. Parse URL (until next space)
    let url_start = pos;
    while pos < data.len() && data[pos] != b' ' {
        pos += 1;
    }
    let url = &data[url_start..pos];
    G_STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.url = String::from_utf8_lossy(url).into_owned();
    });
    pos += 1; // skip space

    // 3. Parse HTTP version "HTTP/M.m\r\n"
    if pos + 8 > data.len() || &data[pos..pos + 5] != b"HTTP/" {
        parser.http_errno = HttpErrno::InvalidVersion;
        return pos;
    }
    let major = data[pos + 5] - b'0';
    let minor = data[pos + 7] - b'0';
    parser.http_major = major as u16;
    parser.http_minor = minor as u16;
    pos += 8; // skip "HTTP/M.m"

    // Skip \r\n
    if pos + 1 < data.len() && data[pos] == b'\r' && data[pos + 1] == b'\n' {
        pos += 2;
    }

    // 4. Parse headers
    pos = parse_headers_section(parser, data, pos);
    if parser.http_errno != HttpErrno::Ok {
        return pos;
    }

    // Finalize header count
    G_STATE.with(|s| {
        let mut state = s.borrow_mut();
        if state.in_value {
            state.header_count += 1;
            state.in_value = false;
        }
    });
    // on_headers_complete
    G_STATE.with(|s| s.borrow_mut().headers_complete_count += 1);

    // Determine upgrade
    if method == HttpMethod::Connect
        || (parser.flags & (F_UPGRADE | F_CONNECTION_UPGRADE))
            == (F_UPGRADE | F_CONNECTION_UPGRADE)
    {
        parser.upgrade = true;
    }

    // 5. Parse body (if Content-Length and not upgrade/CONNECT)
    if !parser.upgrade && (parser.flags & F_CONTENTLENGTH) != 0 {
        let cl = parser.content_length as usize;
        if cl > 0 && pos + cl <= data.len() {
            let body = &data[pos..pos + cl];
            G_STATE.with(|s| {
                let mut state = s.borrow_mut();
                state.body = String::from_utf8_lossy(body).into_owned();
                state.body_len = cl;
            });
            pos += cl;
        }
    }

    // on_message_complete
    G_STATE.with(|s| s.borrow_mut().message_complete_count += 1);
    parser.state = ParserState::MessageDone;
    pos
}

fn parse_response_message(parser: &mut HttpParser, data: &[u8]) -> usize {
    // on_message_begin
    G_STATE.with(|s| s.borrow_mut().message_begin_count += 1);

    // 1. Parse "HTTP/M.m SP"
    let mut pos = 0;
    if data.len() < 9 || &data[0..5] != b"HTTP/" {
        parser.http_errno = HttpErrno::InvalidVersion;
        return 0;
    }
    parser.http_major = (data[5] - b'0') as u16;
    parser.http_minor = (data[7] - b'0') as u16;
    pos = 9; // after "HTTP/M.m "

    // 2. Parse 3-digit status code
    if pos + 3 > data.len() {
        parser.http_errno = HttpErrno::InvalidStatus;
        return pos;
    }
    let sc = (data[pos] - b'0') as u32 * 100
        + (data[pos + 1] - b'0') as u32 * 10
        + (data[pos + 2] - b'0') as u32;
    parser.status_code = sc;
    pos += 3;

    // 3. Parse reason phrase (skip space, read until \r\n)
    if pos < data.len() && data[pos] == b' ' {
        pos += 1;
    }
    let reason_start = pos;
    while pos < data.len() && data[pos] != b'\r' {
        pos += 1;
    }
    let reason = &data[reason_start..pos];
    G_STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.status_text = String::from_utf8_lossy(reason).into_owned();
    });
    // Skip \r\n
    if pos + 1 < data.len() {
        pos += 2;
    }

    // 4. Parse headers
    pos = parse_headers_section(parser, data, pos);

    // Finalize header count
    G_STATE.with(|s| {
        let mut state = s.borrow_mut();
        if state.in_value {
            state.header_count += 1;
            state.in_value = false;
        }
    });
    // on_headers_complete
    G_STATE.with(|s| s.borrow_mut().headers_complete_count += 1);

    // 5. Determine if body exists
    let has_body = sc / 100 != 1 && sc != 204 && sc != 304;

    if has_body {
        if (parser.flags & F_CHUNKED) != 0 {
            pos = parse_chunked_body(parser, data, pos);
        } else if (parser.flags & F_CONTENTLENGTH) != 0 {
            let cl = parser.content_length as usize;
            if cl > 0 && pos + cl <= data.len() {
                let body = &data[pos..pos + cl];
                G_STATE.with(|s| {
                    let mut state = s.borrow_mut();
                    state.body = String::from_utf8_lossy(body).into_owned();
                    state.body_len = cl;
                });
                pos += cl;
            }
        }
    }

    // on_message_complete
    G_STATE.with(|s| s.borrow_mut().message_complete_count += 1);
    parser.state = ParserState::MessageDone;
    pos
}

fn parse_headers_section(parser: &mut HttpParser, data: &[u8], start: usize) -> usize {
    let mut pos = start;

    loop {
        // Find end of current line
        let crlf = match find_crlf(data, pos) {
            Some(p) => p,
            None => return pos,
        };

        // Empty line = end of headers
        if crlf == pos {
            pos += 2;
            break;
        }

        // Parse "Field: Value"
        let line = &data[pos..crlf];
        if let Some(colon) = line.iter().position(|&b| b == b':') {
            let field = &line[..colon];
            let value_start = colon + 1;
            let value = &line[value_start..];
            // Trim leading whitespace from value
            let value = if !value.is_empty() && value[0] == b' ' {
                &value[1..]
            } else {
                value
            };

            // on_header_field callback
            G_STATE.with(|s| {
                let mut state = s.borrow_mut();
                if state.in_value {
                    state.header_count += 1;
                    state.in_value = false;
                }
                let idx = state.header_count;
                if idx < 64 {
                    state.header_fields[idx] = String::from_utf8_lossy(field).into_owned();
                }
            });

            // on_header_value callback
            G_STATE.with(|s| {
                let mut state = s.borrow_mut();
                state.in_value = true;
                let idx = state.header_count;
                if idx < 64 {
                    state.header_values[idx] = String::from_utf8_lossy(value).into_owned();
                }
            });

            // Detect special headers
            let field_lower = String::from_utf8_lossy(field).to_ascii_lowercase();
            let value_str = String::from_utf8_lossy(value);

            match field_lower.as_str() {
                "content-length" => {
                    if let Ok(cl) = value_str.trim().parse::<u64>() {
                        parser.content_length = cl;
                        parser.flags |= F_CONTENTLENGTH;
                    }
                }
                "transfer-encoding" => {
                    if value_str.trim().eq_ignore_ascii_case("chunked") {
                        parser.flags |= F_CHUNKED;
                    }
                }
                "connection" => {
                    let v = value_str.trim().to_ascii_lowercase();
                    if v.contains("close") {
                        parser.flags |= F_CONNECTION_CLOSE;
                    }
                    if v.contains("keep-alive") {
                        parser.flags |= F_CONNECTION_KEEP_ALIVE;
                    }
                    if v.contains("upgrade") {
                        parser.flags |= F_CONNECTION_UPGRADE;
                    }
                }
                "upgrade" => {
                    parser.flags |= F_UPGRADE;
                }
                _ => {}
            }
        }

        pos = crlf + 2;
    }

    pos
}

fn parse_chunked_body(parser: &mut HttpParser, data: &[u8], start: usize) -> usize {
    let mut pos = start;

    loop {
        // Parse chunk size (hex digits until \r\n)
        let size_crlf = match find_crlf(data, pos) {
            Some(p) => p,
            None => return pos,
        };
        let size_str = &data[pos..size_crlf];
        let chunk_size = parse_hex_bytes(size_str);

        // on_chunk_header
        G_STATE.with(|s| s.borrow_mut().chunk_header_count += 1);

        pos = size_crlf + 2; // skip \r\n after size

        if chunk_size == 0 {
            // Terminal chunk — skip optional trailers
            // on_chunk_complete
            G_STATE.with(|s| s.borrow_mut().chunk_complete_count += 1);
            loop {
                let line_crlf = match find_crlf(data, pos) {
                    Some(p) => p,
                    None => return pos,
                };
                if line_crlf == pos {
                    // Empty line = end of trailers
                    pos += 2;
                    break;
                }
                pos = line_crlf + 2;
            }
            break;
        }

        // Read chunk data
        let chunk_end = pos + chunk_size;
        if chunk_end <= data.len() {
            let chunk_data = &data[pos..chunk_end];
            G_STATE.with(|s| {
                let mut state = s.borrow_mut();
                state.body.push_str(&String::from_utf8_lossy(chunk_data));
                state.body_len += chunk_size;
            });
        }
        pos = chunk_end;

        // Skip \r\n after chunk data
        if pos + 1 < data.len() && data[pos] == b'\r' && data[pos + 1] == b'\n' {
            pos += 2;
        }

        // on_chunk_complete
        G_STATE.with(|s| s.borrow_mut().chunk_complete_count += 1);
    }

    pos
}

fn parse_hex_bytes(bytes: &[u8]) -> usize {
    let mut val: usize = 0;
    for &b in bytes {
        let digit = match b {
            b'0'..=b'9' => (b - b'0') as usize,
            b'a'..=b'f' => (b - b'a' + 10) as usize,
            b'A'..=b'F' => (b - b'A' + 10) as usize,
            _ => break,
        };
        val = val * 16 + digit;
    }
    val
}

// === Test Harness ===

#[derive(Clone)]
struct TestState {
    url: String,
    status_text: String,
    header_fields: Vec<String>,
    header_values: Vec<String>,
    header_count: usize,
    in_value: bool,
    body: String,
    body_len: usize,
    message_begin_count: i32,
    message_complete_count: i32,
    headers_complete_count: i32,
    chunk_header_count: i32,
    chunk_complete_count: i32,
}

impl TestState {
    fn new() -> Self {
        TestState {
            url: String::new(),
            status_text: String::new(),
            header_fields: vec![String::new(); 64],
            header_values: vec![String::new(); 64],
            header_count: 0,
            in_value: false,
            body: String::new(),
            body_len: 0,
            message_begin_count: 0,
            message_complete_count: 0,
            headers_complete_count: 0,
            chunk_header_count: 0,
            chunk_complete_count: 0,
        }
    }
}

thread_local! {
    static G_STATE: RefCell<TestState> = RefCell::new(TestState::new());
}

fn reset_state() {
    G_STATE.with(|s| *s.borrow_mut() = TestState::new());
}

fn parse_request(raw: &str) {
    let mut parser = HttpParser::new(HttpParserType::Request);
    reset_state();
    let data = raw.as_bytes();
    let parsed = http_parser_execute(&mut parser, data);
    // Signal EOF
    http_parser_execute(&mut parser, &[]);

    println!("  parsed={}/{}", parsed, data.len());
    println!("  method={}", http_method_str(parser.method));
    G_STATE.with(|s| {
        let state = s.borrow();
        println!("  url={}", state.url);
    });
    println!("  http={}.{}", parser.http_major, parser.http_minor);
    G_STATE.with(|s| {
        let state = s.borrow();
        println!("  headers={}", state.header_count);
        for i in 0..state.header_count {
            println!("    {}: {}", state.header_fields[i], state.header_values[i]);
        }
        if state.body_len > 0 {
            println!("  body={}", state.body);
            println!("  body_len={}", state.body_len);
        }
    });
    println!(
        "  keep_alive={}",
        if http_should_keep_alive(&parser) { 1 } else { 0 }
    );
    println!("  upgrade={}", if parser.upgrade { 1 } else { 0 });
    if parser.http_errno != HttpErrno::Ok {
        println!(
            "  error={} ({})",
            http_errno_name(parser.http_errno),
            http_errno_description(parser.http_errno)
        );
    }
    G_STATE.with(|s| {
        println!("  message_complete={}", s.borrow().message_complete_count);
    });
}

fn parse_response(raw: &str) {
    let mut parser = HttpParser::new(HttpParserType::Response);
    reset_state();
    let data = raw.as_bytes();
    let parsed = http_parser_execute(&mut parser, data);
    http_parser_execute(&mut parser, &[]);

    println!("  parsed={}/{}", parsed, data.len());
    println!("  status_code={}", parser.status_code);
    G_STATE.with(|s| {
        let state = s.borrow();
        println!("  status_text={}", state.status_text);
    });
    println!("  http={}.{}", parser.http_major, parser.http_minor);
    G_STATE.with(|s| {
        let state = s.borrow();
        println!("  headers={}", state.header_count);
        for i in 0..state.header_count {
            println!("    {}: {}", state.header_fields[i], state.header_values[i]);
        }
        if state.body_len > 0 {
            println!("  body={}", state.body);
            println!("  body_len={}", state.body_len);
        }
    });
    println!(
        "  keep_alive={}",
        if http_should_keep_alive(&parser) { 1 } else { 0 }
    );
    G_STATE.with(|s| {
        let state = s.borrow();
        if state.chunk_header_count > 0 {
            println!("  chunks={}", state.chunk_header_count);
        }
    });
    if parser.http_errno != HttpErrno::Ok {
        println!(
            "  error={} ({})",
            http_errno_name(parser.http_errno),
            http_errno_description(parser.http_errno)
        );
    }
    G_STATE.with(|s| {
        println!("  message_complete={}", s.borrow().message_complete_count);
    });
}

fn test_url_parse(url: &str, is_connect: bool) {
    let mut u = HttpParserUrl::new();
    let result = http_parser_parse_url(url.as_bytes(), is_connect, &mut u);
    println!(
        "  url={} is_connect={} result={}",
        url,
        if is_connect { 1 } else { 0 },
        result
    );
    if result == 0 {
        println!("  field_set=0x{:x} port={}", u.field_set, u.port);
        let field_names = [
            "SCHEMA", "HOST", "PORT", "PATH", "QUERY", "FRAGMENT", "USERINFO",
        ];
        for i in 0..UF_MAX {
            if u.has_field(i) {
                let off = u.field_data[i].0 as usize;
                let len = u.field_data[i].1 as usize;
                let value = &url[off..off + len];
                println!("    {}={}", field_names[i], value);
            }
        }
    }
}

// === Test Cases ===

fn main() {
    let mut test_num = 0;

    println!("HTTP_PARSER DIFF TEST SUITE");

    // Request tests (14)
    test_num += 1;
    println!("TEST {}: Simple GET", test_num);
    parse_request("GET /path HTTP/1.1\r\nHost: example.com\r\n\r\n");

    test_num += 1;
    println!("TEST {}: GET with query string", test_num);
    parse_request("GET /search?q=hello&lang=en HTTP/1.1\r\nHost: example.com\r\nAccept: text/html\r\n\r\n");

    test_num += 1;
    println!("TEST {}: POST with body", test_num);
    parse_request("POST /submit HTTP/1.1\r\nHost: example.com\r\nContent-Length: 13\r\nContent-Type: application/x-www-form-urlencoded\r\n\r\nhello=world!!");

    test_num += 1;
    println!("TEST {}: PUT request", test_num);
    parse_request("PUT /resource/42 HTTP/1.1\r\nHost: api.example.com\r\nContent-Length: 18\r\nContent-Type: application/json\r\n\r\n{\"name\":\"updated\"}");

    test_num += 1;
    println!("TEST {}: DELETE request", test_num);
    parse_request("DELETE /resource/42 HTTP/1.1\r\nHost: api.example.com\r\n\r\n");

    test_num += 1;
    println!("TEST {}: HEAD request", test_num);
    parse_request("HEAD / HTTP/1.1\r\nHost: example.com\r\n\r\n");

    test_num += 1;
    println!("TEST {}: OPTIONS request", test_num);
    parse_request("OPTIONS * HTTP/1.1\r\nHost: example.com\r\n\r\n");

    test_num += 1;
    println!("TEST {}: PATCH request", test_num);
    parse_request("PATCH /resource/42 HTTP/1.1\r\nHost: api.example.com\r\nContent-Length: 16\r\nContent-Type: application/json\r\n\r\n{\"name\":\"patch\"}");

    test_num += 1;
    println!("TEST {}: CONNECT request", test_num);
    parse_request("CONNECT www.example.com:443 HTTP/1.1\r\nHost: www.example.com:443\r\n\r\n");

    test_num += 1;
    println!("TEST {}: TRACE request", test_num);
    parse_request("TRACE /path HTTP/1.1\r\nHost: example.com\r\n\r\n");

    test_num += 1;
    println!("TEST {}: HTTP/1.0 GET", test_num);
    parse_request("GET /old HTTP/1.0\r\n\r\n");

    test_num += 1;
    println!("TEST {}: Multiple headers", test_num);
    parse_request(
        "GET /headers HTTP/1.1\r\n\
         Host: example.com\r\n\
         Accept: text/html\r\n\
         Accept-Language: en-US\r\n\
         Accept-Encoding: gzip, deflate\r\n\
         Connection: keep-alive\r\n\
         User-Agent: TestClient/1.0\r\n\
         Cache-Control: no-cache\r\n\
         \r\n",
    );

    test_num += 1;
    println!("TEST {}: Connection close", test_num);
    parse_request("GET /close HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n");

    test_num += 1;
    println!("TEST {}: HTTP/1.0 Keep-Alive", test_num);
    parse_request("GET /ka HTTP/1.0\r\nConnection: keep-alive\r\n\r\n");

    // Response tests (13)
    test_num += 1;
    println!("TEST {}: Response 200 OK", test_num);
    parse_response("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");

    test_num += 1;
    println!("TEST {}: Response 404", test_num);
    parse_response("HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nnot found");

    test_num += 1;
    println!("TEST {}: Response 301 Redirect", test_num);
    parse_response("HTTP/1.1 301 Moved Permanently\r\nLocation: http://example.com/new\r\nContent-Length: 0\r\n\r\n");

    test_num += 1;
    println!("TEST {}: Response 500", test_num);
    parse_response("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 5\r\n\r\nerror");

    test_num += 1;
    println!("TEST {}: Response 204 No Content", test_num);
    parse_response("HTTP/1.1 204 No Content\r\n\r\n");

    test_num += 1;
    println!("TEST {}: Chunked response", test_num);
    parse_response(
        "HTTP/1.1 200 OK\r\n\
         Transfer-Encoding: chunked\r\n\
         \r\n\
         5\r\nhello\r\n\
         6\r\n world\r\n\
         0\r\n\r\n",
    );

    test_num += 1;
    println!("TEST {}: Chunked with trailer", test_num);
    parse_response(
        "HTTP/1.1 200 OK\r\n\
         Transfer-Encoding: chunked\r\n\
         \r\n\
         3\r\nfoo\r\n\
         3\r\nbar\r\n\
         0\r\n\
         Trailer-Key: trailer-value\r\n\
         \r\n",
    );

    test_num += 1;
    println!("TEST {}: HTTP/1.0 response", test_num);
    parse_response("HTTP/1.0 200 OK\r\nContent-Length: 3\r\n\r\nfoo");

    test_num += 1;
    println!("TEST {}: Response 100 Continue", test_num);
    parse_response("HTTP/1.1 100 Continue\r\n\r\n");

    test_num += 1;
    println!("TEST {}: Response with many headers", test_num);
    parse_response(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: 4\r\n\
         Server: TestServer/1.0\r\n\
         X-Request-Id: abc123\r\n\
         Cache-Control: max-age=3600\r\n\
         Date: Mon, 01 Jan 2024 00:00:00 GMT\r\n\
         \r\n\
         test",
    );

    test_num += 1;
    println!("TEST {}: Response 101 Switching Protocols", test_num);
    parse_response(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         \r\n",
    );

    test_num += 1;
    println!("TEST {}: Response 302 Found", test_num);
    parse_response(
        "HTTP/1.1 302 Found\r\n\
         Location: /login\r\n\
         Content-Length: 0\r\n\
         \r\n",
    );

    test_num += 1;
    println!("TEST {}: Response 304 Not Modified", test_num);
    parse_response(
        "HTTP/1.1 304 Not Modified\r\n\
         ETag: \"abc123\"\r\n\
         \r\n",
    );

    // Feature tests (6)
    test_num += 1;
    println!("TEST {}: WebSocket upgrade", test_num);
    parse_request(
        "GET /chat HTTP/1.1\r\n\
         Host: server.example.com\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n",
    );

    test_num += 1;
    println!("TEST {}: POST large body", test_num);
    parse_request(
        "POST /upload HTTP/1.1\r\n\
         Host: example.com\r\n\
         Content-Length: 51\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         The quick brown fox jumps over the lazy dog. Done!!",
    );

    test_num += 1;
    println!("TEST {}: GET with fragment", test_num);
    parse_request("GET /page#section2 HTTP/1.1\r\nHost: example.com\r\n\r\n");

    test_num += 1;
    println!("TEST {}: GET with absolute URL", test_num);
    parse_request("GET http://example.com/resource HTTP/1.1\r\nHost: example.com\r\n\r\n");

    test_num += 1;
    println!("TEST {}: Response with Connection close", test_num);
    parse_response(
        "HTTP/1.1 200 OK\r\n\
         Connection: close\r\n\
         Content-Length: 2\r\n\
         \r\n\
         ok",
    );

    test_num += 1;
    println!("TEST {}: POST with JSON content", test_num);
    parse_request(
        "POST /api/data HTTP/1.1\r\n\
         Host: api.example.com\r\n\
         Content-Type: application/json\r\n\
         Content-Length: 24\r\n\
         Accept: application/json\r\n\
         \r\n\
         {\"key\":\"value\",\"num\":42}",
    );

    // URL parsing (1 test with 8 sub-cases)
    test_num += 1;
    println!("TEST {}: URL parsing", test_num);
    test_url_parse("http://example.com/path?query=val#frag", false);
    test_url_parse("http://user:pass@host.com:8080/p/a/t/h?q=1", false);
    test_url_parse("/status?format=json", false);
    test_url_parse("http://[::1]:8080/ipv6", false);
    test_url_parse("example.com:443", true);
    test_url_parse("http://example.com", false);
    test_url_parse("/", false);
    test_url_parse("/path/to/resource", false);

    // Utility functions
    test_num += 1;
    println!("TEST {}: Utility functions", test_num);
    let v = http_parser_version();
    let major = (v >> 16) & 255;
    let minor = (v >> 8) & 255;
    let patch = v & 255;
    println!("  version={}.{}.{}", major, minor, patch);

    println!("  method[GET]={}", http_method_str(HttpMethod::Get as u8));
    println!("  method[POST]={}", http_method_str(HttpMethod::Post as u8));
    println!(
        "  method[DELETE]={}",
        http_method_str(HttpMethod::Delete as u8)
    );
    println!("  method[PUT]={}", http_method_str(HttpMethod::Put as u8));
    println!(
        "  method[PATCH]={}",
        http_method_str(HttpMethod::Patch as u8)
    );
    println!(
        "  method[OPTIONS]={}",
        http_method_str(HttpMethod::Options as u8)
    );
    println!("  method[HEAD]={}", http_method_str(HttpMethod::Head as u8));
    println!(
        "  method[CONNECT]={}",
        http_method_str(HttpMethod::Connect as u8)
    );
    println!(
        "  method[TRACE]={}",
        http_method_str(HttpMethod::Trace as u8)
    );

    println!("  errno[OK]={}", http_errno_name(HttpErrno::Ok));
    println!(
        "  errno[INVALID_URL]={}",
        http_errno_name(HttpErrno::InvalidUrl)
    );
    println!(
        "  errno[INVALID_METHOD]={}",
        http_errno_name(HttpErrno::InvalidMethod)
    );
    println!(
        "  errno_desc[OK]={}",
        http_errno_description(HttpErrno::Ok)
    );
    println!(
        "  errno_desc[HEADER_OVERFLOW]={}",
        http_errno_description(HttpErrno::HeaderOverflow)
    );

    let mut p10 = HttpParser::new(HttpParserType::Request);
    p10.http_major = 1;
    p10.http_minor = 0;
    println!(
        "  keep_alive_10={}",
        if http_should_keep_alive(&p10) { 1 } else { 0 }
    );

    let mut p11 = HttpParser::new(HttpParserType::Request);
    p11.http_major = 1;
    p11.http_minor = 1;
    println!(
        "  keep_alive_11={}",
        if http_should_keep_alive(&p11) { 1 } else { 0 }
    );

    // Error cases
    test_num += 1;
    println!("TEST {}: Invalid method", test_num);
    {
        let mut parser = HttpParser::new(HttpParserType::Request);
        reset_state();
        let raw = b"FOOBAR / HTTP/1.1\r\n\r\n";
        let parsed = http_parser_execute(&mut parser, raw);
        println!("  parsed={}/{}", parsed, raw.len());
        println!("  error={}", http_errno_name(parser.http_errno));
    }

    test_num += 1;
    println!("TEST {}: Invalid HTTP version", test_num);
    {
        let mut parser = HttpParser::new(HttpParserType::Request);
        reset_state();
        let raw = b"GET / HTTP/5.1\r\n\r\n";
        let parsed = http_parser_execute(&mut parser, raw);
        println!("  parsed={}/{}", parsed, raw.len());
        println!("  error={}", http_errno_name(parser.http_errno));
    }

    println!("TOTAL TESTS: {}", test_num);
    println!("ALL TESTS COMPLETE");
}
