use super::common::{
    choice, is_word_or_dash, nonterminal, optional, parse_grammar, repetition, sequence, special, terminal, unescape, ParsedGrammar,
    Pattern, Reader, TokenType,
};
use super::{RailroadNode, RailroadRule};
use crate::errors::Result;

// `railroad-beta`: Mermaid's own notation, one constructor call per node.

mod t {
    pub const NONTERMINAL: i32 = 1;
    pub const ZERO_OR_MORE: i32 = 2;
    pub const ONE_OR_MORE: i32 = 3;
    pub const SEQUENCE: i32 = 4;
    pub const OPTIONAL: i32 = 5;
    pub const TERMINAL: i32 = 6;
    pub const SPECIAL: i32 = 7;
    pub const CHOICE: i32 = 8;
    pub const EQUALS: i32 = 9;
    pub const SEMICOLON: i32 = 10;
    pub const OPEN: i32 = 11;
    pub const COMMA: i32 = 12;
    pub const CLOSE: i32 = 13;
    pub const TITLE: i32 = 14;
    pub const ID: i32 = 17;
    pub const STRING: i32 = 18;
}

const LONGER_ID: &[usize] = &[17];

const IR_TOKENS: &[TokenType] = &[
    TokenType::vis_longer("railroad-beta", Pattern::Keyword("railroad-beta"), LONGER_ID),
    TokenType::vis_longer("nonterminal", Pattern::Literal("nonterminal"), LONGER_ID),
    TokenType::vis_longer("zeroOrMore", Pattern::Literal("zeroOrMore"), LONGER_ID),
    TokenType::vis_longer("oneOrMore", Pattern::Literal("oneOrMore"), LONGER_ID),
    TokenType::vis_longer("sequence", Pattern::Literal("sequence"), LONGER_ID),
    TokenType::vis_longer("optional", Pattern::Literal("optional"), LONGER_ID),
    TokenType::vis_longer("terminal", Pattern::Literal("terminal"), LONGER_ID),
    TokenType::vis_longer("special", Pattern::Literal("special"), LONGER_ID),
    TokenType::vis_longer("choice", Pattern::Literal("choice"), LONGER_ID),
    TokenType::vis("=", Pattern::Literal("=")),
    TokenType::vis(";", Pattern::Literal(";")),
    TokenType::vis("(", Pattern::Literal("(")),
    TokenType::vis(",", Pattern::Literal(",")),
    TokenType::vis(")", Pattern::Literal(")")),
    TokenType::vis("TITLE", Pattern::Title(is_word_or_dash)),
    TokenType::vis("ACC_TITLE", Pattern::AccTitle),
    TokenType::vis("ACC_DESCR", Pattern::AccDescr),
    TokenType::vis("RR_ID", Pattern::Ident),
    TokenType::vis("RR_STRING", Pattern::Quoted),
    TokenType::hid("RR_WHITESPACE", Pattern::Whitespace),
    TokenType::hid("RR_YAML", Pattern::Yaml),
    TokenType::hid("RR_DIRECTIVE", Pattern::Directive),
    TokenType::hid("RR_SINGLE_LINE_COMMENT", Pattern::SingleLineComment),
    TokenType::hid("RR_BLOCK_COMMENT", Pattern::BlockComment("/*", "*/")),
];

enum OpenExpr {
    Sequence(Vec<RailroadNode>),
    Choice(Vec<RailroadNode>),
    Optional,
    OneOrMore,
    ZeroOrMore,
}

fn argument(r: &mut Reader) -> Result<String> {
    r.i += 1;
    r.expect(t::OPEN)?;
    let value = unescape(&r.expect(t::STRING)?);
    r.expect(t::CLOSE)?;
    Ok(value)
}

fn expression(r: &mut Reader) -> Result<RailroadNode> {
    let mut open: Vec<OpenExpr> = Vec::new();
    loop {
        let mut value: RailroadNode = match r.kind() {
            t::TERMINAL => terminal(argument(r)?),
            t::NONTERMINAL => nonterminal(argument(r)?),
            t::SPECIAL => special(argument(r)?),
            t::SEQUENCE => {
                r.i += 1;
                r.expect(t::OPEN)?;
                open.push(OpenExpr::Sequence(Vec::new()));
                continue;
            }
            t::CHOICE => {
                r.i += 1;
                r.expect(t::OPEN)?;
                open.push(OpenExpr::Choice(Vec::new()));
                continue;
            }
            t::OPTIONAL => {
                r.i += 1;
                r.expect(t::OPEN)?;
                open.push(OpenExpr::Optional);
                continue;
            }
            t::ONE_OR_MORE => {
                r.i += 1;
                r.expect(t::OPEN)?;
                open.push(OpenExpr::OneOrMore);
                continue;
            }
            t::ZERO_OR_MORE => {
                r.i += 1;
                r.expect(t::OPEN)?;
                open.push(OpenExpr::ZeroOrMore);
                continue;
            }
            _ => return Err(r.fail("an expression")),
        };

        loop {
            let Some(parent) = open.last_mut() else {
                return Ok(value);
            };
            match parent {
                OpenExpr::Sequence(items) | OpenExpr::Choice(items) => {
                    items.push(value);
                    if r.accept(t::COMMA) {
                        break;
                    }
                    r.expect(t::CLOSE)?;
                    value = match open.pop().unwrap() {
                        OpenExpr::Sequence(items) => sequence(items),
                        OpenExpr::Choice(items) => choice(items),
                        _ => unreachable!(),
                    };
                }
                OpenExpr::Optional | OpenExpr::OneOrMore | OpenExpr::ZeroOrMore => {
                    r.expect(t::CLOSE)?;
                    value = match open.pop().unwrap() {
                        OpenExpr::Optional => optional(value),
                        OpenExpr::OneOrMore => repetition(value, 1.0, f64::INFINITY),
                        OpenExpr::ZeroOrMore => repetition(value, 0.0, f64::INFINITY),
                        _ => unreachable!(),
                    };
                }
            }
        }
    }
}

pub fn parse_ir(src: &str) -> Result<ParsedGrammar> {
    parse_grammar(src, IR_TOKENS, t::TITLE, unescape, |r| {
        let name = r.expect(t::ID)?;
        r.expect(t::EQUALS)?;
        let definition = expression(r)?;
        r.expect(t::SEMICOLON)?;
        Ok(RailroadRule { name, definition })
    })
}
