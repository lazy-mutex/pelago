use super::tokens::t;
use crate::chars::{is_digit, is_space, is_word};

pub struct Tokens {
    pub types: Vec<usize>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
enum S {
    INITIAL,
    ID,
    ALIAS,
    LINE,
    CONFIG,
    acc_title,
    acc_descr,
    acc_descr_multiline,
}

const C_ACTOR_START: u8 = 1;
const C_ACTOR_REST: u8 = 2;
const C_ID: u8 = 4;
const C_ID_WIDE: u8 = 8;
const C_LINE: u8 = 16;
const C_MENU: u8 = 32;

// Character classes of Mermaid's lexer rules, for ASCII. Other characters are tested by is_space.
const fn build_class() -> [u8; 128] {
    let mut table = [0u8; 128];
    let mut c = 0usize;
    while c < 128 {
        let ch = c as u8;
        let ws = ch == b' ' || (ch >= 9 && ch <= 13);
        let mut bits = 0u8;
        if !matches!(
            ch,
            b'/' | b'\\' | b'+' | b'(' | b')' | b'<' | b'-' | b'>' | b':' | b'\n' | b',' | b';'
        ) {
            bits |= C_ACTOR_START;
        }
        if !matches!(ch, b'+' | b'<' | b'-' | b'>' | b':' | b'\n' | b',' | b';') {
            bits |= C_ACTOR_REST;
        }
        if !matches!(ch, b'<' | b'>' | b':' | b'\n' | b',' | b';' | b'@') {
            bits |= C_ID_WIDE | if ws { 0 } else { C_ID };
        }
        if !matches!(ch, b'#' | b'\n' | b';') {
            bits |= C_LINE;
        }
        if !matches!(
            ch,
            b'/' | b'\\' | b'+' | b'(' | b')' | b'<' | b'>' | b':' | b'\n' | b',' | b';' | b'-'
        ) && !ws
        {
            bits |= C_MENU;
        }
        table[c] = bits;
        c += 1;
    }
    table
}

const CLASS: [u8; 128] = build_class();

struct Lexer<'a> {
    src: &'a [char],
    n: usize,
    types: Vec<usize>,
    texts: Vec<String>,
    starts: Vec<usize>,
    stack: Vec<S>,
    state: S,
    p: usize,
    next_nl: isize,
}

impl<'a> Lexer<'a> {
    fn at(&self, q: usize) -> Option<char> {
        self.src.get(q).copied()
    }

    fn slice(&self, s: usize, e: usize) -> String {
        self.src[s..e].iter().collect()
    }

    fn emit(&mut self, kind: usize, s: usize, e: usize) {
        let text = self.slice(s, e);
        self.types.push(kind);
        self.texts.push(text);
        self.starts.push(s);
        self.p = e;
    }

    fn emit_text(&mut self, kind: usize, s: usize, e: usize, text: String) {
        self.types.push(kind);
        self.texts.push(text);
        self.starts.push(s);
        self.p = e;
    }

    fn push(&mut self, s: S) {
        self.stack.push(self.state);
        self.state = s;
    }

    fn pop(&mut self) {
        if let Some(prev) = self.stack.pop() {
            self.state = prev;
        }
    }

    fn eol(&mut self, q: usize) -> usize {
        if self.next_nl < q as isize {
            self.next_nl = match self.src[q..].iter().position(|&c| c == '\n') {
                Some(rel) => (q + rel) as isize,
                None => self.n as isize,
            };
        }
        self.next_nl as usize
    }

    fn has(&self, q: usize, bit: u8) -> bool {
        let Some(c) = self.at(q) else {
            return false;
        };
        if (c as u32) < 128 {
            (CLASS[c as usize] & bit) != 0
        } else if bit == C_ID || bit == C_MENU {
            !is_space(c)
        } else {
            true
        }
    }

    // Matches a lowercase literal without regard to the case of the source.
    fn ci(&self, q: usize, word: &str) -> bool {
        let wb = word.as_bytes();
        if q + wb.len() > self.n {
            return false;
        }
        for (k, &w) in wb.iter().enumerate() {
            let c = self.src[q + k] as u32;
            let matched = if w.is_ascii_lowercase() {
                c < 128 && ((c as u8) | 32) == w
            } else {
                c == w as u32
            };
            if !matched {
                return false;
            }
        }
        true
    }

    fn starts_with(&self, q: usize, lit: &str) -> bool {
        let lb = lit.as_bytes();
        if q + lb.len() > self.n {
            return false;
        }
        lb.iter()
            .enumerate()
            .all(|(k, &b)| self.src[q + k] == b as char)
    }

