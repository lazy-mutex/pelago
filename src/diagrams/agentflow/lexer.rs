use super::tokens::tok;
use crate::chars::{is_line_end, is_space, is_unicode_letter, is_word};

#[derive(Default)]
pub struct Tokens {
    pub types: Vec<u8>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
    // Four numbers per token, counted the way Jison counts them:
    // first line, first column, last line, last column. Lines start at 1, columns at 0.
    pub locs: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    String,
    MdString,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Dir,
    Text,
    EllipseText,
    TrapText,
    EdgeText,
    Click,
    CallbackName,
    CallbackArgs,
    ShapeData,
    ShapeDataStr,
}

fn is_node_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '"' | '#' | '$' | '%' | '&' | '\'' | '*' | '+' | '.' | '`' | '?' | '\\' | '_' | '/'
        )
}

const DIRECTIONS: [&str; 5] = ["TB", "BT", "RL", "LR", "TD"];

// Replaces `/\n\s*/g` with `<br/>`.
fn replace_br(s: &str) -> String {
    if !s.contains('\n') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            out.push_str("<br/>");
            while chars.peek().is_some_and(|&n| is_space(n)) {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

struct Lexer<'a> {
    src: &'a [char],
    n: usize,
    out: Tokens,
    stack: Vec<S>,
    state: S,
    p: usize,
    line: usize,
    col: usize,
    first_graph: bool,
    has_direction: bool,
    has_at: bool,
    dir_failed: usize,
    run_end: usize,
    run_at: Option<usize>,
}

impl Lexer<'_> {
    fn at(&self, i: usize) -> char {
        self.src.get(i).copied().unwrap_or('\0')
    }

    fn slice(&self, s: usize, e: usize) -> String {
        self.src[s..e].iter().collect()
    }

    // Moves over one match. Jison counts \r\n, \r and \n as one line break each, and after a
    // break its column stops at the first U+2028 or U+2029.
    fn advance(&mut self, e: usize) {
        let mut brk: Option<usize> = None;
        let mut k = self.p;
        while k < e {
            let ch = self.src[k];
            if ch == '\n' {
                self.line += 1;
                brk = Some(k);
            } else if ch == '\r' {
                if k + 1 < e && self.src[k + 1] == '\n' {
                    k += 1;
                }
                self.line += 1;
                brk = Some(k);
            }
            k += 1;
        }
        match brk {
            None => self.col += e - self.p,
            Some(b) => {
                let mut m = b + 1;
                while m < e {
                    let ch = self.src[m];
                    if ch == '\u{2028}' || ch == '\u{2029}' {
                        break;
                    }
                    m += 1;
                }
                self.col = m - b - 1;
            }
        }
        self.p = e;
    }

    fn emit(&mut self, kind: u8, e: usize) {
        let text = self.slice(self.p, e);
        self.out.types.push(kind);
        self.out.texts.push(text);
        self.out.starts.push(self.p);
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
        self.advance(e);
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
    }

    // For a token that cannot hold a line break.
    fn emit_flat(&mut self, kind: u8, e: usize) {
        let text = self.slice(self.p, e);
        self.out.types.push(kind);
        self.out.texts.push(text);
        self.out.starts.push(self.p);
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
        self.col += e - self.p;
        self.p = e;
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
    }

    // For a run that Jison matches one character at a time, where \r\n counts as two line breaks.
    fn emit_chars(&mut self, kind: u8, e: usize) {
        let text = self.slice(self.p, e);
        self.out.types.push(kind);
        self.out.texts.push(text);
        self.out.starts.push(self.p);
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
        for k in self.p..e {
            let ch = self.src[k];
            if ch == '\n' || ch == '\r' {
                self.line += 1;
                self.col = 0;
            } else {
                self.col += 1;
            }
        }
        self.p = e;
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
    }

    fn emit_text(&mut self, kind: u8, text: String, e: usize) {
        self.out.types.push(kind);
        self.out.texts.push(text);
        self.out.starts.push(self.p);
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
        self.advance(e);
        self.out.locs.push(self.line);
        self.out.locs.push(self.col);
    }

    fn push(&mut self, s: S) {
        self.stack.push(self.state);
        self.state = s;
    }

    fn pop(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    fn starts_with(&self, word: &str, at: usize) -> bool {
        let mut i = at;
        for c in word.chars() {
            if i >= self.n || self.src[i] != c {
                return false;
            }
            i += 1;
        }
        true
    }

    fn kw(&self, word: &str) -> bool {
        self.starts_with(word, self.p) && !is_word(self.at(self.p + word.chars().count()))
    }

    fn skip_space(&self, mut q: usize) -> usize {
        while q < self.n && is_space(self.src[q]) {
            q += 1;
        }
        q
    }

    fn blanks(&self, mut from: usize) -> usize {
        while from < self.n && matches!(self.src[from], ' ' | '\t') {
            from += 1;
        }
        from
    }

    fn find(&self, c: char, from: usize) -> usize {
        (from..self.n).find(|&i| self.src[i] == c).unwrap_or(self.n)
    }

    // `word\s*sep\s*`
    fn keyword_then(&self, word: &str, sep: char) -> Option<usize> {
        if !self.starts_with(word, self.p) {
            return None;
        }
        let q = self.skip_space(self.p + word.len());
        (self.at(q) == sep).then(|| self.skip_space(q + 1))
    }

    // `word\s+`
    fn keyword_space(&self, word: &str) -> Option<usize> {
        if !self.starts_with(word, self.p) {
            return None;
        }
        let q = self.p + word.len();
        is_space(self.at(q)).then(|| self.skip_space(q))
    }

    // End of `\s*--+[x>]\s*` starting at q.
    fn arrow_end(&self, q: usize) -> Option<usize> {
        let j = self.skip_space(q);
        if self.at(j) != '-' || self.at(j + 1) != '-' {
            return None;
        }
        let mut k = j + 2;
        while self.at(k) == '-' {
            k += 1;
        }
        if matches!(self.at(k), 'x' | '>') {
            Some(self.skip_space(k + 1))
        } else {
            None
        }
    }

    fn link(&mut self) -> bool {
        let j = self.skip_space(self.p);
        if self.at(j) != '-' {
            return false;
        }
        let d = self.at(j + 1);
        let mut k = j + 2;
        if d == '-' {
            while self.at(k) == '-' {
                k += 1;
            }
            if matches!(self.at(k), 'x' | '>') {
                let e = self.skip_space(k + 1);
                self.emit(tok::LINK, e);
            } else {
                let e = self.skip_space(j + 2);
                self.push(S::EdgeText);
                self.emit(tok::START_LINK, e);
            }
            return true;
        }
        if d == '.' {
            while self.at(k) == '.' {
                k += 1;
            }
            if self.at(k) == '-' {
                let e = self.skip_space(k + 1);
                self.emit(tok::LINK, e);
                return true;
            }
        }
        false
    }

    // `%%` not followed by `{`, through the end of the line.
    fn comment_end(&self, q: usize) -> Option<usize> {
        if self.at(q) != '%' || self.at(q + 1) != '%' || self.at(q + 2) == '{' {
            return None;
        }
        Some(self.find('\n', q + 2))
    }

    fn quote(&mut self) {
        if self.at(self.p + 1) == '`' {
            self.push(S::MdString);
            self.advance(self.p + 2);
        } else {
            self.push(S::String);
            self.advance(self.p + 1);
        }
    }

    fn opener(&mut self, c: char) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        match c {
            '(' => {
                if d == '-' {
                    self.push(S::EllipseText);
                    self.emit_flat(tok::ELLIPSE_START, p + 2);
                } else if d == '[' {
                    self.push(S::Text);
                    self.emit_flat(tok::STADIUMSTART, p + 2);
                } else if d == '(' && self.at(p + 2) == '(' {
                    self.push(S::Text);
                    self.emit_flat(tok::DOUBLECIRCLESTART, p + 3);
                } else {
                    self.push(S::Text);
                    self.emit_flat(tok::PS, p + 1);
                }
                true
            }
            '[' => {
                if d == '[' {
                    self.push(S::Text);
                    self.emit_flat(tok::SUBROUTINESTART, p + 2);
                } else if d == '(' {
                    self.push(S::Text);
                    self.emit_flat(tok::CYLINDERSTART, p + 2);
                } else if d == '/' {
                    self.push(S::TrapText);
                    self.emit_flat(tok::TRAPSTART, p + 2);
                } else if d == '\\' {
                    self.push(S::TrapText);
                    self.emit_flat(tok::INVTRAPSTART, p + 2);
                } else {
                    self.push(S::Text);
                    self.emit_flat(tok::SQS, p + 1);
                }
                true
            }
            '|' => {
                self.push(S::Text);
                self.emit_flat(tok::PIPE, p + 1);
                true
            }
            '{' => {
                self.push(S::Text);
                self.emit_flat(tok::DIAMOND_START, p + 1);
                true
            }
            _ => false,
        }
    }

    fn node_string(&mut self) -> bool {
        let mut q = self.p;
        while q < self.n {
            let ch = self.src[q];
            if ch.is_ascii() && is_node_char(ch) {
                q += 1;
            } else if ch == '-' && q + 1 < self.n {
                if matches!(self.src[q + 1], '>' | '-' | '.') {
                    break;
                }
                q += 1;
            } else {
                break;
            }
        }
        if q == self.p {
            return false;
        }
        self.emit_flat(tok::NODE_STRING, q);
        true
    }

    // `/.*direction\s+XX[^\n]*/y` for each direction in turn.
    fn direction_stmt(&self, line_end: usize) -> Option<(u8, usize)> {
        let p = self.p;
        let word = "direction".len();
        if line_end < p + word {
            return None;
        }
        let candidates: Vec<usize> = (p..=line_end - word)
            .rev()
            .filter(|&s| self.starts_with("direction", s))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        for (i, dir) in DIRECTIONS.iter().enumerate() {
            for &s in &candidates {
                let after = s + word;
                if !is_space(self.at(after)) {
                    continue;
                }
                let q = self.skip_space(after);
                if self.starts_with(dir, q) {
                    return Some((tok::DIRECTION_TB + i as u8, self.find('\n', q + 2)));
                }
            }
        }
        None
    }

    fn initial(&mut self, c: char) -> bool {
        let p = self.p;
        match c {
            'a' => {
                if self.starts_with("acc", p) {
                    if let Some(e) = self.keyword_then("accTitle", ':') {
                        self.push(S::AccTitle);
                        self.emit(tok::ACC_TITLE, e);
                        return true;
                    }
                    if let Some(e) = self.keyword_then("accDescr", ':') {
                        self.push(S::AccDescr);
                        self.emit(tok::ACC_DESCR, e);
                        return true;
                    }
                    if let Some(e) = self.keyword_then("accDescr", '{') {
                        self.push(S::AccDescrMultiline);
                        self.advance(e);
                        return true;
                    }
                }
                if self.kw("agentflow-beta") {
                    if self.first_graph {
                        self.first_graph = false;
                        self.push(S::Dir);
                    }
                    self.emit_flat(tok::GRAPH, p + 14);
                    return true;
                }
            }
            '@' => {
                if self.at(p + 1) == '{' {
                    self.push(S::ShapeData);
                    self.emit_text(tok::SHAPE_DATA, String::new(), p + 2);
                    return true;
                }
            }
            'c' => {
                if let Some(e) = self.keyword_space("call") {
                    self.push(S::CallbackName);
                    self.advance(e);
                    return true;
                }
                if self.kw("classDef") {
                    self.emit_flat(tok::CLASSDEF, p + 8);
                    return true;
                }
                if self.kw("class") {
                    self.emit_flat(tok::CLASS, p + 5);
                    return true;
                }
                if let Some(e) = self.keyword_space("click") {
                    self.push(S::Click);
                    self.advance(e);
                    return true;
                }
                if self.kw("connector") {
                    self.emit_flat(tok::CONNECTOR, p + 9);
                    return true;
                }
            }
            '"' => {
                self.quote();
                return true;
            }
            's' => {
                if self.kw("style") {
                    self.emit_flat(tok::STYLE, p + 5);
                    return true;
                }
            }
            'd' => {
                if self.kw("default") {
                    self.emit_flat(tok::DEFAULT, p + 7);
                    return true;
                }
            }
            'l' => {
                if self.kw("linkStyle") {
                    self.emit_flat(tok::LINKSTYLE, p + 9);
                    return true;
                }
            }
            'i' => {
                if self.kw("interpolate") {
                    self.emit_flat(tok::INTERPOLATE, p + 11);
                    return true;
                }
            }
            'h' => {
                if self.starts_with("href", p) && is_space(self.at(p + 4)) {
                    self.emit(tok::HREF, p + 5);
                    return true;
                }
            }
            'f' => {
                if self.kw("flow") {
                    self.emit_flat(tok::FLOW, p + 4);
                    return true;
                }
            }
            'g' => {
                if self.kw("global") {
                    let e = self.blanks(p + 6);
                    self.emit_flat(tok::GLOBAL, e);
                    return true;
                }
            }
            'e' => {
                if self.kw("end") {
                    let e = self.blanks(p + 3);
                    self.emit_flat(tok::END_KW, e);
                    return true;
                }
            }
            '_' => {
                for target in ["_self", "_blank", "_parent", "_top"] {
                    if self.kw(target) {
                        self.emit_flat(tok::LINK_TARGET, p + target.len());
                        return true;
                    }
                }
            }
            _ => {}
        }

        if self.has_direction && p >= self.dir_failed {
            let line_end = (p..self.n).find(|&i| is_line_end(self.src[i])).unwrap_or(self.n);
            if let Some((kind, e)) = self.direction_stmt(line_end) {
                self.emit(kind, e);
                return true;
            }
            // No direction statement starts anywhere in the rest of this line either.
            self.dir_failed = line_end;
        }

        // A link id is a run of non-space characters up to its last `@` that is not followed by `{` or `"`.
        if self.has_at && c != '"' && !is_space(c) {
            if p >= self.run_end {
                let mut q = p;
                while q < self.n && self.src[q] != '"' && !is_space(self.src[q]) {
                    q += 1;
                }
                self.run_end = q;
                self.run_at = None;
                let top = q.min(self.n - 1);
                for k in (p..top).rev() {
                    if self.src[k] == '@' && !matches!(self.at(k + 1), '{' | '"') {
                        self.run_at = Some(k);
                        break;
                    }
                }
            }
            if let Some(at) = self.run_at.filter(|&at| at > p) {
                self.emit_flat(tok::LINK_ID, at + 1);
                return true;
            }
        }

        if c.is_ascii_digit() {
            let mut q = p + 1;
            while q < self.n && self.src[q].is_ascii_digit() {
                q += 1;
            }
            self.emit_flat(tok::NUM, q);
            return true;
        }

        match c {
            '#' => {
                self.emit_flat(tok::BRKT, p + 1);
                return true;
            }
            ':' => {
                if self.at(p + 1) == ':' && self.at(p + 2) == ':' {
                    self.emit_flat(tok::STYLE_SEPARATOR, p + 3);
                } else {
                    self.emit_flat(tok::COLON, p + 1);
                }
                return true;
            }
            '&' => {
                self.emit_flat(tok::AMP, p + 1);
                return true;
            }
            ';' => {
                self.emit_flat(tok::SEMI, p + 1);
                return true;
            }
            ',' => {
                self.emit_flat(tok::COMMA, p + 1);
                return true;
            }
            '*' => {
                self.emit_flat(tok::MULT, p + 1);
                return true;
            }
            '<' => {
                self.emit_flat(tok::TAGSTART, p + 1);
                return true;
            }
            '-' => {
                if self.link() || self.node_string() {
                    return true;
                }
                self.emit_flat(tok::MINUS, p + 1);
                return true;
            }
            '(' | '|' | '{' => return self.opener(c),
            '[' => {
                if self.at(p + 1) == '|' {
                    self.emit_flat(tok::VERTEX_WITH_PROPS_START, p + 2);
                    return true;
                }
                return self.opener(c);
            }
            '>' => {
                self.push(S::Text);
                self.emit_flat(tok::TAGEND, p + 1);
                return true;
            }
            '^' => {
                self.emit_flat(tok::UP, p + 1);
                return true;
            }
            '\\' => {
                if self.at(p + 1) == '|' {
                    self.emit_flat(tok::SEP, p + 2);
                    return true;
                }
                return self.node_string();
            }
            'v' => {
                if !is_word(self.at(p + 1)) {
                    self.emit_flat(tok::DOWN, p + 1);
                    return true;
                }
                return self.node_string();
            }
            '%' => {
                if let Some(e) = self.comment_end(p) {
                    self.emit(tok::COMMENT, e);
                    return true;
                }
                return self.node_string();
            }
            _ => {}
        }

        if is_space(c) {
            if self.link() {
                return true;
            }
            if matches!(c, ' ' | '\t') {
                if let Some(e) = self.comment_end(self.blanks(p)) {
                    self.emit(tok::COMMENT, e);
                    return true;
                }
            }
            let mut e = p;
            loop {
                let ch = self.at(e);
                if ch == '\n' {
                    e += 1;
                } else if ch == '\r' && self.at(e + 1) == '\n' {
                    e += 2;
                } else {
                    break;
                }
            }
            if e > p {
                self.emit(tok::NEWLINE, e);
            } else {
                self.emit(tok::SPACE, p + 1);
            }
            return true;
        }

        if c.is_ascii() {
            return self.node_string();
        }
        if is_unicode_letter(c) {
            self.emit_flat(tok::UNICODE_TEXT, p + 1);
            return true;
        }
        false
    }

    fn step(&mut self, c: char) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        match self.state {
            S::Initial => self.initial(c),

            S::String => {
                if c == '"' {
                    if d == '`' {
                        self.push(S::MdString);
                        self.advance(p + 2);
                    } else {
                        self.pop();
                        self.advance(p + 1);
                    }
                } else {
                    let e = self.find('"', p);
                    self.emit(tok::STR, e);
                }
                true
            }

            S::MdString => {
                if c == '`' {
                    if d == '"' {
                        self.pop();
                        self.advance(p + 2);
                        true
                    } else {
                        false
                    }
                } else if c == '"' {
                    self.quote();
                    true
                } else {
                    let mut q = p + 1;
                    while q < self.n && self.src[q] != '`' && self.src[q] != '"' {
                        q += 1;
                    }
                    self.emit(tok::MD_STR, q);
                    true
                }
            }

            S::AccTitle | S::AccDescr => {
                let e = self.find('\n', p);
                let kind = if self.state == S::AccTitle {
                    tok::ACC_TITLE_VALUE
                } else {
                    tok::ACC_DESCR_VALUE
                };
                self.pop();
                self.emit(kind, e);
                true
            }

            S::AccDescrMultiline => {
                if c == '}' {
                    self.pop();
                    self.advance(p + 1);
                } else {
                    let e = self.find('}', p);
                    self.emit(tok::ACC_DESCR_MULTILINE_VALUE, e);
                }
                true
            }

            S::Dir => {
                if c == '"' {
                    self.quote();
                    return true;
                }
                // `(?:\r?\n)*\s*\n`: up to the last `\n` in the whitespace run starting at `p`.
                let ws_end = self.skip_space(p);
                if let Some(last) = (p..ws_end).rev().find(|&i| self.src[i] == '\n') {
                    self.pop();
                    self.emit(tok::NODIR, last + 1);
                    return true;
                }
                let mut end = None;
                for dir in ["LR", "RL", "TB", "BT", "TD", "BR"] {
                    if self.starts_with(dir, ws_end) && !is_word(self.at(ws_end + 2)) {
                        end = Some(ws_end + 2);
                        break;
                    }
                }
                if end.is_none() && matches!(self.at(ws_end), '<' | '>' | '^') {
                    end = Some(ws_end + 1);
                }
                if end.is_none() && self.at(ws_end) == 'v' && !is_word(self.at(ws_end + 1)) {
                    end = Some(ws_end + 1);
                }
                match end {
                    Some(e) => {
                        self.pop();
                        self.emit(tok::DIR, e);
                        true
                    }
                    None => self.opener(c),
                }
            }

            S::Text => match c {
                '"' => {
                    self.quote();
                    true
                }
                '(' | '[' | '{' => self.opener(c),
                ']' => {
                    self.pop();
                    if d == ')' {
                        self.emit_flat(tok::STADIUMEND, p + 2);
                    } else if d == ']' {
                        self.emit_flat(tok::SUBROUTINEEND, p + 2);
                    } else {
                        self.emit_flat(tok::SQE, p + 1);
                    }
                    true
                }
                ')' => {
                    self.pop();
                    if d == ']' {
                        self.emit_flat(tok::CYLINDEREND, p + 2);
                    } else if d == ')' && self.at(p + 2) == ')' {
                        self.emit_flat(tok::DOUBLECIRCLEEND, p + 3);
                    } else {
                        self.emit_flat(tok::PE, p + 1);
                    }
                    true
                }
                '|' => {
                    self.pop();
                    self.emit_flat(tok::PIPE, p + 1);
                    true
                }
                '}' => {
                    self.pop();
                    self.emit_flat(tok::DIAMOND_STOP, p + 1);
                    true
                }
                '%' => {
                    // Jison drops the lone `%` from Mermaid's text pattern, so only a comment may start with one.
                    match self.comment_end(p) {
                        Some(e) => {
                            self.advance(e);
                            true
                        }
                        None => false,
                    }
                }
                _ => {
                    let mut q = p + 1;
                    while q < self.n {
                        if matches!(
                            self.src[q],
                            '[' | ']' | '(' | ')' | '{' | '}' | '|' | '"' | '%'
                        ) {
                            break;
                        }
                        q += 1;
                    }
                    self.emit(tok::TEXT, q);
                    true
                }
            },

            S::EllipseText => {
                if c == '"' {
                    self.quote();
                    true
                } else if c == '%' && self.comment_end(p).is_some() {
                    let e = self.comment_end(p).unwrap();
                    self.advance(e);
                    true
                } else if matches!(c, '-' | '/' | ')') && d == ')' {
                    self.pop();
                    self.emit_flat(tok::ELLIPSE_END, p + 2);
                    true
                } else if matches!(c, '(' | '[' | '{') {
                    self.opener(c)
                } else if matches!(c, ')' | ']' | '}') {
                    false
                } else {
                    let mut q = p + 1;
                    while q < self.n {
                        let ch = self.src[q];
                        if matches!(ch, '"' | '(' | ')' | '[' | ']' | '{' | '}') {
                            break;
                        }
                        if matches!(ch, '-' | '/') && self.at(q + 1) == ')' {
                            break;
                        }
                        if ch == '%' && self.at(q + 1) == '%' && self.at(q + 2) != '{' {
                            break;
                        }
                        q += 1;
                    }
                    self.emit_chars(tok::TEXT, q);
                    true
                }
            }

            S::TrapText => {
                if c == '"' {
                    self.quote();
                    return true;
                }
                match c {
                    '(' => {
                        if d == ']' && !(matches!(d, '-' | '[') || (d == '(' && self.at(p + 2) == '(')) {
                            self.pop();
                            self.emit_flat(tok::TRAPEND, p + 2);
                            true
                        } else {
                            self.opener(c)
                        }
                    }
                    '[' | '{' => self.opener(c),
                    '\\' => {
                        if d == ']' {
                            self.pop();
                            self.emit_flat(tok::TRAPEND, p + 2);
                        } else {
                            self.emit_flat(tok::TEXT, p + 1);
                        }
                        true
                    }
                    '/' => {
                        if d == ']' {
                            self.pop();
                            self.emit_flat(tok::INVTRAPEND, p + 2);
                        } else {
                            self.emit_flat(tok::TEXT, p + 1);
                        }
                        true
                    }
                    ']' | ')' => {
                        if d == ']' {
                            self.pop();
                            self.emit_flat(tok::TRAPEND, p + 2);
                            true
                        } else {
                            false
                        }
                    }
                    '}' => false,
                    '%' => match self.comment_end(p) {
                        Some(e) => {
                            self.advance(e);
                            true
                        }
                        None => false,
                    },
                    _ => {
                        if matches!(c, '?' | '=') && d == ']' {
                            self.pop();
                            self.emit_flat(tok::TRAPEND, p + 2);
                            return true;
                        }
                        let mut q = p + 1;
                        while q < self.n {
                            if matches!(
                                self.src[q],
                                '\\' | '[' | ']' | '(' | ')' | '{' | '}' | '/' | '%'
                            ) {
                                break;
                            }
                            q += 1;
                        }
                        self.emit(tok::TEXT, q);
                        true
                    }
                }
            }

            S::EdgeText => {
                if c == '"' {
                    self.quote();
                    return true;
                }
                if let Some(e) = self.arrow_end(p) {
                    self.pop();
                    self.emit(tok::LINK, e);
                    return true;
                }
                if c == '-' && d == '-' {
                    return false;
                }
                let mut q = p + 1;
                while q < self.n {
                    let ch = self.src[q];
                    if ch == '"' {
                        break;
                    }
                    if ch == '-' {
                        if self.at(q + 1) == '-' {
                            break;
                        }
                    } else if is_space(ch) && self.arrow_end(q).is_some() {
                        break;
                    }
                    q += 1;
                }
                self.emit_chars(tok::EDGE_TEXT, q);
                true
            }

            S::Click => {
                if c == '"' {
                    self.quote();
                } else if is_space(c) {
                    self.pop();
                    self.advance(p + 1);
                } else {
                    let mut q = p + 1;
                    while q < self.n && !is_space(self.src[q]) {
                        q += 1;
                    }
                    self.emit_flat(tok::CLICK, q);
                }
                true
            }

            S::CallbackName => {
                if c == '(' {
                    let q = self.skip_space(p + 1);
                    self.pop();
                    if self.at(q) == ')' {
                        self.advance(q + 1);
                    } else {
                        self.push(S::CallbackArgs);
                        self.advance(p + 1);
                    }
                } else {
                    let e = self.find('(', p);
                    self.emit(tok::CALLBACKNAME, e);
                }
                true
            }

            S::CallbackArgs => {
                if c == ')' {
                    self.pop();
                    self.advance(p + 1);
                } else {
                    let e = self.find(')', p);
                    self.emit(tok::CALLBACKARGS, e);
                }
                true
            }

            S::ShapeData => {
                if c == '"' {
                    self.push(S::ShapeDataStr);
                    self.emit_flat(tok::SHAPE_DATA, p + 1);
                    true
                } else if c == '}' {
                    self.pop();
                    self.advance(p + 1);
                    true
                } else if c == '^' {
                    false
                } else {
                    let mut q = p + 1;
                    while q < self.n && !matches!(self.src[q], '}' | '^' | '"') {
                        q += 1;
                    }
                    self.emit(tok::SHAPE_DATA, q);
                    true
                }
            }

            S::ShapeDataStr => {
                if c == '"' {
                    self.pop();
                    self.emit_flat(tok::SHAPE_DATA, p + 1);
                } else {
                    let e = self.find('"', p);
                    let replaced = replace_br(&self.slice(p, e));
                    self.emit_text(tok::SHAPE_DATA, replaced, e);
                }
                true
            }
        }
    }
}

