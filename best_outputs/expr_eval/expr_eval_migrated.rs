use std::collections::HashMap;
use std::fmt;

// ========== Token types ==========

#[derive(Debug, Clone, Copy, PartialEq)]
enum TokenType {
    Number,
    String,
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Comma,
    Assign,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
    Not,
    Semicolon,
    Let,
    If,
    Else,
    While,
    Eof,
    Error,
}

#[derive(Debug, Clone)]
struct Token {
    typ: TokenType,
    num_val: f64,
    str_val: String,
}

impl Token {
    fn new(typ: TokenType) -> Self {
        Token {
            typ,
            num_val: 0.0,
            str_val: String::new(),
        }
    }

    fn number(val: f64) -> Self {
        Token {
            typ: TokenType::Number,
            num_val: val,
            str_val: String::new(),
        }
    }

    fn string(s: &str) -> Self {
        Token {
            typ: TokenType::String,
            num_val: 0.0,
            str_val: s.to_string(),
        }
    }

    fn ident(s: &str) -> Self {
        let typ = match s {
            "let" => TokenType::Let,
            "if" => TokenType::If,
            "else" => TokenType::Else,
            "while" => TokenType::While,
            _ => TokenType::Ident,
        };
        Token {
            typ,
            num_val: 0.0,
            str_val: s.to_string(),
        }
    }
}

// ========== Lexer ==========

struct Lexer {
    src: Vec<char>,
    pos: usize,
}

