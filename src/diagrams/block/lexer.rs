use crate::chars::{is_space, is_word};

use super::tokens::t;

pub struct Tokens {
    pub types: Vec<usize>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    String,
    MdString,
    Node,
    BlockArrow,
    ArrowDir,
    Llabel,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    StyleStmnt,
    StyleDefinition,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
}

const DIRECTIONS: &[&str] = &["right", "left", "x", "y", "up", "down"];

// Characters that end a node id for ASCII < 128: ( ) [ { } < > - : = and whitespace.
fn is_id_stop(c: char) -> bool {
    let code = c as u32;
    if code < 128 {
        matches!(
            code as u8,
            9 | 10 | 11 | 12 | 13 | 32 | 40 | 41 | 45 | 58 | 60 | 61 | 62 | 91 | 123 | 125
        )
    } else {
        is_space(c)
    }
}

fn starts_with_ascii(src: &[char], pos: usize, pat: &str) -> bool {
    if pos + pat.len() > src.len() {
        return false;
    }
    for (i, b) in pat.bytes().enumerate() {
        if src[pos + i] != b as char {
            return false;
        }
    }
    true
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types = Vec::new();
    let mut texts = Vec::new();
    let mut starts = Vec::new();
    let mut stack: Vec<S> = Vec::new();
    let mut state = S::Initial;
    let mut p = 0usize;

    let ws = |mut q: usize| -> usize {
        while q < n && is_space(src[q]) {
            q += 1;
        }
        q
    };

    let digits = |mut q: usize| -> usize {
        while q < n && src[q].is_ascii_digit() {
            q += 1;
        }
        q
    };

    let word = |mut q: usize| -> usize {
        while q < n && is_word(src[q]) {
            q += 1;
        }
        q
    };

    let keyword = |pos: usize, kw: &str| -> bool {
        starts_with_ascii(src, pos, kw)
            && !src.get(pos + kw.len()).copied().is_some_and(is_word)
    };

    // End of `.+-[xo>]?` at q, or None.
    let dotted = |mut q: usize| -> Option<usize> {
        if src.get(q) != Some(&'.') {
            return None;
        }
        q += 1;
        while src.get(q) == Some(&'.') {
            q += 1;
        }
        if src.get(q) != Some(&'-') {
            return None;
        }
        match src.get(q + 1).copied() {
            Some('x' | 'o' | '>') => Some(q + 2),
            _ => Some(q + 1),
        }
    };

    // End of a complete link at q, before its trailing whitespace, or None.
    let link_end = |mut q: usize| -> Option<usize> {
        if matches!(src.get(q).copied(), Some('x' | 'o' | '<')) {
            q += 1;
        }
        let line = src.get(q).copied()?;
        if line == '.' {
            return dotted(q);
        }
        if line != '-' && line != '=' {
            return None;
        }
        let mut k = q + 1;
        while src.get(k) == Some(&line) {
            k += 1;
        }
        if k - q == 1 {
            return if line == '-' { dotted(k) } else { None };
        }
        if matches!(src.get(k).copied(), Some('x' | 'o' | '>')) {
            return Some(k + 1);
        }
        if k - q >= 3 { Some(k) } else { None }
    };

    let up_to = |ch: char, from: usize| -> usize {
        let mut q = from;
        while q < n {
            if src[q] == ch {
                return q;
            }
            q += 1;
        }
        n
    };

    let slice_str = |s: usize, e: usize| -> String { src[s..e].iter().collect() };

    macro_rules! put {
        ($tok:expr, $txt:expr, $end:expr) => {{
            types.push($tok);
            texts.push($txt);
            starts.push(p);
            p = $end;
        }};
    }

    macro_rules! emit {
        ($tok:expr, $end:expr) => {{
            let end_val = $end;
            let s = slice_str(p, end_val);
            put!($tok, s, end_val);
        }};
    }

    macro_rules! push_state {
        ($next:expr) => {{
            stack.push(state);
            state = $next;
        }};
    }

    macro_rules! pop_state {
        () => {{
            if let Some(prev) = stack.pop() {
                state = prev;
            }
        }};
    }

    loop {
        if p >= n {
            let eof_tok = match state {
                S::Initial => Some(t::EOF),
                S::String => Some(t::STR),
                S::ClassDefId => Some(t::CLASSDEF_STYLEOPTS),
                S::ClassStyle => Some(t::STYLECLASS),
                S::StyleDefinition => Some(t::STYLE_DEFINITION_DATA),
                S::AccTitle => Some(t::acc_title_value),
                S::AccDescr => Some(t::acc_descr_value),
                S::AccDescrMultiline => Some(t::acc_descr_multiline_value),
                _ => None,
            };
            p = n;
            if let Some(tok) = eof_tok {
                types.push(tok);
                texts.push(String::new());
                starts.push(p);
            }
            break;
        }

        let c = src[p];
        let mut ok = true;

        match state {
            S::Initial => {
                if is_space(c) {
                    p = ws(p + 1);
                } else {
                    let mut handled = false;
                    match c {
                        'b' => {
                            if starts_with_ascii(src, p, "block") {
                                if keyword(p, "block-beta") {
                                    emit!(t::KEY, p + 10);
                                    handled = true;
                                } else if src.get(p + 5) == Some(&':') {
                                    emit!(t::ID_BLOCK, p + 6);
                                    handled = true;
                                } else if !src.get(p + 5).copied().is_some_and(is_word) {
                                    emit!(t::KEY, p + 5);
                                    handled = true;
                                }
                            }
                        }
                        'c' => {
                            if starts_with_ascii(src, p, "columns") {
                                let q = ws(p + 7);
                                if q > p + 7 {
                                    if starts_with_ascii(src, q, "auto")
                                        && !src.get(q + 4).copied().is_some_and(is_word)
                                    {
                                        put!(t::COLUMNS, "-1".to_string(), q + 4);
                                        handled = true;
                                    } else {
                                        let e = digits(q);
                                        if e > q {
                                            put!(t::COLUMNS, slice_str(q, e), e);
                                            handled = true;
                                        }
                                    }
                                }
                            } else if starts_with_ascii(src, p, "class") {
                                let def = starts_with_ascii(src, p + 5, "Def");
                                let from = if def { p + 8 } else { p + 5 };
                                let q = ws(from);
                                if q > from {
                                    push_state!(if def { S::ClassDef } else { S::Class });
                                    emit!(if def { t::CLASSDEF } else { t::CLASS }, q);
                                    handled = true;
                                }
                            }
                        }
                        '"' => {
                            if src.get(p + 1) == Some(&'`') {
                                push_state!(S::MdString);
                                p += 2;
                            } else {
                                push_state!(S::String);
                                p += 1;
                            }
                            handled = true;
                        }
                        's' => {
                            if starts_with_ascii(src, p, "space") {
                                if src.get(p + 5) == Some(&':') {
                                    let e = digits(p + 6);
                                    if e > p + 6 {
                                        put!(t::SPACE_BLOCK, slice_str(p + 6, e), e);
                                        handled = true;
                                    }
                                }
                                if !handled && !src.get(p + 5).copied().is_some_and(is_word) {
                                    put!(t::SPACE_BLOCK, "1".to_string(), p + 5);
                                    handled = true;
                                }
                            } else if starts_with_ascii(src, p, "style") {
                                let q = ws(p + 5);
                                if q > p + 5 {
                                    push_state!(S::StyleStmnt);
                                    emit!(t::STYLE, q);
                                    handled = true;
                                }
                            }
                        }
                        'd' => {
                            if keyword(p, "default") {
                                emit!(t::DEFAULT, p + 7);
                                handled = true;
                            }
                        }
                        'l' => {
                            if keyword(p, "linkStyle") {
                                emit!(t::LINKSTYLE, p + 9);
                                handled = true;
                            }
                        }
                        'i' => {
                            if keyword(p, "interpolate") {
                                emit!(t::INTERPOLATE, p + 11);
                                handled = true;
                            }
                        }
                        'a' => {
                            if starts_with_ascii(src, p, "accTitle") {
                                let q = ws(p + 8);
                                if src.get(q) == Some(&':') {
                                    push_state!(S::AccTitle);
                                    emit!(t::acc_title, ws(q + 1));
                                    handled = true;
                                }
                            } else if starts_with_ascii(src, p, "accDescr") {
                                let q = ws(p + 8);
                                let d = src.get(q).copied();
                                if d == Some(':') {
                                    push_state!(S::AccDescr);
                                    emit!(t::acc_descr, ws(q + 1));
                                    handled = true;
                                } else if d == Some('{') {
                                    push_state!(S::AccDescrMultiline);
                                    p = ws(q + 1);
                                    handled = true;
                                }
                            }
                        }
                        'e' => {
                            if keyword(p, "end") {
                                emit!(t::end, ws(p + 3));
                                handled = true;
                            }
                        }
                        '-' => {
                            if src.get(p + 1) == Some(&')') {
                                push_state!(S::Node);
                                emit!(t::NODE_DSTART, p + 2);
                            } else if let Some(e) = link_end(p) {
                                emit!(t::LINK, ws(e));
                            } else {
                                let q = p;
                                let a = src.get(q).copied();
                                let b = src.get(q + 1).copied();
                                if (a == Some('-') && matches!(b, Some('-' | '.')))
                                    || (a == Some('=') && b == Some('='))
                                {
                                    push_state!(S::Llabel);
                                    emit!(t::START_LINK, ws(q + 2));
                                } else {
                                    ok = false;
                                }
                            }
                            handled = true;
                        }
                        '=' => {
                            if let Some(e) = link_end(p) {
                                emit!(t::LINK, ws(e));
                            } else if src.get(p + 1) == Some(&'=') {
                                push_state!(S::Llabel);
                                emit!(t::START_LINK, ws(p + 2));
                            } else {
                                ok = false;
                            }
                            handled = true;
                        }
                        '<' => {
                            if src.get(p + 1) == Some(&'[') {
                                push_state!(S::BlockArrow);
                                emit!(t::BLOCK_ARROW_START, p + 2);
                            } else if let Some(e) = link_end(p) {
                                emit!(t::LINK, ws(e));
                            } else {
                                let q = p + 1;
                                let a = src.get(q).copied();
                                let b = src.get(q + 1).copied();
                                if (a == Some('-') && matches!(b, Some('-' | '.')))
                                    || (a == Some('=') && b == Some('='))
                                {
                                    push_state!(S::Llabel);
                                    emit!(t::START_LINK, ws(q + 2));
                                } else {
                                    ok = false;
                                }
                            }
                            handled = true;
                        }
                        '(' => {
                            let d = src.get(p + 1).copied();
                            let len = if matches!(d, Some('-' | '[')) {
                                2
                            } else if d == Some('(') {
                                if src.get(p + 2) == Some(&'(') { 3 } else { 2 }
                            } else {
                                1
                            };
                            push_state!(S::Node);
                            emit!(t::NODE_DSTART, p + len);
                            handled = true;
                        }
                        ')' | '{' => {
                            let len = if src.get(p + 1) == Some(&c) { 2 } else { 1 };
                            push_state!(S::Node);
                            emit!(t::NODE_DSTART, p + len);
                            handled = true;
                        }
                        '>' => {
                            push_state!(S::Node);
                            emit!(t::NODE_DSTART, p + 1);
                            handled = true;
                        }
                        '[' => {
                            let d = src.get(p + 1).copied();
                            let len = if matches!(d, Some('[' | '|' | '(' | '\\' | '/')) {
                                2
                            } else {
                                1
                            };
                            push_state!(S::Node);
                            emit!(t::NODE_DSTART, p + len);
                            handled = true;
                        }
                        ':' => {
                            let e = digits(p + 1);
                            if e > p + 1 {
                                put!(t::SIZE, slice_str(p + 1, e), e);
                            } else {
                                ok = false;
                            }
                            handled = true;
                        }
                        _ => {}
                    }
                    if !handled {
                        let mut q = p;
                        while q < n && !is_id_stop(src[q]) {
                            q += 1;
                        }
                        if q > p {
                            emit!(t::NODE_ID, q);
                        } else {
                            ok = false;
                        }
                    }
                }
            }

            S::String => {
                if c == '"' {
                    pop_state!();
                    p += 1;
                } else {
                    emit!(t::STR, up_to('"', p));
                }
            }

            S::MdString => {
                if c == '`' {
                    ok = src.get(p + 1) == Some(&'"');
                    if ok {
                        pop_state!();
                        p += 2;
                    }
                } else if c == '"' {
                    ok = false;
                } else {
                    let mut q = p + 1;
                    while q < n && !matches!(src[q], '`' | '"') {
                        q += 1;
                    }
                    emit!(t::MD_STR, q);
                }
            }

            S::Node => {
                let d = src.get(p + 1).copied();
                if c == '"' {
                    if d == Some('`') {
                        push_state!(S::MdString);
                        p += 2;
                    } else {
                        push_state!(S::String);
                        p += 1;
                    }
                } else {
                    let len = match c {
                        '(' => {
                            if d == Some('-') {
                                2
                            } else if d == Some('(') {
                                if src.get(p + 2) == Some(&'(') { 3 } else { 2 }
                            } else {
                                1
                            }
                        }
                        ')' => {
                            if d == Some(')') {
                                if src.get(p + 2) == Some(&')') { 3 } else { 2 }
                            } else if d == Some(']') {
                                2
                            } else {
                                1
                            }
                        }
                        '}' => {
                            if d == Some('}') { 2 } else { 1 }
                        }
                        '-' => {
                            if d == Some(')') { 2 } else { 0 }
                        }
                        ']' => {
                            if matches!(d, Some(']' | ')' | '>')) { 2 } else { 1 }
                        }
                        '\\' | '/' => {
                            if d == Some(']') { 2 } else { 0 }
                        }
                        _ => 0,
                    };
                    if len == 0 {
                        ok = false;
                    } else {
                        pop_state!();
                        emit!(t::NODE_DEND, p + len);
                    }
                }
            }

            S::BlockArrow => {
                if c == '"' {
                    if src.get(p + 1) == Some(&'`') {
                        push_state!(S::MdString);
                        p += 2;
                    } else {
                        push_state!(S::String);
                        p += 1;
                    }
                } else if c == ']' && src.get(p + 1) == Some(&'>') {
                    let q = ws(p + 2);
                    ok = src.get(q) == Some(&'(');
                    if ok {
                        push_state!(S::ArrowDir);
                        p = q + 1;
                    }
                } else {
                    ok = false;
                }
            }

            S::ArrowDir => {
                if c == ')' {
                    pop_state!();
                    pop_state!();
                    put!(t::BLOCK_ARROW_END, "]>".to_string(), ws(p + 1));
                } else {
                    let from = ws(if c == ',' { p + 1 } else { p });
                    let mut matched = false;
                    for dir in DIRECTIONS {
                        if starts_with_ascii(src, from, dir) {
                            let e = ws(from + dir.len());
                            let s_from = if c == ',' { from } else { p };
                            put!(t::DIR, slice_str(s_from, e), e);
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        ok = false;
                    }
                }
            }

            S::Llabel => {
                if c == '"' {
                    if src.get(p + 1) == Some(&'`') {
                        push_state!(S::MdString);
                        p += 2;
                    } else {
                        emit!(t::LINK_LABEL, p + 1);
                        push_state!(S::String);
                    }
                } else if let Some(e) = link_end(ws(p)) {
                    pop_state!();
                    emit!(t::LINK, ws(e));
                } else {
                    ok = false;
                }
            }

            S::ClassDef => {
                let mut tok = t::DEFAULT_CLASSDEF_ID;
                let mut k = if starts_with_ascii(src, p, "DEFAULT") { p + 7 } else { p };
                let mut q = ws(k);
                if k == p || q == k {
                    tok = t::CLASSDEF_ID;
                    k = word(p);
                    q = ws(k);
                }
                ok = k > p && q > k;
                if ok {
                    state = S::ClassDefId;
                    emit!(tok, q);
                }
            }

            S::ClassDefId => {
                let e = up_to('\n', p);
                pop_state!();
                emit!(t::CLASSDEF_STYLEOPTS, e);
            }

            S::Class => {
                let mut k = word(p);
                if k == p {
                    ok = false;
                } else {
                    while src.get(k) == Some(&',') {
                        let from = ws(k + 1);
                        let to = word(from);
                        if to == from {
                            break;
                        }
                        k = to;
                    }
                    state = S::ClassStyle;
                    emit!(t::CLASSENTITY_IDS, k);
                }
            }

            S::ClassStyle => {
                let e = up_to('\n', p);
                pop_state!();
                emit!(t::STYLECLASS, e);
            }

            S::StyleStmnt => {
                let mut k = word(p);
                if k == p {
                    ok = false;
                } else {
                    while src.get(k) == Some(&',') {
                        let from = ws(k + 1);
                        let to = word(from);
                        if to == from {
                            break;
                        }
                        k = to;
                    }
                    state = S::StyleDefinition;
                    emit!(t::STYLE_ENTITY_IDS, k);
                }
            }

            S::StyleDefinition => {
                let e = up_to('\n', p);
                pop_state!();
                emit!(t::STYLE_DEFINITION_DATA, e);
            }

            S::AccTitle => {
                let e = up_to('\n', p);
                pop_state!();
                emit!(t::acc_title_value, e);
            }

            S::AccDescr => {
                let e = up_to('\n', p);
                pop_state!();
                emit!(t::acc_descr_value, e);
            }

            S::AccDescrMultiline => {
                if c == '}' {
                    pop_state!();
                    p += 1;
                } else {
                    emit!(t::acc_descr_multiline_value, up_to('}', p));
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
