pub mod db;
pub mod graph;
pub mod lexer;
pub mod markers;
pub mod members;
pub mod parser;
pub mod render;
pub mod tokens;
pub mod types;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;
use db::ClassDb;
use parser::parse_class_diagram;
use render::render_class;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = ClassDb::new();
    if let Some(t) = title.filter(|s| !s.is_empty()) {
        db.title = Some(t);
    }
    parse_class_diagram(source, &mut db)?;
    Ok(render_class(&db, config, options))
}
