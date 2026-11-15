use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UnsupportedDiagram,
    Syntax,
    Semantic,
    Limit,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UnsupportedDiagram => "unsupported-diagram",
            ErrorCode::Syntax => "syntax",
            ErrorCode::Semantic => "semantic",
            ErrorCode::Limit => "limit",
        }
    }
}

// Input character range [from, to) replaced by `length` output characters.
pub type Edit = (usize, usize, usize);

fn step_input(edits: &[Edit], at: usize) -> usize {
    let mut shift: isize = 0;
    for &(from, to, length) in edits {
        let start = (from as isize - shift) as usize;
        if at < start {
            break;
        }
        if at < start + length {
            return from + (at - start).min((to - from).saturating_sub(1));
        }
        shift += (to - from) as isize - length as isize;
    }
    (at as isize + shift).max(0) as usize
}

pub fn input_offset(steps: &[Vec<Edit>], mut at: usize) -> usize {
    for edits in steps.iter().rev() {
        at = step_input(edits, at);
    }
    at
}

#[derive(Debug, Clone)]
pub struct PeleError {
    pub message: String,
    pub code: ErrorCode,
    pub diagram: Option<&'static str>,
    pub line: usize,
    pub column: usize,
    pub snippet: String,
    pub(crate) raw_offset: Option<usize>,
    pub(crate) detail: String,
    pub(crate) lexical: bool,
}

impl PeleError {
    pub fn new(message: impl Into<String>, code: ErrorCode) -> Self {
        PeleError {
            message: message.into(),
            code,
            diagram: None,
            line: 0,
            column: 0,
            snippet: String::new(),
            raw_offset: None,
            detail: String::new(),
            lexical: false,
        }
    }

    pub fn of(mut self, diagram: &'static str) -> Self {
        self.diagram = Some(diagram);
        self
    }

    pub(crate) fn relocate(&mut self, parsed: &[char], orig: &[char], steps: &[Vec<Edit>]) {
        let Some(offset) = self.raw_offset else { return };
        let mut end = parsed.len();
        while end > 0 && parsed[end - 1].is_whitespace() {
            end -= 1;
        }
        let mapped = if end > 0 && offset >= end {
            input_offset(steps, end - 1) + 1
        } else {
            input_offset(steps, offset)
        };
        let loc = locate_at(orig, mapped);
        self.line = loc.line;
        self.column = loc.column;
        self.snippet = loc.snippet;
        self.message = format_syntax_message(self.line, &self.snippet, &self.detail, self.lexical);
    }
}

impl fmt::Display for PeleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PeleError {}

pub type Result<T> = std::result::Result<T, PeleError>;

pub struct Location {
    pub line: usize,
    pub column: usize,
    pub snippet: String,
}

fn locate_at(src: &[char], offset: usize) -> Location {
    let offset = offset.min(src.len());
    let mut line = 1;
    let mut line_start = 0;
    for (i, &c) in src.iter().enumerate() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let mut line_end = offset;
    while line_end < src.len() && src[line_end] != '\n' {
        line_end += 1;
    }
    let column = offset - line_start + 1;
    let text: String = src[line_start..line_end.max(line_start)].iter().collect();
    Location { line, column, snippet: format!("{}\n{}^", text, "-".repeat(column - 1)) }
}

// `offset` counts characters of `src`. Positions are those of the text handed to the parser.
pub fn locate(src: &[char], mut offset: usize) -> Location {
    // Keep EOF errors on the last content line.
    let mut end = src.len();
    while end > 0 && src[end - 1].is_whitespace() {
        end -= 1;
    }
    if end > 0 && offset >= end {
        offset = end;
    }
    locate_at(src, offset)
}

fn format_syntax_message(line: usize, snippet: &str, detail: &str, lexical: bool) -> String {
    if lexical {
        format!("Lexical error on line {}. Unrecognized text.\n{}", line, snippet)
    } else {
        format!("Parse error on line {}:\n{}\n{}", line, snippet, detail)
    }
}

pub fn syntax_error(diagram: &'static str, src: &[char], offset: usize, detail: &str, lexical: bool) -> PeleError {
    let loc = locate(src, offset);
    let message = format_syntax_message(loc.line, &loc.snippet, detail, lexical);
    PeleError {
        message,
        code: ErrorCode::Syntax,
        diagram: Some(diagram),
        line: loc.line,
        column: loc.column,
        snippet: loc.snippet,
        raw_offset: Some(offset),
        detail: detail.to_string(),
        lexical,
    }
}
