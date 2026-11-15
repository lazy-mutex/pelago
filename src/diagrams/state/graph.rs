use super::types::{Description, StateClassDef, StateStmt, Stmt};
use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct StateNode {
    pub id: String,
    // rect, rectWithTitle, stateStart, stateEnd, choice, fork, join, divider, roundedWithTitle, note or noteGroup.
    pub shape: Option<String>,
    pub label: Option<String>,
    pub description: Vec<String>,
    pub css_classes: String,
    pub css_compiled_styles: Vec<String>,
    pub css_styles: Vec<String>,
    pub kind: Option<String>,
    pub is_group: bool,
    pub parent_id: Option<String>,
    // The direction inside a composite: its last `direction` statement, or TB.
    pub dir: Option<String>,
    pub position: Option<String>,
    pub note_for: Option<String>,
    pub color_index: Option<usize>,
}

// Labels are markdown throughout, as in Mermaid.
#[derive(Debug, Clone)]
pub struct StateEdge {
    pub id: String,
    pub start: String,
    pub end: String,
    pub label: String,
    pub classes: String,
    pub pattern: Option<&'static str>,
}

#[derive(Debug, Clone, Default)]
pub struct StateGraph {
    pub nodes: Vec<StateNode>,
    pub edges: Vec<StateEdge>,
}

struct Draft {
    shape: Option<String>,
    description: Description,
    css_classes: String,
    css_styles: Vec<String>,
    kind: Option<String>,
    dir: Option<String>,
    color_index: Option<usize>,
}

struct Frame<'a> {
    item: Option<&'a StateStmt>,
    doc: &'a [Stmt],
    at: usize,
    alt: bool,
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn direction(doc: &[Stmt]) -> String {
    let mut dir = "TB";
    for item in doc {
        if let Stmt::Dir(d) = item {
            dir = &d.value;
        }
    }
    dir.to_string()
}

