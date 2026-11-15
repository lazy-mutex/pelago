use super::common::{
    choice, fold, is_alnum_or_dash, nonterminal, optional, parse_grammar, repetition, sequence, terminal, ParsedGrammar, Pattern, Reader,
    TokenType,
};
use super::{RailroadNode, RailroadRule};
use crate::errors::Result;

// `railroad-abnf-beta`: ABNF as in RFC 5234, with each rule closed by a semicolon.

mod t {
    pub const EQUALS: i32 = 1;
    pub const SEMICOLON: i32 = 2;
    pub const SLASH: i32 = 3;
    pub const GROUP: i32 = 4;
    pub const OPTION: i32 = 6;
    pub const TITLE: i32 = 8;
    pub const NAME: i32 = 11;
    pub const STRING: i32 = 12;
    pub const NUMBER: i32 = 13;
    pub const REPEAT: i32 = 14;
    pub const EXACT: i32 = 15;
}

const LONGER_NAME: &[usize] = &[11];
const LONGER_COMMENT: &[usize] = &[20];

const ABNF_TOKENS: &[TokenType] = &[
    TokenType::vis_longer("railroad-abnf-beta", Pattern::Keyword("railroad-abnf-beta"), LONGER_NAME),
    TokenType::vis("=", Pattern::Literal("=")),
    // A semicolon closes a rule only at the end of its line; with anything after it, it starts a comment.
    TokenType::vis_longer(";", Pattern::Literal(";"), LONGER_COMMENT),
    TokenType::vis("/", Pattern::Literal("/")),
    TokenType::vis("(", Pattern::Literal("(")),
    TokenType::vis(")", Pattern::Literal(")")),
    TokenType::vis("[", Pattern::Literal("[")),
    TokenType::vis("]", Pattern::Literal("]")),
    TokenType::vis("TITLE", Pattern::Title(is_alnum_or_dash)),
    TokenType::vis("ACC_TITLE", Pattern::AccTitle),
    TokenType::vis("ACC_DESCR", Pattern::AccDescr),
    TokenType::vis("ABNF_RULENAME", Pattern::AbnfName),
    TokenType::vis("ABNF_STRING", Pattern::AbnfString),
    TokenType::vis("ABNF_NUMVAL", Pattern::AbnfNumVal),
    TokenType::vis("ABNF_REPEAT", Pattern::AbnfRepeat),
    TokenType::vis("ABNF_EXACT_REPEAT", Pattern::AbnfExact),
    TokenType::hid("ABNF_WHITESPACE", Pattern::Whitespace),
    TokenType::hid("ABNF_YAML", Pattern::Yaml),
    TokenType::hid("ABNF_DIRECTIVE", Pattern::Directive),
    TokenType::hid("ABNF_SINGLE_LINE_COMMENT", Pattern::SingleLineComment),
    TokenType::hid("ABNF_COMMENT", Pattern::LineComment(';')),
];

enum AbnfPart {
    Alternation(Vec<AbnfPart>),
    Concatenation(Vec<AbnfPart>),
    Element { repeat: Option<String>, primary: Box<AbnfPart> },
    Terminal(String),
    RuleName(String),
    Group(Box<AbnfPart>),
    OptionalGroup(Box<AbnfPart>),
}

struct Frame {
    alternatives: Vec<Vec<AbnfPart>>,
    current_concat: Vec<AbnfPart>,
    is_optional: Option<bool>,
    close: i32,
    repeat: Option<String>,
}

impl Frame {
    fn new(is_optional: Option<bool>, close: i32, repeat: Option<String>) -> Self {
        Self { alternatives: Vec::new(), current_concat: Vec::new(), is_optional, close, repeat }
    }

    fn into_alternation(mut self) -> AbnfPart {
        self.alternatives.push(self.current_concat);
        AbnfPart::Alternation(self.alternatives.into_iter().map(AbnfPart::Concatenation).collect())
    }
}

fn slice_inner(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() >= 2 {
        chars[1..chars.len() - 1].iter().collect()
    } else {
        String::new()
    }
}

