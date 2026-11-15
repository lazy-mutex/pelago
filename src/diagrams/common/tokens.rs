use crate::chars::{is_line_end, is_space};
use crate::errors::{syntax_error, PeleError, Result};

pub fn slice_starts_with(src: &[char], at: usize, lit: &str) -> bool {
    let mut i = at;
    for ch in lit.chars() {
        if src.get(i) != Some(&ch) {
            return false;
        }
        i += 1;
    }
    true
}

#[derive(Clone, Copy)]
pub enum Pattern {
    Literal(&'static str),
    Keyword(&'static str),
    Title,
    // `rest_matches(src, after_title)` returns true if `rest` matches at `after_title` (causing `titleBefore` NOT to match bare `title`).
    TitleBefore(fn(&[char], usize) -> bool),
    AccTitle,
    AccDescr,
    QuotedString,
    Newline,
    Whitespace,
    Yaml,
    Directive,
    SingleLineComment,
    // `(src, at, previous_kind) -> Option<end_index>`
    Custom(fn(&[char], usize, i32) -> Option<usize>),
    // `(src, at, previous_kind, opener) -> Option<end_index>` with `opener(src, at) -> bool` marking pattern dead if opener matched.
    CustomWithOpener(fn(&[char], usize, i32) -> Option<usize>, fn(&[char], usize) -> bool),
    // Factory called once per `tokenize` run so the scanner closure may remember state across positions.
    Scanner(fn() -> Box<dyn FnMut(&[char], usize, i32) -> Option<usize>>),
}

#[derive(Clone, Copy)]
pub struct TokenType {
    pub name: &'static str,
    pub pattern: Pattern,
    pub hidden: bool,
    pub longer: &'static [usize],
}

impl TokenType {
    pub const fn vis(name: &'static str, pattern: Pattern) -> Self {
        Self { name, pattern, hidden: false, longer: &[] }
    }

    pub const fn vis_longer(name: &'static str, pattern: Pattern, longer: &'static [usize]) -> Self {
        Self { name, pattern, hidden: false, longer }
    }

    pub const fn hid(name: &'static str, pattern: Pattern) -> Self {
        Self { name, pattern, hidden: true, longer: &[] }
    }
}

pub const ACC_DESCR: TokenType = TokenType::vis("ACC_DESCR", Pattern::AccDescr);
pub const ACC_TITLE: TokenType = TokenType::vis("ACC_TITLE", Pattern::AccTitle);
pub const TITLE: TokenType = TokenType::vis("TITLE", Pattern::Title);
pub const STRING: TokenType = TokenType::vis("STRING", Pattern::QuotedString);
pub const NEWLINE: TokenType = TokenType::vis("NEWLINE", Pattern::Newline);
pub const WHITESPACE: TokenType = TokenType::hid("WHITESPACE", Pattern::Whitespace);
pub const YAML: TokenType = TokenType::hid("YAML", Pattern::Yaml);
pub const DIRECTIVE: TokenType = TokenType::hid("DIRECTIVE", Pattern::Directive);
pub const SINGLE_LINE_COMMENT: TokenType = TokenType::hid("SINGLE_LINE_COMMENT", Pattern::SingleLineComment);

pub const fn keyword(word: &'static str) -> TokenType {
    TokenType::vis(word, Pattern::Keyword(word))
}

pub const fn title_before(rest_matches: fn(&[char], usize) -> bool) -> TokenType {
    TokenType::vis("TITLE", Pattern::TitleBefore(rest_matches))
}

pub const fn acc_descr(name: &'static str) -> TokenType {
    TokenType::vis(name, Pattern::AccDescr)
}

fn match_eol(src: &[char], at: usize) -> Option<usize> {
    match src.get(at) {
        Some('\r') if src.get(at + 1) == Some(&'\n') => Some(at + 2),
        Some('\n') => Some(at + 1),
        _ => None,
    }
}

fn skip_tabs_spaces(src: &[char], mut at: usize) -> usize {
    while at < src.len() && (src[at] == '\t' || src[at] == ' ') {
        at += 1;
    }
    at
}

fn scan_to_line_or_comment(src: &[char], mut i: usize) -> usize {
    let n = src.len();
    while i < n && src[i] != '\n' && src[i] != '\r' {
        if src[i] == '%' && src.get(i + 1) == Some(&'%') {
            break;
        }
        i += 1;
    }
    i
}

#[allow(clippy::type_complexity)]
fn match_token(
    k: usize,
    types: &[TokenType],
    src: &[char],
    at: usize,
    prev: i32,
    dead: &mut [bool],
    unclosed_acc: &mut usize,
    scanners: &mut [Option<Box<dyn FnMut(&[char], usize, i32) -> Option<usize>>>],
) -> Option<usize> {
    if dead[k] {
        return None;
    }
    let n = src.len();
    match types[k].pattern {
        Pattern::Literal(lit) => {
            slice_starts_with(src, at, lit).then(|| at + lit.chars().count())
        }
        Pattern::Keyword(word) => {
            if !slice_starts_with(src, at, word) {
                return None;
            }
            let end = at + word.chars().count();
            let ok = match src.get(end) {
                None => true,
                Some(&c) if is_space(c) => true,
                Some('%') if src.get(end + 1) == Some(&'%') => true,
                _ => false,
            };
            ok.then_some(end)
        }
        Pattern::Title => {
            let p = skip_tabs_spaces(src, at);
            if !slice_starts_with(src, p, "title") {
                return None;
            }
            let after = p + 5;
            match src.get(after) {
                Some('\t') | Some(' ') => Some(scan_to_line_or_comment(src, after + 1)),
                _ => Some(after),
            }
        }
        Pattern::TitleBefore(rest_fn) => {
            let p = skip_tabs_spaces(src, at);
            if !slice_starts_with(src, p, "title") {
                return None;
            }
            let after = p + 5;
            match src.get(after) {
                Some('\t') | Some(' ') => Some(scan_to_line_or_comment(src, after + 1)),
                _ if rest_fn(src, after) => None,
                _ => Some(after),
            }
        }
        Pattern::AccTitle => {
            let p = skip_tabs_spaces(src, at);
            if !slice_starts_with(src, p, "accTitle") {
                return None;
            }
            let colon = skip_tabs_spaces(src, p + 8);
            if src.get(colon) != Some(&':') {
                return None;
            }
            Some(scan_to_line_or_comment(src, colon + 1))
        }
        Pattern::AccDescr => {
            let p = skip_tabs_spaces(src, at);
            if !slice_starts_with(src, p, "accDescr") {
                return None;
            }
            let q = skip_tabs_spaces(src, p + 8);
            if src.get(q) == Some(&':') {
                return Some(scan_to_line_or_comment(src, q + 1));
            }
            let mut b = p + 8;
            while b < n && is_space(src[b]) {
                b += 1;
            }
            if src.get(b) != Some(&'{') || b >= *unclosed_acc {
                return None;
            }
            let mut j = b + 1;
            while j < n && src[j] != '}' {
                j += 1;
            }
            if j < n {
                Some(j + 1)
            } else {
                *unclosed_acc = b;
                None
            }
        }
        Pattern::QuotedString => {
            let q = *src.get(at)?;
            if q != '"' && q != '\'' {
                return None;
            }
            let mut i = at + 1;
            while i < n {
                let c = src[i];
                if c == q {
                    return Some(i + 1);
                }
                if c == '\\' {
                    let next = *src.get(i + 1)?;
                    if is_line_end(next) {
                        return None;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            None
        }
        Pattern::Newline => match_eol(src, at),
        Pattern::Whitespace => {
            let end = skip_tabs_spaces(src, at);
            (end > at).then_some(end)
        }
        Pattern::Yaml => {
            if !slice_starts_with(src, at, "---") {
                return None;
            }
            let i = skip_tabs_spaces(src, at + 3);
            let Some(after_open) = match_eol(src, i) else {
                return None;
            };
            let mut cur = after_open;
            while cur < n {
                if slice_starts_with(src, cur, "---") {
                    let close_end = cur + 3;
                    if let Some(after_close) = match_eol(src, close_end) {
                        return Some(after_close);
                    }
                    if src.get(close_end).map_or(true, |&c| is_space(c)) {
                        return Some(close_end);
                    }
                }
                while cur < n && src[cur] != '\n' {
                    cur += 1;
                }
                if cur < n {
                    cur += 1;
                }
            }
            dead[k] = true;
            None
        }
        Pattern::Directive => {
            let i = skip_tabs_spaces(src, at);
            if !slice_starts_with(src, i, "%%{") {
                return None;
            }
            let mut j = i + 3;
            while j < n {
                if slice_starts_with(src, j, "}%%") {
                    let close_end = j + 3;
                    if let Some(after_close) = match_eol(src, close_end) {
                        return Some(after_close);
                    }
                    if src.get(close_end).map_or(true, |&c| is_space(c)) {
                        return Some(close_end);
                    }
                }
                j += 1;
            }
            dead[k] = true;
            None
        }
        Pattern::SingleLineComment => {
            let i = skip_tabs_spaces(src, at);
            if !slice_starts_with(src, i, "%%") {
                return None;
            }
            let mut j = i + 2;
            while j < n && src[j] != '\n' && src[j] != '\r' {
                j += 1;
            }
            Some(j)
        }
        Pattern::Custom(func) => func(src, at, prev),
        Pattern::CustomWithOpener(func, opener) => {
            let res = func(src, at, prev);
            if res.is_none() && opener(src, at) {
                dead[k] = true;
            }
            res
        }
        Pattern::Scanner(factory) => {
            let scanner = scanners[k].get_or_insert_with(factory);
            scanner(src, at, prev)
        }
    }
}

pub struct Tokens {
    pub kinds: Vec<i32>,
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
}

pub fn tokenize(src: &[char], types: &[TokenType], diagram: &'static str) -> Result<Tokens> {
    let mut kinds: Vec<i32> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut ends: Vec<usize> = Vec::new();
    let mut dead = vec![false; types.len()];
    let mut unclosed_acc = usize::MAX;
    #[allow(clippy::type_complexity)]
    let mut scanners: Vec<Option<Box<dyn FnMut(&[char], usize, i32) -> Option<usize>>>> =
        (0..types.len()).map(|_| None).collect();
    let n = src.len();
    let mut p = 0usize;

    'scan: while p < n {
        let prev = kinds.last().copied().unwrap_or(-1);
        for k in 0..types.len() {
            let Some(mut end) = match_token(k, types, src, p, prev, &mut dead, &mut unclosed_acc, &mut scanners) else {
                continue;
            };
            if end <= p {
                continue;
            }
            let mut kind = k;
            for &alt in types[k].longer {
                if let Some(alt_end) = match_token(alt, types, src, p, prev, &mut dead, &mut unclosed_acc, &mut scanners) {
                    if alt_end > end {
                        kind = alt;
                        end = alt_end;
                        break;
                    }
                }
            }
            if !types[kind].hidden {
                kinds.push(kind as i32);
                starts.push(p);
                ends.push(end);
            }
            p = end;
            continue 'scan;
        }
        return Err(syntax_error(diagram, src, p, "", true));
    }
    kinds.push(-1);
    starts.push(n);
    ends.push(n);
    Ok(Tokens { kinds, starts, ends })
}

pub struct Reader<'a> {
    pub i: usize,
    pub src: &'a [char],
    pub tokens: Tokens,
    pub types: &'a [TokenType],
    pub diagram: &'static str,
}

impl<'a> Reader<'a> {
    pub fn new(src: &'a [char], tokens: Tokens, types: &'a [TokenType], diagram: &'static str) -> Self {
        Self { i: 0, src, tokens, types, diagram }
    }

    pub fn kind(&self) -> i32 {
        self.tokens.kinds[self.i]
    }

    pub fn kind_at(&self, index: usize) -> i32 {
        self.tokens.kinds.get(index).copied().unwrap_or(-1)
    }

    pub fn text(&self) -> String {
        self.text_at(self.i)
    }

    pub fn text_at(&self, index: usize) -> String {
        self.src[self.tokens.starts[index]..self.tokens.ends[index]].iter().collect()
    }

    pub fn slice(&self) -> &'a [char] {
        &self.src[self.tokens.starts[self.i]..self.tokens.ends[self.i]]
    }

    pub fn take(&mut self) -> String {
        let s = self.text_at(self.i);
        self.i += 1;
        s
    }

    pub fn accept(&mut self, kind: i32) -> bool {
        if self.tokens.kinds[self.i] != kind {
            return false;
        }
        self.i += 1;
        true
    }

    pub fn expect(&mut self, kind: i32) -> Result<String> {
        if self.tokens.kinds[self.i] != kind {
            return Err(self.fail(&format!("token of type '{}'", self.types[kind as usize].name)));
        }
        Ok(self.take())
    }

    pub fn fail(&self, expected: &str) -> PeleError {
        let found = if self.kind() == -1 {
            "end of input".to_string()
        } else {
            format!("`{}`", self.text())
        };
        let mut at = self.tokens.starts[self.i];
        if self.kind() == -1 {
            let mut k = self.i;
            while k > 0 && self.src[self.tokens.starts[k - 1]..self.tokens.ends[k - 1]].iter().all(|&c| is_space(c)) {
                k -= 1;
            }
            if k > 0 {
                at = self.tokens.ends[k - 1];
            }
        }
        syntax_error(self.diagram, self.src, at, &format!("Expecting {} but found {}.", expected, found), false)
    }
}

fn single_spaces(text: &str) -> String {
    let trimmed = text.trim_matches(is_space);
    let chars: Vec<char> = trimmed.chars().collect();
    let mut out = String::with_capacity(trimmed.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' || chars[i] == '\t' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                j += 1;
            }
            if j - i >= 2 {
                out.push(' ');
            } else {
                out.push(chars[i]);
            }
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn title_value(text: &str) -> String {
    let idx = text.find("title").map_or(0, |p| p + 5);
    let rest = &text[idx..];
    if rest.starts_with(['\t', ' ']) {
        let line = rest.split(['\n', '\r']).next().unwrap_or("");
        single_spaces(line)
    } else {
        String::new()
    }
}

pub fn acc_title_value(text: &str) -> String {
    let after_kw = &text[text.find("accTitle").map_or(0, |p| p + 8)..];
    let idx = after_kw.find(':').map_or(after_kw.len(), |p| p + 1);
    let line = after_kw[idx..].split(['\n', '\r']).next().unwrap_or("");
    single_spaces(line)
}

pub fn acc_descr_value(text: &str) -> String {
    let after = &text[text.find("accDescr").map_or(0, |p| p + 8)..];
    let trimmed_tabs = after.trim_start_matches(['\t', ' ']);
    if let Some(colon_rest) = trimmed_tabs.strip_prefix(':') {
        let line = colon_rest.split(['\n', '\r']).next().unwrap_or("");
        return single_spaces(line);
    }
    let trimmed_ws = after.trim_start_matches(is_space);
    if let Some(brace_rest) = trimmed_ws.strip_prefix('{') {
        if let Some(close) = brace_rest.find('}') {
            let body = &brace_rest[..close];
            // Emulate:
            // .replace(/^\s*/gm, '')
            // .replace(/\s+$/gm, '')
            // .replace(/[\t ]{2,}/gm, ' ')
            // .replace(/[\n\r]{2,}/gm, '\n')
            let mut normalized = String::with_capacity(body.len());
            let chars: Vec<char> = body.chars().collect();
            let mut i = 0;
            // Split into lines preserving CR/LF sequences, strip leading/trailing non-newline spaces on each line,
            // and collapse empty/whitespace-only runs across newlines.
            // Specifically: /^\s*/gm in JS without `s` flag matches `\s*` at start of string AND after every `\n` or `\r`!
            // Note `\s` includes `\n` and `\r`! So `^\s*` at index 0 eats all leading whitespace/newlines,
            // and after any remaining `\n`/`\r`, eats all leading whitespace/newlines too!
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
            while i < chars.len() {
                let line_start = i;
                while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
                    i += 1;
                }
                let line_str: String = chars[line_start..i].iter().collect();
                let cleaned = single_spaces(&line_str);
                if !cleaned.is_empty() {
                    if !normalized.is_empty() {
                        normalized.push('\n');
                    }
                    normalized.push_str(&cleaned);
                }
                while i < chars.len() && is_space(chars[i]) {
                    i += 1;
                }
            }
            return normalized;
        }
    }
    String::new()
}

pub fn string_value(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 {
        return String::new();
    }
    let inner = &chars[1..chars.len() - 1];
    if !inner.contains(&'\\') {
        return inner.iter().collect();
    }
    let mut out = String::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c == '\\' && i + 1 < inner.len() {
            i += 1;
            out.push(match inner[i] {
                'b' => '\u{0008}',
                'f' => '\u{000c}',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'v' => '\u{000b}',
                '0' => '\0',
                other => other,
            });
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}
