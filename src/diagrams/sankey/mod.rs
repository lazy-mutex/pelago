pub mod db;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod render;

use crate::errors::Result;
use crate::options::{RenderOptions, Rendered};
use crate::value::Value;

use db::SankeyDb;
use parser::{parse_sankey, prepare_text_for_parsing};
use render::render_sankey;

pub fn render_diagram(
    source: &str,
    config: &Value,
    options: &RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> Result<Rendered> {
    let mut db = SankeyDb::new();
    if let Some(t) = title.filter(|s| !s.is_empty()) {
        db.title = Some(t);
    }
    let prepared = prepare_text_for_parsing(source, None);
    parse_sankey(&prepared, &mut db, Some(source))?;
    render_sankey(&db, config, options)
}