impl Lexer {
    fn new(src: &str) -> Self {
        Lexer {
            src: src.chars().collect(),
            pos: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn skip_comment(&mut self) {
        if self.pos + 1 < self.src.len() && self.src[self.pos] == '/' && self.src[self.pos + 1] == '/' {
            while self.pos < self.src.len() && self.src[self.pos] != '\n' {
                self.pos += 1;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.src.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        Some(ch)
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        self.skip_comment();
        self.skip_whitespace();

        let Some(c) = self.peek() else {
            return Token::new(TokenType::Eof);
        };

        // Numbers
        if c.is_ascii_digit() || (c == '.' && self.peek_next().map_or(false, |ch| ch.is_ascii_digit())) {
            let start = self.pos;
            while self.peek().map_or(false, |ch| ch.is_ascii_digit() || ch == '.') {
                self.pos += 1;
            }
            let num_str: String = self.src[start..self.pos].iter().collect();
            return Token::number(num_str.parse().unwrap_or(0.0));
        }

        // Strings
        if c == '"' {
            self.pos += 1;
            let mut result = String::new();
            while let Some(ch) = self.peek() {
                if ch == '"' {
                    break;
                }
                if ch == '\\' && self.peek_next().is_some() {
                    self.pos += 1;
                    if let Some(escaped) = self.peek() {
                        result.push(escaped);
                        self.pos += 1;
                    }
                } else {
                    result.push(ch);
                    self.pos += 1;
                }
            }
            if self.peek() == Some('"') {
                self.pos += 1;
            }
            return Token::string(&result);
        }

        // Identifiers and keywords
        if c.is_alphabetic() || c == '_' {
            let start = self.pos;
            while self.peek().map_or(false, |ch| ch.is_alphanumeric() || ch == '_') {
                self.pos += 1;
            }
            let ident: String = self.src[start..self.pos].iter().collect();
            return Token::ident(&ident);
        }

        // Two-char operators
        if let Some(c2) = self.peek_next() {
            let two_char = match (c, c2) {
                ('=', '=') => Some(TokenType::Eq),
                ('!', '=') => Some(TokenType::Neq),
                ('<', '=') => Some(TokenType::Lte),
                ('>', '=') => Some(TokenType::Gte),
                ('&', '&') => Some(TokenType::And),
                ('|', '|') => Some(TokenType::Or),
                _ => None,
            };
            if let Some(typ) = two_char {
                self.pos += 2;
                return Token::new(typ);
            }
        }

        // Single-char operators
        self.pos += 1;
        match c {
            '+' => Token::new(TokenType::Plus),
            '-' => Token::new(TokenType::Minus),
            '*' => Token::new(TokenType::Star),
            '/' => Token::new(TokenType::Slash),
            '%' => Token::new(TokenType::Percent),
            '(' => Token::new(TokenType::LParen),
            ')' => Token::new(TokenType::RParen),
            ',' => Token::new(TokenType::Comma),
            '=' => Token::new(TokenType::Assign),
            '<' => Token::new(TokenType::Lt),
            '>' => Token::new(TokenType::Gt),
            '!' => Token::new(TokenType::Not),
            ';' => Token::new(TokenType::Semicolon),
            _ => {
                let mut t = Token::new(TokenType::Error);
                t.str_val = c.to_string();
                t
            }
        }
    }

    fn peek_token(&mut self) -> Token {
        let saved_pos = self.pos;
        let token = self.next_token();
        self.pos = saved_pos;
        token
    }
}

// ========== Value type ==========

#[derive(Debug, Clone)]
enum Value {
    Number(f64),
    String(String),
    Bool(bool),
    None,
}

impl Value {
    fn is_truthy(&self) -> bool {
        match self {
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Bool(b) => *b,
            Value::None => false,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::None => "none",
        }
    }
}

// Helper function to format numbers like C's %.6g
fn format_number_g(n: f64) -> String {
    if n == (n as i32) as f64 {
        return format!("{}", n as i32);
    }
    
    // Check if we should use exponential notation
    let abs_n = n.abs();
    if abs_n != 0.0 && (abs_n < 0.0001 || abs_n >= 1000000.0) {
        // Use exponential notation
        let exp_str = format!("{:e}", n);
        // Parse and reformat to match C's behavior
        if let Some(e_pos) = exp_str.find('e') {
            let mantissa = &exp_str[..e_pos];
            let exponent = &exp_str[e_pos+1..];
            
            // Remove trailing zeros from mantissa
            let trimmed_mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
            
            // Format exponent without leading zeros
            let exp_val: i32 = exponent.parse().unwrap_or(0);
            return format!("{}e{:+}", trimmed_mantissa, exp_val);
        }
    }
    
    // Regular decimal notation
    let formatted = format!("{:.6}", n);
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    
    // Check total significant digits
    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() == 2 {
        let int_digits = parts[0].trim_start_matches('-').len();
        let frac_digits = parts[1].len();
        if int_digits + frac_digits > 6 {
            // Need to limit precision
            let precision = 6 - int_digits;
            if precision > 0 {
                return format!("{:.prec$}", n, prec = precision)
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string();
            } else {
                return format!("{:.0}", n);
            }
        }
    }
    
    trimmed.to_string()
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{}", format_number_g(*n)),
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            Value::None => write!(f, "none"),
        }
    }
}

// ========== Variable store ==========

struct VarStore {
    vars: HashMap<String, Value>,
}

impl VarStore {
    fn new() -> Self {
        VarStore {
            vars: HashMap::new(),
        }
    }

    fn get(&self, name: &str) -> Value {
        self.vars.get(name).cloned().unwrap_or(Value::None)
    }

    fn set(&mut self, name: &str, val: Value) {
        self.vars.insert(name.to_string(), val);
    }
}

// ========== String utilities ==========

fn string_repeat(s: &str, times: usize) -> String {
    s.repeat(times)
}

fn string_reverse(s: &str) -> String {
    s.chars().rev().collect()
}

fn string_upper(s: &str) -> String {
    s.to_uppercase()
}

fn string_lower(s: &str) -> String {
    s.to_lowercase()
}

fn string_trim(s: &str) -> String {
    s.trim().to_string()
}

fn string_index_of(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

fn string_substring(s: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let start = start.min(len);
    let end = end.min(len);
    if start >= end {
        String::new()
    } else {
        chars[start..end].iter().collect()
    }
}

// ========== Parser + Evaluator ==========

struct Parser {
    lexer: Lexer,
    current: Token,
    error: Option<String>,
}

impl Parser {
    fn new(src: &str) -> Self {
        let mut lexer = Lexer::new(src);
        let current = lexer.next_token();
        Parser {
            lexer,
            current,
            error: None,
        }
    }

    fn error(&mut self, msg: &str) {
        if self.error.is_none() {
            self.error = Some(msg.to_string());
        }
    }

    fn advance(&mut self) {
        self.current = self.lexer.next_token();
    }

    fn expect(&mut self, typ: TokenType) -> bool {
        if self.current.typ == typ {
            self.advance();
            true
        } else {
            self.error(&format!("Expected {:?}, got {:?}", typ, self.current.typ));
            false
        }
    }

    fn parse_expr(&mut self, vars: &mut VarStore) -> Value {
        if self.error.is_some() {
            return Value::None;
        }

        // Check for assignment: IDENT = expr
        if self.current.typ == TokenType::Ident {
            let saved_pos = self.lexer.pos;
            let saved_current = self.current.clone();
            let name = self.current.str_val.clone();

            self.advance();
            if self.current.typ == TokenType::Assign {
                self.advance();
                let val = self.parse_expr(vars);
                vars.set(&name, val.clone());
                return val;
            }

            // Not an assignment, restore state
            self.lexer.pos = saved_pos;
            self.current = saved_current;
        }

        self.parse_or(vars)
    }

    fn parse_or(&mut self, vars: &mut VarStore) -> Value {
        let mut left = self.parse_and(vars);
        if self.error.is_some() {
            return Value::None;
        }

        while self.current.typ == TokenType::Or {
            self.advance();
            if left.is_truthy() {
                // Short circuit
                self.parse_and(vars);
                left = Value::Bool(true);
            } else {
                let right = self.parse_and(vars);
                left = Value::Bool(right.is_truthy());
            }
        }
        left
    }

    fn parse_and(&mut self, vars: &mut VarStore) -> Value {
        let mut left = self.parse_equality(vars);
        if self.error.is_some() {
            return Value::None;
        }

        while self.current.typ == TokenType::And {
            self.advance();
            if !left.is_truthy() {
                // Short circuit
                self.parse_equality(vars);
                left = Value::Bool(false);
            } else {
                let right = self.parse_equality(vars);
                left = Value::Bool(right.is_truthy());
            }
        }
        left
    }

    fn parse_equality(&mut self, vars: &mut VarStore) -> Value {
        let mut left = self.parse_comparison(vars);
        if self.error.is_some() {
            return Value::None;
        }

        while matches!(self.current.typ, TokenType::Eq | TokenType::Neq) {
            let op = self.current.typ;
            self.advance();
            let right = self.parse_comparison(vars);
            if self.error.is_some() {
                return Value::None;
            }

            let equal = match (&left, &right) {
                (Value::Number(a), Value::Number(b)) => a == b,
                (Value::String(a), Value::String(b)) => a == b,
                (Value::Bool(a), Value::Bool(b)) => a == b,
                (Value::None, Value::None) => true,
                _ => false,
            };

            left = Value::Bool(if op == TokenType::Eq { equal } else { !equal });
        }
        left
    }

    fn parse_comparison(&mut self, vars: &mut VarStore) -> Value {
        let mut left = self.parse_add(vars);
        if self.error.is_some() {
            return Value::None;
        }

        while matches!(self.current.typ, TokenType::Lt | TokenType::Gt | TokenType::Lte | TokenType::Gte) {
            let op = self.current.typ;
            self.advance();
            let right = self.parse_add(vars);
            if self.error.is_some() {
                return Value::None;
            }

            let result = match (&left, &right) {
                (Value::Number(a), Value::Number(b)) => match op {
                    TokenType::Lt => a < b,
                    TokenType::Gt => a > b,
                    TokenType::Lte => a <= b,
                    TokenType::Gte => a >= b,
                    _ => false,
                },
                (Value::String(a), Value::String(b)) => match op {
                    TokenType::Lt => a < b,
                    TokenType::Gt => a > b,
                    TokenType::Lte => a <= b,
                    TokenType::Gte => a >= b,
                    _ => false,
                },
                _ => {
                    self.error("Cannot compare these types");
                    false
                }
            };
            left = Value::Bool(result);
        }
        left
    }

    fn parse_add(&mut self, vars: &mut VarStore) -> Value {
        let mut left = self.parse_mul(vars);
        if self.error.is_some() {
            return Value::None;
        }

        while matches!(self.current.typ, TokenType::Plus | TokenType::Minus) {
            let op = self.current.typ;
            self.advance();
            let right = self.parse_mul(vars);
            if self.error.is_some() {
                return Value::None;
            }

            if op == TokenType::Plus {
                // String concatenation
                if matches!(&left, Value::String(_)) || matches!(&right, Value::String(_)) {
                    let lstr = match &left {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => format_number_g(*n),
                        _ => String::new(),
                    };
                    let rstr = match &right {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => format_number_g(*n),
                        _ => String::new(),
                    };
                    left = Value::String(lstr + &rstr);
                    continue;
                }
                match (&left, &right) {
                    (Value::Number(a), Value::Number(b)) => left = Value::Number(a + b),
                    _ => {
                        self.error("Addition requires numbers or strings");
                        return Value::None;
                    }
                }
            } else {
                match (&left, &right) {
                    (Value::Number(a), Value::Number(b)) => left = Value::Number(a - b),
                    _ => {
                        self.error("Subtraction requires numbers");
                        return Value::None;
                    }
                }
            }
        }
        left
    }

    fn parse_mul(&mut self, vars: &mut VarStore) -> Value {
        let mut left = self.parse_unary(vars);
        if self.error.is_some() {
            return Value::None;
        }

        while matches!(self.current.typ, TokenType::Star | TokenType::Slash | TokenType::Percent) {
            let op = self.current.typ;
            self.advance();
            let right = self.parse_unary(vars);
            if self.error.is_some() {
                return Value::None;
            }

            match (&left, &right) {
                (Value::Number(a), Value::Number(b)) => {
                    match op {
                        TokenType::Star => left = Value::Number(a * b),
                        TokenType::Slash => {
                            if *b == 0.0 {
                                self.error("Division by zero");
                                return Value::None;
                            }
                            left = Value::Number(a / b);
                        }
                        TokenType::Percent => {
                            if *b == 0.0 {
                                self.error("Modulo by zero");
                                return Value::None;
                            }
                            left = Value::Number((*a as i32 % *b as i32) as f64);
                        }
                        _ => {}
                    }
                }
                _ => {
                    self.error("Arithmetic requires numbers");
                    return Value::None;
                }
            }
        }
        left
    }

    fn parse_unary(&mut self, vars: &mut VarStore) -> Value {
        if self.error.is_some() {
            return Value::None;
        }

        if self.current.typ == TokenType::Minus {
            self.advance();
            let v = self.parse_unary(vars);
            match v {
                Value::Number(n) => Value::Number(-n),
                _ => {
                    self.error("Cannot negate non-number");
                    Value::None
                }
            }
        } else if self.current.typ == TokenType::Not {
            self.advance();
            let v = self.parse_unary(vars);
            Value::Bool(!v.is_truthy())
        } else {
            self.parse_primary(vars)
        }
    }

    fn parse_primary(&mut self, vars: &mut VarStore) -> Value {
        if self.error.is_some() {
            return Value::None;
        }

        let t = self.current.clone();

        match t.typ {
            TokenType::Number => {
                self.advance();
                Value::Number(t.num_val)
            }
            TokenType::String => {
                self.advance();
                Value::String(t.str_val)
            }
            TokenType::Ident => {
                let name = t.str_val.clone();
                self.advance();

                // Function call
                if self.current.typ == TokenType::LParen {
                    self.advance();
                    let mut args = Vec::new();

                    if self.current.typ != TokenType::RParen {
                        args.push(self.parse_expr(vars));
                        while self.current.typ == TokenType::Comma {
                            self.advance();
                            args.push(self.parse_expr(vars));
                        }
                    }
                    self.expect(TokenType::RParen);
                    call_builtin(&name, &args)
                } else {
                    // Variable reference
                    vars.get(&name)
                }
            }
            TokenType::LParen => {
                self.advance();
                let v = self.parse_expr(vars);
                self.expect(TokenType::RParen);
                v
            }
            _ => {
                self.error("Unexpected token in expression");
                Value::None
            }
        }
    }

    fn parse_statement(&mut self, vars: &mut VarStore) -> Value {
        if self.error.is_some() {
            return Value::None;
        }

        // let statement
        if self.current.typ == TokenType::Let {
            self.advance();
            if self.current.typ != TokenType::Ident {
                self.error("Expected variable name after 'let'");
                return Value::None;
            }
            let name = self.current.str_val.clone();
            self.advance();

            if !self.expect(TokenType::Assign) {
                return Value::None;
            }
            let val = self.parse_expr(vars);
            if self.current.typ == TokenType::Semicolon {
                self.advance();
            }
            vars.set(&name, val.clone());
            return val;
        }

        // if statement
        if self.current.typ == TokenType::If {
            self.advance();
            if !self.expect(TokenType::LParen) {
                return Value::None;
            }
            let cond = self.parse_expr(vars);
            if !self.expect(TokenType::RParen) {
                return Value::None;
            }

            if cond.is_truthy() {
                let result = self.parse_statement(vars);
                // Skip else branch if present
                if self.current.typ == TokenType::Else {
                    self.advance();
                    self.parse_statement(vars);
                }
                result
            } else {
                // Skip then branch
                self.parse_statement(vars);
                if self.current.typ == TokenType::Else {
                    self.advance();
                    self.parse_statement(vars)
                } else {
                    Value::None
                }
            }
        } else if self.current.typ == TokenType::While {
            // while statement
            self.advance();
            if !self.expect(TokenType::LParen) {
                return Value::None;
            }

            // Save position for loop
            let cond_pos = self.lexer.pos;
            let cond_tok = self.current.clone();
            let mut last = Value::None;
            let mut iterations = 0;
            let max_iterations = 10000;

            loop {
                // Re-parse condition
                self.lexer.pos = cond_pos;
                self.current = cond_tok.clone();

                let cond = self.parse_expr(vars);
                if self.error.is_some() {
                    return Value::None;
                }
                if !self.expect(TokenType::RParen) {
                    return Value::None;
                }

                if !cond.is_truthy() {
                    // Skip body one more time to advance past it
                    self.parse_statement(vars);
                    break;
                }

                last = self.parse_statement(vars);
                iterations += 1;

                if iterations >= max_iterations {
                    self.error("While loop exceeded maximum iterations");
                    return Value::None;
                }
            }
            last
        } else {
            // Expression statement
            let val = self.parse_expr(vars);
            if self.current.typ == TokenType::Semicolon {
                self.advance();
            }
            val
        }
    }
}

fn call_builtin(name: &str, args: &[Value]) -> Value {
    match name {
        "abs" if args.len() == 1 => {
            if let Value::Number(n) = &args[0] {
                Value::Number(n.abs())
            } else {
                Value::None
            }
        }
        "sqrt" if args.len() == 1 => {
            if let Value::Number(n) = &args[0] {
                Value::Number(n.sqrt())
            } else {
                Value::None
            }
        }
        "min" if args.len() == 2 => {
            if let (Value::Number(a), Value::Number(b)) = (&args[0], &args[1]) {
                Value::Number(a.min(*b))
            } else {
                Value::None
            }
        }
        "max" if args.len() == 2 => {
            if let (Value::Number(a), Value::Number(b)) = (&args[0], &args[1]) {
                Value::Number(a.max(*b))
            } else {
                Value::None
            }
        }
        "pow" if args.len() == 2 => {
            if let (Value::Number(a), Value::Number(b)) = (&args[0], &args[1]) {
                Value::Number(a.powf(*b))
            } else {
                Value::None
            }
        }
        "floor" if args.len() == 1 => {
            if let Value::Number(n) = &args[0] {
                Value::Number(n.floor())
            } else {
                Value::None
            }
        }
        "ceil" if args.len() == 1 => {
            if let Value::Number(n) = &args[0] {
                Value::Number(n.ceil())
            } else {
                Value::None
            }
        }
        "round" if args.len() == 1 => {
            if let Value::Number(n) = &args[0] {
                Value::Number(n.round())
            } else {
                Value::None
            }
        }
        "print" | "println" => {
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    print!(" ");
                }
                print!("{}", arg);
            }
            println!();
            Value::None
        }
        "len" if args.len() == 1 => {
            if let Value::String(s) = &args[0] {
                Value::Number(s.len() as f64)
            } else {
                Value::None
            }
        }
        "type" if args.len() == 1 => {
            Value::String(args[0].type_name().to_string())
        }
        "str" if args.len() == 1 => {
            Value::String(args[0].to_string())
        }
        "num" if args.len() == 1 => {
            match &args[0] {
                Value::String(s) => Value::Number(s.parse().unwrap_or(0.0)),
                Value::Number(n) => Value::Number(*n),
                _ => Value::None,
            }
        }
        "upper" if args.len() == 1 => {
            if let Value::String(s) = &args[0] {
                Value::String(string_upper(s))
            } else {
                Value::None
            }
        }
        "lower" if args.len() == 1 => {
            if let Value::String(s) = &args[0] {
                Value::String(string_lower(s))
            } else {
                Value::None
            }
        }
        "trim" if args.len() == 1 => {
            if let Value::String(s) = &args[0] {
                Value::String(string_trim(s))
            } else {
                Value::None
            }
        }
        "reverse" if args.len() == 1 => {
            if let Value::String(s) = &args[0] {
                Value::String(string_reverse(s))
            } else {
                Value::None
            }
        }
        "repeat" if args.len() == 2 => {
            if let (Value::String(s), Value::Number(n)) = (&args[0], &args[1]) {
                Value::String(string_repeat(s, *n as usize))
            } else {
                Value::None
            }
        }
        "index_of" if args.len() == 2 => {
            if let (Value::String(haystack), Value::String(needle)) = (&args[0], &args[1]) {
                match string_index_of(haystack, needle) {
                    Some(idx) => Value::Number(idx as f64),
                    None => Value::Number(-1.0),
                }
            } else {
                Value::None
            }
        }
        "substring" if args.len() == 3 => {
            if let (Value::String(s), Value::Number(start), Value::Number(end)) = (&args[0], &args[1], &args[2]) {
                Value::String(string_substring(s, *start as usize, *end as usize))
            } else {
                Value::None
            }
        }
        "contains" if args.len() == 2 => {
            if let (Value::String(haystack), Value::String(needle)) = (&args[0], &args[1]) {
                Value::Bool(haystack.contains(needle))
            } else {
                Value::None
            }
        }
        "starts_with" if args.len() == 2 => {
            if let (Value::String(s), Value::String(prefix)) = (&args[0], &args[1]) {
                Value::Bool(s.starts_with(prefix))
            } else {
                Value::None
            }
        }
        "ends_with" if args.len() == 2 => {
            if let (Value::String(s), Value::String(suffix)) = (&args[0], &args[1]) {
                Value::Bool(s.ends_with(suffix))
            } else {
                Value::None
            }
        }
        "char_at" if args.len() == 2 => {
            if let (Value::String(s), Value::Number(idx)) = (&args[0], &args[1]) {
                let chars: Vec<char> = s.chars().collect();
                let idx = *idx as usize;
                if idx < chars.len() {
                    Value::String(chars[idx].to_string())
                } else {
                    Value::String(String::new())
                }
            } else {
                Value::None
            }
        }
        _ => {
            eprintln!("Error: unknown function '{}' with {} args", name, args.len());
            Value::None
        }
    }
}

// ========== History ==========

struct History {
    items: Vec<Value>,
}

impl History {
    fn new() -> Self {
        History { items: Vec::new() }
    }

