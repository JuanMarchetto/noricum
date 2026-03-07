// cJSON Full Combined — idiomatic Rust translation
// Based on the MIT-licensed cJSON library by Dave Gamble
// Translated from tests/fixtures/cjson_full/cjson_full_combined.c

// ---- Core data model ----

#[derive(Clone, Debug)]
enum JsonValue {
    Null,
    Bool(bool),
    Number { value: f64, int_value: i32 },
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
    Raw(String),
}

impl JsonValue {
    fn is_null(&self) -> bool { matches!(self, JsonValue::Null) }
    fn is_bool(&self) -> bool { matches!(self, JsonValue::Bool(_)) }
    fn is_true(&self) -> bool { matches!(self, JsonValue::Bool(true)) }
    fn is_false(&self) -> bool { matches!(self, JsonValue::Bool(false)) }
    fn is_number(&self) -> bool { matches!(self, JsonValue::Number { .. }) }
    fn is_string(&self) -> bool { matches!(self, JsonValue::Str(_)) }
    fn is_array(&self) -> bool { matches!(self, JsonValue::Array(_)) }
    fn is_object(&self) -> bool { matches!(self, JsonValue::Object(_)) }
    fn is_raw(&self) -> bool { matches!(self, JsonValue::Raw(_)) }

    fn get_string_value(&self) -> Option<&str> {
        if let JsonValue::Str(s) = self { Some(s) } else { None }
    }

    fn get_number_value(&self) -> f64 {
        if let JsonValue::Number { value, .. } = self { *value } else { f64::NAN }
    }

    fn get_valuedouble(&self) -> f64 {
        match self {
            JsonValue::Number { value, .. } => *value,
            JsonValue::Bool(true) => 1.0,
            _ => 0.0,
        }
    }

    fn get_valueint(&self) -> i32 {
        match self {
            JsonValue::Number { int_value, .. } => *int_value,
            JsonValue::Bool(true) => 1,
            _ => 0,
        }
    }

    fn get_object_item(&self, name: &str) -> Option<&JsonValue> {
        if let JsonValue::Object(entries) = self {
            entries.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
        } else {
            None
        }
    }

    fn get_object_item_case_sensitive(&self, name: &str) -> Option<&JsonValue> {
        if let JsonValue::Object(entries) = self {
            entries.iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v)
        } else {
            None
        }
    }

    fn has_object_item(&self, name: &str) -> bool {
        self.get_object_item(name).is_some()
    }

    fn get_array_size(&self) -> usize {
        if let JsonValue::Array(items) = self { items.len() } else { 0 }
    }

    fn get_array_item(&self, index: usize) -> Option<&JsonValue> {
        if let JsonValue::Array(items) = self { items.get(index) } else { None }
    }

    fn add_item_to_array(&mut self, item: JsonValue) -> bool {
        if let JsonValue::Array(items) = self { items.push(item); true } else { false }
    }

    fn add_item_to_object(&mut self, key: &str, item: JsonValue) -> bool {
        if let JsonValue::Object(entries) = self {
            entries.push((key.to_string(), item));
            true
        } else {
            false
        }
    }

    fn delete_item_from_object(&mut self, key: &str) {
        if let JsonValue::Object(entries) = self {
            entries.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        }
    }

    fn delete_item_from_array(&mut self, index: usize) {
        if let JsonValue::Array(items) = self {
            if index < items.len() { items.remove(index); }
        }
    }

    fn detach_item_from_array(&mut self, index: usize) -> Option<JsonValue> {
        if let JsonValue::Array(items) = self {
            if index < items.len() { Some(items.remove(index)) } else { None }
        } else {
            None
        }
    }

    fn insert_item_in_array(&mut self, index: usize, item: JsonValue) -> bool {
        if let JsonValue::Array(items) = self {
            let idx = index.min(items.len());
            items.insert(idx, item);
            true
        } else {
            false
        }
    }

    fn replace_item_in_array(&mut self, index: usize, item: JsonValue) -> bool {
        if let JsonValue::Array(items) = self {
            if index < items.len() { items[index] = item; true } else { false }
        } else {
            false
        }
    }

    fn replace_item_in_object(&mut self, key: &str, item: JsonValue) -> bool {
        if let JsonValue::Object(entries) = self {
            for entry in entries.iter_mut() {
                if entry.0.eq_ignore_ascii_case(key) {
                    entry.0 = key.to_string();
                    entry.1 = item;
                    return true;
                }
            }
        }
        false
    }
}

