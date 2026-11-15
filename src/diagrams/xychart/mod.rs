pub mod db;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod ticks;
pub mod tokens;

use db::XyChartDb;
use parser::parse_xychart;
use render::render_xychart;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    let mut db = XyChartDb { title, ..Default::default() };
    parse_xychart(source, &mut db)?;
    render_xychart(&db, config, options)
}
