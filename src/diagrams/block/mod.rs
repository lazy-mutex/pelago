pub mod arrows;
pub mod db;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod tokens;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;

use db::BlockDb;
use parser::parse_block;
use render::render_block;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = BlockDb::new();
    db.title = title;
    parse_block(source, &mut db)?;
    render_block(&db, config, options)
}
