pub mod db;
pub mod graph;
pub mod lexer;
pub mod markers;
pub mod parser;
pub mod render;
pub mod tokens;
pub mod types;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;
use db::ErDb;
use parser::parse_er;
use render::render_er;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = ErDb::default();
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        db.title = Some(t);
    }
    parse_er(source, &mut db)?;
    Ok(render_er(&db, config, options))
}