impl PartialEq for JsonValue {
    fn eq(&self, other: &Self) -> bool {
        compare_json(self, other, true)
    }
}

fn compare_json(a: &JsonValue, b: &JsonValue, case_sensitive: bool) -> bool {
    match (a, b) {
        (JsonValue::Null, JsonValue::Null) => true,
        (JsonValue::Bool(x), JsonValue::Bool(y)) => x == y,
        (JsonValue::Number { value: va, .. }, JsonValue::Number { value: vb, .. }) => {
            compare_double(*va, *vb)
        }
        (JsonValue::Str(sa), JsonValue::Str(sb)) => sa == sb,
        (JsonValue::Raw(sa), JsonValue::Raw(sb)) => sa == sb,
        (JsonValue::Array(aa), JsonValue::Array(ab)) => {
            aa.len() == ab.len()
                && aa.iter().zip(ab.iter()).all(|(x, y)| compare_json(x, y, case_sensitive))
        }
        (JsonValue::Object(oa), JsonValue::Object(ob)) => {
            oa.iter().all(|(k, v)| {
                find_in_entries(ob, k, case_sensitive)
                    .map_or(false, |bv| compare_json(v, bv, case_sensitive))
            }) && ob.iter().all(|(k, v)| {
                find_in_entries(oa, k, case_sensitive)
                    .map_or(false, |av| compare_json(v, av, case_sensitive))
            })
        }
        _ => false,
    }
}

fn find_in_entries<'a>(
    entries: &'a [(String, JsonValue)],
    key: &str,
    case_sensitive: bool,
) -> Option<&'a JsonValue> {
    if case_sensitive {
        entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    } else {
        entries
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }
}

fn compare_double(a: f64, b: f64) -> bool {
    let max_val = a.abs().max(b.abs());
    (a - b).abs() <= max_val * f64::EPSILON
}

// ---- Number helpers ----

fn make_number(num: f64) -> JsonValue {
    let int_value = if num >= i32::MAX as f64 {
        i32::MAX
    } else if num <= i32::MIN as f64 {
        i32::MIN
    } else {
        num as i32
    };
    JsonValue::Number { value: num, int_value }
}

fn create_int_array(numbers: &[i32]) -> JsonValue {
    JsonValue::Array(numbers.iter().map(|&n| make_number(n as f64)).collect())
}

fn create_double_array(numbers: &[f64]) -> JsonValue {
    JsonValue::Array(numbers.iter().map(|&n| make_number(n)).collect())
}

// ---- Parser ----

const NESTING_LIMIT: usize = 1000;

