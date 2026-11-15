pub mod box_drawing;
pub mod icons;
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
    let ast = model::read_tree_view(source)?;
    let tree = model::build_tree_view(ast, title);
    Ok(render::render_tree_view(&tree, config, options))
}