    fn word(&self, lit: &str) -> bool {
        self.ci(self.p, lit) && !self.at(self.p + lit.len()).is_some_and(is_word)
    }

    fn skip_ws(&self, mut q: usize) -> usize {
        while q < self.n && is_space(self.src[q]) {
            q += 1;
        }
        q
    }

    fn skip_same_line(&mut self) -> bool {
        let mut q = self.p;
        while q < self.n {
            let c = self.src[q];
            if c == '\n' || !is_space(c) {
                break;
            }
            q += 1;
        }
        if q == self.p {
            return false;
        }
        self.p = q;
        true
    }

    fn line_run(&self, mut q: usize) -> usize {
        while q < self.n && self.has(q, C_LINE) {
            q += 1;
        }
        q
    }

    fn begin(&mut self, kind: usize, len: usize, next: S) {
        self.emit(kind, self.p, self.p + len);
        self.push(next);
    }

    fn keyword(&mut self, lit: &str, kind: usize, next: Option<S>) -> bool {
        if !self.word(lit) {
            return false;
        }
        self.emit(kind, self.p, self.p + lit.len());
        if let Some(s) = next {
            self.push(s);
        }
        true
    }

    // The actor menu keywords count only when spaces and the start of a name follow.
    fn menu(&mut self, lit: &str, kind: usize) -> bool {
        if !self.ci(self.p, lit) {
            return false;
        }
        let mut q = self.p + lit.len();
        let Some(first) = self.at(q) else {
            return false;
        };
        if first != ' ' && first != '\t' {
            return false;
        }
        while self.at(q).is_some_and(|c| c == ' ' || c == '\t') {
            q += 1;
        }
        if q >= self.n || !self.has(q, C_MENU) {
            return false;
        }
        self.emit(kind, self.p, self.p + lit.len());
        true
    }

    fn arrow_ahead(&self, q: usize) -> bool {
        let Some(c) = self.at(q) else {
            return false;
        };
        let d = self.at(q + 1);
        if c == '-' {
            if matches!(
                d,
                Some('x' | 'X' | ')' | '\\' | '/' | '-')
            ) {
                return true;
            }
            return d == Some('|') && matches!(self.at(q + 2), Some('\\' | '/'));
        }
        if c == '/' || c == '\\' {
            return (d == Some('|') || d == Some(c)) && self.at(q + 2) == Some('-');
        }
        c == '(' && d == Some(')')
    }

    fn actor(&mut self) {
        let mut q = self.p + 1;
        while q < self.n && self.has(q, C_ACTOR_START) {
            q += 1;
        }
        while q < self.n && !self.arrow_ahead(q) {
            let mut r = q;
            while self.at(r) == Some('-') {
                r += 1;
            }
            if r >= self.n || !self.has(r, C_ACTOR_REST) {
                break;
            }
            r += 1;
            while r < self.n && self.has(r, C_ACTOR_REST) {
                r += 1;
            }
            q = r;
        }
        let trimmed = self.slice(self.p, q).trim_matches(is_space).to_string();
        self.emit_text(t::ACTOR, self.p, q, trimmed);
    }

    fn number(&mut self) -> bool {
        let mut q = self.p;
        while self.at(q).is_some_and(is_digit) {
            q += 1;
        }
        let whole = q > self.p;
        if self.at(q) == Some('.') && self.at(q + 1).is_some_and(is_digit) {
            let r = if self.at(q + 2).is_some_and(is_digit) {
                q + 3
            } else {
                q + 2
            };
            if matches!(self.at(r), Some(' ' | '\n')) {
                self.emit(t::NUM, self.p, r);
                return true;
            }
        }
        if whole && matches!(self.at(q), Some(' ' | '\n')) {
            self.emit(t::NUM, self.p, q);
            return true;
        }
        false
    }

