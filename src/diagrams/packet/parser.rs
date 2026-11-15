use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, keyword, string_value, title_value, tokenize, Pattern,
    Reader, TokenType, ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE, SINGLE_LINE_COMMENT, STRING,
    TITLE, WHITESPACE, YAML,
};
use crate::errors::Result;

const T_PACKET_BETA: i32 = 0;
const T_PACKET: i32 = 1;
const T_DASH: i32 = 2;
const T_PLUS: i32 = 3;
const T_COLON: i32 = 4;
const T_ACC_DESCR: i32 = 5;
const T_ACC_TITLE: i32 = 6;
const T_TITLE: i32 = 7;
const T_INT: i32 = 8;
const T_STRING: i32 = 9;
const T_NEWLINE: i32 = 10;

fn scan_int(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    match *src.get(at)? {
        '0' => Some(at + 1),
        '1'..='9' => {
            let mut end = at + 1;
            while end < src.len() && src[end].is_ascii_digit() {
                end += 1;
            }
            if src.get(end) != Some(&'.') {
                Some(end)
            } else if end - 1 > at {
                Some(end - 1)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub const PACKET_TOKENS: &[TokenType] = &[
    TokenType::vis("packet-beta", Pattern::Literal("packet-beta")),
    keyword("packet"),
    // A front matter block also starts with dashes, so Langium lets it win over a dash.
    TokenType::vis_longer("-", Pattern::Literal("-"), &[12]),
    TokenType::vis("+", Pattern::Literal("+")),
    TokenType::vis(":", Pattern::Literal(":")),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    TokenType::vis("INT", Pattern::Custom(scan_int)),
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Debug, Clone, Default)]
pub struct PacketBlockAst {
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub bits: Option<f64>,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct PacketAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub blocks: Vec<PacketBlockAst>,
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

pub fn parse_packet(src: &str) -> Result<PacketAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, PACKET_TOKENS, "packet")?;
    let mut r = Reader::new(&chars, tokens, PACKET_TOKENS, "packet");
    let mut ast = PacketAst::default();

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    if !r.accept(T_PACKET_BETA) {
        r.expect(T_PACKET)?;
    }
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
            T_INT | T_PLUS => {
                let mut block = PacketBlockAst::default();
                if r.accept(T_PLUS) {
                    let bits_str = r.expect(T_INT)?;
                    block.bits = Some(bits_str.parse::<f64>().unwrap_or(0.0));
                } else {
                    let start_str = r.take();
                    block.start = Some(start_str.parse::<f64>().unwrap_or(0.0));
                    if r.accept(T_DASH) {
                        let end_str = r.expect(T_INT)?;
                        block.end = Some(end_str.parse::<f64>().unwrap_or(0.0));
                    }
                }
                r.expect(T_COLON)?;
                block.label = string_value(&r.expect(T_STRING)?);
                ast.blocks.push(block);
                end_of_line(&mut r)?;
            }
            _ => return Err(r.fail("a field, a title, or a line break")),
        }
    }
    Ok(ast)
}
