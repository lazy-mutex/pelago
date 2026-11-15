use super::db::ErDb;
use super::types::{ErAttribute, Identification};
use crate::chars::is_space;
use std::collections::{HashMap, HashSet};

pub struct ErGraphNode<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub alias: &'a str,
    pub attributes: &'a [ErAttribute],
    pub is_group: bool,
    pub parent_id: Option<String>,
    pub css_styles: &'a [String],
    pub css_compiled_styles: Vec<String>,
    pub css_classes: String,
    pub dir: Option<&'a str>,
    pub label_type: &'static str,
}

pub struct ErGraphEdge {
    pub id: String,
    pub start: String,
    pub end: String,
    pub label: String,
    pub arrow_type_start: &'static str,
    pub arrow_type_end: &'static str,
    pub dashed: bool,
}

pub struct ErGraph<'a> {
    pub nodes: Vec<ErGraphNode<'a>>,
    pub edges: Vec<ErGraphEdge>,
}

fn compiled<'a, I: Iterator<Item = &'a str>>(
    classes: &HashMap<String, super::types::ErClassDef>,
    class_names: I,
) -> Vec<String> {
    let mut out = Vec::new();
    for name in class_names {
        let Some(def) = classes.get(name) else { continue };
        for s in &def.styles {
            out.push(s.trim_matches(is_space).to_string());
        }
        for s in &def.text_styles {
            out.push(s.trim_matches(is_space).to_string());
        }
    }
    out
}

// Resolves a parsed ER diagram into what is drawn: subgraphs as groups, entities as nodes with
// their class styles, and relationships as edges with a marker type at each end.
pub fn build_er_graph(model: &ErDb) -> ErGraph<'_> {
    let mut nodes: Vec<ErGraphNode<'_>> = Vec::new();
    let mut edges: Vec<ErGraphEdge> = Vec::with_capacity(model.relationships.len());
    let subgraphs = &model.subgraphs;

    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    let mut subgraph_ids: HashSet<&str> = HashSet::new();
    for subgraph in subgraphs.iter().rev() {
        subgraph_ids.insert(&subgraph.id);
        for id in &subgraph.nodes {
            parent_of.insert(id.as_str(), subgraph.id.as_str());
        }
    }

    for subgraph in subgraphs.iter().rev() {
        nodes.push(ErGraphNode {
            id: &subgraph.id,
            label: &subgraph.title,
            alias: "",
            attributes: &[],
            is_group: true,
            parent_id: parent_of.get(subgraph.id.as_str()).map(|&p| p.to_string()),
            css_styles: &subgraph.css_styles,
            css_compiled_styles: compiled(&model.classes, subgraph.classes.iter().map(String::as_str)),
            css_classes: subgraph.classes.join(" "),
            dir: subgraph.dir.as_deref(),
            label_type: subgraph.label_type,
        });
    }

    // A name that is also a subgraph id stands for the subgraph. Mermaid drops such an entity and
    // leaves relationships written before the subgraph pointing at nothing; here they follow it.
    let mut moved: HashMap<&str, &str> = HashMap::new();
    for (name, entity) in &model.entities {
        if subgraph_ids.contains(name.as_str()) {
            moved.insert(entity.id.as_str(), name.as_str());
            continue;
        }
        nodes.push(ErGraphNode {
            id: &entity.id,
            label: &entity.label,
            alias: &entity.alias,
            attributes: &entity.attributes,
            is_group: false,
            parent_id: parent_of.get(name.as_str()).map(|&p| p.to_string()),
            css_styles: &entity.css_styles,
            css_compiled_styles: compiled(&model.classes, entity.css_classes.split(' ')),
            css_classes: entity.css_classes.clone(),
            dir: None,
            label_type: "markdown",
        });
    }

    for (index, relationship) in model.relationships.iter().enumerate() {
        let start = moved
            .get(relationship.entity_a.as_str())
            .map(|&s| s.to_string())
            .unwrap_or_else(|| relationship.entity_a.clone());
        let end = moved
            .get(relationship.entity_b.as_str())
            .map(|&s| s.to_string())
            .unwrap_or_else(|| relationship.entity_b.clone());
        edges.push(ErGraphEdge {
            id: format!("id_{}_{}_{}", relationship.entity_a, relationship.entity_b, index),
            start,
            end,
            label: relationship.role_a.clone(),
            arrow_type_start: relationship.rel_spec.card_b.as_lower_str(),
            arrow_type_end: relationship.rel_spec.card_a.as_lower_str(),
            dashed: relationship.rel_spec.rel_type != Identification::Identifying,
        });
    }

    ErGraph { nodes, edges }
}
