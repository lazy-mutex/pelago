use crate::chars::is_space;

#[derive(Default)]
pub struct Tokens {
    pub types: Vec<i32>,
    pub texts: Vec<String>,
    pub starts: Vec<usize>,
}

pub fn line_end(src: &[char], from: usize) -> usize {
    let mut i = from;
    while i < src.len() && src[i] != '\n' {
        i += 1;
    }
    i
}

// `word` is lowercase ASCII letters; the grammars are case-insensitive.
pub fn keyword_at(src: &[char], p: usize, word: &str) -> bool {
    for (i, w) in word.bytes().enumerate() {
        let Some(&c) = src.get(p + i) else {
            return false;
        };
        if ((c as u32) | 32) != w as u32 {
            return false;
        }
    }
    true
}

// Skips comments and whitespace. Stops at a line break that starts a token, which is a NEWLINE.
// Jison compiles `\%%` to `%`, so a single `%` starts a comment.
pub fn skip_trivia(src: &[char], from: usize) -> usize {
    let n = src.len();
    let mut p = from;
    while p < n {
        let c = src[p];
        if c == '%' && src.get(p + 1) != Some(&'{') {
            p = line_end(src, p);
        } else if c == '\n' {
            return p;
        } else if is_space(c) {
            p += 1;
            while p < n && is_space(src[p]) {
                p += 1;
            }
        } else if c == '#' {
            p = line_end(src, p);
        } else {
            return p;
        }
    }
    p
}

fn skip_space(src: &[char], from: usize) -> usize {
    let mut p = from;
    while p < src.len() && is_space(src[p]) {
        p += 1;
    }
    p
}

// Lexes an accTitle or accDescr statement at `p` and returns where it ends, or `p` if there is none.
// The token types acc_title, acc_title_value, acc_descr, acc_descr_value and
// acc_descr_multiline_value must be numbered consecutively from `first`.
pub fn accessibility(src: &[char], p: usize, out: &mut Tokens, first: i32) -> usize {
    let descr = keyword_at(src, p, "accdescr");
    if !descr && !keyword_at(src, p, "acctitle") {
        return p;
    }
    let n = src.len();
    let mut q = skip_space(src, p + 8);
    let Some(&c) = src.get(q) else {
        return p;
    };
    let mut emit = |kind: i32, s: usize, e: usize| {
        out.types.push(kind);
        out.texts.push(src[s..e].iter().collect());
        out.starts.push(s);
    };
    if c == ':' {
        q = skip_space(src, q + 1);
        let end = line_end(src, q);
        emit(if descr { first + 2 } else { first }, p, q);
        emit(if descr { first + 3 } else { first + 1 }, q, end);
        return end;
    }
    if !descr || c != '{' {
        return p;
    }
    q = skip_space(src, q + 1);
    while src.get(q) != Some(&'}') {
        let mut end = q;
        while end < n && src[end] != '}' {
            end += 1;
        }
        emit(first + 4, q, end);
        if q == n {
            return n;
        }
        q = end;
        if q == n {
            return n;
        }
    }
    q + 1
}
