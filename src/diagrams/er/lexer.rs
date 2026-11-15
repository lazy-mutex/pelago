use super::tokens::t;
use crate::chars::{is_digit, is_line_end, is_space, is_word};

pub struct Tokens {
    pub types: Vec<i32>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    Block,
    BlockBq,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Style,
}

const TEXT: u8 = 1;
const STYLE_TEXT: u8 = 2;
const ATTR_START: u8 = 4;
const ATTR_REST: u8 = 8;

const fn build_class_table() -> [u8; 128] {
    let mut table = [0u8; 128];
    let mut c = 0usize;
    while c < 128 {
        let word = (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95;
        let letter = word && (c < 48 || c > 57);
        if word {
            table[c] |= TEXT | STYLE_TEXT | ATTR_REST;
        }
        if letter {
            table[c] |= ATTR_START;
        }
        c += 1;
    }
    table[b'-' as usize] |= TEXT | STYLE_TEXT | ATTR_REST;
    table[b'*' as usize] |= TEXT | STYLE_TEXT | ATTR_REST | ATTR_START;
    table[b'.' as usize] |= TEXT | ATTR_REST;
    table[b'[' as usize] |= ATTR_REST;
    table[b']' as usize] |= ATTR_REST;
    table[b'(' as usize] |= ATTR_REST;
    table[b')' as usize] |= ATTR_REST;
    table[b',' as usize] |= ATTR_REST;
    table
}

const CLASS: [u8; 128] = build_class_table();

// Mermaid's attribute pattern is case-insensitive, which lets the micro sign into its \u00c0-\uffff range.
fn is_attr(c: char, kind: u8) -> bool {
    let code = c as u32;
    if code < 128 {
        (CLASS[code as usize] & kind) != 0
    } else {
        code >= 0xc0 || code == 0xb5
    }
}

fn is_text_char(c: char) -> bool {
    let code = c as u32;
    code >= 128 || (CLASS[code as usize] & TEXT) != 0
}

fn is_style_text_char(c: char) -> bool {
    let code = c as u32;
    code >= 128 || (CLASS[code as usize] & STYLE_TEXT) != 0
}

// Matches a lower-case keyword at `at`, ignoring case.
fn kw_at(src: &[char], at: usize, word: &str) -> bool {
    if at + word.len() > src.len() {
        return false;
    }
    for (i, b) in word.bytes().enumerate() {
        let c = src[at + i];
        if b.is_ascii_lowercase() {
            if !c.is_ascii() || (c as u8).to_ascii_lowercase() != b {
                return false;
            }
        } else if c != (b as char) {
            return false;
        }
    }
    true
}

fn kwb_at(src: &[char], at: usize, word: &str) -> bool {
    kw_at(src, at, word) && !src.get(at + word.len()).copied().is_some_and(is_word)
}

fn match_acc(src: &[char], at: usize, kw: &str, sep: char) -> Option<usize> {
    let n = src.len();
    if !kw_at(src, at, kw) {
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

fn next_line_end(src: &[char], q: usize) -> usize {
    let n = src.len();
    let mut i = q;
    while i < n && !is_line_end(src[i]) {
        i += 1;
    }
    i
}

fn digits(src: &[char], mut q: usize) -> usize {
    let n = src.len();
    while q < n && is_digit(src[q]) {
        q += 1;
    }
    q
}

// Matches `/.*direction\s+XX[^\n]*/iy` at `p`, where `line_end` is `next_line_end(src, p)`.
fn match_direction_line(src: &[char], p: usize, line_end: usize, dir_code: &str) -> Option<usize> {
    if line_end < p + 9 {
        return None;
    }
    let n = src.len();
    let mut m = line_end - 9;
    loop {
        if kw_at(src, m, "direction") {
            let mut q = m + 9;
            while q < n && is_space(src[q]) {
                q += 1;
            }
            if q > m + 9 && kw_at(src, q, dir_code) {
                let mut e = q + 2;
                while e < n && src[e] != '\n' {
                    e += 1;
                }
                return Some(e);
            }
        }
        if m == p {
            break;
        }
        m -= 1;
    }
    None
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types: Vec<i32> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut state = State::Initial;
    let mut p = 0usize;
    let mut dir_next: isize = -2;
    let mut dir_failed: usize = 0;
    let mut line_end: isize = -1;
    let mut tilde_next: isize = -2;
    let mut tilde_line: isize = -1;
    let mut tilde_last: isize = -1;
    let mut ws_next: isize = -1;

    while p < n {
        let c = src[p];
        match state {
            State::Initial => {
                if (c == 'a' || c == 'A') && kw_at(src, p, "acc") {
                    if let Some(e) = match_acc(src, p, "acctitle", ':') {
                        state = State::AccTitle;
                        types.push(t::ACC_TITLE);
                        texts.push(src[p..e].iter().collect());
                        starts.push(p);
                        p = e;
                        continue;
                    }
                    if let Some(e) = match_acc(src, p, "accdescr", ':') {
                        state = State::AccDescr;
                        types.push(t::ACC_DESCR);
                        texts.push(src[p..e].iter().collect());
                        starts.push(p);
                        p = e;
                        continue;
                    }
                    if let Some(e) = match_acc(src, p, "accdescr", '{') {
                        state = State::AccDescrMultiline;
                        p = e;
                        continue;
                    }
                }

                // A direction statement swallows its whole line, from wherever the lexer stands on it.
                if dir_next != -1 {
                    if dir_next < p as isize {
                        let mut found: isize = -1;
                        let mut k = p;
                        while k + 9 <= n {
                            if kw_at(src, k, "direction") {
                                found = k as isize;
                                break;
                            }
                            k += 1;
                        }
                        dir_next = found;
                    }
                    if dir_next != -1 && p >= dir_failed {
                        if line_end < p as isize {
                            line_end = next_line_end(src, p) as isize;
                        }
                        if dir_next < line_end {
                            let le = line_end as usize;
                            let mut matched_dir = false;
                            for (idx, code) in ["tb", "bt", "rl", "lr"].iter().enumerate() {
                                if let Some(e) = match_direction_line(src, p, le, code) {
                                    types.push(t::DIRECTION_TB + idx as i32);
                                    texts.push(src[p..e].iter().collect());
                                    starts.push(p);
                                    p = e;
                                    matched_dir = true;
                                    break;
                                }
                            }
                            if matched_dir {
                                continue;
                            }
                            dir_failed = le;
                        }
                    }
                }

                match c {
                    ' ' | '\t' | '\r' => {
                        let mut e = p + 1;
                        while e < n && matches!(src[e], ' ' | '\t' | '\r') {
                            e += 1;
                        }
                        p = e;
                        continue;
                    }
                    '\n' => {
                        let mut e = p + 1;
                        while e < n && src[e] == '\n' {
                            e += 1;
                        }
                        types.push(t::NEWLINE);
                        texts.push(src[p..e].iter().collect());
                        starts.push(p);
                        p = e;
                        continue;
                    }
                    '"' => {
                        let mut found = None;
                        let mut q = p + 1;
                        while q < n {
                            if src[q] == '"' {
                                found = Some(q);
                                break;
                            }
                            q += 1;
                        }
                        if let Some(e) = found {
                            let mut is_name = e > p + 1;
                            let mut k = p + 1;
                            while is_name && k < e {
                                if matches!(src[k], '%' | '\\' | '\r' | '\n' | '\x0b' | '\x08') {
                                    is_name = false;
                                }
                                k += 1;
                            }
                            types.push(if is_name { t::ENTITY_NAME } else { t::WORD });
                            texts.push(src[p..e + 1].iter().collect());
                            starts.push(p);
                            p = e + 1;
                        } else {
                            types.push(t::CHAR);
                            texts.push("\"".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        continue;
                    }
                    '{' => {
                        state = State::Block;
                        types.push(t::BLOCK_START);
                        texts.push("{".to_string());
                        starts.push(p);
                        p += 1;
                        continue;
                    }
                    '#' => {
                        types.push(t::BRKT);
                        texts.push("#".to_string());
                        starts.push(p);
                        p += 1;
                        continue;
                    }
                    ',' => {
                        types.push(t::COMMA);
                        texts.push(",".to_string());
                        starts.push(p);
                        p += 1;
                        continue;
                    }
                    ':' => {
                        if src.get(p + 1) == Some(&':') && src.get(p + 2) == Some(&':') {
                            types.push(t::STYLE_SEPARATOR);
                            texts.push(":::".to_string());
                            starts.push(p);
                            p += 3;
                        } else {
                            types.push(t::COLON);
                            texts.push(":".to_string());
                            starts.push(p);
                            p += 1;
                        }
                        continue;
                    }
                    '[' => {
                        types.push(t::SQS);
                        texts.push("[".to_string());
                        starts.push(p);
                        p += 1;
                        continue;
                    }
                    ']' => {
                        types.push(t::SQE);
                        texts.push("]".to_string());
                        starts.push(p);
                        p += 1;
                        continue;
                    }
                    _ => {}
                }

                if is_digit(c) {
                    let d = src.get(p + 1).copied();
                    if d == Some('+') && (c == '1' || c == '0') {
                        types.push(if c == '1' { t::ONE_OR_MORE } else { t::ZERO_OR_MORE });
                        texts.push(src[p..p + 2].iter().collect());
                        starts.push(p);
                        p += 2;
                        continue;
                    }
                    let q = digits(src, p + 1);
                    if src.get(q) == Some(&'.') && src.get(q + 1).copied().is_some_and(is_digit) {
                        let e = digits(src, q + 2);
                        types.push(t::DECIMAL_NUM);
                        texts.push(src[p..e].iter().collect());
                        starts.push(p);
                        p = e;
                        continue;
                    }
                    if c == '1' {
                        if d.is_some_and(is_space) {
                            let mut r = p + 2;
                            while r < n && is_space(src[r]) {
                                r += 1;
                            }
                            if src.get(r).copied().is_some_and(|f| is_word(f) || f == '"' || f == '\'') {
                                types.push(t::ONLY_ONE);
                                texts.push("1".to_string());
                                starts.push(p);
                                p += 1;
                                continue;
                            }
                        } else if matches!(d, Some('-' | '.')) && matches!(src.get(p + 2), Some('-' | '.')) {
                            types.push(t::ONLY_ONE);
                            texts.push("1".to_string());
                            starts.push(p);
                            p += 1;
                            continue;
                        }
                        if !d.is_some_and(is_word) {
                            types.push(t::ENTITY_ONE);
                            texts.push("1".to_string());
                            starts.push(p);
                            p += 1;
                            continue;
                        }
                    }
                    types.push(t::NUM);
                    texts.push(src[p..q].iter().collect());
                    starts.push(p);
                    p = q;
                    continue;
                }

                let lc = c.to_ascii_lowercase();
                let d = src.get(p + 1).copied();
                let mut kw_hit: Option<(i32, usize)> = None;
                match lc {
                    'e' => {
                        if kwb_at(src, p, "erdiagram") {
                            kw_hit = Some((t::ER_DIAGRAM, p + 9));
                        } else if kwb_at(src, p, "end") {
                            let mut e = p + 3;
                            while e < n && is_space(src[e]) {
                                e += 1;
                            }
                            kw_hit = Some((t::END_KW, e));
                        }
                    }
                    's' => {
                        if kwb_at(src, p, "style") {
                            state = State::Style;
                            kw_hit = Some((t::STYLE, p + 5));
                        } else if kwb_at(src, p, "subgraph") {
                            kw_hit = Some((t::SUBGRAPH, p + 8));
                        }
                    }
                    'c' => {
                        if kwb_at(src, p, "classdef") {
                            state = State::Style;
                            kw_hit = Some((t::CLASSDEF, p + 8));
                        } else if kwb_at(src, p, "class") {
                            kw_hit = Some((t::CLASS, p + 5));
                        }
                    }
                    'o' => {
                        if kwb_at(src, p, "one or zero") {
                            kw_hit = Some((t::ZERO_OR_ONE, p + 11));
                        } else if kwb_at(src, p, "one or more") || kwb_at(src, p, "one or many") {
                            kw_hit = Some((t::ONE_OR_MORE, p + 11));
                        } else if kwb_at(src, p, "one") {
                            kw_hit = Some((t::ONLY_ONE, p + 3));
                        } else if kwb_at(src, p, "only one") {
                            kw_hit = Some((t::ONLY_ONE, p + 8));
                        } else if d == Some('|') {
                            kw_hit = Some((t::ZERO_OR_ONE, p + 2));
                        } else if d == Some('{') {
                            kw_hit = Some((t::ZERO_OR_MORE, p + 2));
                        } else if kwb_at(src, p, "optionally to") {
                            kw_hit = Some((t::NON_IDENTIFYING, p + 13));
                        }
                    }
                    'z' => {
                        if kwb_at(src, p, "zero or one") {
                            kw_hit = Some((t::ZERO_OR_ONE, p + 11));
                        } else if kwb_at(src, p, "zero or more") || kwb_at(src, p, "zero or many") {
                            kw_hit = Some((t::ZERO_OR_MORE, p + 12));
                        }
                    }
                    'm' => {
                        if kw_at(src, p, "many(0)") {
                            kw_hit = Some((t::ZERO_OR_MORE, p + 7));
                        } else if kw_at(src, p, "many(1)") {
                            kw_hit = Some((t::ONE_OR_MORE, p + 7));
                        } else if kwb_at(src, p, "many") {
                            kw_hit = Some((t::ZERO_OR_MORE, p + 4));
                        }
                    }
                    't' => {
                        if kwb_at(src, p, "to") {
                            kw_hit = Some((t::IDENTIFYING, p + 2));
                        }
                    }
                    'u' => {
                        if matches!(d, Some('.' | '-' | '|')) {
                            kw_hit = Some((t::MD_PARENT, p + 1));
                        }
                    }
                    '|' => {
                        if matches!(d, Some('o' | 'O')) && !src.get(p + 2).copied().is_some_and(is_word) {
                            kw_hit = Some((t::ZERO_OR_ONE, p + 2));
                        } else if d == Some('|') {
                            kw_hit = Some((t::ONLY_ONE, p + 2));
                        } else if d == Some('{') {
                            kw_hit = Some((t::ONE_OR_MORE, p + 2));
                        }
                    }
                    '}' => {
                        if matches!(d, Some('o' | 'O')) && !src.get(p + 2).copied().is_some_and(is_word) {
                            kw_hit = Some((t::ZERO_OR_MORE, p + 2));
                        } else if d == Some('|') {
                            kw_hit = Some((t::ONE_OR_MORE, p + 2));
                        }
                    }
                    '.' => {
                        if matches!(d, Some('.' | '-')) {
                            kw_hit = Some((t::NON_IDENTIFYING, p + 2));
                        }
                    }
                    '-' => {
                        if d == Some('-') {
                            kw_hit = Some((t::IDENTIFYING, p + 2));
                        } else if d == Some('.') {
                            kw_hit = Some((t::NON_IDENTIFYING, p + 2));
                        }
                    }
                    _ => {}
                }
                if let Some((tok, e)) = kw_hit {
                    types.push(tok);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                    continue;
                }

                if is_text_char(c) {
                    let mut e = p + 1;
                    while e < n && is_text_char(src[e]) {
                        e += 1;
                    }
                    types.push(t::UNICODE_TEXT);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                    continue;
                }

                types.push(t::CHAR);
                texts.push(c.to_string());
                starts.push(p);
                p += 1;
            }

            State::Block => {
                if is_space(c) {
                    p += 1;
                    while p < n && is_space(src[p]) {
                        p += 1;
                    }
                    continue;
                }
                if matches!(c, 'p' | 'P' | 'f' | 'F' | 'u' | 'U')
                    && matches!(src.get(p + 1), Some('k' | 'K'))
                    && !src.get(p + 2).copied().is_some_and(is_word)
                {
                    types.push(t::ATTRIBUTE_KEY);
                    texts.push(src[p..p + 2].iter().collect());
                    starts.push(p);
                    p += 2;
                    continue;
                }

                // Mermaid's generic type rule: a run of non-space characters holding a `~`, through the last `~` on the line.
                if tilde_next != -1 {
                    if tilde_next < p as isize {
                        let mut found: isize = -1;
                        let mut k = p;
                        while k < n {
                            if src[k] == '~' {
                                found = k as isize;
                                break;
                            }
                            k += 1;
                        }
                        tilde_next = found;
                    }
                    if tilde_next != -1 {
                        if ws_next < p as isize {
                            let mut q = p;
                            while q < n && !is_space(src[q]) {
                                q += 1;
                            }
                            ws_next = q as isize;
                        }
                        if tilde_next < ws_next {
                            if tilde_line < p as isize {
                                let tl = next_line_end(src, p);
                                tilde_line = tl as isize;
                                let mut last: isize = -1;
                                let mut k = tl;
                                while k > 0 {
                                    k -= 1;
                                    if src[k] == '~' {
                                        last = k as isize;
                                        break;
                                    }
                                }
                                tilde_last = last;
                            }
                            if tilde_last > tilde_next {
                                let mut e = tilde_last as usize + 1;
                                while e < n && !is_space(src[e]) {
                                    e += 1;
                                }
                                types.push(t::ATTRIBUTE_WORD);
                                texts.push(src[p..e].iter().collect());
                                starts.push(p);
                                p = e;
                                continue;
                            }
                        }
                    }
                }

                if is_attr(c, ATTR_START) {
                    let mut e = p + 1;
                    while e < n && is_attr(src[e], ATTR_REST) {
                        e += 1;
                    }
                    types.push(t::ATTRIBUTE_WORD);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                    continue;
                }
                if c == '`' {
                    state = State::BlockBq;
                    p += 1;
                    continue;
                }
                if c == '"' {
                    let mut q = p + 1;
                    let mut found = None;
                    while q < n {
                        if src[q] == '"' {
                            found = Some(q);
                            break;
                        }
                        q += 1;
                    }
                    if let Some(e) = found {
                        types.push(t::COMMENT);
                        texts.push(src[p..e + 1].iter().collect());
                        starts.push(p);
                        p = e + 1;
                        continue;
                    }
                } else if c == '}' {
                    state = State::Initial;
                    types.push(t::BLOCK_STOP);
                    texts.push("}".to_string());
                    starts.push(p);
                    p += 1;
                    continue;
                }
                types.push(t::CHAR);
                texts.push(c.to_string());
                starts.push(p);
                p += 1;
            }

            State::BlockBq => {
                if c == '`' {
                    state = State::Block;
                    p += 1;
                } else {
                    let mut e = p;
                    while e < n && src[e] != '`' {
                        e += 1;
                    }
                    types.push(t::ATTRIBUTE_WORD);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                }
            }

            State::AccTitle | State::AccDescr => {
                let mut e = p;
                while e < n && src[e] != '\n' {
                    e += 1;
                }
                types.push(if state == State::AccTitle { t::ACC_TITLE_VALUE } else { t::ACC_DESCR_VALUE });
                texts.push(src[p..e].iter().collect());
                starts.push(p);
                p = e;
                state = State::Initial;
            }

            State::AccDescrMultiline => {
                if c == '}' {
                    state = State::Initial;
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

            State::Style => {
                if c == '\n' {
                    let mut e = p + 1;
                    while e < n && src[e] == '\n' {
                        e += 1;
                    }
                    state = State::Initial;
                    types.push(t::NEWLINE);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                } else if is_space(c) {
                    p += 1;
                    while p < n && is_space(src[p]) {
                        p += 1;
                    }
                } else if c == ':' {
                    types.push(t::COLON);
                    texts.push(":".to_string());
                    starts.push(p);
                    p += 1;
                } else if c == ',' {
                    types.push(t::COMMA);
                    texts.push(",".to_string());
                    starts.push(p);
                    p += 1;
                } else if c == '#' {
                    types.push(t::BRKT);
                    texts.push("#".to_string());
                    starts.push(p);
                    p += 1;
                } else if c == ';' {
                    types.push(t::SEMI);
                    texts.push(";".to_string());
                    starts.push(p);
                    p += 1;
                } else if is_style_text_char(c) {
                    let mut e = p + 1;
                    while e < n && is_style_text_char(src[e]) {
                        e += 1;
                    }
                    types.push(t::STYLE_TEXT);
                    texts.push(src[p..e].iter().collect());
                    starts.push(p);
                    p = e;
                } else {
                    types.push(t::ERROR);
                    texts.push(String::new());
                    starts.push(p);
                    return Tokens { types, texts, starts };
                }
            }
        }
    }

    // At the end of input Jison's INITIAL state yields EOF, the title and description states yield
    // an empty value, and the block and style states yield nothing.
    let last = match state {
        State::Initial => t::EOF,
        State::AccTitle => t::ACC_TITLE_VALUE,
        State::AccDescr => t::ACC_DESCR_VALUE,
        State::AccDescrMultiline => t::ACC_DESCR_MULTILINE_VALUE,
        State::Block | State::BlockBq | State::Style => -1,
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
