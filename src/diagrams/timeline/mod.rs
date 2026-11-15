pub mod db;
pub mod lexer;
pub mod parser;
pub mod render;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;
use db::TimelineDb;
use parser::parse_timeline;
use render::render_timeline;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = TimelineDb::default();
    db.title = title;
    parse_timeline(source, &mut db)?;
    Ok(render_timeline(&db, config, options))
}
