use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::svg::builder::js_number;

use super::builder::{
    Flow, NodeInput, Point, SourceStrategy, WardleyAnnotation, WardleyBuildResult, WardleyBuilder,
    WardleyForce, WardleyLink, WardleyNote, WardleyTrend,
};
use super::parser::{LabelAst, WardleyAst};

#[derive(Debug, Clone)]
pub struct WardleyModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub data: WardleyBuildResult,
}

// Values up to 1 are fractions; larger ones are already percentages.
fn to_percent(value: f64, context: &str) -> Result<f64> {
    let normalized = if value <= 1.0 { value * 100.0 } else { value };
    if normalized < 0.0 || normalized > 100.0 {
        return Err(PeleError::new(
            format!(
                "{} must be between 0-1 (decimal) or 0-100 (percentage). Received: {}",
                context,
                js_number(value)
            ),
            ErrorCode::Semantic,
        )
        .of("wardley"));
    }
    Ok(normalized)
}

fn to_coordinates(visibility: f64, evolution: f64, context: &str) -> Result<Point> {
    Ok(Point {
        x: to_percent(evolution, &format!("{} evolution", context))?,
        y: to_percent(visibility, &format!("{} visibility", context))?,
    })
}

fn flow_from_port(port: Option<&str>) -> Option<Flow> {
    match port {
        Some("+<>") => Some(Flow::Bidirectional),
        Some("+<") => Some(Flow::Backward),
        Some("+>") => Some(Flow::Forward),
        _ => None,
    }
}

fn flow_from_arrow(arrow: Option<&str>) -> (Option<Flow>, Option<String>) {
    let Some(arrow) = arrow else {
        return (None, None);
    };
    if !arrow.starts_with('+') {
        return (None, None);
    }
    let label = arrow
        .strip_prefix("+'")
        .and_then(|rest| rest.find('\'').map(|end| rest[..end].to_string()));
    let flow = if arrow.contains("<>") {
        Some(Flow::Bidirectional)
    } else if arrow.contains('<') {
        Some(Flow::Backward)
    } else if arrow.contains('>') {
        Some(Flow::Forward)
    } else {
        None
    };
    (flow, label)
}

fn offsets(label: Option<&LabelAst>) -> (Option<f64>, Option<f64>) {
    match label {
        Some(l) => (
            Some((if l.neg_x { -1.0 } else { 1.0 }) * l.offset_x),
            Some((if l.neg_y { -1.0 } else { 1.0 }) * l.offset_y),
        ),
        None => (None, None),
    }
}

