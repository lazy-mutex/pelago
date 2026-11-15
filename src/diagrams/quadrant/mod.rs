pub mod db;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod tokens;

use db::QuadrantDb;
use parser::parse_quadrant;
use render::render_quadrant;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    let mut db = QuadrantDb { title, ..Default::default() };
    parse_quadrant(source, &mut db)?;
    Ok(render_quadrant(&db, config, options))
}
