use crate::chars::{is_line_end, is_space, is_word};
use crate::errors::{syntax_error, Result};

pub mod t {
    pub const END: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const SPACELINE: u8 = 2;
    pub const NL: u8 = 3;
    pub const EOF: u8 = 4;
    pub const KEYWORD: u8 = 5;
    pub const SPACELIST: u8 = 6;
    pub const ICON: u8 = 7;
    pub const CLASS: u8 = 8;
    pub const NODE_DSTART: u8 = 9;
    pub const NODE_ID: u8 = 10;
    pub const NODE_DESCR: u8 = 11;
    pub const NODE_DEND: u8 = 12;
    pub const SHAPE_DATA: u8 = 13;
}

const NAMES: [&str; 14] = [
    "1",
    "INVALID",
    "SPACELINE",
    "NL",
    "EOF",
    "",
    "SPACELIST",
    "ICON",
    "CLASS",
    "NODE_DSTART",
    "NODE_ID",
    "NODE_DESCR",
    "NODE_DEND",
    "SHAPE_DATA",
];

pub fn token_name(kind: u8, kanban: bool) -> &'static str {
    if kind == t::KEYWORD {
        if kanban { "KANBAN" } else { "MINDMAP" }
    } else {
        NAMES[kind as usize]
    }
}

pub mod node_type {
    pub const DEFAULT: u8 = 0;
    pub const NO_BORDER: u8 = 0;
    pub const ROUNDED_RECT: u8 = 1;
    pub const RECT: u8 = 2;
    pub const CIRCLE: u8 = 3;
    pub const CLOUD: u8 = 4;
    pub const BANG: u8 = 5;
    pub const HEXAGON: u8 = 6;
}

pub fn get_type(start: &str, end: &str) -> u8 {
    match start {
        "[" => node_type::RECT,
        "(" => if end == ")" { node_type::ROUNDED_RECT } else { node_type::CLOUD },
        "((" => node_type::CIRCLE,
        ")" => node_type::CLOUD,
        "))" => node_type::BANG,
        "{{" => node_type::HEXAGON,
        _ => node_type::DEFAULT,
    }
}

pub trait OutlineDb {
    fn get_type(&self, start: &str, end: &str) -> u8 {
        get_type(start, end)
    }
    fn add_node(&mut self, level: isize, id: &str, descr: &str, node_type: u8, shape_data: Option<&str>) -> Result<()>;
    fn decorate_node(&mut self, icon: Option<&str>, class: Option<&str>);
}

