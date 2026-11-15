use std::collections::HashMap;

use crate::chars::is_space;
use crate::errors::{locate, ErrorCode, PeleError, Result};
use crate::svg::builder::js_round;

const INDENT_UNIT: &str = "    ";

fn is_all_box_char(c: char) -> bool {
    matches!(c, '─' | '━' | '│' | '┃' | '└' | '┗' | '├' | '┣')
}

fn is_branch_char(c: char) -> bool {
    matches!(c, '└' | '┗' | '├' | '┣')
}

fn is_dash_char(c: char) -> bool {
    matches!(c, '─' | '━')
}

fn is_decoration_only(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| is_space(c) || c == '│' || c == '┃')
}

fn is_box_only(line: &str) -> bool {
    !line.is_empty() && line.chars().all(|c| is_space(c) || is_all_box_char(c))
}

fn is_comment_line(line: &str) -> bool {
    line.trim_start_matches(is_space).starts_with("%%")
}

// /^\s*(title[\t ]|accTitle[\t ]*:|accDescr[\t ]*[:{])/
fn is_metadata_line(line: &str) -> bool {
    let t = line.trim_start_matches(is_space);
    if t.starts_with("title ") || t.starts_with("title\t") {
        return true;
    }
    if let Some(rest) = t.strip_prefix("accTitle") {
        if rest.trim_start_matches([' ', '\t']).starts_with(':') {
            return true;
        }
    }
    if let Some(rest) = t.strip_prefix("accDescr") {
        if rest.trim_start_matches([' ', '\t']).starts_with([':', '{']) {
            return true;
        }
    }
    false
}

pub struct PreprocessResult {
    pub text: String,
    pub line_map: HashMap<usize, usize>,
    pub columns: HashMap<usize, Box<dyn Fn(usize) -> usize>>,
}

pub fn is_box_drawing_format<S: AsRef<str>>(lines: &[S]) -> bool {
    lines.iter().any(|line| line.as_ref().chars().any(is_all_box_char))
}

fn infer_segment_width(content_lines: &[String]) -> usize {
    for line in content_lines {
        if let Some(idx) = line.chars().position(is_branch_char) {
            if idx > 0 {
                return idx;
            }
        }
    }
    4
}

