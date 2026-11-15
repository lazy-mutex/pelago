use crate::chars::{is_space, is_unicode_letter, is_word};

// Token types, numbered as in Mermaid's grammar.
pub mod tok {
    pub const END: u8 = 0;
    pub const ERROR: u8 = 1;
    pub const ACC_TITLE: u8 = 2;
    pub const ACC_TITLE_VALUE: u8 = 3;
    pub const ACC_DESCR: u8 = 4;
    pub const ACC_DESCR_VALUE: u8 = 5;
    pub const ACC_DESCR_MULTILINE_VALUE: u8 = 6;
    pub const SHAPE_DATA: u8 = 7;
    pub const CALLBACKNAME: u8 = 8;
    pub const CALLBACKARGS: u8 = 9;
    pub const MD_STR: u8 = 10;
    pub const STR: u8 = 11;
    pub const STYLE: u8 = 12;
    pub const DEFAULT: u8 = 13;
    pub const LINKSTYLE: u8 = 14;
    pub const INTERPOLATE: u8 = 15;
    pub const CLASSDEF: u8 = 16;
    pub const CLASS: u8 = 17;
    pub const HREF: u8 = 18;
    pub const CLICK: u8 = 19;
    pub const GRAPH: u8 = 20;
    pub const SUBGRAPH: u8 = 21;
    pub const END_KW: u8 = 22;
    pub const LINK_TARGET: u8 = 23;
    pub const NODIR: u8 = 24;
    pub const DIR: u8 = 25;
    pub const DIRECTION_TB: u8 = 26;
    pub const DIRECTION_TD: u8 = 30;
    pub const LINK_ID: u8 = 31;
    pub const NUM: u8 = 32;
    pub const BRKT: u8 = 33;
    pub const STYLE_SEPARATOR: u8 = 34;
    pub const COLON: u8 = 35;
    pub const AMP: u8 = 36;
    pub const SEMI: u8 = 37;
    pub const COMMA: u8 = 38;
    pub const MULT: u8 = 39;
    pub const LINK: u8 = 40;
    pub const START_LINK: u8 = 41;
    pub const EDGE_TEXT: u8 = 42;
    pub const ELLIPSE_END: u8 = 43;
    pub const TEXT: u8 = 44;
    pub const ELLIPSE_START: u8 = 45;
    pub const STADIUMEND: u8 = 46;
    pub const STADIUMSTART: u8 = 47;
    pub const SUBROUTINEEND: u8 = 48;
    pub const SUBROUTINESTART: u8 = 49;
    pub const VERTEX_WITH_PROPS_START: u8 = 50;
    pub const TAGEND: u8 = 51;
    pub const CYLINDEREND: u8 = 52;
    pub const CYLINDERSTART: u8 = 53;
    pub const DOUBLECIRCLEEND: u8 = 54;
    pub const DOUBLECIRCLESTART: u8 = 55;
    pub const TRAPEND: u8 = 56;
    pub const INVTRAPEND: u8 = 57;
    pub const TRAPSTART: u8 = 58;
    pub const INVTRAPSTART: u8 = 59;
    pub const TAGSTART: u8 = 60;
    pub const UP: u8 = 61;
    pub const SEP: u8 = 62;
    pub const DOWN: u8 = 63;
    pub const NODE_STRING: u8 = 64;
    pub const MINUS: u8 = 65;
    pub const UNICODE_TEXT: u8 = 66;
    pub const PIPE: u8 = 67;
    pub const PE: u8 = 68;
    pub const PS: u8 = 69;
    pub const SQE: u8 = 70;
    pub const SQS: u8 = 71;
    pub const DIAMOND_STOP: u8 = 72;
    pub const DIAMOND_START: u8 = 73;
    pub const NEWLINE: u8 = 74;
    pub const SPACE: u8 = 75;
    pub const EOF: u8 = 76;
}