    fn push(&mut self, v: Value) {
        if self.items.len() < 100 {
            self.items.push(v);
        }
    }

    fn get(&self, index: usize) -> Value {
        self.items.get(index).cloned().unwrap_or(Value::None)
    }
}

// ========== Evaluate a line of input ==========

fn eval_line(line: &str, vars: &mut VarStore) -> Value {
    let mut parser = Parser::new(line);

    let mut last = Value::None;
    while parser.current.typ != TokenType::Eof && parser.error.is_none() {
        last = parser.parse_statement(vars);
    }

    if let Some(err) = parser.error {
        eprintln!("Error: {}", err);
        return Value::None;
    }

    last
}

// ========== Statistical functions ==========

struct DataSet {
    data: Vec<f64>,
}

impl DataSet {
    fn new() -> Self {
        DataSet { data: Vec::new() }
    }

    fn push(&mut self, val: f64) {
        self.data.push(val);
    }

    fn mean(&self) -> f64 {
        if self.data.is_empty() {
            0.0
        } else {
            self.data.iter().sum::<f64>() / self.data.len() as f64
        }
    }

    fn variance(&self) -> f64 {
        if self.data.len() < 2 {
            0.0
        } else {
            let mean = self.mean();
            let sum_sq: f64 = self.data.iter().map(|&x| (x - mean).powi(2)).sum();
            sum_sq / (self.data.len() - 1) as f64
        }
    }

