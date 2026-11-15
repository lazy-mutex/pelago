use super::db::VennDb;
use crate::chars::{is_digit, is_hex, is_space, is_word};
use crate::diagrams::common::caseless::word_at;

pub mod t {
    pub const END: usize = 0;
    pub const ERROR: usize = 1;
    pub const EOF: usize = 2;
    pub const NEWLINE: usize = 3;
    pub const VENN: usize = 4;
    pub const TITLE: usize = 5;
    pub const SET: usize = 6;
    pub const UNION: usize = 7;
    pub const TEXT: usize = 8;
    pub const INDENT_TEXT: usize = 9;
    pub const STYLE: usize = 10;
    pub const BRACKET_LABEL: usize = 11;
    pub const NUMERIC: usize = 12;
    pub const HEXCOLOR: usize = 13;
    pub const RGBCOLOR: usize = 14;
    pub const RGBACOLOR: usize = 15;
    pub const IDENTIFIER: usize = 16;
    pub const STRING: usize = 17;
    pub const COMMA: usize = 18;
    pub const COLON: usize = 19;
}

pub const TOKEN_NAMES: [&str; 20] = [
    "$end",
    "INVALID",
    "EOF",
    "NEWLINE",
    "VENN",
    "TITLE",
    "SET",
    "UNION",
    "TEXT",
    "INDENT_TEXT",
    "STYLE",
    "BRACKET_LABEL",
    "NUMERIC",
    "HEXCOLOR",
    "RGBCOLOR",
    "RGBACOLOR",
    "IDENTIFIER",
    "STRING",
    "COMMA",
    "COLON",
];

const KEYWORDS: [(&str, usize); 5] = [
    ("venn-beta", t::VENN),
    ("set", t::SET),
    ("union", t::UNION),
    ("text", t::TEXT),
    ("style", t::STYLE),
];

// Reads tokens one at a time, as Mermaid's Jison lexer for venn diagrams does: every rule
// ignores case, the first rule that matches wins, and a keyword must end at a word boundary.
pub struct VennLexer<'a> {
    pub text: String,
    pub start: usize,
    src: &'a [char],
    at: usize,
    line_start: bool,
    ended: bool,
}

impl<'a> VennLexer<'a> {
    pub fn new(src: &'a [char]) -> Self {
        Self {
            text: String::new(),
            start: 0,
            src,
            at: 0,
            line_start: false,
            ended: false,
        }
    }

    fn word(&self, at: usize, w: &str) -> bool {
        word_at(self.src, at, w)
    }

    fn keyword(&self, at: usize, w: &str) -> bool {
        self.word(at, w) && !self.src.get(at + w.len()).is_some_and(|&c| is_word(c))
    }

    // The end of `rgb(1, 2, 3)` or `rgba(1, 2, 3, 0.5)` whose parenthesis opens at `at`.
    fn color(&self, at: usize, parts: usize) -> Option<usize> {
        let n = self.src.len();
        let mut i = at + 1;
        for part in 0..parts {
            while i < n && is_space(self.src[i]) {
                i += 1;
            }
            let from = i;
            while i < n && (is_digit(self.src[i]) || self.src[i] == '.') {
                i += 1;
            }
            if i == from {
                return None;
            }
            while i < n && is_space(self.src[i]) {
                i += 1;
            }
            let expected = if part == parts - 1 { ')' } else { ',' };
            if self.src.get(i) != Some(&expected) {
                return None;
            }
            i += 1;
        }
        Some(i)
    }

    fn after_line_feed(&self, at: usize) -> bool {
        let mut k = at as isize - 1;
        while k >= 0 && matches!(self.src[k as usize], ' ' | '\t') {
            k -= 1;
        }
        k >= 0 && self.src[k as usize] == '\n'
    }

    fn line_end(&self, from: usize) -> usize {
        let mut i = from;
        while i < self.src.len() && self.src[i] != '\n' {
            i += 1;
        }
        i
    }

    fn find_char(&self, ch: char, from: usize) -> Option<usize> {
        let mut i = from;
        while i < self.src.len() {
            if self.src[i] == ch {
                return Some(i);
            }
            i += 1;
        }
        None
    }

    fn token(&mut self, kind: usize, end: usize) -> usize {
        self.text = self.src[self.at..end].iter().collect();
        self.at = end;
        kind
    }

