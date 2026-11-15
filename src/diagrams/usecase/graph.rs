use super::types::{
    arrow, ActorType, Animation, BoundaryType, LabelType, OrdMap, Relationship, RelationshipType,
    UseCaseShape, UsecaseJsonNode, UsecaseModel,
};
use crate::svg::builder::js_number;
use crate::value::Value;

#[derive(Debug, Clone)]
pub struct JsonRow {
    // Blank on the second and later values of an array.
    pub key: String,
    // The key again, for every row.
    pub accessible_key: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    UsecaseActor,
    UsecaseActorHollow,
    UsecaseActorAwesome,
    UsecaseActorIcon,
    UsecaseEllipse,
    UsecaseBusiness,
    Rect,
    Note,
    UsecaseJsonTable,
    UsecaseSystemBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeRelationType {
    Association,
    Include,
    Extend,
    Generalization,
    Note,
}

impl EdgeRelationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Association => "association",
            Self::Include => "include",
            Self::Extend => "extend",
            Self::Generalization => "generalization",
            Self::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgePattern {
    Solid,
    Dotted,
}

#[derive(Debug, Clone)]
pub struct UsecaseGraphNode {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub shape: NodeShape,
    pub is_group: bool,
    pub padding: f64,
    pub css_classes: String,
    pub css_styles: Vec<String>,
    pub css_compiled_styles: Vec<String>,
    pub actor_type: Option<ActorType>,
    pub business: Option<bool>,
    pub icon: Option<String>,
    pub stereotype: Option<String>,
    pub parent_id: Option<String>,
    pub note_target: Option<String>,
    pub json_rows: Option<Vec<JsonRow>>,
    pub boundary_type: Option<BoundaryType>,
}

#[derive(Debug, Clone)]
pub struct UsecaseGraphEdge {
    pub id: String,
    pub start: String,
    pub end: String,
    pub relationship_type: EdgeRelationType,
    pub pattern: EdgePattern,
    pub arrow_type_start: &'static str,
    pub arrow_type_end: &'static str,
    pub label: Option<String>,
    pub label_type: Option<LabelType>,
    pub classes: String,
    pub style: Vec<String>,
    pub css_compiled_styles: Vec<String>,
    pub animate: bool,
    pub animation: Option<Animation>,
    pub minlen: usize,
    pub explicit_id: bool,
    // The dotted line from a note to what it is about.
    pub internal: bool,
}

#[derive(Debug, Clone)]
pub struct UsecaseGraph {
    pub nodes: Vec<UsecaseGraphNode>,
    pub edges: Vec<UsecaseGraphEdge>,
}

// Start and end markers for each arrow type, in the order of the Arrow enum.
const MARKERS: [(&str, &str); 7] = [
    ("none", "arrow_point"),
    ("arrow_point", "none"),
    ("none", "none"),
    ("none", "arrow_circle"),
    ("none", "arrow_cross"),
    ("arrow_circle", "none"),
    ("arrow_cross", "none"),
];

fn actor_shape(actor_type: ActorType) -> NodeShape {
    match actor_type {
        ActorType::Normal => NodeShape::UsecaseActor,
        ActorType::Hollow => NodeShape::UsecaseActorHollow,
        ActorType::Awesome => NodeShape::UsecaseActorAwesome,
        ActorType::Icon => NodeShape::UsecaseActorIcon,
    }
}

fn scalar(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Str(s)) => s.clone(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Num(n)) => {
            if n.is_infinite() {
                if *n > 0.0 {
                    "Infinity".to_string()
                } else {
                    "-Infinity".to_string()
                }
            } else if n.is_nan() {
                "NaN".to_string()
            } else {
                js_number(*n)
            }
        }
        Some(other) => other.to_text(),
    }
}

fn escape_pointer(key: &str) -> String {
    if !key.contains('~') && !key.contains('/') {
        return key.to_string();
    }
    key.replace('~', "~0").replace('/', "~1")
}