    fn stddev(&self) -> f64 {
        self.variance().sqrt()
    }

    fn min(&self) -> f64 {
        self.data.iter().copied().fold(f64::INFINITY, f64::min)
    }

    fn max(&self) -> f64 {
        self.data.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }

    fn median(&self) -> f64 {
        if self.data.is_empty() {
            return 0.0;
        }

        let mut sorted = self.data.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let len = sorted.len();
        if len % 2 == 0 {
            (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
        } else {
            sorted[len / 2]
        }
    }
}

// ========== Main: test program ==========

fn test_basic_arithmetic(vars: &mut VarStore) {
    println!("=== Basic Arithmetic ===");

    let r = eval_line("2 + 3", vars);
    println!("2 + 3 = {}", r);

    let r = eval_line("10 - 4 * 2", vars);
    println!("10 - 4 * 2 = {}", r);

    let r = eval_line("(10 - 4) * 2", vars);
    println!("(10 - 4) * 2 = {}", r);

    let r = eval_line("100 / 7", vars);
    println!("100 / 7 = {}", r);

    let r = eval_line("17 % 5", vars);
    println!("17 % 5 = {}", r);

    let r = eval_line("-42", vars);
    println!("-42 = {}", r);

    let r = eval_line("2 + 3 * 4 - 1", vars);
    println!("2 + 3 * 4 - 1 = {}", r);
}

fn test_comparisons(vars: &mut VarStore) {
    println!("\n=== Comparisons ===");

    let r = eval_line("5 > 3", vars);
    println!("5 > 3 = {}", r);

    let r = eval_line("3 >= 3", vars);
    println!("3 >= 3 = {}", r);

    let r = eval_line("2 < 1", vars);
    println!("2 < 1 = {}", r);

    let r = eval_line("10 == 10", vars);
    println!("10 == 10 = {}", r);

    let r = eval_line("10 != 5", vars);
    println!("10 != 5 = {}", r);

    let r = eval_line("!0", vars);
    println!("!0 = {}", r);

    let r = eval_line("!1", vars);
    println!("!1 = {}", r);
}

fn test_logical_ops(vars: &mut VarStore) {
    println!("\n=== Logical Operations ===");

    let r = eval_line("1 && 1", vars);
    println!("1 && 1 = {}", r);

    let r = eval_line("1 && 0", vars);
    println!("1 && 0 = {}", r);

    let r = eval_line("0 || 1", vars);
    println!("0 || 1 = {}", r);

    let r = eval_line("0 || 0", vars);
    println!("0 || 0 = {}", r);

    let r = eval_line("(5 > 3) && (10 < 20)", vars);
    println!("(5 > 3) && (10 < 20) = {}", r);
}

fn test_variables(vars: &mut VarStore) {
    println!("\n=== Variables ===");

    eval_line("let x = 42;", vars);
    let r = eval_line("x", vars);
    println!("x = {}", r);

    eval_line("let y = x * 2;", vars);
    let r = eval_line("y", vars);
    println!("y = x * 2 = {}", r);

    eval_line("x = 100;", vars);
    let r = eval_line("x", vars);
    println!("x reassigned = {}", r);

    eval_line("let sum = x + y;", vars);
    let r = eval_line("sum", vars);
    println!("sum = x + y = {}", r);
}

fn test_strings(vars: &mut VarStore) {
    println!("\n=== Strings ===");

    let r = eval_line("\"hello\"", vars);
    println!("literal = {}", r);

    let r = eval_line("\"hello\" + \" \" + \"world\"", vars);
    println!("concat = {}", r);

    let r = eval_line("\"count: \" + 42", vars);
    println!("str + num = {}", r);

    let r = eval_line("len(\"hello\")", vars);
    println!("len(\"hello\") = {}", r);

    let r = eval_line("upper(\"hello\")", vars);
    println!("upper(\"hello\") = {}", r);

    let r = eval_line("lower(\"WORLD\")", vars);
    println!("lower(\"WORLD\") = {}", r);

    let r = eval_line("reverse(\"abcde\")", vars);
    println!("reverse(\"abcde\") = {}", r);

    let r = eval_line("trim(\"  spaces  \")", vars);
    println!("trim(\"  spaces  \") = {}", r);

    let r = eval_line("repeat(\"ab\", 3)", vars);
    println!("repeat(\"ab\", 3) = {}", r);

    let r = eval_line("contains(\"hello world\", \"world\")", vars);
    println!("contains(\"hello world\", \"world\") = {}", r);

    let r = eval_line("starts_with(\"hello\", \"hel\")", vars);
    println!("starts_with(\"hello\", \"hel\") = {}", r);

    let r = eval_line("ends_with(\"hello\", \"llo\")", vars);
    println!("ends_with(\"hello\", \"llo\") = {}", r);

    let r = eval_line("index_of(\"hello world\", \"world\")", vars);
    println!("index_of(\"hello world\", \"world\") = {}", r);

    let r = eval_line("substring(\"hello world\", 0, 5)", vars);
    println!("substring(\"hello world\", 0, 5) = {}", r);

    let r = eval_line("char_at(\"abcde\", 2)", vars);
    println!("char_at(\"abcde\", 2) = {}", r);

    let r = eval_line("\"abc\" < \"def\"", vars);
    println!("\"abc\" < \"def\" = {}", r);

    let r = eval_line("\"hello\" == \"hello\"", vars);
    println!("\"hello\" == \"hello\" = {}", r);
}

fn test_builtins(vars: &mut VarStore) {
    println!("\n=== Built-in Functions ===");

    let r = eval_line("abs(-7)", vars);
    println!("abs(-7) = {}", r);

    let r = eval_line("sqrt(144)", vars);
    println!("sqrt(144) = {}", r);

    let r = eval_line("min(3, 7)", vars);
    println!("min(3, 7) = {}", r);

    let r = eval_line("max(3, 7)", vars);
    println!("max(3, 7) = {}", r);

    let r = eval_line("pow(2, 10)", vars);
    println!("pow(2, 10) = {}", r);

    let r = eval_line("floor(3.7)", vars);
    println!("floor(3.7) = {}", r);

    let r = eval_line("ceil(3.2)", vars);
    println!("ceil(3.2) = {}", r);

    let r = eval_line("round(3.5)", vars);
    println!("round(3.5) = {}", r);

    let r = eval_line("type(42)", vars);
    println!("type(42) = {}", r);

    let r = eval_line("type(\"hi\")", vars);
    println!("type(\"hi\") = {}", r);

    let r = eval_line("str(123)", vars);
    println!("str(123) = {}", r);

    let r = eval_line("num(\"456\")", vars);
    println!("num(\"456\") = {}", r);
}

fn test_conditionals(vars: &mut VarStore) {
    println!("\n=== Conditionals ===");

    vars.vars.clear();
    eval_line("let x = 10;", vars);

    eval_line("if (x > 5) print(\"x is big\");", vars);
    eval_line("if (x < 5) print(\"x is small\"); else print(\"x is not small\");", vars);

    eval_line("let grade = 85;", vars);
    eval_line("if (grade >= 90) print(\"A\"); else if (grade >= 80) print(\"B\"); else if (grade >= 70) print(\"C\"); else print(\"F\");", vars);
}

fn test_complex_expressions(vars: &mut VarStore) {
    println!("\n=== Complex Expressions ===");

    vars.vars.clear();

    // Chained assignments
    eval_line("let a = 5;", vars);
    eval_line("let b = a * 2 + 3;", vars);
    eval_line("let c = b - a;", vars);
    let r = eval_line("c", vars);
    println!("c = (5*2+3) - 5 = {}", r);

    // Nested function calls
    let r = eval_line("max(min(10, 20), min(5, 15))", vars);
    println!("max(min(10,20), min(5,15)) = {}", r);

    // String + number expressions
    eval_line("let name = \"world\";", vars);
    let r = eval_line("\"hello \" + name + \" #\" + 42", vars);
    println!("string concat = {}", r);

    // Factorial via manual unrolling
    eval_line("let f = 1 * 2 * 3 * 4 * 5 * 6 * 7 * 8 * 9 * 10;", vars);
    let r = eval_line("f", vars);
    println!("10! = {}", r);

    // Boolean chains
    let r = eval_line("(5 > 3) && (10 != 11) && (\"abc\" < \"def\")", vars);
    println!("complex bool = {}", r);

    // Nested conditionals
    eval_line("let score = 92;", vars);
    eval_line("if (score >= 90) print(\"Grade: A\"); else if (score >= 80) print(\"Grade: B\");", vars);

    // Type checking
    let r = eval_line("type(3.14)", vars);
    println!("type(3.14) = {}", r);
    let r = eval_line("type(\"hi\")", vars);
    println!("type(\"hi\") = {}", r);
    let r = eval_line("type(5 > 3)", vars);
    println!("type(5 > 3) = {}", r);
}

fn test_dataset() {
    println!("\n=== Dataset Statistics ===");

    let mut ds = DataSet::new();
    let values = vec![4.0, 8.0, 15.0, 16.0, 23.0, 42.0];

    for &val in &values {
        ds.push(val);
    }

    print!("Data: ");
    for (i, &val) in ds.data.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("{:.0}", val);
    }
    println!();

