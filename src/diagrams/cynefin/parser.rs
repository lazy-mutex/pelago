use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, keyword, slice_starts_with, string_value, title_value,
    tokenize, Pattern, Reader, TokenType, ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE,
    SINGLE_LINE_COMMENT, STRING, TITLE, WHITESPACE, YAML,
};
use crate::errors::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomainName {
    Chaotic,
    Clear,
    Complex,
    Complicated,
    Confusion,
}

impl DomainName {
    pub fn as_str(self) -> &'static str {
        match self {
            DomainName::Complex => "complex",
            DomainName::Complicated => "complicated",
            DomainName::Clear => "clear",
            DomainName::Chaotic => "chaotic",
            DomainName::Confusion => "confusion",
        }
    }

    fn from_token(s: &str) -> Self {
        match s {
            "complex" => DomainName::Complex,
            "complicated" => DomainName::Complicated,
            "clear" => DomainName::Clear,
            "chaotic" => DomainName::Chaotic,
            _ => DomainName::Confusion,
        }
    }
}

mod t {
    pub const KEYWORD_COLON: i32 = 0;
    pub const KEYWORD: i32 = 1;
    pub const ARROW: i32 = 2;
    pub const COLON: i32 = 3;
    pub const DOMAIN: i32 = 4;
    pub const ACC_DESCR: i32 = 5;
    pub const ACC_TITLE: i32 = 6;
    pub const TITLE: i32 = 7;
    pub const STRING: i32 = 8;
    pub const NEWLINE: i32 = 9;
}

fn match_domain(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    for word in ["complex", "complicated", "clear", "chaotic", "confusion"] {
        if slice_starts_with(src, at, word) {
            return Some(at + word.len());
        }
    }
    None
}

pub const CYNEFIN_TOKENS: &[TokenType] = &[
    TokenType::vis("cynefin-beta:", Pattern::Literal("cynefin-beta:")),
    keyword("cynefin-beta"),
    TokenType::vis("-->", Pattern::Literal("-->")),
    TokenType::vis(":", Pattern::Literal(":")),
    TokenType::vis("DOMAIN_NAME", Pattern::Custom(match_domain)),
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

#[derive(Debug, Clone)]
pub struct DomainBlockAst {
    pub domain: DomainName,
    pub items: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TransitionAst {
    pub from: DomainName,
    pub to: DomainName,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CynefinAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub domains: Vec<DomainBlockAst>,
    pub transitions: Vec<TransitionAst>,
}

fn skip_lines(r: &mut Reader) {
    while r.kind() == t::NEWLINE {
        r.i += 1;
    }
}

fn end_of_line(r: &mut Reader) -> Result<()> {
    if r.kind() == -1 {
        return Ok(());
    }
    if r.kind() != t::NEWLINE {
        return Err(r.fail("a line break or the end of input"));
    }
    skip_lines(r);
    Ok(())
}

pub fn parse_cynefin(src: &str) -> Result<CynefinAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, CYNEFIN_TOKENS, "cynefin")?;
    let mut r = Reader::new(&chars, tokens, CYNEFIN_TOKENS, "cynefin");
    let mut ast = CynefinAst::default();

    skip_lines(&mut r);
    if r.kind() != t::KEYWORD && r.kind() != t::KEYWORD_COLON {
        return Err(r.fail("'cynefin-beta'"));
    }
    r.i += 1;

    while r.kind() != -1 {
        match r.kind() {
            t::NEWLINE => r.i += 1,
            t::ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(&r.take()));
                end_of_line(&mut r)?;
            }
            t::ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            t::TITLE => {
                ast.title = Some(title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            t::DOMAIN => {
                let from = DomainName::from_token(&r.take());
                if r.accept(t::ARROW) {
                    let to = DomainName::from_token(&r.expect(t::DOMAIN)?);
                    let label = if r.accept(t::COLON) {
                        Some(string_value(&r.expect(t::STRING)?))
                    } else {
                        None
                    };
                    end_of_line(&mut r)?;
                    ast.transitions.push(TransitionAst { from, to, label });
                } else {
                    let mut items = Vec::new();
                    skip_lines(&mut r);
                    while r.kind() == t::STRING {
                        items.push(string_value(&r.take()));
                        skip_lines(&mut r);
                    }
                    ast.domains.push(DomainBlockAst { domain: from, items });
                }
            }
            _ => return Err(r.fail("a domain, a transition, a title, or a line break")),
        }
    }
    Ok(ast)
}
