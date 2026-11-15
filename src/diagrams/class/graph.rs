use super::db::{relation_type, ClassDb};
use super::types::{ClassNode, RelationEnd};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    Rect,
    ClassBox,
    Note,
    Interface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndMarker {
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgePattern {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub is_group: bool,
    pub shape: NodeShape,
    pub parent_id: Option<String>,
    // Index of the namespace node this one is drawn in, or -1.
    pub parent: i32,
    pub class_node: Option<ClassNode>,
    // Position among the classes, in declaration order. Namespaces and notes have none.
    pub color_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub id: String,
    pub start: String,
    pub end: String,
    // Indexes into the node list. Ids alone are ambiguous: a class may be named `note0` or `interface0`.
    pub source: i32,
    pub target: i32,
    pub label: Option<String>,
    pub arrow_type_start: EndMarker,
    pub arrow_type_end: EndMarker,
    pub start_label_right: String,
    pub end_label_left: String,
    pub pattern: EdgePattern,
    pub classes: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct ClassGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

const MARKERS: [EndMarker; 5] = [
    EndMarker::Aggregation,
    EndMarker::Extension,
    EndMarker::Composition,
    EndMarker::Dependency,
    EndMarker::Lollipop,
];

fn marker(end: RelationEnd) -> EndMarker {
    match end {
        RelationEnd::Marker(code) => MARKERS.get(code as usize).copied().unwrap_or(EndMarker::None),
        RelationEnd::None => EndMarker::None,
    }
}

// Resolves a parsed class diagram into the nodes and edges that are drawn. With `hierarchical`
// off, only namespaces the source declares are kept and each is labeled with its full name.
pub fn build_class_graph(model: &ClassDb, hierarchical: bool) -> ClassGraph {
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut namespace_at: HashMap<String, usize> = HashMap::new();
    let mut class_at: HashMap<String, usize> = HashMap::new();
    let mut interface_at: HashMap<String, usize> = HashMap::new();
    let mut declared: HashMap<String, Option<String>> = HashMap::new();

    let mut declared_ancestor = |id: Option<&str>| -> Option<String> {
        let id = id?;
        if hierarchical {
            return Some(id.to_string());
        }
        let mut chain: Vec<String> = Vec::new();
        let mut current: Option<String> = Some(id.to_string());
        let mut found: Option<String> = None;
        while let Some(cur) = current.filter(|s| !s.is_empty()) {
            if let Some(cached) = declared.get(&cur) {
                found = cached.clone();
                break;
            }
            let Some(ns) = model.namespaces.get(&cur) else {
                break;
            };
            if ns.explicit {
                found = Some(cur);
                break;
            }
            let next = ns.parent.clone();
            chain.push(cur);
            current = next;
        }
        for visited in chain {
            declared.insert(visited, found.clone());
        }
        found
    };

    for ns in model.namespaces.values() {
        if !hierarchical && !ns.explicit {
            continue;
        }
        namespace_at.insert(ns.id.clone(), nodes.len());
        nodes.push(GraphNode {
            id: ns.id.clone(),
            label: if hierarchical { ns.label.clone() } else { ns.id.clone() },
            is_group: true,
            shape: NodeShape::Rect,
            parent_id: if hierarchical { ns.parent.clone() } else { None },
            parent: -1,
            class_node: None,
            color_index: None,
        });
    }

    let group_of = |namespace_at: &HashMap<String, usize>, id: Option<&str>| -> i32 {
        id.and_then(|s| namespace_at.get(s)).map_or(-1, |&idx| idx as i32)
    };

    for node in &mut nodes {
        node.parent = group_of(&namespace_at, node.parent_id.as_deref());
    }

    let mut color_index = 0usize;
    for class_node in model.classes.values() {
        let parent_id = declared_ancestor(class_node.parent.as_deref());
        let parent = group_of(&namespace_at, parent_id.as_deref());
        class_at.insert(class_node.id.clone(), nodes.len());
        nodes.push(GraphNode {
            id: class_node.id.clone(),
            label: class_node.label.clone(),
            is_group: false,
            shape: NodeShape::ClassBox,
            parent_id,
            parent,
            class_node: Some(class_node.clone()),
            color_index: Some(color_index),
        });
        color_index += 1;
    }

    for note in model.notes.values() {
        let parent_id = declared_ancestor(note.parent.as_deref());
        let parent = group_of(&namespace_at, parent_id.as_deref());
        let target = note.class_name.as_deref().and_then(|cn| class_at.get(cn).copied());
        if let Some(target_idx) = target {
            edges.push(GraphEdge {
                id: format!("edgeNote{}", note.index),
                start: note.id.clone(),
                end: nodes[target_idx].id.clone(),
                source: nodes.len() as i32,
                target: target_idx as i32,
                label: None,
                arrow_type_start: EndMarker::None,
                arrow_type_end: EndMarker::None,
                start_label_right: String::new(),
                end_label_left: String::new(),
                pattern: EdgePattern::Dotted,
                classes: "relation",
            });
        }
        nodes.push(GraphNode {
            id: note.id.clone(),
            label: note.text.clone(),
            is_group: false,
            shape: NodeShape::Note,
            parent_id,
            parent,
            class_node: None,
            color_index: None,
        });
    }

    // Mermaid leaves every lollipop interface outside the namespaces. Here it stays beside its class.
    for item in &model.interfaces {
        let owner = class_at.get(&item.class_id).copied();
        interface_at.insert(item.id.clone(), nodes.len());
        let parent = owner.map_or(-1, |idx| nodes[idx].parent);
        nodes.push(GraphNode {
            id: item.id.clone(),
            label: item.label.clone(),
            is_group: false,
            shape: NodeShape::Interface,
            parent_id: None,
            parent,
            class_node: None,
            color_index: None,
        });
    }

    let mut count = 0usize;
    for relation in &model.relations {
        count += 1;
        let type1 = relation.relation.type1;
        let type2 = relation.relation.type2;
        // add_relation swaps in an interface for the lollipop end of a relation with no other marker.
        let start_is_iface =
            type1 == RelationEnd::Marker(relation_type::LOLLIPOP) && type2 == RelationEnd::None;
        let end_is_iface = !start_is_iface
            && type2 == RelationEnd::Marker(relation_type::LOLLIPOP)
            && type1 == RelationEnd::None;
        let start_at = if start_is_iface { &interface_at } else { &class_at };
        let end_at = if end_is_iface { &interface_at } else { &class_at };
        edges.push(GraphEdge {
            id: format!("id_{}_{}_{}", relation.id1, relation.id2, count),
            start: relation.id1.clone(),
            end: relation.id2.clone(),
            source: start_at.get(&relation.id1).map_or(-1, |&idx| idx as i32),
            target: end_at.get(&relation.id2).map_or(-1, |&idx| idx as i32),
            label: relation.title.clone(),
            arrow_type_start: marker(type1),
            arrow_type_end: marker(type2),
            start_label_right: if relation.relation_title1 == "none" {
                String::new()
            } else {
                relation.relation_title1.clone()
            },
            end_label_left: if relation.relation_title2 == "none" {
                String::new()
            } else {
                relation.relation_title2.clone()
            },
            pattern: if relation.relation.line_type == 1 {
                EdgePattern::Dashed
            } else {
                EdgePattern::Solid
            },
            classes: "relation",
        });
    }

    ClassGraph { nodes, edges }
}