// One row for each leaf of a JSON object, depth first in the order the keys were written.
// Nesting is kept on a stack of its own, so depth is limited by memory and not by the call stack.
pub fn flatten_json_rows(json: &UsecaseJsonNode) -> Vec<JsonRow> {
    let mut rows = Vec::new();
    let mut stack: Vec<(Option<&Value>, String, String)> =
        vec![(Some(&json.value), String::new(), String::new())];
    let mut children: Vec<(Option<&Value>, String, String)> = Vec::new();
    while let Some((current, path, pointer)) = stack.pop() {
        children.clear();
        match current {
            Some(Value::Arr(items)) => {
                if items.is_empty() {
                    rows.push(JsonRow {
                        key: path.clone(),
                        accessible_key: path,
                        value: "[]".to_string(),
                    });
                } else if items
                    .iter()
                    .all(|item| !matches!(item, Value::Arr(_) | Value::Obj(_)))
                {
                    for (i, item) in items.iter().enumerate() {
                        rows.push(JsonRow {
                            key: if i == 0 { path.clone() } else { String::new() },
                            accessible_key: path.clone(),
                            value: scalar(Some(item)),
                        });
                    }
                } else {
                    for (i, item) in items.iter().enumerate() {
                        children.push((
                            Some(item),
                            format!("{}[{}]", path, i),
                            format!("{}/{}", pointer, i),
                        ));
                    }
                }
            }
            Some(Value::Obj(entries)) => {
                let fallback_keys: Vec<String>;
                let keys: &[String] = match json.property_order.get(&pointer) {
                    Some(ordered) => ordered.as_slice(),
                    None => {
                        fallback_keys = entries.iter().map(|(k, _)| k.clone()).collect();
                        fallback_keys.as_slice()
                    }
                };
                if keys.is_empty() {
                    rows.push(JsonRow {
                        key: path.clone(),
                        accessible_key: path,
                        value: "{}".to_string(),
                    });
                } else {
                    for key in keys {
                        let val = entries.iter().find(|(k, _)| k == key).map(|(_, v)| v);
                        let child_path = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{}.{}", path, key)
                        };
                        let child_ptr = format!("{}/{}", pointer, escape_pointer(key));
                        children.push((val, child_path, child_ptr));
                    }
                }
            }
            other => {
                rows.push(JsonRow {
                    key: path.clone(),
                    accessible_key: path,
                    value: scalar(other),
                });
            }
        }
        while let Some(child) = children.pop() {
            stack.push(child);
        }
    }
    rows
}

fn join_classes<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut out = String::new();
    for p in parts {
        if p.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(p);
    }
    out
}

fn markers(relationship: &Relationship) -> (&'static str, &'static str) {
    if relationship.rel_type == RelationshipType::Generalization {
        return ("none", "extension");
    }
    if relationship.rel_type != RelationshipType::Association {
        return MARKERS[arrow::SOLID_ARROW];
    }
    MARKERS
        .get(relationship.arrow_type)
        .copied()
        .unwrap_or(MARKERS[arrow::LINE_SOLID])
}

