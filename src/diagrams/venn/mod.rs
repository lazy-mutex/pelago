pub mod db;
pub mod layout;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod types;

use db::VennDb;
use parser::parse_venn;
use render::render_venn;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    let mut db = VennDb::default();
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        db.title = Some(t);
    }
    parse_venn(source, &mut db)?;
    Ok(render_venn(&db, config, options))
}