// Mermaid's dataFetcher: walks the documents and resolves them into the nodes and edges that are drawn.
pub fn build_state_graph(
    root_doc: &[Stmt],
    states: &HashMap<String, StateStmt>,
    classes: &HashMap<String, StateClassDef>,
) -> Result<StateGraph> {
    let mut nodes: Vec<StateNode> = Vec::new();
    let mut edges: Vec<StateEdge> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut drafts: HashMap<String, Draft> = HashMap::new();
    let mut slots: HashMap<String, Option<usize>> = HashMap::new();
    let mut counter: usize = 0;
    let mut next_slot: usize = 0;

    let insert = |mut node: StateNode, nodes: &mut Vec<StateNode>, by_id: &mut HashMap<String, usize>| {
        // The two literal ids are what DOMPurify leaves of `<<fork>>` and `<<choice>>` in Mermaid; kept for parity.
        if node.id.is_empty() || node.id == "</join></fork>" || node.id == "</choice>" {
            return;
        }
        if !node.css_classes.is_empty() {
            // One copy, however many classes: a state named in a class statement many times has a long list.
            let mut compiled: Option<Vec<String>> = None;
            for name in node.css_classes.split(' ') {
                let Some(def) = classes.get(name) else {
                    continue;
                };
                let list = compiled.get_or_insert_with(|| node.css_compiled_styles.clone());
                for style in &def.styles {
                    list.push(style.clone());
                }
            }
            if let Some(list) = compiled {
                node.css_compiled_styles = list;
            }
        }
        if let Some(&idx) = by_id.get(&node.id) {
            let parent_id = node.parent_id.clone().or_else(|| nodes[idx].parent_id.clone());
            nodes[idx] = node;
            if parent_id.is_some() {
                nodes[idx].parent_id = parent_id;
            }
        } else {
            let idx = nodes.len();
            by_id.insert(node.id.clone(), idx);
            nodes.push(node);
        }
    };

    let visit = |parent: Option<&StateStmt>,
                 item: &StateStmt,
                 alt: bool,
                 drafts: &mut HashMap<String, Draft>,
                 slots: &mut HashMap<String, Option<usize>>,
                 next_slot: &mut usize,
                 nodes: &mut Vec<StateNode>,
                 by_id: &mut HashMap<String, usize>,
                 edges: &mut Vec<StateEdge>,
                 counter: &mut usize| {
        let id = item.id.clone();
        if id == "root" {
            return;
        }
        let known = states.get(&id);
        let class_str = known.map(|k| k.classes.join(" ")).unwrap_or_default();
        let styles = known.map(|k| k.styles.clone()).unwrap_or_default();

        if !drafts.contains_key(&id) {
            let mut shape: Option<String> = Some("rect".to_string());
            if item.start == Some(true) {
                shape = Some("stateStart".to_string());
            } else if item.start == Some(false) {
                shape = Some("stateEnd".to_string());
            }
            if item.kind.as_deref() != Some("default") {
                shape = item.kind.clone();
            }
            drafts.insert(
                id.clone(),
                Draft {
                    shape,
                    description: Description::Single(id.clone()),
                    css_classes: format!("{} statediagram-state", class_str),
                    css_styles: styles.clone(),
                    kind: None,
                    dir: None,
                    color_index: None,
                },
            );
        }

        let draft = drafts.get_mut(&id).unwrap();
        if let Some(ref item_desc) = item.description {
            if item_desc.is_truthy() {
                let current = std::mem::replace(&mut draft.description, Description::Single(String::new()));
                match current {
                    Description::List(mut list) => {
                        draft.shape = Some("rectWithTitle".to_string());
                        list.extend(item_desc.clone().into_vec());
                        draft.description = Description::List(list);
                    }
                    Description::Single(s) if !s.is_empty() => {
                        draft.shape = Some("rectWithTitle".to_string());
                        if s == id {
                            draft.description = Description::List(item_desc.clone().into_vec());
                        } else {
                            let mut list = vec![s];
                            list.extend(item_desc.clone().into_vec());
                            draft.description = Description::List(list);
                        }
                    }
                    Description::Single(_) => {
                        draft.shape = Some("rect".to_string());
                        draft.description = item_desc.clone();
                    }
                }
            }
        }
        if draft.description.js_len() == 1 && draft.shape.as_deref() == Some("rectWithTitle") {
            draft.shape = Some(
                if draft.kind.as_deref() == Some("group") {
                    "roundedWithTitle"
                } else {
                    "rect"
                }
                .to_string(),
            );
        }

        if draft.kind.is_none() {
            if let Some(ref doc) = item.doc {
                draft.kind = Some("group".to_string());
                draft.dir = Some(direction(doc));
                draft.shape = Some(
                    if item.kind.as_deref() == Some("divider") {
                        "divider"
                    } else {
                        "roundedWithTitle"
                    }
                    .to_string(),
                );
                // A container takes the next palette slot unless it is a region, which shares its composite's.
                if draft.shape.as_deref() == Some("divider")
                    && parent.is_some_and(|p| slots.contains_key(&p.id))
                {
                    draft.color_index = parent.and_then(|p| slots.get(&p.id)).copied().flatten();
                } else {
                    let slot = *next_slot;
                    *next_slot += 1;
                    draft.color_index = if !js_trim(&class_str).is_empty() || !styles.is_empty() {
                        None
                    } else {
                        Some(slot)
                    };
                }
                slots.insert(id.clone(), draft.color_index);
                draft.css_classes = format!(
                    "{} statediagram-cluster {}",
                    draft.css_classes,
                    if alt { "statediagram-cluster-alt" } else { "" }
                );
            }
        }

        let (mut label, description_lines) = match &draft.description {
            Description::List(list) => (
                list.first().cloned(),
                if list.len() > 1 { list[1..].to_vec() } else { Vec::new() },
            ),
            Description::Single(s) => (Some(s.clone()), Vec::new()),
        };
        if draft.shape.as_deref() == Some("divider") {
            label = Some(String::new());
        }
        let parent_id = parent.filter(|p| p.id != "root").map(|p| p.id.clone());
        let is_group = draft.kind.as_deref() == Some("group");
        let draft_css_classes = draft.css_classes.clone();

        let node = StateNode {
            id: id.clone(),
            shape: draft.shape.clone(),
            label,
            description: description_lines,
            css_classes: draft.css_classes.clone(),
            css_compiled_styles: Vec::new(),
            css_styles: draft.css_styles.clone(),
            kind: draft.kind.clone(),
            is_group,
            parent_id,
            dir: draft.dir.clone(),
            position: None,
            note_for: None,
            color_index: draft.color_index,
        };

        let Some(ref note) = item.note else {
            insert(node, nodes, by_id);
            return;
        };

        // Mermaid puts each note in a group of its own, named after the state. Pele keeps that
        // group in the model and lays the note out beside its state instead.
        let aside = |aside_id: String, shape: &str, css_classes: String, group_flag: bool| -> StateNode {
            StateNode {
                id: aside_id,
                shape: Some(shape.to_string()),
                label: Some(note.text.clone()),
                description: Vec::new(),
                css_classes,
                css_compiled_styles: Vec::new(),
                css_styles: Vec::new(),
                kind: Some(if group_flag { "group" } else { "node" }.to_string()),
                is_group: group_flag,
                parent_id: None,
                dir: None,
                position: note.position.clone(),
                note_for: None,
                color_index: None,
            }
        };

        let note_id = format!("{}----note-{}", id, *counter);
        *counter += 1;
        let note_parent_id = format!("{}----parent", id);
        let mut note_node = aside(note_id.clone(), "note", "statediagram-note".to_string(), false);
        note_node.parent_id = Some(note_parent_id.clone());
        note_node.note_for = Some(id.clone());
        insert(aside(note_parent_id, "noteGroup", draft_css_classes, true), nodes, by_id);
        insert(note_node, nodes, by_id);
        insert(node, nodes, by_id);

        let left = note.position.as_deref() == Some("left of");
        let (from, to) = if left { (note_id, id) } else { (id, note_id) };
        edges.push(StateEdge {
            id: format!("{}-{}", from, to),
            start: from,
            end: to,
            label: String::new(),
            classes: "transition note-edge".to_string(),
            pattern: Some("dashed"),
        });
    };

    let mut stack: Vec<Frame<'_>> = vec![Frame { item: None, doc: root_doc, at: 0, alt: true }];
    while let Some(frame) = stack.last_mut() {
        if frame.at >= frame.doc.len() {
            stack.pop();
            continue;
        }
        let stmt = &frame.doc[frame.at];
        frame.at += 1;
        let parent_item = frame.item;
        let alt = frame.alt;
        match stmt {
            Stmt::State(st) => {
                visit(
                    parent_item,
                    st,
                    alt,
                    &mut drafts,
                    &mut slots,
                    &mut next_slot,
                    &mut nodes,
                    &mut by_id,
                    &mut edges,
                    &mut counter,
                );
                if let Some(ref child_doc) = st.doc {
                    stack.push(Frame { item: Some(st), doc: child_doc, at: 0, alt: !alt });
                }
            }
            Stmt::Relation(rel) => {
                visit(
                    parent_item,
                    &rel.state1,
                    alt,
                    &mut drafts,
                    &mut slots,
                    &mut next_slot,
                    &mut nodes,
                    &mut by_id,
                    &mut edges,
                    &mut counter,
                );
                visit(
                    parent_item,
                    &rel.state2,
                    alt,
                    &mut drafts,
                    &mut slots,
                    &mut next_slot,
                    &mut nodes,
                    &mut by_id,
                    &mut edges,
                    &mut counter,
                );
                let edge_id = format!("edge{}", counter);
                counter += 1;
                edges.push(StateEdge {
                    id: edge_id,
                    start: rel.state1.id.clone(),
                    end: rel.state2.id.clone(),
                    label: rel.description.clone().unwrap_or_default(),
                    classes: "transition".to_string(),
                    pattern: None,
                });
            }
            _ => {}
        }
    }

    for node in &nodes {
        if node.is_group && !node.description.is_empty() {
            return Err(PeleError::new(
                format!(
                    "Group nodes can only have label. Remove the additional description for node [{}]",
                    node.id
                ),
                ErrorCode::Semantic,
            )
            .of("state"));
        }
    }

    Ok(StateGraph { nodes, edges })
}
