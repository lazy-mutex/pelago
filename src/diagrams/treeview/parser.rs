use crate::chars::{is_blank, is_space, is_word};
use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, slice_starts_with, title_before, title_value, tokenize,
    Pattern, Reader, TokenType, ACC_DESCR, ACC_TITLE, WHITESPACE,
};
use crate::errors::Result;

mod t {
    pub const KEYWORD: i32 = 0;
    pub const ACC_DESCR: i32 = 1;
    pub const ACC_TITLE: i32 = 2;
    pub const TITLE: i32 = 3;
    pub const CLASS_ANNOTATION: i32 = 4;
    pub const ICON_ANNOTATION: i32 = 5;
    pub const DESC_ANNOTATION: i32 = 6;
    pub const INDENTATION: i32 = 7;
    pub const QUOTED_NAME: i32 = 8;
    pub const BARE_NAME: i32 = 12;
}

const LONGER_BARE: &[usize] = &[t::BARE_NAME as usize];

fn annotation_at(src: &[char], at: usize) -> bool {
    if slice_starts_with(src, at, "##") || slice_starts_with(src, at, "icon(") {
        return true;
    }
    if !slice_starts_with(src, at, ":::") {
        return false;
    }
    let mut i = at + 3;
    while i < src.len() && is_blank(src[i]) {
        i += 1;
    }
    src.get(i).is_some_and(|&c| c.is_ascii_alphabetic() || c == '_')
}

