// Configuration values, as read from front matter (YAML), init directives (JSON) and the host.

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    // Keys keep the order they were written in.
    Obj(Vec<(String, Value)>),
}

pub static NULL: Value = Value::Null;

impl Value {
    pub fn object() -> Value {
        Value::Obj(Vec::new())
    }

    pub fn get(&self, key: &str) -> &Value {
        match self {
            Value::Obj(list) => list.iter().find(|(k, _)| k == key).map_or(&NULL, |(_, v)| v),
            _ => &NULL,
        }
    }

    pub fn has(&self, key: &str) -> bool {
        matches!(self, Value::Obj(list) if list.iter().any(|(k, _)| k == key))
    }

    pub fn set(&mut self, key: &str, value: Value) {
        if let Value::Obj(list) = self {
            match list.iter_mut().find(|(k, _)| k == key) {
                Some(entry) => entry.1 = value,
                None => list.push((key.to_string(), value)),
            }
        }
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Obj(_))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    // JavaScript truthiness.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            _ => true,
        }
    }

    // What JavaScript's String() gives for a scalar.
    pub fn to_text(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Num(n) => crate::svg::builder::js_number(*n),
            Value::Str(s) => s.clone(),
            Value::Arr(list) => list.iter().map(Value::to_text).collect::<Vec<_>>().join(","),
            Value::Obj(_) => "[object Object]".into(),
        }
    }

    // Number() of a scalar.
    pub fn to_number(&self) -> f64 {
        match self {
            Value::Null => 0.0,
            Value::Bool(b) => *b as u8 as f64,
            Value::Num(n) => *n,
            Value::Str(s) => {
                let t = s.trim();
                if t.is_empty() {
                    0.0
                } else {
                    t.parse().unwrap_or(f64::NAN)
                }
            }
            _ => f64::NAN,
        }
    }

    // Deep merge of objects; anything else is replaced.
    pub fn merge(&mut self, source: &Value) {
        let Value::Obj(entries) = source else { return };
        for (key, value) in entries {
            if matches!(key.as_str(), "__proto__" | "constructor" | "prototype") {
                continue;
            }
            if value.is_object() {
                if !self.get(key).is_object() {
                    self.set(key, Value::object());
                }
                if let Value::Obj(list) = self {
                    if let Some(entry) = list.iter_mut().find(|(k, _)| k == key) {
                        entry.1.merge(value);
                    }
                }
            } else {
                self.set(key, value.clone());
            }
        }
    }
}

// ---- JSON ----

pub fn parse_json(text: &str) -> Result<Value, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut p = Json { s: &chars, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != chars.len() {
        return Err(format!("Unexpected text at position {}.", p.i));
    }
    Ok(v)
}

struct Json<'a> {
    s: &'a [char],
    i: usize,
}

