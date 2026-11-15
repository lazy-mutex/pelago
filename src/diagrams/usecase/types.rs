use crate::value::Value;

// Insertion-order preserving map matching JavaScript's `Map`.
#[derive(Debug, Clone, PartialEq)]
pub struct OrdMap<K, V> {
    entries: Vec<(K, V)>,
}

impl<K, V> Default for OrdMap<K, V> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

#[allow(dead_code)]
impl<K: PartialEq, V> OrdMap<K, V> {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: std::borrow::Borrow<Q>,
        Q: PartialEq + ?Sized,
    {
        self.entries
            .iter()
            .find(|(k, _)| k.borrow() == key)
            .map(|(_, v)| v)
    }

    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: std::borrow::Borrow<Q>,
        Q: PartialEq + ?Sized,
    {
        self.entries
            .iter_mut()
            .find(|(k, _)| k.borrow() == key)
            .map(|(_, v)| v)
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: PartialEq + ?Sized,
    {
        self.entries.iter().any(|(k, _)| k.borrow() == key)
    }

    pub fn insert(&mut self, key: K, value: V) {
        if let Some((_, existing)) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            *existing = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: PartialEq + ?Sized,
    {
        let idx = self.entries.iter().position(|(k, _)| k.borrow() == key)?;
        Some(self.entries.remove(idx).1)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.entries.iter()
    }

    pub fn into_values(self) -> impl Iterator<Item = V> {
        self.entries.into_iter().map(|(_, v)| v)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelType {
    Text,
    Markdown,
}

impl LabelType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Markdown => "markdown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorType {
    Normal,
    Hollow,
    Awesome,
    Icon,
}

impl ActorType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Hollow => "hollow",
            Self::Awesome => "awesome",
            Self::Icon => "icon",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseCaseShape {
    Ellipse,
    Rect,
}

impl UseCaseShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ellipse => "ellipse",
            Self::Rect => "rect",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryType {
    Rect,
    Package,
}

impl BoundaryType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::Package => "package",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationshipType {
    Association,
    Include,
    Extend,
    Generalization,
}

impl RelationshipType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Association => "association",
            Self::Include => "include",
            Self::Extend => "extend",
            Self::Generalization => "generalization",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Animation {
    Fast,
    Slow,
}

impl Animation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Slow => "slow",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TB,
    TD,
    BT,
    RL,
    LR,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TB => "TB",
            Self::TD => "TD",
            Self::BT => "BT",
            Self::RL => "RL",
            Self::LR => "LR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Actor,
    Usecase,
    Boundary,
    Json,
    Edge,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Actor => "actor",
            Self::Usecase => "usecase",
            Self::Boundary => "boundary",
            Self::Json => "json",
            Self::Edge => "edge",
        }
    }
}

// Start and end character offsets in the source, end exclusive.
pub type Span = (usize, usize);