pub const TOKEN_NAMES: [&str; 77] = [
    "$end", "INVALID", "acc_title", "acc_title_value", "acc_descr", "acc_descr_value", "acc_descr_multiline_value", "SHAPE_DATA", "CALLBACKNAME",
    "CALLBACKARGS", "MD_STR", "STR", "STYLE", "DEFAULT", "LINKSTYLE", "INTERPOLATE", "CLASSDEF", "CLASS", "HREF", "CLICK", "GRAPH", "subgraph", "end",
    "LINK_TARGET", "NODIR", "DIR", "direction_tb", "direction_bt", "direction_rl", "direction_lr", "direction_td", "LINK_ID", "NUM", "BRKT",
    "STYLE_SEPARATOR", "COLON", "AMP", "SEMI", "COMMA", "MULT", "LINK", "START_LINK", "EDGE_TEXT", "-)", "TEXT", "(-", "STADIUMEND", "STADIUMSTART",
    "SUBROUTINEEND", "SUBROUTINESTART", "VERTEX_WITH_PROPS_START", "TAGEND", "CYLINDEREND", "CYLINDERSTART", "DOUBLECIRCLEEND", "DOUBLECIRCLESTART",
    "TRAPEND", "INVTRAPEND", "TRAPSTART", "INVTRAPSTART", "TAGSTART", "UP", "SEP", "DOWN", "NODE_STRING", "MINUS", "UNICODE_TEXT", "PIPE", "PE", "PS",
    "SQE", "SQS", "DIAMOND_STOP", "DIAMOND_START", "NEWLINE", "SPACE", "EOF",
];

#[derive(Default)]
pub struct Tokens {
    pub types: Vec<u8>,
    pub texts: Vec<String>,
    // Where each token starts, in characters.
    pub starts: Vec<usize>,
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
    ThickEdgeText,
    DottedEdgeText,
    Click,
    CallbackName,
    CallbackArgs,
    ShapeData,
    ShapeDataStr,
}

fn node_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!\"#$%&'*+.`?\\_/".contains(c)
}

fn is_line_end(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

const DIRECTIONS: [&str; 5] = ["TB", "BT", "RL", "LR", "TD"];

struct Lexer<'a> {
    src: &'a [char],
    n: usize,
    out: Tokens,
    stack: Vec<S>,
    state: S,
    p: usize,
    first_graph: bool,
    has_direction: bool,
    has_at: bool,
    run_end: usize,
    run_at: Option<usize>,
    dir_failed: usize,
}

