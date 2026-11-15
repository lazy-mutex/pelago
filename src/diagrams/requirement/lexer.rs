use super::tokens::{keyword_token, t};
use crate::chars::{is_line_end, is_space, is_word};

pub struct Tokens {
    pub types: Vec<i32>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    String,
    Style,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

const LONGEST_KEYWORD: usize = 22;

fn is_stop(c: char) -> bool {
    matches!(c, ':' | ',' | '\r' | '\n' | '{' | '<' | '>' | '-' | '=')
}

fn eq_ascii_ci(slice: &[char], lower: &str) -> bool {
    if slice.len() != lower.len() {
        return false;
    }
    for (&ch, b) in slice.iter().zip(lower.bytes()) {
        if !ch.is_ascii() || (ch as u8).to_ascii_lowercase() != b {
            return false;
        }
    }
    true
}

fn match_acc(src: &[char], at: usize, kw: &str, sep: char) -> Option<usize> {
    let n = src.len();
    if at + kw.len() > n || !eq_ascii_ci(&src[at..at + kw.len()], kw) {
        return None;
    }
    let mut i = at + kw.len();
    while i < n && is_space(src[i]) {
        i += 1;
    }
    if src.get(i) != Some(&sep) {
        return None;
    }
    i += 1;
    while i < n && is_space(src[i]) {
        i += 1;
    }
    Some(i)
}

fn line_break(src: &[char], q: usize) -> usize {
    let n = src.len();
    let mut i = q;
    while i < n && src[i] != '\n' {
        i += 1;
    }
    i
}

// Produces the token stream of Mermaid's requirement lexer. Its rules are tried in order and
// the first match wins, so a line that holds a direction statement anywhere is one token.
pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types: Vec<i32> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut stack: Vec<State> = Vec::new();
    let mut state = State::Initial;
    let mut p = 0usize;

