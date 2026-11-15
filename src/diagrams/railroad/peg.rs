use super::common::{
    choice, fold, is_word_or_dash, nonterminal, optional, parse_grammar, repetition, sequence, special, terminal, unescape, ParsedGrammar,
    Pattern, Reader, TokenType,
};
use super::{RailroadNode, RailroadRule};
use crate::errors::Result;

// `railroad-peg-beta`: parsing expression grammars.

mod t {
    pub const ARROW: i32 = 1;
    pub const SEMICOLON: i32 = 2;
    pub const SLASH: i32 = 3;
    pub const AND: i32 = 4;
    pub const NOT: i32 = 5;
    pub const QUESTION: i32 = 6;
    pub const STAR: i32 = 7;
    pub const PLUS: i32 = 8;
    pub const GROUP: i32 = 9;
    pub const GROUP_END: i32 = 10;
    pub const DOT: i32 = 11;
    pub const TITLE: i32 = 12;
    pub const ID: i32 = 15;
    pub const STRING: i32 = 16;
}

const LONGER_ID: &[usize] = &[15];

const PEG_TOKENS: &[TokenType] = &[
    TokenType::vis_longer("railroad-peg-beta", Pattern::Keyword("railroad-peg-beta"), LONGER_ID),
    TokenType::vis("<-", Pattern::Literal("<-")),
    TokenType::vis(";", Pattern::Literal(";")),
    TokenType::vis("/", Pattern::Literal("/")),
    TokenType::vis("&", Pattern::Literal("&")),
    TokenType::vis("!", Pattern::Literal("!")),
    TokenType::vis("?", Pattern::Literal("?")),
    TokenType::vis("*", Pattern::Literal("*")),
    TokenType::vis("+", Pattern::Literal("+")),
    TokenType::vis("(", Pattern::Literal("(")),
    TokenType::vis(")", Pattern::Literal(")")),
    TokenType::vis(".", Pattern::Literal(".")),
    TokenType::vis("TITLE", Pattern::Title(is_word_or_dash)),
    TokenType::vis("ACC_TITLE", Pattern::AccTitle),
    TokenType::vis("ACC_DESCR", Pattern::AccDescr),
    TokenType::vis("PEG_ID", Pattern::Ident),
    TokenType::vis("PEG_STRING", Pattern::Quoted),
    TokenType::hid("PEG_WHITESPACE", Pattern::Whitespace),
    TokenType::hid("PEG_YAML", Pattern::Yaml),
    TokenType::hid("PEG_DIRECTIVE", Pattern::Directive),
    TokenType::hid("PEG_SINGLE_LINE_COMMENT", Pattern::SingleLineComment),
    TokenType::hid("PEG_LINE_COMMENT", Pattern::LineComment('#')),
];

enum PegPart {
    Choice(Vec<PegPart>),
    Sequence(Vec<PegPart>),
    Prefix { operator: Option<String>, suffix: Box<PegPart> },
    Suffix { primary: Box<PegPart>, operator: Option<String> },
    Literal(String),
    Identifier(String),
    Any(String),
    Group(Box<PegPart>),
}

struct Frame {
    alternatives: Vec<Vec<PegPart>>,
    current_seq: Vec<PegPart>,
    operator: Option<String>,
}

impl Frame {
    fn new(operator: Option<String>) -> Self {
        Self { alternatives: Vec::new(), current_seq: Vec::new(), operator }
    }

    fn into_choice(mut self) -> PegPart {
        self.alternatives.push(self.current_seq);
        PegPart::Choice(self.alternatives.into_iter().map(PegPart::Sequence).collect())
    }
}

fn definition(r: &mut Reader) -> Result<PegPart> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut top = Frame::new(None);

    loop {
        let mut operator = if matches!(r.kind(), t::AND | t::NOT) { Some(r.take()) } else { None };
        let mut primary: PegPart;
        match r.kind() {
            t::STRING => primary = PegPart::Literal(unescape(&r.take())),
            t::ID => primary = PegPart::Identifier(r.take()),
            t::DOT => primary = PegPart::Any(r.take()),
            t::GROUP => {
                r.i += 1;
                stack.push(top);
                top = Frame::new(operator);
                continue;
            }
            _ => return Err(r.fail("a literal, a name, a dot, or an opening parenthesis")),
        }

        loop {
            let suffix_op = if matches!(r.kind(), t::QUESTION | t::STAR | t::PLUS) { Some(r.take()) } else { None };
            let suffix = PegPart::Suffix { primary: Box::new(primary), operator: suffix_op };
            top.current_seq.push(PegPart::Prefix { operator, suffix: Box::new(suffix) });

            let k = r.kind();
            if k == t::GROUP_END && !stack.is_empty() {
                r.i += 1;
                let saved_op = top.operator.clone();
                primary = PegPart::Group(Box::new(top.into_choice()));
                operator = saved_op;
                top = stack.pop().unwrap();
                continue;
            }
            if k == t::SLASH {
                r.i += 1;
                let prev = std::mem::take(&mut top.current_seq);
                top.alternatives.push(prev);
            } else if !matches!(k, t::AND | t::NOT | t::STRING | t::ID | t::DOT | t::GROUP) {
                if stack.is_empty() {
                    return Ok(top.into_choice());
                }
                return Err(r.fail("token of type ')'"));
            }
            break;
        }
    }
}

fn part_children(part: &PegPart) -> Vec<&PegPart> {
    match part {
        PegPart::Choice(items) | PegPart::Sequence(items) => items.iter().collect(),
        PegPart::Prefix { suffix, .. } => vec![suffix.as_ref()],
        PegPart::Suffix { primary, .. } => vec![primary.as_ref()],
        PegPart::Group(inner) => vec![inner.as_ref()],
        PegPart::Literal(_) | PegPart::Identifier(_) | PegPart::Any(_) => Vec::new(),
    }
}

fn pred_label(node: &RailroadNode) -> String {
    match node {
        RailroadNode::Terminal(v) => format!("\"{}\"", v),
        RailroadNode::NonTerminal(n) => n.clone(),
        RailroadNode::Special(t) => t.clone(),
        _ => "(...)".to_string(),
    }
}

fn build_node(part: &PegPart, mut kids: Vec<RailroadNode>) -> RailroadNode {
    match part {
        PegPart::Choice(_) => choice(kids),
        PegPart::Sequence(_) => sequence(kids),
        PegPart::Prefix { operator, .. } => {
            let kid = kids.pop().unwrap();
            match operator {
                Some(op) => special(format!("{}{}", op, pred_label(&kid))),
                None => kid,
            }
        }
        PegPart::Suffix { operator, .. } => {
            let kid = kids.pop().unwrap();
            match operator.as_deref() {
                None => kid,
                Some("?") => optional(kid),
                Some("+") => repetition(kid, 1.0, f64::INFINITY),
                _ => repetition(kid, 0.0, f64::INFINITY),
            }
        }
        PegPart::Literal(v) => terminal(v.clone()),
        PegPart::Identifier(n) => nonterminal(n.clone()),
        PegPart::Any(d) => special(d.clone()),
        PegPart::Group(_) => kids.pop().unwrap(),
    }
}

pub fn parse_peg(src: &str) -> Result<ParsedGrammar> {
    parse_grammar(src, PEG_TOKENS, t::TITLE, unescape, |r| {
        let name = r.expect(t::ID)?;
        r.expect(t::ARROW)?;
        let tree = definition(r)?;
        r.expect(t::SEMICOLON)?;
        let def = fold(&tree, part_children, build_node);
        Ok(RailroadRule { name, definition: def })
    })
}
