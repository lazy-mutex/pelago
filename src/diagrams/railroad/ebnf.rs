use super::common::{
    choice, fold, is_word_or_dash, nonterminal, optional, parse_grammar, repetition, sequence, special, terminal, unescape, ParsedGrammar,
    Pattern, Reader, TokenType,
};
use super::{RailroadNode, RailroadRule};
use crate::errors::Result;

// `railroad-ebnf-beta`: EBNF in the W3C and ISO 14977 styles.

mod t {
    pub const DEFINES: i32 = 1;
    pub const EQUALS: i32 = 2;
    pub const SEMICOLON: i32 = 3;
    pub const BAR: i32 = 4;
    pub const COMMA: i32 = 5;
    pub const GROUP: i32 = 6;
    pub const OPTION: i32 = 8;
    pub const REPEAT: i32 = 10;
    pub const QUESTION: i32 = 12;
    pub const STAR: i32 = 13;
    pub const PLUS: i32 = 14;
    pub const MINUS: i32 = 15;
    pub const TITLE: i32 = 16;
    pub const ID: i32 = 19;
    pub const STRING: i32 = 20;
    pub const SPECIAL: i32 = 21;
}

const LONGER_ID: &[usize] = &[19];
const LONGER_ISO_COMMENT: &[usize] = &[27];
const LONGER_SPECIAL: &[usize] = &[21];
const LONGER_YAML: &[usize] = &[23];

const EBNF_TOKENS: &[TokenType] = &[
    TokenType::vis_longer("railroad-ebnf-beta", Pattern::Keyword("railroad-ebnf-beta"), LONGER_ID),
    TokenType::vis("::=", Pattern::Literal("::=")),
    TokenType::vis("=", Pattern::Literal("=")),
    TokenType::vis(";", Pattern::Literal(";")),
    TokenType::vis("|", Pattern::Literal("|")),
    TokenType::vis(",", Pattern::Literal(",")),
    TokenType::vis_longer("(", Pattern::Literal("("), LONGER_ISO_COMMENT),
    TokenType::vis(")", Pattern::Literal(")")),
    TokenType::vis("[", Pattern::Literal("[")),
    TokenType::vis("]", Pattern::Literal("]")),
    TokenType::vis("{", Pattern::Literal("{")),
    TokenType::vis("}", Pattern::Literal("}")),
    TokenType::vis_longer("?", Pattern::Literal("?"), LONGER_SPECIAL),
    TokenType::vis("*", Pattern::Literal("*")),
    TokenType::vis("+", Pattern::Literal("+")),
    TokenType::vis_longer("-", Pattern::Literal("-"), LONGER_YAML),
    TokenType::vis("TITLE", Pattern::Title(is_word_or_dash)),
    TokenType::vis("ACC_TITLE", Pattern::AccTitle),
    TokenType::vis("ACC_DESCR", Pattern::AccDescr),
    TokenType::vis("EBNF_ID", Pattern::Ident),
    TokenType::vis("EBNF_STRING", Pattern::Quoted),
    TokenType::vis("EBNF_SPECIAL_SEQUENCE", Pattern::EbnfSpecial),
    TokenType::hid("EBNF_WHITESPACE", Pattern::Whitespace),
    TokenType::hid("EBNF_YAML", Pattern::Yaml),
    TokenType::hid("EBNF_DIRECTIVE", Pattern::Directive),
    TokenType::hid("EBNF_SINGLE_LINE_COMMENT", Pattern::SingleLineComment),
    TokenType::hid("EBNF_BLOCK_COMMENT", Pattern::BlockComment("/*", "*/")),
    TokenType::hid("EBNF_ISO_COMMENT", Pattern::BlockComment("(*", "*)")),
];

#[derive(Clone, Copy)]
enum BracketKind {
    Group,
    Optional,
    Repetition,
}

enum EbnfPart {
    Choice(Vec<EbnfPart>),
    Sequence(Vec<EbnfPart>),
    Term { base: Box<EbnfPart>, postfixes: Vec<Postfix> },
    Terminal(String),
    NonTerminal(String),
    Special(String),
    Group(Box<EbnfPart>),
    Optional(Box<EbnfPart>),
    Repetition(Box<EbnfPart>),
}

enum Postfix {
    Optional,
    ZeroOrMore,
    OneOrMore,
    Exception(EbnfPart),
}

struct Frame {
    alternatives: Vec<Vec<EbnfPart>>,
    current_seq: Vec<EbnfPart>,
    bracket: Option<BracketKind>,
    close: i32,
    except: bool,
}

impl Frame {
    fn new(bracket: Option<BracketKind>, close: i32, except: bool) -> Self {
        Self { alternatives: Vec::new(), current_seq: Vec::new(), bracket, close, except }
    }

    fn into_choice(mut self) -> EbnfPart {
        self.alternatives.push(self.current_seq);
        EbnfPart::Choice(self.alternatives.into_iter().map(EbnfPart::Sequence).collect())
    }
}

