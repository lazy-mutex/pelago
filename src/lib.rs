//! Renders Mermaid diagrams to SVG. A Rust port of Pele (https://pele.run).
#![allow(clippy::needless_range_loop, clippy::too_many_arguments, clippy::type_complexity)]

pub mod chars;
pub mod detect;
pub mod diagrams;
pub mod errors;
pub mod layout;
pub mod options;
pub mod preprocess;
pub mod svg;
pub mod text;
pub mod url;
pub mod value;

use detect::detect_diagram;
use diagrams::flowchart::{db::FlowDb, parser::parse_flowchart, render::render_flowchart};
use diagrams::railroad::{parse_railroad, render_railroad};
use diagrams::swimlane::render_swimlane;
use errors::{ErrorCode, PeleError, Result};
pub use options::{LinkInfo, RenderOptions, Rendered, Theme};
use preprocess::{encode_entities, preprocess, source_edit_steps};
use svg::resolve::resolve_svg;
use text::entities::{decode_entities, written_codes};
use value::Value;

// Mermaid's default maxTextSize.
const DEFAULT_LIMIT: usize = 50000;
// Mermaid has no limit on its output. A source within the limit can still describe a drawing
// of many megabytes.
const DEFAULT_OUTPUT_LIMIT: usize = 4_000_000;
// Mermaid stops at 500. `A & B --> C & D` multiplies: without a limit, a few kilobytes of
// source ask for millions of edges.
const DEFAULT_MAX_EDGES: usize = 5000;

// All 33 Pele diagram types.
pub const SUPPORTED: &[&str] = &[
    "agentflow",
    "architecture",
    "block",
    "c4",
    "class",
    "cynefin",
    "er",
    "eventmodeling",
    "flowchart",
    "gantt",
    "gitGraph",
    "info",
    "ishikawa",
    "journey",
    "kanban",
    "mindmap",
    "packet",
    "pie",
    "quadrantChart",
    "radar",
    "railroad",
    "requirement",
    "sankey",
    "sequence",
    "state",
    "swimlane",
    "timeline",
    "treemap",
    "treeView",
    "usecase",
    "venn",
    "wardley",
    "xychart",
];

#[derive(Debug, Clone)]
pub struct RenderResult {
    // Pele's name for the diagram type, and the type as Mermaid reports it.
    pub diagram: &'static str,
    pub mermaid: &'static str,
    pub svg: String,
    pub width: f64,
    pub height: f64,
    pub links: Vec<LinkInfo>,
}

pub fn detect_type(text: &str) -> Option<&'static str> {
    preprocess(text).ok().and_then(|pre| detect_diagram(&pre.text)).map(|found| found.diagram)
}

pub fn supports(text: &str) -> bool {
    detect_type(text).is_some_and(|kind| SUPPORTED.contains(&kind))
}

fn tagged(mut error: PeleError, diagram: &'static str, raw_text: &str, parsed_source: &str, keep_comments: bool) -> PeleError {
    error.diagram.get_or_insert(diagram);
    if error.raw_offset.is_some() {
        let parsed_chars: Vec<char> = parsed_source.chars().collect();
        let (orig_chars, steps) = source_edit_steps(raw_text, keep_comments);
        error.relocate(&parsed_chars, &orig_chars, &steps);
    }
    error.message = written_codes(&error.message);
    error.snippet = written_codes(&error.snippet);
    error
}

