pub mod db;
pub mod parser;
pub mod render;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut database = db::KanbanDb::default();
    database.title = title;
    parser::parse_kanban(source, &mut database)?;
    Ok(render::render_kanban(&database, config, options))
}