#[derive(Debug, Clone)]
pub struct Actor {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub actor_type: ActorType,
    pub icon: Option<String>,
    pub business: bool,
    pub stereotype: Option<String>,
    pub parent_id: Option<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UseCase {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub shape: UseCaseShape,
    pub business: bool,
    pub stereotype: Option<String>,
    pub parent_id: Option<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SystemBoundary {
    pub id: String,
    pub label: String,
    pub label_type: LabelType,
    pub boundary_type: BoundaryType,
    pub members: Vec<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

pub mod arrow {
    pub const SOLID_ARROW: usize = 0;
    pub const BACK_ARROW: usize = 1;
    pub const LINE_SOLID: usize = 2;
    pub const CIRCLE_ARROW: usize = 3;
    pub const CROSS_ARROW: usize = 4;
    pub const CIRCLE_ARROW_REVERSED: usize = 5;
    pub const CROSS_ARROW_REVERSED: usize = 6;
}

#[derive(Debug, Clone)]
pub struct Relationship {
    pub id: String,
    pub explicit_id: bool,
    pub source: String,
    pub target: String,
    pub rel_type: RelationshipType,
    pub arrow_type: usize,
    pub label: Option<String>,
    pub label_type: Option<LabelType>,
    pub minlen: usize,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub animate: bool,
    pub animation: Option<Animation>,
}

#[derive(Debug, Clone)]
pub struct UsecaseNote {
    pub id: String,
    pub target: String,
    pub label: String,
    pub label_type: LabelType,
}

#[derive(Debug, Clone)]
pub struct UsecaseJsonNode {
    pub id: String,
    pub value: Value,
    // The keys of each object in the order they were written, by JSON Pointer.
    pub property_order: OrdMap<String, Vec<String>>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub id: String,
    pub styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MetadataOccurrence {
    pub key: String,
    pub span: Span,
    pub key_span: Span,
    pub value_span: Span,
}

#[derive(Debug, Clone)]
pub struct NodeOccurrence {
    pub id: String,
    pub span: Span,
    pub id_span: Span,
    pub label_span: Option<Span>,
    pub defines: Option<bool>,
    pub stereotype_span: Option<Span>,
    pub metadata: Option<Vec<MetadataOccurrence>>,
    pub class_spans: Option<Vec<Span>>,
}

#[derive(Debug, Clone)]
pub struct EdgeOccurrence {
    pub id: String,
    pub span: Span,
    pub label_span: Option<Span>,
    pub id_span: Option<Span>,
    pub metadata: Option<Vec<MetadataOccurrence>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Node,
    Edge,
    Group,
    Note,
    Json,
    Metadata,
    EdgeMetadata,
    ClassDef,
    ClassAssign,
    Style,
    Direction,
    AccTitle,
    AccDescr,
    Comment,
    Blank,
}

// One statement of the source with the positions of its parts, as in Mermaid's use case AST.
#[derive(Debug, Clone)]
pub struct GraphStatement {
    pub kind: StatementKind,
    pub span: Span,
    pub nodes: Option<Vec<NodeOccurrence>>,
    pub edges: Option<Vec<EdgeOccurrence>>,
    pub group: Option<String>,
    pub id_span: Option<Span>,
    pub title_span: Option<Span>,
    pub end_span: Option<Span>,
    pub r#ref: Option<String>,
    pub ref_span: Option<Span>,
    pub children: Option<Vec<GraphStatement>>,
    pub metadata: Option<Vec<MetadataOccurrence>>,
    pub class_spans: Option<Vec<Span>>,
}

impl GraphStatement {
    pub fn new(kind: StatementKind) -> Self {
        Self {
            kind,
            span: (0, 0),
            nodes: None,
            edges: None,
            group: None,
            id_span: None,
            title_span: None,
            end_span: None,
            r#ref: None,
            ref_span: None,
            children: None,
            metadata: None,
            class_spans: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UsecaseModel {
    pub actors: OrdMap<String, Actor>,
    pub use_cases: OrdMap<String, UseCase>,
    pub system_boundaries: OrdMap<String, SystemBoundary>,
    pub relationships: Vec<Relationship>,
    pub notes: OrdMap<String, UsecaseNote>,
    pub json_nodes: OrdMap<String, UsecaseJsonNode>,
    pub class_defs: OrdMap<String, ClassDef>,
    pub direction: Direction,
    pub title: Option<String>,
    pub acc_title: String,
    pub acc_descr: String,
    // The accessible description as written, before the indentation of its lines is dropped.
    pub acc_description: String,
    pub source: String,
    pub header_span: Span,
    pub statements: Vec<GraphStatement>,
}

pub fn empty_model() -> UsecaseModel {
    UsecaseModel {
        actors: OrdMap::new(),
        use_cases: OrdMap::new(),
        system_boundaries: OrdMap::new(),
        relationships: Vec::new(),
        notes: OrdMap::new(),
        json_nodes: OrdMap::new(),
        class_defs: OrdMap::new(),
        direction: Direction::LR,
        title: None,
        acc_title: String::new(),
        acc_descr: String::new(),
        acc_description: String::new(),
        source: String::new(),
        header_span: (0, 0),
        statements: Vec::new(),
    }
}
