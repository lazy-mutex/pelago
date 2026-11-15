use super::tokens::t;
use crate::chars::{is_line_end, is_space, is_unicode_letter, is_word};

pub struct Tokens {
    pub types: Vec<u16>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum S {
    Initial = 0,
    String = 1,
    BqString = 2,
    Generic = 3,
    CallbackName = 4,
    CallbackArgs = 5,
    AccTitle = 6,
    AccDescr = 7,
    AccDescrMultiline = 8,
    Cls = 9,
    ClassBody = 10,
    Namespace = 11,
    NamespaceBody = 12,
}

// The token each state gives at the end of input, before the parser sees the end itself.
const AT_END: [u16; 13] = [
    t::EOF,
    t::STR,
    t::EOF,
    t::GENERICTYPE,
    t::CALLBACK_NAME,
    t::CALLBACK_ARGS,
    t::acc_title_value,
    t::acc_descr_value,
    t::acc_descr_multiline_value,
    t::EOF,
    t::EOF_IN_STRUCT,
    t::EOF,
    t::EOF_IN_STRUCT,
];

fn slice_str(src: &[char], s: usize, e: usize) -> String {
    if s >= e {
        String::new()
    } else {
        src[s..e].iter().collect()
    }
}

fn starts_with_at(src: &[char], word: &str, at: usize) -> bool {
    let bytes = word.as_bytes();
    if at + bytes.len() > src.len() {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if src[at + i] != b as char {
            return false;
        }
    }
    true
}

fn find_substr(src: &[char], word: &str, from: usize) -> Option<usize> {
    let wlen = word.len();
    if from + wlen > src.len() {
        return None;
    }
    (from..=src.len() - wlen).find(|&i| starts_with_at(src, word, i))
}

fn index_of(src: &[char], ch: char, from: usize) -> Option<usize> {
    if from >= src.len() {
        return None;
    }
    src[from..].iter().position(|&c| c == ch).map(|rel| from + rel)
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types: Vec<u16> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut stack: Vec<S> = Vec::new();
    let mut state = S::Initial;
    let mut p = 0usize;
    let mut dir_next = find_substr(src, "direction", 0);
    let mut line_end: isize = -1;
    let mut dir_failed = 0usize;

    let at = |idx: usize| -> Option<char> { src.get(idx).copied() };
    let is_word_at = |idx: usize| -> bool { src.get(idx).copied().is_some_and(is_word) };
    let kw = |word: &str, pos: usize| -> bool {
        starts_with_at(src, word, pos) && !is_word_at(pos + word.len())
    };
    let skip_ws = |mut q: usize| -> usize {
        while q < n && is_space(src[q]) {
            q += 1;
        }
        q
    };
    let until = |ch: char, from: usize| -> usize { index_of(src, ch, from).unwrap_or(n) };

    macro_rules! emit {
        ($tok:expr, $s:expr, $e:expr) => {{
            let s_val = $s;
            let e_val = $e;
            types.push($tok);
            texts.push(slice_str(src, s_val, e_val));
            starts.push(s_val);
            p = e_val;
            true
        }};
    }
    macro_rules! push_state {
        ($s:expr) => {{
            stack.push(state);
            state = $s;
        }};
    }
    macro_rules! pop_state {
        () => {{
            if let Some(prev) = stack.pop() {
                state = prev;
            }
        }};
    }
    // A run of whitespace is a NEWLINE up to its last line feed; the rest is skipped.
    macro_rules! space {
        ($leaves:expr) => {{
            let mut q = p;
            let mut last_lf: Option<usize> = None;
            while q < n && is_space(src[q]) {
                if src[q] == '\n' {
                    last_lf = Some(q);
                }
                q += 1;
            }
            if let Some(k) = last_lf {
                if $leaves {
                    pop_state!();
                }
                let _ = emit!(t::NEWLINE, p, k + 1);
            } else {
                p = q;
            }
        }};
    }
    // The keywords that every state recognizes, even inside generics and backtick names.
    macro_rules! keyword {
        ($c:expr) => {{
            match $c {
                'c' => {
                    if kw("cssClass", p) {
                        emit!(t::CSSCLASS, p, p + 8)
                    } else if kw("callback", p) {
                        emit!(t::CALLBACK, p, p + 8)
                    } else if kw("click", p) {
                        emit!(t::CLICK, p, p + 5)
                    } else {
                        false
                    }
                }
                'l' => {
                    if kw("link", p) {
                        emit!(t::LINK, p, p + 4)
                    } else {
                        false
                    }
                }
                'n' => {
                    if kw("note for", p) {
                        emit!(t::NOTE_FOR, p, p + 8)
                    } else if kw("note", p) {
                        emit!(t::NOTE, p, p + 4)
                    } else {
                        false
                    }
                }
                '<' => {
                    if at(p + 1) == Some('<') {
                        emit!(t::ANNOTATION_START, p, p + 2)
                    } else {
                        false
                    }
                }
                '>' => {
                    if at(p + 1) == Some('>') {
                        emit!(t::ANNOTATION_END, p, p + 2)
                    } else {
                        false
                    }
                }
                'h' => {
                    if kw("href", p) {
                        emit!(t::HREF, p, p + 4)
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }};
    }
    macro_rules! common {
        ($c:expr) => {{
            let ch = $c;
            if keyword!(ch) {
                true
            } else {
                let d = at(p + 1);
                let matched = match ch {
                    '"' => {
                        push_state!(S::String);
                        p += 1;
                        Some(true)
                    }
                    '~' => {
                        push_state!(S::Generic);
                        p += 1;
                        Some(true)
                    }
                    '`' => {
                        push_state!(S::BqString);
                        p += 1;
                        Some(true)
                    }
                    '_' => {
                        if kw("_self", p) {
                            Some(emit!(t::LINK_TARGET, p, p + 5))
                        } else if kw("_blank", p) {
                            Some(emit!(t::LINK_TARGET, p, p + 6))
                        } else if kw("_parent", p) {
                            Some(emit!(t::LINK_TARGET, p, p + 7))
                        } else if kw("_top", p) {
                            Some(emit!(t::LINK_TARGET, p, p + 4))
                        } else {
                            None
                        }
                    }
                    '<' => Some(if d == Some('|') {
                        emit!(t::EXTENSION, p, p + 2)
                    } else {
                        emit!(t::DEPENDENCY, p, p + 1)
                    }),
                    '|' => Some(d == Some('>') && emit!(t::EXTENSION, p, p + 2)),
                    '>' => Some(emit!(t::DEPENDENCY, p, p + 1)),
                    '*' => Some(emit!(t::COMPOSITION, p, p + 1)),
                    'o' => {
                        if !d.is_some_and(is_word) {
                            Some(emit!(t::AGGREGATION, p, p + 1))
                        } else {
                            None
                        }
                    }
                    '(' => Some(d == Some(')') && emit!(t::LOLLIPOP, p, p + 2)),
                    '-' => Some(if d == Some('-') {
                        emit!(t::LINE, p, p + 2)
                    } else {
                        emit!(t::MINUS, p, p + 1)
                    }),
                    '.' => Some(if d == Some('.') {
                        emit!(t::DOTTED_LINE, p, p + 2)
                    } else {
                        emit!(t::DOT, p, p + 1)
                    }),
                    ':' => {
                        let mut q = p + 1;
                        while q < n {
                            let qc = src[q];
                            if qc == ':' || qc == '\n' || qc == ';' {
                                break;
                            }
                            q += 1;
                        }
                        if q > p + 1 {
                            Some(emit!(t::LABEL, p, q))
                        } else {
                            Some(d == Some(':') && at(p + 2) == Some(':') && emit!(t::STYLE_SEPARATOR, p, p + 3))
                        }
                    }
                    '+' => Some(emit!(t::PLUS, p, p + 1)),
                    '%' => Some(emit!(t::PCT, p, p + 1)),
                    '=' => Some(emit!(t::EQUALS, p, p + 1)),
                    '[' => Some(emit!(t::SQS, p, p + 1)),
                    ']' => Some(emit!(t::SQE, p, p + 1)),
                    '!' | '#' | '$' | '&' | '\'' | ',' | '?' | '\\' | '/' => {
                        Some(emit!(t::PUNCTUATION, p, p + 1))
                    }
                    _ => None,
                };
                if let Some(res) = matched {
                    res
                } else if is_word(ch) {
                    let mut q = p + 1;
                    while is_word_at(q) {
                        q += 1;
                    }
                    emit!(t::ALPHA, p, q)
                } else {
                    is_unicode_letter(ch) && emit!(t::UNICODE_TEXT, p, p + 1)
                }
            }
        }};
    }

    loop {
        if p >= n {
            types.push(AT_END[state as usize]);
            texts.push(String::new());
            starts.push(n);
            break;
        }
        let c = src[p];
        let mut ok = true;
        match state {
            S::Initial => {
                // Direction check: sticky /.*direction\s+(TB|BT|RL|LR)[^\n]*/y
                let mut matched_dir = false;
                if dir_next.is_some() && p >= dir_failed {
                    if dir_next.unwrap() < p {
                        dir_next = find_substr(src, "direction", p);
                    }
                    if let Some(dn) = dir_next {
                        if line_end < p as isize {
                            let le = (p..n).find(|&q| is_line_end(src[q])).unwrap_or(n);
                            line_end = le as isize;
                        }
                        let le = line_end as usize;
                        if dn < le {
                            for (idx, code) in ["TB", "BT", "RL", "LR"].iter().enumerate() {
                                if le >= 9 && dn <= le - 9 {
                                    let mut d = le - 9;
                                    loop {
                                        if starts_with_at(src, "direction", d) {
                                            let w = skip_ws(d + 9);
                                            if w > d + 9 && starts_with_at(src, code, w) {
                                                let end = until('\n', w + 2);
                                                let _ = emit!(t::direction_tb + idx as u16, p, end);
                                                matched_dir = true;
                                                break;
                                            }
                                        }
                                        if d == dn {
                                            break;
                                        }
                                        d -= 1;
                                    }
                                }
                                if matched_dir {
                                    break;
                                }
                            }
                            if !matched_dir {
                                dir_failed = le;
                            }
                        }
                    }
                }
                if matched_dir {
                    // handled
                } else if is_space(c) {
                    space!(false);
                } else {
                    let mut handled = false;
                    match c {
                        '%' => {
                            if at(p + 1) == Some('%') {
                                let mut e = until('\n', p);
                                while e < n && (src[e] == '\n' || src[e] == '\r') {
                                    e += 1;
                                }
                                p = e;
                                handled = true;
                            }
                        }
                        'a' => {
                            let title = starts_with_at(src, "accTitle", p);
                            if title || starts_with_at(src, "accDescr", p) {
                                let q = skip_ws(p + 8);
                                let d = at(q);
                                if d == Some(':') {
                                    let _ = emit!(
                                        if title { t::acc_title } else { t::acc_descr },
                                        p,
                                        skip_ws(q + 1)
                                    );
                                    push_state!(if title { S::AccTitle } else { S::AccDescr });
                                    handled = true;
                                } else if d == Some('{') && !title {
                                    p = skip_ws(q + 1);
                                    push_state!(S::AccDescrMultiline);
                                    handled = true;
                                }
                            }
                        }
                        'c' => {
                            if kw("classDiagram-v2", p) {
                                let _ = emit!(t::CLASS_DIAGRAM, p, p + 15);
                                handled = true;
                            } else if kw("classDiagram", p) {
                                let _ = emit!(t::CLASS_DIAGRAM, p, p + 12);
                                handled = true;
                            } else if starts_with_at(src, "call", p) && at(p + 4).is_some_and(is_space) {
                                p = skip_ws(p + 4);
                                push_state!(S::CallbackName);
                                handled = true;
                            } else if kw("classDef", p) {
                                let _ = emit!(t::CLASSDEF, p, p + 8);
                                handled = true;
                            } else if kw("class", p) {
                                push_state!(S::Cls);
                                let _ = emit!(t::CLASS, p, p + 5);
                                handled = true;
                            }
                        }
                        '[' => {
                            if starts_with_at(src, "*]", p + 1) {
                                let _ = emit!(t::EDGE_STATE, p, p + 3);
                                handled = true;
                            }
                        }
                        's' => {
                            if kw("style", p) {
                                let _ = emit!(t::STYLE, p, p + 5);
                                handled = true;
                            }
                        }
                        'n' => {
                            if kw("namespace", p) {
                                push_state!(S::Namespace);
                                let _ = emit!(t::NAMESPACE, p, p + 9);
                                handled = true;
                            }
                        }
                        ',' => {
                            let _ = emit!(t::COMMA, p, p + 1);
                            handled = true;
                        }
                        '#' => {
                            let _ = emit!(t::BRKT, p, p + 1);
                            handled = true;
                        }
                        _ => {}
                    }
                    if !handled {
                        ok = common!(c) || (c == ':' && emit!(t::COLON, p, p + 1));
                    }
                }
            }

            S::Cls => {
                if is_space(c) {
                    space!(true);
                } else if c == '}' {
                    pop_state!();
                    pop_state!();
                    let _ = emit!(t::STRUCT_STOP, p, p + 1);
                } else if c == '{' {
                    push_state!(S::ClassBody);
                    let _ = emit!(t::STRUCT_START, p, p + 1);
                } else {
                    ok = common!(c);
                }
            }

            S::ClassBody => {
                if c == '"' {
                    push_state!(S::String);
                    p += 1;
                } else if c == '}' {
                    pop_state!();
                    let _ = emit!(t::STRUCT_STOP, p, p + 1);
                } else if c == '[' && starts_with_at(src, "*]", p + 1) {
                    let _ = emit!(t::EDGE_STATE, p, p + 3);
                } else if c == '{' {
                    let _ = emit!(t::OPEN_IN_STRUCT, p, p + 1);
                } else if c == '\n' {
                    p += 1;
                } else {
                    let mut q = p + 1;
                    while q < n {
                        let ch = src[q];
                        if ch == '{' || ch == '}' || ch == '\n' {
                            break;
                        }
                        q += 1;
                    }
                    let _ = emit!(t::MEMBER, p, q);
                }
            }

            S::Namespace => {
                if is_space(c) {
                    space!(true);
                } else if c == '{' {
                    push_state!(S::NamespaceBody);
                    let _ = emit!(t::STRUCT_START, p, p + 1);
                } else if c == '}' {
                    // The brace is read again in the state the namespace was opened from.
                    pop_state!();
                } else if c == 'n' && kw("namespace", p) {
                    push_state!(S::Namespace);
                    let _ = emit!(t::NAMESPACE, p, p + 9);
                } else {
                    ok = common!(c);
                }
            }

            S::NamespaceBody => {
                if is_space(c) {
                    space!(false);
                } else if c == '}' {
                    pop_state!();
                    let _ = emit!(t::STRUCT_STOP, p, p + 1);
                } else if c == 'n' && kw("namespace", p) {
                    push_state!(S::Namespace);
                    let _ = emit!(t::NAMESPACE, p, p + 9);
                } else if c == '[' && starts_with_at(src, "*]", p + 1) {
                    let _ = emit!(t::EDGE_STATE, p, p + 3);
                } else if c == 'c' && kw("class", p) {
                    push_state!(S::Cls);
                    let _ = emit!(t::CLASS, p, p + 5);
                } else {
                    ok = common!(c);
                }
            }

            S::String => {
                if c == '"' {
                    pop_state!();
                    p += 1;
                } else {
                    let _ = emit!(t::STR, p, until('"', p));
                }
            }

            S::Generic => {
                if c == '"' {
                    push_state!(S::String);
                    p += 1;
                } else if keyword!(c) {
                    // handled
                } else if c == '~' {
                    pop_state!();
                    p += 1;
                } else {
                    let _ = emit!(t::GENERICTYPE, p, until('~', p));
                }
            }

            S::BqString => {
                if c == '"' {
                    push_state!(S::String);
                    p += 1;
                } else if keyword!(c) {
                    // handled
                } else if c == '~' {
                    push_state!(S::Generic);
                    p += 1;
                } else if c == '`' {
                    pop_state!();
                    p += 1;
                } else {
                    let _ = emit!(t::BQUOTE_STR, p, until('`', p));
                }
            }

            S::CallbackName => {
                if c == '(' {
                    let q = skip_ws(p + 1);
                    pop_state!();
                    if at(q) == Some(')') {
                        p = q + 1;
                    } else {
                        push_state!(S::CallbackArgs);
                        p += 1;
                    }
                } else {
                    let _ = emit!(t::CALLBACK_NAME, p, until('(', p));
                }
            }

            S::CallbackArgs => {
                if c == ')' {
                    pop_state!();
                    p += 1;
                } else {
                    let _ = emit!(t::CALLBACK_ARGS, p, until(')', p));
                }
            }

            S::AccTitle | S::AccDescr => {
                let tok = if state == S::AccTitle {
                    t::acc_title_value
                } else {
                    t::acc_descr_value
                };
                let _ = emit!(tok, p, until('\n', p));
                pop_state!();
            }

            S::AccDescrMultiline => {
                if c == '}' {
                    pop_state!();
                    p += 1;
                } else {
                    let _ = emit!(t::acc_descr_multiline_value, p, until('}', p));
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

    types.push(t::END);
    texts.push(String::new());
    starts.push(n);
    Tokens { types, texts, starts }
}