    pub fn next(&mut self, host: &mut VennDb) -> usize {
        let n = self.src.len();
        loop {
            let mut i = self.at;
            self.start = i;
            self.text.clear();
            if i >= n {
                if self.ended {
                    return t::END;
                }
                self.ended = true;
                return t::EOF;
            }
            let c = self.src[i];
            if c == '%' && self.src.get(i + 1) == Some(&'%') && self.src.get(i + 2) != Some(&'{') {
                self.at = self.line_end(i + 2);
                continue;
            }
            if self.line_start
                && self.word(i, "title")
                && self.after_line_feed(i)
                && self.src[i + 5..self.line_end(i + 5)].iter().all(|&ch| is_space(ch))
            {
                let mut j = self.line_end(i + 5);
                while j < n && (self.src[j] == '\n' || self.src[j] == '\r') {
                    j += 1;
                }
                self.at = j;
                continue;
            }
            if self.line_start {
                if c == ' ' || c == '\t' {
                    let mut j = i + 1;
                    while j < n && (self.src[j] == ' ' || self.src[j] == '\t') {
                        j += 1;
                    }
                    self.at = j;
                    if self.keyword(j, "text") && host.get_indent_mode() {
                        self.at = j + 4;
                        self.line_start = false;
                        self.text = self.src[i..j].iter().collect();
                        return t::INDENT_TEXT;
                    }
                    continue;
                }
                if c != '\n' && c != '\r' {
                    host.set_indent_mode(false);
                    self.line_start = false;
                }
            }
            if c == '\n' || c == '\r' {
                i += 1;
                while i < n && (self.src[i] == '\n' || self.src[i] == '\r') {
                    i += 1;
                }
                self.line_start = true;
                return self.token(t::NEWLINE, i);
            }
            if c == '%' && self.src.get(i + 1) == Some(&'%') {
                self.at = self.line_end(i);
                continue;
            }
            if c == ' ' || c == '\t' {
                i += 1;
                while i < n && (self.src[i] == ' ' || self.src[i] == '\t') {
                    i += 1;
                }
                self.at = i;
                continue;
            }

            if c.is_ascii_alphabetic() || c == '_' {
                if self.word(i, "title")
                    && self.src.get(i + 5).is_some_and(|&sep| is_space(sep) && sep != '\n' && sep != '\r')
                {
                    let mut end = i + 6;
                    while end < n && !matches!(self.src[end], '#' | '\n' | ';') {
                        end += 1;
                    }
                    if end > i + 6 {
                        return self.token(t::TITLE, end);
                    }
                }
                for (kw, kind) in KEYWORDS {
                    if self.keyword(i, kw) && self.src.get(i + kw.len()) != Some(&'-') {
                        return self.token(kind, i + kw.len());
                    }
                }
                if self.word(i, "rgb") {
                    let alpha = self.src.get(i + 3).is_some_and(|ch| ch.eq_ignore_ascii_case(&'a'));
                    let open = i + if alpha { 4 } else { 3 };
                    if self.src.get(open) == Some(&'(') {
                        if let Some(end) = self.color(open, if alpha { 4 } else { 3 }) {
                            return self.token(if alpha { t::RGBACOLOR } else { t::RGBCOLOR }, end);
                        }
                    }
                }
                i += 1;
                while i < n && (is_word(self.src[i]) || self.src[i] == '-') {
                    i += 1;
                }
                return self.token(t::IDENTIFIER, i);
            }
            if c == '[' {
                if self.src.get(i + 1) == Some(&'"') {
                    let Some(quote) = self.find_char('"', i + 2) else {
                        return t::ERROR;
                    };
                    if self.src.get(quote + 1) != Some(&']') {
                        return t::ERROR;
                    }
                    self.at = quote + 2;
                    self.text = self.src[i + 2..quote].iter().collect();
                    return t::BRACKET_LABEL;
                }
                let mut end = i + 1;
                while end < n && self.src[end] != ']' && self.src[end] != '"' {
                    end += 1;
                }
                if end == i + 1 || self.src.get(end) != Some(&']') {
                    return t::ERROR;
                }
                self.at = end + 1;
                let raw: String = self.src[i + 1..end].iter().collect();
                self.text = raw.trim_matches(is_space).to_string();
                return t::BRACKET_LABEL;
            }
            if is_digit(c) || matches!(c, '.' | '+' | '-') {
                if c == '+' || c == '-' {
                    i += 1;
                }
                let from = i;
                while i < n && is_digit(self.src[i]) {
                    i += 1;
                }
                if i > from {
                    if self.src.get(i) == Some(&'.') && self.src.get(i + 1).is_some_and(|&d| is_digit(d)) {
                        i += 2;
                        while i < n && is_digit(self.src[i]) {
                            i += 1;
                        }
                    }
                } else if self.src.get(i) == Some(&'.') && self.src.get(i + 1).is_some_and(|&d| is_digit(d)) {
                    i += 2;
                    while i < n && is_digit(self.src[i]) {
                        i += 1;
                    }
                } else {
                    return t::ERROR;
                }
                return self.token(t::NUMERIC, i);
            }
            if c == '#' {
                let mut end = i + 1;
                let limit = (i + 9).min(n);
                while end < limit && is_hex(self.src[end]) {
                    end += 1;
                }
                return if end >= i + 4 { self.token(t::HEXCOLOR, end) } else { t::ERROR };
            }
            if c == '"' {
                return match self.find_char('"', i + 1) {
                    Some(quote) => self.token(t::STRING, quote + 1),
                    None => t::ERROR,
                };
            }
            if c == ',' {
                return self.token(t::COMMA, i + 1);
            }
            if c == ':' {
                return self.token(t::COLON, i + 1);
            }
            return t::ERROR;
        }
    }
}