impl Json<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], ' ' | '\t' | '\n' | '\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn word(&mut self, w: &str, v: Value) -> Result<Value, String> {
        let n = w.chars().count();
        if self.i + n <= self.s.len() && self.s[self.i..self.i + n].iter().copied().eq(w.chars()) {
            self.i += n;
            Ok(v)
        } else {
            Err(format!("Unexpected token at position {}.", self.i))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some('{') => {
                self.i += 1;
                let mut out = Vec::new();
                self.ws();
                if self.peek() == Some('}') {
                    self.i += 1;
                    return Ok(Value::Obj(out));
                }
                loop {
                    self.ws();
                    if self.peek() != Some('"') {
                        return Err(format!("Expected a key at position {}.", self.i));
                    }
                    let key = self.string()?;
                    self.ws();
                    if self.peek() != Some(':') {
                        return Err(format!("Expected ':' at position {}.", self.i));
                    }
                    self.i += 1;
                    self.ws();
                    let v = self.value()?;
                    match out.iter_mut().find(|(k, _): &&mut (String, Value)| *k == key) {
                        Some(entry) => entry.1 = v,
                        None => out.push((key, v)),
                    }
                    self.ws();
                    match self.peek() {
                        Some(',') => self.i += 1,
                        Some('}') => {
                            self.i += 1;
                            return Ok(Value::Obj(out));
                        }
                        _ => return Err(format!("Expected ',' or '}}' at position {}.", self.i)),
                    }
                }
            }
            Some('[') => {
                self.i += 1;
                let mut out = Vec::new();
                self.ws();
                if self.peek() == Some(']') {
                    self.i += 1;
                    return Ok(Value::Arr(out));
                }
                loop {
                    self.ws();
                    out.push(self.value()?);
                    self.ws();
                    match self.peek() {
                        Some(',') => self.i += 1,
                        Some(']') => {
                            self.i += 1;
                            return Ok(Value::Arr(out));
                        }
                        _ => return Err(format!("Expected ',' or ']' at position {}.", self.i)),
                    }
                }
            }
            Some('"') => Ok(Value::Str(self.string()?)),
            Some('t') => self.word("true", Value::Bool(true)),
            Some('f') => self.word("false", Value::Bool(false)),
            Some('n') => self.word("null", Value::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => {
                let start = self.i;
                self.i += 1;
                while self.i < self.s.len() && matches!(self.s[self.i], '0'..='9' | '.' | 'e' | 'E' | '+' | '-') {
                    self.i += 1;
                }
                let text: String = self.s[start..self.i].iter().collect();
                text.parse().map(Value::Num).map_err(|_| format!("Bad number at position {}.", start))
            }
            _ => Err(format!("Unexpected token at position {}.", self.i)),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else { return Err("Unterminated string.".into()) };
            self.i += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let Some(e) = self.peek() else { return Err("Unterminated string.".into()) };
                    self.i += 1;
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let unit = |p: &mut Self| -> Result<u32, String> {
                                if p.i + 4 > p.s.len() {
                                    return Err("Bad escape.".into());
                                }
                                let hex: String = p.s[p.i..p.i + 4].iter().collect();
                                p.i += 4;
                                u32::from_str_radix(&hex, 16).map_err(|_| "Bad escape.".to_string())
                            };
                            let mut code = unit(self)?;
                            if (0xd800..0xdc00).contains(&code) && self.peek() == Some('\\') && self.s.get(self.i + 1) == Some(&'u') {
                                self.i += 2;
                                let low = unit(self)?;
                                code = 0x10000 + ((code - 0xd800) << 10) + (low.wrapping_sub(0xdc00) & 0x3ff);
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => out.push(other),
                    }
                }
                '\n' | '\r' => return Err("Line break in a string.".into()),
                _ => out.push(c),
            }
        }
    }
}

// ---- YAML ----
//
// The subset of YAML that front matter and node metadata use: block and flow mappings and
// sequences, plain and quoted scalars, literal and folded block scalars, and comments.

pub fn parse_yaml(text: &str) -> Result<Value, String> {
    let mut lines: Vec<Line> = Vec::new();
    for (number, raw) in text.split('\n').enumerate() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if raw.contains('\t') && raw.trim_start_matches(' ').starts_with('\t') {
            return Err(format!("Tabs cannot indent YAML (line {}).", number + 1));
        }
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        lines.push(Line { indent, text: raw.to_string(), number: number + 1 });
    }
    let mut p = Yaml { lines, at: 0 };
    p.skip_blank();
    if p.at >= p.lines.len() {
        return Ok(Value::Null);
    }
    if p.lines[p.at].text.trim() == "---" {
        p.at += 1;
        p.skip_blank();
        if p.at >= p.lines.len() {
            return Ok(Value::Null);
        }
    }
    let indent = p.lines[p.at].indent;
    let v = p.block(indent)?;
    p.skip_blank();
    if p.at < p.lines.len() {
        return Err(format!("Unexpected text on line {}.", p.lines[p.at].number));
    }
    Ok(v)
}

struct Line {
    indent: usize,
    text: String,
    number: usize,
}

struct Yaml {
    lines: Vec<Line>,
    at: usize,
}

fn strip_comment(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut quote = 0u8;
    for (i, &b) in bytes.iter().enumerate() {
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
        } else if (b == b'"' || b == b'\'') && (i == 0 || matches!(bytes[i - 1], b' ' | b':' | b'[' | b'{' | b',')) {
            quote = b;
        } else if b == b'#' && (i == 0 || bytes[i - 1] == b' ') {
            return &s[..i];
        }
    }
    s
}

