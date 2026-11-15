pub mod actors;
pub mod colors;
pub mod db;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod tokens;
pub mod types;

use db::{SeqDb, SeqDbOptions};
use parser::parse_sequence;
use render::render_sequence;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    let seq = config.get("sequence");
    let mut db = SeqDb::new(SeqDbOptions {
        wrap: config.get("wrap").as_bool(),
        sequence_wrap: seq.get("wrap").as_bool() == Some(true),
    });
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        db.set_diagram_title(t);
    }
    parse_sequence(source, &mut db)?;
    Ok(render_sequence(&db, config, options))
}
