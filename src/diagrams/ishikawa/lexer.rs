use crate::chars::{is_line_end, is_space, is_word};

pub mod t {
    pub const END: u8 = 0;
    pub const EOF: u8 = 1;
    pub const SPACELINE: u8 = 2;
    pub const NL: u8 = 3;
    pub const ISHIKAWA: u8 = 4;
    pub const SPACELIST: u8 = 5;
    pub const TEXT: u8 = 6;
}

pub const TOKEN_NAMES: [&str; 7] = [
    "$end",
    "EOF",
    "SPACELINE",
    "NL",
    "ISHIKAWA",
    "SPACELIST",
    "TEXT",
];

pub struct Tokens {
    pub types: Vec<u8>,
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
}

fn keyword(src: &[char], at: usize) -> usize {
    let word = b"ishikawa-beta";
    let mut k = 0usize;
    while k < 13 {
        let Some(&ch) = src.get(at + k) else {
            break;
        };
        let c = ch as u32;
        let w = word[k] as u32;
        if c != w && !(c >= 65 && c <= 90 && c + 32 == w) {
            break;
        }
        k += 1;
    }
    // `ishikawa-beta`, or failing that `ishikawa`, either one ending at a word boundary.
    if k == 13 && !src.get(at + 13).is_some_and(|&ch| is_word(ch)) {
        return 13;
    }
    if k >= 8 && !src.get(at + 8).is_some_and(|&ch| is_word(ch)) {
        8
    } else {
        0
    }
}

// Produces the tokens of Mermaid's Jison lexer for Ishikawa diagrams, where the first rule that
// matches wins: white space up to a comment or up to the last line break in it is a blank line,
// a single line break before text is a new line, other white space is indentation, and the rest
// of a line is text.
pub fn tokenize(src: &[char]) -> Tokens {
    let mut types = Vec::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let n = src.len();
    let mut i = 0usize;
    while i < n {
        let mut j = i;
        let mut last_break: isize = -1;
        while j < n && is_space(src[j]) {
            if src[j] == '\n' {
                last_break = j as isize;
            }
            j += 1;
        }
        let kind: u8;
        let end: usize;
        if src.get(j) == Some(&'%') && src.get(j + 1) == Some(&'%') {
            kind = t::SPACELINE;
            let mut e = j + 2;
            while e < n && !is_line_end(src[e]) {
                e += 1;
            }
            end = e;
        } else if j == i {
            let length = keyword(src, i);
            if length > 0 {
                kind = t::ISHIKAWA;
                end = i + length;
            } else {
                kind = t::TEXT;
                end = (i..n).find(|&idx| src[idx] == '\n').unwrap_or(n);
            }
        } else if last_break > i as isize {
            kind = t::SPACELINE;
            end = last_break as usize + 1;
        } else if last_break == i as isize {
            kind = t::NL;
            end = i + 1;
        } else {
            kind = t::SPACELIST;
            end = j;
        }
        types.push(kind);
        starts.push(i);
        ends.push(end);
        i = end;
    }
    types.push(t::EOF);
    types.push(t::END);
    starts.push(n);
    starts.push(n);
    ends.push(n);
    ends.push(n);
    Tokens { types, starts, ends }
}