// The position of the `:` that ends a mapping key, if the text is a `key: value` entry.
fn key_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut i = 0;
    if bytes[0] == b'"' || bytes[0] == b'\'' {
        let q = bytes[0];
        i = 1;
        while i < bytes.len() && bytes[i] != q {
            if q == b'"' && bytes[i] == b'\\' {
                i += 1;
            }
            i += 1;
        }
        i += 1;
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        return (i < bytes.len() && bytes[i] == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ')).then_some(i);
    }
    if matches!(bytes[0], b'[' | b'{') {
        return None;
    }
    while i < bytes.len() {
        if bytes[i] == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn scalar(text: &str) -> Result<Value, String> {
    let s = text.trim();
    if s.is_empty() || s == "~" || s == "null" || s == "Null" || s == "NULL" {
        return Ok(Value::Null);
    }
    match s {
        "true" | "True" | "TRUE" => return Ok(Value::Bool(true)),
        "false" | "False" | "FALSE" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let first = s.as_bytes()[0];
    if first == b'"' {
        if s.len() < 2 || !s.ends_with('"') {
            return Err("Unterminated double-quoted string.".into());
        }
        let mut out = String::new();
        let mut chars = s[1..s.len() - 1].chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).unwrap_or('\u{fffd}'));
                }
                Some(other) => out.push(other),
                None => {}
            }
        }
        return Ok(Value::Str(out));
    }
    if first == b'\'' {
        if s.len() < 2 || !s.ends_with('\'') {
            return Err("Unterminated single-quoted string.".into());
        }
        return Ok(Value::Str(s[1..s.len() - 1].replace("''", "'")));
    }
    let numeric = s.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+' | b'e' | b'E'))
        && s.bytes().any(|b| b.is_ascii_digit());
    if numeric {
        if let Ok(n) = s.parse::<f64>() {
            return Ok(Value::Num(n));
        }
    }
    if let Some(hex) = s.strip_prefix("0x") {
        if let Ok(n) = u64::from_str_radix(hex, 16) {
            return Ok(Value::Num(n as f64));
        }
    }
    Ok(Value::Str(s.to_string()))
}

// A flow collection or scalar written on one logical line.
struct Flow<'a> {
    s: &'a [char],
    i: usize,
}

impl Flow<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_whitespace() {
            self.i += 1;
        }
    }

    fn quoted(&mut self) -> String {
        let q = self.s[self.i];
        let start = self.i;
        self.i += 1;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if c == '\\' && q == '"' {
                self.i += 2;
                continue;
            }
            if c == q {
                if q == '\'' && self.s.get(self.i + 1) == Some(&'\'') {
                    self.i += 2;
                    continue;
                }
                self.i += 1;
                break;
            }
            self.i += 1;
        }
        self.i = self.i.min(self.s.len());
        self.s[start..self.i].iter().collect()
    }

    // A scalar up to one of `stops` at nesting depth zero.
    fn plain(&mut self, stops: &[char], key: bool) -> String {
        let start = self.i;
        while self.i < self.s.len() {
            let c = self.s[self.i];
            if stops.contains(&c) {
                break;
            }
            if key && c == ':' && self.s.get(self.i + 1).map_or(true, |n| n.is_whitespace() || *n == ',' || *n == '}') {
                break;
            }
            self.i += 1;
        }
        self.s[start..self.i].iter().collect::<String>().trim().to_string()
    }

    fn value(&mut self, stops: &[char]) -> Result<Value, String> {
        self.ws();
        match self.s.get(self.i) {
            Some('{') => {
                self.i += 1;
                let mut out: Vec<(String, Value)> = Vec::new();
                loop {
                    self.ws();
                    match self.s.get(self.i) {
                        None => return Err("Unterminated flow mapping.".into()),
                        Some('}') => {
                            self.i += 1;
                            return Ok(Value::Obj(out));
                        }
                        Some(',') => {
                            self.i += 1;
                            continue;
                        }
                        _ => {}
                    }
                    let key = if matches!(self.s[self.i], '"' | '\'') {
                        match scalar(&self.quoted())? {
                            Value::Str(s) => s,
                            other => other.to_text(),
                        }
                    } else {
                        self.plain(&[',', '}'], true)
                    };
                    self.ws();
                    let v = if self.s.get(self.i) == Some(&':') {
                        self.i += 1;
                        self.value(&[',', '}'])?
                    } else {
                        Value::Null
                    };
                    match out.iter_mut().find(|(k, _)| *k == key) {
                        Some(entry) => entry.1 = v,
                        None => out.push((key, v)),
                    }
                }
            }
            Some('[') => {
                self.i += 1;
                let mut out = Vec::new();
                loop {
                    self.ws();
                    match self.s.get(self.i) {
                        None => return Err("Unterminated flow sequence.".into()),
                        Some(']') => {
                            self.i += 1;
                            return Ok(Value::Arr(out));
                        }
                        Some(',') => {
                            self.i += 1;
                            continue;
                        }
                        _ => out.push(self.value(&[',', ']'])?),
                    }
                }
            }
            Some('"') | Some('\'') => {
                let text = self.quoted();
                scalar(&text)
            }
            _ => scalar(&self.plain(stops, false)),
        }
    }
}

