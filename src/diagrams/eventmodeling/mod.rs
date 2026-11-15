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
    line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let ast = parser::parse_event_model(source)?;
    let built = model::build_event_model(ast, title, line_offset)?;
    Ok(render::render_event_model(&built, config, options))
}
