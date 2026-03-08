use std::fmt;

#[derive(Debug, Clone)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>), // Changed to Vec to preserve insertion order
}

impl JsonValue {
    fn new_object() -> Self {
        JsonValue::Object(Vec::new())
    }

    fn new_string(s: &str) -> Self {
        JsonValue::String(s.to_string())
    }

    fn new_number(n: f64) -> Self {
        JsonValue::Number(n)
    }

    fn new_bool(b: bool) -> Self {
        JsonValue::Bool(b)
    }

    fn new_int_array(numbers: &[i32]) -> Self {
        JsonValue::Array(numbers.iter().map(|&n| JsonValue::Number(n as f64)).collect())
    }

    fn add_to_object(&mut self, key: &str, value: JsonValue) {
        if let JsonValue::Object(vec) = self {
            vec.push((key.to_string(), value));
        }
    }

    fn get_object_item(&self, key: &str) -> Option<&JsonValue> {
        if let JsonValue::Object(vec) = self {
            for (k, v) in vec {
                if k == key {
                    return Some(v);
                }
            }
        }
        None
    }

    fn get_array_item(&self, index: usize) -> Option<&JsonValue> {
        if let JsonValue::Array(arr) = self {
            arr.get(index)
        } else {
            None
        }
    }

    fn get_array_size(&self) -> usize {
        if let JsonValue::Array(arr) = self {
            arr.len()
        } else {
            0
        }
    }

    fn as_string(&self) -> Option<&str> {
        if let JsonValue::String(s) = self {
            Some(s)
        } else {
            None
        }
    }

    fn as_int(&self) -> Option<i32> {
        if let JsonValue::Number(n) = self {
            Some(*n as i32)
        } else {
            None
        }
    }

    fn parse(input: &str) -> Option<JsonValue> {
        Parser::new(input).parse_value()
    }
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonValue::Null => write!(f, "null"),
            JsonValue::Bool(b) => write!(f, "{}", b),
            JsonValue::Number(n) => {
                if n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64 {
                    write!(f, "{}", *n as i32)
                } else {
                    write!(f, "{}", n)
                }
            }
            JsonValue::String(s) => write!(f, "\"{}\"", escape_string(s)),
            JsonValue::Array(arr) => {
                write!(f, "[")?;
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            JsonValue::Object(vec) => {
                write!(f, "{{")?;
                for (i, (k, v)) in vec.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "\"{}\":{}", escape_string(k), v)?;
                }
                write!(f, "}}")
            }
        }
    }
}

fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { input, pos: 0 }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            match self.input.as_bytes()[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn consume(&mut self) -> Option<u8> {
        if self.pos < self.input.len() {
            let byte = self.input.as_bytes()[self.pos];
            self.pos += 1;
            Some(byte)
        } else {
            None
        }
    }

    fn parse_string(&mut self) -> Option<String> {
        if self.consume()? != b'"' {
            return None;
        }
        let mut result = String::new();
        loop {
            match self.consume()? {
                b'"' => return Some(result),
                b'\\' => {
                    match self.consume()? {
                        b'n' => result.push('\n'),
                        b't' => result.push('\t'),
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        c => result.push(c as char),
                    }
                }
                c => result.push(c as char),
            }
        }
    }

    fn parse_number(&mut self) -> Option<f64> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.consume();
        }
        while self.peek().map_or(false, |c| c.is_ascii_digit()) {
            self.consume();
        }
        if self.peek() == Some(b'.') {
            self.consume();
            while self.peek().map_or(false, |c| c.is_ascii_digit()) {
                self.consume();
            }
        }
        self.input[start..self.pos].parse().ok()
    }

    fn parse_array(&mut self) -> Option<JsonValue> {
        self.consume(); // '['
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.consume();
            return Some(JsonValue::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek()? {
                b',' => {
                    self.consume();
                    self.skip_whitespace();
                }
                b']' => {
                    self.consume();
                    return Some(JsonValue::Array(items));
                }
                _ => return None,
            }
        }
    }

    fn parse_object(&mut self) -> Option<JsonValue> {
        self.consume(); // '{'
        self.skip_whitespace();
        let mut vec = Vec::new();
        if self.peek() == Some(b'}') {
            self.consume();
            return Some(JsonValue::Object(vec));
        }
        loop {
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.consume()? != b':' {
                return None;
            }
            self.skip_whitespace();
            let value = self.parse_value()?;
            vec.push((key, value));
            self.skip_whitespace();
            match self.peek()? {
                b',' => {
                    self.consume();
                    self.skip_whitespace();
                }
                b'}' => {
                    self.consume();
                    return Some(JsonValue::Object(vec));
                }
                _ => return None,
            }
        }
    }

    fn parse_literal(&mut self, literal: &str, value: JsonValue) -> Option<JsonValue> {
        let bytes = literal.as_bytes();
        for &byte in bytes {
            if self.consume()? != byte {
                return None;
            }
        }
        Some(value)
    }

    fn parse_value(&mut self) -> Option<JsonValue> {
        self.skip_whitespace();
        match self.peek()? {
            b'"' => Some(JsonValue::String(self.parse_string()?)),
            b'-' | b'0'..=b'9' => Some(JsonValue::Number(self.parse_number()?)),
            b'[' => self.parse_array(),
            b'{' => self.parse_object(),
            b't' => self.parse_literal("true", JsonValue::Bool(true)),
            b'f' => self.parse_literal("false", JsonValue::Bool(false)),
            b'n' => self.parse_literal("null", JsonValue::Null),
            _ => None,
        }
    }
}

fn main() {
    // Test 1: Create and print object
    let mut obj = JsonValue::new_object();
    obj.add_to_object("name", JsonValue::new_string("noricum"));
    obj.add_to_object("version", JsonValue::new_number(1.0));
    obj.add_to_object("valid", JsonValue::new_bool(true));
    println!("create: {}", obj);

    // Test 2: Parse JSON
    if let Some(parsed) = JsonValue::parse("{\"key\":\"value\",\"num\":42}") {
        if let Some(key_val) = parsed.get_object_item("key").and_then(|v| v.as_string()) {
            println!("key={}", key_val);
        }
        if let Some(num_val) = parsed.get_object_item("num").and_then(|v| v.as_int()) {
            println!("num={}", num_val);
        }
    }

    // Test 3: Array
    let nums = [1, 2, 3, 4, 5];
    let arr = JsonValue::new_int_array(&nums);
    println!("array_size={}", arr.get_array_size());
    if let Some(item) = arr.get_array_item(2).and_then(|v| v.as_int()) {
        println!("item_2={}", item);
    }

    println!("done");
}