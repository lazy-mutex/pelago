use super::tokens::t;
use crate::chars::{is_line_end, is_space, is_word};
use crate::diagrams::common::caseless::word_at;

pub struct Tokens {
    pub types: Vec<u16>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
    // Set when lexing stopped on the grammar's own error rather than on unrecognized text.
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Struct,
    State,
    StateString,
    StateId,
    Scale,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    Style,
    StyleDefStyles,
    Note,
    NoteId,
    NoteText,
    FloatingNote,
    FloatingNoteId,
}

struct Finder<F> {
    from: usize,
    next: Option<usize>,
    init: bool,
    pred: F,
}

impl<F: Fn(usize) -> bool> Finder<F> {
    fn new(pred: F) -> Self {
        Self { from: 0, next: None, init: false, pred }
    }

    fn find(&mut self, q: usize, n: usize) -> Option<usize> {
        if !self.init || q < self.from || self.next.is_some_and(|nx| q > nx) {
            self.next = (q..n).find(|&i| (self.pred)(i));
            self.from = q;
            self.init = true;
        }
        self.next
    }
}

fn slice_str(src: &[char], s: usize, e: usize) -> String {
    if s >= e {
        String::new()
    } else {
        src[s..e].iter().collect()
    }
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn index_of(src: &[char], ch: char, from: usize) -> Option<usize> {
    if from >= src.len() {
        return None;
    }
    src[from..].iter().position(|&c| c == ch).map(|rel| from + rel)
}

fn is_stereotype_at(src: &[char], k: usize) -> bool {
    let n = src.len();
    if k + 8 > n {
        return false;
    }
    let (open, close) = match (src[k], src[k + 1]) {
        ('<', '<') => ('<', '>'),
        ('[', '[') => ('[', ']'),
        _ => return false,
    };
    let _ = open;
    if (word_at(src, k + 2, "fork") || word_at(src, k + 2, "join"))
        && src[k + 6] == close
        && src[k + 7] == close
    {
        return true;
    }
    k + 10 <= n && word_at(src, k + 2, "choice") && src[k + 8] == close && src[k + 9] == close
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types: Vec<u16> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut stack: Vec<S> = Vec::new();
    let mut state = S::Initial;
    let mut p = 0usize;

    let mut next_direction = Finder::new(|i| word_at(src, i, "direction"));
    let mut next_stereotype = Finder::new(|i| is_stereotype_at(src, i));
    let mut next_line_end = Finder::new(|i| is_line_end(src[i]));
    let mut next_percent = Finder::new(|i| i + 1 < n && src[i] == '%' && src[i + 1] == '%');
    let mut next_brace = Finder::new(|i| src[i] == '{');

    // The last `direction <dir>` of each kind on the line being lexed, and where its keyword ends.
    let mut dir_from: isize = -1;
    let mut dir_to: isize = -1;
    let mut dir_at: [isize; 4] = [-1; 4];
    let mut dir_end: [usize; 4] = [0; 4];

    // The same for <<fork>>, <<join>>, <<choice>> and their [[ ]] spellings.
    let mut stereo_from: isize = -1;
    let mut stereo_to: isize = -1;
    let mut stereo_at: [isize; 6] = [-1; 6];

    let at = |idx: usize| -> Option<char> { src.get(idx).copied() };
    let is_word_at = |idx: usize| -> bool { src.get(idx).copied().is_some_and(is_word) };
    let ws_end = |q: usize| -> usize {
        let mut j = q;
        while j < n && is_space(src[j]) {
            j += 1;
        }
        j
    };
    let word_end = |q: usize| -> usize {
        let mut j = q;
        while j < n && is_word(src[j]) {
            j += 1;
        }
        j
    };
    let eol = |q: usize| -> usize { index_of(src, '\n', q).unwrap_or(n) };

    macro_rules! emit {
        ($tok:expr, $s:expr, $e:expr) => {{
            let s_val = $s;
            let e_val = $e;
            types.push($tok);
            texts.push(slice_str(src, s_val, e_val));
            starts.push(s_val);
            p = e_val;
        }};
    }
    macro_rules! emit_text {
        ($tok:expr, $text:expr, $s:expr, $e:expr) => {{
            let s_val = $s;
            let e_val = $e;
            types.push($tok);
            texts.push($text);
            starts.push(s_val);
            p = e_val;
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
    macro_rules! finish {
        ($tok:expr, $err:expr) => {{
            types.push($tok);
            texts.push(String::new());
            starts.push(p);
            return Tokens { types, texts, starts, error: $err };
        }};
    }

    loop {
        if p >= n {
            // Jison tries the current state's rules once more on the empty remainder.
            match state {
                S::Initial => emit!(t::NL, n, n),
                S::AccTitle => emit!(t::acc_title_value, n, n),
                S::AccDescr => emit!(t::acc_descr_value, n, n),
                S::AccDescrMultiline => emit!(t::acc_descr_multiline_value, n, n),
                S::ClassDefId => emit!(t::CLASSDEF_STYLEOPTS, n, n),
                S::ClassStyle => emit!(t::STYLECLASS, n, n),
                S::StyleDefStyles => emit!(t::STYLEDEF_STYLEOPTS, n, n),
                S::StateString => emit!(t::STATE_DESCR, n, n),
                S::FloatingNote => emit!(t::NOTE_TEXT, n, n),
                S::StateId | S::FloatingNoteId => emit!(t::ID, n, n),
                _ => {}
            }
            finish!(t::END, None);
        }

        let c = src[p];
        match state {
            S::Initial | S::Struct => {
                let root = state == S::Initial;
                let lc = c.to_ascii_lowercase();
                if root {
                    if lc == 'c' && word_at(src, p, "click") && !is_word_at(p + 5) {
                        emit!(t::CLICK, p, p + 5);
                        continue;
                    }
                    if lc == 'h' && word_at(src, p, "href") && !is_word_at(p + 4) {
                        emit!(t::HREF, p, p + 4);
                        continue;
                    }
                    if c == '"' {
                        if let Some(close) = index_of(src, '"', p + 1) {
                            emit!(t::STRING, p, close + 1);
                            continue;
                        }
                    }
                    if lc == 'd' && word_at(src, p, "default") && !is_word_at(p + 7) {
                        emit!(t::DEFAULT, p, p + 7);
                        continue;
                    }
                    {
                        if (p as isize) < dir_from || (p as isize) > dir_to {
                            let first = next_direction.find(p, n);
                            let end = if first.is_none() { n } else { next_line_end.find(p, n).unwrap_or(n) };
                            dir_from = p as isize;
                            dir_to = end as isize;
                            dir_at = [-1; 4];
                            let mut d_opt = first;
                            while let Some(d) = d_opt {
                                if d >= end {
                                    break;
                                }
                                let w = ws_end(d + 9);
                                if w > d + 9 {
                                    let kind = if word_at(src, w, "tb") {
                                        Some(0)
                                    } else if word_at(src, w, "bt") {
                                        Some(1)
                                    } else if word_at(src, w, "rl") {
                                        Some(2)
                                    } else if word_at(src, w, "lr") {
                                        Some(3)
                                    } else {
                                        None
                                    };
                                    if let Some(k) = kind {
                                        dir_at[k] = d as isize;
                                        dir_end[k] = w + 2;
                                    }
                                }
                                d_opt = next_direction.find(d + 1, n);
                            }
                        }
                        if let Some(kind) = (0..4).find(|&k| dir_at[k] >= p as isize) {
                            emit!(t::direction_tb + kind as u16, p, eol(dir_end[kind]));
                            continue;
                        }
                    }
                    if c == '\n' {
                        let mut j = p + 1;
                        while at(j) == Some('\n') {
                            j += 1;
                        }
                        emit!(t::NL, p, j);
                        continue;
                    }
                    if is_space(c) {
                        p = ws_end(p);
                        continue;
                    }
                } else if c != '\n' && is_space(c) {
                    let mut j = p + 1;
                    while j < n && src[j] != '\n' && is_space(src[j]) {
                        j += 1;
                    }
                    p = j;
                    continue;
                }

                if c == '#' || (c == '%' && at(p + 1) == Some('%') && at(p + 2) != Some('{')) {
                    p = eol(p);
                    continue;
                }

                if lc == 's' {
                    if root && word_at(src, p, "scale") {
                        let e = ws_end(p + 5);
                        if e > p + 5 {
                            emit!(t::scale, p, e);
                            push_state!(S::Scale);
                            continue;
                        }
                    }
                    if word_at(src, p, "style") {
                        let e = ws_end(p + 5);
                        if e > p + 5 {
                            emit!(t::style, p, e);
                            push_state!(S::Style);
                            continue;
                        }
                    }
                    if word_at(src, p, "state") {
                        let e = ws_end(p + 5);
                        if e > p + 5 {
                            p = e;
                            push_state!(S::State);
                            continue;
                        }
                        if root && word_at(src, p + 5, "diagram") {
                            let from = if word_at(src, p + 12, "-v2") { p + 15 } else { p + 12 };
                            let e = ws_end(from);
                            if e > from {
                                emit!(t::SD, p, e);
                                continue;
                            }
                        }
                    }
                } else if lc == 'c' {
                    if word_at(src, p, "class") {
                        let def = word_at(src, p + 5, "def");
                        let from = if def { p + 8 } else { p + 5 };
                        let e = ws_end(from);
                        if e > from {
                            emit!(if def { t::classDef } else { t::class }, p, e);
                            push_state!(if def { S::ClassDef } else { S::Class });
                            continue;
                        }
                    }
                } else if lc == 'a' && root {
                    if word_at(src, p, "acctitle") || word_at(src, p, "accdescr") {
                        let title = src[p + 3].to_ascii_lowercase() == 't';
                        let q = ws_end(p + 8);
                        let d = at(q);
                        if d == Some(':') {
                            emit!(if title { t::acc_title } else { t::acc_descr }, p, ws_end(q + 1));
                            push_state!(if title { S::AccTitle } else { S::AccDescr });
                            continue;
                        }
                        if d == Some('{') && !title {
                            p = ws_end(q + 1);
                            push_state!(S::AccDescrMultiline);
                            continue;
                        }
                    }
                }

                if root {
                    if c == '{' {
                        emit!(t::STRUCT_START, p, p + 1);
                        push_state!(S::Struct);
                        continue;
                    }
                } else {
                    if (p as isize) < dir_from || (p as isize) > dir_to {
                        let first = next_direction.find(p, n);
                        let end = if first.is_none() { n } else { next_line_end.find(p, n).unwrap_or(n) };
                        dir_from = p as isize;
                        dir_to = end as isize;
                        dir_at = [-1; 4];
                        let mut d_opt = first;
                        while let Some(d) = d_opt {
                            if d >= end {
                                break;
                            }
                            let w = ws_end(d + 9);
                            if w > d + 9 {
                                let kind = if word_at(src, w, "tb") {
                                    Some(0)
                                } else if word_at(src, w, "bt") {
                                    Some(1)
                                } else if word_at(src, w, "rl") {
                                    Some(2)
                                } else if word_at(src, w, "lr") {
                                    Some(3)
                                } else {
                                    None
                                };
                                if let Some(k) = kind {
                                    dir_at[k] = d as isize;
                                    dir_end[k] = w + 2;
                                }
                            }
                            d_opt = next_direction.find(d + 1, n);
                        }
                    }
                    if let Some(kind) = (0..4).find(|&k| dir_at[k] >= p as isize) {
                        emit!(t::direction_tb + kind as u16, p, eol(dir_end[kind]));
                        continue;
                    }
                    if c == '}' {
                        emit!(t::STRUCT_STOP, p, p + 1);
                        pop_state!();
                        continue;
                    }
                    if c == '\n' {
                        p += 1;
                        continue;
                    }
                }

                if lc == 'n' && word_at(src, p, "note") {
                    let e = ws_end(p + 4);
                    if e > p + 4 {
                        emit!(t::note, p, e);
                        push_state!(S::Note);
                        continue;
                    }
                }
                if root && lc == 'h' && word_at(src, p, "hide empty description") && !is_word_at(p + 22) {
                    emit!(t::HIDE_EMPTY, p, p + 22);
                    continue;
                }
                if c == '[' && at(p + 1) == Some('*') && at(p + 2) == Some(']') {
                    emit!(t::EDGE_STATE, p, p + 3);
                    continue;
                }

                let mut j = p;
                while j < n {
                    let d = src[j];
                    if d == ':' || d == '-' || d == '{' || is_space(d) {
                        break;
                    }
                    j += 1;
                }
                if j > p {
                    let pct = next_percent.find(p, n);
                    match pct {
                        None => emit!(t::ID, p, j),
                        Some(at_pct) if at_pct + 2 > j => emit!(t::ID, p, j),
                        Some(at_pct) if at_pct == p => p = j,
                        Some(at_pct) => emit!(t::ID, p, at_pct),
                    }
                    continue;
                }

                if c == ':' {
                    j = p + 1;
                    while j < n {
                        let d = src[j];
                        if d == '\n' || d == ';' {
                            break;
                        }
                        if d == ':' {
                            match at(j + 1) {
                                None | Some(':') | Some('\n') | Some(';') => break,
                                Some(_) => j += 1,
                            }
                        }
                        j += 1;
                    }
                    if j > p + 1 {
                        let text = js_trim(&slice_str(src, p, j)).to_string();
                        emit_text!(t::DESCR, text, p, j);
                        continue;
                    }
                    if at(p + 1) == Some(':') && at(p + 2) == Some(':') {
                        emit!(t::STYLE_SEPARATOR, p, p + 3);
                        continue;
                    }
                } else if c == '-' && at(p + 1) == Some('-') {
                    if at(p + 2) == Some('>') {
                        emit!(t::ARROW, p, p + 3);
                        continue;
                    }
                    if !root {
                        emit!(t::CONCURRENT, p, p + 2);
                        continue;
                    }
                }

                if !root {
                    finish!(t::ERROR, None);
                }
                emit!(t::INVALID, p, p + 1);
            }

            S::State => {
                if c != '\n' && is_space(c) {
                    let mut j = p + 1;
                    while j < n && src[j] != '\n' && is_space(src[j]) {
                        j += 1;
                    }
                    p = j;
                    continue;
                }
                if c == '#' || (c == '%' && at(p + 1) == Some('%') && at(p + 2) != Some('{')) {
                    p = eol(p);
                    continue;
                }

                if (p as isize) < stereo_from || (p as isize) > stereo_to {
                    let first = next_stereotype.find(p, n);
                    let end = if first.is_none() { n } else { next_line_end.find(p, n).unwrap_or(n) };
                    stereo_from = p as isize;
                    stereo_to = end as isize;
                    stereo_at = [-1; 6];
                    let mut d_opt = first;
                    while let Some(d) = d_opt {
                        if d >= end {
                            break;
                        }
                        let base = if src[d] == '<' { 0 } else { 3 };
                        let offset = match src[d + 2].to_ascii_lowercase() {
                            'f' => 0,
                            'j' => 1,
                            _ => 2,
                        };
                        stereo_at[base + offset] = d as isize;
                        d_opt = next_stereotype.find(d + 1, n);
                    }
                }
                if let Some(kind) = (0..6).find(|&k| stereo_at[k] >= p as isize) {
                    let at_pos = stereo_at[kind] as usize;
                    let choice = kind == 2 || kind == 5;
                    let tok = if choice {
                        t::CHOICE
                    } else if kind % 3 == 0 {
                        t::FORK
                    } else {
                        t::JOIN
                    };
                    let text = js_trim(&slice_str(src, p, at_pos)).to_string();
                    emit_text!(tok, text, p, at_pos + if choice { 10 } else { 8 });
                    pop_state!();
                    continue;
                }

                if c == '"' {
                    p += 1;
                    push_state!(S::StateString);
                    continue;
                }

                let q = if c == '\n' { ws_end(p) } else { p };
                if word_at(src, q, "as") {
                    let e = ws_end(q + 2);
                    if e > q + 2 {
                        emit!(t::AS, p, e);
                        push_state!(S::StateId);
                        continue;
                    }
                }

                if c == '\n' {
                    p += 1;
                    pop_state!();
                    continue;
                }
                if c == '{' {
                    emit!(t::STRUCT_START, p, p + 1);
                    pop_state!();
                    push_state!(S::Struct);
                    continue;
                }
                if is_word(c) {
                    let first = word_end(p);
                    let second = ws_end(first);
                    if second > first && is_word_at(second) {
                        if let Some(brace) = next_brace.find(second + 1, n) {
                            let le = next_line_end.find(second, n).unwrap_or(n);
                            if brace < le {
                                let err = format!(
                                    "Error: State name must be a single word. Found: \"{}\"",
                                    js_trim(&slice_str(src, p, brace + 1))
                                );
                                finish!(t::ERROR, Some(err));
                            }
                        }
                    }
                }

                let mut j = p + 1;
                while j < n {
                    let d = src[j];
                    if d == '{' || is_space(d) {
                        break;
                    }
                    j += 1;
                }
                emit!(t::COMPOSIT_STATE, p, j);
            }

            S::StateString => {
                if c == '"' {
                    p += 1;
                    pop_state!();
                    continue;
                }
                let close = index_of(src, '"', p).unwrap_or(n);
                emit!(t::STATE_DESCR, p, close);
            }

            S::StateId => {
                let mut j = p;
                while j < n {
                    let d = src[j];
                    if d == '\n' || d == '{' {
                        break;
                    }
                    j += 1;
                }
                let pct = next_percent.find(p, n);
                let emitted = match pct {
                    None => {
                        emit!(t::ID, p, j);
                        true
                    }
                    Some(at_pct) if at_pct + 2 > j => {
                        emit!(t::ID, p, j);
                        true
                    }
                    Some(at_pct) if at_pct == p => {
                        p = j;
                        false
                    }
                    Some(at_pct) => {
                        emit!(t::ID, p, at_pct);
                        true
                    }
                };
                if emitted {
                    pop_state!();
                }
            }

            S::Scale => {
                if c.is_ascii_digit() {
                    let mut j = p + 1;
                    while j < n && src[j].is_ascii_digit() {
                        j += 1;
                    }
                    emit!(t::WIDTH, p, j);
                    continue;
                }
                let q = ws_end(p);
                if q > p && word_at(src, q, "width") && !is_word_at(q + 5) {
                    p = q + 5;
                    pop_state!();
                    continue;
                }
                finish!(t::ERROR, None);
            }

            S::AccTitle => {
                emit!(t::acc_title_value, p, eol(p));
                pop_state!();
            }

            S::AccDescr => {
                emit!(t::acc_descr_value, p, eol(p));
                pop_state!();
            }

            S::AccDescrMultiline => {
                if c == '}' {
                    p += 1;
                    pop_state!();
                    continue;
                }
                let close = index_of(src, '}', p).unwrap_or(n);
                emit!(t::acc_descr_multiline_value, p, close);
            }

            S::ClassDef => {
                let is_default = word_at(src, p, "default") && ws_end(p + 7) > p + 7;
                let j = if is_default { p + 7 } else { word_end(p) };
                let e = ws_end(j);
                if j == p || e == j {
                    finish!(t::ERROR, None);
                }
                emit!(if is_default { t::DEFAULT_CLASSDEF_ID } else { t::CLASSDEF_ID }, p, e);
                pop_state!();
                push_state!(S::ClassDefId);
            }

            S::ClassDefId => {
                emit!(t::CLASSDEF_STYLEOPTS, p, eol(p));
                pop_state!();
            }

            S::Class => {
                let mut j = word_end(p);
                if j == p {
                    finish!(t::ERROR, None);
                }
                while at(j) == Some(',') {
                    let k = ws_end(j + 1);
                    let e = word_end(k);
                    if e == k {
                        break;
                    }
                    j = e;
                }
                emit!(t::CLASSENTITY_IDS, p, j);
                pop_state!();
                push_state!(S::ClassStyle);
            }

            S::ClassStyle => {
                emit!(t::STYLECLASS, p, eol(p));
                pop_state!();
            }

            S::Style => {
                let mut j = p;
                while j < n && (is_word(src[j]) || src[j] == ',') {
                    j += 1;
                }
                let e = ws_end(j);
                if j == p || e == j {
                    finish!(t::ERROR, None);
                }
                emit!(t::STYLE_IDS, p, e);
                pop_state!();
                push_state!(S::StyleDefStyles);
            }

            S::StyleDefStyles => {
                emit!(t::STYLEDEF_STYLEOPTS, p, eol(p));
                pop_state!();
            }

            S::Note => {
                if word_at(src, p, "left of") && !is_word_at(p + 7) {
                    emit!(t::left_of, p, p + 7);
                    pop_state!();
                    push_state!(S::NoteId);
                    continue;
                }
                if word_at(src, p, "right of") && !is_word_at(p + 8) {
                    emit!(t::right_of, p, p + 8);
                    pop_state!();
                    push_state!(S::NoteId);
                    continue;
                }
                if c == '"' {
                    p += 1;
                    pop_state!();
                    push_state!(S::FloatingNote);
                    continue;
                }
                finish!(t::ERROR, None);
            }

            S::NoteId => {
                let q = ws_end(p);
                let mut j = q;
                while j < n {
                    let d = src[j];
                    if d == ':' || d == '-' || is_space(d) {
                        break;
                    }
                    j += 1;
                }
                if j == q {
                    finish!(t::ERROR, None);
                }
                let pct = next_percent.find(p, n);
                let emitted = match pct {
                    None => {
                        emit!(t::ID, p, j);
                        true
                    }
                    Some(at_pct) if at_pct + 2 > j => {
                        emit!(t::ID, p, j);
                        true
                    }
                    Some(at_pct) if at_pct == p => {
                        p = j;
                        false
                    }
                    Some(at_pct) => {
                        emit!(t::ID, p, at_pct);
                        true
                    }
                };
                if emitted {
                    pop_state!();
                    push_state!(S::NoteText);
                }
            }

            S::NoteText => {
                let q = ws_end(p);
                if at(q) == Some(':') {
                    let mut j = q + 1;
                    while j < n {
                        let d = src[j];
                        if d == ':' || d == '\n' || d == ';' {
                            break;
                        }
                        j += 1;
                    }
                    if j > q + 1 {
                        // Mermaid drops two characters here, expecting " :"; with no space before the colon
                        // the first character of the note goes too.
                        let text = js_trim(&slice_str(src, (p + 2).min(j), j)).to_string();
                        emit_text!(t::NOTE_TEXT, text, p, j);
                        pop_state!();
                        continue;
                    }
                }
                let mut k_opt = index_of(src, '\n', p);
                let mut found_end: Option<usize> = None;
                while let Some(k) = k_opt {
                    let e = ws_end(k);
                    if word_at(src, e, "end note") && !is_word_at(e + 8) {
                        found_end = Some(e);
                        break;
                    }
                    k_opt = index_of(src, '\n', e);
                }
                let Some(e) = found_end else {
                    finish!(t::ERROR, None);
                };
                let text = js_trim(&slice_str(src, p, e)).to_string();
                emit_text!(t::NOTE_TEXT, text, p, e + 8);
                pop_state!();
            }

            S::FloatingNote => {
                let q = ws_end(p);
                if word_at(src, q, "as") {
                    emit!(t::AS, p, ws_end(q + 2));
                    pop_state!();
                    push_state!(S::FloatingNoteId);
                    continue;
                }
                if c == '"' {
                    p += 1;
                    continue;
                }
                let close = index_of(src, '"', p).unwrap_or(n);
                emit!(t::NOTE_TEXT, p, close);
            }

            S::FloatingNoteId => {
                let j = eol(p);
                let pct = next_percent.find(p, n);
                let emitted = match pct {
                    None => {
                        emit!(t::ID, p, j);
                        true
                    }
                    Some(at_pct) if at_pct + 2 > j => {
                        emit!(t::ID, p, j);
                        true
                    }
                    Some(at_pct) if at_pct == p => {
                        p = j;
                        false
                    }
                    Some(at_pct) => {
                        emit!(t::ID, p, at_pct);
                        true
                    }
                };
                if emitted {
                    pop_state!();
                }
            }
        }
    }
}
