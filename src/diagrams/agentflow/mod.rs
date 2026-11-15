pub mod color_slots;
pub mod db;
pub mod diagnostics;
pub mod graph;
pub mod lexer;
pub mod parser;
pub mod render;
pub mod shapes;
pub mod tokens;
pub mod types;

pub const KEEP_COMMENTS: bool = true;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    line_offset: usize,
    max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    // Mermaid reads this setting from the flowchart section for agentflow too.
    let flow = config.get("flowchart");
    let mut db = db::AgentflowDb::new(db::AgentflowDbOptions {
        inherit_dir: flow.get("inheritDir").as_bool() == Some(true),
        direction: config.get("direction").as_str().map(String::from),
        line_offset,
        source: Some(source.to_string()),
        max_edges: Some(max_edges),
    });
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        db.title = Some(t);
    }
    parser::parse_agentflow(source, &mut db)?;
    render::agent_graph(&mut db, config)?;
    Ok(render::render_agentflow(&db, config, options))
}
