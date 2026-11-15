pub mod db;
pub mod lexer;
pub mod parser;
pub mod render;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;
use db::JourneyDb;
use parser::parse_journey;
use render::render_journey;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = JourneyDb::default();
    db.title = title;
    parse_journey(source, &mut db)?;
    Ok(render_journey(&db, config, options))
}
