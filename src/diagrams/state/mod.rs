pub mod db;
pub mod graph;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod tokens;
pub mod types;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;
use db::StateDb;
use parser::parse_state;
use render::render_state;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = StateDb::new();
    if let Some(t) = title.filter(|s| !s.is_empty()) {
        db.title = Some(t);
    }
    parse_state(source, &mut db)?;
    Ok(render_state(&db, config, options))
}
