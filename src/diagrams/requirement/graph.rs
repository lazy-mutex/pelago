use super::db::{relationships, RequirementDb};
use super::types::{Requirement, RequirementElement};

pub struct GraphNode<'a> {
    pub id: &'a str,
    pub requirement: Option<&'a Requirement>,
    pub element: Option<&'a RequirementElement>,
    pub css_styles: &'a [String],
    pub css_classes: String,
}

pub struct GraphEdge {
    pub id: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub rel_type: String,
    pub contains: bool,
}

pub struct RequirementGraph<'a> {
    pub nodes: Vec<GraphNode<'a>>,
    pub edges: Vec<GraphEdge>,
    pub direction: &'a str,
}

// The nodes and edges to lay out: requirements first, then elements, as Mermaid orders them.
pub fn build_requirement_graph(model: &RequirementDb) -> RequirementGraph<'_> {
    let mut nodes = Vec::with_capacity(model.requirements.len() + model.elements.len());
    for requirement in &model.requirements {
        nodes.push(GraphNode {
            id: &requirement.name,
            requirement: Some(requirement),
            element: None,
            css_styles: &requirement.css_styles,
            css_classes: requirement.classes.join(" "),
        });
    }
    for element in &model.elements {
        nodes.push(GraphNode {
            id: &element.name,
            requirement: None,
            element: Some(element),
            css_styles: &element.css_styles,
            css_classes: element.classes.join(" "),
        });
    }
    let edges = model
        .relations
        .iter()
        .enumerate()
        .map(|(index, relation)| GraphEdge {
            id: format!("{}-{}-{}", relation.src, relation.dst, index),
            start: model.has_node(&relation.src).then(|| relation.src.clone()),
            end: model.has_node(&relation.dst).then(|| relation.dst.clone()),
            rel_type: relation.rel_type.clone(),
            contains: relation.rel_type == relationships::CONTAINS,
        })
        .collect();
    RequirementGraph { nodes, edges, direction: &model.direction }
}
