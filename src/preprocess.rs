use crate::errors::{Edit, ErrorCode, PeleError, Result};
use crate::value::{parse_json, parse_yaml, Value};

pub struct Preprocessed {
    pub text: String,
    pub title: Option<String>,
    pub config: Value,
    // The text before its comment lines are removed, and the number of front matter lines before it.
    pub with_comments: String,
    pub line_offset: usize,
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_line_break(c: Option<&char>) -> bool {
    matches!(c, Some('\n') | Some('\r'))
}

fn is_space(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_whitespace() || *c == '\u{feff}')
}

pub fn normalize_line_breaks(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

// Removes `%%` comment lines: /^\s*%%(?!{)[^\n]+\n?/gm
pub fn cleanup_comments(text: &str) -> String {
    cleanup_comments_inner(text, None)
}

fn cleanup_comments_inner(text: &str, mut steps: Option<&mut Vec<Vec<Edit>>>) -> String {
    let stripped = if !text.contains("%%") {
        if let Some(steps) = steps.as_deref_mut() {
            steps.push(Vec::new());
        }
        text.to_string()
    } else {
        let s: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut edits = Vec::new();
        let mut i = 0;
        while i < s.len() {
            // `i` is at the start of a line.
            let mut j = i;
            while j < s.len() && s[j].is_whitespace() {
                j += 1;
            }
            let comment = j + 2 < s.len() && s[j] == '%' && s[j + 1] == '%' && s[j + 2] != '{' && s[j + 2] != '\n';
            let mut end = if comment { j } else { i };
            while end < s.len() && s[end] != '\n' {
                end += 1;
            }
            if end < s.len() {
                end += 1;
            }
            if comment {
                edits.push((i, end, 0));
            } else {
                out.extend(&s[i..end]);
            }
            i = end;
        }
        if let Some(steps) = steps.as_deref_mut() {
            steps.push(edits);
        }
        out
    };
    let trimmed = stripped.trim_start();
    if let Some(steps) = steps {
        let drop_chars = stripped[..stripped.len() - trimmed.len()].chars().count();
        steps.push(vec![(0, drop_chars, 0)]);
    }
    trimmed.to_string()
}

struct FrontMatter {
    indent: String,
    body: String,
    length: usize,
}

fn starts_with(s: &[char], at: usize, needle: &[char]) -> bool {
    at + needle.len() <= s.len() && s[at..at + needle.len()] == *needle
}

// Finds a `---` fenced YAML block at the start of the text the way Mermaid's front matter regex does.
fn match_front_matter(s: &[char]) -> Option<FrontMatter> {
    let mut cursor = 0;
    while cursor < s.len() && is_space(s.get(cursor)) && !is_line_break(s.get(cursor)) {
        cursor += 1;
    }
    let indent: Vec<char> = s[..cursor].to_vec();
    if !starts_with(s, cursor, &['-', '-', '-']) {
        return None;
    }
    let mut opening_ends = Vec::new();
    let mut i = cursor + 3;
    while i < s.len() && is_space(s.get(i)) {
        if is_line_break(s.get(i)) {
            opening_ends.push(i);
        }
        i += 1;
    }
    if opening_ends.is_empty() {
        return None;
    }
    let mut fence = indent.clone();
    fence.extend(['-', '-', '-']);
    let closing_end = |i: usize| -> Option<usize> {
        if !is_line_break(s.get(i)) || !starts_with(s, i + 1, &fence) {
            return None;
        }
        let mut last = None;
        let mut j = i + 1 + fence.len();
        while j < s.len() && is_space(s.get(j)) {
            if is_line_break(s.get(j)) {
                last = Some(j);
            }
            j += 1;
        }
        last
    };
    let at_needle = |i: usize| s[i] == '\n' && starts_with(s, i + 1, &fence);

    let mut last_fence = None;
    let mut i = s.len();
    while i > opening_ends[0] + 1 {
        i -= 1;
        if at_needle(i) && closing_end(i).is_some() {
            last_fence = Some(i);
            break;
        }
    }
    let last_fence = last_fence?;

    let mut opening_end = opening_ends[0];
    for &candidate in &opening_ends {
        if candidate < last_fence {
            opening_end = candidate;
        }
    }
    let body_start = opening_end + 1;
    for i in body_start..=last_fence {
        if !at_needle(i) {
            continue;
        }
        if let Some(end) = closing_end(i) {
            return Some(FrontMatter {
                indent: indent.iter().collect(),
                body: s[body_start..i].iter().collect(),
                length: end + 1,
            });
        }
    }
    None
}

// Reads `%%{init: {...}}%%` and `%%{wrap}%%` directives into `config` and removes them.
fn directives(text: &str, config: &mut Value, steps: Option<&mut Vec<Vec<Edit>>>) -> String {
    if !text.contains("%%{") {
        return text.to_string();
    }
    let s: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut edits = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if !starts_with(&s, i, &['%', '%', '{']) {
            out.push(s[i]);
            i += 1;
            continue;
        }
        let mut j = i + 3;
        while j < s.len() && s[j].is_whitespace() {
            j += 1;
        }
        let word_start = j;
        while j < s.len() && is_word(s[j]) {
            j += 1;
        }
        if j == word_start {
            out.push(s[i]);
            i += 1;
            continue;
        }
        let kind: String = s[word_start..j].iter().collect::<String>().to_lowercase();
        while j < s.len() && s[j].is_whitespace() {
            j += 1;
        }
        if j < s.len() && s[j] == ':' {
            j += 1;
            while j < s.len() && s[j].is_whitespace() {
                j += 1;
            }
        }
        let mut body: Option<String> = None;
        if j < s.len() && is_word(s[j]) {
            while j < s.len() && is_word(s[j]) {
                j += 1;
            }
        } else {
            let start = j;
            while j < s.len() && !starts_with(&s, j, &['}', '%', '%']) {
                j += 1;
            }
            body = Some(s[start..j].iter().collect());
        }
        // Trailing whitespace belongs to the directive only when the closing fence follows it.
        let mut k = j;
        while k < s.len() && s[k].is_whitespace() {
            k += 1;
        }
        if starts_with(&s, k, &['}', '%', '%']) {
            j = k + 3;
        } else if body.is_none() {
            j = k;
        }
        if kind == "wrap" {
            config.set("wrap", Value::Bool(true));
        } else if kind == "init" || kind == "initialize" {
            if let Some(body) = body {
                // Mermaid ignores an init directive whose body is not valid JSON.
                if let Ok(args) = parse_json(body.replace('\'', "\"").trim()) {
                    if args.is_object() {
                        config.merge(&args);
                    }
                }
            }
        }
        let end = j.max(i + 1);
        edits.push((i, end, 0));
        i = end;
    }
    if let Some(steps) = steps {
        steps.push(edits);
    }
    out
}

