use super::tokens::t;
use crate::chars::{is_digit, is_letter, is_line_end, is_space, is_word};
use crate::diagrams::common::caseless::word_at;

pub struct Tokens {
    pub types: Vec<usize>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

const S_INITIAL: usize = 0;
const S_TITLE: usize = 1;
const S_ACC_TITLE: usize = 2;
const S_ACC_DESCR: usize = 3;
const S_ACC_DESCR_MULTILINE: usize = 4;
const S_STRING: usize = 5;
const S_MD_STRING: usize = 6;
const S_CLASS_NAME: usize = 7;
const S_POINT_START: usize = 8;
const S_POINT_X: usize = 9;
const S_POINT_Y: usize = 10;

// The token each state gives for the empty rest of the input, where it gives one.
const AT_END: [usize; 6] = [
    t::EOF,
    t::TITLE_VALUE,
    t::ACC_TITLE_VALUE,
    t::ACC_DESCR_VALUE,
    t::ACC_DESCR_MULTILINE_VALUE,
    t::STR,
];

fn single_token(c: char) -> usize {
    match c {
        ':' => t::COLON,
        '+' => t::PLUS,
        ',' => t::COMMA,
        '=' => t::EQUALS,
        '*' => t::MULT,
        '#' => t::BRKT,
        '_' => t::UNDERSCORE,
        '.' => t::DOT,
        '&' => t::AMP,
        '-' => t::MINUS,
        ' ' | '\t' | '\x0b' | '\x0c' => t::SPACE,
        ';' => t::SEMI,
        '!' | '$' | '%' | '\'' | '`' | '?' | '\\' | '/' => t::PUNCTUATION,
        _ => 0,
    }
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types: Vec<usize> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut state: usize = S_INITIAL;
    let mut p: usize = 0;
    let mut space_from: usize = 0;
    let mut space_to: usize = 0;
    let mut dash_from: usize = 0;
    let mut dash_to: usize = 0;
    let mut ws_from: usize = 0;
    let mut ws_to: usize = 0;
    let mut gap_from: usize = 0;
    let mut gap_to: usize = 0;

    let line_end = |q: usize| -> usize {
        let mut i = q;
        while i < n && src[i] != '\n' {
            i += 1;
        }
        i
    };
    let spaces = |mut q: usize| -> usize {
        while q < n && src[q] == ' ' {
            q += 1;
        }
        q
    };
    let run = |mut q: usize| -> usize {
        while q < n && is_space(src[q]) {
            q += 1;
        }
        q
    };
    // Mermaid's `(1)|(0(.\d+)?)`, where the dot is any character within a line.
    let coordinate = |q: usize| -> Option<usize> {
        let c = *src.get(q)?;
        if c == '1' {
            return Some(q + 1);
        }
        if c != '0' {
            return None;
        }
        let mut e = q + 1;
        if e < n && !is_line_end(src[e]) && src.get(e + 1).is_some_and(|&d| is_digit(d)) {
            e += 2;
            while e < n && is_digit(src[e]) {
                e += 1;
            }
        }
        Some(e)
    };

    macro_rules! emit {
        ($kind:expr, $s:expr, $e:expr) => {{
            let s = $s;
            let e = $e;
            types.push($kind);
            texts.push(src[s..e].iter().collect());
            starts.push(s);
            p = e;
        }};
    }
    macro_rules! push {
        ($next_state:expr) => {{
            stack.push(state);
            state = $next_state;
        }};
    }
    macro_rules! pop {
        () => {{
            if let Some(prev) = stack.pop() {
                state = prev;
            }
        }};
    }

    'lexing: while p < n {
        let c = src[p];
        let next = src.get(p + 1).copied();

        match state {
            S_INITIAL => {}

            S_TITLE | S_ACC_TITLE | S_ACC_DESCR => {
                let end = line_end(p);
                emit!(AT_END[state], p, end);
                pop!();
                continue;
            }

            S_ACC_DESCR_MULTILINE => {
                if c == '}' {
                    p += 1;
                    pop!();
                } else {
                    let mut e = p;
                    while e < n && src[e] != '}' {
                        e += 1;
                    }
                    emit!(t::ACC_DESCR_MULTILINE_VALUE, p, e);
                }
                continue;
            }

            S_STRING => {
                if c == '"' {
                    p += 1;
                    pop!();
                } else {
                    let mut e = p;
                    while e < n && src[e] != '"' {
                        e += 1;
                    }
                    emit!(t::STR, p, e);
                }
                continue;
            }

            S_MD_STRING => {
                if c == '`' && next == Some('"') {
                    p += 2;
                    pop!();
                    continue;
                }
                if c != '`' && c != '"' {
                    let mut e = p + 1;
                    while e < n && src[e] != '`' && src[e] != '"' {
                        e += 1;
                    }
                    emit!(t::MD_STR, p, e);
                    continue;
                }
                break 'lexing;
            }

            S_CLASS_NAME => {
                if is_word(c) {
                    let mut e = p + 1;
                    while e < n && is_word(src[e]) {
                        e += 1;
                    }
                    emit!(t::CLASS_NAME, p, e);
                    pop!();
                    continue;
                }
                break 'lexing;
            }

            S_POINT_START => {
                if let Some(e) = coordinate(p) {
                    emit!(t::POINT_X, p, e);
                    push!(S_POINT_X);
                    continue;
                }
                let k = run(p);
                if src.get(k) == Some(&']') {
                    p = spaces(k + 1);
                    pop!();
                    continue;
                }
                break 'lexing;
            }

            S_POINT_X => {
                let k = run(p);
                if src.get(k) == Some(&',') {
                    p = run(k + 1);
                    state = S_POINT_Y;
                    continue;
                }
                break 'lexing;
            }

            S_POINT_Y => {
                if let Some(e) = coordinate(p) {
                    emit!(t::POINT_Y, p, e);
                    pop!();
                    continue;
                }
                break 'lexing;
            }

            _ => unreachable!(),
        }

        if c == '%' && next == Some('%') {
            p = line_end(p + 2);
            continue;
        }
        if c == '\n' || c == '\r' {
            let mut e = p + 1;
            while e < n && (src[e] == '\n' || src[e] == '\r') {
                e += 1;
            }
            emit!(t::NEWLINE, p, e);
            continue;
        }

        let lower = (c as u32) | 32;
        if lower == b't' as u32 && word_at(src, p, "title") && !src.get(p + 5).is_some_and(|&ch| is_word(ch)) {
            emit!(t::TITLE, p, p + 5);
            push!(S_TITLE);
            continue;
        }
        if lower == b'a' as u32 && word_at(src, p, "acc") {
            let descr = word_at(src, p + 3, "descr");
            if descr || word_at(src, p + 3, "title") {
                let k = run(p + 8);
                let d = src.get(k).copied();
                if d == Some(':') {
                    let end = run(k + 1);
                    emit!(if descr { t::ACC_DESCR } else { t::ACC_TITLE }, p, end);
                    push!(if descr { S_ACC_DESCR } else { S_ACC_TITLE });
                    continue;
                }
                if d == Some('{') && descr {
                    p = run(k + 1);
                    push!(S_ACC_DESCR_MULTILINE);
                    continue;
                }
            }
        }

        // Axis, quadrant and chart keywords and the axis arrow take the spaces around them.
        let mut j = p;
        if c == ' ' {
            if p < space_from || p >= space_to {
                space_from = p;
                space_to = spaces(p);
            }
            j = space_to;
        }
        let mut kw_type = 0usize;
        let mut kw_end = 0usize;
        if let Some(&d) = src.get(j) {
            let dl = (d as u32) | 32;
            if dl == b'x' as u32 && word_at(src, j, "x-axis") {
                kw_type = t::X_AXIS;
                kw_end = j + 6;
            } else if dl == b'y' as u32 && word_at(src, j, "y-axis") {
                kw_type = t::Y_AXIS;
                kw_end = j + 6;
            } else if d == '-' {
                if j < dash_from || j >= dash_to {
                    dash_from = j;
                    dash_to = j;
                    while dash_to < n && src[dash_to] == '-' {
                        dash_to += 1;
                    }
                }
                if dash_to - j >= 2 && src.get(dash_to) == Some(&'>') {
                    kw_type = t::DELIMITER;
                    kw_end = dash_to + 1;
                }
            } else if dl == b'q' as u32 && word_at(src, j, "quadrant") {
                let q = src.get(j + 9).copied();
                if src.get(j + 8) == Some(&'-') && matches!(q, Some('1'..='4')) {
                    kw_type = t::QUADRANT_1 + (q.unwrap() as usize - '1' as usize);
                    kw_end = j + 10;
                } else if word_at(src, j + 8, "chart") {
                    kw_type = t::QUADRANT;
                    kw_end = j + 13;
                }
            }
        }
        if kw_type != 0 {
            let end = spaces(kw_end);
            emit!(kw_type, p, end);
            continue;
        }

        if lower == b'c' as u32 && word_at(src, p, "classdef") && !src.get(p + 8).is_some_and(|&ch| is_word(ch)) {
            emit!(t::CLASSDEF, p, p + 8);
            continue;
        }
        if c == '"' {
            if next == Some('`') {
                p += 2;
                push!(S_MD_STRING);
            } else {
                p += 1;
                push!(S_STRING);
            }
            continue;
        }
        if c == ':' && next == Some(':') && src.get(p + 2) == Some(&':') {
            p += 3;
            push!(S_CLASS_NAME);
            continue;
        }
        // A point's `: [`, with any whitespace before, between and after, line breaks included.
        if c == ':' || is_space(c) {
            let mut k = p;
            if c != ':' {
                if p < ws_from || p >= ws_to {
                    ws_from = p;
                    ws_to = run(p);
                }
                k = ws_to;
            }
            if src.get(k) == Some(&':') {
                if k + 1 < gap_from || k + 1 >= gap_to {
                    gap_from = k + 1;
                    gap_to = run(k + 1);
                }
                if src.get(gap_to) == Some(&'[') {
                    let end = run(gap_to + 1);
                    emit!(t::POINT_START, p, end);
                    push!(S_POINT_START);
                    continue;
                }
            }
        }

        if is_letter(c) {
            let mut e = p + 1;
            while e < n && is_letter(src[e]) {
                e += 1;
            }
            emit!(t::ALPHA, p, e);
            continue;
        }
        if (c as u32) >= 128 {
            let mut e = p + 1;
            while e < n && (src[e] as u32) >= 128 {
                e += 1;
            }
            emit!(t::UNICODE_TEXT, p, e);
            continue;
        }
        if is_digit(c) {
            let mut e = p + 1;
            while e < n && is_digit(src[e]) {
                e += 1;
            }
            emit!(t::NUM, p, e);
            continue;
        }
        let s_tok = single_token(c);
        if s_tok != 0 {
            emit!(s_tok, p, p + 1);
            continue;
        }
        break;
    }

    if p < n {
        types.push(t::ERROR);
        texts.push(String::new());
        starts.push(p);
        return Tokens { types, texts, starts };
    }
    if state < AT_END.len() {
        types.push(AT_END[state]);
        texts.push(String::new());
        starts.push(n);
    }
    types.push(t::END);
    texts.push(String::new());
    starts.push(n);
    Tokens { types, texts, starts }
}
