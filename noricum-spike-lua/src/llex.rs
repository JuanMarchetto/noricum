//! llex — the Lua lexer / scanner.
//!
//! Port of Lua 5.4/5.5's `llex.c` (~600 LOC C). Tokenizes a byte
//! stream into Lua tokens: reserved words, names, numbers, strings,
//! and single-character operators.

#![allow(dead_code)]

use crate::contract::{GlobalState, LuaInteger, LuaNumber, StringHandle};

/// Single-char tokens are their own code. Multi-char / reserved
/// tokens start at `FIRST_RESERVED = 257`.
pub const FIRST_RESERVED: i32 = 257;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reserved {
    TkAnd = FIRST_RESERVED,
    TkBreak,
    TkDo,
    TkElse,
    TkElseif,
    TkEnd,
    TkFalse,
    TkFor,
    TkFunction,
    TkGoto,
    TkIf,
    TkIn,
    TkLocal,
    TkNil,
    TkNot,
    TkOr,
    TkRepeat,
    TkReturn,
    TkThen,
    TkTrue,
    TkUntil,
    TkWhile,
    // Other terminal symbols
    TkIdiv,
    TkConcat,
    TkDots,
    TkEq,
    TkGe,
    TkLe,
    TkNe,
    TkShl,
    TkShr,
    TkDbcolon,
    TkEos,
    TkFlt,
    TkInt,
    TkName,
    TkString,
}

pub const TK_AND: i32 = Reserved::TkAnd as i32;
pub const TK_BREAK: i32 = Reserved::TkBreak as i32;
pub const TK_DO: i32 = Reserved::TkDo as i32;
pub const TK_ELSE: i32 = Reserved::TkElse as i32;
pub const TK_ELSEIF: i32 = Reserved::TkElseif as i32;
pub const TK_END: i32 = Reserved::TkEnd as i32;
pub const TK_FALSE: i32 = Reserved::TkFalse as i32;
pub const TK_FOR: i32 = Reserved::TkFor as i32;
pub const TK_FUNCTION: i32 = Reserved::TkFunction as i32;
pub const TK_GOTO: i32 = Reserved::TkGoto as i32;
pub const TK_IF: i32 = Reserved::TkIf as i32;
pub const TK_IN: i32 = Reserved::TkIn as i32;
pub const TK_LOCAL: i32 = Reserved::TkLocal as i32;
pub const TK_NIL: i32 = Reserved::TkNil as i32;
pub const TK_NOT: i32 = Reserved::TkNot as i32;
pub const TK_OR: i32 = Reserved::TkOr as i32;
pub const TK_REPEAT: i32 = Reserved::TkRepeat as i32;
pub const TK_RETURN: i32 = Reserved::TkReturn as i32;
pub const TK_THEN: i32 = Reserved::TkThen as i32;
pub const TK_TRUE: i32 = Reserved::TkTrue as i32;
pub const TK_UNTIL: i32 = Reserved::TkUntil as i32;
pub const TK_WHILE: i32 = Reserved::TkWhile as i32;
pub const TK_IDIV: i32 = Reserved::TkIdiv as i32;
pub const TK_CONCAT: i32 = Reserved::TkConcat as i32;
pub const TK_DOTS: i32 = Reserved::TkDots as i32;
pub const TK_EQ: i32 = Reserved::TkEq as i32;
pub const TK_GE: i32 = Reserved::TkGe as i32;
pub const TK_LE: i32 = Reserved::TkLe as i32;
pub const TK_NE: i32 = Reserved::TkNe as i32;
pub const TK_SHL: i32 = Reserved::TkShl as i32;
pub const TK_SHR: i32 = Reserved::TkShr as i32;
pub const TK_DBCOLON: i32 = Reserved::TkDbcolon as i32;
pub const TK_EOS: i32 = Reserved::TkEos as i32;
pub const TK_FLT: i32 = Reserved::TkFlt as i32;
pub const TK_INT: i32 = Reserved::TkInt as i32;
pub const TK_NAME: i32 = Reserved::TkName as i32;
pub const TK_STRING: i32 = Reserved::TkString as i32;

pub const NUM_RESERVED: usize = (TK_WHILE - FIRST_RESERVED + 1) as usize;

const RESERVED_WORDS: [&str; NUM_RESERVED] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for",
    "function", "goto", "if", "in", "local", "nil", "not",
    "or", "repeat", "return", "then", "true", "until", "while",
];

