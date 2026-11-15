use super::db::{FlowDb, FlowNode, LabelType};
use crate::value::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct GraphNode {
    pub id: String,
    pub label: Option<String>,
    pub label_type: LabelType,
    pub shape: String,
    pub is_group: bool,
    pub parent_id: Option<String>,
    pub css_styles: Vec<String>,
    pub css_compiled_styles: Vec<String>,
    pub css_classes: String,
    pub dir: Option<String>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub tooltip: Option<String>,
    pub icon: Option<String>,
    pub pos: Option<String>,
    pub img: Option<String>,
    pub asset_width: Option<f64>,
    pub asset_height: Option<f64>,
    pub constraint: Option<String>,
    pub color_index: Option<usize>,
    pub metadata: Vec<(String, Value)>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphEdge {
    pub id: String,
    pub is_user_defined_id: bool,
    pub start: String,
    pub end: String,
    pub kind: String,
    pub label: String,
    pub label_type: LabelType,
    pub thickness: Option<String>,
    pub pattern: Option<String>,
    pub minlen: Option<usize>,
    pub arrow_type_start: String,
    pub arrow_type_end: String,
    pub css_compiled_styles: Vec<String>,
    pub style: Vec<String>,
    pub classes: String,
    pub animate: Option<bool>,
    pub animation: Option<String>,
    pub curve: Option<String>,
}

pub struct FlowGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

fn shape_of(vertex: &FlowNode) -> String {
    if vertex.img.is_some() {
        return "imageSquare".into();
    }
    if vertex.icon.is_some() {
        return match vertex.form.as_deref() {
            Some("circle") => "iconCircle",
            Some("square") => "iconSquare",
            Some("rounded") => "iconRounded",
            _ => "icon",
        }
        .into();
    }
    match vertex.shape.as_deref() {
        Some("square") | None => "squareRect".into(),
        Some("round") => "roundedRect".into(),
        Some(other) => other.into(),
    }
}

fn arrows(kind: Option<&str>) -> (String, String) {
    match kind {
        Some(k @ ("arrow_point" | "arrow_circle" | "arrow_cross")) => ("none".into(), k.into()),
        Some(k @ ("double_arrow_point" | "double_arrow_circle" | "double_arrow_cross")) => {
            let single = k.replacen("double_", "", 1);
            (single.clone(), single)
        }
        _ => ("none".into(), "arrow_point".into()),
    }
}

fn collapsed(metadata: &[(String, Value)]) -> bool {
    metadata.iter().any(|(k, v)| k == "view" && v.as_str() == Some("collapsed"))
}

// Resolves a parsed flowchart into the nodes and edges that are actually drawn:
// group membership, collapsed subgraphs, class styles and edge defaults.
pub fn build_flow_graph(model: &FlowDb, default_curve: Option<&str>) -> FlowGraph {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let subgraphs = &model.subgraphs;

    let compiled = |class_names: &mut dyn Iterator<Item = &str>| -> Vec<String> {
        let mut out = Vec::new();
        for name in class_names {
            let Some(def) = model.class(name) else { continue };
            out.extend(def.styles.iter().map(|s| s.trim().to_string()));
            out.extend(def.text_styles.iter().map(|s| s.trim().to_string()));
        }
        out
    };

    let mut subgraph_parent: HashMap<&str, &str> = HashMap::new();
    for sg in subgraphs {
        for child in &sg.nodes {
            if model.subgraph(child).is_some() {
                subgraph_parent.insert(child, &sg.id);
            }
        }
    }

    let mut declaration_index: HashMap<&str, usize> = HashMap::new();
    let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for sg in subgraphs {
        if let Some(parent) = subgraph_parent.get(sg.id.as_str()) {
            children_of.entry(parent).or_default().push(&sg.id);
        }
    }
    let mut next_index = 0;
    for sg in subgraphs {
        if subgraph_parent.contains_key(sg.id.as_str()) {
            continue;
        }
        let mut pending: Vec<&str> = vec![&sg.id];
        while let Some(id) = pending.pop() {
            if declaration_index.contains_key(id) {
                continue;
            }
            declaration_index.insert(id, next_index);
            next_index += 1;
            if let Some(kids) = children_of.get(id) {
                pending.extend(kids.iter().rev());
            }
        }
    }

    let is_collapsed = |id: &str| model.subgraph(id).is_some_and(|sg| collapsed(&sg.metadata));
    let any_collapsed = subgraphs.iter().any(|sg| collapsed(&sg.metadata));
    // The outermost collapsed subgraph around a subgraph.
    let outermost_collapsed = |id: &str| -> Option<String> {
        let mut chain: Vec<&str> = Vec::new();
        let mut current = Some(id);
        while let Some(c) = current {
            if chain.contains(&c) {
                break;
            }
            chain.push(c);
            current = subgraph_parent.get(c).copied();
        }
        chain.iter().rev().find(|c| is_collapsed(c)).map(|c| c.to_string())
    };

    let mut hidden: HashSet<String> = HashSet::new();
    let mut collapsed_into: HashMap<String, String> = HashMap::new();
    if any_collapsed {
        for sg in subgraphs {
            let Some(ancestor) = outermost_collapsed(&sg.id) else { continue };
            if sg.id != ancestor {
                hidden.insert(sg.id.clone());
                collapsed_into.insert(sg.id.clone(), ancestor.clone());
            }
            for child in &sg.nodes {
                if *child == ancestor {
                    continue;
                }
                hidden.insert(child.clone());
                collapsed_into.insert(child.clone(), ancestor.clone());
            }
        }
    }

    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    let mut groups: HashSet<&str> = HashSet::new();
    for sg in subgraphs.iter().rev() {
        if hidden.contains(&sg.id) {
            continue;
        }
        if !sg.nodes.is_empty() {
            groups.insert(&sg.id);
        }
        for id in &sg.nodes {
            parent_of.insert(id, &sg.id);
        }
    }

    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for sg in subgraphs.iter().rev() {
        if hidden.contains(&sg.id) {
            continue;
        }
        let folded = collapsed(&sg.metadata);
        nodes.push(GraphNode {
            id: sg.id.clone(),
            label: Some(sg.title.clone()),
            label_type: sg.label_type,
            parent_id: parent_of.get(sg.id.as_str()).map(|p| p.to_string()),
            css_compiled_styles: compiled(&mut sg.classes.iter().map(String::as_str)),
            css_classes: sg.classes.join(" "),
            shape: if folded { "collapsedGroup" } else { "rect" }.into(),
            dir: sg.dir.clone(),
            is_group: !folded,
            color_index: declaration_index.get(sg.id.as_str()).copied(),
            metadata: if folded { Vec::new() } else { sg.metadata.clone() },
            ..Default::default()
        });
        by_id.insert(&sg.id, nodes.len() - 1);
    }

    for vertex in &model.nodes {
        if hidden.contains(&vertex.id) {
            continue;
        }
        if let Some(&at) = by_id.get(vertex.id.as_str()) {
            let existing = &mut nodes[at];
            existing.css_styles = vertex.styles.clone();
            existing.css_compiled_styles = compiled(&mut vertex.classes.iter().map(String::as_str));
            existing.css_classes = vertex.classes.join(" ");
            continue;
        }
        let is_group = groups.contains(vertex.id.as_str());
        nodes.push(GraphNode {
            id: vertex.id.clone(),
            label: vertex.text.clone(),
            label_type: vertex.label_type,
            parent_id: parent_of.get(vertex.id.as_str()).map(|p| p.to_string()),
            css_styles: vertex.styles.clone(),
            css_compiled_styles: compiled(&mut ["default", "node"].into_iter().chain(vertex.classes.iter().map(String::as_str))),
            css_classes: format!("default {}", vertex.classes.join(" ")),
            dir: vertex.dir.clone(),
            link: vertex.link.clone(),
            link_target: vertex.link_target.clone(),
            tooltip: model.tooltips.get(&vertex.id).cloned(),
            icon: vertex.icon.clone(),
            pos: vertex.pos.clone(),
            img: vertex.img.clone(),
            asset_width: vertex.asset_width,
            asset_height: vertex.asset_height,
            constraint: vertex.constraint.clone(),
            is_group,
            shape: if is_group { "rect".into() } else { shape_of(vertex) },
            ..Default::default()
        });
    }

    for (index, raw) in model.edges.iter().enumerate() {
        let start = collapsed_into.get(&raw.start).unwrap_or(&raw.start).clone();
        let end = collapsed_into.get(&raw.end).unwrap_or(&raw.end).clone();
        if start == end && (collapsed_into.contains_key(&raw.start) || collapsed_into.contains_key(&raw.end)) {
            continue;
        }
        let (arrow_start, arrow_end) = arrows(raw.kind.as_deref());
        let mut styles = model.default_edge_style.clone().unwrap_or_default();
        if let Some(style) = &raw.style {
            styles.extend(style.iter().cloned());
        }
        let invisible = raw.stroke.as_deref() == Some("invisible");
        let plain = invisible || raw.kind.as_deref() == Some("arrow_open");
        edges.push(GraphEdge {
            id: if raw.id.is_empty() { format!("L_{}_{}_{}", start, end, index) } else { raw.id.clone() },
            is_user_defined_id: raw.is_user_defined_id,
            start,
            end,
            kind: raw.kind.clone().unwrap_or_else(|| "normal".into()),
            label: raw.text.clone(),
            label_type: raw.label_type,
            thickness: raw.stroke.clone(),
            pattern: raw.stroke.clone(),
            minlen: raw.length,
            classes: if invisible { "" } else { "edge-thickness-normal edge-pattern-solid flowchart-link" }.into(),
            arrow_type_start: if plain { "none".into() } else { arrow_start },
            arrow_type_end: if plain { "none".into() } else { arrow_end },
            css_compiled_styles: compiled(&mut raw.classes.iter().map(String::as_str)),
            style: styles,
            animate: raw.animate,
            animation: raw.animation.clone(),
            curve: [raw.interpolate.as_deref(), model.default_interpolate.as_deref(), default_curve].into_iter().flatten().find(|c| !c.is_empty()).map(String::from),
        });
    }

    FlowGraph { nodes, edges }
}
