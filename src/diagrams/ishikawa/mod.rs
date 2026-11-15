pub mod db;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod types;

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
    let mut database = db::IshikawaDb::default();
    database.title = title;
    parser::parse_ishikawa(source, &mut database)?;
    Ok(render::render_ishikawa(&database, config, options))
}
