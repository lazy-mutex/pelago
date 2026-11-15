use super::lexer::{line_column, location_text};
use super::types::{
    empty_model, Actor, ActorType, Animation, BoundaryType, ClassDef, Direction, EdgeOccurrence,
    GraphStatement, LabelType, OrdMap, Relationship, RelationshipType, Span,
    StatementKind, SymbolKind, SystemBoundary, UseCase, UseCaseShape, UsecaseJsonNode,
    UsecaseNote, UsecaseModel,
};
use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::value::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct DraftLabel {
    pub text: String,
    pub label_type: LabelType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftPropertyValue {
    Str(String),
    Bool(bool),
}

// key_span is also where an error about the property points.
#[derive(Debug, Clone)]
pub struct DraftProperty {
    pub key: String,
    pub value: DraftPropertyValue,
    pub span: Span,
    pub key_span: Span,
    pub value_span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Actor,
    Usecase,
}

impl ElementKind {
    pub fn as_symbol(self) -> SymbolKind {
        match self {
            Self::Actor => SymbolKind::Actor,
            Self::Usecase => SymbolKind::Usecase,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DraftBoundary {
    pub id: String,
    pub label: DraftLabel,
    pub span: Span,
    pub generated: bool,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DraftElement {
    pub id: String,
    pub label: DraftLabel,
    pub span: Span,
    pub generated: bool,
    pub kind: ElementKind,
    pub parent: Option<DraftBoundary>,
    pub shape: Option<UseCaseShape>,
    pub metadata: Option<Vec<DraftProperty>>,
    pub stereotype: Option<String>,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DraftJson {
    pub id: String,
    pub value: Value,
    pub property_order: OrdMap<String, Vec<String>>,
    pub span: Span,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DraftEndpoint {
    pub id: String,
    pub label: DraftLabel,
    pub span: Span,
    pub generated: bool,
    // Whether this mention declares the element, and whether it carries a `:::` suffix.
    pub declares: bool,
    pub classed: bool,
}

#[derive(Debug, Clone)]
pub struct DraftRelationship {
    pub source: DraftEndpoint,
    pub target: DraftEndpoint,
    pub span: Span,
    pub id: Option<String>,
    pub id_span: Option<Span>,
    pub rel_type: RelationshipType,
    pub arrow_type: usize,
    pub label: Option<DraftLabel>,
    pub minlen: usize,
}

#[derive(Debug, Clone)]
pub struct DraftNote {
    pub target: String,
    pub span: Span,
    pub label: DraftLabel,
}

#[derive(Debug, Clone)]
pub struct DraftMetadata {
    pub target: String,
    pub span: Span,
    pub properties: Vec<DraftProperty>,
    pub stmt_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct DraftClassTarget {
    pub id: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct DraftClassAssign {
    pub targets: Vec<DraftClassTarget>,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DraftStyleAssign {
    pub target: String,
    pub span: Span,
    pub styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DraftClassDef {
    pub ids: Vec<String>,
    pub styles: Vec<String>,
}

// Everything the parser read, in source order, before any of it is resolved.
#[derive(Debug, Clone)]
pub struct Drafts {
    pub elements: Vec<DraftElement>,
    pub boundaries: Vec<DraftBoundary>,
    pub jsons: Vec<DraftJson>,
    pub relationships: Vec<DraftRelationship>,
    pub notes: Vec<DraftNote>,
    pub metadata: Vec<DraftMetadata>,
    pub classes: Vec<DraftClassAssign>,
    pub styles: Vec<DraftStyleAssign>,
    pub class_defs: Vec<DraftClassDef>,
    pub direction: Option<Direction>,
    pub acc_title: String,
    pub acc_description: String,
}

pub fn empty_drafts() -> Drafts {
    Drafts {
        elements: Vec::new(),
        boundaries: Vec::new(),
        jsons: Vec::new(),
        relationships: Vec::new(),
        notes: Vec::new(),
        metadata: Vec::new(),
        classes: Vec::new(),
        styles: Vec::new(),
        class_defs: Vec::new(),
        direction: None,
        acc_title: String::new(),
        acc_description: String::new(),
    }
}

#[derive(Clone)]
struct Origin {
    kind: SymbolKind,
    span: Span,
    #[allow(dead_code)]
    generated: bool,
    // The label a generated id was derived from.
    label: Option<String>,
}

#[derive(Clone)]
struct ElementState {
    kind: ElementKind,
    id: String,
    label: DraftLabel,
    span: Span,
    #[allow(dead_code)]
    generated: bool,
    parent: Option<DraftBoundary>,
    shape: Option<UseCaseShape>,
    stereotype: Option<String>,
    stereotype_span: Option<Span>,
    classes: Vec<String>,
    actor_type: Option<ActorType>,
    icon: Option<String>,
    business: Option<bool>,
}

#[derive(Clone)]
struct BoundaryState {
    id: String,
    label: DraftLabel,
    span: Span,
    classes: Vec<String>,
    boundary_type: Option<BoundaryType>,
    members: Vec<String>,
}

fn push_unique(target: &mut Vec<String>, values: &[String]) {
    if values.is_empty() {
        return;
    }
    let mut seen: HashSet<String> = target.iter().cloned().collect();
    for value in values {
        if seen.insert(value.clone()) {
            target.push(value.clone());
        }
    }
}

// Which kind of element the keys of a metadata block could only belong to, for the error message.
fn infer_kind(properties: &[DraftProperty]) -> Option<&'static str> {
    const NAMES: [&str; 4] = ["actor", "usecase", "boundary", "edge"];
    let mut possible = [true, true, true, true];
    for prop in properties {
        match prop.key.as_str() {
            "icon" => {
                possible = [true, false, false, false];
            }
            "animate" | "animation" => {
                possible = [false, false, false, true];
            }
            "type" => {
                possible = [false, false, false, false];
                if let DraftPropertyValue::Str(ref v) = prop.value {
                    if v == "rect" || v == "package" {
                        possible[2] = true;
                    } else if v == "normal" || v == "hollow" || v == "awesome" {
                        possible[0] = true;
                    }
                }
            }
            "business" => {
                possible[2] = false;
                possible[3] = false;
            }
            _ => return None,
        }
    }
    let count = possible.iter().filter(|&&b| b).count();
    if count == 1 {
        possible.iter().position(|&b| b).map(|idx| NAMES[idx])
    } else {
        None
    }
}

fn collapse_acc_descr(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' && i + 1 < chars.len() && is_space(chars[i + 1]) {
            out.push('\n');
            i += 2;
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

enum DeclAction {
    Element(usize),
    Boundary(usize),
    Json(usize),
    Edge(String, Span),
}

// Resolves what the parser read into a model, once the whole diagram is read: which ids are
// actors, use cases, boundaries, JSON nodes or edges, in what order they appear, and whether the
// statements about them agree. Ports Mermaid's UsecaseModelBuilder, with its messages.
pub fn resolve_usecase(
    source_str: &str,
    src: &[char],
    drafts: Drafts,
    header_span: Span,
    mut statements: Vec<GraphStatement>,
    title: Option<String>,
    line_offset: usize,
) -> Result<UsecaseModel> {
    let mut model = empty_model();
    let mut symbols: OrdMap<String, Origin> = OrdMap::new();
    let mut elements: OrdMap<String, ElementState> = OrdMap::new();
    let mut boundaries: OrdMap<String, BoundaryState> = OrdMap::new();
    let mut edge_indices: HashMap<String, usize> = HashMap::new();
    // The offset at which each id is first mentioned, which decides the order of the model.
    let mut first: HashMap<String, usize> = HashMap::new();

    let at = |span: Span| -> String { location_text(src, span.0, span.1, line_offset) };

    let make_error = |message: String, span: Span| -> PeleError {
        let (line, column) = line_column(src, span.0, line_offset);
        let mut err = PeleError::new(message, ErrorCode::Semantic).of("usecase");
        err.line = line;
        err.column = column;
        err
    };

    let fail = |message: &str, span: Span| -> PeleError {
        make_error(format!("{} at {}", message, at(span)), span)
    };

    let conflict = |message: &str,
                    current: Span,
                    previous: Span,
                    current_label: Option<&str>,
                    previous_label: Option<&str>|
     -> PeleError {
        let suffix = |lbl: Option<&str>| match lbl {
            Some(l) => format!(" (label \"{}\")", l),
            None => String::new(),
        };
        make_error(
            format!(
                "{} at {}{}; previous declaration at {}{}",
                message,
                at(current),
                suffix(current_label),
                at(previous),
                suffix(previous_label)
            ),
            current,
        )
    };

    let invalid = |id: &str, kind: &str, property: &DraftProperty| -> PeleError {
        fail(
            &format!(
                "Metadata property '{}' is invalid for {} '{}'",
                property.key, kind, id
            ),
            property.key_span,
        )
    };

    fn mention(first: &mut HashMap<String, usize>, id: &str, offset: usize) {
        match first.get_mut(id) {
            Some(prev) => {
                if offset < *prev {
                    *prev = offset;
                }
            }
            None => {
                first.insert(id.to_string(), offset);
            }
        }
    }

    let register = |symbols: &mut OrdMap<String, Origin>,
                    id: &str,
                    label_text: &str,
                    span: Span,
                    generated: bool,
                    kind: SymbolKind| {
        symbols.insert(
            id.to_string(),
            Origin {
                kind,
                span,
                generated,
                label: Some(label_text.to_string()),
            },
        );
    };

    // An id means one thing in the whole diagram, and an id made from a label may be declared once.
    let claim = |symbols: &OrdMap<String, Origin>,
                 id: &str,
                 label_text: &str,
                 span: Span,
                 generated: bool,
                 kind: SymbolKind|
     -> Result<()> {
        let Some(origin) = symbols.get(id) else {
            return Ok(());
        };
        let label = if generated { Some(label_text) } else { None };
        let origin_label = if origin.generated {
            origin.label.as_deref()
        } else {
            None
        };
        if origin.kind != kind {
            return Err(conflict(
                &format!(
                    "ID '{}' is declared as both {} and {}",
                    id,
                    origin.kind.as_str(),
                    kind.as_str()
                ),
                span,
                origin.span,
                label,
                origin_label,
            ));
        }
        if origin.generated || generated {
            return Err(conflict(
                &format!("Generated ID '{}' collides with another declaration", id),
                span,
                origin.span,
                label,
                origin_label,
            ));
        }
        Ok(())
    };

    let unique =
        |symbols: &mut OrdMap<String, Origin>, id: &str, kind: SymbolKind, span: Span| -> Result<()> {
            if let Some(previous) = symbols.get(id) {
                return Err(conflict(
                    &format!(
                        "ID '{}' is declared more than once ({} and {})",
                        id,
                        previous.kind.as_str(),
                        kind.as_str()
                    ),
                    span,
                    previous.span,
                    None,
                    if previous.generated {
                        previous.label.as_deref()
                    } else {
                        None
                    },
                ));
            }
            symbols.insert(
                id.to_string(),
                Origin {
                    kind,
                    span,
                    generated: false,
                    label: None,
                },
            );
            Ok(())
        };

    // A metadata statement replaces what a declaration said; two declarations have to agree.
    let actor_property =
        |state: &mut ElementState, property: &DraftProperty, replace: bool| -> Result<()> {
            match (property.key.as_str(), &property.value) {
                ("type", DraftPropertyValue::Str(v))
                    if matches!(v.as_str(), "normal" | "hollow" | "awesome") =>
                {
                    let parsed = match v.as_str() {
                        "hollow" => ActorType::Hollow,
                        "awesome" => ActorType::Awesome,
                        _ => ActorType::Normal,
                    };
                    if !replace {
                        if let Some(prev) = state.actor_type {
                            if prev != parsed {
                                return Err(conflict(
                                    &format!(
                                        "Actor '{}' has conflicting {} metadata",
                                        state.id, property.key
                                    ),
                                    property.key_span,
                                    state.span,
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                    state.actor_type = Some(parsed);
                    Ok(())
                }
                ("icon", DraftPropertyValue::Str(v)) => {
                    if !replace {
                        if let Some(ref prev) = state.icon {
                            if prev != v {
                                return Err(conflict(
                                    &format!(
                                        "Actor '{}' has conflicting {} metadata",
                                        state.id, property.key
                                    ),
                                    property.key_span,
                                    state.span,
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                    state.icon = Some(v.clone());
                    Ok(())
                }
                ("business", DraftPropertyValue::Bool(b)) => {
                    if !replace {
                        if let Some(prev) = state.business {
                            if prev != *b {
                                return Err(conflict(
                                    &format!(
                                        "Actor '{}' has conflicting {} metadata",
                                        state.id, property.key
                                    ),
                                    property.key_span,
                                    state.span,
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                    state.business = Some(*b);
                    Ok(())
                }
                _ => Err(invalid(&state.id, "actor", property)),
            }
        };

    let element_metadata = |state: &mut ElementState,
                            properties: Option<&[DraftProperty]>,
                            replace: bool|
     -> Result<()> {
        for property in properties.unwrap_or(&[]) {
            if state.kind == ElementKind::Actor {
                actor_property(state, property, replace)?;
            } else if property.key == "business" {
                if let DraftPropertyValue::Bool(b) = property.value {
                    if !replace {
                        if let Some(prev) = state.business {
                            if prev != b {
                                return Err(conflict(
                                    &format!(
                                        "Use case '{}' has conflicting business metadata",
                                        state.id
                                    ),
                                    property.key_span,
                                    state.span,
                                    None,
                                    None,
                                ));
                            }
                        }
                    }
                    state.business = Some(b);
                } else {
                    return Err(invalid(&state.id, "usecase", property));
                }
            } else {
                return Err(invalid(&state.id, "usecase", property));
            }
        }
        Ok(())
    };

    let publish = |model: &mut UsecaseModel, state: &ElementState| {
        let stereotype = state.stereotype.as_ref().filter(|s| !s.is_empty()).cloned();
        let parent_id = state
            .parent
            .as_ref()
            .filter(|p| !p.id.is_empty())
            .map(|p| p.id.clone());
        if state.kind == ElementKind::Actor {
            model.use_cases.remove(&state.id);
            let existing_styles = model
                .actors
                .get(&state.id)
                .map(|a| a.styles.clone())
                .unwrap_or_default();
            let has_icon = state.icon.as_ref().is_some_and(|ic| !ic.is_empty());
            let actor = Actor {
                id: state.id.clone(),
                label: state.label.text.clone(),
                label_type: state.label.label_type,
                actor_type: if has_icon {
                    ActorType::Icon
                } else {
                    state.actor_type.unwrap_or(ActorType::Normal)
                },
                icon: if has_icon { state.icon.clone() } else { None },
                business: state.business.unwrap_or(false),
                stereotype,
                parent_id,
                classes: state.classes.clone(),
                styles: existing_styles,
            };
            model.actors.insert(state.id.clone(), actor);
        } else {
            model.actors.remove(&state.id);
            let existing_styles = model
                .use_cases
                .get(&state.id)
                .map(|u| u.styles.clone())
                .unwrap_or_default();
            let use_case = UseCase {
                id: state.id.clone(),
                label: state.label.text.clone(),
                label_type: state.label.label_type,
                shape: state.shape.unwrap_or(UseCaseShape::Ellipse),
                business: state.business.unwrap_or(false),
                stereotype,
                parent_id,
                classes: state.classes.clone(),
                styles: existing_styles,
            };
            model.use_cases.insert(state.id.clone(), use_case);
        }
    };

    for rel in &drafts.relationships {
        mention(&mut first, &rel.source.id, rel.source.span.0);
        mention(&mut first, &rel.target.id, rel.target.span.0);
    }

    let mut declarations: Vec<(usize, DeclAction)> = Vec::new();
    for (idx, draft) in drafts.elements.iter().enumerate() {
        mention(&mut first, &draft.id, draft.span.0);
        declarations.push((draft.span.0, DeclAction::Element(idx)));
    }
    for (idx, draft) in drafts.boundaries.iter().enumerate() {
        mention(&mut first, &draft.id, draft.span.0);
        declarations.push((draft.span.0, DeclAction::Boundary(idx)));
    }
    for (idx, draft) in drafts.jsons.iter().enumerate() {
        mention(&mut first, &draft.id, draft.span.0);
        declarations.push((draft.span.0, DeclAction::Json(idx)));
    }
    for rel in &drafts.relationships {
        if let (Some(ref id), Some(id_span)) = (&rel.id, rel.id_span) {
            if !id.is_empty() {
                declarations.push((id_span.0, DeclAction::Edge(id.clone(), id_span)));
            }
        }
    }
    declarations.sort_by_key(|d| d.0);

    for (_, action) in declarations {
        match action {
            DeclAction::Element(idx) => {
                let draft = &drafts.elements[idx];
                claim(
                    &symbols,
                    &draft.id,
                    &draft.label.text,
                    draft.span,
                    draft.generated,
                    draft.kind.as_symbol(),
                )?;
                let parent = draft.parent.as_ref().filter(|p| !p.id.is_empty()).cloned();
                if let Some(existing) = elements.get_mut(&draft.id) {
                    if existing.label.text != draft.label.text
                        || existing.label.label_type != draft.label.label_type
                    {
                        return Err(conflict(
                            &format!("ID '{}' has conflicting labels", draft.id),
                            draft.span,
                            existing.span,
                            None,
                            None,
                        ));
                    }
                    if let (Some(ds), Some(es)) = (draft.shape, existing.shape) {
                        if ds != es {
                            return Err(conflict(
                                &format!("Use case '{}' has conflicting shapes", draft.id),
                                draft.span,
                                existing.span,
                                None,
                                None,
                            ));
                        }
                    }
                    // A boundary whose title makes an empty id is no parent, but it still takes the place of one.
                    if let (Some(ref p), Some(ref ep)) = (
                        &parent,
                        existing.parent.as_ref().filter(|p| !p.id.is_empty()),
                    ) {
                        if p.id != ep.id {
                            return Err(conflict(
                                &format!(
                                    "Element '{}' belongs to more than one system boundary",
                                    draft.id
                                ),
                                p.span,
                                ep.span,
                                None,
                                None,
                            ));
                        }
                    }
                    if let (Some(ref dst), Some(ref est)) =
                        (draft.stereotype.as_ref().filter(|s| !s.is_empty()), existing.stereotype.as_ref().filter(|s| !s.is_empty()))
                    {
                        if dst != est {
                            return Err(conflict(
                                &format!("Element '{}' has conflicting stereotypes", draft.id),
                                draft.span,
                                existing.stereotype_span.unwrap_or(existing.span),
                                None,
                                None,
                            ));
                        }
                    }
                    if existing.shape.is_none() {
                        existing.shape = draft.shape;
                    }
                    if existing.parent.is_none() {
                        existing.parent = draft.parent.clone();
                    }
                    if existing.stereotype.as_ref().map_or(true, |s| s.is_empty()) {
                        if let Some(ref st) = draft.stereotype {
                            if !st.is_empty() {
                                existing.stereotype = Some(st.clone());
                            }
                        }
                    }
                    if existing.stereotype_span.is_none()
                        && draft.stereotype.as_ref().is_some_and(|s| !s.is_empty())
                    {
                        existing.stereotype_span = Some(draft.span);
                    }
                    push_unique(&mut existing.classes, &draft.classes);
                    element_metadata(existing, draft.metadata.as_deref(), false)?;
                } else {
                    let has_stereo = draft.stereotype.as_ref().is_some_and(|s| !s.is_empty());
                    let mut state = ElementState {
                        kind: draft.kind,
                        id: draft.id.clone(),
                        label: draft.label.clone(),
                        span: draft.span,
                        generated: draft.generated,
                        parent,
                        shape: draft.shape,
                        stereotype: draft.stereotype.as_ref().filter(|s| !s.is_empty()).cloned(),
                        stereotype_span: if has_stereo { Some(draft.span) } else { None },
                        classes: draft.classes.clone(),
                        actor_type: None,
                        icon: None,
                        business: None,
                    };
                    element_metadata(&mut state, draft.metadata.as_deref(), false)?;
                    elements.insert(draft.id.clone(), state);
                    register(
                        &mut symbols,
                        &draft.id,
                        &draft.label.text,
                        draft.span,
                        draft.generated,
                        draft.kind.as_symbol(),
                    );
                }
            }
            DeclAction::Boundary(idx) => {
                let draft = &drafts.boundaries[idx];
                claim(
                    &symbols,
                    &draft.id,
                    &draft.label.text,
                    draft.span,
                    draft.generated,
                    SymbolKind::Boundary,
                )?;
                if let Some(existing) = boundaries.get_mut(&draft.id) {
                    if existing.label.text != draft.label.text
                        || existing.label.label_type != draft.label.label_type
                    {
                        return Err(conflict(
                            &format!("Boundary '{}' has conflicting titles", draft.id),
                            draft.span,
                            existing.span,
                            None,
                            None,
                        ));
                    }
                    push_unique(&mut existing.classes, &draft.classes);
                } else {
                    boundaries.insert(
                        draft.id.clone(),
                        BoundaryState {
                            id: draft.id.clone(),
                            label: draft.label.clone(),
                            span: draft.span,
                            classes: draft.classes.clone(),
                            boundary_type: None,
                            members: Vec::new(),
                        },
                    );
                    register(
                        &mut symbols,
                        &draft.id,
                        &draft.label.text,
                        draft.span,
                        draft.generated,
                        SymbolKind::Boundary,
                    );
                }
            }
            DeclAction::Json(idx) => {
                let draft = &drafts.jsons[idx];
                unique(&mut symbols, &draft.id, SymbolKind::Json, draft.span)?;
            }
            DeclAction::Edge(id, id_span) => {
                unique(&mut symbols, &id, SymbolKind::Edge, id_span)?;
            }
        }
    }

    let mut elem_list: Vec<ElementState> = elements.values().cloned().collect();
    elem_list.sort_by_key(|s| first.get(&s.id).copied().unwrap_or(0));
    for state in &elem_list {
        publish(&mut model, state);
    }

    let mut boundary_list: Vec<&BoundaryState> = boundaries.values().collect();
    boundary_list.sort_by_key(|b| first.get(&b.id).copied().unwrap_or(0));
    for state in boundary_list {
        model.system_boundaries.insert(
            state.id.clone(),
            SystemBoundary {
                id: state.id.clone(),
                label: state.label.text.clone(),
                label_type: state.label.label_type,
                boundary_type: BoundaryType::Rect,
                members: Vec::new(),
                classes: Vec::new(),
                styles: Vec::new(),
            },
        );
    }

    let mut json_list: Vec<&DraftJson> = drafts.jsons.iter().collect();
    json_list.sort_by_key(|j| first.get(&j.id).copied().unwrap_or(0));
    for draft in json_list {
        model.json_nodes.insert(
            draft.id.clone(),
            UsecaseJsonNode {
                id: draft.id.clone(),
                value: draft.value.clone(),
                property_order: draft.property_order.clone(),
                classes: draft.classes.clone(),
                styles: Vec::new(),
            },
        );
    }

    let mut anonymous = 0usize;
    for draft in &drafts.relationships {
        let mut resolve_end = |end: &DraftEndpoint| -> Result<SymbolKind> {
            if end.classed && !end.declares {
                return Err(fail(
                    &format!(
                        "Relationship endpoint '{}' uses ::: without declaring the node",
                        end.id
                    ),
                    end.span,
                ));
            }
            if let Some(origin) = symbols.get(&end.id) {
                return Ok(origin.kind);
            }
            elements.insert(
                end.id.clone(),
                ElementState {
                    kind: ElementKind::Usecase,
                    id: end.id.clone(),
                    label: end.label.clone(),
                    span: end.span,
                    generated: end.generated,
                    parent: None,
                    shape: Some(UseCaseShape::Ellipse),
                    stereotype: None,
                    stereotype_span: None,
                    classes: Vec::new(),
                    actor_type: None,
                    icon: None,
                    business: None,
                },
            );
            register(
                &mut symbols,
                &end.id,
                &end.label.text,
                end.span,
                end.generated,
                SymbolKind::Usecase,
            );
            mention(&mut first, &end.id, end.span.0);
            Ok(SymbolKind::Usecase)
        };

        let source_kind = resolve_end(&draft.source)?;
        let target_kind = resolve_end(&draft.target)?;
        let related = |k: SymbolKind| {
            matches!(k, SymbolKind::Actor | SymbolKind::Usecase | SymbolKind::Json)
        };
        if !related(source_kind) {
            let prev_span = symbols.get(&draft.source.id).unwrap().span;
            return Err(conflict(
                &format!(
                    "Relationship source '{}' cannot be {}",
                    draft.source.id,
                    source_kind.as_str()
                ),
                draft.source.span,
                prev_span,
                None,
                None,
            ));
        }
        if !related(target_kind) {
            let prev_span = symbols.get(&draft.target.id).unwrap().span;
            return Err(conflict(
                &format!(
                    "Relationship target '{}' cannot be {}",
                    draft.target.id,
                    target_kind.as_str()
                ),
                draft.target.span,
                prev_span,
                None,
                None,
            ));
        }
        if matches!(
            draft.rel_type,
            RelationshipType::Include | RelationshipType::Extend
        ) && (source_kind != SymbolKind::Usecase || target_kind != SymbolKind::Usecase)
        {
            return Err(fail(
                &format!(
                    "{} relationship requires use-case endpoints",
                    draft.rel_type.as_str()
                ),
                draft.span,
            ));
        }
        if draft.rel_type == RelationshipType::Generalization
            && (source_kind == SymbolKind::Json || source_kind != target_kind)
        {
            return Err(fail(
                "Generalization requires actor-to-actor or use-case-to-use-case endpoints",
                draft.span,
            ));
        }
        if draft.rel_type == RelationshipType::Association
            && (source_kind == SymbolKind::Json || target_kind == SymbolKind::Json)
            && draft.arrow_type > 2
        {
            return Err(fail(
                &format!(
                    "JSON relationship '{}' to '{}' permits only point, reversed-point, or markerless solid association",
                    draft.source.id, draft.target.id
                ),
                draft.span,
            ));
        }

        let explicit_id = draft.id.as_ref().is_some_and(|s| !s.is_empty());
        let id = match draft.id.as_ref().filter(|s| !s.is_empty()) {
            Some(explicit) => explicit.clone(),
            None => {
                let gen = format!("edge-{}", anonymous);
                anonymous += 1;
                gen
            }
        };
        let relationship = Relationship {
            id: id.clone(),
            explicit_id,
            source: draft.source.id.clone(),
            target: draft.target.id.clone(),
            rel_type: draft.rel_type,
            arrow_type: draft.arrow_type,
            label: draft.label.as_ref().map(|l| l.text.clone()),
            label_type: draft.label.as_ref().map(|l| l.label_type),
            minlen: draft.minlen,
            classes: Vec::new(),
            styles: Vec::new(),
            animate: false,
            animation: None,
        };
        let rel_idx = model.relationships.len();
        model.relationships.push(relationship);
        edge_indices.insert(id, rel_idx);
    }

    // The relationships may have added use cases; put all elements back in the order of first mention.
    let declared_actors = model.actors.clone();
    let declared_use_cases = model.use_cases.clone();
    model.actors.clear();
    model.use_cases.clear();
    let mut ordered_elements: Vec<ElementState> = elements.values().cloned().collect();
    ordered_elements.sort_by_key(|s| first.get(&s.id).copied().unwrap_or(0));
    for state in &ordered_elements {
        if let Some(actor) = declared_actors.get(&state.id) {
            model.actors.insert(state.id.clone(), actor.clone());
        } else if let Some(uc) = declared_use_cases.get(&state.id) {
            model.use_cases.insert(state.id.clone(), uc.clone());
        } else {
            publish(&mut model, state);
        }
    }

    for meta in &drafts.metadata {
        let Some(origin) = symbols.get(&meta.target) else {
            let inferred = infer_kind(&meta.properties);
            let suffix = match inferred {
                Some(inf) => format!(" (metadata implies {})", inf),
                None => String::new(),
            };
            return Err(fail(
                &format!("Metadata target '{}' is unresolved{}", meta.target, suffix),
                meta.span,
            ));
        };
        let kind = origin.kind;
        if kind == SymbolKind::Actor || kind == SymbolKind::Usecase {
            element_metadata(
                elements.get_mut(&meta.target).unwrap(),
                Some(&meta.properties),
                true,
            )?;
        } else {
            if kind == SymbolKind::Edge {
                if let Some(stmt_idx) = meta.stmt_index {
                    let st = &mut statements[stmt_idx];
                    st.kind = StatementKind::EdgeMetadata;
                    st.edges = Some(vec![EdgeOccurrence {
                        id: meta.target.clone(),
                        span: st.span,
                        label_span: None,
                        id_span: Some(meta.span),
                        metadata: st.metadata.clone(),
                    }]);
                    st.nodes = None;
                }
            }
            for property in &meta.properties {
                match (kind, property.key.as_str(), &property.value) {
                    (SymbolKind::Boundary, "type", DraftPropertyValue::Str(v))
                        if v == "rect" || v == "package" =>
                    {
                        if let Some(b) = boundaries.get_mut(&meta.target) {
                            b.boundary_type = Some(if v == "package" {
                                BoundaryType::Package
                            } else {
                                BoundaryType::Rect
                            });
                        } else {
                            return Err(invalid(&meta.target, kind.as_str(), property));
                        }
                    }
                    (SymbolKind::Edge, "animate", DraftPropertyValue::Bool(b)) => {
                        if let Some(&eidx) = edge_indices.get(&meta.target) {
                            model.relationships[eidx].animate = *b;
                        } else {
                            return Err(invalid(&meta.target, kind.as_str(), property));
                        }
                    }
                    (SymbolKind::Edge, "animation", DraftPropertyValue::Str(v))
                        if v == "fast" || v == "slow" =>
                    {
                        if let Some(&eidx) = edge_indices.get(&meta.target) {
                            let rel = &mut model.relationships[eidx];
                            rel.animation = Some(if v == "slow" {
                                Animation::Slow
                            } else {
                                Animation::Fast
                            });
                            rel.animate = true;
                        } else {
                            return Err(invalid(&meta.target, kind.as_str(), property));
                        }
                    }
                    _ => return Err(invalid(&meta.target, kind.as_str(), property)),
                }
            }
        }
    }

    for rel in &mut model.relationships {
        if rel.animation.is_some() {
            rel.animate = true;
        }
    }

    for state in elements.values() {
        if state.kind == ElementKind::Actor {
            let has_icon = state.icon.as_ref().is_some_and(|ic| !ic.is_empty());
            if has_icon {
                if let Some(at_type) = state.actor_type {
                    if at_type != ActorType::Normal {
                        return Err(fail(
                            &format!(
                                "Actor '{}' cannot combine icon with type '{}'",
                                state.id,
                                at_type.as_str()
                            ),
                            state.span,
                        ));
                    }
                }
            }
            if state.business == Some(true)
                && (has_icon || state.actor_type == Some(ActorType::Awesome))
            {
                return Err(fail(
                    &format!(
                        "Business actor '{}' must use normal or hollow geometry",
                        state.id
                    ),
                    state.span,
                ));
            }
        } else if state.shape == Some(UseCaseShape::Rect) && state.business == Some(true) {
            return Err(fail(
                &format!(
                    "Rectangular use case '{}' cannot be a business use case",
                    state.id
                ),
                state.span,
            ));
        }
        publish(&mut model, state);
    }

    let mut sorted_draft_elems: Vec<&DraftElement> = drafts.elements.iter().collect();
    sorted_draft_elems.sort_by_key(|d| d.span.0);
    for draft in sorted_draft_elems {
        let Some(ref p) = draft.parent.as_ref().filter(|p| !p.id.is_empty()) else {
            continue;
        };
        let members = &mut boundaries.get_mut(&p.id).unwrap().members;
        if !members.contains(&draft.id) {
            members.push(draft.id.clone());
        }
    }
    for state in boundaries.values() {
        let boundary = model.system_boundaries.get_mut(&state.id).unwrap();
        boundary.boundary_type = state.boundary_type.unwrap_or(BoundaryType::Rect);
        boundary.members = state.members.clone();
        boundary.classes = state.classes.clone();
    }

    for (note_idx, draft) in drafts.notes.iter().enumerate() {
        let Some(origin) = symbols.get(&draft.target) else {
            return Err(fail(
                &format!("Note target '{}' is unresolved", draft.target),
                draft.span,
            ));
        };
        if origin.kind != SymbolKind::Actor && origin.kind != SymbolKind::Usecase {
            return Err(conflict(
                &format!(
                    "Note target '{}' must be an actor or use case, not {}",
                    draft.target,
                    origin.kind.as_str()
                ),
                draft.span,
                origin.span,
                None,
                None,
            ));
        }
        let id = format!("note-{}", note_idx);
        model.notes.insert(
            id.clone(),
            UsecaseNote {
                id,
                target: draft.target.clone(),
                label: draft.label.text.clone(),
                label_type: draft.label.label_type,
            },
        );
    }

    for def in &drafts.class_defs {
        for id in &def.ids {
            model.class_defs.insert(
                id.clone(),
                ClassDef {
                    id: id.clone(),
                    styles: def.styles.clone(),
                },
            );
        }
    }

    for assignment in &drafts.classes {
        for target in &assignment.targets {
            let kind = symbols.get(&target.id).map(|o| o.kind);
            let found = match kind {
                Some(SymbolKind::Actor) => model
                    .actors
                    .get_mut(&target.id)
                    .map(|a| &mut a.classes),
                Some(SymbolKind::Usecase) => model
                    .use_cases
                    .get_mut(&target.id)
                    .map(|u| &mut u.classes),
                Some(SymbolKind::Boundary) => model
                    .system_boundaries
                    .get_mut(&target.id)
                    .map(|b| &mut b.classes),
                Some(SymbolKind::Json) => model
                    .json_nodes
                    .get_mut(&target.id)
                    .map(|j| &mut j.classes),
                _ => edge_indices
                    .get(&target.id)
                    .map(|&idx| &mut model.relationships[idx].classes),
            };
            let Some(classes) = found else {
                return Err(fail(
                    &format!(
                        "Class/style target '{}' is unresolved or anonymous",
                        target.id
                    ),
                    target.span,
                ));
            };
            push_unique(classes, &assignment.classes);
        }
    }

    for assignment in &drafts.styles {
        let kind = symbols.get(&assignment.target).map(|o| o.kind);
        let found = match kind {
            Some(SymbolKind::Actor) => model
                .actors
                .get_mut(&assignment.target)
                .map(|a| &mut a.styles),
            Some(SymbolKind::Usecase) => model
                .use_cases
                .get_mut(&assignment.target)
                .map(|u| &mut u.styles),
            Some(SymbolKind::Boundary) => model
                .system_boundaries
                .get_mut(&assignment.target)
                .map(|b| &mut b.styles),
            Some(SymbolKind::Json) => model
                .json_nodes
                .get_mut(&assignment.target)
                .map(|j| &mut j.styles),
            _ => edge_indices
                .get(&assignment.target)
                .map(|&idx| &mut model.relationships[idx].styles),
        };
        let Some(styles) = found else {
            return Err(fail(
                &format!(
                    "Class/style target '{}' is unresolved or anonymous",
                    assignment.target
                ),
                assignment.span,
            ));
        };
        styles.extend(assignment.styles.iter().cloned());
    }

    if let Some(dir) = drafts.direction {
        model.direction = dir;
    }
    model.title = title;
    model.acc_title = drafts.acc_title;
    model.acc_descr = collapse_acc_descr(&drafts.acc_description);
    model.acc_description = drafts.acc_description;
    model.source = source_str.to_string();
    model.header_span = header_span;
    model.statements = statements;
    Ok(model)
}