// Replaces /\bline\s+(\d+)\b/gi using `line_map`.
pub fn remap_error_lines(message: &str, line_map: &HashMap<usize, usize>) -> String {
    let chars: Vec<char> = message.chars().collect();
    let n = chars.len();
    let is_w = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::with_capacity(message.len());
    let mut i = 0usize;
    while i < n {
        let word_boundary_before = i == 0 || !is_w(chars[i - 1]);
        if word_boundary_before
            && i + 4 < n
            && chars[i].eq_ignore_ascii_case(&'l')
            && chars[i + 1].eq_ignore_ascii_case(&'i')
            && chars[i + 2].eq_ignore_ascii_case(&'n')
            && chars[i + 3].eq_ignore_ascii_case(&'e')
            && is_space(chars[i + 4])
        {
            let mut j = i + 4;
            while j < n && is_space(chars[j]) {
                j += 1;
            }
            let d_start = j;
            while j < n && chars[j].is_ascii_digit() {
                j += 1;
            }
            let word_boundary_after = j == n || !is_w(chars[j]);
            if j > d_start && word_boundary_after {
                let digits: String = chars[d_start..j].iter().collect();
                if let Ok(parsed) = digits.parse::<usize>() {
                    if let Some(&orig) = line_map.get(&parsed) {
                        out.push_str(&format!("line {}", orig));
                        i = j;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

pub fn line_start(chars: &[char], line: usize) -> usize {
    let mut at = 0usize;
    for _ in 1..line {
        while at < chars.len() && chars[at] != '\n' {
            at += 1;
        }
        if at < chars.len() {
            at += 1;
        }
    }
    at
}

fn fail<T>(input_chars: &[char], index: usize, message: &str) -> Result<T> {
    let loc = locate(input_chars, line_start(input_chars, index + 1));
    let mut err = PeleError::new(format!("Line {}: {}", loc.line, message), ErrorCode::Syntax).of("treeView");
    err.line = loc.line;
    Err(err)
}

fn before_tabs(line_chars: &[char], at: usize) -> usize {
    let mut i = 0usize;
    let mut n = 0usize;
    while i < line_chars.len() && n < at {
        n += if line_chars[i] == '\t' { 4 } else { 1 };
        i += 1;
    }
    i
}

pub fn preprocess_box_drawing(input: &str) -> Result<PreprocessResult> {
    let mut line_map: HashMap<usize, usize> = HashMap::new();
    let mut columns: HashMap<usize, Box<dyn Fn(usize) -> usize>> = HashMap::new();
    if !input.chars().any(is_all_box_char) {
        return Ok(PreprocessResult { text: input.to_string(), line_map, columns });
    }
    let lines: Vec<&str> = input.split('\n').collect();

    let Some(keyword_idx) = lines.iter().position(|line| line.trim_matches(is_space) == "treeView-beta") else {
        return Ok(PreprocessResult { text: input.to_string(), line_map, columns });
    };

    let passes = |line: &str| -> bool {
        line.trim_matches(is_space).is_empty() || is_comment_line(line) || is_metadata_line(line)
    };

    let mut content_lines: Vec<String> = Vec::new();
    for &line in &lines[keyword_idx + 1..] {
        if passes(line) || is_decoration_only(line) {
            continue;
        }
        content_lines.push(line.replace('\t', "    "));
    }
    if !is_box_drawing_format(&content_lines) {
        return Ok(PreprocessResult { text: input.to_string(), line_map, columns });
    }

    let input_chars: Vec<char> = input.chars().collect();
    let segment_width = infer_segment_width(&content_lines);
    let mut out: Vec<String> = Vec::new();
    let mut keep = |text: String, index: usize, col_fn: Option<Box<dyn Fn(usize) -> usize>>| {
        out.push(text);
        line_map.insert(out.len(), index + 1);
        if let Some(f) = col_fn {
            columns.insert(out.len(), f);
        }
    };

    for (i, &line) in lines.iter().enumerate().take(keyword_idx + 1) {
        keep(line.to_string(), i, None);
    }

    for (i, &line) in lines.iter().enumerate().skip(keyword_idx + 1) {
        if passes(line) {
            keep(line.to_string(), i, None);
            continue;
        }
        if is_decoration_only(line) {
            continue;
        }

        let normalized = line.replace('\t', "    ");
        let norm_chars: Vec<char> = normalized.chars().collect();
        if let Some(branch_idx) = norm_chars.iter().copied().position(is_branch_char) {
            let depth = js_round(branch_idx as f64 / segment_width as f64) as usize + 1;
            let mut pos = branch_idx + 1;
            while pos < norm_chars.len() && is_dash_char(norm_chars[pos]) {
                pos += 1;
            }
            while pos < norm_chars.len() && norm_chars[pos] == ' ' {
                pos += 1;
            }
            let tail: String = norm_chars[pos..].iter().collect();
            let content = tail.trim_end_matches(is_space);
            if content.is_empty() {
                return fail(
                    &input_chars,
                    i,
                    "Empty node — expected a filename or directory name after the box-drawing prefix",
                );
            }
            let indent = INDENT_UNIT.repeat(depth);
            let indent_chars = 4 * depth;
            let orig_chars: Vec<char> = line.chars().collect();
            keep(
                format!("{}{}", indent, content),
                i,
                Some(Box::new(move |at: usize| before_tabs(&orig_chars, pos + at.saturating_sub(indent_chars)))),
            );
        } else if is_box_only(&normalized) {
            continue;
        } else if norm_chars.iter().copied().any(is_all_box_char) {
            keep(line.to_string(), i, None);
        } else if norm_chars.first().is_some_and(|&c| is_space(c)) {
            return fail(
                &input_chars,
                i,
                "Unexpected indentation without box-drawing characters. In box-drawing format, use ├── or └── prefixes for indented nodes.",
            );
        } else {
            keep(line.to_string(), i, None);
        }
    }

    Ok(PreprocessResult {
        text: out.join("\n"),
        line_map,
        columns,
    })
}