pub fn tokenize(text: &str, src: &[char]) -> Tokens {
    let n = src.len();
    let mut lexer = Lexer {
        src,
        n,
        out: Tokens::default(),
        stack: Vec::new(),
        state: S::Initial,
        p: 0,
        line: 1,
        col: 0,
        first_graph: true,
        has_direction: text.contains("direction"),
        has_at: text.contains('@'),
        dir_failed: 0,
        run_end: 0,
        run_at: None,
    };

    loop {
        if lexer.p >= n {
            let eof_kind = match lexer.state {
                S::Initial => Some(tok::EOF),
                S::AccTitle => Some(tok::ACC_TITLE_VALUE),
                S::AccDescr => Some(tok::ACC_DESCR_VALUE),
                S::AccDescrMultiline => Some(tok::ACC_DESCR_MULTILINE_VALUE),
                S::Click => Some(tok::CLICK),
                S::CallbackName => Some(tok::CALLBACKNAME),
                S::CallbackArgs => Some(tok::CALLBACKARGS),
                _ => None,
            };
            if let Some(kind) = eof_kind {
                lexer.emit_flat(kind, n);
            }
            break;
        }
        let c = src[lexer.p];
        if !lexer.step(c) {
            lexer.out.types.push(tok::ERROR);
            lexer.out.texts.push(String::new());
            lexer.out.starts.push(lexer.p);
            lexer
                .out
                .locs
                .extend([lexer.line, lexer.col, lexer.line, lexer.col]);
            return lexer.out;
        }
    }

    lexer.out.types.push(tok::END);
    lexer.out.texts.push(String::new());
    lexer.out.starts.push(n);
    lexer
        .out
        .locs
        .extend([lexer.line, lexer.col, lexer.line, lexer.col]);
    lexer.out
}
