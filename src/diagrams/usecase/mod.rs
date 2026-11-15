pub mod db;
pub mod graph;
pub mod json;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod tokens;
pub mod types;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;
pub use parser::parse_usecase;
pub use render::render_usecase;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let model = parse_usecase(source, title, line_offset)?;
    Ok(render_usecase(&model, config, options))
}