fn definition(r: &mut Reader) -> Result<EbnfPart> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut top = Frame::new(None, -2, false);
    let mut except = false;

    loop {
        let mut primary: EbnfPart;
        let kind = r.kind();
        match kind {
            t::STRING => primary = EbnfPart::Terminal(unescape(&r.take())),
            t::ID => primary = EbnfPart::NonTerminal(r.take()),
            t::SPECIAL => {
                let raw = r.take();
                let chars: Vec<char> = raw.chars().collect();
                let inner: String = chars[1..chars.len() - 1].iter().collect();
                primary = EbnfPart::Special(inner.trim().to_string());
            }
            t::GROUP | t::OPTION | t::REPEAT => {
                r.i += 1;
                let bk = match kind {
                    t::GROUP => BracketKind::Group,
                    t::OPTION => BracketKind::Optional,
                    _ => BracketKind::Repetition,
                };
                stack.push(top);
                top = Frame::new(Some(bk), kind + 1, except);
                except = false;
                continue;
            }
            _ => return Err(r.fail("a terminal, a name, or an opening bracket")),
        }

        loop {
            if except {
                if let Some(EbnfPart::Term { postfixes, .. }) = top.current_seq.last_mut() {
                    postfixes.push(Postfix::Exception(primary));
                }
                except = false;
            } else {
                top.current_seq.push(EbnfPart::Term { base: Box::new(primary), postfixes: Vec::new() });
            }
            if let Some(EbnfPart::Term { postfixes, .. }) = top.current_seq.last_mut() {
                loop {
                    let k = r.kind();
                    if k == t::QUESTION {
                        r.take();
                        postfixes.push(Postfix::Optional);
                    } else if k == t::STAR {
                        r.take();
                        postfixes.push(Postfix::ZeroOrMore);
                    } else if k == t::PLUS {
                        r.take();
                        postfixes.push(Postfix::OneOrMore);
                    } else {
                        break;
                    }
                }
            }
            let k = r.kind();
            if k == top.close {
                r.i += 1;
                let bk = top.bracket.unwrap();
                let was_except = top.except;
                let inner_choice = Box::new(top.into_choice());
                primary = match bk {
                    BracketKind::Group => EbnfPart::Group(inner_choice),
                    BracketKind::Optional => EbnfPart::Optional(inner_choice),
                    BracketKind::Repetition => EbnfPart::Repetition(inner_choice),
                };
                except = was_except;
                top = stack.pop().unwrap();
                continue;
            }
            if k == t::MINUS {
                r.i += 1;
                except = true;
            } else if k == t::COMMA {
                r.i += 1;
            } else if k == t::BAR {
                r.i += 1;
                let prev = std::mem::take(&mut top.current_seq);
                top.alternatives.push(prev);
            } else if !matches!(k, t::STRING | t::ID | t::SPECIAL | t::GROUP | t::OPTION | t::REPEAT) {
                if stack.is_empty() {
                    return Ok(top.into_choice());
                }
                return Err(r.fail(&format!("token of type '{}'", EBNF_TOKENS[top.close as usize].name)));
            }
            break;
        }
    }
}

fn part_children(part: &EbnfPart) -> Vec<&EbnfPart> {
    match part {
        EbnfPart::Choice(alts) | EbnfPart::Sequence(alts) => alts.iter().collect(),
        EbnfPart::Term { base, postfixes } => {
            let mut kids = vec![base.as_ref()];
            for pf in postfixes {
                if let Postfix::Exception(ex) = pf {
                    kids.push(ex);
                }
            }
            kids
        }
        EbnfPart::Group(inner) | EbnfPart::Optional(inner) | EbnfPart::Repetition(inner) => vec![inner.as_ref()],
        EbnfPart::Terminal(_) | EbnfPart::NonTerminal(_) | EbnfPart::Special(_) => Vec::new(),
    }
}

fn build_node(part: &EbnfPart, mut kids: Vec<RailroadNode>) -> RailroadNode {
    match part {
        EbnfPart::Choice(_) => choice(kids),
        EbnfPart::Sequence(_) => sequence(kids),
        EbnfPart::Term { postfixes, .. } => {
            let mut iter = kids.into_iter();
            let mut result = iter.next().unwrap();
            for pf in postfixes {
                result = match pf {
                    Postfix::Optional => optional(result),
                    Postfix::ZeroOrMore => repetition(result, 0.0, f64::INFINITY),
                    Postfix::OneOrMore => repetition(result, 1.0, f64::INFINITY),
                    Postfix::Exception(_) => RailroadNode::Sequence(vec![result, terminal("-".to_string()), iter.next().unwrap()]),
                };
            }
            result
        }
        EbnfPart::Terminal(v) => terminal(v.clone()),
        EbnfPart::NonTerminal(n) => nonterminal(n.clone()),
        EbnfPart::Special(s) => special(s.clone()),
        EbnfPart::Group(_) => kids.pop().unwrap(),
        EbnfPart::Optional(_) => optional(kids.pop().unwrap()),
        EbnfPart::Repetition(_) => repetition(kids.pop().unwrap(), 0.0, f64::INFINITY),
    }
}

pub fn parse_ebnf(src: &str) -> Result<ParsedGrammar> {
    parse_grammar(src, EBNF_TOKENS, t::TITLE, unescape, |r| {
        let name = r.expect(t::ID)?;
        if !r.accept(t::EQUALS) {
            r.expect(t::DEFINES)?;
        }
        let tree = definition(r)?;
        r.expect(t::SEMICOLON)?;
        let def = fold(&tree, part_children, build_node);
        Ok(RailroadRule { name, definition: def })
    })
}
