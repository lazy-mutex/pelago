use super::{RailroadNode, RailroadRule};
use crate::errors::{syntax_error, Result};

// Shared tokenizer, metadata extractors, string unescaping, and non-recursive tree fold
// for Mermaid's four railroad notations (`railroad-beta`, `ebnf`, `abnf`, `peg`).

pub fn is_word_or_dash(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

pub fn is_alnum_or_dash(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

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
    Title(fn(char) -> bool),
    AccTitle,
    AccDescr,
    Ident,
    AbnfName,
    Quoted,
    AbnfString,
    AbnfNumVal,
    AbnfRepeat,
    AbnfExact,
    EbnfSpecial,
    Whitespace,
    Yaml,
    Directive,
    SingleLineComment,
    BlockComment(&'static str, &'static str),
    LineComment(char),
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

fn is_js_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn match_eol(src: &[char], at: usize) -> Option<usize> {
    match src.get(at) {
        Some('\r') if src.get(at + 1) == Some(&'\n') => Some(at + 2),
        Some('\n') => Some(at + 1),
        _ => None,
    }
}

fn match_pattern(pat: Pattern, src: &[char], at: usize, unclosed_acc: &mut usize, dead: &mut bool) -> Option<usize> {
    if *dead {
        return None;
    }
    let n = src.len();
    match pat {
        Pattern::Literal(lit) => {
            if slice_starts_with(src, at, lit) {
                Some(at + lit.chars().count())
            } else {
                None
            }
        }
        Pattern::Keyword(word) => {
            if !slice_starts_with(src, at, word) {
                return None;
            }
            let end = at + word.chars().count();
            let ok = match src.get(end) {
                None => true,
                Some(c) if c.is_whitespace() => true,
                Some('%') if src.get(end + 1) == Some(&'%') => true,
                _ => false,
            };
            ok.then_some(end)
        }
        Pattern::Title(rest_fn) => {
            if !slice_starts_with(src, at, "title") {
                return None;
            }
            let after = at + 5;
            match src.get(after) {
                Some(&c) if c == '\t' || c == ' ' => {
                    let mut i = after + 1;
                    while i < n && src[i] != '\n' && src[i] != '\r' {
                        if src[i] == '%' && src.get(i + 1) == Some(&'%') {
                            break;
                        }
                        i += 1;
                    }
                    Some(i)
                }
                Some(&c) if rest_fn(c) => None,
                _ => Some(after),
            }
        }
        Pattern::AccTitle => {
            if !slice_starts_with(src, at, "accTitle") {
                return None;
            }
            let mut i = at + 8;
            while i < n && (src[i] == '\t' || src[i] == ' ') {
                i += 1;
            }
            if src.get(i) != Some(&':') {
                return None;
            }
            i += 1;
            while i < n && src[i] != '\n' && src[i] != '\r' {
                if src[i] == '%' && src.get(i + 1) == Some(&'%') {
                    break;
                }
                i += 1;
            }
            Some(i)
        }
        Pattern::AccDescr => {
            if !slice_starts_with(src, at, "accDescr") {
                return None;
            }
            let mut i = at + 8;
            while i < n && (src[i] == '\t' || src[i] == ' ') {
                i += 1;
            }
            if src.get(i) == Some(&':') {
                i += 1;
                while i < n && src[i] != '\n' && src[i] != '\r' {
                    if src[i] == '%' && src.get(i + 1) == Some(&'%') {
                        break;
                    }
                    i += 1;
                }
                return Some(i);
            }
            while i < n && src[i].is_whitespace() {
                i += 1;
            }
            if src.get(i) != Some(&'{') || i >= *unclosed_acc {
                return None;
            }
            let mut j = i + 1;
            while j < n && src[j] != '}' {
                j += 1;
            }
            if j < n {
                Some(j + 1)
            } else {
                *unclosed_acc = i;
                None
            }
        }
        Pattern::Ident => {
            let c = *src.get(at)?;
            if !(c.is_ascii_alphabetic() || c == '_') {
                return None;
            }
            let mut i = at + 1;
            while i < n && is_word_or_dash(src[i]) {
                i += 1;
            }
            Some(i)
        }
        Pattern::AbnfName => {
            let c = *src.get(at)?;
            if !c.is_ascii_alphabetic() {
                return None;
            }
            let mut i = at + 1;
            while i < n && is_alnum_or_dash(src[i]) {
                i += 1;
            }
            Some(i)
        }
        Pattern::Quoted => {
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
                    if is_js_line_terminator(next) {
                        return None;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            None
        }
        Pattern::AbnfString => {
            if src.get(at) != Some(&'"') {
                return None;
            }
            let mut i = at + 1;
            while i < n {
                if src[i] == '"' {
                    return Some(i + 1);
                }
                i += 1;
            }
            None
        }
        Pattern::AbnfNumVal => {
            if src.get(at) != Some(&'%') {
                return None;
            }
            let base = *src.get(at + 1)?;
            if !matches!(base, 'x' | 'X' | 'd' | 'D' | 'b' | 'B') {
                return None;
            }
            let mut i = at + 2;
            let start_digits = i;
            while i < n && src[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i == start_digits {
                return None;
            }
            if let Some(&sep @ ('-' | '.')) = src.get(i) {
                if sep == '-' {
                    let mut j = i + 1;
                    while j < n && src[j].is_ascii_hexdigit() {
                        j += 1;
                    }
                    if j > i + 1 {
                        i = j;
                    }
                } else {
                    loop {
                        if src.get(i) != Some(&'.') {
                            break;
                        }
                        let mut j = i + 1;
                        while j < n && src[j].is_ascii_hexdigit() {
                            j += 1;
                        }
                        if j == i + 1 {
                            break;
                        }
                        i = j;
                    }
                }
            }
            Some(i)
        }
        Pattern::AbnfRepeat => {
            let mut i = at;
            while i < n && src[i].is_ascii_digit() {
                i += 1;
            }
            if src.get(i) != Some(&'*') {
                return None;
            }
            i += 1;
            while i < n && src[i].is_ascii_digit() {
                i += 1;
            }
            Some(i)
        }
        Pattern::AbnfExact => {
            let mut i = at;
            while i < n && src[i].is_ascii_digit() {
                i += 1;
            }
            (i > at).then_some(i)
        }
        Pattern::EbnfSpecial => {
            if src.get(at) != Some(&'?') {
                return None;
            }
            let mut solid = false;
            let mut i = at + 1;
            while i < n {
                let c = src[i];
                if c == '?' {
                    return solid.then_some(i + 1);
                }
                if c == ';' {
                    return None;
                }
                if !c.is_whitespace() {
                    solid = true;
                }
                i += 1;
            }
            None
        }
        Pattern::Whitespace => {
            let mut i = at;
            while i < n && matches!(src[i], '\t' | ' ' | '\r' | '\n') {
                i += 1;
            }
            (i > at).then_some(i)
        }
        Pattern::Yaml => {
            if !slice_starts_with(src, at, "---") {
                return None;
            }
            let mut i = at + 3;
            while i < n && (src[i] == '\t' || src[i] == ' ') {
                i += 1;
            }
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
                    if src.get(close_end).map_or(true, |c| c.is_whitespace()) {
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
            *dead = true;
            None
        }
        Pattern::Directive => {
            let mut i = at;
            while i < n && (src[i] == '\t' || src[i] == ' ') {
                i += 1;
            }
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
                    if src.get(close_end).map_or(true, |c| c.is_whitespace()) {
                        return Some(close_end);
                    }
                }
                j += 1;
            }
            *dead = true;
            None
        }
        Pattern::SingleLineComment => {
            let mut i = at;
            while i < n && (src[i] == '\t' || src[i] == ' ') {
                i += 1;
            }
            if !slice_starts_with(src, i, "%%") {
                return None;
            }
            i += 2;
            while i < n && src[i] != '\n' && src[i] != '\r' {
                i += 1;
            }
            Some(i)
        }
        Pattern::BlockComment(open, close) => {
            if !slice_starts_with(src, at, open) {
                return None;
            }
            let mut i = at + open.chars().count();
            let close_len = close.chars().count();
            while i < n {
                if slice_starts_with(src, i, close) {
                    return Some(i + close_len);
                }
                i += 1;
            }
            *dead = true;
            None
        }
        Pattern::LineComment(ch) => {
            if src.get(at) != Some(&ch) {
                return None;
            }
            let mut i = at + 1;
            while i < n && src[i] != '\n' && src[i] != '\r' {
                i += 1;
            }
            Some(i)
        }
    }
}

pub struct Tokens {
    pub kinds: Vec<i32>,
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
}

pub fn tokenize(src: &[char], types: &[TokenType], diagram: &'static str) -> Result<Tokens> {
    let mut kinds = Vec::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut dead = vec![false; types.len()];
    let mut unclosed_acc = usize::MAX;
    let n = src.len();
    let mut p = 0;

    'scan: while p < n {
        for k in 0..types.len() {
            let Some(mut end) = match_pattern(types[k].pattern, src, p, &mut unclosed_acc, &mut dead[k]) else {
                continue;
            };
            if end <= p {
                continue;
            }
            let mut kind = k;
            for &alt in types[k].longer {
                if let Some(alt_end) = match_pattern(types[alt].pattern, src, p, &mut unclosed_acc, &mut dead[alt]) {
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

    pub fn text(&self) -> String {
        self.src[self.tokens.starts[self.i]..self.tokens.ends[self.i]].iter().collect()
    }

    pub fn take(&mut self) -> String {
        let s = self.text();
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

    pub fn fail(&self, expected: &str) -> crate::errors::PeleError {
        let found = if self.kind() == -1 {
            "end of input".to_string()
        } else {
            format!("`{}`", self.text())
        };
        let mut at = self.tokens.starts[self.i];
        if self.kind() == -1 {
            let mut k = self.i;
            while k > 0 && self.src[self.tokens.starts[k - 1]..self.tokens.ends[k - 1]].iter().all(|c| c.is_whitespace()) {
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
    let trimmed = text.trim();
    let mut out = String::with_capacity(trimmed.len());
    let mut run = 0usize;
    for ch in trimmed.chars() {
        if ch == ' ' || ch == '\t' {
            run += 1;
        } else {
            if run >= 2 {
                out.push(' ');
            } else if run == 1 {
                // Keep a single tab or space as written when len == 1.
            }
            run = 0;
            out.push(ch);
        }
    }
    // Re-implement exact `/[\t ]{2,}/gm -> ' '`
    let mut exact = String::with_capacity(trimmed.len());
    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' || chars[i] == '\t' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
                j += 1;
            }
            if j - i >= 2 {
                exact.push(' ');
            } else {
                exact.push(chars[i]);
            }
            i = j;
        } else {
            exact.push(chars[i]);
            i += 1;
        }
    }
    let _ = out;
    exact
}

pub fn title_value(text: &str) -> String {
    let idx = text.find("title").map_or(0, |p| p + 5);
    let rest = &text[idx..];
    let line = rest.split(['\n', '\r']).next().unwrap_or("");
    single_spaces(line)
}

pub fn acc_title_value(text: &str) -> String {
    let idx = text.find(':').map_or(text.len(), |p| p + 1);
    let line = text[idx..].split(['\n', '\r']).next().unwrap_or("");
    single_spaces(line)
}

pub fn acc_descr_value(text: &str) -> String {
    let after = &text[text.find("accDescr").map_or(0, |p| p + 8)..];
    let trimmed = after.trim_start_matches(['\t', ' ']);
    if let Some(colon_rest) = trimmed.strip_prefix(':') {
        let line = colon_rest.split(['\n', '\r']).next().unwrap_or("");
        return single_spaces(line);
    }
    if let Some(open) = after.find('{') {
        if let Some(close) = after[open + 1..].find('}') {
            let body = &after[open + 1..open + 1 + close];
            let mut lines: Vec<String> = Vec::new();
            for raw_line in body.split(['\n', '\r']) {
                lines.push(single_spaces(raw_line));
            }
            let joined = lines.join("\n");
            let mut collapsed = String::with_capacity(joined.len());
            let mut nl = false;
            for ch in joined.trim().chars() {
                if ch == '\n' {
                    if !nl {
                        collapsed.push('\n');
                    }
                    nl = true;
                } else {
                    nl = false;
                    collapsed.push(ch);
                }
            }
            return collapsed;
        }
    }
    String::new()
}

// Mermaid's railroad value converters know three escapes; any other escaped character stands for itself.
pub fn unescape(text: &str) -> String {
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
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

pub struct ParsedGrammar {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub rules: Vec<RailroadRule>,
}

pub fn parse_grammar<F>(
    src: &str,
    types: &[TokenType],
    meta: i32,
    unquote: fn(&str) -> String,
    mut parse_rule: F,
) -> Result<ParsedGrammar>
where
    F: FnMut(&mut Reader) -> Result<RailroadRule>,
{
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, types, "railroad")?;
    let mut r = Reader::new(&chars, tokens, types, "railroad");
    r.expect(0)?;
    let mut out = ParsedGrammar { title: None, acc_title: None, acc_descr: None, rules: Vec::new() };
    loop {
        let k = r.kind();
        if k == meta {
            let t = title_value(&r.take());
            let chars_t: Vec<char> = t.chars().collect();
            let parsed_title = if chars_t.len() >= 2 && (chars_t[0] == '"' || chars_t[0] == '\'') && chars_t.last() == Some(&chars_t[0]) {
                unquote(&t)
            } else {
                t
            };
            out.title = Some(parsed_title);
        } else if k == meta + 1 {
            out.acc_title = Some(acc_title_value(&r.take()));
        } else if k == meta + 2 {
            out.acc_descr = Some(acc_descr_value(&r.take()));
        } else {
            break;
        }
    }
    while r.kind() != -1 {
        out.rules.push(parse_rule(&mut r)?);
    }
    Ok(out)
}

pub fn terminal(value: String) -> RailroadNode {
    RailroadNode::Terminal(value)
}

pub fn nonterminal(name: String) -> RailroadNode {
    RailroadNode::NonTerminal(name)
}

pub fn special(text: String) -> RailroadNode {
    RailroadNode::Special(text)
}

pub fn optional(element: RailroadNode) -> RailroadNode {
    RailroadNode::Optional(Box::new(element))
}

pub fn repetition(element: RailroadNode, min: f64, max: f64) -> RailroadNode {
    RailroadNode::Repetition { element: Box::new(element), min, max }
}

pub fn sequence(mut elements: Vec<RailroadNode>) -> RailroadNode {
    if elements.len() == 1 {
        elements.pop().unwrap()
    } else {
        RailroadNode::Sequence(elements)
    }
}

pub fn choice(mut alternatives: Vec<RailroadNode>) -> RailroadNode {
    if alternatives.len() == 1 {
        alternatives.pop().unwrap()
    } else {
        RailroadNode::Choice(alternatives)
    }
}

// Rebuilds a tree bottom-up without recursion, so nesting depth is bounded by memory, not stack.
pub fn fold<'a, T, C, B, R>(root: &'a T, mut children: C, mut build: B) -> R
where
    C: FnMut(&'a T) -> Vec<&'a T>,
    B: FnMut(&'a T, Vec<R>) -> R,
{
    let mut nodes: Vec<&'a T> = vec![root];
    let mut lists: Vec<Vec<&'a T>> = vec![children(root)];
    let mut done: Vec<Vec<R>> = vec![Vec::new()];
    loop {
        let top = nodes.len() - 1;
        let done_len = done[top].len();
        if done_len < lists[top].len() {
            let child = lists[top][done_len];
            nodes.push(child);
            lists.push(children(child));
            done.push(Vec::new());
        } else {
            let results = done.pop().unwrap();
            lists.pop();
            let node = nodes.pop().unwrap();
            let value = build(node, results);
            if nodes.is_empty() {
                return value;
            }
            done.last_mut().unwrap().push(value);
        }
    }
}
