use super::tokens::t;
use crate::chars::{is_blank, is_digit, is_hex, is_letter, is_space};
use crate::errors::{ErrorCode, PeleError, Result};

#[derive(Debug, Clone)]
pub struct Tokens {
    pub types: Vec<usize>,
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
}

const KEYWORD_LENGTH: [usize; 21] = [0, 12, 5, 14, 3, 9, 2, 2, 2, 2, 2, 4, 3, 4, 8, 5, 5, 7, 6, 4, 5];

fn is_word_ascii(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_css_punct(c: char) -> bool {
    matches!(
        c,
        '!' | '#' | '$' | '&' | '*' | '+' | '/' | '=' | '?' | '^' | '|' | '~'
    )
}

fn starts_with(src: &[char], at: usize, word: &str) -> bool {
    if at + word.len() > src.len() {
        return false;
    }
    for (k, ch) in word.chars().enumerate() {
        if src[at + k] != ch {
            return false;
        }
    }
    true
}

// Mermaid's INCLUDE and EXTEND are the only keywords matched without regard to case.
fn starts_with_no_case(src: &[char], at: usize, word: &str) -> bool {
    if at + word.len() > src.len() {
        return false;
    }
    for (k, ch) in word.chars().enumerate() {
        if src[at + k].to_ascii_lowercase() != ch {
            return false;
        }
    }
    true
}

// The keyword that is a prefix of the text at `at`, tried in the order of Mermaid's token list.
fn keyword_at(src: &[char], at: usize, c: char) -> usize {
    match c {
        'a' => {
            if starts_with(src, at, "actor") {
                t::ACTOR
            } else {
                0
            }
        }
        's' => {
            if starts_with(src, at, "systemBoundary") {
                t::SYSTEM_BOUNDARY
            } else if starts_with(src, at, "style") {
                t::STYLE
            } else {
                0
            }
        }
        'e' => {
            if starts_with(src, at, "end") {
                t::END
            } else if starts_with_no_case(src, at, "extend") {
                t::EXTEND
            } else {
                0
            }
        }
        'E' => {
            if starts_with_no_case(src, at, "extend") {
                t::EXTEND
            } else {
                0
            }
        }
        'd' => {
            if starts_with(src, at, "direction") {
                t::DIRECTION
            } else {
                0
            }
        }
        'T' => {
            if starts_with(src, at, "TD") {
                t::TD
            } else if starts_with(src, at, "TB") {
                t::TB
            } else {
                0
            }
        }
        'B' => {
            if starts_with(src, at, "BT") {
                t::BT
            } else {
                0
            }
        }
        'L' => {
            if starts_with(src, at, "LR") {
                t::LR
            } else {
                0
            }
        }
        'R' => {
            if starts_with(src, at, "RL") {
                t::RL
            } else {
                0
            }
        }
        'n' => {
            if starts_with(src, at, "note") {
                t::NOTE
            } else {
                0
            }
        }
        'f' => {
            if starts_with(src, at, "for") {
                t::FOR
            } else if starts_with(src, at, "false") {
                t::FALSE
            } else {
                0
            }
        }
        'j' => {
            if starts_with(src, at, "json") {
                t::JSON
            } else {
                0
            }
        }
        'c' => {
            if starts_with(src, at, "classDef") {
                t::CLASS_DEF
            } else if starts_with(src, at, "class") {
                t::CLASS
            } else {
                0
            }
        }
        'i' | 'I' => {
            if starts_with_no_case(src, at, "include") {
                t::INCLUDE
            } else {
                0
            }
        }
        't' => {
            if starts_with(src, at, "true") {
                t::TRUE
            } else {
                0
            }
        }
        'u' => {
            if starts_with(src, at, "usecase-beta") {
                t::USECASE
            } else {
                0
            }
        }
        _ => 0,
    }
}

// Maps an offset in `src` (with entity placeholders `ﬂ°...¶ß`) to `(line, column, utf16_offset)`
// in the text with `#code;` restored, matching Pele's `written` + `lineColumn` + `locationText`.
fn written_metrics(src: &[char], offset: usize, line_offset: usize) -> (usize, usize, usize) {
    let target = offset.min(src.len());
    let mut line = 1 + line_offset;
    let mut col = 1usize;
    let mut utf16 = 0usize;
    let mut i = 0usize;
    while i < target {
        let c = src[i];
        // Entity placeholder `ﬂ°name¶ß` or `ﬂ°°digits¶ß` came from `#name;` / `#digits;`.
        if c == 'ﬂ' && src.get(i + 1) == Some(&'°') {
            if let Some(rel_stop) = src[i + 2..]
                .iter()
                .position(|&ch| ch == '¶')
                .filter(|&rel| src.get(i + 2 + rel + 1) == Some(&'ß'))
            {
                let stop = i + 2 + rel_stop;
                let numeric = src.get(i + 2) == Some(&'°');
                let name_start = i + if numeric { 3 } else { 2 };
                let written_len = 2 + (stop - name_start); // `#` + name + `;`
                let end_placeholder = stop + 2;
                if target < end_placeholder {
                    let inside = (target - i).min(written_len.saturating_sub(1));
                    col += inside;
                    utf16 += inside;
                    break;
                }
                col += written_len;
                utf16 += written_len;
                i = end_placeholder;
                continue;
            }
        }
        if c == '\r' {
            if i + 1 < target && src[i + 1] == '\n' {
                utf16 += 2;
                i += 2;
            } else {
                utf16 += 1;
                i += 1;
            }
            line += 1;
            col = 1;
            continue;
        }
        if c == '\n' {
            utf16 += 1;
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        let u = c.len_utf16();
        col += u;
        utf16 += u;
        i += 1;
    }
    (line, col, utf16)
}

pub fn line_column(src: &[char], offset: usize, line_offset: usize) -> (usize, usize) {
    let (line, col, _) = written_metrics(src, offset, line_offset);
    (line, col)
}

pub fn location_text(src: &[char], start: usize, end: usize, line_offset: usize) -> String {
    let (line, column, from) = written_metrics(src, start, line_offset);
    let (_, _, to) = written_metrics(src, end, line_offset);
    format!("line {}, column {} [{},{})", line, column, from, to)
}

// The end of a quoted string that opens at `at`, or None when the line ends first.
fn string_end(src: &[char], at: usize, quote: char) -> Option<usize> {
    for (rel, &c) in src[at + 1..].iter().enumerate() {
        if c == quote {
            return Some(at + 1 + rel + 1);
        }
        if c == '\n' || c == '\r' {
            return None;
        }
    }
    None
}

fn starts_token(src: &[char], at: usize) -> bool {
    let Some(&c) = src.get(at) else { return true };
    if c != '"' && c != '\'' {
        return true;
    }
    (c == '"' && src.get(at + 1) == Some(&'`')) || string_end(src, at, c).is_some()
}

// Mermaid reports the first character no token starts with, and how many characters its lexer
// skipped before one did.
fn lex_error(src: &[char], offset: usize, length: usize, line_offset: usize) -> PeleError {
    let (line, column, from) = written_metrics(src, offset, line_offset);
    let (_, _, to) = written_metrics(src, offset + length, line_offset);
    let ch_str = src
        .get(offset)
        .map(|c| {
            // Match JS `src.charAt(offset)` (single UTF-16 code unit; ASCII for quote errors).
            let mut buf = [0u16; 2];
            let units = c.encode_utf16(&mut buf);
            String::from_utf16_lossy(&units[..1])
        })
        .unwrap_or_default();
    let mut err = PeleError::new(
        format!(
            "Error lexing usecase diagram: unexpected character: ->{}<- at offset: {}, skipped {} characters. at line {}, column {} [{},{})",
            ch_str,
            from,
            to.saturating_sub(from),
            line,
            column,
            from,
            to
        ),
        ErrorCode::Syntax,
    )
    .of("usecase");
    err.line = line;
    err.column = column;
    err
}

// Produces the tokens of Mermaid's Chevrotain lexer for use case diagrams: the first pattern in
// its list that matches wins, a keyword gives way to a longer identifier, and `json id@` and `<<`
// switch to modes that read a JSON object and a stereotype.
pub fn tokenize(src: &[char], line_offset: usize) -> Result<Tokens> {
    let mut types = Vec::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let n = src.len();
    let mut i = 0usize;
    // Whether only blanks came before on this line, where comments and accessibility lines may start.
    let mut line_start = true;
    let mut no_brace = false;

    while i < n {
        let c = src[i];
        if c == ' ' || c == '\t' {
            i += 1;
            continue;
        }
        let mut kind: usize;
        let mut end = i + 1;
        let mut fresh = false;

        if is_word_ascii(c) {
            kind = 0;
            if c == 'a' && line_start && starts_with(src, i, "acc") {
                let descr = starts_with(src, i + 3, "Descr");
                if descr || starts_with(src, i + 3, "Title") {
                    let mut j = i + 8;
                    while src.get(j).copied().is_some_and(is_blank) {
                        j += 1;
                    }
                    let next = src.get(j).copied();
                    if next == Some(':') {
                        kind = if descr { t::ACC_DESCR_LINE } else { t::ACC_TITLE_LINE };
                        end = j + 1;
                        while end < n && src[end] != '\n' && src[end] != '\r' {
                            end += 1;
                        }
                    } else if next == Some('{') && descr && !no_brace {
                        if let Some(rel) = src[j + 1..].iter().position(|&ch| ch == '}') {
                            kind = t::ACC_DESCR_BLOCK;
                            end = j + 1 + rel + 1;
                        } else {
                            no_brace = true;
                        }
                    }
                }
            } else if c == 'j'
                && starts_with(src, i, "json")
                && src.get(i + 4).copied().is_some_and(is_blank)
            {
                let mut j = i + 5;
                while src.get(j).copied().is_some_and(is_blank) {
                    j += 1;
                }
                let id_start = j;
                while src.get(j).copied().is_some_and(is_word_ascii) {
                    j += 1;
                }
                if j > id_start {
                    while src.get(j).copied().is_some_and(is_blank) {
                        j += 1;
                    }
                    if src.get(j) == Some(&'@') {
                        j += 1;
                        while src.get(j).copied().is_some_and(is_blank) {
                            j += 1;
                        }
                        if src.get(j) == Some(&'{') {
                            types.push(t::JSON_DECLARATION_START);
                            starts.push(i);
                            ends.push(j);
                            let mut depth = 0i32;
                            let mut quoted = false;
                            let mut escaped = false;
                            let mut k = j;
                            while k < n {
                                let ch = src[k];
                                if quoted {
                                    if escaped {
                                        escaped = false;
                                    } else if ch == '\\' {
                                        escaped = true;
                                    } else if ch == '"' {
                                        quoted = false;
                                    }
                                } else if ch == '"' {
                                    quoted = true;
                                } else if ch == '{' {
                                    depth += 1;
                                } else if ch == '}' {
                                    depth -= 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                k += 1;
                            }
                            types.push(if k < n {
                                t::JSON_OBJECT_LITERAL
                            } else {
                                t::UNCLOSED_JSON_OBJECT_LITERAL
                            });
                            starts.push(j);
                            i = if k < n { k + 1 } else { n };
                            ends.push(i);
                            line_start = false;
                            continue;
                        }
                    }
                }
            }
            if kind == 0 {
                while end < n && is_word_ascii(src[end]) {
                    end += 1;
                }
                let keyword = keyword_at(src, i, c);
                if keyword != 0 {
                    // A keyword that is only the start of a longer word is an identifier.
                    if keyword == t::USECASE || end - i == KEYWORD_LENGTH[keyword] {
                        kind = keyword;
                        end = i + KEYWORD_LENGTH[keyword];
                    } else {
                        kind = t::IDENTIFIER;
                    }
                } else if (c == 'o' || c == 'x')
                    && src.get(i + 1) == Some(&'-')
                    && src.get(i + 2) == Some(&'-')
                {
                    kind = if c == 'o' {
                        t::BACKWARD_CIRCLE
                    } else {
                        t::BACKWARD_CROSS
                    };
                    end = i + 3;
                } else if is_digit(c) {
                    // `1mg` is an identifier, and `1.5px` is a number because the identifier stops at the dot.
                    let mut digits = i + 1;
                    while src.get(digits).copied().is_some_and(is_digit) {
                        digits += 1;
                    }
                    if src.get(digits) == Some(&'.')
                        && src.get(digits + 1).copied().is_some_and(is_digit)
                    {
                        kind = t::NUMBER;
                        end = digits + 2;
                        while src.get(end).copied().is_some_and(is_digit) {
                            end += 1;
                        }
                        while src.get(end).copied().is_some_and(is_letter) {
                            end += 1;
                        }
                    } else {
                        kind = t::IDENTIFIER;
                    }
                } else {
                    kind = t::IDENTIFIER;
                    while src.get(end) == Some(&'-')
                        && src.get(end + 1).copied().is_some_and(is_word_ascii)
                    {
                        kind = t::CSS_IDENTIFIER;
                        end += 2;
                        while src.get(end).copied().is_some_and(is_word_ascii) {
                            end += 1;
                        }
                    }
                }
            }
        } else if c == '\n' {
            kind = t::NEWLINE;
            fresh = true;
        } else if c == '\r' {
            kind = t::NEWLINE;
            fresh = true;
            if src.get(end) == Some(&'\n') {
                end += 1;
            }
        } else if c == '"' || c == '\'' {
            if c == '"' && src.get(i + 1) == Some(&'`') {
                if let Some(close) =
                    (i + 2..n.saturating_sub(1)).find(|&k| src[k] == '`' && src[k + 1] == '"')
                {
                    kind = t::MARKDOWN_STRING;
                    end = close + 2;
                } else {
                    kind = t::UNCLOSED_MARKDOWN_STRING;
                    end = n;
                }
            } else {
                kind = t::PLAIN_STRING;
                if let Some(found_end) = string_end(src, i, c) {
                    end = found_end;
                } else {
                    let mut resume = i + 1;
                    while resume < n && !starts_token(src, resume) {
                        resume += 1;
                    }
                    return Err(lex_error(src, i, resume - i, line_offset));
                }
            }
        } else if c == '%' {
            if line_start && src.get(end) == Some(&'%') {
                kind = t::COMMENT;
                while end < n && src[end] != '\n' && src[end] != '\r' {
                    end += 1;
                }
            } else {
                kind = t::PERCENT;
            }
        } else if c == '-' {
            if src.get(end) != Some(&'-') {
                kind = t::DASH;
            } else {
                while src.get(end) == Some(&'-') {
                    end += 1;
                }
                let next = src.get(end).copied();
                let two = end - i == 2;
                if two && next == Some('|') && src.get(end + 1) == Some(&'>') {
                    kind = t::GENERALIZATION;
                    end += 2;
                } else if next == Some('>') {
                    kind = t::FORWARD_SOLID;
                    end += 1;
                } else if two && matches!(next, Some('o' | 'x')) {
                    kind = if next == Some('o') {
                        t::FORWARD_CIRCLE
                    } else {
                        t::FORWARD_CROSS
                    };
                    end += 1;
                } else {
                    kind = t::MARKERLESS_SOLID;
                }
            }
        } else if c == '.' {
            let next = src.get(end).copied();
            if next == Some('.') && src.get(end + 1) == Some(&'>') {
                kind = t::DEPENDENCY_ARROW;
                end += 2;
            } else if next.is_some_and(is_digit) {
                kind = t::NUMBER;
                while src.get(end).copied().is_some_and(is_digit) {
                    end += 1;
                }
                while src.get(end).copied().is_some_and(is_letter) {
                    end += 1;
                }
            } else {
                kind = t::DOT;
            }
        } else if c == '<' {
            if src.get(end) == Some(&'<') {
                types.push(t::STEREOTYPE_START);
                starts.push(i);
                ends.push(i + 2);
                i += 2;
                if i >= n {
                    break;
                }
                if src.get(i) == Some(&'>') && src.get(i + 1) == Some(&'>') {
                    kind = t::STEREOTYPE_END;
                    end = i + 2;
                } else {
                    let mut close = i;
                    let mut found = false;
                    while close < n {
                        let ch = src[close];
                        if ch == '\n' || ch == '\r' {
                            break;
                        }
                        if ch == '>' && src.get(close + 1) == Some(&'>') {
                            found = true;
                            break;
                        }
                        close += 1;
                    }
                    if found && src[i..close].iter().any(|&ch| !is_space(ch)) {
                        types.push(t::STEREOTYPE_TEXT);
                        starts.push(i);
                        ends.push(close);
                        kind = t::STEREOTYPE_END;
                        i = close;
                        end = close + 2;
                    } else {
                        while close < n && src[close] != '\n' && src[close] != '\r' {
                            close += 1;
                        }
                        if close == i {
                            while close < n && (src[close] == '\n' || src[close] == '\r') {
                                close += 1;
                            }
                            return Err(lex_error(src, i, close - i, line_offset));
                        }
                        kind = t::UNCLOSED_STEREOTYPE_TEXT;
                        end = close;
                    }
                }
            } else if src.get(end) == Some(&'-') && src.get(end + 1) == Some(&'-') {
                kind = t::BACKWARD_SOLID;
                end += 2;
                while src.get(end) == Some(&'-') {
                    end += 1;
                }
            } else {
                kind = t::LABEL_PUNCTUATION;
            }
        } else if c == ':' {
            if src.get(end) == Some(&':') && src.get(end + 1) == Some(&':') {
                kind = t::CLASS_SEPARATOR;
                end += 2;
            } else {
                kind = t::COLON;
            }
        } else if c == '@' {
            if src.get(end) == Some(&'{') {
                kind = t::METADATA_START;
                end += 1;
            } else {
                kind = t::AT;
            }
        } else if c == '{' {
            kind = t::LBRACE;
        } else if c == '}' {
            kind = t::RBRACE;
        } else if c == '[' {
            kind = t::LBRACKET;
        } else if c == ']' {
            kind = t::RBRACKET;
        } else if c == '(' {
            kind = t::LPAREN;
        } else if c == ')' {
            kind = t::RPAREN;
        } else if c == ',' {
            kind = t::COMMA;
        } else if c == '#' {
            while src.get(end).copied().is_some_and(is_hex) {
                end += 1;
            }
            kind = if end > i + 1 {
                t::HASH_COLOR
            } else {
                t::CSS_PUNCTUATION
            };
        } else if c == '\\' {
            if src.get(end) == Some(&',') {
                kind = t::CSS_ESCAPED_COMMA;
                end += 1;
            } else {
                kind = t::LABEL_PUNCTUATION;
            }
        } else if (c as u32) < 127 && (c as u32) > 32 {
            kind = if is_css_punct(c) {
                t::CSS_PUNCTUATION
            } else {
                t::LABEL_PUNCTUATION
            };
        } else {
            kind = t::LABEL_SYMBOL;
            while end < n {
                let ch = src[end] as u32;
                if (32..127).contains(&ch) || ch == 9 || ch == 10 || ch == 13 {
                    break;
                }
                end += 1;
            }
        }

        types.push(kind);
        starts.push(i);
        ends.push(end);
        i = end;
        line_start = fresh;
    }

    for _ in 0..3 {
        types.push(t::EOF);
        starts.push(n);
        ends.push(n);
    }
    Ok(Tokens { types, starts, ends })
}