struct Parser<'a> {
    input: &'a [u8],
    offset: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            input: input.as_bytes(),
            offset: 0,
            depth: 0,
        }
    }

    fn can_access(&self, index: usize) -> bool {
        self.offset + index < self.input.len()
    }

    fn current(&self) -> u8 {
        if self.offset < self.input.len() {
            self.input[self.offset]
        } else {
            0
        }
    }

    fn skip_whitespace(&mut self) {
        while self.offset < self.input.len() && self.input[self.offset] <= b' ' {
            self.offset += 1;
        }
    }

    fn skip_bom(&mut self) {
        if self.input.len() >= 3
            && self.input[0] == 0xEF
            && self.input[1] == 0xBB
            && self.input[2] == 0xBF
        {
            self.offset = 3;
        }
    }

    fn parse(&mut self) -> Option<JsonValue> {
        self.skip_bom();
        self.skip_whitespace();
        self.parse_value()
    }

    fn parse_value(&mut self) -> Option<JsonValue> {
        if !self.can_access(0) {
            return None;
        }
        let remaining = &self.input[self.offset..];
        if remaining.starts_with(b"null") {
            self.offset += 4;
            return Some(JsonValue::Null);
        }
        if remaining.starts_with(b"false") {
            self.offset += 5;
            return Some(JsonValue::Bool(false));
        }
        if remaining.starts_with(b"true") {
            self.offset += 4;
            return Some(JsonValue::Bool(true));
        }
        match self.current() {
            b'"' => self.parse_string(),
            b'-' | b'0'..=b'9' => self.parse_number(),
            b'[' => self.parse_array(),
            b'{' => self.parse_object(),
            _ => None,
        }
    }

    fn parse_number(&mut self) -> Option<JsonValue> {
        let start = self.offset;
        while self.can_access(0) {
            match self.current() {
                b'0'..=b'9' | b'+' | b'-' | b'e' | b'E' | b'.' => self.offset += 1,
                _ => break,
            }
        }
        if self.offset == start {
            return None;
        }
        let s = std::str::from_utf8(&self.input[start..self.offset]).ok()?;
        let value: f64 = s.parse().ok()?;
        Some(make_number(value))
    }

    fn parse_hex4(input: &[u8]) -> Option<u32> {
        if input.len() < 4 {
            return None;
        }
        let mut h: u32 = 0;
        for i in 0..4 {
            let digit = match input[i] {
                b'0'..=b'9' => (input[i] - b'0') as u32,
                b'A'..=b'F' => 10 + (input[i] - b'A') as u32,
                b'a'..=b'f' => 10 + (input[i] - b'a') as u32,
                _ => return None,
            };
            h = (h << 4) | digit;
        }
        Some(h)
    }

    fn parse_string(&mut self) -> Option<JsonValue> {
        if self.current() != b'"' {
            return None;
        }
        self.offset += 1;
        let mut result = Vec::new();
        loop {
            if !self.can_access(0) {
                return None;
            }
            let c = self.current();
            if c == b'"' {
                self.offset += 1;
                let s = String::from_utf8(result).ok()?;
                return Some(JsonValue::Str(s));
            }
            if c == b'\\' {
                self.offset += 1;
                if !self.can_access(0) {
                    return None;
                }
                match self.current() {
                    b'b' => {
                        result.push(b'\x08');
                        self.offset += 1;
                    }
                    b'f' => {
                        result.push(b'\x0C');
                        self.offset += 1;
                    }
                    b'n' => {
                        result.push(b'\n');
                        self.offset += 1;
                    }
                    b'r' => {
                        result.push(b'\r');
                        self.offset += 1;
                    }
                    b't' => {
                        result.push(b'\t');
                        self.offset += 1;
                    }
                    b'"' => {
                        result.push(b'"');
                        self.offset += 1;
                    }
                    b'\\' => {
                        result.push(b'\\');
                        self.offset += 1;
                    }
                    b'/' => {
                        result.push(b'/');
                        self.offset += 1;
                    }
                    b'u' => {
                        self.offset += 1;
                        if self.offset + 4 > self.input.len() {
                            return None;
                        }
                        let first = Self::parse_hex4(&self.input[self.offset..])?;
                        self.offset += 4;
                        let codepoint;
                        if (0xD800..=0xDBFF).contains(&first) {
                            if self.offset + 6 > self.input.len() {
                                return None;
                            }
                            if self.input[self.offset] != b'\\'
                                || self.input[self.offset + 1] != b'u'
                            {
                                return None;
                            }
                            self.offset += 2;
                            let second = Self::parse_hex4(&self.input[self.offset..])?;
                            self.offset += 4;
                            if !(0xDC00..=0xDFFF).contains(&second) {
                                return None;
                            }
                            codepoint =
                                0x10000 + (((first & 0x3FF) << 10) | (second & 0x3FF));
                        } else if (0xDC00..=0xDFFF).contains(&first) {
                            return None;
                        } else {
                            codepoint = first;
                        }
                        let ch = char::from_u32(codepoint)?;
                        let mut buf = [0u8; 4];
                        let encoded = ch.encode_utf8(&mut buf);
                        result.extend_from_slice(encoded.as_bytes());
                    }
                    _ => return None,
                }
            } else {
                result.push(c);
                self.offset += 1;
            }
        }
    }

    fn parse_array(&mut self) -> Option<JsonValue> {
        if self.current() != b'[' {
            return None;
        }
        if self.depth >= NESTING_LIMIT {
            return None;
        }
        self.depth += 1;
        self.offset += 1;
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.can_access(0) && self.current() == b']' {
            self.offset += 1;
            self.depth -= 1;
            return Some(JsonValue::Array(items));
        }
        loop {
            self.skip_whitespace();
            let val = self.parse_value()?;
            items.push(val);
            self.skip_whitespace();
            if !self.can_access(0) {
                return None;
            }
            if self.current() == b']' {
                self.offset += 1;
                self.depth -= 1;
                return Some(JsonValue::Array(items));
            }
            if self.current() != b',' {
                return None;
            }
            self.offset += 1;
        }
    }

    fn parse_object(&mut self) -> Option<JsonValue> {
        if self.current() != b'{' {
            return None;
        }
        if self.depth >= NESTING_LIMIT {
            return None;
        }
        self.depth += 1;
        self.offset += 1;
        self.skip_whitespace();
        let mut entries = Vec::new();
        if self.can_access(0) && self.current() == b'}' {
            self.offset += 1;
            self.depth -= 1;
            return Some(JsonValue::Object(entries));
        }
        loop {
            self.skip_whitespace();
            let key = match self.parse_string()? {
                JsonValue::Str(s) => s,
                _ => return None,
            };
            self.skip_whitespace();
            if !self.can_access(0) || self.current() != b':' {
                return None;
            }
            self.offset += 1;
            self.skip_whitespace();
            let val = self.parse_value()?;
            entries.push((key, val));
            self.skip_whitespace();
            if !self.can_access(0) {
                return None;
            }
            if self.current() == b'}' {
                self.offset += 1;
                self.depth -= 1;
                return Some(JsonValue::Object(entries));
            }
            if self.current() != b',' {
                return None;
            }
            self.offset += 1;
        }
    }
}