    fn dash(&mut self) {
        let p = self.p;
        let d = self.at(p + 1);
        let e = self.at(p + 2);
        if d == Some('>') {
            if e == Some('>') {
                self.emit(t::SOLID_ARROW, p, p + 3);
            } else {
                self.emit(t::SOLID_OPEN_ARROW, p, p + 2);
            }
        } else if matches!(d, Some('x' | 'X')) {
            self.emit(t::SOLID_CROSS, p, p + 2);
        } else if d == Some(')') {
            self.emit(t::SOLID_POINT, p, p + 2);
        } else if d == Some('|') && e == Some('\\') {
            self.emit(t::SOLID_ARROW_TOP, p, p + 3);
        } else if d == Some('|') && e == Some('/') {
            self.emit(t::SOLID_ARROW_BOTTOM, p, p + 3);
        } else if d == Some('\\') && e == Some('\\') {
            self.emit(t::STICK_ARROW_TOP, p, p + 3);
        } else if d == Some('/') && e == Some('/') {
            self.emit(t::STICK_ARROW_BOTTOM, p, p + 3);
        } else if d == Some('-') {
            let f = self.at(p + 3);
            if e == Some('>') {
                if f == Some('>') {
                    self.emit(t::DOTTED_ARROW, p, p + 4);
                } else {
                    self.emit(t::DOTTED_OPEN_ARROW, p, p + 3);
                }
            } else if matches!(e, Some('x' | 'X')) {
                self.emit(t::DOTTED_CROSS, p, p + 3);
            } else if e == Some(')') {
                self.emit(t::DOTTED_POINT, p, p + 3);
            } else if e == Some('|') && f == Some('\\') {
                self.emit(t::SOLID_ARROW_TOP_DOTTED, p, p + 4);
            } else if e == Some('|') && f == Some('/') {
                self.emit(t::SOLID_ARROW_BOTTOM_DOTTED, p, p + 4);
            } else if e == Some('\\') && f == Some('\\') {
                self.emit(t::STICK_ARROW_TOP_DOTTED, p, p + 4);
            } else if e == Some('/') && f == Some('/') {
                self.emit(t::STICK_ARROW_BOTTOM_DOTTED, p, p + 4);
            } else {
                self.emit(t::MINUS, p, p + 1);
            }
        } else {
            self.emit(t::MINUS, p, p + 1);
        }
    }

