use super::color_slots::assign_color_slots;
use super::db::AgentflowDb;
use super::shapes::{normalise_node_shapes, resolve_shape_alias};
use super::types::{DiagnosticId, LabelType, Metadata};
use crate::chars::is_space;
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
    // Palette slot of a container.
    pub color_index: Option<usize>,
    // What a node is, which picks its palette slot.
    pub kind: Option<String>,
    pub metadata: Option<Metadata>,
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
    pub animate: Option<Value>,
    pub animation: Option<Value>,
    pub curve: Option<String>,
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

fn compiled<'a>(db: &AgentflowDb, class_names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out = Vec::new();
    for name in class_names {
        if let Some(def) = db.class(name) {
            out.extend(def.styles.iter().map(|s| s.trim_matches(is_space).to_string()));
            out.extend(def.text_styles.iter().map(|s| s.trim_matches(is_space).to_string()));
        }
    }
    out
}

fn is_collapsed_meta(metadata: Option<&Metadata>) -> bool {
    metadata.is_some_and(|m| {
        m.iter()
            .find(|(k, _)| k == "view")
            .is_some_and(|(_, v)| v.as_str() == Some("collapsed"))
    })
}

// Resolves a parsed agentflow into the nodes and edges that are drawn: flow membership, collapsed
// flows, shapes reduced to the ones agentflow has, class styles and palette slots. Shape and
// containment problems are reported on the model as diagnostics.
pub fn build_agent_graph(
    db: &mut AgentflowDb,
    palette_length: usize,
    default_curve: Option<&str>,
) -> AgentGraph {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    // Everything inside a collapsed flow is hidden, and stands for its outermost collapsed flow.
    // Mermaid walks the whole inside of each collapsed flow, in the order the flows closed, and the
    // last walk to reach a thing wins. A thing has one container, so a later walk can only enter an
    // earlier one through its root: it stops there, and the root's owner is followed afterwards.
    let mut hidden_set: HashSet<String> = HashSet::new();
    let mut hidden_list: Vec<String> = Vec::new();
    let mut owner: HashMap<String, String> = HashMap::new();
    let mut walked: HashSet<String> = HashSet::new();
    for sg_idx in 0..db.subgraphs.len() {
        let sg_id = db.subgraphs[sg_idx].id.clone();
        if !is_collapsed_meta(db.subgraphs[sg_idx].metadata.as_ref()) || hidden_set.contains(&sg_id) {
            continue;
        }
        let mut pending = vec![sg_id.clone()];
        while let Some(cur) = pending.pop() {
            let Some(members) = db.subgraph(&cur).map(|s| s.nodes.clone()) else {
                continue;
            };
            for child_id in members {
                if child_id == sg_id || hidden_set.contains(&child_id) {
                    continue;
                }
                hidden_set.insert(child_id.clone());
                hidden_list.push(child_id.clone());
                owner.insert(child_id.clone(), sg_id.clone());
                if !walked.contains(&child_id) {
                    pending.push(child_id);
                }
            }
        }
        walked.insert(sg_id);
    }

    let mut collapsed_into: HashMap<String, String> = HashMap::new();
    for id in &hidden_list {
        let mut path = vec![id.clone()];
        let mut top = owner.get(id).unwrap().clone();
        loop {
            let known = collapsed_into.get(&top).cloned();
            let up = known.clone().or_else(|| owner.get(&top).cloned());
            let Some(up) = up else {
                break;
            };
            if known.is_none() {
                path.push(top);
            }
            top = up;
            if known.is_some() {
                break;
            }
        }
        for p in path {
            collapsed_into.insert(p, top.clone());
        }
    }

    let sg_count = db.subgraphs.len();
    let mut position: HashMap<String, usize> = HashMap::with_capacity(sg_count);
    for (i, sg) in db.subgraphs.iter().enumerate() {
        position.insert(sg.id.clone(), i);
    }

    // Two flows can each name the other. The nesting that would close the loop is dropped.
    let mut parent_of: HashMap<String, String> = HashMap::new();
    let closes_loop =
        |child_id: &str, parent_id: &str, at: usize, parent_of: &HashMap<String, String>| -> bool {
            // Only a flow handled earlier in this pass can already be above the parent.
            let Some(&child_at) = position.get(child_id) else {
                return false;
            };
            if child_at < at {
                return false;
            }
            let mut current: Option<&str> = Some(parent_id);
            let mut hops = 0;
            while let Some(cur) = current {
                if hops > sg_count {
                    break;
                }
                if cur == child_id {
                    return true;
                }
                current = parent_of.get(cur).map(String::as_str);
                hops += 1;
            }
            false
        };

    let mut containment_warnings: Vec<(String, String)> = Vec::new();
    for i in (0..sg_count).rev() {
        let sg = &db.subgraphs[i];
        if hidden_set.contains(&sg.id) {
            continue;
        }
        for id in &sg.nodes {
            if closes_loop(id, &sg.id, i, &parent_of) {
                containment_warnings.push((
                    id.clone(),
                    format!(
                        "Container \"{}\" cannot contain \"{}\" because \"{}\" already contains it. The nesting that would close the loop is dropped.",
                        sg.id, id, id
                    ),
                ));
                continue;
            }
            parent_of.insert(id.clone(), sg.id.clone());
        }
    }
    for (node_id, msg) in containment_warnings {
        db.emit_warning(DiagnosticId::ContainmentViolation, msg, Some(&node_id), None);
    }

    let mut by_id: HashMap<String, usize> = HashMap::new();
    for i in (0..sg_count).rev() {
        let sg = &db.subgraphs[i];
        if hidden_set.contains(&sg.id) {
            continue;
        }
        let collapsed = is_collapsed_meta(sg.metadata.as_ref());
        let metadata = if collapsed {
            let mut m = sg.metadata.clone().unwrap_or_default();
            match m.iter_mut().find(|(k, _)| k == "containerType") {
                Some(entry) => entry.1 = Value::Str(sg.kind.to_string()),
                None => m.push(("containerType".to_string(), Value::Str(sg.kind.to_string()))),
            }
            Some(m)
        } else {
            sg.metadata.clone()
        };
        let node = GraphNode {
            id: sg.id.clone(),
            label: Some(sg.title.clone()),
            label_type: sg.label_type,
            parent_id: parent_of.get(&sg.id).cloned(),
            css_styles: Vec::new(),
            css_compiled_styles: compiled(db, sg.classes.iter().map(String::as_str)),
            css_classes: sg.classes.join(" "),
            shape: if collapsed {
                "collapsedGroup".to_string()
            } else {
                "flowGroup".to_string()
            },
            dir: sg.dir.clone(),
            link: None,
            link_target: None,
            tooltip: None,
            color_index: None,
            kind: None,
            is_group: !collapsed,
            metadata,
        };
        by_id.insert(sg.id.clone(), nodes.len());
        nodes.push(node);
    }

    for vertex in &db.nodes {
        if hidden_set.contains(&vertex.id) {
            continue;
        }
        if let Some(&idx) = by_id.get(&vertex.id) {
            nodes[idx].css_styles = vertex.styles.clone();
            nodes[idx].css_compiled_styles = compiled(db, vertex.classes.iter().map(String::as_str));
            nodes[idx].css_classes = vertex.classes.join(" ");
            continue;
        }
        let resolved_type = resolve_shape_alias(vertex.shape.as_deref());
        let class_chain = ["default", "node"]
            .into_iter()
            .chain(vertex.classes.iter().map(String::as_str));
        let shape = match resolved_type {
            None | Some("square") => "squareRect".to_string(),
            Some(other) => other.to_string(),
        };
        let node = GraphNode {
            id: vertex.id.clone(),
            label: vertex.text.clone(),
            label_type: vertex.label_type,
            parent_id: parent_of.get(&vertex.id).cloned(),
            css_styles: vertex.styles.clone(),
            css_compiled_styles: compiled(db, class_chain),
            css_classes: format!("default {}", vertex.classes.join(" ")),
            dir: vertex.dir.clone(),
            link: vertex.link.clone(),
            link_target: vertex.link_target.clone(),
            tooltip: db.tooltips.get(&vertex.id).cloned(),
            color_index: None,
            kind: None,
            metadata: vertex.metadata.clone(),
            is_group: false,
            shape,
        };
        by_id.insert(vertex.id.clone(), nodes.len());
        nodes.push(node);
    }

    let non_empty = |s: Option<&str>| s.filter(|v| !v.is_empty()).map(String::from);
    for (index, raw) in db.edges.iter().enumerate() {
        let start = collapsed_into
            .get(&raw.start)
            .cloned()
            .unwrap_or_else(|| raw.start.clone());
        let end = collapsed_into
            .get(&raw.end)
            .cloned()
            .unwrap_or_else(|| raw.end.clone());
        // An edge between two things inside one collapsed flow has nothing left to connect.
        if start == end && raw.start != raw.end {
            continue;
        }
        let mut styles = db.default_edge_style.clone().unwrap_or_default();
        if let Some(ref raw_style) = raw.style {
            styles.extend(raw_style.iter().cloned());
        }
        let arrow = match raw.kind.as_deref() {
            Some("arrow_open") => "none",
            Some("arrow_cross") => "arrow_cross",
            Some("arrow_point") => "arrow_point",
            _ => "arrow_point",
        };
        let curve = non_empty(raw.interpolate.as_deref())
            .or_else(|| non_empty(db.default_interpolate.as_deref()))
            .or_else(|| non_empty(default_curve));
        edges.push(GraphEdge {
            id: if raw.id.is_empty() {
                format!("L_{}_{}_{}", start, end, index)
            } else {
                raw.id.clone()
            },
            is_user_defined_id: raw.is_user_defined_id,
            start,
            end,
            kind: raw.kind.clone().unwrap_or_else(|| "normal".to_string()),
            label: raw.text.clone(),
            label_type: raw.label_type,
            thickness: raw.stroke.clone(),
            pattern: raw.stroke.clone(),
            minlen: raw.length,
            classes: "edge-thickness-normal edge-pattern-solid flowchart-link".to_string(),
            arrow_type_start: "none".to_string(),
            arrow_type_end: arrow.to_string(),
            css_compiled_styles: compiled(db, raw.classes.iter().map(String::as_str)),
            style: styles,
            animate: raw.animate.clone(),
            animation: raw.animation.clone(),
            curve,
            metadata: raw.metadata.clone(),
        });
    }

    normalise_node_shapes(&mut nodes, db);

    // Containers are numbered in the order they were written: a walk of the nesting, parents first.
    // `subgraphs` itself is in the order the blocks closed, with a nested flow before its parent.
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for sg in &db.subgraphs {
        let Some(parent) = parent_of.get(&sg.id) else {
            continue;
        };
        children
            .entry(parent.clone())
            .or_default()
            .push(sg.id.clone());
    }
    let mut order: HashMap<String, usize> = HashMap::new();
    for sg in &db.subgraphs {
        if parent_of.contains_key(&sg.id) {
            continue;
        }
        let mut pending = vec![sg.id.clone()];
        while let Some(id) = pending.pop() {
            if order.contains_key(&id) {
                continue;
            }
            let next_idx = order.len();
            order.insert(id.clone(), next_idx);
            if let Some(kids) = children.get(&id) {
                for kid in kids.iter().rev() {
                    pending.push(kid.clone());
                }
            }
        }
    }

    assign_color_slots(
        &mut nodes,
        |id| {
            if db.is_connector(id) {
                Some("connector")
            } else {
                db.node(id).map(|v| db.vertex_kind(v).as_str())
            }
        },
        &order,
        palette_length,
    );

    AgentGraph { nodes, edges }
}
