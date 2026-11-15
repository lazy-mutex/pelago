pub mod builder;
pub mod model;
pub mod parser;
pub mod render;

use model::build_wardley;
use parser::parse_wardley;
use render::render_wardley;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    let ast = parse_wardley(source)?;
    let model = build_wardley(ast, title)?;
    Ok(render_wardley(&model, config, options))
}
