use crate::diagrams::common::outline::{parse_outline, tokenize_outline, OutlineDb, Tokens};
use crate::errors::Result;

pub fn tokenize(src: &str) -> Tokens {
    let chars: Vec<char> = src.chars().collect();
    tokenize_outline(&chars, false)
}

pub fn parse_mindmap(src: &str, db: &mut dyn OutlineDb) -> Result<()> {
    parse_outline(src, db, false)
}
