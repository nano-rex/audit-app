//! JSON values: parsing, building, and writing. Objects keep their key order, as the record
//! attributes they mirror do. The helpers follow Python's rules (truthiness, str(), int(),
//! equality), because the application's behaviour was defined in those terms.

use std::fmt::Write as _;

#[derive(Debug, Clone, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

pub static NULL: Value = Value::Null;

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        self.py_eq(other)
    }
}

impl Value {
    pub fn object() -> Value {
        Value::Object(Vec::new())
    }

    pub fn array() -> Value {
        Value::Array(Vec::new())
    }

    pub fn str(text: impl Into<String>) -> Value {
        Value::Str(text.into())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(items) => items.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    /// The member, or null when it is missing (Python's dict.get).
    pub fn at(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Value::Object(items) => items.iter_mut().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    /// Set a key of an object, keeping its place when it already exists.
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        let value = value.into();
        if let Value::Object(items) = self {
            if let Some(slot) = items.iter_mut().find(|(name, _)| name == key) {
                slot.1 = value;
            } else {
                items.push((key.to_string(), value));
            }
        }
    }

    /// Python's dict.setdefault: set only when missing.
    pub fn set_default(&mut self, key: &str, value: impl Into<Value>) {
        if !self.has(key) {
            self.set(key, value);
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        if let Value::Object(items) = self {
            if let Some(index) = items.iter().position(|(name, _)| name == key) {
                return Some(items.remove(index).1);
            }
        }
        None
    }

    /// Python's `a | b` for dicts: b's members replace or follow a's.
    pub fn merged(&self, other: &Value) -> Value {
        let mut result = if self.is_object() { self.clone() } else { Value::object() };
        if let Value::Object(items) = other {
            for (key, value) in items {
                result.set(key, value.clone());
            }
        }
        result
    }

    pub fn push(&mut self, value: impl Into<Value>) {
        if let Value::Array(items) = self {
            items.push(value.into());
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(value) => Some(*value as f64),
            Value::Float(value) => Some(*value),
            Value::Bool(value) => Some(*value as i64 as f64),
            Value::Str(text) => text.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(value) => Some(*value),
            Value::Float(value) => Some(*value as i64),
            Value::Bool(value) => Some(*value as i64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The array's items, or none for anything else (Python's `value or []` over a list).
    pub fn items(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            _ => &[],
        }
    }

    pub fn as_object(&self) -> Option<&Vec<(String, Value)>> {
        match self {
            Value::Object(items) => Some(items),
            _ => None,
        }
    }

    /// The object's members, or none for anything else.
    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Object(items) => items,
            _ => &[],
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    pub fn is_str(&self) -> bool {
        matches!(self, Value::Str(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }

    /// Python's truthiness: empty strings, zero, empty containers and null are false.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(value) => *value,
            Value::Int(value) => *value != 0,
            Value::Float(value) => *value != 0.0,
            Value::Str(text) => !text.is_empty(),
            Value::Array(items) => !items.is_empty(),
            Value::Object(items) => !items.is_empty(),
        }
    }

    /// Python's `a or b`.
    pub fn or<'a>(&'a self, other: &'a Value) -> &'a Value {
        if self.truthy() {
            self
        } else {
            other
        }
    }

    /// The value as Python's str() shows it.
    pub fn py_str(&self) -> String {
        match self {
            Value::Null => "None".into(),
            Value::Bool(value) => if *value { "True".into() } else { "False".into() },
            Value::Int(value) => value.to_string(),
            Value::Float(value) => float_repr(*value),
            Value::Str(text) => text.clone(),
            other => other.py_repr(),
        }
    }

    /// Python's repr() of containers, used only where Python would have shown one in text.
    pub fn py_repr(&self) -> String {
        match self {
            Value::Str(text) => format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'")),
            Value::Array(items) => format!("[{}]", items.iter().map(Value::py_repr).collect::<Vec<_>>().join(", ")),
            Value::Object(items) => format!("{{{}}}", items.iter().map(|(key, value)| format!("'{key}': {}", value.py_repr())).collect::<Vec<_>>().join(", ")),
            other => other.py_str(),
        }
    }

    /// Python's `str(value or "")`: the text of a truthy value, else empty.
    pub fn text(&self) -> String {
        if self.truthy() {
            self.py_str()
        } else {
            String::new()
        }
    }

    /// Python's int(): integers, floats truncated, booleans, and text holding an integer.
    pub fn py_int(&self) -> Result<i64, String> {
        match self {
            Value::Int(value) => Ok(*value),
            Value::Float(value) if value.is_finite() => Ok(value.trunc() as i64),
            Value::Float(_) => Err("cannot convert float NaN or infinity to integer".into()),
            Value::Bool(value) => Ok(*value as i64),
            Value::Str(text) => {
                let trimmed = text.trim();
                let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed).replace('_', "");
                if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) && !trimmed.contains("__") {
                    trimmed.replace('_', "").parse::<i64>().map_err(|_| format!("invalid literal for int() with base 10: {}", self.py_repr()))
                } else {
                    Err(format!("invalid literal for int() with base 10: {}", self.py_repr()))
                }
            }
            Value::Null => Err("int() argument must be a string, a bytes-like object or a real number, not 'NoneType'".into()),
            Value::Array(_) => Err("int() argument must be a string, a bytes-like object or a real number, not 'list'".into()),
            Value::Object(_) => Err("int() argument must be a string, a bytes-like object or a real number, not 'dict'".into()),
        }
    }

    /// Python's float().
    pub fn py_float(&self) -> Result<f64, String> {
        match self {
            Value::Int(value) => Ok(*value as f64),
            Value::Float(value) => Ok(*value),
            Value::Bool(value) => Ok(*value as i64 as f64),
            Value::Str(text) => {
                let trimmed = text.trim().to_ascii_lowercase();
                match trimmed.as_str() {
                    "nan" | "+nan" | "-nan" => Ok(f64::NAN),
                    "inf" | "+inf" | "infinity" | "+infinity" => Ok(f64::INFINITY),
                    "-inf" | "-infinity" => Ok(f64::NEG_INFINITY),
                    _ => trimmed.replace('_', "").parse::<f64>().map_err(|_| format!("could not convert string to float: {}", self.py_repr())),
                }
            }
            Value::Null => Err("float() argument must be a string or a real number, not 'NoneType'".into()),
            _ => Err("float() argument must be a string or a real number".into()),
        }
    }

    /// Python's ==: numbers compare by value (True == 1 == 1.0), containers member by member.
    pub fn py_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.py_eq(y)),
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len() && a.iter().all(|(key, value)| b.iter().any(|(other_key, other_value)| other_key == key && value.py_eq(other_value)))
            }
            (a, b) if is_numeric(a) && is_numeric(b) => match (a, b) {
                (Value::Float(_), _) | (_, Value::Float(_)) => a.as_f64() == b.as_f64(),
                _ => a.as_i64() == b.as_i64(),
            },
            _ => false,
        }
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        write_value(&mut out, self);
        out
    }

    pub fn to_json_bytes(&self) -> Vec<u8> {
        self.to_json().into_bytes()
    }
}

