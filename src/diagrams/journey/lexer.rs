use crate::chars::{is_space, is_word};
use crate::diagrams::common::scan::{accessibility, keyword_at, skip_trivia, Tokens};

pub mod t {
    pub const END: i32 = 0;
    pub const EOF: i32 = 1;
    pub const INVALID: i32 = 2;
    pub const NEWLINE: i32 = 3;
    pub const JOURNEY: i32 = 4;
    pub const TITLE: i32 = 5;
    pub const ACC_TITLE: i32 = 6;
    pub const ACC_TITLE_VALUE: i32 = 7;
    pub const ACC_DESCR: i32 = 8;
    pub const ACC_DESCR_VALUE: i32 = 9;
    pub const ACC_DESCR_MULTILINE_VALUE: i32 = 10;
    pub const SECTION: i32 = 11;
    pub const TASK_NAME: i32 = 12;
    pub const TASK_DATA: i32 = 13;
    pub const COLON: i32 = 14;
}

pub const TOKEN_NAMES: [&str; 15] = [
    "$end",
    "EOF",
    "INVALID",
    "NEWLINE",
    "journey",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "section",
    "taskName",
    "taskData",
    ":",
];

// `#`, a line break, and `;` end every kind of text; a colon also ends a task name or a section.
fn text_end(src: &[char], from: usize, colon: bool) -> usize {
    let n = src.len();
    let mut e = from;
    while e < n {
        let c = src[e];
        if c == '#' || c == '\n' || c == ';' || (colon && c == ':') {
            break;
        }
        e += 1;
    }
    e
}

fn emit_token(out: &mut Tokens, src: &[char], kind: i32, p: &mut usize, e: usize) {
    out.types.push(kind);
    out.texts.push(src[*p..e].iter().collect());
    out.starts.push(*p);
    *p = e;
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut out = Tokens::default();
    let mut p = 0usize;

    loop {
        p = skip_trivia(src, p);
        if p >= n {
            break;
        }
        let c = src[p];
        let mut e = p + 1;
        if c == '\n' {
            while e < n && src[e] == '\n' {
                e += 1;
            }
            emit_token(&mut out, src, t::NEWLINE, &mut p, e);
            continue;
        }
        if keyword_at(src, p, "journey") && !src.get(p + 7).is_some_and(|&ch| is_word(ch)) {
            let end = p + 7;
            emit_token(&mut out, src, t::JOURNEY, &mut p, end);
            continue;
        }
        if keyword_at(src, p, "title") {
            // Unlike Mermaid, ignore bare titles.
            e = p + 5;
            while e < n && src[e] != '\n' && is_space(src[e]) {
                e += 1;
            }
            if e == n || src[e] == '\n' {
                p = e;
                continue;
            }
            if let Some(&sep) = src.get(p + 5) {
                if sep != '\r' && is_space(sep) {
                    e = text_end(src, p + 6, false);
                    if e > p + 6 {
                        emit_token(&mut out, src, t::TITLE, &mut p, e);
                        continue;
                    }
                }
            }
        }
        e = accessibility(src, p, &mut out, t::ACC_TITLE);
        if e != p {
            p = e;
            continue;
        }
        // Like Mermaid, a bare section uses the next line as its name.
        if keyword_at(src, p, "section") && src.get(p + 7).is_some_and(|&ch| is_space(ch)) {
            e = text_end(src, p + 8, true);
            if e > p + 8 {
                emit_token(&mut out, src, t::SECTION, &mut p, e);
                continue;
            }
        }
        if c == ':' {
            e = text_end(src, p + 1, false);
            if e > p + 1 {
                emit_token(&mut out, src, t::TASK_DATA, &mut p, e);
            } else {
                let end = p + 1;
                emit_token(&mut out, src, t::COLON, &mut p, end);
            }
        } else if c == ';' {
            let end = p + 1;
            emit_token(&mut out, src, t::INVALID, &mut p, end);
        } else {
            let te = text_end(src, p + 1, true);
            emit_token(&mut out, src, t::TASK_NAME, &mut p, te);
        }
    }

    // Jison marks its lexer done when a rule runs on empty input, so an accessibility value that
    // matches nothing at the very end of the text is followed by no EOF token.
    if out.starts.is_empty() || *out.starts.last().unwrap() < n {
        emit_token(&mut out, src, t::EOF, &mut p, n);
    }
    emit_token(&mut out, src, t::END, &mut p, n);
    out
}