pub fn render(text: &str, options: &RenderOptions) -> Result<RenderResult> {
    let limit = options.limit.unwrap_or(DEFAULT_LIMIT);
    if text.encode_utf16().count() > limit {
        return Err(PeleError::new(format!("Diagram source is longer than the limit of {} characters.", limit), ErrorCode::Limit));
    }
    let pre = preprocess(text)?;
    let Some(found) = detect_diagram(&pre.text) else {
        return Err(PeleError::new("No diagram type detected for the given text.", ErrorCode::UnsupportedDiagram));
    };
    if !SUPPORTED.contains(&found.diagram) {
        return Err(PeleError::new(format!("Diagram type \"{}\" is not supported yet.", found.diagram), ErrorCode::UnsupportedDiagram).of(found.diagram));
    }
    let mut config = options.config.clone().filter(Value::is_object).unwrap_or_else(Value::object);
    config.merge(&pre.config);
    let keep_comments = matches!(found.diagram, "agentflow" | "eventmodeling");
    let source = format!(
        "{}\n",
        encode_entities(if keep_comments { &pre.with_comments } else { &pre.text })
    );
    let section_key = match found.diagram {
        "xychart" => "xyChart",
        "wardley" => "wardley-beta",
        other => other,
    };
    let fixed = config.get(section_key).get("useMaxWidth").as_bool() == Some(false);
    let mut effective_options = options.clone();
    effective_options.responsive = Some(options.responsive.unwrap_or(!fixed));
    let max_edges = options.max_edges.unwrap_or(DEFAULT_MAX_EDGES);
    let tag = |e: PeleError| tagged(e, found.diagram, text, &source, keep_comments);

    let mut rendered = match found.diagram {
        "agentflow" => diagrams::agentflow::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "architecture" => diagrams::architecture::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "block" => diagrams::block::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "c4" => diagrams::c4::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "class" => diagrams::class::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "cynefin" => diagrams::cynefin::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "er" => diagrams::er::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "eventmodeling" => diagrams::eventmodeling::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "gantt" => diagrams::gantt::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "gitGraph" => diagrams::git::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "info" => diagrams::info::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "ishikawa" => diagrams::ishikawa::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "journey" => diagrams::journey::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "kanban" => diagrams::kanban::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "mindmap" => diagrams::mindmap::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "packet" => diagrams::packet::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "pie" => diagrams::pie::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "quadrantChart" => diagrams::quadrant::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "radar" => diagrams::radar::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "railroad" => {
            let model = parse_railroad(&source, pre.title).map_err(tag)?;
            render_railroad(&model, &config, &effective_options)
        }
        "requirement" => diagrams::requirement::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "sankey" => diagrams::sankey::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "sequence" => diagrams::sequence::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "state" => diagrams::state::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "swimlane" => {
            let mut db = FlowDb::default();
            let own = config.get("swimlane");
            let flow_cfg = if own.is_object() { own } else { config.get("flowchart") };
            db.inherit_dir = flow_cfg.get("inheritDir").as_bool() == Some(true);
            db.max_edges = Some(max_edges);
            db.title = pre.title.filter(|t| !t.is_empty());
            parse_flowchart(&source, &mut db).map_err(tag)?;
            render_swimlane(&db, &config, &effective_options)
        }
        "timeline" => diagrams::timeline::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "treemap" => diagrams::treemap::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "treeView" => diagrams::treeview::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "usecase" => diagrams::usecase::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "venn" => diagrams::venn::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "wardley" => diagrams::wardley::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        "xychart" => diagrams::xychart::render_diagram(&source, &config, &effective_options, pre.title, pre.line_offset, max_edges).map_err(tag)?,
        _ => {
            let mut db = FlowDb::default();
            db.inherit_dir = config.get("flowchart").get("inheritDir").as_bool() == Some(true);
            db.max_edges = Some(max_edges);
            db.title = pre.title.filter(|t| !t.is_empty());
            parse_flowchart(&source, &mut db).map_err(tag)?;
            render_flowchart(&db, &config, &effective_options)
        }
    };
    if !options.raw {
        rendered.svg = resolve_svg(&rendered.svg, options);
    }

    let output_limit = options.output_limit.unwrap_or(DEFAULT_OUTPUT_LIMIT);
    if rendered.svg.len() > output_limit {
        return Err(PeleError::new(format!("Diagram output is longer than the limit of {} characters.", output_limit), ErrorCode::Limit).of(found.diagram));
    }
    for link in &mut rendered.links {
        link.id = decode_entities(&link.id);
    }
    Ok(RenderResult { diagram: found.diagram, mermaid: found.mermaid, svg: rendered.svg, width: rendered.width, height: rendered.height, links: rendered.links })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use crate::value::parse_json;
    use crate::{detect_type, render, supports, RenderOptions};

    #[link(wasm_import_module = "typst_env")]
    extern "C" {
        fn wasm_minimal_protocol_send_result_to_host(ptr: *const u8, len: usize);
        fn wasm_minimal_protocol_write_args_to_buffer(ptr: *mut u8);
    }

    fn read_args(lens: &[usize]) -> Vec<u8> {
        let total: usize = lens.iter().sum();
        let mut buf = vec![0u8; total];
        if total > 0 {
            unsafe {
                wasm_minimal_protocol_write_args_to_buffer(buf.as_mut_ptr());
            }
        }
        buf
    }

    fn send_ok(bytes: &[u8]) -> i32 {
        unsafe {
            wasm_minimal_protocol_send_result_to_host(bytes.as_ptr(), bytes.len());
        }
        0
    }

    fn send_err(msg: &str) -> i32 {
        let bytes = msg.as_bytes();
        unsafe {
            wasm_minimal_protocol_send_result_to_host(bytes.as_ptr(), bytes.len());
        }
        1
    }

    #[export_name = "render"]
    pub extern "C" fn wasm_render(source_len: usize) -> i32 {
        let buf = read_args(&[source_len]);
        let Ok(source) = std::str::from_utf8(&buf) else {
            return send_err("Mermaid source is not valid UTF-8.");
        };
        match render(source, &RenderOptions::default()) {
            Ok(res) => send_ok(res.svg.as_bytes()),
            Err(err) => send_err(&err.message),
        }
    }

    #[export_name = "render_with_options"]
    pub extern "C" fn wasm_render_with_options(source_len: usize, options_len: usize) -> i32 {
        let buf = read_args(&[source_len, options_len]);
        let (Ok(source), Ok(opts_str)) = (std::str::from_utf8(&buf[..source_len]), std::str::from_utf8(&buf[source_len..])) else {
            return send_err("Plugin arguments must be valid UTF-8.");
        };
        let options = if opts_str.trim().is_empty() {
            RenderOptions::default()
        } else {
            match parse_json(opts_str) {
                Ok(val) => RenderOptions::from_json_value(&val),
                Err(err) => return send_err(&format!("Invalid render options JSON: {}", err)),
            }
        };
        match render(source, &options) {
            Ok(res) => send_ok(res.svg.as_bytes()),
            Err(err) => send_err(&err.message),
        }
    }

    #[export_name = "supports"]
    pub extern "C" fn wasm_supports(source_len: usize) -> i32 {
        let buf = read_args(&[source_len]);
        let Ok(source) = std::str::from_utf8(&buf) else {
            return send_err("Mermaid source is not valid UTF-8.");
        };
        send_ok(if supports(source) { b"1" } else { b"0" })
    }

    #[export_name = "detect_type"]
    pub extern "C" fn wasm_detect_type(source_len: usize) -> i32 {
        let buf = read_args(&[source_len]);
        let Ok(source) = std::str::from_utf8(&buf) else {
            return send_err("Mermaid source is not valid UTF-8.");
        };
        send_ok(detect_type(source).unwrap_or("").as_bytes())
    }
}
