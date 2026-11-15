pub mod db;
pub mod icons;
pub mod layout;
pub mod parser;
pub mod render;

pub fn render_diagram(
    source: &str,
    config: &crate::value::Value,
    options: &crate::options::RenderOptions,
    title: Option<String>,
    _line_offset: usize,
    _max_edges: usize,
) -> crate::errors::Result<crate::options::Rendered> {
    let mut model = db::ArchitectureDb::default();
    model.title = title.filter(|t| !t.is_empty());
    let ast = parser::parse_architecture(source)?;
    db::populate(ast, &mut model)?;
    Ok(render::render_architecture(&model, config, options))
}
