use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelType {
    #[default]
    Text,
    String,
    Markdown,
}

impl LabelType {
    pub fn as_str(self) -> &'static str {
        match self {
            LabelType::Text => "text",
            LabelType::String => "string",
            LabelType::Markdown => "markdown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowText {
    pub text: String,
    pub kind: LabelType,
}

// A source span as Jison reports it: lines start at 1, columns at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Loc {
    pub first_line: usize,
    pub first_column: usize,
    pub last_line: usize,
    pub last_column: usize,
}

pub type Metadata = Vec<(String, Value)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertexKind {
    Tool,
    Action,
    Input,
    Refdoc,
    Decision,
    Connector,
    Task,
}

impl VertexKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VertexKind::Tool => "tool",
            VertexKind::Action => "action",
            VertexKind::Input => "input",
            VertexKind::Refdoc => "refdoc",
            VertexKind::Decision => "decision",
            VertexKind::Connector => "connector",
            VertexKind::Task => "task",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeSemantic {
    Sequence,
    Reference,
    Failure,
}

impl EdgeSemantic {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeSemantic::Sequence => "sequence",
            EdgeSemantic::Reference => "reference",
            EdgeSemantic::Failure => "failure",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AgentNode {
    pub id: String,
    pub text: Option<String>,
    pub label_type: LabelType,
    pub shape: Option<String>,
    pub styles: Vec<String>,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub props: Option<Vec<(String, String)>>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub metadata: Option<Metadata>,
    pub is_connector: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AgentEdge {
    pub start: String,
    pub end: String,
    pub kind: Option<String>,
    pub text: String,
    pub label_type: LabelType,
    pub stroke: Option<String>,
    pub edge_semantic: Option<EdgeSemantic>,
    pub length: Option<usize>,
    pub style: Option<Vec<String>>,
    pub classes: Vec<String>,
    pub id: String,
    pub is_user_defined_id: bool,
    pub interpolate: Option<String>,
    pub animate: Option<Value>,
    pub animation: Option<Value>,
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentClassDef {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentSubgraph {
    pub id: String,
    pub nodes: Vec<String>,
    pub title: String,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub label_type: LabelType,
    pub kind: &'static str,
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, Default)]
pub struct LinkInfo {
    pub kind: String,
    pub stroke: String,
    pub length: Option<usize>,
    pub edge_semantic: Option<EdgeSemantic>,
    pub text: Option<FlowText>,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DocItem {
    Text(String),
    List(Vec<String>),
    Dir(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ElementPosition {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub start_index: usize,
    pub end_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementType {
    Vertex,
    Edge,
    Subgraph,
    Connector,
    Attachment,
}

impl StatementType {
    pub fn as_str(self) -> &'static str {
        match self {
            StatementType::Vertex => "vertex",
            StatementType::Edge => "edge",
            StatementType::Subgraph => "subgraph",
            StatementType::Connector => "connector",
            StatementType::Attachment => "attachment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementMapping {
    pub id: String,
    pub kind: StatementType,
    pub position: ElementPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticId {
    ShapeUnsupported,
    ShapeRemoved,
    EdgeOperatorUnsupported,
    ReferenceEdgeLabelRejected,
    ConnectorRefUnresolved,
    ConnectorRefNotAConnector,
    MetadataKeyMisapplied,
    DuplicateIdNode,
    ReservedSyntheticId,
    ContainmentViolation,
    EdgeSemanticContradiction,
    FlowNoInput,
}

impl DiagnosticId {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticId::ShapeUnsupported => "SHAPE_UNSUPPORTED",
            DiagnosticId::ShapeRemoved => "SHAPE_REMOVED",
            DiagnosticId::EdgeOperatorUnsupported => "EDGE_OPERATOR_UNSUPPORTED",
            DiagnosticId::ReferenceEdgeLabelRejected => "REFERENCE_EDGE_LABEL_REJECTED",
            DiagnosticId::ConnectorRefUnresolved => "CONNECTOR_REF_UNRESOLVED",
            DiagnosticId::ConnectorRefNotAConnector => "CONNECTOR_REF_NOT_A_CONNECTOR",
            DiagnosticId::MetadataKeyMisapplied => "METADATA_KEY_MISAPPLIED",
            DiagnosticId::DuplicateIdNode => "DUPLICATE_ID_NODE",
            DiagnosticId::ReservedSyntheticId => "RESERVED_SYNTHETIC_ID",
            DiagnosticId::ContainmentViolation => "CONTAINMENT_VIOLATION",
            DiagnosticId::EdgeSemanticContradiction => "EDGE_SEMANTIC_CONTRADICTION",
            DiagnosticId::FlowNoInput => "FLOW_NO_INPUT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub id: DiagnosticId,
    pub severity: Severity,
    pub message: String,
    pub node_id: Option<String>,
    pub edge_id: Option<String>,
    pub position: Option<ElementPosition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticVertex {
    pub id: String,
    pub label: Option<String>,
    pub shape: Option<String>,
    pub vertex_kind: Option<VertexKind>,
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticEdge {
    pub start: String,
    pub end: String,
    pub id: Option<String>,
    pub label: Option<String>,
    pub kind: Option<String>,
    pub stroke: Option<String>,
    pub edge_semantic: Option<EdgeSemantic>,
    pub length: Option<usize>,
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticSubgraph {
    pub id: String,
    pub kind: Option<String>,
    pub title: Option<String>,
    pub nodes: Vec<String>,
    pub metadata: Option<Metadata>,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticConnector {
    pub id: String,
    pub title: Option<String>,
    pub metadata: Option<Metadata>,
}

// The diagram with presentation-only controls left out, for tools that read what it means.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticModel {
    pub direction: Option<String>,
    pub vertices: Vec<SemanticVertex>,
    pub edges: Vec<SemanticEdge>,
    pub sub_graphs: Vec<SemanticSubgraph>,
    pub connectors: Vec<SemanticConnector>,
    pub diagnostics: Vec<Diagnostic>,
}