pub struct Tokens {
    pub types: Vec<u8>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

const S_INITIAL: u8 = 0;
const S_CLASS: u8 = 1;
const S_ICON: u8 = 2;
const S_NODE: u8 = 3;
const S_NSTR: u8 = 4;
const S_NSTR2: u8 = 5;
const S_DATA: u8 = 6;
const S_DATA_STR: u8 = 7;

// Jison's case-insensitive flag folds ASCII letters only.
fn letters(src: &[char], p: usize, word: &str) -> bool {
    for (k, w) in word.bytes().enumerate() {
        let Some(&c) = src.get(p + k) else {
            return false;
        };
        if ((c as u32) | 32) != w as u32 {
            return false;
        }
    }
    true
}

fn replace_br(s: &[char]) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == '\n' {
            out.push_str("<br/>");
            i += 1;
            while i < s.len() && is_space(s[i]) {
                i += 1;
            }
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

pub fn tokenize_outline(src: &[char], kanban: bool) -> Tokens {
    let n = src.len();
    let mut types = Vec::new();
    let mut texts = Vec::new();
    let mut starts = Vec::new();
    let keyword = if kanban { "kanban" } else { "mindmap" };
    let mut state = S_INITIAL;
    let mut p = 0usize;
    let mut next_break: isize = -1;

    let match_line = |from: usize| -> Option<usize> {
        let mut i = from;
        while i < n && !is_line_end(src[i]) {
            i += 1;
        }
        (i > from).then_some(i)
    };

    let up_to = |from: usize, ch: char| -> usize {
        let mut i = from;
        while i < n && src[i] != ch {
            i += 1;
        }
        i
    };

    while p < n {
        let c = src[p] as u32;
        let d = src.get(p + 1).map_or(u32::MAX, |&ch| ch as u32);
        if state == S_INITIAL {
            let mut w = p;
            if c <= 32 || c >= 160 {
                while w < n && is_space(src[w]) {
                    w += 1;
                }
            }
            if kanban && c == 64 && d == 123 {
                types.push(t::SHAPE_DATA);
                texts.push(String::new());
                starts.push(p);
                p += 2;
                state = S_DATA;
            } else if src.get(w) == Some(&'%') && src.get(w + 1) == Some(&'%') {
                let end = match_line(w).unwrap_or(w);
                types.push(t::SPACELINE);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_INITIAL;
            } else if w > p {
                if next_break < p as isize {
                    next_break = up_to(p, '\n') as isize;
                }
                if next_break >= w as isize {
                    types.push(t::SPACELIST);
                    texts.push(src[p..w].iter().collect());
                    starts.push(p);
                    p = w;
                    state = S_INITIAL;
                } else if let Some(last_nl) = (p + 1..w).rev().find(|&idx| src[idx] == '\n') {
                    let end = last_nl + 1;
                    types.push(t::SPACELINE);
                    texts.push(src[p..end].iter().collect());
                    starts.push(p);
                    p = end;
                    state = S_INITIAL;
                } else {
                    types.push(t::NL);
                    texts.push(src[p..p + 1].iter().collect());
                    starts.push(p);
                    p += 1;
                    state = S_INITIAL;
                }
            } else if letters(src, p, keyword) && !src.get(p + keyword.len()).is_some_and(|&ch| is_word(ch)) {
                let end = p + keyword.len();
                types.push(t::KEYWORD);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_INITIAL;
            } else if c == 58 && d == 58 && src.get(p + 2) == Some(&':') {
                p += 3;
                state = S_CLASS;
            } else if c == 58 && d == 58 && letters(src, p + 2, "icon") && src.get(p + 6) == Some(&'(') {
                p += 7;
                state = S_ICON;
            } else if (c == 45 && d == 41) || (c == 40 && (d == 45 || d == 40)) || ((c == 41 || c == 123) && d == c) {
                let end = p + 2;
                types.push(t::NODE_DSTART);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_NODE;
            } else if c == 41 || c == 40 || c == 91 {
                let end = p + 1;
                types.push(t::NODE_DSTART);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_NODE;
            } else {
                let mut e = p;
                while e < n {
                    let ch = src[e];
                    if matches!(ch, '(' | '[' | '\n' | ')' | '{' | '}') || (kanban && ch == '@') {
                        break;
                    }
                    e += 1;
                }
                if e > p {
                    types.push(t::NODE_ID);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                    state = S_INITIAL;
                } else {
                    break;
                }
            }
        } else if state == S_NODE {
            if c == 34 {
                state = if d == 96 { S_NSTR2 } else { S_NSTR };
                p += if d == 96 { 2 } else { 1 };
            } else if c == 41 || c == 40 {
                let end = if d == c || (c == 40 && d == 45) { p + 2 } else { p + 1 };
                types.push(t::NODE_DEND);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_INITIAL;
            } else if c == 93 {
                let end = p + 1;
                types.push(t::NODE_DEND);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_INITIAL;
            } else if (c == 125 && d == 125) || (c == 45 && d == 41) {
                let end = p + 2;
                types.push(t::NODE_DEND);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_INITIAL;
            } else {
                // A lone closing brace takes the rest of its line as text.
                let e = if c == 125 {
                    match_line(p)
                } else {
                    let mut idx = p;
                    while idx < n && !matches!(src[idx], ')' | ']' | '(' | '}') {
                        idx += 1;
                    }
                    (idx > p).then_some(idx)
                };
                if let Some(end) = e {
                    types.push(t::NODE_DESCR);
                    texts.push(src[p..end].iter().collect());
                    starts.push(p);
                    p = end;
                    state = S_NODE;
                } else {
                    break;
                }
            }
        } else if state == S_NSTR {
            if c == 34 {
                p += 1;
                state = S_NODE;
            } else {
                let end = up_to(p, '"');
                types.push(t::NODE_DESCR);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_NSTR;
            }
        } else if state == S_NSTR2 {
            if c == 96 && d == 34 {
                p += 2;
                state = S_NODE;
            } else {
                let mut end = p;
                while end < n && src[end] != '`' && src[end] != '"' {
                    end += 1;
                }
                if end > p {
                    types.push(t::NODE_DESCR);
                    texts.push(src[p..end].iter().collect());
                    starts.push(p);
                    p = end;
                    state = S_NSTR2;
                } else {
                    break;
                }
            }
        } else if state == S_CLASS {
            if let Some(end) = match_line(p) {
                types.push(t::CLASS);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_INITIAL;
            } else if c == 10 {
                p += 1;
                state = S_INITIAL;
            } else {
                break;
            }
        } else if state == S_ICON {
            if c == 41 {
                p += 1;
                state = S_INITIAL;
            } else {
                let end = up_to(p, ')');
                types.push(t::ICON);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_ICON;
            }
        } else if state == S_DATA {
            if c == 34 {
                let end = p + 1;
                types.push(t::SHAPE_DATA);
                texts.push(src[p..end].iter().collect());
                starts.push(p);
                p = end;
                state = S_DATA_STR;
            } else if c == 125 {
                p += 1;
                state = S_INITIAL;
            } else {
                let mut end = p;
                while end < n && !matches!(src[end], '}' | '^' | '"') {
                    end += 1;
                }
                if end > p {
                    types.push(t::SHAPE_DATA);
                    texts.push(src[p..end].iter().collect());
                    starts.push(p);
                    p = end;
                    state = S_DATA;
                } else {
                    break;
                }
            }
        } else if state == S_DATA_STR && c == 34 {
            let end = p + 1;
            types.push(t::SHAPE_DATA);
            texts.push(src[p..end].iter().collect());
            starts.push(p);
            p = end;
            state = S_DATA;
        } else {
            let end = up_to(p, '"');
            types.push(t::SHAPE_DATA);
            texts.push(replace_br(&src[p..end]));
            starts.push(p);
            p = end;
        }
    }

    if p < n {
        types.push(t::ERROR);
        texts.push(String::new());
        starts.push(p);
    } else {
        if state == S_INITIAL {
            types.push(t::EOF);
            texts.push(String::new());
            starts.push(n);
        }
        types.push(t::END);
        texts.push(String::new());
        starts.push(n);
    }
    Tokens { types, texts, starts }
}

const STOP: &str = "'SPACELINE', 'NL', 'EOF'";

fn is_stop(kind: u8) -> bool {
    kind == t::SPACELINE || kind == t::NL || kind == t::EOF
}

fn is_start(kind: u8) -> bool {
    matches!(kind, t::SPACELINE | t::SPACELIST | t::ICON | t::CLASS | t::NODE_DSTART | t::NODE_ID)
}

pub fn parse_outline(src_str: &str, db: &mut dyn OutlineDb, kanban: bool) -> Result<()> {
    let diagram = if kanban { "kanban" } else { "mindmap" };
    let src: Vec<char> = src_str.chars().collect();
    let Tokens { types, texts, starts } = tokenize_outline(&src, kanban);
    let data = if kanban { ", 'SHAPE_DATA'" } else { "" };
    let mut i = 0usize;

    let fail = |idx: usize, expected: &str| -> crate::errors::PeleError {
        let k = types[idx];
        if k == t::ERROR {
            syntax_error(diagram, &src, starts[idx], "", true)
        } else {
            syntax_error(diagram, &src, starts[idx], &format!("Expecting {}, got '{}'", expected, token_name(k, kanban)), false)
        }
    };

    let keyword = format!("'{}'", token_name(t::KEYWORD, kanban));
    if types[i] == t::SPACELINE {
        i += 1;
        while types[i] == t::SPACELINE || types[i] == t::NL {
            i += 1;
        }
        if types[i] != t::KEYWORD {
            return Err(fail(i, &format!("'SPACELINE', 'NL', {}", keyword)));
        }
    } else if types[i] != t::KEYWORD {
        return Err(fail(i, &format!("'SPACELINE', {}", keyword)));
    }
    i += 1;
    let mut first = "'SPACELINE', 'NL', 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'";
    if types[i] == t::NL {
        i += 1;
        first = "'SPACELINE', 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'";
    }

    loop {
        let mut k = types[i];
        let mut level: isize = 0;
        let indented = k == t::SPACELIST;
        if indented {
            level = texts[i].encode_utf16().count() as isize;
            i += 1;
            k = types[i];
        }
        if k == t::NODE_ID || k == t::NODE_DSTART {
            let mut id: Option<&str> = None;
            let descr: &str;
            let mut ntype = 0u8;
            if k == t::NODE_ID {
                id = Some(&texts[i]);
                i += 1;
            }
            if types[i] == t::NODE_DSTART {
                let start = &texts[i];
                i += 1;
                if types[i] != t::NODE_DESCR {
                    return Err(fail(i, "'NODE_DESCR'"));
                }
                descr = &texts[i];
                i += 1;
                if types[i] != t::NODE_DEND {
                    return Err(fail(i, "'NODE_DEND'"));
                }
                let end = &texts[i];
                i += 1;
                if !is_stop(types[i]) && types[i] != t::SHAPE_DATA {
                    return Err(fail(i, &format!("{}{}", STOP, data)));
                }
                ntype = db.get_type(start, end);
            } else {
                if !is_stop(types[i]) && types[i] != t::SHAPE_DATA {
                    return Err(fail(i, &format!("{}, 'NODE_DSTART'{}", STOP, data)));
                }
                descr = id.unwrap();
            }
            if types[i] == t::SHAPE_DATA {
                let mut shape_data = texts[i].clone();
                i += 1;
                while types[i] == t::SHAPE_DATA {
                    shape_data.push_str(&texts[i]);
                    i += 1;
                }
                if !is_stop(types[i]) {
                    return Err(fail(i, &format!("{}{}", STOP, data)));
                }
                db.add_node(level, id.unwrap_or(descr), descr, ntype, Some(&shape_data))?;
            } else {
                db.add_node(level, id.unwrap_or(descr), descr, ntype, None)?;
            }
        } else if k == t::ICON || k == t::CLASS {
            let text = &texts[i];
            i += 1;
            if !is_stop(types[i]) {
                return Err(fail(i, STOP));
            }
            if k == t::ICON {
                db.decorate_node(Some(text), None);
            } else {
                db.decorate_node(None, Some(text));
            }
        } else if indented {
            if !is_stop(k) {
                return Err(fail(i, &format!("{}, 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'", STOP)));
            }
        } else if k == t::SPACELINE {
            i += 1;
            if !is_stop(types[i]) {
                return Err(fail(i, STOP));
            }
        } else {
            return Err(fail(i, first));
        }

        i += 1;
        while types[i] == t::NL || types[i] == t::EOF {
            i += 1;
        }
        if types[i] == t::END {
            return Ok(());
        }
        if !is_start(types[i]) {
            return Err(fail(i, &format!("{}, 'SPACELIST', 'ICON', 'CLASS', 'NODE_DSTART', 'NODE_ID'", STOP)));
        }
    }
}
