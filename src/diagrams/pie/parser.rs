use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, keyword, string_value, title_value, tokenize, Pattern,
    Reader, TokenType, ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE, SINGLE_LINE_COMMENT, STRING,
    TITLE, WHITESPACE, YAML,
};
use crate::errors::Result;

const T_SHOW_DATA: i32 = 0;
const T_PIE: i32 = 1;
const T_COLON: i32 = 2;
const T_NUMBER: i32 = 3;
const T_ACC_DESCR: i32 = 4;
const T_ACC_TITLE: i32 = 5;
const T_TITLE: i32 = 6;
const T_STRING: i32 = 7;
const T_NEWLINE: i32 = 8;

fn digits(src: &[char], from: usize) -> usize {
    let mut i = from;
    while i < src.len() && src[i].is_ascii_digit() {
        i += 1;
    }
    i
}

// Answers as NUMBER_PIE's pattern does. The pattern reads a whole run of digits before it finds
// there is no fraction, and would do so again from every digit of a long run of zeros.
fn number_scanner() -> Box<dyn FnMut(&[char], usize, i32) -> Option<usize>> {
    let mut from: usize = 0;
    let mut end: usize = 0;
    let mut fraction: isize = -1;
    Box::new(move |src: &[char], p: usize, _prev: i32| {
        let first = *src.get(p)?;
        if first != '-' && !first.is_ascii_digit() {
            return None;
        }
        let at = if first == '-' { p + 1 } else { p };
        if at < from || at >= end {
            from = at;
            end = digits(src, at);
            fraction = -1;
            if end > at && src.get(end) == Some(&'.') {
                let stop = digits(src, end + 1);
                let length = stop - end - 1;
                if length > 0 && src.get(stop) != Some(&'.') {
                    fraction = stop as isize;
                } else if length > 1 {
                    fraction = (stop - 1) as isize;
                }
            }
        }
        if end == at {
            return None;
        }
        if fraction >= 0 {
            return Some(fraction as usize);
        }
        if src.get(at) == Some(&'0') {
            return if src.get(at + 1) == Some(&'.') { None } else { Some(at + 1) };
        }
        if src.get(end) != Some(&'.') {
            return Some(end);
        }
        if end - 1 > at {
            Some(end - 1)
        } else {
            None
        }
    })
}

pub const PIE_TOKENS: &[TokenType] = &[
    keyword("showData"),
    keyword("pie"),
    TokenType::vis(":", Pattern::Literal(":")),
    TokenType::vis("NUMBER_PIE", Pattern::Scanner(number_scanner)),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Debug, Clone, Default)]
pub struct PieAst {
    pub show_data: bool,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub sections: Vec<(String, f64)>,
}

fn end_of_line(r: &mut Reader) -> Result<()> {
    if r.kind() == -1 {
        return Ok(());
    }
    if r.kind() != T_NEWLINE {
        return Err(r.fail("a line break or the end of input"));
    }
    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    Ok(())
}

pub fn parse_pie(src: &str) -> Result<PieAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, PIE_TOKENS, "pie")?;
    let mut r = Reader::new(&chars, tokens, PIE_TOKENS, "pie");
    let mut ast = PieAst::default();

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    r.expect(T_PIE)?;
    ast.show_data = r.accept(T_SHOW_DATA);
    while r.kind() != -1 {
        match r.kind() {
            T_NEWLINE => {
                r.i += 1;
            }
            T_ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(&r.take()));
                end_of_line(&mut r)?;
            }
            T_ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            T_TITLE => {
                ast.title = Some(title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            T_STRING => {
                let label = string_value(&r.take());
                r.expect(T_COLON)?;
                let raw_val = r.expect(T_NUMBER)?;
                let value = raw_val.parse::<f64>().unwrap_or(f64::NAN);
                ast.sections.push((label, value));
                end_of_line(&mut r)?;
            }
            _ => return Err(r.fail("a section, a title, or a line break")),
        }
    }
    Ok(ast)
}
