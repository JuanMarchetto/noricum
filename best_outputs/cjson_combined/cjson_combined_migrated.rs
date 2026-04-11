use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    fn new_object() -> Self {
        JsonValue::Object(Vec::new())
    }

    fn new_string(s: &str) -> Self {
        JsonValue::String(s.to_string())
    }

    fn new_number(num: f64) -> Self {
        JsonValue::Number(num)
    }

    fn new_bool(b: bool) -> Self {
        JsonValue::Bool(b)
    }

    fn new_int_array(numbers: &[i32]) -> Self {
        JsonValue::Array(numbers.iter().map(|&n| JsonValue::Number(n as f64)).collect())
    }

    fn add_string_to_object(&mut self, name: &str, string: &str) -> Option<()> {
        match self {
            JsonValue::Object(vec) => {
                vec.push((name.to_string(), JsonValue::String(string.to_string())));
                Some(())
            }
            _ => None,
        }
    }

    fn add_number_to_object(&mut self, name: &str, number: f64) -> Option<()> {
        match self {
            JsonValue::Object(vec) => {
                vec.push((name.to_string(), JsonValue::Number(number)));
                Some(())
            }
            _ => None,
        }
    }

    fn add_bool_to_object(&mut self, name: &str, boolean: bool) -> Option<()> {
        match self {
            JsonValue::Object(vec) => {
                vec.push((name.to_string(), JsonValue::Bool(boolean)));
                Some(())
            }
            _ => None,
        }
    }

    fn get_object_item(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(vec) => vec.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn get_array_item(&self, index: usize) -> Option<&JsonValue> {
        match self {
            JsonValue::Array(vec) => vec.get(index),
            _ => None,
        }
    }

    fn get_array_size(&self) -> usize {
        match self {
            JsonValue::Array(vec) => vec.len(),
            _ => 0,
        }
    }
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonValue::Null => write!(f, "null"),
            JsonValue::Bool(true) => write!(f, "true"),
            JsonValue::Bool(false) => write!(f, "false"),
            JsonValue::Number(n) => {
                // Match C's %g formatting exactly
                if *n == 0.0 {
                    write!(f, "0")
                } else if (n.fract().abs() <= 1e-10) && (*n >= -2147483648.0) && (*n <= 2147483647.0) {
                    write!(f, "{}", *n as i32)
                } else {
                    // Simple approximation of %g formatting
                    let s = format!("{}", n);
                    if s.contains('e') || s.contains('.') {
                        write!(f, "{}", s.trim_end_matches('0').trim_end_matches('.'))
                    } else {
                        write!(f, "{}", s)
                    }
                }
            }
            JsonValue::String(s) => {
                write!(f, "\"")?;
                for c in s.chars() {
                    match c {
                        '"' => write!(f, "\\\"")?,
                        '\\' => write!(f, "\\\\")?,
                        '\n' => write!(f, "\\n")?,
                        '\t' => write!(f, "\\t")?,
                        _ => write!(f, "{}", c)?,
                    }
                }
                write!(f, "\"")
            }
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
                for (i, (key, value)) in vec.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "\"{}\":{}", key, value)?;
                }
                write!(f, "}}")
            }
        }
    }
}

#[derive(Debug)]
struct ParseError(String);

impl std::error::Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Parse error: {}", self.0)
    }
}

impl FromStr for JsonValue {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars().peekable();
        parse_value(&mut chars)
    }
}

fn skip_whitespace(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
}

fn parse_string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String, ParseError> {
    if chars.next() != Some('"') {
        return Err(ParseError("Expected opening quote".to_string()));
    }
    
    let mut result = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(result),
            '\\' => {
                let escaped = chars.next().ok_or(ParseError("Unterminated escape sequence".to_string()))?;
                match escaped {
                    'n' => result.push('\n'),
                    't' => result.push('\t'),
                    '"' => result.push('"'),
                    '\\' => result.push('\\'),
                    _ => result.push(escaped),
                }
            }
            _ => result.push(c),
        }
    }
    Err(ParseError("Unterminated string".to_string()))
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<JsonValue, ParseError> {
    let mut num_str = String::new();
    let mut has_dot = false;
    
    if let Some(&'-') = chars.peek() {
        num_str.push(chars.next().unwrap());
    }
    
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            num_str.push(chars.next().unwrap());
        } else if c == '.' && !has_dot {
            has_dot = true;
            num_str.push(chars.next().unwrap());
        } else {
            break;
        }
    }
    
    num_str.parse::<f64>()
        .map(JsonValue::Number)
        .map_err(|_| ParseError("Invalid number".to_string()))
}