fn is_numeric(value: &Value) -> bool {
    matches!(value, Value::Int(_) | Value::Float(_) | Value::Bool(_))
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::Str(value.to_string())
    }
}

impl From<&String> for Value {
    fn from(value: &String) -> Self {
        Value::Str(value.clone())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::Str(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Int(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::Int(value as i64)
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Value::Int(value as i64)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<&Value> for Value {
    fn from(value: &Value) -> Self {
        value.clone()
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(value: Option<T>) -> Self {
        value.map(Into::into).unwrap_or(Value::Null)
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(value: Vec<T>) -> Self {
        Value::Array(value.into_iter().map(Into::into).collect())
    }
}

/// Build an object from (key, value) pairs.
pub fn obj<const N: usize>(items: [(&str, Value); N]) -> Value {
    Value::Object(items.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
}

/// A list of text values.
pub fn strings<S: AsRef<str>>(items: &[S]) -> Value {
    Value::Array(items.iter().map(|item| Value::Str(item.as_ref().to_string())).collect())
}

/// Python's repr of a float: shortest round-trip digits, always with a decimal point.
pub fn float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-4..1e16).contains(&magnitude) {
        // Python switches to exponent notation outside this range: 1e+16, 1e-05.
        let text = format!("{value:e}");
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let exponent: i32 = exponent.parse().unwrap_or(0);
        return format!("{mantissa}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs());
    }
    let text = format!("{value}");
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// Python's `{value:g}`: up to six significant digits, trailing zeros removed.
pub fn format_g(value: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    if !value.is_finite() {
        return float_repr(value);
    }
    let exponent = value.abs().log10().floor() as i32;
    if !(-4..6).contains(&exponent) {
        let text = format!("{:.5e}", value);
        let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
        let mantissa = trim_zeros(mantissa);
        let exp: i32 = exp.parse().unwrap_or(0);
        return format!("{mantissa}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let decimals = (5 - exponent).max(0) as usize;
    trim_zeros(&format!("{value:.decimals$}"))
}

fn trim_zeros(text: &str) -> String {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text.to_string()
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        let escape = match ch {
            '"' => "\\\"",
            '\\' => "\\\\",
            '\n' => "\\n",
            '\r' => "\\r",
            '\t' => "\\t",
            '\u{08}' => "\\b",
            '\u{0c}' => "\\f",
            // Line and paragraph separators are valid JSON but not valid JavaScript source.
            '\u{2028}' => "\\u2028",
            '\u{2029}' => "\\u2029",
            c if (c as u32) < 0x20 => {
                out.push_str(&text[start..index]);
                let _ = write!(out, "\\u{:04x}", c as u32);
                start = index + 1;
                continue;
            }
            _ => continue,
        };
        out.push_str(&text[start..index]);
        out.push_str(escape);
        start = index + ch.len_utf8();
    }
    out.push_str(&text[start..]);
    out.push('"');
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Int(number) => {
            let _ = write!(out, "{number}");
        }
        Value::Float(number) => {
            if number.is_finite() {
                out.push_str(&float_repr(*number));
            } else if number.is_nan() {
                out.push_str("NaN");
            } else if *number > 0.0 {
                out.push_str("Infinity");
            } else {
                out.push_str("-Infinity");
            }
        }
        Value::Str(text) => write_string(out, text),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        Value::Object(items) => {
            out.push('{');
            for (index, (key, item)) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(out, key);
                out.push(':');
                write_value(out, item);
            }
            out.push('}');
        }
    }
}

pub fn parse(text: &str) -> Result<Value, String> {
    parse_bytes(text.as_bytes())
}

/// Parse UTF-8 JSON text. Messages name the problem the way Python's json module does.
pub fn parse_bytes(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "'utf-8' codec can't decode the request body".to_string())?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut parser = Parser { bytes: text.as_bytes(), pos: 0, depth: 0 };
    parser.space();
    let value = parser.value()?;
    parser.space();
    if parser.pos != parser.bytes.len() {
        return Err(format!("Extra data: char {}", parser.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what}: char {}", self.pos))
    }

    fn expect(&mut self, word: &str) -> Result<(), String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(())
        } else {
            self.fail("Expecting value")
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.depth += 1;
        if self.depth > 500 {
            return self.fail("Too deeply nested");
        }
        let result = match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::Str),
            Some(b't') => self.expect("true").map(|_| Value::Bool(true)),
            Some(b'f') => self.expect("false").map(|_| Value::Bool(false)),
            Some(b'n') => self.expect("null").map(|_| Value::Null),
            Some(b'N') => self.expect("NaN").map(|_| Value::Float(f64::NAN)),
            Some(b'I') => self.expect("Infinity").map(|_| Value::Float(f64::INFINITY)),
            Some(b'-') if self.bytes[self.pos..].starts_with(b"-Infinity") => {
                self.pos += 9;
                Ok(Value::Float(f64::NEG_INFINITY))
            }
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            _ => self.fail("Expecting value"),
        };
        self.depth -= 1;
        result
    }

    fn object(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut items: Vec<(String, Value)> = Vec::new();
        self.space();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(items));
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                return self.fail("Expecting property name enclosed in double quotes");
            }
            let key = self.string()?;
            self.space();
            if self.peek() != Some(b':') {
                return self.fail("Expecting ':' delimiter");
            }
            self.pos += 1;
            self.space();
            let value = self.value()?;
            if let Some(slot) = items.iter_mut().find(|(name, _)| *name == key) {
                slot.1 = value;
            } else {
                items.push((key, value));
            }
            self.space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(items));
                }
                _ => return self.fail("Expecting ',' delimiter"),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.space();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.space();
            items.push(self.value()?);
            self.space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return self.fail("Expecting ',' delimiter"),
            }
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == digits || (self.bytes[digits] == b'0' && self.pos - digits > 1) {
            self.pos = start;
            return self.fail("Expecting value");
        }
        let mut float = false;
        if self.peek() == Some(b'.') && matches!(self.bytes.get(self.pos + 1), Some(b'0'..=b'9')) {
            float = true;
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mark = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                float = true;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            } else {
                self.pos = mark;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("0");
        if !float {
            if let Ok(value) = text.parse::<i64>() {
                return Ok(Value::Int(value));
            }
        }
        text.parse::<f64>().map(Value::Float).or_else(|_| self.fail("Expecting value"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.bytes.get(self.pos..self.pos + 4).ok_or_else(|| format!("Invalid \\uXXXX escape: char {}", self.pos))?;
        let text = std::str::from_utf8(digits).unwrap_or("");
        let code = u32::from_str_radix(text, 16).map_err(|_| format!("Invalid \\uXXXX escape: char {}", self.pos))?;
        self.pos += 4;
        Ok(code)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let start = self.pos;
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            // The input is valid UTF-8 and the stops are ASCII, so the run is too.
            out.push_str(std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or(""));
            match self.peek() {
                None => return self.fail("Unterminated string starting at"),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let escape = self.peek().ok_or_else(|| format!("Unterminated string: char {}", self.pos))?;
                    self.pos += 1;
                    match escape {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'/' => out.push('/'),
                        b'\\' => out.push('\\'),
                        b'"' => out.push('"'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code) && self.bytes[self.pos..].starts_with(b"\\u") {
                                let save = self.pos;
                                self.pos += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                } else {
                                    self.pos = save;
                                }
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => {
                            self.pos -= 2;
                            return self.fail("Invalid \\escape");
                        }
                    }
                }
                Some(_) => return self.fail("Invalid control character at"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_order_and_unicode() {
        let text = r#"{"b":1,"a":[true,null,2.5,"歌\n\"x\""],"c":{"d":-3}}"#;
        let value = parse(text).unwrap();
        assert_eq!(value.to_json(), text);
        assert_eq!(value.at("c").at("d").as_i64(), Some(-3));
        assert_eq!(parse(r#""🎵""#).unwrap().as_str(), Some("🎵"));
    }

    #[test]
    fn python_semantics() {
        assert_eq!(float_repr(1.0), "1.0");
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(format_g(70.0), "70");
        assert_eq!(format_g(72.5), "72.5");
        assert!(Value::Int(1).py_eq(&Value::Float(1.0)));
        assert!(Value::Bool(true).py_eq(&Value::Int(1)));
        assert_eq!(Value::str(" 42 ").py_int(), Ok(42));
        assert!(Value::str("4.2").py_int().is_err());
        assert!(!Value::str("").truthy());
        assert!(parse("[1,]").is_err());
        assert!(parse("01").is_err());
    }
}
