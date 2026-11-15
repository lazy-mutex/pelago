use super::tokens::t;
use crate::chars::{is_digit, is_letter, is_space, is_word};
use crate::diagrams::common::caseless::word_at;

pub struct Tokens {
    pub types: Vec<usize>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

const S_INITIAL: usize = 0;
const S_AXIS_DATA: usize = 1;
const S_AXIS_BAND_DATA: usize = 2;
const S_DATA: usize = 3;
const S_DATA_INNER: usize = 4;
const S_STRING: usize = 5;
const S_ACC_TITLE: usize = 6;
const S_ACC_DESCR: usize = 7;
const S_ACC_DESCR_MULTILINE: usize = 8;

const AT_END: [usize; 9] = [
    t::EOF,
    t::EOF,
    t::EOF,
    t::EOF,
    t::EOF,
    t::STR,
    t::ACC_TITLE_VALUE,
    t::ACC_DESCR_VALUE,
    t::ACC_DESCR_MULTILINE_VALUE,
];

const BROKEN_RULE: &str = concat!(
    "`)",
    "                                    ",
    "{ this.pushstate(md_string); }\n<md_string>(?:(?!`\").)+",
    "                  ",
    "{ return md_str; }\n<md_string>(?:`"
);

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
        ';' => t::SEMI,
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

    let line_end = |q: usize| -> usize {
        let mut i = q;
        while i < n && src[i] != '\n' {
            i += 1;
        }
        i
    };
    let run = |mut q: usize| -> usize {
        while q < n && is_space(src[q]) {
            q += 1;
        }
        q
    };
    let digits = |mut q: usize| -> usize {
        while q < n && is_digit(src[q]) {
            q += 1;
        }
        q
    };
    let keyword = |at: usize, word: &str| -> bool {
        word_at(src, at, word) && !src.get(at + word.len()).is_some_and(|&ch| is_word(ch))
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

    while p < n {
        let c = src[p];

        if state == S_STRING {
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
        if state == S_ACC_TITLE || state == S_ACC_DESCR {
            let end = line_end(p);
            emit!(AT_END[state], p, end);
            pop!();
            continue;
        }
        if state == S_ACC_DESCR_MULTILINE {
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

        let next = src.get(p + 1).copied();
        if c == '%' && next == Some('%') {
            p = line_end(p + 2);
            continue;
        }
        if c == '\n' || c == '\r' {
            if (state == S_AXIS_DATA || state == S_DATA) && (c == '\n' || next == Some('\n')) {
                let end = if c == '\n' { p + 1 } else { p + 2 };
                emit!(t::NEWLINE, p, end);
                pop!();
                continue;
            }
            let mut e = p + 1;
            while e < n && (src[e] == '\n' || src[e] == '\r') {
                e += 1;
            }
            emit!(t::NEWLINE, p, e);
            continue;
        }

        match c.to_ascii_lowercase() {
            't' if c.is_ascii_alphabetic() => {
                if keyword(p, "title") {
                    emit!(t::TITLE, p, p + 5);
                    continue;
                }
            }
            'a' if c.is_ascii_alphabetic() => {
                if word_at(src, p, "acc") {
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
            }
            'x' if c.is_ascii_alphabetic() => {
                if keyword(p, "xychart-beta") {
                    emit!(t::XYCHART, p, p + 12);
                    continue;
                }
                if keyword(p, "xychart") {
                    emit!(t::XYCHART, p, p + 7);
                    continue;
                }
                if keyword(p, "x-axis") {
                    emit!(t::X_AXIS, p, p + 6);
                    push!(S_AXIS_DATA);
                    continue;
                }
            }
            'y' if c.is_ascii_alphabetic() => {
                if keyword(p, "y-axis") {
                    emit!(t::Y_AXIS, p, p + 6);
                    push!(S_AXIS_DATA);
                    continue;
                }
            }
            'v' if c.is_ascii_alphabetic() => {
                if word_at(src, p, "vertical") {
                    emit!(t::CHART_ORIENTATION, p, p + 8);
                    continue;
                }
            }
            'h' if c.is_ascii_alphabetic() => {
                if word_at(src, p, "horizontal") {
                    emit!(t::CHART_ORIENTATION, p, p + 10);
                    continue;
                }
            }
            'l' if c.is_ascii_alphabetic() => {
                if keyword(p, "line") {
                    emit!(t::LINE, p, p + 4);
                    push!(S_DATA);
                    continue;
                }
            }
            'b' if c.is_ascii_alphabetic() => {
                if keyword(p, "bar") {
                    emit!(t::BAR, p, p + 3);
                    push!(S_DATA);
                    continue;
                }
            }
            _ => {}
        }

        if c == '[' {
            if state == S_AXIS_DATA {
                push!(S_AXIS_BAND_DATA);
            } else if state == S_DATA {
                push!(S_DATA_INNER);
            }
            emit!(t::SQUARE_BRACES_START, p, p + 1);
            continue;
        }
        if c == ']' {
            if state == S_DATA_INNER || state == S_AXIS_BAND_DATA {
                pop!();
            }
            emit!(t::SQUARE_BRACES_END, p, p + 1);
            continue;
        }
        if state == S_AXIS_DATA && c == '-' && next == Some('-') && src.get(p + 2) == Some(&'>') {
            emit!(t::ARROW_DELIMITER, p, p + 3);
            continue;
        }
        if state == S_AXIS_DATA || state == S_DATA_INNER {
            let q = if c == '+' || c == '-' { p + 1 } else { p };
            let d = src.get(q).copied();
            let mut end_opt: Option<usize> = None;
            if d.is_some_and(is_digit) {
                let mut e = digits(q + 1);
                if src.get(e) == Some(&'.') && src.get(e + 1).copied().is_some_and(is_digit) {
                    e = digits(e + 2);
                }
                end_opt = Some(e);
            } else if d == Some('.') && src.get(q + 1).copied().is_some_and(is_digit) {
                end_opt = Some(digits(q + 2));
            }
            if let Some(e) = end_opt {
                emit!(t::NUMBER_WITH_DECIMAL, p, e);
                continue;
            }
        }
        if c == '`' && word_at(src, p, BROKEN_RULE) {
            p += BROKEN_RULE.len();
            pop!();
            continue;
        }
        if c == '"' {
            p += 1;
            push!(S_STRING);
            continue;
        }
        if is_letter(c) {
            let mut e = p + 1;
            while e < n && is_letter(src[e]) {
                e += 1;
            }
            emit!(t::ALPHA, p, e);
            continue;
        }
        if is_digit(c) {
            let end = digits(p + 1);
            emit!(t::NUM, p, end);
            continue;
        }
        if is_space(c) {
            p = run(p + 1);
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
    types.push(AT_END[state]);
    texts.push(String::new());
    starts.push(n);
    types.push(t::END);
    texts.push(String::new());
    starts.push(n);
    Tokens { types, texts, starts }
}