fn parse_array(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<JsonValue, ParseError> {
    if chars.next() != Some('[') {
        return Err(ParseError("Expected '['".to_string()));
    }
    
    skip_whitespace(chars);
    if let Some(&']') = chars.peek() {
        chars.next();
        return Ok(JsonValue::Array(Vec::new()));
    }
    
    let mut items = Vec::new();
    loop {
        skip_whitespace(chars);
        items.push(parse_value(chars)?);
        skip_whitespace(chars);
        
        match chars.next() {
            Some(']') => break,
            Some(',') => continue,
            Some(_) => return Err(ParseError("Expected ',' or ']'".to_string())),
            None => return Err(ParseError("Unexpected end of array".to_string())),
        }
    }
    
    Ok(JsonValue::Array(items))
}

fn parse_object(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<JsonValue, ParseError> {
    if chars.next() != Some('{') {
        return Err(ParseError("Expected '{'".to_string()));
    }
    
    skip_whitespace(chars);
    if let Some(&'}') = chars.peek() {
        chars.next();
        return Ok(JsonValue::Object(Vec::new()));
    }
    
    let mut vec = Vec::new();
    loop {
        skip_whitespace(chars);
        let key = parse_string(chars)?;
        skip_whitespace(chars);
        
        if chars.next() != Some(':') {
            return Err(ParseError("Expected ':'".to_string()));
        }
        
        skip_whitespace(chars);
        let value = parse_value(chars)?;
        vec.push((key, value));
        skip_whitespace(chars);
        
        match chars.next() {
            Some('}') => break,
            Some(',') => continue,
            Some(_) => return Err(ParseError("Expected ',' or '}'".to_string())),
            None => return Err(ParseError("Unexpected end of object".to_string())),
        }
    }
    
    Ok(JsonValue::Object(vec))
}

fn parse_value(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<JsonValue, ParseError> {
    skip_whitespace(chars);
    
    match chars.peek() {
        Some('"') => Ok(JsonValue::String(parse_string(chars)?)),
        Some('{') => parse_object(chars),
        Some('[') => parse_array(chars),
        Some('t') => {
            let word: String = chars.take(4).collect();
            if word == "true" {
                Ok(JsonValue::Bool(true))
            } else {
                Err(ParseError("Expected 'true'".to_string()))
            }
        }
        Some('f') => {
            let word: String = chars.take(5).collect();
            if word == "false" {
                Ok(JsonValue::Bool(false))
            } else {
                Err(ParseError("Expected 'false'".to_string()))
            }
        }
        Some('n') => {
            let word: String = chars.take(4).collect();
            if word == "null" {
                Ok(JsonValue::Null)
            } else {
                Err(ParseError("Expected 'null'".to_string()))
            }
        }
        Some(&c) if c.is_ascii_digit() || c == '-' => parse_number(chars),
        Some(_) => Err(ParseError("Unexpected character".to_string())),
        None => Err(ParseError("Unexpected end of input".to_string())),
    }
}

fn main() -> Result<(), ParseError> {
    // Test 1: Create and print object
    let mut obj = JsonValue::new_object();
    obj.add_string_to_object("name", "noricum").unwrap();
    obj.add_number_to_object("version", 1.0).unwrap();
    obj.add_bool_to_object("valid", true).unwrap();
    println!("create: {}", obj);

    // Test 2: Parse JSON
    let parsed: JsonValue = r#"{"key":"value","num":42}"#.parse()?;
    if let Some(JsonValue::String(val)) = parsed.get_object_item("key") {
        println!("key={}", val);
    }
    if let Some(JsonValue::Number(num)) = parsed.get_object_item("num") {
        println!("num={}", *num as i32);
    }

    // Test 3: Array
    let nums = [1, 2, 3, 4, 5];
    let arr = JsonValue::new_int_array(&nums);
    println!("array_size={}", arr.get_array_size());
    if let Some(JsonValue::Number(num)) = arr.get_array_item(2) {
        println!("item_2={}", *num as i32);
    }

    println!("done");
    Ok(())
}