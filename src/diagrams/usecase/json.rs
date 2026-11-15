use super::types::OrdMap;
use crate::chars::{is_digit, is_hex};
use crate::errors::{ErrorCode, PeleError, Result};
use crate::text::entities::written_codes;
use crate::value::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct OrderedJson {
    pub value: Value,
    pub property_order: OrdMap<String, Vec<String>>,
}

struct WalkError {
    offset: usize,
    message: &'static str,
}

impl WalkError {
    fn new(offset: usize, message: &'static str) -> Self {
        Self { offset, message }
    }
}

struct Frame {
    is_object: bool,
    pointer: String,
    keys: Vec<String>,
    // For each key read so far, the stretch of `created` its value added.
    ranges: HashMap<String, (usize, usize)>,
    entries: Vec<(String, Value)>,
    items: Vec<Value>,
    key: String,
    from: usize,
    index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Value,
    Key,
    After,
}

fn escape_pointer_key(key: &str) -> String {
    if !key.contains('~') && !key.contains('/') {
        return key.to_string();
    }
    key.replace('~', "~0").replace('/', "~1")
}

fn skip_ws(s: &[char], at: &mut usize) {
    while *at < s.len() && matches!(s[*at], ' ' | '\t' | '\n' | '\r') {
        *at += 1;
    }
}

fn parse_hex4(s: &[char], at: usize) -> Option<u32> {
    if at + 4 > s.len() {
        return None;
    }
    let mut code = 0u32;
    for &ch in &s[at..at + 4] {
        if !is_hex(ch) {
            return None;
        }
        code = (code << 4) | ch.to_digit(16)?;
    }
    Some(code)
}

fn read_json_string(s: &[char], at: &mut usize) -> std::result::Result<String, WalkError> {
    let n = s.len();
    *at += 1;
    let mut out = String::new();
    while *at < n {
        let start = *at;
        let ch = s[*at];
        *at += 1;
        if ch == '"' {
            return Ok(out);
        }
        if (ch as u32) < 0x20 {
            return Err(WalkError::new(start, "Bad control character in string literal in JSON"));
        }
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let escape_at = *at;
        if *at >= n {
            return Err(WalkError::new(*at, "Unterminated string in JSON"));
        }
        let escape = s[*at];
        *at += 1;
        match escape {
            '"' | '\\' | '/' => out.push(escape),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let Some(code) = parse_hex4(s, *at) else {
                    return Err(WalkError::new(*at, "Bad Unicode escape in JSON"));
                };
                *at += 4;
                if (0xD800..=0xDBFF).contains(&code)
                    && *at + 6 <= n
                    && s[*at] == '\\'
                    && s[*at + 1] == 'u'
                {
                    if let Some(low) = parse_hex4(s, *at + 2) {
                        if (0xDC00..=0xDFFF).contains(&low) {
                            *at += 6;
                            let combined = 0x10000 + (((code - 0xD800) << 10) | (low - 0xDC00));
                            out.push(char::from_u32(combined).unwrap_or('\u{FFFD}'));
                            continue;
                        }
                    }
                }
                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            }
            _ => return Err(WalkError::new(escape_at, "Bad escaped character in JSON")),
        }
    }
    Err(WalkError::new(*at, "Unterminated string in JSON"))
}

fn read_literal(s: &[char], at: &mut usize, word: &str) -> std::result::Result<(), WalkError> {
    for (k, expected) in word.chars().enumerate() {
        if s.get(*at + k).copied() != Some(expected) {
            return Err(WalkError::new(*at + k, "Unexpected token in JSON"));
        }
    }
    *at += word.len();
    Ok(())
}

