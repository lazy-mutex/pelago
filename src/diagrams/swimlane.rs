use super::flowchart::db::{FlowDb, LabelType};
use super::flowchart::graph::{FlowGraph, GraphNode};
use super::flowchart::render::{render_flowchart_variant, FlowVariant};
use crate::layout::compound::{CEdge, CNode, CompoundResult, Dir};
use crate::layout::lanes::lane_layout;
use crate::layout::layered::LayeredOptions;
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{label_svg, num};
use crate::svg::ink::ink_fill;
use crate::svg::theme::ResolvedStyle;
use crate::text::label::Label;
use crate::value::Value;

const DEFAULT_LANE: &str = "__swimlane_default__";

// Nodes outside every lane share one unnamed lane, as in Mermaid. Lanes all run the same way,
// so a `direction` inside a subgraph has no effect.
fn prepare(graph: &mut FlowGraph) {
    let mut loose = false;
    for node in &mut graph.nodes {
        if node.is_group {
            node.dir = None;
        } else if node.parent_id.is_none() {
            loose = true;
        }
    }
    if !loose {
        return;
    }
    for node in &mut graph.nodes {
        if !node.is_group && node.parent_id.is_none() {
            node.parent_id = Some(DEFAULT_LANE.to_string());
        }
    }
    graph.nodes.push(GraphNode {
        id: DEFAULT_LANE.to_string(),
        label: None,
        label_type: LabelType::Text,
        shape: "rect".to_string(),
        is_group: true,
        parent_id: None,
        css_styles: Vec::new(),
        css_compiled_styles: Vec::new(),
        css_classes: String::new(),
        dir: None,
        ..Default::default()
    });
}

// A lane: an outline the length of the diagram, with its title in a band at the start.
fn draw_lane(c: &CNode, label: &Label, style: &ResolvedStyle, classes: &str, id: &str, dir: Dir) -> String {
    let left = c.x - c.w / 2.0;
    let top = c.y - c.h / 2.0;
    let sideways = matches!(dir, Dir::LR | Dir::RL);
    let cx = if sideways { left + c.pad_top / 2.0 } else { c.x };
    let cy = if sideways { c.y } else { top + c.pad_top / 2.0 };
    let text = label_svg(
        label,
        cx,
        cy,
        &format!(" class=\"pele-cluster-label\"{}{}", ink_fill(&style.shape, &style.text, Some("var(--_m)")), style.text),
    );
    let title_svg = if sideways && !text.is_empty() {
        format!("<g transform=\"rotate(-90 {} {})\">{}</g>", num(cx), num(cy), text)
    } else {
        text
    };
    format!(
        "<g class=\"pele-cluster pele-lane{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"var(--_b)\"/><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\" stroke=\"var(--_b)\"{}/>{}</g>",
        classes,
        id,
        num(left),
        num(top),
        num(c.w),
        num(c.h),
        num(left),
        num(top),
        num(if sideways { c.pad_top } else { c.w }),
        num(if sideways { c.h } else { c.pad_top }),
        style.shape,
        title_svg
    )
}

fn lane_band(c: &CNode, dir: Dir) -> [f64; 4] {
    let left = c.x - c.w / 2.0;
    let top = c.y - c.h / 2.0;
    if matches!(dir, Dir::LR | Dir::RL) {
        [left, top, left + c.pad_top, top + c.h]
    } else {
        [left, top, left + c.w, top + c.pad_top]
    }
}

fn swimlane_layout(nodes: &mut [CNode], edges: &mut [CEdge], dir: Dir, options: &LayeredOptions, tight: bool) -> CompoundResult {
    lane_layout(nodes, edges, dir, options, if tight { Some(12.0) } else { None })
}

const SWIMLANE_VARIANT: FlowVariant = FlowVariant {
    kind: "swimlane",
    prepare,
    layout: swimlane_layout,
    draw_group: Some(draw_lane),
    filled_box: Some(lane_band),
};

// A swimlane diagram is a flowchart whose top-level subgraphs are lanes: same syntax, same model.
pub fn render_swimlane(db: &FlowDb, config: &Value, options: &RenderOptions) -> Rendered {
    render_flowchart_variant(db, config, options, Some(&SWIMLANE_VARIANT))
}