    // Where each kind of direction statement starts and where its keyword ends, found in one pass.
    let mut dir_starts: [Vec<usize>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut dir_ends: [Vec<usize>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut dir_seen = [0usize; 4];
    let mut any_dir = false;
    let mut line_end: isize = -1;

    let mut m = 0usize;
    while m + 12 <= n {
        if eq_ascii_ci(&src[m..m + 9], "direction") && is_space(src[m + 9]) {
            let mut q = m + 10;
            while q < n && is_space(src[q]) {
                q += 1;
            }
            if q + 2 <= n {
                let kind = if eq_ascii_ci(&src[q..q + 2], "tb") {
                    Some(0)
                } else if eq_ascii_ci(&src[q..q + 2], "bt") {
                    Some(1)
                } else if eq_ascii_ci(&src[q..q + 2], "rl") {
                    Some(2)
                } else if eq_ascii_ci(&src[q..q + 2], "lr") {
                    Some(3)
                } else {
                    None
                };
                if let Some(k) = kind {
                    dir_starts[k].push(m);
                    dir_ends[k].push(q + 2);
                    any_dir = true;
                    m = q + 2;
                    continue;
                }
            }
        }
        m += 1;
    }

    while p < n {
        let c = src[p];
        let mut ok = true;
        match state {
            State::Initial => {
                ok = false;
                if (c == 't' || c == 'T')
                    && p + 5 < n
                    && eq_ascii_ci(&src[p + 1..p + 5], "itle")
                    && is_space(src[p + 5])
                {
                    let mut q = p + 6;
                    while q < n {
                        let d = src[q];
                        if d == '#' || d == '\n' || d == ';' {
                            break;
                        }
                        q += 1;
                    }
                    if q > p + 6 {
                        types.push(t::TITLE);
                        texts.push(src[p..q].iter().collect());
                        starts.push(p);
                        p = q;
                        ok = true;
                    }
                }
                if !ok
                    && (c == 'a' || c == 'A')
                    && p + 2 < n
                    && (src[p + 1] == 'c' || src[p + 1] == 'C')
                    && (src[p + 2] == 'c' || src[p + 2] == 'C')
                {
                    if let Some(e) = match_acc(src, p, "acctitle", ':') {
                        stack.push(state);
                        state = State::AccTitle;
                        types.push(t::ACC_TITLE);
                        texts.push(src[p..e].iter().collect());
                        starts.push(p);
                        p = e;
                        ok = true;
                    } else if let Some(e) = match_acc(src, p, "accdescr", ':') {
                        stack.push(state);
                        state = State::AccDescr;
                        types.push(t::ACC_DESCR);
                        texts.push(src[p..e].iter().collect());
                        starts.push(p);
                        p = e;
                        ok = true;
                    } else if let Some(e) = match_acc(src, p, "accdescr", '{') {
                        stack.push(state);
                        state = State::AccDescrMultiline;
                        p = e;
                        ok = true;
                    }
                }
                if !ok && any_dir && !is_line_end(c) {
                    if line_end < p as isize {
                        let mut le = p;
                        while le < n && !is_line_end(src[le]) {
                            le += 1;
                        }
                        line_end = le as isize;
                        let bound = le;
                        for k in 0..4 {
                            let list = &dir_starts[k];
                            let mut j = dir_seen[k];
                            while j < list.len() && list[j] <= bound {
                                j += 1;
                            }
                            dir_seen[k] = j;
                        }
                    }
                    for k in 0..4 {
                        if dir_seen[k] > 0 {
                            let j = dir_seen[k] - 1;
                            if dir_starts[k][j] >= p {
                                let e = line_break(src, dir_ends[k][j]);
                                types.push(t::DIRECTION_TB + k as i32);
                                texts.push(src[p..e].iter().collect());
                                starts.push(p);
                                p = e;
                                ok = true;
                                break;
                            }
                        }
                    }
                }
                if !ok {
                    if c == '\n' || (c == '\r' && src.get(p + 1) == Some(&'\n')) {
                        let mut q = p;
                        while q < n {
                            let d = src[q];
                            if d == '\n' {
                                q += 1;
                            } else if d == '\r' && src.get(q + 1) == Some(&'\n') {
                                q += 2;
                            } else {
                                break;
                            }
                        }
                        types.push(t::NEWLINE);
                        texts.push(src[p..q].iter().collect());
                        starts.push(p);
                        p = q;
                        ok = true;
                    } else if is_space(c) {
                        p += 1;
                        while p < n && is_space(src[p]) {
                            p += 1;
                        }
                        ok = true;
                    } else if c == '#' || c == '%' {
                        p = line_break(src, p);
                        ok = true;
                    } else if is_word(c) {
                        let mut q = p + 1;
                        while q < n && is_word(src[q]) {
                            q += 1;
                        }
                        let mut matched_kw = false;
                        if q - p <= LONGEST_KEYWORD {
                            let lower: String = src[p..q].iter().map(|ch| ch.to_ascii_lowercase()).collect();
                            if let Some(kw) = keyword_token(&lower) {
                                if kw == t::STYLE || kw == t::CLASSDEF || kw == t::CLASS {
                                    stack.push(state);
                                    state = State::Style;
                                }
                                types.push(kw);
                                texts.push(src[p..q].iter().collect());
                                starts.push(p);
                                p = q;
                                matched_kw = true;
                            }
                        }
                        if !matched_kw {
                            while q < n && !is_stop(src[q]) {
                                q += 1;
                            }
                            let raw: String = src[p..q].iter().collect();
                            types.push(t::UNQ_STRING);
                            texts.push(raw.trim_matches(is_space).to_string());
                            starts.push(p);
                            p = q;
                        }
                        ok = true;
                    } else {
                        match c {
                            '{' => {
                                types.push(t::STRUCT_START);
                                texts.push("{".to_string());
                                starts.push(p);
                                p += 1;
                                ok = true;
                            }
                            '}' => {
                                types.push(t::STRUCT_STOP);
                                texts.push("}".to_string());
                                starts.push(p);
                                p += 1;
                                ok = true;
                            }
                            ':' => {
                                if src.get(p + 1) == Some(&':') && src.get(p + 2) == Some(&':') {
                                    types.push(t::STYLE_SEPARATOR);
                                    texts.push(":::".to_string());
                                    starts.push(p);
                                    p += 3;
                                } else {
                                    types.push(t::COLONSEP);
                                    texts.push(":".to_string());
                                    starts.push(p);
                                    p += 1;
                                }
                                ok = true;
                            }
                            '<' => {
                                if src.get(p + 1) == Some(&'-') {
                                    types.push(t::END_ARROW_L);
                                    texts.push("<-".to_string());
                                    starts.push(p);
                                    p += 2;
                                    ok = true;
                                }
                            }
                            '-' => {
                                if src.get(p + 1) == Some(&'>') {
                                    types.push(t::END_ARROW_R);
                                    texts.push("->".to_string());
                                    starts.push(p);
                                    p += 2;
                                } else {
                                    types.push(t::LINE);
                                    texts.push("-".to_string());
                                    starts.push(p);
                                    p += 1;
                                }
                                ok = true;
                            }
                            '"' => {
                                stack.push(state);
                                state = State::String;
                                p += 1;
                                ok = true;
                            }
                            ',' => {
                                types.push(t::COMMA);
                                texts.push(",".to_string());
                                starts.push(p);
                                p += 1;
                                ok = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            State::Style => {
                if is_word(c) {
                    let mut q = p + 1;
                    while q < n && is_word(src[q]) {
                        q += 1;
                    }
                    types.push(t::ALPHA);
                    texts.push(src[p..q].iter().collect());
                    starts.push(p);
                    p = q;
                } else {
                    match c {
                        ':' => {
                            types.push(t::COLON);
                            texts.push(":".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        ';' => {
                            types.push(t::SEMICOLON);
                            texts.push(";".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        '%' => {
                            types.push(t::PERCENT);
                            texts.push("%".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        '-' => {
                            types.push(t::MINUS);
                            texts.push("-".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        '#' => {
                            types.push(t::BRKT);
                            texts.push("#".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        ' ' => {
                            p += 1;
                        }
                        '"' => {
                            stack.push(state);
                            state = State::String;
                            p += 1;
                        }
                        '\n' => {
                            if let Some(prev) = stack.pop() {
                                state = prev;
                            }
                            p += 1;
                        }
                        ',' => {
                            types.push(t::COMMA);
                            texts.push(",".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        _ => ok = false,
                    }
                }
            }
            State::String => {
                if c == '"' {
                    if let Some(prev) = stack.pop() {
                        state = prev;
                    }
                    p += 1;
                } else {
                    let mut e = p;
                    while e < n && src[e] != '"' {
                        e += 1;
                    }
                    types.push(t::Q_STRING);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                }
            }
            State::AccTitle => {
                if let Some(prev) = stack.pop() {
                    state = prev;
                }
                let e = line_break(src, p);
                types.push(t::ACC_TITLE_VALUE);
                texts.push(src[p..e].iter().collect());
                starts.push(p);
                p = e;
            }
            State::AccDescr => {
                if let Some(prev) = stack.pop() {
                    state = prev;
                }
                let e = line_break(src, p);
                types.push(t::ACC_DESCR_VALUE);
                texts.push(src[p..e].iter().collect());
                starts.push(p);
                p = e;
            }
            State::AccDescrMultiline => {
                if c == '}' {
                    if let Some(prev) = stack.pop() {
                        state = prev;
                    }
                    p += 1;
                } else {
                    let mut e = p;
                    while e < n && src[e] != '}' {
                        e += 1;
                    }
                    types.push(t::ACC_DESCR_MULTILINE_VALUE);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                }
            }
        }
        if !ok {
            types.push(t::ERROR);
            texts.push(String::new());
            starts.push(p);
            return Tokens { types, texts, starts };
        }
    }

    // At the end of input Jison still tries the rules that can match nothing, once.
    let last = match state {
        State::Initial => t::EOF,
        State::String => t::Q_STRING,
        State::AccTitle => t::ACC_TITLE_VALUE,
        State::AccDescr => t::ACC_DESCR_VALUE,
        State::AccDescrMultiline => t::ACC_DESCR_MULTILINE_VALUE,
        State::Style => -1,
    };
    if last != -1 {
        types.push(last);
        texts.push(String::new());
        starts.push(n);
    }
    types.push(t::END);
    texts.push(String::new());
    starts.push(n);
    Tokens { types, texts, starts }
}