    fn reverse(&mut self, c: char) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        if (d != Some('|') && d != Some(c)) || self.at(p + 2) != Some('-') {
            return false;
        }
        let dotted = self.at(p + 3) == Some('-');
        let top = c == '/';
        let kind = if d == Some('|') {
            if dotted {
                if top {
                    t::SOLID_ARROW_TOP_REVERSE_DOTTED
                } else {
                    t::SOLID_ARROW_BOTTOM_REVERSE_DOTTED
                }
            } else if top {
                t::SOLID_ARROW_TOP_REVERSE
            } else {
                t::SOLID_ARROW_BOTTOM_REVERSE
            }
        } else if dotted {
            if top {
                t::STICK_ARROW_TOP_REVERSE_DOTTED
            } else {
                t::STICK_ARROW_BOTTOM_REVERSE_DOTTED
            }
        } else if top {
            t::STICK_ARROW_TOP_REVERSE
        } else {
            t::STICK_ARROW_BOTTOM_REVERSE
        };
        self.emit(kind, p, p + if dotted { 4 } else { 3 });
        true
    }

    fn title(&mut self) -> bool {
        if !self.ci(self.p, "title") {
            return false;
        }
        let mut q = self.p + 5;
        let legacy = self.at(q) == Some(':');
        if legacy {
            q += 1;
        }
        let mut e = q;
        while e < self.n && self.src[e] != '\n' && is_space(self.src[e]) {
            e += 1;
        }
        if e == self.n || self.src[e] == '\n' {
            self.p = e;
            return true;
        }
        let Some(sep) = self.at(q) else {
            return false;
        };
        if !is_space(sep) || sep == '\r' || !self.has(q + 1, C_LINE) {
            return false;
        }
        let end = self.line_run(q + 1);
        self.emit(if legacy { t::legacy_title } else { t::title }, self.p, end);
        true
    }

    fn acc(&mut self) -> bool {
        let descr = self.ci(self.p, "accdescr");
        if !descr && !self.ci(self.p, "acctitle") {
            return false;
        }
        let q = self.skip_ws(self.p + 8);
        let c = self.at(q);
        if c == Some(':') {
            let end = self.skip_ws(q + 1);
            self.emit(if descr { t::acc_descr } else { t::acc_title }, self.p, end);
            self.push(if descr { S::acc_descr } else { S::acc_title });
            return true;
        }
        if descr && c == Some('{') {
            self.p = self.skip_ws(q + 1);
            self.push(S::acc_descr_multiline);
            return true;
        }
        false
    }

    fn keywords(&mut self, c: u8) -> bool {
        match c | 32 {
            b'a' => {
                self.keyword("actor", t::participant_actor, Some(S::ID))
                    || self.keyword("alt", t::alt, Some(S::LINE))
                    || self.keyword("and", t::and, Some(S::LINE))
                    || self.keyword("activate", t::activate, Some(S::ID))
                    || self.acc()
                    || self.keyword("autonumber", t::autonumber, None)
            }
            b'b' => {
                self.keyword("box", t::r#box, Some(S::LINE))
                    || self.keyword("break", t::r#break, Some(S::LINE))
            }
            b'c' => {
                self.keyword("create", t::create, None)
                    || self.keyword("critical", t::critical, Some(S::LINE))
            }
            b'd' => {
                self.keyword("destroy", t::destroy, Some(S::ID))
                    || self.menu("details", t::details)
                    || self.keyword("deactivate", t::deactivate, Some(S::ID))
            }
            b'e' => {
                self.keyword("else", t::r#else, Some(S::LINE)) || self.keyword("end", t::end, None)
            }
            b'l' => {
                self.keyword("loop", t::r#loop, Some(S::LINE))
                    || self.keyword("left of", t::left_of, None)
                    || self.menu("links", t::links)
                    || self.menu("link", t::link)
            }
            b'n' => self.keyword("note", t::note, None),
            b'o' => {
                self.keyword("opt", t::opt, Some(S::LINE))
                    || self.keyword("option", t::option, Some(S::LINE))
                    || self.keyword("over", t::over, None)
                    || self.keyword("off", t::off, None)
            }
            b'p' => {
                self.keyword("participant", t::participant, Some(S::ID))
                    || self.keyword("par", t::par, Some(S::LINE))
                    || self.keyword("par_over", t::par_over, Some(S::LINE))
                    || self.menu("properties", t::properties)
            }
            b'r' => {
                self.keyword("rect", t::rect, Some(S::LINE))
                    || self.keyword("right of", t::right_of, None)
            }
            b's' => self.keyword("sequencediagram", t::SD, None),
            b't' => self.title(),
            _ => false,
        }
    }

    fn initial(&mut self) {
        let p = self.p;
        let c = self.src[p];
        if c == '\n' {
            let mut q = p + 1;
            while self.at(q) == Some('\n') {
                q += 1;
            }
            self.emit(t::NEWLINE, p, q);
            return;
        }
        if is_space(c) {
            self.p = self.skip_ws(p + 1);
            return;
        }
        if c == '#' || (c == '%' && self.at(p + 1) != Some('{')) {
            self.p = self.eol(p);
            return;
        }
        if (is_digit(c) || c == '.') && self.number() {
            return;
        }
        if (c as u32) < 128 {
            let lower = (c as u8) | 32;
            if lower.is_ascii_lowercase() && self.keywords(c as u8) {
                return;
            }
        }
        if c == ',' {
            self.emit(t::COMMA, p, p + 1);
        } else if c == ';' {
            self.emit(t::NEWLINE, p, p + 1);
        } else if self.has(p, C_ACTOR_START) {
            self.actor();
        } else if c == '-' {
            self.dash();
        } else if c == '<' && self.starts_with(p, "<<->>") {
            self.emit(t::BIDIRECTIONAL_SOLID_ARROW, p, p + 5);
        } else if c == '<' && self.starts_with(p, "<<-->>") {
            self.emit(t::BIDIRECTIONAL_DOTTED_ARROW, p, p + 6);
        } else if (c == '/' || c == '\\') && self.reverse(c) {
            return;
        } else if c == ':' {
            let end = self.line_run(p + 1);
            self.emit(t::TXT, p, end);
        } else if c == '+' {
            self.emit(t::PLUS, p, p + 1);
        } else if c == '(' && self.at(p + 1) == Some(')') {
            self.emit(t::CENTRAL, p, p + 2);
        } else {
            self.emit(t::INVALID, p, p + 1);
        }
    }

    // A participant name, up to an alias, a config object, or the end of the statement.
    fn id(&mut self) -> bool {
        if self.skip_same_line() {
            return true;
        }
        let p = self.p;
        let c = self.src[p];
        if c == '#' {
            self.p = self.eol(p);
            return true;
        }
        if c == '@' && self.at(p + 1) == Some('{') {
            self.begin(t::CONFIG_START, 2, S::CONFIG);
            return true;
        }
        let mut a = p;
        while a < self.n && self.has(a, C_ID) {
            a += 1;
        }
        if a > p {
            let w = self.skip_ws(a);
            if self.at(w) == Some('@') && self.at(w + 1) == Some('{') {
                self.emit(t::ACTOR, p, a);
                return true;
            }
            if w > a && self.ci(w, "as") && self.at(w + 2).is_some_and(is_space) {
                self.emit(t::ACTOR, p, a);
                self.push(S::ALIAS);
                return true;
            }
        }
        let mut b = a;
        while b < self.n && self.has(b, C_ID_WIDE) {
            b += 1;
        }
        if b > p {
            let end = self.at(b);
            // Before any other delimiter the name must be followed by a comment, so it ends at the last one.
            let mut cut = b;
            if b < self.n && end != Some('\n') && end != Some(';') {
                cut -= 1;
                while cut > p && self.src[cut] != '#' {
                    cut -= 1;
                }
            }
            if cut > p {
                let trimmed = self.slice(p, cut).trim_matches(is_space).to_string();
                self.emit_text(t::ACTOR, p, cut, trimmed);
                self.pop();
                return true;
            }
        }
        if self.at(b) == Some('<') {
            let end = self.eol(p);
            self.emit(t::INVALID, p, end);
            self.pop();
            return true;
        }
        if c != '\n' {
            let e = self.eol(p);
            let trimmed = self.slice(p, e).trim_matches(is_space).to_string();
            self.emit_text(t::INVALID, p, e, trimmed);
            self.pop();
            return true;
        }
        false
    }

    fn step(&mut self) -> bool {
        match self.state {
            S::INITIAL => {
                self.initial();
                true
            }
            S::ID => self.id(),
            S::ALIAS => {
                if self.skip_same_line() {
                    return true;
                }
                if self.src[self.p] == '#' {
                    self.p = self.eol(self.p);
                    return true;
                }
                self.pop();
                self.pop();
                if self.word("as") {
                    self.begin(t::AS, 2, S::LINE);
                } else {
                    self.emit(t::NEWLINE, self.p, self.p);
                }
                true
            }
            S::LINE => {
                if self.skip_same_line() {
                    return true;
                }
                if self.src[self.p] == '#' {
                    self.p = self.eol(self.p);
                    return true;
                }
                let end = self.line_run(self.p);
                self.emit(t::restOfLine, self.p, end);
                self.pop();
                true
            }
            S::CONFIG => {
                let p = self.p;
                if self.src[p] != '}' {
                    let q = self.src[p..]
                        .iter()
                        .position(|&c| c == '}')
                        .map_or(self.n, |rel| p + rel);
                    self.emit(t::CONFIG_CONTENT, p, q);
                    true
                } else {
                    let w = self.skip_ws(p + 1);
                    let alias =
                        w > p + 1 && self.ci(w, "as") && self.at(w + 2).is_some_and(is_space);
                    self.emit(t::CONFIG_END, p, p + 1);
                    self.pop();
                    if alias {
                        self.push(S::ALIAS);
                    } else {
                        self.pop();
                    }
                    true
                }
            }
            S::acc_title | S::acc_descr => {
                let kind = if self.state == S::acc_title {
                    t::acc_title_value
                } else {
                    t::acc_descr_value
                };
                let end = self.eol(self.p);
                self.emit(kind, self.p, end);
                self.pop();
                true
            }
            S::acc_descr_multiline => {
                let p = self.p;
                if self.src[p] == '}' {
                    self.p += 1;
                    self.pop();
                } else {
                    let q = self.src[p..]
                        .iter()
                        .position(|&c| c == '}')
                        .map_or(self.n, |rel| p + rel);
                    self.emit(t::acc_descr_multiline_value, p, q);
                }
                true
            }
        }
    }
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut lex = Lexer {
        src,
        n,
        types: Vec::new(),
        texts: Vec::new(),
        starts: Vec::new(),
        stack: Vec::new(),
        state: S::INITIAL,
        p: 0,
        next_nl: -1,
    };

    while lex.p < n {
        if !lex.step() {
            lex.types.push(t::ERROR);
            lex.texts.push(String::new());
            lex.starts.push(lex.p);
            return Tokens {
                types: lex.types,
                texts: lex.texts,
                starts: lex.starts,
            };
        }
    }

    // Mermaid's lexer tries its rules once more on the empty remainder before reporting the end.
    match lex.state {
        S::INITIAL | S::ALIAS => lex.emit(t::NEWLINE, n, n),
        S::LINE => lex.emit(t::restOfLine, n, n),
        S::acc_title => lex.emit(t::acc_title_value, n, n),
        S::acc_descr => lex.emit(t::acc_descr_value, n, n),
        S::acc_descr_multiline => lex.emit(t::acc_descr_multiline_value, n, n),
        S::ID | S::CONFIG => {}
    }
    lex.types.push(t::END);
    lex.texts.push(String::new());
    lex.starts.push(n);
    Tokens {
        types: lex.types,
        texts: lex.texts,
        starts: lex.starts,
    }
}
