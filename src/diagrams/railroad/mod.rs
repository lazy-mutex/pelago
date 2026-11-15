pub mod abnf;
pub mod common;
pub mod ebnf;
pub mod ir;
pub mod peg;
pub mod render;

use crate::errors::{ErrorCode, PeleError, Result};
pub use render::render_railroad;

#[derive(Debug, Clone, PartialEq)]
pub enum RailroadNode {
    Terminal(String),
    NonTerminal(String),
    Special(String),
    Sequence(Vec<RailroadNode>),
    Choice(Vec<RailroadNode>),
    Optional(Box<RailroadNode>),
    Repetition { element: Box<RailroadNode>, min: f64, max: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RailroadRule {
    pub name: String,
    pub definition: RailroadNode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notation {
    Railroad,
    Ebnf,
    Abnf,
    Peg,
}

#[derive(Debug, Clone)]
pub struct RailroadModel {
    pub notation: Notation,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub rules: Vec<RailroadRule>,
}

pub fn notation_of(source: &str) -> Option<Notation> {
    let trimmed = source.trim_start();
    let lower: String = trimmed.chars().take(20).map(|c| c.to_ascii_lowercase()).collect();
    if lower.starts_with("railroad-ebnf-beta") {
        Some(Notation::Ebnf)
    } else if lower.starts_with("railroad-abnf-beta") {
        Some(Notation::Abnf)
    } else if lower.starts_with("railroad-peg-beta") {
        Some(Notation::Peg)
    } else if lower.starts_with("railroad-beta") {
        Some(Notation::Railroad)
    } else {
        None
    }
}

pub fn parse_railroad(source: &str, front_title: Option<String>) -> Result<RailroadModel> {
    let Some(notation) = notation_of(source) else {
        return Err(PeleError::new("No railroad diagram keyword found.", ErrorCode::Syntax).of("railroad"));
    };
    let parsed = match notation {
        Notation::Ebnf => ebnf::parse_ebnf(source)?,
        Notation::Abnf => abnf::parse_abnf(source)?,
        Notation::Peg => peg::parse_peg(source)?,
        Notation::Railroad => ir::parse_ir(source)?,
    };
    let acc_title = parsed.acc_title.map(|s| s.trim_start().to_string());
    let acc_descr = parsed.acc_descr.map(|s| {
        let mut out = String::with_capacity(s.len());
        let mut after_nl = false;
        for ch in s.chars() {
            if ch == '\n' {
                out.push('\n');
                after_nl = true;
            } else if after_nl && ch.is_whitespace() {
                continue;
            } else {
                after_nl = false;
                out.push(ch);
            }
        }
        out
    });
    let title = parsed.title.or_else(|| front_title.filter(|t| !t.is_empty()));
    Ok(RailroadModel { notation, title, acc_title, acc_descr, rules: parsed.rules })
}
