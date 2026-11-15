use super::types::DiagnosticId;

// Mermaid's diagnostic vocabulary for agentflow. Only the ids in EMITTED are raised by a parser;
// the rest are reserved for tools that check a diagram's meaning.
pub const ALL_DIAGNOSTICS: &[DiagnosticId] = &[
    DiagnosticId::ShapeUnsupported,
    DiagnosticId::ShapeRemoved,
    DiagnosticId::EdgeOperatorUnsupported,
    DiagnosticId::ReferenceEdgeLabelRejected,
    DiagnosticId::ConnectorRefUnresolved,
    DiagnosticId::ConnectorRefNotAConnector,
    DiagnosticId::MetadataKeyMisapplied,
    DiagnosticId::DuplicateIdNode,
    DiagnosticId::ReservedSyntheticId,
    DiagnosticId::ContainmentViolation,
    DiagnosticId::EdgeSemanticContradiction,
    DiagnosticId::FlowNoInput,
];

pub const EMITTED: &[DiagnosticId] = &[
    DiagnosticId::ShapeUnsupported,
    DiagnosticId::ShapeRemoved,
    DiagnosticId::ContainmentViolation,
];

pub const RESERVED: &[DiagnosticId] = &[
    DiagnosticId::EdgeOperatorUnsupported,
    DiagnosticId::ReferenceEdgeLabelRejected,
    DiagnosticId::ConnectorRefUnresolved,
    DiagnosticId::ConnectorRefNotAConnector,
    DiagnosticId::MetadataKeyMisapplied,
    DiagnosticId::DuplicateIdNode,
    DiagnosticId::ReservedSyntheticId,
    DiagnosticId::EdgeSemanticContradiction,
    DiagnosticId::FlowNoInput,
];