// Mermaid rewrites double-quoted HTML attributes to single quotes so they survive quoted labels.
fn requote_tags(text: &str) -> String {
    let Some(last_close) = text.rfind('>').filter(|&at| at > 0 && text.contains('<')) else {
        return text.to_string();
    };
    let s: Vec<char> = text[..=last_close].chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == '<' && i + 1 < s.len() && is_word(s[i + 1]) {
            let mut j = i + 1;
            while j < s.len() && is_word(s[j]) {
                j += 1;
            }
            let attrs_start = j;
            while j < s.len() && s[j] != '>' {
                j += 1;
            }
            if j < s.len() {
                out.extend(&s[i..attrs_start]);
                let attrs = &s[attrs_start..j];
                let mut k = 0;
                while k < attrs.len() {
                    if attrs[k] == '=' && attrs.get(k + 1) == Some(&'"') {
                        if let Some(close) = attrs[k + 2..].iter().position(|&c| c == '"') {
                            out.push_str("='");
                            out.extend(&attrs[k + 2..k + 2 + close]);
                            out.push('\'');
                            k += close + 3;
                            continue;
                        }
                    }
                    out.push(attrs[k]);
                    k += 1;
                }
                out.push('>');
                i = j + 1;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out.push_str(&text[last_close + 1..]);
    out
}

pub fn preprocess(source: &str) -> Result<Preprocessed> {
    preprocess_inner(source, None)
}

fn preprocess_inner(source: &str, mut steps: Option<&mut Vec<Vec<Edit>>>) -> Result<Preprocessed> {
    let mut text = requote_tags(&normalize_line_breaks(source));
    let mut config = Value::object();
    let mut title = None;
    let mut line_offset = 0;

    if text.contains("---") {
        let chars: Vec<char> = text.chars().collect();
        if let Some(front) = match_front_matter(&chars) {
            let body = if front.indent.is_empty() {
                front.body.clone()
            } else {
                front.body.split('\n').map(|line| line.strip_prefix(front.indent.as_str()).unwrap_or(line)).collect::<Vec<_>>().join("\n")
            };
            let parsed = parse_yaml(&body)
                .map_err(|e| PeleError::new(format!("Front matter is not valid YAML. {}", e), ErrorCode::Syntax))?;
            if parsed.is_object() {
                if parsed.get("title").truthy() {
                    title = Some(parsed.get("title").to_text());
                }
                if parsed.get("config").is_object() {
                    config.merge(parsed.get("config"));
                }
                if parsed.get("displayMode").truthy() {
                    if !config.get("gantt").is_object() {
                        config.set("gantt", Value::object());
                    }
                    let mut gantt = config.get("gantt").clone();
                    gantt.set("displayMode", Value::Str(parsed.get("displayMode").to_text()));
                    config.set("gantt", gantt);
                }
            }
            line_offset = chars[..front.length].iter().filter(|&&c| c == '\n').count();
            text = chars[front.length..].iter().collect();
            if let Some(steps) = steps.as_deref_mut() {
                steps.push(vec![(0, front.length, 0)]);
            }
        }
    }

    let text = directives(&text, &mut config, steps);
    Ok(Preprocessed { text: cleanup_comments(&text), title, config, with_comments: text, line_offset })
}

// Does what Mermaid's `text.replace(/keyword.*:\S*#.*;/g, drop the last character)` does:
// on a line where the keyword is followed by a colon, a color-like `#`, and later a semicolon,
// the last semicolon goes.
fn drop_color_semicolons(s: Vec<char>, keyword: &str, steps: Option<&mut Vec<Vec<Edit>>>) -> Vec<char> {
    let mut edits = Vec::new();
    let key: Vec<char> = keyword.chars().collect();
    let find = |from: usize| (from..s.len().saturating_sub(key.len().saturating_sub(1))).find(|&i| starts_with(&s, i, &key));
    let mut from = find(0);
    if from.is_none() {
        if let Some(steps) = steps {
            steps.push(edits);
        }
        return s;
    }
    let mut drop = Vec::new();
    while let Some(start) = from {
        let mut line_end = start;
        while line_end < s.len() && !matches!(s[line_end], '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            line_end += 1;
        }
        let mut after_colon = false;
        let mut hash = None;
        for j in start + key.len()..line_end {
            let c = s[j];
            if c == ':' {
                after_colon = true;
            } else if c == '#' {
                if after_colon {
                    hash = Some(j);
                    break;
                }
            } else if c.is_whitespace() {
                after_colon = false;
            }
        }
        if let Some(hash) = hash {
            if let Some(semi) = (hash + 1..line_end).rev().find(|&j| s[j] == ';') {
                drop.push(semi);
                edits.push((semi, semi + 1, 0));
            }
        }
        from = if line_end >= s.len() { None } else { find(line_end + 1) };
    }
    if let Some(steps) = steps {
        steps.push(edits);
    }
    if drop.is_empty() {
        return s;
    }
    s.iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, &c)| c).collect()
}

