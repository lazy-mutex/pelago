use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, keyword, title_value, tokenize, Reader, TokenType, ACC_DESCR,
    ACC_TITLE, DIRECTIVE, NEWLINE, SINGLE_LINE_COMMENT, TITLE, WHITESPACE, YAML,
};
use crate::errors::Result;

const T_SHOW_INFO: i32 = 0;
const T_INFO: i32 = 1;
const T_ACC_DESCR: i32 = 2;
const T_ACC_TITLE: i32 = 3;
const T_TITLE: i32 = 4;
const T_NEWLINE: i32 = 5;

pub const INFO_TOKENS: &[TokenType] = &[
    keyword("showInfo"),
    keyword("info"),
    ACC_DESCR,
    ACC_TITLE,
    TITLE,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Debug, Clone, Default)]
pub struct InfoAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

pub fn parse_info(src: &str) -> Result<InfoAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, INFO_TOKENS, "info")?;
    let mut r = Reader::new(&chars, tokens, INFO_TOKENS, "info");
    let mut ast = InfoAst::default();

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    r.expect(T_INFO)?;
    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    if r.accept(T_SHOW_INFO) {
        while r.kind() == T_NEWLINE {
            r.i += 1;
        }
    }
    while r.kind() != -1 {
        let kind = r.kind();
        if kind == T_ACC_DESCR {
            ast.acc_descr = Some(acc_descr_value(&r.take()));
        } else if kind == T_ACC_TITLE {
            ast.acc_title = Some(acc_title_value(&r.take()));
        } else if kind == T_TITLE {
            ast.title = Some(title_value(&r.take()));
        } else {
            return Err(r.fail("a title or the end of input"));
        }
        if r.kind() == -1 {
            break;
        }
        if r.kind() != T_NEWLINE {
            return Err(r.fail("a line break or the end of input"));
        }
        while r.kind() == T_NEWLINE {
            r.i += 1;
        }
    }
    Ok(ast)
}