fn alternation(r: &mut Reader) -> Result<AbnfPart> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut top = Frame::new(None, -2, None);

    loop {
        let mut repeat = if matches!(r.kind(), t::REPEAT | t::EXACT) { Some(r.take()) } else { None };
        let mut primary: AbnfPart;
        let kind = r.kind();
        match kind {
            t::STRING => primary = AbnfPart::Terminal(slice_inner(&r.take())),
            t::NUMBER => primary = AbnfPart::Terminal(r.take()),
            t::NAME => primary = AbnfPart::RuleName(r.take()),
            t::GROUP | t::OPTION => {
                r.i += 1;
                stack.push(top);
                top = Frame::new(Some(kind == t::OPTION), kind + 1, repeat);
                continue;
            }
            _ => return Err(r.fail("a string, a number, a name, or an opening bracket")),
        }

        loop {
            top.current_concat.push(AbnfPart::Element { repeat, primary: Box::new(primary) });
            let k = r.kind();
            if k == top.close {
                r.i += 1;
                let opt = top.is_optional.unwrap();
                let saved_repeat = top.repeat.clone();
                let inner = Box::new(top.into_alternation());
                primary = if opt { AbnfPart::OptionalGroup(inner) } else { AbnfPart::Group(inner) };
                repeat = saved_repeat;
                top = stack.pop().unwrap();
                continue;
            }
            if k == t::SLASH {
                r.i += 1;
                let prev = std::mem::take(&mut top.current_concat);
                top.alternatives.push(prev);
            } else if !matches!(k, t::STRING | t::NUMBER | t::NAME | t::GROUP | t::OPTION | t::REPEAT | t::EXACT) {
                if stack.is_empty() {
                    return Ok(top.into_alternation());
                }
                return Err(r.fail(&format!("token of type '{}'", ABNF_TOKENS[top.close as usize].name)));
            }
            break;
        }
    }
}

fn part_children(part: &AbnfPart) -> Vec<&AbnfPart> {
    match part {
        AbnfPart::Alternation(items) | AbnfPart::Concatenation(items) => items.iter().collect(),
        AbnfPart::Element { primary, .. } => vec![primary.as_ref()],
        AbnfPart::Group(inner) | AbnfPart::OptionalGroup(inner) => vec![inner.as_ref()],
        AbnfPart::Terminal(_) | AbnfPart::RuleName(_) => Vec::new(),
    }
}

fn parse_js_int(digits: &str) -> f64 {
    digits.parse::<f64>().unwrap_or(f64::INFINITY)
}

fn build_node(part: &AbnfPart, mut kids: Vec<RailroadNode>) -> RailroadNode {
    match part {
        AbnfPart::Alternation(_) => choice(kids),
        AbnfPart::Concatenation(_) => sequence(kids),
        AbnfPart::Element { repeat, .. } => {
            let kid = kids.pop().unwrap();
            let Some(rep) = repeat.as_deref() else {
                return kid;
            };
            match rep.find('*') {
                None => {
                    let exact = parse_js_int(rep);
                    if exact == 0.0 {
                        repetition(kid, 0.0, 0.0)
                    } else {
                        repetition(kid, exact, exact)
                    }
                }
                Some(star) => {
                    let min = if star > 0 { parse_js_int(&rep[..star]) } else { 0.0 };
                    let max = if star + 1 < rep.len() { parse_js_int(&rep[star + 1..]) } else { f64::INFINITY };
                    if min == 0.0 && max == 1.0 {
                        optional(kid)
                    } else {
                        repetition(kid, min, max)
                    }
                }
            }
        }
        AbnfPart::Terminal(v) => terminal(v.clone()),
        AbnfPart::RuleName(n) => nonterminal(n.clone()),
        AbnfPart::Group(_) => kids.pop().unwrap(),
        AbnfPart::OptionalGroup(_) => optional(kids.pop().unwrap()),
    }
}

pub fn parse_abnf(src: &str) -> Result<ParsedGrammar> {
    parse_grammar(src, ABNF_TOKENS, t::TITLE, slice_inner, |r| {
        let name = r.expect(t::NAME)?;
        r.expect(t::EQUALS)?;
        let tree = alternation(r)?;
        r.expect(t::SEMICOLON)?;
        let def = fold(&tree, part_children, build_node);
        Ok(RailroadRule { name, definition: def })
    })
}