fn cjson_parse(input: &str) -> Option<JsonValue> {
    Parser::new(input).parse()
}

// ---- Printer ----

fn print_string_escaped(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.bytes() {
        match c {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\x08' => out.push_str("\\b"),
            b'\x0C' => out.push_str("\\f"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            c if c < 32 => {
                out.push_str(&format!("\\u{:04x}", c));
            }
            _ => out.push(c as char),
        }
    }
    out.push('"');
    out
}

/// Format like C's %.*g: use fixed notation if exponent is in [-4, precision),
/// otherwise use scientific notation. Trailing zeros are stripped.
fn format_g(value: f64, precision: usize) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let exp = value.abs().log10().floor() as i32;
    let s = if exp >= 0 && (exp as usize) < precision {
        // Fixed notation
        let decimal_digits = if precision as i32 - 1 - exp > 0 {
            (precision as i32 - 1 - exp) as usize
        } else {
            0
        };
        format!("{:.*}", decimal_digits, value)
    } else if exp < 0 && exp >= -4 {
        let decimal_digits = precision as i32 - 1 - exp;
        format!("{:.*}", decimal_digits as usize, value)
    } else {
        // Scientific notation
        let s = format!("{:.*e}", precision - 1, value);
        // C uses e+XX format (two-digit exponent minimum)
        // Rust uses e notation, need to match C format
        if let Some(pos) = s.find('e') {
            let mantissa = &s[..pos];
            let exp_str = &s[pos + 1..];
            let exp_val: i32 = exp_str.parse().unwrap_or(0);
            let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
            if exp_val == 0 {
                mantissa.to_string()
            } else {
                format!("{}e{:+03}", mantissa, exp_val)
            }
        } else {
            s
        }
    };
    // Strip trailing zeros after decimal point
    if s.contains('.') && !s.contains('e') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else {
        s
    }
}