    println!("Count: {}", ds.data.len());
    println!("Mean: {:.2}", ds.mean());
    println!("Variance: {:.2}", ds.variance());
    println!("Stddev: {:.2}", ds.stddev());
    println!("Min: {:.0}", ds.min());
    println!("Max: {:.0}", ds.max());
    println!("Median: {:.1}", ds.median());

    // Test with odd count
    ds.push(50.0);
    println!("Median (7 items): {:.1}", ds.median());
}

fn test_hashmap() {
    println!("\n=== HashMap ===");

    let mut map: HashMap<String, i32> = HashMap::new();

    map.insert("alice".to_string(), 95);
    map.insert("bob".to_string(), 87);
    map.insert("charlie".to_string(), 72);
    map.insert("diana".to_string(), 91);
    map.insert("eve".to_string(), 88);

    println!("alice: {}", map.get("alice").unwrap_or(&0));
    println!("bob: {}", map.get("bob").unwrap_or(&0));
    println!("charlie: {}", map.get("charlie").unwrap_or(&0));

    // Update
    map.insert("charlie".to_string(), 78);
    println!("charlie (updated): {}", map.get("charlie").unwrap_or(&0));

    // Contains
    println!("contains(diana): {}", map.contains_key("diana") as i32);
    println!("contains(frank): {}", map.contains_key("frank") as i32);

    // Remove
    map.remove("bob");
    println!("contains(bob) after remove: {}", map.contains_key("bob") as i32);
    println!("size after remove: {}", map.len());

    // Many insertions to test collision handling
    for i in 0..100 {
        map.insert(format!("key_{}", i), i * 10);
    }
    println!("size after 100 inserts: {}", map.len());

    // Verify some values
    println!("key_0: {}", map.get("key_0").unwrap_or(&0));
    println!("key_50: {}", map.get("key_50").unwrap_or(&0));
    println!("key_99: {}", map.get("key_99").unwrap_or(&0));
}

fn main() {
    let mut vars = VarStore::new();
    let mut _history = History::new();

    test_basic_arithmetic(&mut vars);
    test_comparisons(&mut vars);
    test_logical_ops(&mut vars);
    test_variables(&mut vars);
    test_strings(&mut vars);
    test_builtins(&mut vars);
    test_conditionals(&mut vars);
    test_complex_expressions(&mut vars);
    test_dataset();
    test_hashmap();

    println!("\nAll tests completed.");
}