fn read_number(s: &[char], at: &mut usize) -> std::result::Result<f64, WalkError> {
    let start = *at;
    let n = s.len();
    if s.get(*at) == Some(&'-') {
        *at += 1;
    }
    match s.get(*at).copied() {
        Some('0') => *at += 1,
        Some(c) if is_digit(c) => {
            while *at < n && is_digit(s[*at]) {
                *at += 1;
            }
        }
        _ => return Err(WalkError::new(*at, "Invalid number in JSON")),
    }
    if s.get(*at) == Some(&'.') {
        *at += 1;
        if !s.get(*at).copied().is_some_and(is_digit) {
            return Err(WalkError::new(*at, "Unterminated fractional number in JSON"));
        }
        while *at < n && is_digit(s[*at]) {
            *at += 1;
        }
    }
    if matches!(s.get(*at), Some('e' | 'E')) {
        *at += 1;
        if matches!(s.get(*at), Some('+' | '-')) {
            *at += 1;
        }
        if !s.get(*at).copied().is_some_and(is_digit) {
            return Err(WalkError::new(*at, "Exponent part is missing a number in JSON"));
        }
        while *at < n && is_digit(s[*at]) {
            *at += 1;
        }
    }
    let raw: String = s[start..*at].iter().collect();
    raw.parse::<f64>()
        .map_err(|_| WalkError::new(start, "Invalid number in JSON"))
}

// Strict non-recursive JSON parser that also records each object's written key order by JSON Pointer.
// Nesting is kept on an explicit heap stack so depth is bounded by memory, never the call stack.
fn parse_and_order(text: &str) -> std::result::Result<(Value, OrdMap<String, Vec<String>>), WalkError> {
    let s: Vec<char> = text.chars().collect();
    let n = s.len();
    let mut order: OrdMap<String, Vec<String>> = OrdMap::new();
    let mut created: Vec<String> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut at = 0usize;
    let mut pointer = String::new();
    let mut state = State::Value;
    let mut completed: Option<Value> = None;

    skip_ws(&s, &mut at);
    loop {
        match state {
            State::Value => {
                skip_ws(&s, &mut at);
                let Some(&c) = s.get(at) else {
                    return Err(WalkError::new(at, "Unexpected end of JSON input"));
                };
                state = State::After;
                match c {
                    '{' => {
                        at += 1;
                        order.insert(pointer.clone(), Vec::new());
                        created.push(pointer.clone());
                        skip_ws(&s, &mut at);
                        if s.get(at) == Some(&'}') {
                            at += 1;
                            completed = Some(Value::Obj(Vec::new()));
                        } else {
                            stack.push(Frame {
                                is_object: true,
                                pointer: std::mem::take(&mut pointer),
                                keys: Vec::new(),
                                ranges: HashMap::new(),
                                entries: Vec::new(),
                                items: Vec::new(),
                                key: String::new(),
                                from: 0,
                                index: 0,
                            });
                            state = State::Key;
                        }
                    }
                    '[' => {
                        at += 1;
                        skip_ws(&s, &mut at);
                        if s.get(at) == Some(&']') {
                            at += 1;
                            completed = Some(Value::Arr(Vec::new()));
                        } else {
                            let child_ptr = format!("{}/0", pointer);
                            stack.push(Frame {
                                is_object: false,
                                pointer: std::mem::take(&mut pointer),
                                keys: Vec::new(),
                                ranges: HashMap::new(),
                                entries: Vec::new(),
                                items: Vec::new(),
                                key: String::new(),
                                from: 0,
                                index: 0,
                            });
                            pointer = child_ptr;
                            state = State::Value;
                        }
                    }
                    '"' => {
                        completed = Some(Value::Str(read_json_string(&s, &mut at)?));
                    }
                    't' => {
                        read_literal(&s, &mut at, "true")?;
                        completed = Some(Value::Bool(true));
                    }
                    'f' => {
                        read_literal(&s, &mut at, "false")?;
                        completed = Some(Value::Bool(false));
                    }
                    'n' => {
                        read_literal(&s, &mut at, "null")?;
                        completed = Some(Value::Null);
                    }
                    c if c == '-' || is_digit(c) => {
                        completed = Some(Value::Num(read_number(&s, &mut at)?));
                    }
                    _ => {
                        return Err(WalkError::new(at, "Unexpected token in JSON"));
                    }
                }
            }
            State::Key => {
                let frame = stack.last_mut().expect("JSON object frame must exist");
                if s.get(at) != Some(&'"') {
                    return Err(WalkError::new(
                        at,
                        if at >= n {
                            "Unexpected end of JSON input"
                        } else {
                            "Expected double-quoted property name in JSON"
                        },
                    ));
                }
                let key = read_json_string(&s, &mut at)?;
                // A repeated key keeps its first place in the order and takes the nested order of its last value.
                if let Some(&(r0, r1)) = frame.ranges.get(&key) {
                    for k in r0..r1 {
                        order.remove(&created[k]);
                    }
                } else {
                    frame.keys.push(key.clone());
                }
                frame.key = key.clone();
                frame.from = created.len();
                skip_ws(&s, &mut at);
                if s.get(at) != Some(&':') {
                    return Err(WalkError::new(at, "Expected ':' after property name in JSON"));
                }
                at += 1;
                pointer = format!("{}/{}", frame.pointer, escape_pointer_key(&key));
                state = State::Value;
            }
            State::After => {
                let Some(frame) = stack.last_mut() else {
                    break;
                };
                let val = completed.take().expect("completed JSON value must exist");
                if frame.is_object {
                    frame.ranges.insert(frame.key.clone(), (frame.from, created.len()));
                    match frame.entries.iter_mut().find(|(k, _)| *k == frame.key) {
                        Some(entry) => entry.1 = val,
                        None => frame.entries.push((frame.key.clone(), val)),
                    }
                } else {
                    frame.items.push(val);
                    frame.index += 1;
                }
                skip_ws(&s, &mut at);
                let closer = if frame.is_object { '}' } else { ']' };
                let Some(&c) = s.get(at) else {
                    return Err(WalkError::new(at, "Unexpected end of JSON input"));
                };
                if c == closer {
                    at += 1;
                    let popped = stack.pop().unwrap();
                    if popped.is_object {
                        order.insert(popped.pointer, popped.keys);
                        completed = Some(Value::Obj(popped.entries));
                    } else {
                        completed = Some(Value::Arr(popped.items));
                    }
                } else if c != ',' {
                    return Err(WalkError::new(
                        at,
                        if frame.is_object {
                            "Expected ',' or '}' after property value in JSON"
                        } else {
                            "Expected ',' or ']' after array element in JSON"
                        },
                    ));
                } else {
                    at += 1;
                    skip_ws(&s, &mut at);
                    let is_obj = frame.is_object;
                    pointer = format!("{}/{}", frame.pointer, frame.index);
                    state = if is_obj { State::Key } else { State::Value };
                }
            }
        }
    }
    skip_ws(&s, &mut at);
    if at != n {
        return Err(WalkError::new(at, "Unexpected non-whitespace character after JSON"));
    }
    Ok((completed.unwrap_or(Value::Null), order))
}

