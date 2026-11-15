use crate::chars::is_word;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum T {
    End = 0,
    // Not a Jison token: the reference lexer would return the same empty token forever from here.
    Stuck = 1,
    Sankey = 2,
    Newline = 3,
    Comma = 4,
    Dquote = 5,
    NonEscapedText = 6,
    EscapedText = 7,
    Eof = 8,
}

pub const TOKEN_NAMES: [&str; 9] = [
    "$end",
    "STUCK",
    "SANKEY",
    "NEWLINE",
    "COMMA",
    "DQUOTE",
    "NON_ESCAPED_TEXT",
    "ESCAPED_TEXT",
    "EOF",
];

pub struct Tokens {
    pub types: Vec<T>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Csv,
    EscapedText,
}

// RFC 4180 TEXTDATA extended to Unicode, excluding controls and invalid XML characters.
fn is_text_data(c: char) -> bool {
    let cp = c as u32;
    (cp >= 32 && cp <= 126 && cp != 34 && cp != 44)
        || (0xa0..=0xd7ff).contains(&cp)
        || (0xe000..=0xfffd).contains(&cp)
        || (0x10000..=0x10ffff).contains(&cp)
}

fn keyword_at(src: &[char], p: usize, word: &str) -> bool {
    let wlen = word.len();
    if p + wlen > src.len() {
        return false;
    }
    for (k, wb) in word.bytes().enumerate() {
        if src[p + k].to_ascii_lowercase() != wb as char {
            return false;
        }
    }
    !src.get(p + wlen).is_some_and(|&c| is_word(c))
}

pub fn tokenize(src: &[char]) -> Tokens {
    let n = src.len();
    let mut types: Vec<T> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut starts: Vec<usize> = Vec::new();
    let mut stack: Vec<S> = Vec::new();
    let mut state = S::Initial;
    let mut p = 0usize;

    let mut emit = |kind: T, s: usize, e: usize, p_out: &mut usize| {
        types.push(kind);
        texts.push(src[s..e].iter().collect());
        starts.push(s);
        *p_out = e;
    };

    while p < n {
        let c = src[p];
        let start = p;
        let mut q = p;
        if state == S::EscapedText {
            if c == '"' && src.get(p + 1) != Some(&'"') {
                if let Some(prev) = stack.pop() {
                    state = prev;
                }
                emit(T::Dquote, p, p + 1, &mut p);
                continue;
            }
            while q < n {
                let d = src[q];
                if is_text_data(d) || d == ',' || d == '\r' || d == '\n' {
                    q += 1;
                } else if d == '"' && src.get(q + 1) == Some(&'"') {
                    q += 2;
                } else {
                    break;
                }
            }
            emit(T::EscapedText, p, q, &mut p);
        } else {
            if state == S::Initial && (c == 's' || c == 'S') {
                let length = if keyword_at(src, p, "sankey-beta") {
                    11
                } else if keyword_at(src, p, "sankey") {
                    6
                } else {
                    0
                };
                if length > 0 {
                    stack.push(state);
                    state = S::Csv;
                    emit(T::Sankey, p, p + length, &mut p);
                    continue;
                }
            }
            if c == '\n' || (c == '\r' && src.get(p + 1) == Some(&'\n')) {
                let end = if c == '\n' { p + 1 } else { p + 2 };
                emit(T::Newline, p, end, &mut p);
                continue;
            }
            if c == ',' {
                emit(T::Comma, p, p + 1, &mut p);
                continue;
            }
            if c == '"' {
                stack.push(state);
                state = S::EscapedText;
                emit(T::Dquote, p, p + 1, &mut p);
                continue;
            }
            while q < n {
                if !is_text_data(src[q]) {
                    break;
                }
                q += 1;
            }
            emit(T::NonEscapedText, p, q, &mut p);
        }
        // A character outside the CSV alphabet matches as empty text and is never consumed.
        if q == start {
            emit(T::Stuck, p, p, &mut p);
            return Tokens { types, texts, starts };
        }
    }

    emit(
        if state == S::EscapedText { T::EscapedText } else { T::Eof },
        n,
        n,
        &mut p,
    );
    emit(T::End, n, n, &mut p);
    Tokens { types, texts, starts }
}
