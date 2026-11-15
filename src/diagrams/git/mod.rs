pub mod db;
pub mod parser;
pub mod render;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;

use db::build_git;
use parser::parse_git;
use render::render_git;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let ast = parse_git(source)?;
    let model = build_git(ast, config, title)?;
    Ok(render_git(&model, config, options))
}
