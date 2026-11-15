pub mod model;
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
    let ast = parser::parse_radar(source)?;
    let model = model::build_radar(ast, title)?;
    Ok(render::render_radar(&model, config, options))
}
