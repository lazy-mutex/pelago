pub mod axis;
pub mod dates;
pub mod db;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod tokens;

use self::db::GanttDb;
use self::parser::parse_gantt;
use self::render::render_gantt;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    let mut db = GanttDb::new();
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        db.title = Some(t);
    }
    if let Some(mode) = config
        .get("gantt")
        .get("displayMode")
        .as_str()
        .or_else(|| config.get("displayMode").as_str())
    {
        db.display_mode = mode.to_string();
    }
    parse_gantt(source, &mut db)?;
    render_gantt(&mut db, config, options)
}