fn print_number(value: f64, int_value: i32) -> String {
    if value.is_nan() || value.is_infinite() {
        return "null".to_string();
    }
    if value == int_value as f64 {
        return format!("{}", int_value);
    }
    // Match C's sprintf("%1.15g") behavior
    let s15 = format_g(value, 15);
    if let Ok(rt) = s15.parse::<f64>() {
        if compare_double(rt, value) {
            return s15;
        }
    }
    // Fall back to sprintf("%1.17g")
    format_g(value, 17)
}

fn print_value_unformatted(val: &JsonValue) -> String {
    match val {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(true) => "true".to_string(),
        JsonValue::Bool(false) => "false".to_string(),
        JsonValue::Number { value, int_value } => print_number(*value, *int_value),
        JsonValue::Str(s) => print_string_escaped(s),
        JsonValue::Raw(s) => s.clone(),
        JsonValue::Array(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&print_value_unformatted(item));
            }
            out.push(']');
            out
        }
        JsonValue::Object(entries) => {
            let mut out = String::from("{");
            for (i, (key, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&print_string_escaped(key));
                out.push(':');
                out.push_str(&print_value_unformatted(val));
            }
            out.push('}');
            out
        }
    }
}

fn print_value_formatted(val: &JsonValue, depth: usize) -> String {
    match val {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(true) => "true".to_string(),
        JsonValue::Bool(false) => "false".to_string(),
        JsonValue::Number { value, int_value } => print_number(*value, *int_value),
        JsonValue::Str(s) => print_string_escaped(s),
        JsonValue::Raw(s) => s.clone(),
        JsonValue::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&print_value_formatted(item, depth + 1));
            }
            out.push(']');
            out
        }
        JsonValue::Object(entries) => {
            if entries.is_empty() {
                return "{}".to_string();
            }
            let mut out = String::from("{\n");
            let indent = "\t".repeat(depth + 1);
            let closing_indent = "\t".repeat(depth);
            for (i, (key, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&indent);
                out.push_str(&print_string_escaped(key));
                out.push_str(":\t");
                out.push_str(&print_value_formatted(val, depth + 1));
            }
            out.push('\n');
            out.push_str(&closing_indent);
            out.push('}');
            out
        }
    }
}

fn cjson_print(val: &JsonValue) -> String {
    print_value_formatted(val, 0)
}

fn cjson_print_unformatted(val: &JsonValue) -> String {
    print_value_unformatted(val)
}

// ---- Minify ----