impl Yaml {
    fn blank(&self, i: usize) -> bool {
        let t = self.lines[i].text.trim();
        t.is_empty() || t.starts_with('#')
    }

    fn skip_blank(&mut self) {
        while self.at < self.lines.len() && self.blank(self.at) {
            self.at += 1;
        }
    }

    fn block(&mut self, indent: usize) -> Result<Value, String> {
        self.skip_blank();
        if self.at >= self.lines.len() {
            return Ok(Value::Null);
        }
        let text = self.lines[self.at].text[self.lines[self.at].indent..].to_string();
        if text == "-" || text.starts_with("- ") {
            return self.sequence(indent);
        }
        let content = strip_comment(&text).trim_end().to_string();
        if !matches!(content.as_bytes()[0], b'{' | b'[') && key_end(&content).is_some() {
            return self.mapping(indent);
        }
        // A scalar or a flow collection, which may run over several lines.
        self.inline_value(content, indent)
    }

    // Reads the rest of a value that started with `first` on the current line.
    fn inline_value(&mut self, first: String, indent: usize) -> Result<Value, String> {
        let number = self.lines[self.at].number;
        self.at += 1;
        let mut text = first;
        let opens = matches!(text.trim_start().as_bytes().first(), Some(b'{') | Some(b'['));
        if opens {
            while !balanced(&text) && self.at < self.lines.len() {
                text.push('\n');
                text.push_str(strip_comment(&self.lines[self.at].text));
                self.at += 1;
            }
        } else {
            // A plain scalar continues on more deeply indented lines.
            let quoted = matches!(text.trim_start().as_bytes().first(), Some(b'"') | Some(b'\''));
            while self.at < self.lines.len() && !self.blank(self.at) && self.lines[self.at].indent > indent && !quoted {
                text.push(' ');
                text.push_str(strip_comment(self.lines[self.at].text.trim()));
                self.at += 1;
            }
        }
        let chars: Vec<char> = text.chars().collect();
        let mut flow = Flow { s: &chars, i: 0 };
        let v = flow.value(&[]).map_err(|e| format!("{} (line {})", e, number))?;
        flow.ws();
        if flow.i < chars.len() {
            return Err(format!("Unexpected text on line {}.", number));
        }
        Ok(v)
    }

    fn block_scalar(&mut self, header: &str, parent: usize) -> Value {
        let folded = header.starts_with('>');
        let keep = header.contains('+');
        let strip = header.contains('-');
        self.at += 1;
        let mut body: Vec<String> = Vec::new();
        let mut indent: Option<usize> = None;
        while self.at < self.lines.len() {
            let line = &self.lines[self.at];
            if line.text.trim().is_empty() {
                body.push(String::new());
                self.at += 1;
                continue;
            }
            let own = *indent.get_or_insert(line.indent);
            if line.indent <= parent || line.indent < own {
                break;
            }
            body.push(line.text[own..].to_string());
            self.at += 1;
        }
        let mut trailing = 0;
        while body.last().is_some_and(|l| l.is_empty()) {
            body.pop();
            trailing += 1;
        }
        let mut out = if folded {
            let mut s = String::new();
            for (i, line) in body.iter().enumerate() {
                if i > 0 {
                    s.push(if line.is_empty() || body[i - 1].is_empty() { '\n' } else { ' ' });
                }
                s.push_str(line);
            }
            s
        } else {
            body.join("\n")
        };
        if !strip && !body.is_empty() {
            out.push('\n');
            if keep {
                out.push_str(&"\n".repeat(trailing));
            }
        }
        Value::Str(out)
    }

    // The value after `key:` or `- `, given the text that follows on the same line.
    fn after(&mut self, rest: &str, indent: usize, in_sequence: bool) -> Result<Value, String> {
        let rest = strip_comment(rest).trim().to_string();
        if rest.is_empty() {
            self.at += 1;
            self.skip_blank();
            if self.at < self.lines.len() {
                let next = self.lines[self.at].indent;
                let dash = self.lines[self.at].text.trim_start().starts_with('-');
                if next > indent || (!in_sequence && next == indent && dash) {
                    return self.block(next);
                }
            }
            return Ok(Value::Null);
        }
        if rest.starts_with('|') || rest.starts_with('>') {
            return Ok(self.block_scalar(&rest, indent));
        }
        self.inline_value(rest, indent)
    }