const EOZ: i32 = -1;

/// Semantic information attached to a token.
#[derive(Debug, Clone)]
pub enum SemInfo {
    None,
    Integer(LuaInteger),
    Float(LuaNumber),
    String(StringHandle),
}

/// A single token produced by the lexer.
#[derive(Debug, Clone)]
pub struct Token {
    pub token: i32,
    pub seminfo: SemInfo,
}

impl Token {
    pub fn eos() -> Self {
        Token {
            token: TK_EOS,
            seminfo: SemInfo::None,
        }
    }
}

/// Lexer state.
pub struct LexState<'a> {
    pub current: i32,
    pub linenumber: i32,
    pub lastline: i32,
    pub t: Token,
    pub lookahead: Token,
    source: &'a [u8],
    pos: usize,
    buff: Vec<u8>,
    pub gs: *mut GlobalState,
    pub source_name: StringHandle,
}

impl<'a> LexState<'a> {
    pub fn new(
        gs: &mut GlobalState,
        source: &'a [u8],
        source_name: StringHandle,
    ) -> Self {
        let first = if source.is_empty() {
            EOZ
        } else {
            source[0] as i32
        };
        LexState {
            current: first,
            linenumber: 1,
            lastline: 1,
            t: Token::eos(),
            lookahead: Token::eos(),
            source,
            pos: 1,
            buff: Vec::with_capacity(32),
            gs: gs as *mut GlobalState,
            source_name,
        }
    }

    fn gs_mut(&mut self) -> &mut GlobalState {
        unsafe { &mut *self.gs }
    }

    fn gs_ref(&self) -> &GlobalState {
        unsafe { &*self.gs }
    }

    fn next(&mut self) {
        if self.pos < self.source.len() {
            self.current = self.source[self.pos] as i32;
            self.pos += 1;
        } else {
            self.current = EOZ;
        }
    }

    fn save(&mut self, c: u8) {
        self.buff.push(c);
    }

    fn save_and_next(&mut self) {
        self.save(self.current as u8);
        self.next();
    }

    fn is_newline(&self) -> bool {
        self.current == b'\n' as i32 || self.current == b'\r' as i32
    }

    fn inc_line_number(&mut self) {
        let old = self.current;
        self.next();
        if self.is_newline() && self.current != old {
            self.next();
        }
        self.linenumber += 1;
        if self.linenumber >= i32::MAX - 1 {
            self.lex_error("chunk has too many lines");
        }
    }

    fn lex_error(&self, msg: &str) -> ! {
        panic!(
            "{}:{}: {}",
            self.source_name_str(),
            self.linenumber,
            msg
        );
    }

    fn source_name_str(&self) -> String {
        let gs = self.gs_ref();
        let bytes = &gs.heap.string(self.source_name).bytes;
        String::from_utf8_lossy(bytes).to_string()
    }

    fn check_next1(&mut self, c: i32) -> bool {
        if self.current == c {
            self.next();
            true
        } else {
            false
        }
    }

    fn intern_buffer(&mut self) -> StringHandle {
        let buf_copy = self.buff.clone();
        let gs = self.gs_mut();
        let seed = gs.hash_seed;
        gs.intern_short(&buf_copy, seed)
    }

    fn new_string(&mut self, bytes: &[u8]) -> StringHandle {
        let gs = self.gs_mut();
        let seed = gs.hash_seed;
        gs.new_string(bytes, seed)
    }

    fn read_numeral(&mut self) -> Token {
        let first = self.current;
        self.save_and_next();
        let is_hex = first == b'0' as i32
            && (self.current == b'x' as i32 || self.current == b'X' as i32);
        if is_hex {
            self.save_and_next();
        }
        let expo = if is_hex { b'p' } else { b'e' };
        loop {
            let c = self.current;
            if c == expo as i32 || c == expo.to_ascii_uppercase() as i32 {
                self.save_and_next();
                if self.current == b'+' as i32 || self.current == b'-' as i32 {
                    self.save_and_next();
                }
            } else if c >= 0
                && ((c as u8).is_ascii_hexdigit() || c == b'.' as i32)
            {
                self.save_and_next();
            } else {
                break;
            }
        }
        if self.current >= 0 && (self.current as u8).is_ascii_alphabetic() {
            self.save_and_next();
        }
        let s = std::str::from_utf8(&self.buff).unwrap_or("");
        if let Some(i) = Self::parse_lua_integer(s) {
            Token {
                token: TK_INT,
                seminfo: SemInfo::Integer(i),
            }
        } else if let Ok(f) = Self::parse_lua_float(s) {
            Token {
                token: TK_FLT,
                seminfo: SemInfo::Float(f),
            }
        } else {
            self.lex_error("malformed number");
        }
    }

