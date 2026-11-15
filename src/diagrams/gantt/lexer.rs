use super::tokens::t;
use crate::chars::{is_digit, is_space, is_word};
use crate::diagrams::common::caseless::word_at;

pub struct Tokens {
    pub types: Vec<usize>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Href,
    CallbackName,
    CallbackArgs,
    Click,
}

const DAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

fn word_end(src: &[char], at: usize) -> bool {
    !src.get(at).is_some_and(|&c| is_word(c))
}

fn is_inline_space(c: char) -> bool {
    is_space(c) && c != '\r' && c != '\n'
}

fn skip_spaces(src: &[char], mut p: usize) -> usize {
    while p < src.len() && is_space(src[p]) {
        p += 1;
    }
    p
}

fn until_char(src: &[char], target: char, mut from: usize) -> usize {
    while from < src.len() && src[from] != target {
        from += 1;
    }
    from
}

// Matches `<kw>\s[^#\n;]+` case-insensitively at `p`.
fn match_setting(src: &[char], p: usize, kw: &str) -> Option<usize> {
    if !word_at(src, p, kw) {
        return None;
    }
    let after = p + kw.len();
    if !src.get(after).is_some_and(|&c| is_space(c)) {
        return None;
    }
    let start = after + 1;
    let mut e = start;
    while e < src.len() && !matches!(src[e], '#' | '\n' | ';') {
        e += 1;
    }
    (e > start).then_some(e)
}

// Matches `<kw>\b` case-insensitively at `p`.
fn match_bare_kw(src: &[char], p: usize, kw: &str) -> Option<usize> {
    let end = p + kw.len();
    (word_at(src, p, kw) && word_end(src, end)).then_some(end)
}

fn match_date(src: &[char], p: usize) -> bool {
    p + 10 <= src.len()
        && is_digit(src[p])
        && is_digit(src[p + 1])
        && is_digit(src[p + 2])
        && is_digit(src[p + 3])
        && src[p + 4] == '-'
        && is_digit(src[p + 5])
        && is_digit(src[p + 6])
        && src[p + 7] == '-'
        && is_digit(src[p + 8])
        && is_digit(src[p + 9])
        && word_end(src, p + 10)
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types = Vec::new();
    let mut texts = Vec::new();
    let mut starts = Vec::new();
    let mut state = State::Initial;
    let mut p = 0usize;

    macro_rules! emit {
        ($kind:expr, $s:expr, $e:expr) => {{
            let s_val = $s;
            let e_val = $e;
            types.push($kind);
            texts.push(src[s_val..e_val].iter().collect());
            starts.push(s_val);
            p = e_val;
        }};
    }

    loop {
        if p >= n {
            let kind = match state {
                State::Initial => t::EOF,
                State::AccTitle => t::ACC_TITLE_VALUE,
                State::AccDescr => t::ACC_DESCR_VALUE,
                State::AccDescrMultiline => t::ACC_DESCR_MULTILINE_VALUE,
                State::Href => t::HREF,
                State::CallbackName => t::CALLBACKNAME,
                State::CallbackArgs => t::CALLBACKARGS,
                State::Click => t::CLICK,
            };
            types.push(kind);
            texts.push(String::new());
            starts.push(n);
            break;
        }

        let c = src[p];

        match state {
            State::Initial => {
                if c == '%' {
                    if src.get(p + 1) != Some(&'%') {
                        let mut e = until_char(src, '\n', p + 1);
                        while src.get(e) == Some(&'\n') {
                            e += 1;
                        }
                        p = e;
                    } else if src.get(p + 2) == Some(&'{') {
                        // The grammar enters a lexer state it never defines, so nothing can follow.
                        emit!(t::OPEN_DIRECTIVE, p, p + 3);
                        types.push(t::ERROR);
                        texts.push(String::new());
                        starts.push(p);
                        return Tokens { types, texts, starts };
                    } else {
                        p = until_char(src, '\n', p + 2);
                    }
                } else if c == '\n' {
                    let mut e = p + 1;
                    while src.get(e) == Some(&'\n') {
                        e += 1;
                    }
                    emit!(t::NL, p, e);
                } else if is_space(c) {
                    p += 1;
                    while p < n && is_space(src[p]) {
                        p += 1;
                    }
                } else if try_keyword(src, &mut p, &mut state, &mut types, &mut texts, &mut starts) {
                    // Handled by keyword rule.
                } else if is_digit(c) && match_date(src, p) {
                    emit!(t::DATE, p, p + 10);
                } else if c != ':' {
                    let mut e = p + 1;
                    while e < n && src[e] != ':' && src[e] != '\n' {
                        e += 1;
                    }
                    emit!(t::TASK_TXT, p, e);
                } else {
                    let mut e = p + 1;
                    while e < n && !matches!(src[e], '#' | '\n' | ';') {
                        e += 1;
                    }
                    let kind = if e > p + 1 { t::TASK_DATA } else { t::COLON };
                    emit!(kind, p, e);
                }
            }

            State::AccTitle | State::AccDescr => {
                let kind = if state == State::AccTitle {
                    t::ACC_TITLE_VALUE
                } else {
                    t::ACC_DESCR_VALUE
                };
                state = State::Initial;
                let e = until_char(src, '\n', p);
                emit!(kind, p, e);
            }

            State::AccDescrMultiline => {
                if c == '}' {
                    state = State::Initial;
                    p += 1;
                } else {
                    let e = until_char(src, '}', p);
                    emit!(t::ACC_DESCR_MULTILINE_VALUE, p, e);
                }
            }

            State::Href => {
                if c == '"' {
                    state = State::Initial;
                    p += 1;
                } else {
                    let e = until_char(src, '"', p);
                    emit!(t::HREF, p, e);
                }
            }

            State::CallbackName => {
                if c == '(' {
                    let after_spaces = skip_spaces(src, p + 1);
                    if src.get(after_spaces) == Some(&')') {
                        state = State::Initial;
                        p = after_spaces + 1;
                    } else {
                        state = State::CallbackArgs;
                        p += 1;
                    }
                } else {
                    let e = until_char(src, '(', p);
                    emit!(t::CALLBACKNAME, p, e);
                }
            }

            State::CallbackArgs => {
                if c == ')' {
                    state = State::Initial;
                    p += 1;
                } else {
                    let e = until_char(src, ')', p);
                    emit!(t::CALLBACKARGS, p, e);
                }
            }

            State::Click => {
                if is_space(c) {
                    state = State::Initial;
                    p += 1;
                } else {
                    let mut e = p + 1;
                    while e < n && !is_space(src[e]) {
                        e += 1;
                    }
                    emit!(t::CLICK, p, e);
                }
            }
        }
    }

    types.push(t::END);
    texts.push(String::new());
    starts.push(n);
    Tokens { types, texts, starts }
}

