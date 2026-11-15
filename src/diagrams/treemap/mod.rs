pub mod format;
pub mod layout;
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
    let ast = parser::parse_treemap(source)?;
    let model = model::build_treemap(ast, title);
    Ok(render::render_treemap(&model, config, options))
}
