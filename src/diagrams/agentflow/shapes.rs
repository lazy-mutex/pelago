use super::graph::GraphNode;
use super::types::DiagnosticId;
use crate::diagrams::flowchart::shapes::is_valid_shape as is_flowchart_shape;

pub const DEFAULT_SHAPE: &str = "roundedRect";

// Author-facing names for the shapes that carry meaning in an agentflow.
pub fn resolve_shape_alias(shape: Option<&str>) -> Option<&str> {
    let shape = shape?;
    Some(match shape {
        "task" => "roundedRect",
        "tool" => "subroutine",
        "input" => "lean-right",
        "decision" => "diamond",
        "refdoc" => "lin-doc",
        "action" => "hexagon",
        "round" => "rect",
        other => other,
    })
}

pub fn is_removed_shape(shape: &str) -> bool {
    matches!(
        shape,
        "doc"
            | "stadium"
            | "terminal"
            | "circle"
            | "trapezoid"
            | "inv_trapezoid"
            | "inv-trapezoid"
            | "doublecircle"
            | "double-circle"
            | "typeDeclaration"
            | "procs"
            | "lean_left"
            | "lean-left"
            | "in-out"
            | "cylinder"
            | "ellipse"
            | "odd"
            | "tag-rect"
            | "tagged-rectangle"
            | "delay"
            | "half-rounded-rectangle"
            | "lin-rect"
            | "lined-rectangle"
            | "win-pane"
            | "window-pane"
            | "curv-trap"
            | "curved-trapezoid"
    )
}

pub fn is_allowed_shape(shape: &str) -> bool {
    matches!(
        shape,
        "roundedRect"
            | "subroutine"
            | "subprocess"
            | "subproc"
            | "framed-rectangle"
            | "lean-right"
            | "diamond"
            | "lin-doc"
            | "lined-document"
            | "hexagon"
            | "hex"
            | "connector"
            | "collapsedGroup"
    )
}

// Mermaid looks a shape name up with `in` on a plain object, so `constructor` passes there too.
pub fn is_valid_shape(name: &str) -> bool {
    is_flowchart_shape(name) || name == "constructor"
}

pub trait ShapeSink {
    fn emit_shape_error(&mut self, id: DiagnosticId, message: String, node_id: &str);
    fn emit_shape_warning(&mut self, id: DiagnosticId, message: String, node_id: &str);
}

// Reduces every node's shape to one agentflow draws, reporting the removed and unsupported ones.
pub fn normalise_node_shapes(nodes: &mut [GraphNode], sink: &mut dyn ShapeSink) {
    for node in nodes {
        if node.is_group {
            continue;
        }
        if node.shape.is_empty() || node.shape == "squareRect" || node.shape == "rect" {
            node.shape = DEFAULT_SHAPE.to_string();
        }
        if is_removed_shape(&node.shape) {
            sink.emit_shape_error(
                DiagnosticId::ShapeRemoved,
                format!("shape \"{}\" was removed in v0.8.1, using \"{}\"", node.shape, DEFAULT_SHAPE),
                &node.id,
            );
            node.shape = DEFAULT_SHAPE.to_string();
        } else if !is_allowed_shape(&node.shape) {
            sink.emit_shape_warning(
                DiagnosticId::ShapeUnsupported,
                format!("shape \"{}\" is not supported, using \"{}\"", node.shape, DEFAULT_SHAPE),
                &node.id,
            );
            node.shape = DEFAULT_SHAPE.to_string();
        }
    }
}
