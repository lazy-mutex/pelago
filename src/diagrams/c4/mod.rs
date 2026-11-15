pub mod db;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod route;
pub mod tokens;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;

use db::C4Db;
use parser::parse_c4;
use render::render_c4;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = C4Db::new();
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        db.title = Some(t);
    }
    parse_c4(source, &mut db)?;
    render_c4(&db, config, options)
}