fn match_bare_name(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let first = *src.get(at)?;
    if is_blank(first) || matches!(first, '\n' | '\r' | '"' | '\'') {
        return None;
    }
    if slice_starts_with(src, at, ":::") || slice_starts_with(src, at, "icon(") || slice_starts_with(src, at, "##") {
        return None;
    }
    let n = src.len();
    let mut i = at + 1;
    while i < n {
        let c = src[i];
        if c == '\n' || c == '\r' {
            break;
        }
        if is_blank(c) {
            let mut j = i + 1;
            while j < n && is_blank(src[j]) {
                j += 1;
            }
            if annotation_at(src, j) {
                break;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    Some(i)
}

// /(?!%%)[^\n\r]/y
fn title_rest_matches(src: &[char], at: usize) -> bool {
    let Some(&c) = src.get(at) else {
        return false;
    };
    if c == '\n' || c == '\r' {
        return false;
    }
    if c == '%' && src.get(at + 1) == Some(&'%') {
        return false;
    }
    true
}

// /[ \t]+:::[ \t]*[A-Za-z_][\w-]*/y
fn match_class_annotation(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let n = src.len();
    let mut i = at;
    while i < n && is_blank(src[i]) {
        i += 1;
    }
    if i == at || !slice_starts_with(src, i, ":::") {
        return None;
    }
    i += 3;
    while i < n && is_blank(src[i]) {
        i += 1;
    }
    if !src.get(i).is_some_and(|&c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    i += 1;
    while i < n && (is_word(src[i]) || src[i] == '-') {
        i += 1;
    }
    Some(i)
}

// /[ \t]+icon\([\w-]*(?::[\w-]+)?\)/y
fn match_icon_annotation(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let n = src.len();
    let mut i = at;
    while i < n && is_blank(src[i]) {
        i += 1;
    }
    if i == at || !slice_starts_with(src, i, "icon(") {
        return None;
    }
    i += 5;
    while i < n && (is_word(src[i]) || src[i] == '-') {
        i += 1;
    }
    if src.get(i) == Some(&':') {
        i += 1;
        let sub_start = i;
        while i < n && (is_word(src[i]) || src[i] == '-') {
            i += 1;
        }
        if i == sub_start {
            return None;
        }
    }
    (src.get(i) == Some(&')')).then_some(i + 1)
}

// /[ \t]+##[^\n\r]*/y
fn match_desc_annotation(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let n = src.len();
    let mut i = at;
    while i < n && is_blank(src[i]) {
        i += 1;
    }
    if i == at || !slice_starts_with(src, i, "##") {
        return None;
    }
    i += 2;
    while i < n && src[i] != '\n' && src[i] != '\r' {
        i += 1;
    }
    Some(i)
}

fn match_indentation(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let mut i = at;
    while i < src.len() && is_blank(src[i]) {
        i += 1;
    }
    (i > at).then_some(i)
}

fn match_quoted_name(src: &[char], at: usize, _prev: i32) -> Option<usize> {
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

pub const TREEVIEW_TOKENS: &[TokenType] = &[
    TokenType::vis_longer("treeView-beta", Pattern::Keyword("treeView-beta"), LONGER_BARE),
    ACC_DESCR,
    ACC_TITLE,
    title_before(title_rest_matches),
    TokenType::vis("CLASS_ANNOTATION", Pattern::Custom(match_class_annotation)),
    TokenType::vis("ICON_ANNOTATION", Pattern::Custom(match_icon_annotation)),
    TokenType::vis("DESC_ANNOTATION", Pattern::Custom(match_desc_annotation)),
    TokenType::vis("INDENTATION", Pattern::Custom(match_indentation)),
    TokenType::vis("QUOTED_NAME", Pattern::Custom(match_quoted_name)),
    WHITESPACE,
    TokenType::hid("ML_COMMENT", Pattern::Custom(match_ml_comment)),
    TokenType::hid("NL", Pattern::Newline),
    TokenType::vis("BARE_NAME", Pattern::Custom(match_bare_name)),
];

#[derive(Debug, Clone)]
pub struct TreeNodeAst {
    pub indent: Option<usize>,
    pub name: String,
    pub class_annotation: Option<String>,
    pub icon_annotation: Option<String>,
    pub desc_annotation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TreeViewAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub nodes: Vec<TreeNodeAst>,
}

fn trim_blanks_end(text: &str) -> String {
    text.trim_end_matches(is_blank).to_string()
}

pub fn parse_tree_view(src: &str) -> Result<TreeViewAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, TREEVIEW_TOKENS, "treeView")?;
    let mut r = Reader::new(&chars, tokens, TREEVIEW_TOKENS, "treeView");
    let mut ast = TreeViewAst::default();

    r.expect(t::KEYWORD)?;
    loop {
        if r.kind() == t::ACC_DESCR {
            ast.acc_descr = Some(acc_descr_value(&r.take()));
        } else if r.kind() == t::ACC_TITLE {
            ast.acc_title = Some(acc_title_value(&r.take()));
        } else if r.kind() == t::TITLE {
            ast.title = Some(title_value(&r.take()));
        } else {
            break;
        }
    }

    while r.kind() != -1 {
        let indent = if r.kind() == t::INDENTATION {
            Some(r.take().encode_utf16().count())
        } else {
            None
        };
        let name = if r.kind() == t::QUOTED_NAME {
            let text = r.take();
            let q_chars: Vec<char> = text.chars().collect();
            if q_chars.len() >= 2 {
                q_chars[1..q_chars.len() - 1].iter().collect()
            } else {
                String::new()
            }
        } else if r.kind() == t::BARE_NAME {
            trim_blanks_end(&r.take())
        } else {
            return Err(r.fail("a name"));
        };

        let mut class_annotation = None;
        let mut icon_annotation = None;
        let mut desc_annotation = None;
        loop {
            if r.kind() == t::CLASS_ANNOTATION {
                let raw = r.take();
                let trimmed = raw.trim_matches(is_space);
                class_annotation = Some(trimmed[3..].trim_matches(is_space).to_string());
            } else if r.kind() == t::ICON_ANNOTATION {
                let raw = r.take();
                let trimmed = raw.trim_matches(is_space);
                icon_annotation = Some(trimmed[5..trimmed.len() - 1].to_string());
            } else if r.kind() == t::DESC_ANNOTATION {
                let raw = r.take();
                let trimmed = raw.trim_matches(is_space);
                desc_annotation = Some(trimmed[2..].trim_matches(is_space).to_string());
            } else {
                break;
            }
        }

        ast.nodes.push(TreeNodeAst {
            indent,
            name,
            class_annotation,
            icon_annotation,
            desc_annotation,
        });
    }
    Ok(ast)
}
