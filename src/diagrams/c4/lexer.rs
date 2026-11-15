use crate::chars::{is_line_end, is_space, is_word};

use super::tokens::t;

pub struct Tokens {
    pub types: Vec<usize>,
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Struct,
    Attribute,
    String,
    StringKv,
    StringKvKey,
    StringKvValue,
    Acc,
    AccDescrMultiline,
}

fn keyword_token(word: &str) -> Option<usize> {
    Some(match word {
        "C4Context" => t::C4_CONTEXT,
        "C4Container" => t::C4_CONTAINER,
        "C4Component" => t::C4_COMPONENT,
        "C4Dynamic" => t::C4_DYNAMIC,
        "C4Deployment" => t::C4_DEPLOYMENT,
        "Person" => t::PERSON,
        "Person_Ext" => t::PERSON_EXT,
        "System" => t::SYSTEM,
        "SystemDb" => t::SYSTEM_DB,
        "SystemQueue" => t::SYSTEM_QUEUE,
        "System_Ext" => t::SYSTEM_EXT,
        "SystemDb_Ext" => t::SYSTEM_EXT_DB,
        "SystemQueue_Ext" => t::SYSTEM_EXT_QUEUE,
        "Container" => t::CONTAINER,
        "ContainerDb" => t::CONTAINER_DB,
        "ContainerQueue" => t::CONTAINER_QUEUE,
        "Container_Ext" => t::CONTAINER_EXT,
        "ContainerDb_Ext" => t::CONTAINER_EXT_DB,
        "ContainerQueue_Ext" => t::CONTAINER_EXT_QUEUE,
        "Component" => t::COMPONENT,
        "ComponentDb" => t::COMPONENT_DB,
        "ComponentQueue" => t::COMPONENT_QUEUE,
        "Component_Ext" => t::COMPONENT_EXT,
        "ComponentDb_Ext" => t::COMPONENT_EXT_DB,
        "ComponentQueue_Ext" => t::COMPONENT_EXT_QUEUE,
        "Enterprise_Boundary" => t::ENTERPRISE_BOUNDARY,
        "System_Boundary" => t::SYSTEM_BOUNDARY,
        "Boundary" => t::BOUNDARY,
        "Container_Boundary" => t::CONTAINER_BOUNDARY,
        "Deployment_Node" | "Node" => t::NODE,
        "Node_L" => t::NODE_L,
        "Node_R" => t::NODE_R,
        "Rel" => t::REL,
        "BiRel" => t::BIREL,
        "Rel_Up" | "Rel_U" => t::REL_U,
        "Rel_Down" | "Rel_D" => t::REL_D,
        "Rel_Left" | "Rel_L" => t::REL_L,
        "Rel_Right" | "Rel_R" => t::REL_R,
        "Rel_Back" => t::REL_B,
        "RelIndex" => t::REL_INDEX,
        "UpdateElementStyle" => t::UPDATE_EL_STYLE,
        "UpdateRelStyle" => t::UPDATE_REL_STYLE,
        "UpdateLayoutConfig" => t::UPDATE_LAYOUT_CONFIG,
        _ => return None,
    })
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

fn find_ascii(src: &[char], pat: &str, from: usize) -> Option<usize> {
    if pat.is_empty() {
        return Some(from.min(src.len()));
    }
    let len = pat.len();
    if from + len > src.len() {
        return None;
    }
    let first = pat.as_bytes()[0] as char;
    let last_start = src.len() - len;
    let mut i = from;
    while i <= last_start {
        if src[i] == first && starts_with_ascii(src, i, pat) {
            return Some(i);
        }
        i += 1;
    }
    None
}

// `/.*direction\s+XX[^\n]*/y` starting at `p`, where `line_end` is the first line terminator >= `p` (or `src.len()`).
fn match_direction(src: &[char], p: usize, line_end: usize, code: [char; 2]) -> Option<usize> {
    if line_end < p + 9 {
        return None;
    }
    let n = src.len();
    let mut k = line_end - 9;
    loop {
        if starts_with_ascii(src, k, "direction") {
            let mut q = k + 9;
            while q < n && is_space(src[q]) {
                q += 1;
            }
            if q > k + 9 && q + 1 < n && src[q] == code[0] && src[q + 1] == code[1] {
                let mut end = q + 2;
                while end < n && src[end] != '\n' {
                    end += 1;
                }
                return Some(end);
            }
        }
        if k == p {
            break;
        }
        k -= 1;
    }
    None
}

// `/title[^\S\r\n]*(?=[\r\n]|$)\s*/y`
fn match_bare_title(src: &[char], p: usize) -> Option<usize> {
    if !starts_with_ascii(src, p, "title") {
        return None;
    }
    let n = src.len();
    let mut q = p + 5;
    while q < n && is_space(src[q]) && src[q] != '\r' && src[q] != '\n' {
        q += 1;
    }
    if q < n && src[q] != '\r' && src[q] != '\n' {
        return None;
    }
    while q < n && is_space(src[q]) {
        q += 1;
    }
    Some(q)
}

// Follows the lexer Jison compiles from c4Diagram.jison: its rule order, its start conditions,
// and what it returns once the input runs out in each of them.
pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types = Vec::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut state = S::Initial;
    let mut acc_value = t::acc_title_value;
    let mut p = 0usize;
    let mut done = false;
    // Where the next `direction` is; None means none left, Some(usize::MAX) means not searched yet.
    let mut dir_next: Option<usize> = Some(usize::MAX);
    let mut line_end: Option<usize> = None;
    let mut dir_failed = 0usize;

    let skip_ws = |mut q: usize| -> usize {
        while q < n && is_space(src[q]) {
            q += 1;
        }
        q
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

    const DIRS: [[char; 2]; 4] = [['T', 'B'], ['B', 'T'], ['R', 'L'], ['L', 'R']];

    loop {
        if done {
            types.push(t::END);
            starts.push(n);
            ends.push(n);
            break;
        }
        if p >= n {
            done = true;
        }
        let c = src.get(p).copied();

        if state == S::Initial {
            let Some(ch) = c else {
                types.push(t::EOF);
                starts.push(n);
                ends.push(n);
                p = n;
                continue;
            };
            if dir_next.is_some() && p >= dir_failed {
                if dir_next == Some(usize::MAX) || dir_next.is_some_and(|d| d < p) {
                    dir_next = find_ascii(src, "direction", p);
                }
                if let Some(d_pos) = dir_next {
                    let le = match line_end {
                        Some(le) if le >= p => le,
                        _ => {
                            let mut k = p;
                            while k < n && !is_line_end(src[k]) {
                                k += 1;
                            }
                            line_end = Some(k);
                            k
                        }
                    };
                    if d_pos < le {
                        let mut matched = false;
                        for (idx, code) in DIRS.iter().enumerate() {
                            if let Some(end) = match_direction(src, p, le, *code) {
                                types.push(t::direction_tb + idx);
                                starts.push(p);
                                ends.push(end);
                                p = end;
                                matched = true;
                                break;
                            }
                        }
                        if matched {
                            continue;
                        }
                        dir_failed = le;
                    }
                }
            }
            if ch == 't' {
                if let Some(end) = match_bare_title(src, p) {
                    p = end;
                    continue;
                }
                // `title\s[^#\n;]+`
                let q0 = p + 5;
                if starts_with_ascii(src, p, "title") && src.get(q0).copied().is_some_and(is_space) {
                    let from = q0 + 1;
                    let mut q = from;
                    while q < n && !matches!(src[q], '#' | '\n' | ';') {
                        q += 1;
                    }
                    if q > from {
                        types.push(t::title);
                        starts.push(p);
                        ends.push(q);
                        p = q;
                        continue;
                    }
                }
            }
            if ch == 'a' {
                let q0 = p + 14;
                if starts_with_ascii(src, p, "accDescription") && src.get(q0).copied().is_some_and(is_space) {
                    let from = q0 + 1;
                    let mut q = from;
                    while q < n && !matches!(src[q], '#' | '\n' | ';') {
                        q += 1;
                    }
                    if q > from {
                        types.push(t::accDescription);
                        starts.push(p);
                        ends.push(q);
                        p = q;
                        continue;
                    }
                }
                let is_title = starts_with_ascii(src, p, "accTitle");
                if is_title || starts_with_ascii(src, p, "accDescr") {
                    let q = skip_ws(p + 8);
                    let d = src.get(q).copied();
                    if d == Some(':') {
                        let end = skip_ws(q + 1);
                        types.push(if is_title { t::acc_title } else { t::acc_descr });
                        starts.push(p);
                        ends.push(end);
                        p = end;
                        acc_value = if is_title { t::acc_title_value } else { t::acc_descr_value };
                        state = S::Acc;
                        continue;
                    }
                    if d == Some('{') && !is_title {
                        p = skip_ws(q + 1);
                        state = S::AccDescrMultiline;
                        continue;
                    }
                }
            }
            if ch == '%' && src.get(p + 1) == Some(&'%') {
                let mut q = up_to('\n', p + 2);
                while q < n && matches!(src[q], '\n' | '\r') {
                    q += 1;
                }
                p = q;
                continue;
            }
            if is_space(ch) {
                let mut q = p;
                let mut last: Option<usize> = None;
                while q < n && is_space(src[q]) {
                    if src[q] == '\n' {
                        last = Some(q);
                    }
                    q += 1;
                }
                if let Some(l) = last {
                    types.push(t::NEWLINE);
                    starts.push(p);
                    ends.push(l + 1);
                    p = l + 1;
                } else {
                    p = q;
                }
                continue;
            }
            if ch == '{' {
                types.push(t::LBRACE);
                starts.push(p);
                ends.push(p + 1);
                p += 1;
                continue;
            }
            if ch == '}' {
                types.push(t::RBRACE);
                starts.push(p);
                ends.push(p + 1);
                p += 1;
                continue;
            }
            if is_word(ch) {
                let mut q = p + 1;
                while q < n && is_word(src[q]) {
                    q += 1;
                }
                if q - p <= 19 {
                    let word: String = src[p..q].iter().collect();
                    if let Some(tok) = keyword_token(&word) {
                        if tok > t::C4_DEPLOYMENT {
                            state = S::Struct;
                        }
                        types.push(tok);
                        starts.push(p);
                        ends.push(q);
                        p = q;
                        continue;
                    }
                }
            }
            types.push(t::ERROR);
            starts.push(p);
            ends.push(p);
            break;
        }

        if state == S::Struct {
            if p >= n {
                types.push(t::EOF_IN_STRUCT);
                starts.push(n);
                ends.push(n);
                p = n;
            } else if c == Some('(') {
                let mut q = p + 1;
                while src.get(q) == Some(&' ') {
                    q += 1;
                }
                if src.get(q) == Some(&',') {
                    types.push(t::ATTRIBUTE_EMPTY);
                    starts.push(p);
                    ends.push(q + 1);
                    p = q + 1;
                } else {
                    p += 1;
                }
                state = S::Attribute;
            } else if c == Some(')') {
                p += 1;
                state = S::Initial;
            } else {
                types.push(t::ERROR);
                starts.push(p);
                ends.push(p);
                break;
            }
            continue;
        }

        if state == S::Attribute {
            if p >= n {
                continue;
            }
            if c == Some(')') {
                p += 1;
                state = S::Initial;
            } else if c == Some(',') {
                if src.get(p + 1) == Some(&',') {
                    types.push(t::ATTRIBUTE_EMPTY);
                    starts.push(p);
                    ends.push(p + 2);
                    p += 2;
                } else {
                    p += 1;
                }
            } else {
                let mut q = p;
                while src.get(q) == Some(&' ') {
                    q += 1;
                }
                let d = src.get(q).copied();
                if d == Some('"') {
                    if src.get(q + 1) == Some(&'"') {
                        types.push(t::ATTRIBUTE_EMPTY);
                        starts.push(p);
                        ends.push(q + 2);
                        p = q + 2;
                    } else {
                        p = q + 1;
                        state = S::String;
                    }
                } else if d == Some('$') {
                    p = q + 1;
                    state = S::StringKv;
                } else {
                    let end = up_to(',', p);
                    types.push(t::STR);
                    starts.push(p);
                    ends.push(end);
                    p = end;
                }
            }
            continue;
        }

        if state == S::String {
            if c == Some('"') {
                p += 1;
                state = S::Attribute;
            } else {
                let end = up_to('"', p);
                types.push(t::STR);
                starts.push(p);
                ends.push(end);
                p = end;
            }
            continue;
        }

        if state == S::StringKv {
            let end = up_to('=', p);
            types.push(t::STR_KEY);
            starts.push(p);
            ends.push(end);
            p = end;
            state = S::StringKvKey;
            continue;
        }

        if state == S::StringKvKey {
            if p >= n {
                continue;
            }
            let mut q = p + 1;
            if c == Some('=') {
                while src.get(q) == Some(&' ') {
                    q += 1;
                }
            }
            if c != Some('=') || src.get(q) != Some(&'"') {
                types.push(t::ERROR);
                starts.push(p);
                ends.push(p);
                break;
            }
            p = q + 1;
            state = S::StringKvValue;
            continue;
        }

        if state == S::StringKvValue {
            if p >= n {
                continue;
            }
            if c == Some('"') {
                p += 1;
                state = S::Attribute;
            } else {
                let end = up_to('"', p);
                types.push(t::STR_VALUE);
                starts.push(p);
                ends.push(end);
                p = end;
            }
            continue;
        }

        if state == S::Acc {
            let end = up_to('\n', p);
            types.push(acc_value);
            starts.push(p);
            ends.push(end);
            p = end;
            state = S::Initial;
            continue;
        }

        // S::AccDescrMultiline
        if c == Some('}') {
            p += 1;
            state = S::Initial;
        } else {
            let end = up_to('}', p);
            types.push(t::acc_descr_multiline_value);
            starts.push(p);
            ends.push(end);
            p = end;
        }
    }

    Tokens { types, starts, ends }
}