fn collapse_newline_spaces(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' && i + 1 < chars.len() && is_space(chars[i + 1]) {
            out.push('\n');
            i += 1;
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

// Mermaid's populateDb: fills the builder from the syntax tree, in Mermaid's order.
pub fn populate(ast: &WardleyAst, db: &mut WardleyBuilder) -> Result<()> {
    if let Some(size) = &ast.size {
        db.set_size(size.width, size.height);
    }

    if let Some(stages) = &ast.evolution_stages {
        let names = stages
            .iter()
            .map(|stage| {
                let first = stage.name.trim_matches(is_space);
                match stage.second_name.as_deref().filter(|s| !s.is_empty()) {
                    Some(second) => format!("{} / {}", first, second.trim_matches(is_space)),
                    None => first.to_string(),
                }
            })
            .collect();
        let boundaries = stages.iter().filter_map(|stage| stage.boundary).collect();
        db.set_axes(names, boundaries);
    }

    for anchor in &ast.anchors {
        let at = to_coordinates(
            anchor.visibility,
            anchor.evolution,
            &format!("Anchor \"{}\"", anchor.name),
        )?;
        db.add_node(NodeInput {
            id: anchor.name.clone(),
            label: anchor.name.clone(),
            x: Some(at.x),
            y: Some(at.y),
            class_name: Some("anchor".to_string()),
            label_offset_x: None,
            label_offset_y: None,
            inertia: None,
            source_strategy: None,
        });
    }

    for component in &ast.components {
        let at = to_coordinates(
            component.visibility,
            component.evolution,
            &format!("Component \"{}\"", component.name),
        )?;
        let (label_offset_x, label_offset_y) = offsets(component.label.as_ref());
        let strat = component
            .decorator_strategy
            .as_deref()
            .and_then(SourceStrategy::from_str_name);
        db.add_node(NodeInput {
            id: component.name.clone(),
            label: component.name.clone(),
            x: Some(at.x),
            y: Some(at.y),
            class_name: Some("component".to_string()),
            label_offset_x,
            label_offset_y,
            inertia: Some(Some(component.inertia)),
            source_strategy: Some(strat),
        });
    }

    for note in &ast.notes {
        let at = to_coordinates(note.visibility, note.evolution, &format!("Note \"{}\"", note.text))?;
        db.add_note(WardleyNote { text: note.text.clone(), x: at.x, y: at.y });
    }

    for pipeline in &ast.pipelines {
        let Some(y) = db.get_node(&pipeline.parent).and_then(|n| n.y) else {
            return Err(PeleError::new(
                format!(
                    "Pipeline \"{}\" must reference an existing component with coordinates.",
                    pipeline.parent
                ),
                ErrorCode::Semantic,
            )
            .of("wardley"));
        };
        db.start_pipeline(&pipeline.parent);
        for component in &pipeline.components {
            let id = format!("{}_{}", pipeline.parent, component.name);
            let (label_offset_x, label_offset_y) = offsets(component.label.as_ref());
            let x = to_percent(
                component.evolution,
                &format!("Pipeline component \"{}\" evolution", component.name),
            )?;
            db.add_node(NodeInput {
                id: id.clone(),
                label: component.name.clone(),
                x: Some(x),
                y: Some(y),
                class_name: Some("pipeline-component".to_string()),
                label_offset_x,
                label_offset_y,
                inertia: Some(None),
                source_strategy: Some(None),
            });
            db.add_pipeline_component(&pipeline.parent, &id);
        }
    }

    for link in &ast.links {
        let dashed = link
            .arrow
            .as_deref()
            .map_or(false, |a| a.contains("-.->") || a.contains(".-."));
        let (arrow_flow, arrow_label) = flow_from_arrow(link.arrow.as_deref());
        let flow = flow_from_port(link.from_port.as_deref())
            .or_else(|| flow_from_port(link.to_port.as_deref()))
            .or(arrow_flow);
        let source = db.resolve_node_id(&link.from);
        let target = db.resolve_node_id(&link.to);
        db.add_link(WardleyLink {
            source,
            target,
            dashed,
            label: arrow_label.or_else(|| link.link_label.clone()),
            flow,
        });
    }

    for evolve in &ast.evolves {
        if let Some(y) = db.get_node(&evolve.component).and_then(|n| n.y) {
            let target_x = to_percent(
                evolve.target,
                &format!("Evolve target for \"{}\"", evolve.component),
            )?;
            db.add_trend(WardleyTrend {
                node_id: evolve.component.clone(),
                target_x,
                target_y: y,
            });
        }
    }

    if let Some(first) = ast.annotations.first() {
        let at = to_coordinates(first.x, first.y, "Annotations box")?;
        db.set_annotations_box(at.x, at.y);
    }

    for annotation in &ast.annotation {
        let at = to_coordinates(
            annotation.x,
            annotation.y,
            &format!("Annotation {}", js_number(annotation.number)),
        )?;
        db.add_annotation(WardleyAnnotation {
            number: annotation.number,
            coordinates: vec![Point { x: at.x, y: at.y }],
            text: Some(annotation.text.clone()),
        });
    }

    for accelerator in &ast.accelerators {
        let at = to_coordinates(
            accelerator.x,
            accelerator.y,
            &format!("Accelerator \"{}\"", accelerator.name),
        )?;
        db.add_accelerator(WardleyForce { name: accelerator.name.clone(), x: at.x, y: at.y });
    }

    for deaccelerator in &ast.deaccelerators {
        let at = to_coordinates(
            deaccelerator.x,
            deaccelerator.y,
            &format!("Deaccelerator \"{}\"", deaccelerator.name),
        )?;
        db.add_deaccelerator(WardleyForce { name: deaccelerator.name.clone(), x: at.x, y: at.y });
    }

    Ok(())
}

pub fn build_wardley(ast: WardleyAst, title: Option<String>) -> Result<WardleyModel> {
    let mut db = WardleyBuilder::default();
    populate(&ast, &mut db)?;
    let data = db.build()?;
    Ok(WardleyModel {
        title: ast
            .title
            .filter(|s| !s.is_empty())
            .or_else(|| title.filter(|s| !s.is_empty())),
        acc_title: ast
            .acc_title
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches(is_space).to_string()),
        acc_descr: ast
            .acc_descr
            .filter(|s| !s.is_empty())
            .map(|s| collapse_newline_spaces(&s)),
        data,
    })
}