// Resolves a parsed use case diagram into what is drawn: every actor, use case, note, JSON table
// and boundary as a node with its class styles, and every relationship and note connector as an
// edge with its markers. Ports getData from Mermaid's use case DB.
pub fn build_usecase_graph(model: &UsecaseModel) -> UsecaseGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // A later class wins a property but keeps the place the property first took.
    let compiled = |class_names: &[String]| -> Vec<String> {
        let mut out: OrdMap<String, String> = OrdMap::new();
        for name in std::iter::once("default").chain(class_names.iter().map(String::as_str)) {
            let Some(definition) = model.class_defs.get(name) else {
                continue;
            };
            for raw in &definition.styles {
                let style = raw.trim();
                let property = match style.find(':') {
                    Some(colon) => style[..colon].trim(),
                    None => style,
                };
                if !property.is_empty() {
                    out.insert(property.to_string(), style.to_string());
                }
            }
        }
        out.into_values().collect()
    };

    for actor in model.actors.values() {
        let type_cls = format!("usecase-actor-{}", actor.actor_type.as_str());
        let mut cls_parts = vec!["default", "usecase-actor", type_cls.as_str()];
        if actor.business {
            cls_parts.push("usecase-business");
        }
        for c in &actor.classes {
            cls_parts.push(c.as_str());
        }
        nodes.push(UsecaseGraphNode {
            id: actor.id.clone(),
            label: actor.label.clone(),
            label_type: actor.label_type,
            shape: actor_shape(actor.actor_type),
            is_group: false,
            padding: 10.0,
            css_classes: join_classes(cls_parts),
            css_styles: actor.styles.clone(),
            css_compiled_styles: compiled(&actor.classes),
            actor_type: Some(actor.actor_type),
            business: Some(actor.business),
            icon: actor.icon.as_ref().filter(|s| !s.is_empty()).cloned(),
            stereotype: actor.stereotype.as_ref().filter(|s| !s.is_empty()).cloned(),
            parent_id: actor.parent_id.as_ref().filter(|s| !s.is_empty()).cloned(),
            note_target: None,
            json_rows: None,
            boundary_type: None,
        });
    }

    for use_case in model.use_cases.values() {
        let ellipse = use_case.shape == UseCaseShape::Ellipse;
        let shape_cls = format!("usecase-{}", use_case.shape.as_str());
        let mut cls_parts = vec!["default", "usecase-element", shape_cls.as_str()];
        if use_case.business {
            cls_parts.push("usecase-business");
        }
        for c in &use_case.classes {
            cls_parts.push(c.as_str());
        }
        nodes.push(UsecaseGraphNode {
            id: use_case.id.clone(),
            label: use_case.label.clone(),
            label_type: use_case.label_type,
            shape: if !ellipse {
                NodeShape::Rect
            } else if use_case.business {
                NodeShape::UsecaseBusiness
            } else {
                NodeShape::UsecaseEllipse
            },
            is_group: false,
            padding: if ellipse { 20.0 } else { 10.0 },
            css_classes: join_classes(cls_parts),
            css_styles: use_case.styles.clone(),
            css_compiled_styles: compiled(&use_case.classes),
            actor_type: None,
            business: Some(use_case.business),
            icon: None,
            stereotype: use_case
                .stereotype
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
            parent_id: use_case
                .parent_id
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
            note_target: None,
            json_rows: None,
            boundary_type: None,
        });
    }

    for note in model.notes.values() {
        nodes.push(UsecaseGraphNode {
            id: note.id.clone(),
            label: note.label.clone(),
            label_type: note.label_type,
            shape: NodeShape::Note,
            is_group: false,
            padding: 10.0,
            css_classes: "default usecase-note".to_string(),
            css_styles: Vec::new(),
            css_compiled_styles: compiled(&[]),
            actor_type: None,
            business: None,
            icon: None,
            stereotype: None,
            parent_id: None,
            note_target: Some(note.target.clone()),
            json_rows: None,
            boundary_type: None,
        });
    }

    for json in model.json_nodes.values() {
        let mut cls_parts = vec!["default", "usecase-json-table"];
        for c in &json.classes {
            cls_parts.push(c.as_str());
        }
        nodes.push(UsecaseGraphNode {
            id: json.id.clone(),
            label: json.id.clone(),
            label_type: LabelType::Text,
            shape: NodeShape::UsecaseJsonTable,
            is_group: false,
            padding: 10.0,
            css_classes: join_classes(cls_parts),
            css_styles: json.styles.clone(),
            css_compiled_styles: compiled(&json.classes),
            actor_type: None,
            business: None,
            icon: None,
            stereotype: None,
            parent_id: None,
            note_target: None,
            json_rows: Some(flatten_json_rows(json)),
            boundary_type: None,
        });
    }

    for boundary in model.system_boundaries.values() {
        let btype_cls = format!("system-boundary-{}", boundary.boundary_type.as_str());
        let mut cls_parts = vec!["default", "system-boundary", btype_cls.as_str()];
        for c in &boundary.classes {
            cls_parts.push(c.as_str());
        }
        nodes.push(UsecaseGraphNode {
            id: boundary.id.clone(),
            label: boundary.label.clone(),
            label_type: boundary.label_type,
            shape: NodeShape::UsecaseSystemBoundary,
            is_group: true,
            padding: 20.0,
            css_classes: join_classes(cls_parts),
            css_styles: boundary.styles.clone(),
            css_compiled_styles: compiled(&boundary.classes),
            actor_type: None,
            business: None,
            icon: None,
            stereotype: None,
            parent_id: None,
            note_target: None,
            json_rows: None,
            boundary_type: Some(boundary.boundary_type),
        });
    }

    for relationship in &model.relationships {
        let semantic = matches!(
            relationship.rel_type,
            RelationshipType::Include | RelationshipType::Extend
        );
        let (arrow_type_start, arrow_type_end) = markers(relationship);
        let animated = relationship.animate || relationship.animation.is_some();
        let rel_cls = format!("relationship-{}", relationship.rel_type.as_str());
        let anim_cls = format!(
            "edge-animation-{}",
            relationship.animation.unwrap_or(Animation::Fast).as_str()
        );
        let mut cls_parts = vec!["default", "relationship", rel_cls.as_str()];
        for c in &relationship.classes {
            cls_parts.push(c.as_str());
        }
        if animated {
            cls_parts.push(anim_cls.as_str());
        }
        // Include and extend are labelled with their kind, whatever label the relationship carries.
        let (label, label_type) = if semantic {
            (
                Some(relationship.rel_type.as_str().to_string()),
                Some(LabelType::Text),
            )
        } else if relationship.rel_type == RelationshipType::Association {
            (
                relationship.label.as_ref().filter(|s| !s.is_empty()).cloned(),
                relationship.label_type,
            )
        } else {
            (None, None)
        };
        let rel_type = match relationship.rel_type {
            RelationshipType::Association => EdgeRelationType::Association,
            RelationshipType::Include => EdgeRelationType::Include,
            RelationshipType::Extend => EdgeRelationType::Extend,
            RelationshipType::Generalization => EdgeRelationType::Generalization,
        };
        edges.push(UsecaseGraphEdge {
            id: relationship.id.clone(),
            start: relationship.source.clone(),
            end: relationship.target.clone(),
            relationship_type: rel_type,
            pattern: if semantic {
                EdgePattern::Dotted
            } else {
                EdgePattern::Solid
            },
            arrow_type_start,
            arrow_type_end,
            label,
            label_type,
            classes: join_classes(cls_parts),
            style: relationship.styles.clone(),
            css_compiled_styles: compiled(&relationship.classes),
            animate: relationship.animate,
            animation: relationship.animation,
            minlen: relationship.minlen,
            explicit_id: relationship.explicit_id,
            internal: false,
        });
    }

    for note in model.notes.values() {
        edges.push(UsecaseGraphEdge {
            id: format!("{}-edge", note.id),
            start: note.id.clone(),
            end: note.target.clone(),
            relationship_type: EdgeRelationType::Note,
            pattern: EdgePattern::Dotted,
            arrow_type_start: "none",
            arrow_type_end: "none",
            label: None,
            label_type: None,
            classes: "default relationship relationship-note".to_string(),
            style: Vec::new(),
            css_compiled_styles: compiled(&[]),
            animate: false,
            animation: None,
            minlen: 1,
            explicit_id: false,
            internal: true,
        });
    }

    UsecaseGraph { nodes, edges }
}
