use crate::chars::is_space;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct JourneyTask {
    pub task: String,
    // As written: any number, or NaN when the score is not one.
    pub score: f64,
    pub people: Vec<String>,
    // Name of the section in force when the task was declared, '' before the first one.
    pub section: String,
}

#[derive(Debug, Clone, Default)]
pub struct JourneyDb {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub sections: Vec<String>,
    pub tasks: Vec<JourneyTask>,
    current_section: String,
}

// Everyone named on a task, once each, in UTF-16 code unit order.
pub fn journey_actors(tasks: &[JourneyTask]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    for task in tasks {
        for name in &task.people {
            if seen.insert(name.as_str()) {
                names.push(name.clone());
            }
        }
    }
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    names
}

fn collapse_newline_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        out.push(ch);
        i += 1;
        if ch == '\n' {
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
        }
    }
    out
}

// Matches JavaScript `Number(str)`.
fn js_parse_number(raw: &str) -> f64 {
    let s = raw.trim_matches(is_space);
    if s.is_empty() {
        return 0.0;
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return u128::from_str_radix(hex, 16)
                .map(|v| v as f64)
                .unwrap_or(f64::INFINITY);
        }
        return f64::NAN;
    }
    if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        if !bin.is_empty() && bin.bytes().all(|b| b == b'0' || b == b'1') {
            return u128::from_str_radix(bin, 2)
                .map(|v| v as f64)
                .unwrap_or(f64::INFINITY);
        }
        return f64::NAN;
    }
    if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        if !oct.is_empty() && oct.bytes().all(|b| matches!(b, b'0'..=b'7')) {
            return u128::from_str_radix(oct, 8)
                .map(|v| v as f64)
                .unwrap_or(f64::INFINITY);
        }
        return f64::NAN;
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    if matches!(bytes.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut frac_digits = 0;
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        let frac_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - frac_start;
    }
    if int_digits == 0 && frac_digits == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return f64::NAN;
        }
    }
    if i != bytes.len() {
        return f64::NAN;
    }
    s.parse::<f64>().unwrap_or(f64::NAN)
}

impl JourneyDb {
    pub fn set_diagram_title(&mut self, text: String) {
        self.title = Some(text);
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_spaces(text));
    }

    pub fn add_section(&mut self, text: String) {
        self.current_section = text.clone();
        self.sections.push(text);
    }

    // `data` is the rest of the line from its first colon: `:score` or `:score:actor, actor`.
    pub fn add_task(&mut self, task: String, data: &str) {
        let rest: String = data.chars().skip(1).collect();
        let pieces: Vec<&str> = rest.split(':').collect();
        let people = if pieces.len() == 1 {
            Vec::new()
        } else {
            pieces[1]
                .split(',')
                .map(|name| name.trim_matches(is_space).to_string())
                .collect()
        };
        let score = js_parse_number(pieces[0]);
        self.tasks.push(JourneyTask {
            task,
            score,
            people,
            section: self.current_section.clone(),
        });
    }

    pub fn get_actors(&self) -> Vec<String> {
        journey_actors(&self.tasks)
    }
}