    fn mapping(&mut self, indent: usize) -> Result<Value, String> {
        let mut out: Vec<(String, Value)> = Vec::new();
        loop {
            self.skip_blank();
            if self.at >= self.lines.len() || self.lines[self.at].indent != indent {
                if self.at < self.lines.len() && self.lines[self.at].indent > indent {
                    return Err(format!("Unexpected indentation on line {}.", self.lines[self.at].number));
                }
                break;
            }
            let number = self.lines[self.at].number;
            let text = self.lines[self.at].text[indent..].to_string();
            let content = strip_comment(&text).trim_end().to_string();
            let Some(colon) = key_end(&content) else {
                return Err(format!("Expected a key on line {}.", number));
            };
            let key = match scalar(&content[..colon])? {
                Value::Str(s) => s,
                Value::Null => String::new(),
                other => other.to_text(),
            };
            let v = self.after(&text[colon + 1..], indent, false)?;
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some(_) => return Err(format!("Duplicate key \"{}\" on line {}.", key, number)),
                None => out.push((key, v)),
            }
        }
        Ok(Value::Obj(out))
    }

    fn sequence(&mut self, indent: usize) -> Result<Value, String> {
        let mut out = Vec::new();
        loop {
            self.skip_blank();
            if self.at >= self.lines.len() || self.lines[self.at].indent != indent {
                break;
            }
            let text = self.lines[self.at].text[indent..].to_string();
            if !(text == "-" || text.starts_with("- ")) {
                break;
            }
            let rest = text[1..].to_string();
            let inner = rest.trim_start();
            let content = strip_comment(inner).trim_end();
            if !content.is_empty() && !matches!(content.as_bytes()[0], b'{' | b'[' | b'|' | b'>') && key_end(content).is_some() {
                // `- key: value` starts a mapping whose keys line up with `key`.
                let offset = indent + 1 + (rest.len() - inner.len());
                let line = &mut self.lines[self.at];
                line.text = format!("{}{}", " ".repeat(offset), inner);
                line.indent = offset;
                out.push(self.mapping(offset)?);
            } else {
                out.push(self.after(&rest, indent, true)?);
            }
        }
        Ok(Value::Arr(out))
    }
}

fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    let mut quote = '\0';
    let mut prev = '\0';
    for c in text.chars() {
        if quote != '\0' {
            if c == quote && !(quote == '"' && prev == '\\') {
                quote = '\0';
            }
        } else if c == '"' || c == '\'' {
            quote = c;
        } else if c == '{' || c == '[' {
            depth += 1;
        } else if c == '}' || c == ']' {
            depth -= 1;
        }
        prev = c;
    }
    depth <= 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json() {
        let v = parse_json(r#"{"theme":"dark","flowchart":{"curve":"linear","n":[1,2.5,true,null]}}"#).unwrap();
        assert_eq!(v.get("theme").as_str(), Some("dark"));
        assert_eq!(v.get("flowchart").get("curve").as_str(), Some("linear"));
    }

    #[test]
    fn yaml_blocks() {
        let v = parse_yaml("title: Hello world\nconfig:\n  flowchart:\n    curve: linear\n  look: \"neo\"\nlist:\n  - a\n  - b: 1\n    c: 2\n").unwrap();
        assert_eq!(v.get("title").as_str(), Some("Hello world"));
        assert_eq!(v.get("config").get("flowchart").get("curve").as_str(), Some("linear"));
        assert_eq!(v.get("config").get("look").as_str(), Some("neo"));
        let Value::Arr(list) = v.get("list") else { panic!() };
        assert_eq!(list[1].get("c").as_f64(), Some(2.0));
    }

    #[test]
    fn yaml_flow() {
        let v = parse_yaml("{\nshape: diam, label: \"A, b: c\", w: 20\n}").unwrap();
        assert_eq!(v.get("shape").as_str(), Some("diam"));
        assert_eq!(v.get("label").as_str(), Some("A, b: c"));
        assert_eq!(v.get("w").as_f64(), Some(20.0));
    }

    #[test]
    fn yaml_literal() {
        let v = parse_yaml("label: |\n  one\n  two\nshape: rect\n").unwrap();
        assert_eq!(v.get("label").as_str(), Some("one\ntwo\n"));
        assert_eq!(v.get("shape").as_str(), Some("rect"));
    }
}