pub const ENTITY_OPEN: &str = "ﬂ°";
pub const ENTITY_CLOSE: &str = "¶ß";

// Mermaid hides `#name;` entity codes behind placeholder characters so `#` and `;` survive parsing.
pub fn encode_entities(text: &str) -> String {
    encode_entities_inner(text, None)
}

fn encode_entities_inner(text: &str, mut steps: Option<&mut Vec<Vec<Edit>>>) -> String {
    if !text.contains('#') {
        return text.to_string();
    }
    let s = drop_color_semicolons(drop_color_semicolons(text.chars().collect(), "style", steps.as_deref_mut()), "classDef", steps.as_deref_mut());
    let mut out = String::with_capacity(text.len());
    let mut edits = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == '#' {
            let mut j = i + 1;
            while j < s.len() && is_word(s[j]) {
                j += 1;
            }
            if j > i + 1 && j < s.len() && s[j] == ';' {
                let inner: String = s[i + 1..j].iter().collect();
                let is_int = inner.bytes().all(|b| b.is_ascii_digit())
                    || (inner.starts_with('+') && inner.len() > 1 && inner[1..].bytes().all(|b| b.is_ascii_digit()));
                let code_chars = 2 + usize::from(is_int) + (j - (i + 1)) + 2;
                edits.push((i, j + 1, code_chars));
                out.push_str(ENTITY_OPEN);
                if is_int {
                    out.push('°');
                }
                out.push_str(&inner);
                out.push_str(ENTITY_CLOSE);
                i = j + 1;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    if let Some(steps) = steps {
        steps.push(edits);
    }
    out
}

// Reconstructs preprocessing edit steps so syntax errors can report line/column in the input as written.
pub fn source_edit_steps(source: &str, keep_comments: bool) -> (Vec<char>, Vec<Vec<Edit>>) {
    let mut steps = Vec::new();
    if let Ok(pre) = preprocess_inner(source, Some(&mut steps)) {
        let cleaned = if keep_comments {
            pre.with_comments
        } else {
            cleanup_comments_inner(&pre.with_comments, Some(&mut steps))
        };
        let _ = encode_entities_inner(&cleaned, Some(&mut steps));
    }
    let mut orig: Vec<char> = normalize_line_breaks(source).chars().collect();
    orig.push('\n');
    (orig, steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_and_directive() {
        let pre = preprocess("---\ntitle: My chart\nconfig:\n  flowchart:\n    curve: linear\n---\n%%{init: {'theme': 'dark'}}%%\n%% a comment\nflowchart TD\n  A --> B\n").unwrap();
        assert_eq!(pre.title.as_deref(), Some("My chart"));
        assert_eq!(pre.config.get("theme").as_str(), Some("dark"));
        assert_eq!(pre.config.get("flowchart").get("curve").as_str(), Some("linear"));
        assert_eq!(pre.text, "flowchart TD\n  A --> B\n");
        assert_eq!(pre.line_offset, 6);
    }

    #[test]
    fn entities() {
        assert_eq!(encode_entities("A[\"#35; and #quot;\"]"), "A[\"ﬂ°°35¶ß and ﬂ°quot¶ß\"]");
        assert_eq!(encode_entities("style A fill:#f9f;\n"), "style A fill:#f9f\n");
    }

    #[test]
    fn tag_attributes() {
        assert_eq!(requote_tags("A[<span class=\"x\">hi</span>] > b"), "A[<span class='x'>hi</span>] > b");
    }
}
