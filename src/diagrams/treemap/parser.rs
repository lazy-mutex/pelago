use crate::chars::{is_space, is_word};
use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, slice_starts_with, title_before, title_value, tokenize,
    Pattern, Reader, TokenType, ACC_DESCR, ACC_TITLE, WHITESPACE,
};
use crate::errors::Result;

mod t {
    pub const ACC_DESCR: i32 = 0;
    pub const ACC_TITLE: i32 = 1;
    pub const TITLE: i32 = 2;
    pub const KEYWORD: i32 = 3;
    pub const CLASS_DEF: i32 = 4;
    pub const STYLE_SEPARATOR: i32 = 5;
    pub const SEPARATOR: i32 = 6;
    pub const COMMA: i32 = 7;
    pub const INDENTATION: i32 = 8;
    pub const ID: i32 = 12;
    pub const NUMBER: i32 = 13;
    pub const STRING: i32 = 14;
}

fn has_word_after(src: &[char], at: usize) -> bool {
    src.get(at).is_some_and(|&c| is_word(c))
}

fn match_keyword(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if slice_starts_with(src, at, "treemap-beta") {
        Some(at + 12)
    } else if slice_starts_with(src, at, "treemap") {
        Some(at + 7)
    } else {
        None
    }
}

// /classDef\s+([a-zA-Z_][a-zA-Z0-9_]+)(?:\s+([^;\r\n]*))?(?:;)?/y
fn match_class_def(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !slice_starts_with(src, at, "classDef") {
        return None;
    }
    let n = src.len();
    let mut i = at + 8;
    let ws_start = i;
    while i < n && is_space(src[i]) {
        i += 1;
    }
    if i == ws_start {
        return None;
    }
    if !src.get(i).is_some_and(|&c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let name_start = i;
    i += 1;
    while i < n && (src[i].is_ascii_alphanumeric() || src[i] == '_') {
        i += 1;
    }
    if i - name_start < 2 {
        return None;
    }
    let ws2_start = i;
    while i < n && is_space(src[i]) {
        i += 1;
    }
    if i > ws2_start {
        while i < n && !matches!(src[i], ';' | '\r' | '\n') {
            i += 1;
        }
    }
    if src.get(i) == Some(&';') {
        i += 1;
    }
    Some(i)
}

fn match_indentation(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let mut i = at;
    while i < src.len() && (src[i] == ' ' || src[i] == '\t') {
        i += 1;
    }
    (i > at).then_some(i)
}

fn match_ml_comment(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !slice_starts_with(src, at, "%%") {
        return None;
    }
    let mut i = at + 2;
    while i < src.len() && src[i] != '\n' {
        i += 1;
    }
    Some(i)
}

fn match_id2(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !src.get(at).is_some_and(|&c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let mut i = at + 1;
    while i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == '_') {
        i += 1;
    }
    Some(i)
}

fn match_number2(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let mut i = at;
    while i < src.len() && (src[i].is_ascii_digit() || matches!(src[i], '_' | '.' | ',')) {
        i += 1;
    }
    (i > at).then_some(i)
}

// /"[^"]*"|'[^']*'/y
fn match_string2(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let q = *src.get(at)?;
    if q != '"' && q != '\'' {
        return None;
    }
    let mut i = at + 1;
    while i < src.len() {
        if src[i] == q {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

pub const TREEMAP_TOKENS: &[TokenType] = &[
    ACC_DESCR,
    ACC_TITLE,
    title_before(has_word_after),
    TokenType::vis("TREEMAP_KEYWORD", Pattern::Custom(match_keyword)),
    TokenType::vis("CLASS_DEF", Pattern::Custom(match_class_def)),
    TokenType::vis("STYLE_SEPARATOR", Pattern::Literal(":::")),
    TokenType::vis("SEPARATOR", Pattern::Literal(":")),
    TokenType::vis("COMMA", Pattern::Literal(",")),
    TokenType::vis("INDENTATION", Pattern::Custom(match_indentation)),
    WHITESPACE,
    TokenType::hid("ML_COMMENT", Pattern::Custom(match_ml_comment)),
    TokenType::hid("NL", Pattern::Newline),
    TokenType::vis("ID2", Pattern::Custom(match_id2)),
    TokenType::vis("NUMBER2", Pattern::Custom(match_number2)),
    TokenType::vis("STRING2", Pattern::Custom(match_string2)),
];

#[derive(Debug, Clone)]
pub enum TreemapItemAst {
    Section {
        name: String,
        class_selector: Option<String>,
    },
    Leaf {
        name: String,
        value: f64,
        class_selector: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum TreemapRowKind {
    Row {
        indent: Option<usize>,
        item: TreemapItemAst,
    },
    ClassDef {
        indent: Option<usize>,
        class_name: String,
        style_text: Option<String>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct TreemapAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub rows: Vec<TreemapRowKind>,
}

fn parse_class_def_text(raw: &str) -> (String, Option<String>) {
    // Matches /classDef\s+([A-Z_a-z]\w+)(?:\s+([^\n\r;]*))?;?/
    let rest = raw.strip_prefix("classDef").unwrap_or(raw).trim_start_matches(is_space);
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0usize;
    while i < chars.len() && is_word(chars[i]) {
        i += 1;
    }
    let class_name: String = chars[..i].iter().collect();
    let ws_start = i;
    while i < chars.len() && is_space(chars[i]) {
        i += 1;
    }
    let style_text = if i > ws_start {
        let s_start = i;
        while i < chars.len() && !matches!(chars[i], '\n' | '\r' | ';') {
            i += 1;
        }
        if i > s_start {
            Some(chars[s_start..i].iter().collect())
        } else {
            None
        }
    } else {
        None
    };
    (class_name, style_text)
}

// JavaScript's parseFloat on a string after removing commas (may contain digits, '_', '.').
fn js_parse_float(s: &str) -> f64 {
    let trimmed = s.trim_start_matches(is_space);
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    if trimmed[i..].starts_with("Infinity") {
        return if bytes[0] == b'-' { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - digits_start;
    let mut frac_digits = 0usize;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let f_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - f_start;
    }
    if int_digits == 0 && frac_digits == 0 {
        return f64::NAN;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    trimmed[..i].parse::<f64>().unwrap_or(f64::NAN)
}

pub fn parse_treemap(src: &str) -> Result<TreemapAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, TREEMAP_TOKENS, "treemap")?;
    let mut r = Reader::new(&chars, tokens, TREEMAP_TOKENS, "treemap");
    let mut ast = TreemapAst::default();

    r.expect(t::KEYWORD)?;
    while r.kind() != -1 {
        let kind = r.kind();
        if kind == t::ACC_DESCR {
            ast.acc_descr = Some(acc_descr_value(&r.take()));
            continue;
        }
        if kind == t::ACC_TITLE {
            ast.acc_title = Some(acc_title_value(&r.take()));
            continue;
        }
        if kind == t::TITLE {
            ast.title = Some(title_value(&r.take()));
            continue;
        }
        if kind != t::INDENTATION && kind != t::STRING && kind != t::CLASS_DEF {
            return Err(r.fail("a node, a class definition, or a title"));
        }

        let indent = if kind == t::INDENTATION {
            Some(r.take().encode_utf16().count())
        } else {
            None
        };
        if r.kind() == t::CLASS_DEF {
            let text = r.take();
            let (class_name, style_text) = parse_class_def_text(&text);
            ast.rows.push(TreemapRowKind::ClassDef { indent, class_name, style_text });
            continue;
        }

        let raw_str = r.expect(t::STRING)?;
        let str_chars: Vec<char> = raw_str.chars().collect();
        let name: String = if str_chars.len() >= 2 {
            str_chars[1..str_chars.len() - 1].iter().collect()
        } else {
            String::new()
        };

        let after = if r.kind_at(r.i) == t::INDENTATION { r.i + 1 } else { r.i };
        let after_kind = r.kind_at(after);
        let item = if after_kind == t::SEPARATOR || after_kind == t::COMMA {
            r.i = after + 1;
            r.accept(t::INDENTATION);
            let num_text = r.expect(t::NUMBER)?.replace(',', "");
            let value = js_parse_float(&num_text);
            let class_selector = if r.accept(t::STYLE_SEPARATOR) {
                Some(r.expect(t::ID)?)
            } else {
                None
            };
            TreemapItemAst::Leaf { name, value, class_selector }
        } else {
            let class_selector = if r.accept(t::STYLE_SEPARATOR) {
                Some(r.expect(t::ID)?)
            } else {
                None
            };
            TreemapItemAst::Section { name, class_selector }
        };

        ast.rows.push(TreemapRowKind::Row { indent, item });
    }
    Ok(ast)
}
