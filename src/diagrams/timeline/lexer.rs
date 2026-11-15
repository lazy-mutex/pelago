use crate::chars::{is_space, is_word};
use crate::diagrams::common::scan::{accessibility, keyword_at, line_end, skip_trivia, Tokens};

pub mod t {
    pub const END: i32 = 0;
    pub const EOF: i32 = 1;
    pub const INVALID: i32 = 2;
    pub const NEWLINE: i32 = 3;
    pub const TIMELINE: i32 = 4;
    pub const TIMELINE_LR: i32 = 5;
    pub const TIMELINE_TD: i32 = 6;
    pub const TITLE: i32 = 7;
    pub const ACC_TITLE: i32 = 8;
    pub const ACC_TITLE_VALUE: i32 = 9;
    pub const ACC_DESCR: i32 = 10;
    pub const ACC_DESCR_VALUE: i32 = 11;
    pub const ACC_DESCR_MULTILINE_VALUE: i32 = 12;
    pub const SECTION: i32 = 13;
    pub const PERIOD: i32 = 14;
    pub const EVENT: i32 = 15;
}

pub const TOKEN_NAMES: [&str; 16] = [
    "$end",
    "EOF",
    "INVALID",
    "NEWLINE",
    "timeline",
    "timeline_lr",
    "timeline_td",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "section",
    "period",
    "event",
];

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
        if keyword_at(src, p, "timeline") {
            e = p + 8;
            while e < n && (src[e] == ' ' || src[e] == '\t') {
                e += 1;
            }
            let lr = keyword_at(src, e, "lr");
            if e > p + 8
                && (lr || keyword_at(src, e, "td"))
                && !src.get(e + 2).is_some_and(|&ch| is_word(ch))
            {
                emit_token(
                    &mut out,
                    src,
                    if lr { t::TIMELINE_LR } else { t::TIMELINE_TD },
                    &mut p,
                    e + 2,
                );
                continue;
            }
            if !src.get(p + 8).is_some_and(|&ch| is_word(ch)) {
                let end = p + 8;
                emit_token(&mut out, src, t::TIMELINE, &mut p, end);
                continue;
            }
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
                    let le = line_end(src, p + 6);
                    emit_token(&mut out, src, t::TITLE, &mut p, le);
                    continue;
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
            e = p + 8;
            while e < n && src[e] != ':' && src[e] != '\n' {
                e += 1;
            }
            if e > p + 8 {
                emit_token(&mut out, src, t::SECTION, &mut p, e);
                continue;
            }
        }
        if c != ':' {
            e = p + 1;
            while e < n && src[e] != '#' && src[e] != ':' && src[e] != '\n' {
                e += 1;
            }
            emit_token(&mut out, src, t::PERIOD, &mut p, e);
            continue;
        }
        // An event runs to the end of the line or to a colon that is followed by whitespace.
        if src.get(p + 1).is_some_and(|&ch| is_space(ch)) {
            e = p + 2;
            while e < n {
                let k = src[e];
                if k == '\n' || (k == ':' && src.get(e + 1).is_some_and(|&ch| is_space(ch))) {
                    break;
                }
                e += 1;
            }
            if e > p + 2 {
                emit_token(&mut out, src, t::EVENT, &mut p, e);
                continue;
            }
        }
        let end = p + 1;
        emit_token(&mut out, src, t::INVALID, &mut p, end);
    }

    // Jison marks its lexer done when a rule runs on empty input, so an accessibility value that
    // matches nothing at the very end of the text is followed by no EOF token.
    if out.starts.is_empty() || *out.starts.last().unwrap() < n {
        emit_token(&mut out, src, t::EOF, &mut p, n);
    }
    emit_token(&mut out, src, t::END, &mut p, n);
    out
}