fn location_at(s: &[char], offset: usize, mut line: usize, mut column: usize) -> (usize, usize) {
    let end = offset.min(s.len());
    for i in 0..end {
        let c = s[i];
        if c == '\r' {
            line += 1;
            column = 1;
        } else if c == '\n' {
            if i == 0 || s[i - 1] != '\r' {
                line += 1;
                column = 1;
            }
        } else {
            column += c.len_utf16();
        }
    }
    (line, column)
}

fn json_error(message: &str, line: usize, column: usize) -> PeleError {
    let mut err = PeleError::new(
        format!("{} (line {}, column {})", message, line, column),
        ErrorCode::Syntax,
    )
    .of("usecase");
    err.line = line;
    err.column = column;
    err
}

// Parses the body of a `json` node. `origin` gives the line and column the text starts at in the diagram.
pub fn parse_ordered_json(
    text: &str,
    origin: impl FnOnce() -> (usize, usize),
) -> Result<OrderedJson> {
    match parse_and_order(text) {
        Ok((value, property_order)) => {
            if !value.is_object() {
                let (line, column) = origin();
                return Err(json_error("JSON value must have an object root", line, column));
            }
            Ok(OrderedJson { value, property_order })
        }
        Err(first_err) => {
            // Reparse restored entity codes so error messages and positions match the source.
            let shown = written_codes(text);
            let shown_chars: Vec<char> = shown.chars().collect();
            let err = if shown != text {
                parse_and_order(&shown).err().unwrap_or(first_err)
            } else {
                first_err
            };
            let (base_line, base_col) = origin();
            let (line, column) = location_at(&shown_chars, err.offset, base_line, base_col);
            Err(json_error(&format!("Invalid JSON: {}", err.message), line, column))
        }
    }
}