    fn parse_lua_integer(s: &str) -> Option<LuaInteger> {
        let s = s.trim();
        if s.starts_with("0x") || s.starts_with("0X") {
            i64::from_str_radix(&s[2..], 16).ok()
        } else {
            s.parse::<i64>().ok()
        }
    }

    fn parse_lua_float(s: &str) -> Result<LuaNumber, ()> {
        let s = s.trim();
        if s.starts_with("0x") || s.starts_with("0X") {
            Self::parse_hex_float(s).ok_or(())
        } else {
            s.parse::<f64>().map_err(|_| ())
        }
    }

    fn parse_hex_float(s: &str) -> Option<f64> {
        let s = &s[2..]; // skip 0x
        let mut result: f64 = 0.0;
        let mut frac = false;
        let mut frac_div: f64 = 1.0;
        let mut exp: i32 = 0;
        let mut has_exp = false;
        let mut chars = s.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c == '.' {
                frac = true;
                chars.next();
                continue;
            }
            if c == 'p' || c == 'P' {
                chars.next();
                has_exp = true;
                let neg = match chars.peek() {
                    Some('+') => { chars.next(); false }
                    Some('-') => { chars.next(); true }
                    _ => false,
                };
                let e_str: String = chars.collect();
                exp = e_str.parse::<i32>().ok()?;
                if neg { exp = -exp; }
                break;
            }
            let d = c.to_digit(16)? as f64;
            if frac {
                frac_div *= 16.0;
                result += d / frac_div;
            } else {
                result = result * 16.0 + d;
            }
            chars.next();
        }
        if has_exp {
            result *= (2.0f64).powi(exp);
        }
        Some(result)
    }

    fn skip_sep(&mut self) -> usize {
        let s = self.current;
        self.save_and_next();
        let mut count = 0usize;
        while self.current == b'=' as i32 {
            self.save_and_next();
            count += 1;
        }
        if self.current == s {
            count + 2
        } else if count == 0 {
            1
        } else {
            0
        }
    }

    fn read_long_string(&mut self, is_string: bool, sep: usize) {
        self.save_and_next(); // skip 2nd '['
        if self.is_newline() {
            self.inc_line_number();
        }
        loop {
            match self.current {
                c if c == EOZ => {
                    let what = if is_string { "string" } else { "comment" };
                    self.lex_error(&format!("unfinished long {}", what));
                }
                c if c == b']' as i32 => {
                    let old_len = self.buff.len();
                    if self.skip_sep() == sep {
                        self.save_and_next();
                        return;
                    } else {
                        // skip_sep added chars to buff; they stay.
                        let _ = old_len;
                    }
                }
                c if c == b'\n' as i32 || c == b'\r' as i32 => {
                    self.save(b'\n');
                    self.inc_line_number();
                    if !is_string {
                        self.buff.clear();
                    }
                }
                _ => {
                    if is_string {
                        self.save_and_next();
                    } else {
                        self.next();
                    }
                }
            }
        }
    }

    fn read_string(&mut self, del: i32) {
        self.save_and_next(); // keep delimiter
        while self.current != del {
            match self.current {
                c if c == EOZ => self.lex_error("unfinished string"),
                c if c == b'\n' as i32 || c == b'\r' as i32 => {
                    self.lex_error("unfinished string")
                }
                c if c == b'\\' as i32 => {
                    self.save_and_next(); // keep '\\'
                    let resolved = match self.current {
                        c if c == b'a' as i32 => { self.next(); Some(b'\x07') }
                        c if c == b'b' as i32 => { self.next(); Some(b'\x08') }
                        c if c == b'f' as i32 => { self.next(); Some(b'\x0c') }
                        c if c == b'n' as i32 => { self.next(); Some(b'\n') }
                        c if c == b'r' as i32 => { self.next(); Some(b'\r') }
                        c if c == b't' as i32 => { self.next(); Some(b'\t') }
                        c if c == b'v' as i32 => { self.next(); Some(b'\x0b') }
                        c if c == b'\\' as i32
                            || c == b'"' as i32
                            || c == b'\'' as i32 =>
                        {
                            let ch = c as u8;
                            self.next();
                            Some(ch)
                        }
                        c if c == b'\n' as i32 || c == b'\r' as i32 => {
                            self.inc_line_number();
                            Some(b'\n')
                        }
                        c if c == b'x' as i32 => {
                            self.next();
                            let h1 = self.hex_digit();
                            let h2 = self.hex_digit();
                            Some(h1 << 4 | h2)
                        }
                        c if c == b'z' as i32 => {
                            self.buff.pop(); // remove '\\'
                            self.next(); // skip 'z'
                            while self.current >= 0
                                && (self.current as u8).is_ascii_whitespace()
                            {
                                if self.is_newline() {
                                    self.inc_line_number();
                                } else {
                                    self.next();
                                }
                            }
                            None
                        }
                        c if c >= 0 && (c as u8).is_ascii_digit() => {
                            let mut r = 0u32;
                            for _ in 0..3 {
                                if self.current >= 0
                                    && (self.current as u8).is_ascii_digit()
                                {
                                    r = r * 10
                                        + (self.current as u8 - b'0') as u32;
                                    self.next();
                                } else {
                                    break;
                                }
                            }
                            if r > 255 {
                                self.lex_error("decimal escape too large");
                            }
                            // Remove the '\\' we saved.
                            self.buff.pop();
                            Some(r as u8)
                        }
                        _ => None,
                    };
                    if let Some(ch) = resolved {
                        // Remove the saved '\\' and save resolved.
                        if self.buff.last() == Some(&b'\\') {
                            self.buff.pop();
                        }
                        self.save(ch);
                    }
                }
                _ => self.save_and_next(),
            }
        }
        self.save_and_next(); // skip closing delimiter
    }

    fn hex_digit(&mut self) -> u8 {
        if self.current < 0 {
            self.lex_error("hexadecimal digit expected");
        }
        let c = self.current as u8;
        self.next();
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => self.lex_error("hexadecimal digit expected"),
        }
    }

    fn llex(&mut self) -> Token {
        self.buff.clear();
        loop {
            match self.current {
                c if c == b'\n' as i32 || c == b'\r' as i32 => {
                    self.inc_line_number();
                }
                c if c == b' ' as i32
                    || c == b'\t' as i32
                    || c == b'\x0c' as i32
                    || c == b'\x0b' as i32 =>
                {
                    self.next();
                }
                c if c == b'-' as i32 => {
                    self.next();
                    if self.current != b'-' as i32 {
                        return Token {
                            token: b'-' as i32,
                            seminfo: SemInfo::None,
                        };
                    }
                    self.next();
                    if self.current == b'[' as i32 {
                        let sep = self.skip_sep();
                        self.buff.clear();
                        if sep >= 2 {
                            self.read_long_string(false, sep);
                            self.buff.clear();
                            continue;
                        }
                    }
                    while !self.is_newline() && self.current != EOZ {
                        self.next();
                    }
                }
                c if c == b'[' as i32 => {
                    let sep = self.skip_sep();
                    if sep >= 2 {
                        self.read_long_string(true, sep);
                        let content = self.buff[sep..self.buff.len() - sep].to_vec();
                        let h = self.new_string(&content);
                        return Token {
                            token: TK_STRING,
                            seminfo: SemInfo::String(h),
                        };
                    } else if sep == 0 {
                        self.lex_error("invalid long string delimiter");
                    }
                    return Token {
                        token: b'[' as i32,
                        seminfo: SemInfo::None,
                    };
                }
                c if c == b'=' as i32 => {
                    self.next();
                    if self.check_next1(b'=' as i32) {
                        return Token { token: TK_EQ, seminfo: SemInfo::None };
                    }
                    return Token { token: b'=' as i32, seminfo: SemInfo::None };
                }
                c if c == b'<' as i32 => {
                    self.next();
                    if self.check_next1(b'=' as i32) {
                        return Token { token: TK_LE, seminfo: SemInfo::None };
                    } else if self.check_next1(b'<' as i32) {
                        return Token { token: TK_SHL, seminfo: SemInfo::None };
                    }
                    return Token { token: b'<' as i32, seminfo: SemInfo::None };
                }
                c if c == b'>' as i32 => {
                    self.next();
                    if self.check_next1(b'=' as i32) {
                        return Token { token: TK_GE, seminfo: SemInfo::None };
                    } else if self.check_next1(b'>' as i32) {
                        return Token { token: TK_SHR, seminfo: SemInfo::None };
                    }
                    return Token { token: b'>' as i32, seminfo: SemInfo::None };
                }
                c if c == b'/' as i32 => {
                    self.next();
                    if self.check_next1(b'/' as i32) {
                        return Token { token: TK_IDIV, seminfo: SemInfo::None };
                    }
                    return Token { token: b'/' as i32, seminfo: SemInfo::None };
                }
                c if c == b'~' as i32 => {
                    self.next();
                    if self.check_next1(b'=' as i32) {
                        return Token { token: TK_NE, seminfo: SemInfo::None };
                    }
                    return Token { token: b'~' as i32, seminfo: SemInfo::None };
                }
                c if c == b':' as i32 => {
                    self.next();
                    if self.check_next1(b':' as i32) {
                        return Token { token: TK_DBCOLON, seminfo: SemInfo::None };
                    }
                    return Token { token: b':' as i32, seminfo: SemInfo::None };
                }
                c if c == b'"' as i32 || c == b'\'' as i32 => {
                    self.read_string(c);
                    let content = self.buff[1..self.buff.len() - 1].to_vec();
                    let h = self.new_string(&content);
                    return Token {
                        token: TK_STRING,
                        seminfo: SemInfo::String(h),
                    };
                }
                c if c == b'.' as i32 => {
                    self.save_and_next();
                    if self.check_next1(b'.' as i32) {
                        if self.check_next1(b'.' as i32) {
                            return Token { token: TK_DOTS, seminfo: SemInfo::None };
                        }
                        return Token { token: TK_CONCAT, seminfo: SemInfo::None };
                    }
                    if self.current < 0 || !(self.current as u8).is_ascii_digit() {
                        return Token { token: b'.' as i32, seminfo: SemInfo::None };
                    }
                    return self.read_numeral();
                }
                c if c >= b'0' as i32 && c <= b'9' as i32 => {
                    self.buff.clear();
                    return self.read_numeral();
                }
                c if c == EOZ => {
                    return Token::eos();
                }
                _ => {
                    if self.current >= 0
                        && ((self.current as u8).is_ascii_alphabetic()
                            || self.current == b'_' as i32)
                    {
                        // Identifier or reserved word.
                        loop {
                            self.save_and_next();
                            if self.current < 0 {
                                break;
                            }
                            let ch = self.current as u8;
                            if !ch.is_ascii_alphanumeric() && ch != b'_' {
                                break;
                            }
                        }
                        let word = std::str::from_utf8(&self.buff).unwrap_or("");
                        for (i, &rw) in RESERVED_WORDS.iter().enumerate() {
                            if word == rw {
                                return Token {
                                    token: FIRST_RESERVED + i as i32,
                                    seminfo: SemInfo::None,
                                };
                            }
                        }
                        let h = self.new_string(&self.buff.clone());
                        return Token {
                            token: TK_NAME,
                            seminfo: SemInfo::String(h),
                        };
                    } else {
                        let c = self.current;
                        self.next();
                        return Token {
                            token: c,
                            seminfo: SemInfo::None,
                        };
                    }
                }
            }
        }
    }

    pub fn next_token(&mut self) {
        self.lastline = self.linenumber;
        if self.lookahead.token != TK_EOS {
            self.t = self.lookahead.clone();
            self.lookahead = Token::eos();
        } else {
            self.t = self.llex();
        }
    }

    pub fn lookahead_token(&mut self) -> i32 {
        assert_eq!(self.lookahead.token, TK_EOS);
        self.lookahead = self.llex();
        self.lookahead.token
    }

    pub fn syntax_error(&self, msg: &str) -> ! {
        self.lex_error(msg);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::GlobalState;

    fn lex_all(source: &str) -> Vec<Token> {
        let mut gs = GlobalState::default();
        gs.init_metamethod_names();
        let name = gs.new_string(b"=test", 0);
        let mut ls = LexState::new(&mut gs, source.as_bytes(), name);
        let mut tokens = Vec::new();
        loop {
            ls.next_token();
            if ls.t.token == TK_EOS {
                break;
            }
            tokens.push(ls.t.clone());
        }
        tokens
    }

    #[test]
    fn empty_source_yields_no_tokens() {
        let tokens = lex_all("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn reserved_words_tokenize_correctly() {
        let tokens = lex_all("if then else end while do");
        assert_eq!(tokens.len(), 6);
        assert_eq!(tokens[0].token, TK_IF);
        assert_eq!(tokens[1].token, TK_THEN);
        assert_eq!(tokens[2].token, TK_ELSE);
        assert_eq!(tokens[3].token, TK_END);
        assert_eq!(tokens[4].token, TK_WHILE);
        assert_eq!(tokens[5].token, TK_DO);
    }

    #[test]
    fn integer_literal_parsed() {
        let tokens = lex_all("42");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_INT);
        assert!(matches!(tokens[0].seminfo, SemInfo::Integer(42)));
    }

    #[test]
    fn hex_integer_parsed() {
        let tokens = lex_all("0xFF");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_INT);
        assert!(matches!(tokens[0].seminfo, SemInfo::Integer(255)));
    }

    #[test]
    fn float_literal_parsed() {
        let tokens = lex_all("3.14");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_FLT);
        match tokens[0].seminfo {
            #[allow(clippy::approx_constant)]
            SemInfo::Float(f) => assert!((f - 3.14_f64).abs() < 1e-10),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn string_literal_parsed() {
        let tokens = lex_all(r#""hello""#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_STRING);
    }

    #[test]
    fn operators_tokenize() {
        let tokens = lex_all("== ~= <= >= << >> // .. ...");
        assert_eq!(tokens.len(), 9);
        assert_eq!(tokens[0].token, TK_EQ);
        assert_eq!(tokens[1].token, TK_NE);
        assert_eq!(tokens[2].token, TK_LE);
        assert_eq!(tokens[3].token, TK_GE);
        assert_eq!(tokens[4].token, TK_SHL);
        assert_eq!(tokens[5].token, TK_SHR);
        assert_eq!(tokens[6].token, TK_IDIV);
        assert_eq!(tokens[7].token, TK_CONCAT);
        assert_eq!(tokens[8].token, TK_DOTS);
    }

    #[test]
    fn single_char_operators() {
        let tokens = lex_all("+-*/%^#&|~(){}[];,");
        assert_eq!(tokens.len(), 18);
        assert_eq!(tokens[0].token, b'+' as i32);
        assert_eq!(tokens[1].token, b'-' as i32);
    }

    #[test]
    fn comment_skipped() {
        let tokens = lex_all("-- this is a comment\n42");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_INT);
    }

    #[test]
    fn long_comment_skipped() {
        let tokens = lex_all("--[[ multi\nline\ncomment ]]42");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_INT);
    }

    #[test]
    fn name_token_for_identifier() {
        let tokens = lex_all("foobar");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_NAME);
    }

    #[test]
    fn dots_token() {
        let tokens = lex_all("...");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_DOTS);
    }

    #[test]
    fn line_counting() {
        let mut gs = GlobalState::default();
        gs.init_metamethod_names();
        let name = gs.new_string(b"=test", 0);
        let mut ls = LexState::new(&mut gs, b"a\nb\nc", name);
        ls.next_token(); // a (line 1)
        assert_eq!(ls.linenumber, 1);
        ls.next_token(); // b (line 2)
        assert_eq!(ls.linenumber, 2);
        ls.next_token(); // c (line 3)
        assert_eq!(ls.linenumber, 3);
    }

    #[test]
    fn escape_in_string() {
        let tokens = lex_all(r#""\n\t\\""#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_STRING);
    }

    #[test]
    fn dbcolon_token() {
        let tokens = lex_all("::");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, TK_DBCOLON);
    }

    #[test]
    fn complete_lua_statement_tokenizes() {
        let src = "local x = 10 + 20\nif x > 15 then print(x) end";
        let tokens = lex_all(src);
        assert_eq!(tokens[0].token, TK_LOCAL);
        assert_eq!(tokens[1].token, TK_NAME);
        assert_eq!(tokens[2].token, b'=' as i32);
        assert_eq!(tokens[3].token, TK_INT);
        assert_eq!(tokens[4].token, b'+' as i32);
        assert_eq!(tokens[5].token, TK_INT);
        assert_eq!(tokens[6].token, TK_IF);
    }
}