impl Lexer<'_> {
    fn at(&self, i: usize) -> char {
        self.src.get(i).copied().unwrap_or('\0')
    }

    fn slice(&self, s: usize, e: usize) -> String {
        self.src[s..e].iter().collect()
    }

    fn emit(&mut self, kind: u8, s: usize, e: usize) {
        self.out.types.push(kind);
        self.out.texts.push(self.slice(s, e));
        self.out.starts.push(s);
        self.p = e;
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
            if self.at(i) != c || i >= self.n {
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

    // First character after any run of whitespace starting at q, skipping one optional x, o or <.
    fn link_char(&self, q: usize) -> char {
        let j = self.skip_space(q);
        let d = self.at(j);
        if matches!(d, 'x' | 'o' | '<') {
            self.at(j + 1)
        } else {
            d
        }
    }

    // The start of a link's line after `\s*[xo<]?`.
    fn link_start(&self, q: usize) -> usize {
        let j = self.skip_space(q);
        if matches!(self.at(j), 'x' | 'o' | '<') {
            j + 1
        } else {
            j
        }
    }

    // /\s*[xo<]?--+[-xo>]\s*/ and its thick form with `=`.
    fn full_link(&self, q: usize, line: char) -> Option<usize> {
        let start = self.link_start(q);
        let mut j = start;
        while self.at(j) == line {
            j += 1;
        }
        let count = j - start;
        if count >= 2 && matches!(self.at(j), 'x' | 'o' | '>') {
            Some(self.skip_space(j + 1))
        } else if count >= 3 {
            Some(self.skip_space(j))
        } else {
            None
        }
    }

    // /\s*[xo<]?--\s*/ and its thick form.
    fn start_link(&self, q: usize, line: char) -> Option<usize> {
        let start = self.link_start(q);
        (self.at(start) == line && self.at(start + 1) == line).then(|| self.skip_space(start + 2))
    }

    // /\s*[xo<]?-?\.+-[xo>]?\s*/
    fn dotted_link(&self, q: usize) -> Option<usize> {
        let mut j = self.link_start(q);
        if self.at(j) == '-' {
            j += 1;
        }
        let dots = j;
        while self.at(j) == '.' {
            j += 1;
        }
        if j == dots || self.at(j) != '-' {
            return None;
        }
        j += 1;
        if matches!(self.at(j), 'x' | 'o' | '>') {
            j += 1;
        }
        Some(self.skip_space(j))
    }

    // /\s*[xo<]?-\.\s*/
    fn dotted_start(&self, q: usize) -> Option<usize> {
        let start = self.link_start(q);
        (self.at(start) == '-' && self.at(start + 1) == '.').then(|| self.skip_space(start + 2))
    }

    // /\s*~~~+\s*/
    fn invis_at(&self, q: usize) -> Option<usize> {
        let start = self.skip_space(q);
        let mut j = start;
        while self.at(j) == '~' {
            j += 1;
        }
        (j - start >= 3).then(|| self.skip_space(j))
    }

    fn link_at(&self, q: usize, which: S) -> Option<usize> {
        match which {
            S::EdgeText => self.full_link(q, '-'),
            S::ThickEdgeText => self.full_link(q, '='),
            _ => self.dotted_link(q),
        }
    }

    fn quote(&mut self) {
        if self.at(self.p + 1) == '`' {
            self.push(S::MdString);
            self.p += 2;
        } else {
            self.push(S::String);
            self.p += 1;
        }
    }

    fn invis_link(&mut self) -> bool {
        match self.invis_at(self.p) {
            Some(e) => {
                self.emit(tok::LINK, self.p, e);
                true
            }
            None => false,
        }
    }

    fn open(&mut self, state: S, kind: u8, len: usize) -> bool {
        self.push(state);
        self.emit(kind, self.p, self.p + len);
        true
    }

    fn opener(&mut self, c: char) -> bool {
        let d = self.at(self.p + 1);
        match c {
            '(' => match d {
                '-' => self.open(S::EllipseText, tok::ELLIPSE_START, 2),
                '[' => self.open(S::Text, tok::STADIUMSTART, 2),
                '(' if self.at(self.p + 2) == '(' => self.open(S::Text, tok::DOUBLECIRCLESTART, 3),
                _ => self.open(S::Text, tok::PS, 1),
            },
            '[' => match d {
                '[' => self.open(S::Text, tok::SUBROUTINESTART, 2),
                '(' => self.open(S::Text, tok::CYLINDERSTART, 2),
                '/' => self.open(S::TrapText, tok::TRAPSTART, 2),
                '\\' => self.open(S::TrapText, tok::INVTRAPSTART, 2),
                _ => self.open(S::Text, tok::SQS, 1),
            },
            '|' => self.open(S::Text, tok::PIPE, 1),
            '{' => self.open(S::Text, tok::DIAMOND_START, 1),
            _ => false,
        }
    }

    fn graph(&mut self, len: usize) -> bool {
        if self.first_graph {
            self.first_graph = false;
            self.push(S::Dir);
        }
        self.emit(tok::GRAPH, self.p, self.p + len);
        true
    }

    fn word(&mut self, word: &str, kind: u8) -> bool {
        if self.kw(word) {
            self.emit(kind, self.p, self.p + word.len());
            true
        } else {
            false
        }
    }

    fn node_string(&mut self) -> bool {
        let mut q = self.p;
        while q < self.n {
            let ch = self.src[q];
            if node_char(ch) {
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
        self.emit(tok::NODE_STRING, self.p, q);
        true
    }

    fn link(&mut self, c: char) -> bool {
        let p = self.p;
        match self.link_char(p) {
            '-' => {
                if let Some(e) = self.full_link(p, '-') {
                    self.emit(tok::LINK, p, e);
                    return true;
                }
                if let Some(e) = self.start_link(p, '-') {
                    self.push(S::EdgeText);
                    self.emit(tok::START_LINK, p, e);
                    return true;
                }
                if let Some(e) = self.dotted_link(p) {
                    self.emit(tok::LINK, p, e);
                    return true;
                }
                if let Some(e) = self.dotted_start(p) {
                    self.push(S::DottedEdgeText);
                    self.emit(tok::START_LINK, p, e);
                    return true;
                }
            }
            '=' => {
                if let Some(e) = self.full_link(p, '=') {
                    self.emit(tok::LINK, p, e);
                    return true;
                }
                if let Some(e) = self.start_link(p, '=') {
                    self.push(S::ThickEdgeText);
                    self.emit(tok::START_LINK, p, e);
                    return true;
                }
            }
            '.' => {
                if let Some(e) = self.dotted_link(p) {
                    self.emit(tok::LINK, p, e);
                    return true;
                }
            }
            _ => {}
        }
        !matches!(c, '-' | '=' | '.') && self.invis_link()
    }

    // /.*direction\s+XX[^\n]*/ for each direction in turn: the kind of statement and where it ends.
    fn direction(&self) -> Option<(u8, usize)> {
        let p = self.p;
        let line_end = (p..self.n).find(|&i| is_line_end(self.src[i])).unwrap_or(self.n);
        let word = "direction".len();
        let candidates: Vec<usize> = (p..line_end.saturating_sub(word - 1)).rev().filter(|&s| self.starts_with("direction", s)).collect();
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
            'a' if self.starts_with("acc", p) => {
                if let Some(e) = self.keyword_then("accTitle", ':') {
                    self.push(S::AccTitle);
                    self.emit(tok::ACC_TITLE, p, e);
                    return true;
                }
                if let Some(e) = self.keyword_then("accDescr", ':') {
                    self.push(S::AccDescr);
                    self.emit(tok::ACC_DESCR, p, e);
                    return true;
                }
                if let Some(e) = self.keyword_then("accDescr", '{') {
                    self.push(S::AccDescrMultiline);
                    self.p = e;
                    return true;
                }
            }
            '@' if self.at(p + 1) == '{' => {
                self.push(S::ShapeData);
                self.out.types.push(tok::SHAPE_DATA);
                self.out.texts.push(String::new());
                self.out.starts.push(p);
                self.p += 2;
                return true;
            }
            'c' => {
                if let Some(e) = self.keyword_space("call") {
                    self.push(S::CallbackName);
                    self.p = e;
                    return true;
                }
                if self.word("classDef", tok::CLASSDEF) || self.word("class", tok::CLASS) {
                    return true;
                }
                if let Some(e) = self.keyword_space("click") {
                    self.push(S::Click);
                    self.p = e;
                    return true;
                }
            }
            '"' => {
                self.quote();
                return true;
            }
            's' => {
                if self.word("style", tok::STYLE) {
                    return true;
                }
                if self.kw("swimlane-beta") {
                    return self.graph(13);
                }
                if self.word("subgraph", tok::SUBGRAPH) {
                    return true;
                }
            }
            'd' if self.word("default", tok::DEFAULT) => return true,
            'l' if self.word("linkStyle", tok::LINKSTYLE) => return true,
            'i' if self.word("interpolate", tok::INTERPOLATE) => return true,
            'h' if self.starts_with("href", p) && is_space(self.at(p + 4)) => {
                self.emit(tok::HREF, p, p + 5);
                return true;
            }
            'f' => {
                if self.kw("flowchart-elk") {
                    return self.graph(13);
                }
                if self.kw("flowchart") {
                    return self.graph(9);
                }
            }
            'g' if self.kw("graph") => return self.graph(5),
            'e' if self.kw("end") => {
                let e = self.skip_space(p + 3);
                self.emit(tok::END_KW, p, e);
                return true;
            }
            '_' => {
                for target in ["_self", "_blank", "_parent", "_top"] {
                    if self.word(target, tok::LINK_TARGET) {
                        return true;
                    }
                }
            }
            _ => {}
        }

        if self.has_direction && p >= self.dir_failed {
            if let Some((kind, e)) = self.direction() {
                self.emit(kind, p, e);
                return true;
            }
            // No direction statement starts anywhere in the rest of this line either.
            self.dir_failed = (p..self.n).find(|&i| is_line_end(self.src[i])).unwrap_or(self.n);
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
                self.emit(tok::LINK_ID, p, at + 1);
                return true;
            }
        }

        if c.is_ascii_digit() {
            let mut q = p + 1;
            while q < self.n && self.src[q].is_ascii_digit() {
                q += 1;
            }
            self.emit(tok::NUM, p, q);
            return true;
        }

        let single = |lexer: &mut Self, kind: u8, len: usize| {
            lexer.emit(kind, p, p + len);
            true
        };
        match c {
            '#' => return single(self, tok::BRKT, 1),
            ':' => {
                return if self.at(p + 1) == ':' && self.at(p + 2) == ':' { single(self, tok::STYLE_SEPARATOR, 3) } else { single(self, tok::COLON, 1) };
            }
            '&' => return single(self, tok::AMP, 1),
            ';' => return single(self, tok::SEMI, 1),
            ',' => return single(self, tok::COMMA, 1),
            '*' => return single(self, tok::MULT, 1),
            'x' | 'o' | '.' => return self.link(c) || self.node_string(),
            '<' => return self.link(c) || single(self, tok::TAGSTART, 1),
            '-' => return self.link(c) || self.node_string() || single(self, tok::MINUS, 1),
            '=' => return self.link(c),
            '~' => return self.invis_link(),
            '(' | '|' | '{' => return self.opener(c),
            '[' => {
                return if self.at(p + 1) == '|' { single(self, tok::VERTEX_WITH_PROPS_START, 2) } else { self.opener(c) };
            }
            '>' => return self.open(S::Text, tok::TAGEND, 1),
            '^' => return single(self, tok::UP, 1),
            '\\' => return if self.at(p + 1) == '|' { single(self, tok::SEP, 2) } else { self.node_string() },
            'v' => return if !is_word(self.at(p + 1)) { single(self, tok::DOWN, 1) } else { self.node_string() },
            _ => {}
        }

        if is_space(c) {
            if self.link(c) {
                return true;
            }
            // /(?:\r?\n)+/
            let mut e = p;
            loop {
                if self.at(e) == '\n' {
                    e += 1;
                } else if self.at(e) == '\r' && self.at(e + 1) == '\n' {
                    e += 2;
                } else {
                    break;
                }
            }
            if e > p {
                self.emit(tok::NEWLINE, p, e);
                return true;
            }
            // /[^\S\n\r]+/
            let mut e = p;
            while e < self.n && is_space(self.src[e]) && self.src[e] != '\n' && self.src[e] != '\r' {
                e += 1;
            }
            if e > p {
                self.emit(tok::SPACE, p, e);
                return true;
            }
            return false;
        }

        if c.is_ascii() {
            return self.node_string();
        }
        if is_unicode_letter(c) {
            return single(self, tok::UNICODE_TEXT, 1);
        }
        false
    }

    fn edge_text(&mut self, c: char, which: S) -> bool {
        let (a, b) = match which {
            S::EdgeText => ('-', '\0'),
            S::ThickEdgeText => ('=', '\0'),
            _ => ('.', '-'),
        };
        let p = self.p;
        if c == '"' {
            self.quote();
            return true;
        }
        let d = self.link_char(p);
        if d == a || (b != '\0' && d == b) {
            if let Some(e) = self.link_at(p, which) {
                self.pop();
                self.emit(tok::LINK, p, e);
                return true;
            }
        }
        if c == a && (a != '-' || self.at(p + 1) == '-') {
            return false;
        }
        let mut q = p + 1;
        while q < self.n {
            let ch = self.src[q];
            if ch == '"' {
                break;
            }
            if ch == a {
                if a != '-' || self.at(q + 1) == '-' {
                    break;
                }
            } else if matches!(ch, 'x' | 'o' | '<') || (b != '\0' && ch == b) || is_space(ch) {
                let dd = self.link_char(q);
                if (dd == a || (b != '\0' && dd == b)) && self.link_at(q, which).is_some() {
                    break;
                }
            }
            q += 1;
        }
        self.emit(tok::EDGE_TEXT, p, q);
        true
    }

    // Emits text up to the first of `stops`.
    fn text_until(&mut self, stops: &[char]) {
        let mut q = self.p + 1;
        while q < self.n && !stops.contains(&self.src[q]) {
            q += 1;
        }
        self.emit(tok::TEXT, self.p, q);
    }

    fn close(&mut self, kind: u8, len: usize) {
        self.pop();
        self.emit(kind, self.p, self.p + len);
    }

    // One step in the current state. False when the text cannot be read.
    fn step(&mut self, c: char) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        match self.state {
            S::Initial => return self.initial(c),

            S::String => {
                if c == '"' {
                    if d == '`' {
                        self.push(S::MdString);
                        self.p += 2;
                    } else {
                        self.pop();
                        self.p += 1;
                    }
                } else {
                    let e = self.find('"', p);
                    self.emit(tok::STR, p, e);
                }
            }

            S::MdString => {
                if c == '`' {
                    if d != '"' {
                        return false;
                    }
                    self.pop();
                    self.p += 2;
                } else if c == '"' {
                    self.quote();
                } else {
                    let mut q = p + 1;
                    while q < self.n && self.src[q] != '`' && self.src[q] != '"' {
                        q += 1;
                    }
                    self.emit(tok::MD_STR, p, q);
                }
            }

            S::AccTitle | S::AccDescr => {
                let e = self.find('\n', p);
                let kind = if self.state == S::AccTitle { tok::ACC_TITLE_VALUE } else { tok::ACC_DESCR_VALUE };
                self.pop();
                self.emit(kind, p, e);
            }

            S::AccDescrMultiline => {
                if c == '}' {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.find('}', p);
                    self.emit(tok::ACC_DESCR_MULTILINE_VALUE, p, e);
                }
            }

            S::Dir => {
                if c == '"' {
                    self.quote();
                    return true;
                }
                // /(?:\r?\n)*\s*\n/: up to the last line break in the whitespace that follows.
                let ws_end = self.skip_space(p);
                if let Some(last) = (p..ws_end).rev().find(|&i| self.src[i] == '\n') {
                    self.pop();
                    self.emit(tok::NODIR, p, last + 1);
                    return true;
                }
                let boundary = |lexer: &Self, e: usize| !is_word(lexer.at(e));
                let mut end = None;
                for dir in ["LR", "RL", "TB", "BT", "TD", "BR"] {
                    if self.starts_with(dir, ws_end) && boundary(self, ws_end + 2) {
                        end = Some(ws_end + 2);
                        break;
                    }
                }
                if end.is_none() && matches!(self.at(ws_end), '<' | '>' | '^') {
                    end = Some(ws_end + 1);
                }
                if end.is_none() && self.at(ws_end) == 'v' && boundary(self, ws_end + 1) {
                    end = Some(ws_end + 1);
                }
                match end {
                    Some(e) => {
                        self.pop();
                        self.emit(tok::DIR, p, e);
                    }
                    None => return self.invis_link() || self.opener(c),
                }
            }

            S::Text => match c {
                '"' => self.quote(),
                '(' | '[' | '{' => {
                    self.opener(c);
                }
                ']' => match d {
                    ')' => self.close(tok::STADIUMEND, 2),
                    ']' => self.close(tok::SUBROUTINEEND, 2),
                    _ => self.close(tok::SQE, 1),
                },
                ')' => {
                    if d == ']' {
                        self.close(tok::CYLINDEREND, 2);
                    } else if d == ')' && self.at(p + 2) == ')' {
                        self.close(tok::DOUBLECIRCLEEND, 3);
                    } else {
                        self.close(tok::PE, 1);
                    }
                }
                '|' => self.close(tok::PIPE, 1),
                '}' => self.close(tok::DIAMOND_STOP, 1),
                _ => {
                    if (c == '~' || is_space(c)) && self.invis_link() {
                        return true;
                    }
                    self.text_until(&['[', ']', '(', ')', '{', '}', '|', '"']);
                }
            },

            S::EllipseText => {
                if c == '"' {
                    self.quote();
                } else if (c == '~' || is_space(c)) && self.invis_link() {
                } else if matches!(c, '-' | '/' | ')') && d == ')' {
                    self.close(tok::ELLIPSE_END, 2);
                } else if matches!(c, '(' | '[' | '{') {
                    self.opener(c);
                } else if matches!(c, ')' | ']' | '}') {
                    return false;
                } else {
                    let mut q = p + 1;
                    while q < self.n {
                        let ch = self.src[q];
                        if matches!(ch, '"' | '(' | ')' | '[' | ']' | '{' | '}') {
                            break;
                        }
                        if (ch == '-' || ch == '/') && self.at(q + 1) == ')' {
                            break;
                        }
                        if (ch == '~' || is_space(ch)) && self.invis_at(q).is_some() {
                            break;
                        }
                        q += 1;
                    }
                    self.emit(tok::TEXT, p, q);
                }
            }

            S::TrapText => {
                if c == '"' {
                    self.quote();
                    return true;
                }
                if (c == '~' || is_space(c)) && self.invis_link() {
                    return true;
                }
                match c {
                    '(' => {
                        if d == ']' {
                            self.close(tok::TRAPEND, 2);
                        } else {
                            self.opener(c);
                        }
                    }
                    '[' | '{' => {
                        self.opener(c);
                    }
                    '\\' => {
                        if d == ']' {
                            self.close(tok::TRAPEND, 2);
                        } else {
                            self.emit(tok::TEXT, p, p + 1);
                        }
                    }
                    '/' => {
                        if d == ']' {
                            self.close(tok::INVTRAPEND, 2);
                        } else {
                            self.emit(tok::TEXT, p, p + 1);
                        }
                    }
                    ']' | ')' => {
                        if d != ']' {
                            return false;
                        }
                        self.close(tok::TRAPEND, 2);
                    }
                    '}' => return false,
                    _ => {
                        if (c == '?' || c == '=') && d == ']' {
                            self.close(tok::TRAPEND, 2);
                        } else {
                            self.text_until(&['\\', '[', ']', '(', ')', '{', '}', '/']);
                        }
                    }
                }
            }

            S::EdgeText | S::ThickEdgeText | S::DottedEdgeText => return self.edge_text(c, self.state),

            S::Click => {
                if c == '"' {
                    self.quote();
                } else if is_space(c) {
                    self.pop();
                    self.p += 1;
                } else {
                    let mut q = p + 1;
                    while q < self.n && !is_space(self.src[q]) {
                        q += 1;
                    }
                    self.emit(tok::CLICK, p, q);
                }
            }

            S::CallbackName => {
                if c == '(' {
                    // /\(\s*\)/
                    let q = self.skip_space(p + 1);
                    self.pop();
                    if self.at(q) == ')' {
                        self.p = q + 1;
                    } else {
                        self.push(S::CallbackArgs);
                        self.p += 1;
                    }
                } else {
                    let e = self.find('(', p);
                    self.emit(tok::CALLBACKNAME, p, e);
                }
            }

            S::CallbackArgs => {
                if c == ')' {
                    self.pop();
                    self.p += 1;
                } else {
                    let e = self.find(')', p);
                    self.emit(tok::CALLBACKARGS, p, e);
                }
            }

            S::ShapeData => {
                if c == '"' {
                    self.push(S::ShapeDataStr);
                    self.emit(tok::SHAPE_DATA, p, p + 1);
                } else if c == '}' {
                    self.pop();
                    self.p += 1;
                } else if c == '^' {
                    return false;
                } else {
                    let mut q = p + 1;
                    while q < self.n && !matches!(self.src[q], '}' | '^' | '"') {
                        q += 1;
                    }
                    self.emit(tok::SHAPE_DATA, p, q);
                }
            }

            S::ShapeDataStr => {
                if c == '"' {
                    self.pop();
                    self.emit(tok::SHAPE_DATA, p, p + 1);
                } else {
                    let e = self.find('"', p);
                    // A line break in a quoted value, with the indentation after it, is a <br/>.
                    let mut text = String::new();
                    let mut i = p;
                    while i < e {
                        if self.src[i] == '\n' {
                            text.push_str("<br/>");
                            i += 1;
                            while i < e && is_space(self.src[i]) {
                                i += 1;
                            }
                        } else {
                            text.push(self.src[i]);
                            i += 1;
                        }
                    }
                    self.out.types.push(tok::SHAPE_DATA);
                    self.out.texts.push(text);
                    self.out.starts.push(p);
                    self.p = e;
                }
            }
        }
        true
    }
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let has = |needle: &str| {
        let needle: Vec<char> = needle.chars().collect();
        src.windows(needle.len()).any(|w| w == needle.as_slice())
    };
    let mut lexer = Lexer {
        src,
        n,
        out: Tokens::default(),
        stack: Vec::new(),
        state: S::Initial,
        p: 0,
        first_graph: true,
        has_direction: has("direction"),
        has_at: src.contains(&'@'),
        run_end: 0,
        run_at: None,
        dir_failed: 0,
    };
    loop {
        if lexer.p >= n {
            let kind = match lexer.state {
                S::Initial => Some(tok::EOF),
                S::AccTitle => Some(tok::ACC_TITLE_VALUE),
                S::AccDescr => Some(tok::ACC_DESCR_VALUE),
                S::AccDescrMultiline => Some(tok::ACC_DESCR_MULTILINE_VALUE),
                S::Click => Some(tok::CLICK),
                S::CallbackName => Some(tok::CALLBACKNAME),
                S::CallbackArgs => Some(tok::CALLBACKARGS),
                _ => None,
            };
            if let Some(kind) = kind {
                lexer.emit(kind, n, n);
            }
            break;
        }
        let c = src[lexer.p];
        if !lexer.step(c) {
            lexer.out.types.push(tok::ERROR);
            lexer.out.texts.push(String::new());
            lexer.out.starts.push(lexer.p);
            return lexer.out;
        }
    }
    lexer.out.types.push(tok::END);
    lexer.out.texts.push(String::new());
    lexer.out.starts.push(n);
    lexer.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<&'static str> {
        let chars: Vec<char> = src.chars().collect();
        tokenize(&chars).types.iter().map(|&t| TOKEN_NAMES[t as usize]).collect()
    }

    #[test]
    fn simple_edge() {
        assert_eq!(
            kinds("flowchart TD\n  A[Hi] --> B{Q?}\n"),
            ["GRAPH", "DIR", "NEWLINE", "SPACE", "NODE_STRING", "SQS", "TEXT", "SQE", "LINK", "NODE_STRING", "DIAMOND_START", "TEXT", "DIAMOND_STOP", "NEWLINE", "EOF", "$end"]
        );
    }

    #[test]
    fn labelled_edges() {
        assert_eq!(kinds("graph LR\nA -- Yes --> B\n")[3..8], ["NODE_STRING", "START_LINK", "EDGE_TEXT", "LINK", "NODE_STRING"]);
        assert_eq!(kinds("graph LR\nA -->|No| B\n")[3..9], ["NODE_STRING", "LINK", "PIPE", "TEXT", "PIPE", "SPACE"]);
        assert_eq!(kinds("graph LR\nA -.-> B ==> C ~~~ D\n")[3..10], ["NODE_STRING", "LINK", "NODE_STRING", "LINK", "NODE_STRING", "LINK", "NODE_STRING"]);
    }

    #[test]
    fn shapes() {
        assert_eq!(kinds("graph\nA([x]) --> B[(y)] --> C((z))\n")[2..6], ["NODE_STRING", "STADIUMSTART", "TEXT", "STADIUMEND"]);
    }
}