fn try_keyword(
    src: &[char],
    p: &mut usize,
    state: &mut State,
    types: &mut Vec<usize>,
    texts: &mut Vec<String>,
    starts: &mut Vec<usize>,
) -> bool {
    let at = *p;
    let first = (src[at] as u32) | 32;
    let mut push = |kind: usize, end: usize| {
        types.push(kind);
        texts.push(src[at..end].iter().collect());
        starts.push(at);
        *p = end;
    };

    match first {
        // 'a'
        97 => {
            if word_at(src, at, "acctitle") {
                let colon = skip_spaces(src, at + 8);
                if src.get(colon) == Some(&':') {
                    let end = skip_spaces(src, colon + 1);
                    *state = State::AccTitle;
                    push(t::ACC_TITLE, end);
                    return true;
                }
            }
            if word_at(src, at, "accdescr") {
                let next = skip_spaces(src, at + 8);
                if src.get(next) == Some(&':') {
                    let end = skip_spaces(src, next + 1);
                    *state = State::AccDescr;
                    push(t::ACC_DESCR, end);
                    return true;
                }
                if src.get(next) == Some(&'{') {
                    let end = skip_spaces(src, next + 1);
                    *state = State::AccDescrMultiline;
                    *p = end;
                    return true;
                }
            }
            if let Some(end) = match_setting(src, at, "axisformat") {
                push(t::AXIS_FORMAT, end);
                return true;
            }
            if let Some(end) = match_setting(src, at, "accdescription") {
                push(t::ACC_DESCRIPTION, end);
                return true;
            }
        }
        // 'h'
        104 => {
            if word_at(src, at, "href") && src.get(at + 4).is_some_and(|&c| is_space(c)) {
                let q = skip_spaces(src, at + 5);
                if src.get(q) == Some(&'"') {
                    *state = State::Href;
                    *p = q + 1;
                    return true;
                }
            }
        }
        // 'c'
        99 => {
            if word_at(src, at, "call") && src.get(at + 4).is_some_and(|&c| is_space(c)) {
                *state = State::CallbackName;
                *p = skip_spaces(src, at + 5);
                return true;
            }
            if word_at(src, at, "click") && src.get(at + 5).is_some_and(|&c| is_space(c)) {
                *state = State::Click;
                *p = skip_spaces(src, at + 6);
                return true;
            }
        }
        // 't'
        116 => {
            // RE_BARE_TITLE = /title[^\S\r\n]*(?=[\r\n]|$)/iy
            if word_at(src, at, "title") {
                let mut e = at + 5;
                while e < src.len() && is_inline_space(src[e]) {
                    e += 1;
                }
                if e == src.len() || matches!(src[e], '\r' | '\n') {
                    *p = e;
                    return true;
                }
            }
            if let Some(end) = match_bare_kw(src, at, "topaxis") {
                push(t::TOP_AXIS, end);
                return true;
            }
            if let Some(end) = match_setting(src, at, "tickinterval") {
                push(t::TICK_INTERVAL, end);
                return true;
            }
            // /todayMarker\s[^\n;]+/iy (allows `#` for hex colors)
            if word_at(src, at, "todaymarker") && src.get(at + 11).is_some_and(|&c| is_space(c)) {
                let start = at + 12;
                let mut e = start;
                while e < src.len() && !matches!(src[e], '\n' | ';') {
                    e += 1;
                }
                if e > start {
                    push(t::TODAY_MARKER, e);
                    return true;
                }
            }
            // /title[^\S\r\n][^\n]+/iy
            if word_at(src, at, "title") && src.get(at + 5).is_some_and(|&c| is_inline_space(c)) {
                let start = at + 6;
                let mut e = start;
                while e < src.len() && src[e] != '\n' {
                    e += 1;
                }
                if e > start {
                    push(t::TITLE, e);
                    return true;
                }
            }
        }
        // 'w'
        119 => {
            if word_at(src, at, "weekday") && src.get(at + 7).is_some_and(|&c| is_space(c)) {
                let day_at = skip_spaces(src, at + 8);
                for (idx, day) in DAYS.iter().enumerate() {
                    let end = day_at + day.len();
                    if word_at(src, day_at, day) && word_end(src, end) {
                        push(t::WEEKDAY_MONDAY + idx, end);
                        return true;
                    }
                }
            }
            if word_at(src, at, "weekend") && src.get(at + 7).is_some_and(|&c| is_space(c)) {
                let day_at = skip_spaces(src, at + 8);
                if word_at(src, day_at, "friday") && word_end(src, day_at + 6) {
                    push(t::WEEKEND_FRIDAY, day_at + 6);
                    return true;
                }
                if word_at(src, day_at, "saturday") && word_end(src, day_at + 8) {
                    push(t::WEEKEND_SATURDAY, day_at + 8);
                    return true;
                }
            }
        }
        // 'g'
        103 => {
            if let Some(end) = match_bare_kw(src, at, "gantt") {
                push(t::GANTT, end);
                return true;
            }
        }
        // 'd'
        100 => {
            if let Some(end) = match_setting(src, at, "dateformat") {
                push(t::DATE_FORMAT, end);
                return true;
            }
        }
        // 'i'
        105 => {
            if let Some(end) = match_bare_kw(src, at, "inclusiveenddates") {
                push(t::INCLUSIVE_END_DATES, end);
                return true;
            }
            if let Some(end) = match_setting(src, at, "includes") {
                push(t::INCLUDES, end);
                return true;
            }
        }
        // 'e'
        101 => {
            if let Some(end) = match_setting(src, at, "excludes") {
                push(t::EXCLUDES, end);
                return true;
            }
        }
        // 's'
        115 => {
            // /section\s[^\n]+/iy
            if word_at(src, at, "section") && src.get(at + 7).is_some_and(|&c| is_space(c)) {
                let start = at + 8;
                let mut e = start;
                while e < src.len() && src[e] != '\n' {
                    e += 1;
                }
                if e > start {
                    push(t::SECTION, e);
                    return true;
                }
            }
        }
        _ => {}
    }

    false
}