fn cjson_minify(json: &str) -> String {
    let bytes = json.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => {
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < bytes.len() {
                    i += 2;
                }
            }
            b'"' => {
                out.push(b'"');
                i += 1;
                while i < bytes.len() {
                    out.push(bytes[i]);
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 1;
                        out.push(bytes[i]);
                    }
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

// ---- Duplicate ----

fn cjson_duplicate(val: &JsonValue) -> JsonValue {
    val.clone()
}

// ---- Test harness ----

use std::sync::atomic::{AtomicI32, Ordering};

static TEST_COUNT: AtomicI32 = AtomicI32::new(0);
static PASS_COUNT: AtomicI32 = AtomicI32::new(0);

fn check(ok: bool, label: &str) {
    TEST_COUNT.fetch_add(1, Ordering::Relaxed);
    if ok {
        PASS_COUNT.fetch_add(1, Ordering::Relaxed);
        println!("PASS: {}", label);
    } else {
        println!("FAIL: {}", label);
    }
}

// ---- 1. Parse + print round-trip ----
fn test_parse_print() {
    let json =
        r#"{"name":"Noricum","version":1.7,"active":true,"tags":["rust","migration"],"meta":null}"#;
    let root = cjson_parse(json);
    check(root.is_some(), "parse basic object");
    let root = root.unwrap();
    let printed = cjson_print(&root);
    check(!printed.is_empty(), "print formatted");
    println!("Formatted:\n{}", printed);
    let unformatted = cjson_print_unformatted(&root);
    check(!unformatted.is_empty(), "print unformatted");
    println!("Unformatted: {}", unformatted);
}

// ---- 2. Type checking ----
fn test_type_checks() {
    let root = cjson_parse(
        r#"{"s":"hello","n":42,"b":true,"f":false,"null":null,"a":[1,2],"o":{"x":1}}"#,
    );
    check(root.is_some(), "parse for type checks");
    let root = root.unwrap();
    check(
        root.get_object_item("s").map_or(false, |v| v.is_string()),
        "is_string",
    );
    check(
        root.get_object_item("n").map_or(false, |v| v.is_number()),
        "is_number",
    );
    check(
        root.get_object_item("b").map_or(false, |v| v.is_true()),
        "is_true",
    );
    check(
        root.get_object_item("f").map_or(false, |v| v.is_false()),
        "is_false",
    );
    check(
        root.get_object_item("b").map_or(false, |v| v.is_bool()),
        "is_bool",
    );
    check(
        root.get_object_item("null").map_or(false, |v| v.is_null()),
        "is_null",
    );
    check(
        root.get_object_item("a").map_or(false, |v| v.is_array()),
        "is_array",
    );
    check(
        root.get_object_item("o").map_or(false, |v| v.is_object()),
        "is_object",
    );
}

// ---- 3. Getters ----
fn test_getters() {
    let root = cjson_parse(r#"{"name":"test","val":3.14}"#);
    check(root.is_some(), "parse for getters");
    let root = root.unwrap();
    let sv = root
        .get_object_item("name")
        .and_then(|v| v.get_string_value());
    check(sv == Some("test"), "get_string_value");
    let nv = root
        .get_object_item("val")
        .map_or(f64::NAN, |v| v.get_number_value());
    check(nv > 3.13 && nv < 3.15, "get_number_value");
}

// ---- 4. Creation API ----
fn test_creation() {
    let mut root = JsonValue::Object(Vec::new());
    root.add_item_to_object("tool", JsonValue::Str("Noricum".to_string()));
    root.add_item_to_object("score", make_number(100.0));
    root.add_item_to_object("safe", JsonValue::Bool(true));
    root.add_item_to_object("unsafe_blocks", JsonValue::Null);
    let mut tags = JsonValue::Array(Vec::new());
    tags.add_item_to_array(JsonValue::Str("c2rust".to_string()));
    tags.add_item_to_array(JsonValue::Str("llm".to_string()));
    tags.add_item_to_array(make_number(42.0));
    let tags_ref = tags.clone();
    root.add_item_to_object("tags", tags);
    let out = cjson_print_unformatted(&root);
    check(!out.is_empty(), "create complex object");
    println!("Created: {}", out);
    check(tags_ref.get_array_size() == 3, "array_size == 3");
    check(
        tags_ref
            .get_array_item(0)
            .and_then(|v| v.get_string_value())
            == Some("c2rust"),
        "array[0] == c2rust",
    );
    check(
        tags_ref
            .get_array_item(2)
            .map_or(false, |v| v.get_valuedouble() == 42.0),
        "array[2] == 42",
    );
}

// ---- 5. Array creators ----
fn test_array_creators() {
    let ints = [10, 20, 30, 40, 50];
    let doubles = [1.1, 2.2, 3.3];
    let ia = create_int_array(&ints);
    check(ia.get_array_size() == 5, "int_array size 5");
    check(
        ia.get_array_item(2).map_or(false, |v| v.get_valueint() == 30),
        "int_array[2] == 30",
    );
    let ia_str = cjson_print_unformatted(&ia);
    println!("IntArray: {}", ia_str);
    let da = create_double_array(&doubles);
    check(da.get_array_size() == 3, "double_array size 3");
    let da_str = cjson_print_unformatted(&da);
    println!("DoubleArray: {}", da_str);
}

// ---- 6. Tree manipulation ----
fn test_manipulation() {
    let root = cjson_parse(r#"{"a":1,"b":2,"c":3}"#);
    check(root.is_some(), "parse for manipulation");
    let mut root = root.unwrap();
    root.delete_item_from_object("b");
    check(root.get_object_item("b").is_none(), "delete b");
    root.add_item_to_object("d", JsonValue::Str("new".to_string()));
    check(root.get_object_item("d").is_some(), "add d");
    root.replace_item_in_object("a", make_number(99.0));
    check(
        root.get_object_item("a")
            .map_or(false, |v| v.get_valuedouble() == 99.0),
        "replace a=99",
    );
    check(root.has_object_item("c"), "has c");
    check(!root.has_object_item("b"), "no b");
    let out = cjson_print_unformatted(&root);
    println!("Manipulated: {}", out);
}

// ---- 7. Array manipulation ----
fn test_array_manipulation() {
    let mut arr = JsonValue::Array(Vec::new());
    arr.add_item_to_array(make_number(1.0));
    arr.add_item_to_array(make_number(2.0));
    arr.add_item_to_array(make_number(3.0));
    arr.add_item_to_array(make_number(4.0));
    arr.insert_item_in_array(1, make_number(99.0));
    check(arr.get_array_size() == 5, "insert grows array");
    check(
        arr.get_array_item(1)
            .map_or(false, |v| v.get_valuedouble() == 99.0),
        "inserted at [1]",
    );
    arr.delete_item_from_array(0);
    check(arr.get_array_size() == 4, "delete shrinks array");
    let detached = arr.detach_item_from_array(0);
    check(
        detached
            .as_ref()
            .map_or(false, |v| v.get_valuedouble() == 99.0),
        "detach returns item",
    );
    let out = cjson_print_unformatted(&arr);
    println!("Array: {}", out);
}

// ---- 8. Compare ----
fn test_compare() {
    let a = cjson_parse(r#"{"x":1,"y":[2,3]}"#).unwrap();
    let b = cjson_parse(r#"{"x":1,"y":[2,3]}"#).unwrap();
    let c = cjson_parse(r#"{"x":1,"y":[2,4]}"#).unwrap();
    check(compare_json(&a, &b, true), "compare equal");
    check(!compare_json(&a, &c, true), "compare not equal");
}

// ---- 9. Duplicate ----
fn test_duplicate() {
    let orig = cjson_parse(r#"{"key":"value","arr":[1,2,3]}"#).unwrap();
    let mut dup = cjson_duplicate(&orig);
    check(true, "duplicate not null");
    check(compare_json(&orig, &dup, true), "duplicate equals original");
    dup.replace_item_in_object("key", JsonValue::Str("changed".to_string()));
    check(
        orig.get_object_item("key")
            .and_then(|v| v.get_string_value())
            == Some("value"),
        "original unchanged after dup modify",
    );
    let out = cjson_print_unformatted(&dup);
    println!("Duplicate: {}", out);
}

// ---- 10. Minify ----
fn test_minify() {
    let json = "{\n  \"key\" : \"value\" ,\n  \"num\" : 42\n}";
    let minified = cjson_minify(json);
    check(
        minified == r#"{"key":"value","num":42}"#,
        "minify",
    );
    println!("Minified: {}", minified);
}

// ---- 11. Number edge cases ----
fn test_numbers() {
    let root = cjson_parse(
        r#"{"zero":0,"neg":-1,"big":1e10,"small":1e-10,"max":1.7976931348623157e308,"pi":3.14159265358979}"#,
    );
    check(root.is_some(), "parse numbers");
    let root = root.unwrap();
    check(
        root.get_object_item("zero")
            .map_or(false, |v| v.get_valuedouble() == 0.0),
        "zero",
    );
    check(
        root.get_object_item("neg")
            .map_or(false, |v| v.get_valuedouble() == -1.0),
        "neg",
    );
    check(
        root.get_object_item("pi")
            .map_or(false, |v| v.get_valuedouble() > 3.14),
        "pi",
    );
    let out = cjson_print_unformatted(&root);
    println!("Numbers: {}", out);
}

// ---- 12. String escapes ----
fn test_string_escapes() {
    let root = cjson_parse(r#"{"esc":"hello\nworld\ttab\"quote\\\\"}"#);
    check(root.is_some(), "parse escapes");
    let root = root.unwrap();
    let val = root
        .get_object_item("esc")
        .and_then(|v| v.get_string_value())
        .unwrap_or("");
    check(val.contains('\n'), "contains newline");
    check(val.contains('\t'), "contains tab");
    let out = cjson_print(&root);
    println!("Escapes:\n{}", out);
}

// ---- 13. Nested objects ----
fn test_nested() {
    let json = r#"{"level1":{"level2":{"level3":{"value":"deep"}}}}"#;
    let root = cjson_parse(json);
    check(root.is_some(), "parse nested");
    let root = root.unwrap();
    let val = root
        .get_object_item("level1")
        .and_then(|v| v.get_object_item("level2"))
        .and_then(|v| v.get_object_item("level3"))
        .and_then(|v| v.get_object_item("value"))
        .and_then(|v| v.get_string_value());
    check(val == Some("deep"), "deep nested access");
    let out = cjson_print_unformatted(&root);
    println!("Nested: {}", out);
}

// ---- 14. Empty structures ----
fn test_empty() {
    let empty_obj = cjson_parse("{}");
    check(
        empty_obj.as_ref().map_or(false, |v| v.is_object()),
        "parse empty object",
    );
    let eo = cjson_print_unformatted(&empty_obj.unwrap());
    println!("EmptyObj: {}", eo);
    let empty_arr = cjson_parse("[]");
    check(
        empty_arr.as_ref().map_or(false, |v| v.is_array()),
        "parse empty array",
    );
    let ea = cjson_print_unformatted(&empty_arr.unwrap());
    println!("EmptyArr: {}", ea);
}

// ---- 15. Parse errors ----
fn test_parse_errors() {
    check(cjson_parse("{invalid}").is_none(), "reject invalid json");
    check(cjson_parse("").is_none(), "reject empty string");
}

// ---- 16. Case-sensitive object access ----
fn test_case_sensitive() {
    let root = cjson_parse(r#"{"Key":1,"key":2,"KEY":3}"#);
    check(root.is_some(), "parse case-sensitive keys");
    let root = root.unwrap();
    let k1 = root.get_object_item_case_sensitive("Key");
    let k2 = root.get_object_item_case_sensitive("key");
    let k3 = root.get_object_item_case_sensitive("KEY");
    check(k1.map_or(false, |v| v.get_valueint() == 1), "Key == 1");
    check(k2.map_or(false, |v| v.get_valueint() == 2), "key == 2");
    check(k3.map_or(false, |v| v.get_valueint() == 3), "KEY == 3");
}

// ---- main ----
fn main() {
    println!("=== cJSON Full Migration Test ===\n");
    test_parse_print();
    test_type_checks();
    test_getters();
    test_creation();
    test_array_creators();
    test_manipulation();
    test_array_manipulation();
    test_compare();
    test_duplicate();
    test_minify();
    test_numbers();
    test_string_escapes();
    test_nested();
    test_empty();
    test_parse_errors();
    test_case_sensitive();
    let pass = PASS_COUNT.load(Ordering::Relaxed);
    let total = TEST_COUNT.load(Ordering::Relaxed);
    println!("\n=== Results: {}/{} passed ===", pass, total);
    std::process::exit(if pass == total { 0 } else { 1 });
}
