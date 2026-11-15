use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::options::{fit_width, RenderOptions, Rendered};
use crate::svg::builder::{
    esc, esc_text, js_number, js_round, label_svg, num, shared_style, span_style, text_style,
};
use crate::svg::edges::{marker, marker_trim};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::ROUNDED;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;

use super::builder::{Flow, SourceStrategy, WardleyNode};
use super::model::WardleyModel;

const DEFAULT_STAGES: [&str; 4] = ["Genesis", "Custom Built", "Product", "Commodity"];
// Mermaid's canvas has this much room around the plot on every side.
const CANVAS_MARGIN: f64 = 48.0;
const R: f64 = 6.0;
const SQUARE: f64 = 10.0;
const LABEL_OFFSET: f64 = 8.0;
const PIPELINE_PAD: f64 = 15.0;
const FORCE_WIDTH: f64 = 60.0;
const FORCE_HEIGHT: f64 = 30.0;
const FORCE_HEAD: f64 = 20.0;
const ACCENT: &str = "var(--_c)";
const LINE: &str = "var(--_l)";

#[derive(Debug, Clone, Copy)]
struct Placed {
    x: f64,
    y: f64,
}

fn setting(config: &Value, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match config.get("wardley-beta").get(key).as_f64() {
        Some(v) if v >= min && v <= max => v,
        _ => fallback,
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

// Writes a label with its lines starting at x and its first baseline at y.
fn start_label(label: &Label, x: f64, y: f64, attrs: &str) -> String {
    let shared = shared_style(label);
    let mut body = String::new();
    let mut baseline = y;
    for line in &label.lines {
        let mut first = true;
        for span in line {
            let st = span_style(span.style & !shared);
            if first {
                body.push_str(&format!(
                    "<tspan x=\"{}\" y=\"{}\"{}>{}</tspan>",
                    num(x),
                    num(baseline),
                    st,
                    esc(&span.text)
                ));
            } else {
                body.push_str(&format!("<tspan{}>{}</tspan>", st, esc(&span.text)));
            }
            first = false;
        }
        baseline += label.line_height;
    }
    if body.is_empty() {
        String::new()
    } else {
        format!("<text{} xml:space=\"preserve\">{}</text>", text_style(attrs, shared), body)
    }
}

fn dimension(config: &Value, given: Option<f64>, key: &str, fallback: f64) -> f64 {
    let value = match given {
        Some(v) if v.is_finite() => v,
        _ => setting(config, key, fallback, 1.0, 1e6),
    };
    clamp(value, 200.0, 10000.0) - 2.0 * CANVAS_MARGIN
}

pub fn render_wardley(model: &WardleyModel, config: &Value, options: &RenderOptions) -> Rendered {
    let natural = dimension(config, model.data.size.map(|s| s.width), "width", 900.0);
    fit_width(options, natural, 160.0, |plot_w, tight| {
        draw(model, config, options, plot_w, natural, tight)
    })
}

struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl Bounds {
    fn grow(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        if x0 < self.min_x {
            self.min_x = x0;
        }
        if y0 < self.min_y {
            self.min_y = y0;
        }
        if x1 > self.max_x {
            self.max_x = x1;
        }
        if y1 > self.max_y {
            self.max_y = y1;
        }
    }
}

fn reach(
    node: &WardleyNode,
    anchor_slot: &mut Option<Label>,
    ux: f64,
    uy: f64,
    small: f64,
    plot_w: f64,
    tight: bool,
    measurer: &dyn TextMeasurer,
) -> f64 {
    if node.class_name.as_deref() == Some("anchor") {
        let label = anchor_slot.get_or_insert_with(|| {
            let max_w = if tight { (plot_w / 2.0).max(64.0) } else { 4000.0 };
            layout_label(Some(&node.label), false, measurer, small, max_w, style::BOLD)
        });
        return 1.0 / (ux / (label.width / 2.0 + 5.0)).hypot(uy / (small * 0.8));
    }
    if node.is_pipeline_parent == Some(true) {
        return SQUARE / std::f64::consts::SQRT_2;
    }
    if node.source_strategy.is_some() {
        2.0 * R
    } else {
        R
    }
}

fn draw(
    model: &WardleyModel,
    config: &Value,
    options: &RenderOptions,
    plot_w: f64,
    natural_w: f64,
    tight: bool,
) -> Rendered {
    let data = &model.data;
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.8125);
    let tiny = js_round(size * 0.6875);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    // A narrower map is not as much shorter, so that components keep some room between them.
    let plot_h =
        dimension(config, data.size.map(|s| s.height), "height", 600.0) * (plot_w / natural_w).max(0.75);
    let show_grid = config.get("wardley-beta").get("showGrid").as_bool() == Some(true);

    let mut bounds = Bounds { min_x: 0.0, min_y: 0.0, max_x: plot_w, max_y: plot_h };

    let default_max_w = if tight { (plot_w / 2.0).min(120.0).max(64.0) } else { 4000.0 };
    let text_with = |raw: &str, font_size: f64, max_width: f64, st: u8| -> Label {
        layout_label(Some(raw), false, measurer, font_size, max_width, st)
    };
    let text = |raw: &str, font_size: f64| -> Label { text_with(raw, font_size, default_max_w, 0) };
    let project_x = |value: f64| -> f64 { (value / 100.0) * plot_w };
    let project_y = |value: f64| -> f64 { plot_h - (value / 100.0) * plot_h };

    let centered = |bounds: &mut Bounds, label: &Label, cx: f64, cy: f64, attrs: &str| -> String {
        bounds.grow(
            cx - label.width / 2.0,
            cy - label.height / 2.0,
            cx + label.width / 2.0,
            cy + label.height / 2.0,
        );
        label_svg(label, cx, cy, attrs)
    };
    let starting = |bounds: &mut Bounds, label: &Label, x: f64, y: f64, attrs: &str| -> String {
        bounds.grow(x, y - label.size, x + label.width, y - label.size + label.height + 4.0);
        start_label(label, x, y, attrs)
    };

    let custom_stages: Option<&[String]> = data.axes.stages.as_deref().filter(|s| !s.is_empty());
    let stage_count = custom_stages.map_or(DEFAULT_STAGES.len(), |s| s.len());
    let boundaries = data.axes.stage_boundaries.as_deref();
    let custom = boundaries.map_or(false, |b| b.len() == stage_count);
    let mut frame = String::new();
    let mut stage_labels = String::new();
    let mut stage_bottom = plot_h;
    let mut previous = 0.0;

    for index in 0..stage_count {
        let stage_name = match custom_stages {
            Some(list) => list[index].as_str(),
            None => DEFAULT_STAGES[index],
        };
        let start = if custom {
            previous
        } else {
            index as f64 / stage_count as f64
        };
        let end = if let (true, Some(b)) = (custom, boundaries) {
            clamp(b[index], 0.0, 1.0)
        } else {
            (index + 1) as f64 / stage_count as f64
        };
        previous = end;
        if index > 0 {
            frame.push_str(&format!(
                "<path class=\"pele-wardley-stage\" d=\"M{},0V{}\" stroke=\"var(--_b)\" stroke-dasharray=\"4 4\"/>",
                num(start * plot_w),
                num(plot_h)
            ));
        }
        let label = text_with(stage_name, small, ((end - start).abs() * plot_w - 12.0).max(48.0), 0);
        stage_labels.push_str(&centered(
            &mut bounds,
            &label,
            ((start + end) / 2.0) * plot_w,
            plot_h + 8.0 + label.height / 2.0,
            &format!(" class=\"pele-wardley-stage-label\" font-size=\"{}\" fill=\"var(--_m)\"", num(small)),
        ));
        stage_bottom = stage_bottom.max(plot_h + 8.0 + label.height);
    }

    if show_grid {
        for i in 1..4 {
            let k = i as f64;
            frame.push_str(&format!(
                "<path class=\"pele-wardley-grid\" d=\"M{},0V{}M0,{}H{}\" stroke=\"var(--_b)\" stroke-dasharray=\"2 6\"/>",
                num((plot_w * k) / 4.0),
                num(plot_h),
                num((plot_h * k) / 4.0),
                num(plot_w)
            ));
        }
    }
    frame.push_str(&format!(
        "<path class=\"pele-wardley-axis\" d=\"M0,0V{}H{}\" stroke=\"{}\"/>",
        num(plot_h),
        num(plot_w),
        LINE
    ));

    let x_title = text(data.axes.x_label.as_deref().unwrap_or("Evolution"), small);
    let y_title = text(data.axes.y_label.as_deref().unwrap_or("Visibility"), small);
    let axis_attrs = format!(
        " class=\"pele-wardley-axis-label\" font-size=\"{}\" font-weight=\"var(--_hw)\"",
        num(small)
    );
    let axis_labels = format!(
        "{}{}",
        centered(&mut bounds, &x_title, plot_w / 2.0, stage_bottom + 6.0 + x_title.height / 2.0, &axis_attrs),
        label_svg(
            &y_title,
            -16.0,
            plot_h / 2.0,
            &format!("{} transform=\"rotate(-90 -16 {})\"", axis_attrs, num(plot_h / 2.0))
        )
    );
    bounds.grow(-16.0 - y_title.height / 2.0, 0.0, 0.0, 0.0);
    let legend_top = stage_bottom + 6.0 + x_title.height + 10.0;

    let mut placed: Vec<Placed> = Vec::with_capacity(data.nodes.len());
    let mut pos_by_id: HashMap<&str, usize> = HashMap::with_capacity(data.nodes.len());
    for (idx, node) in data.nodes.iter().enumerate() {
        pos_by_id.insert(node.id.as_str(), idx);
        placed.push(Placed {
            x: project_x(node.x.unwrap_or(0.0)),
            y: project_y(node.y.unwrap_or(0.0)),
        });
    }

    // Pipelines: a box around the components, with the parent as a square on its top edge.
    let mut pipelines = String::new();
    let mut members: HashMap<&str, HashSet<&str>> = HashMap::new();
    for pipeline in &data.pipelines {
        let set: HashSet<&str> = pipeline.component_ids.iter().map(|s| s.as_str()).collect();
        members.insert(pipeline.node_id.as_str(), set);
        let mut parts: Vec<Placed> = Vec::new();
        for id in &pipeline.component_ids {
            if let Some(&idx) = pos_by_id.get(id.as_str()) {
                parts.push(placed[idx]);
            }
        }
        if parts.is_empty() {
            continue;
        }
        parts.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
        let left = parts[0].x;
        let right = parts[parts.len() - 1].x;
        let y = parts[0].y;
        let top = y - 2.0 * R;
        if let Some(&parent_idx) = pos_by_id.get(pipeline.node_id.as_str()) {
            placed[parent_idx].x = (left + right) / 2.0;
            placed[parent_idx].y = top - SQUARE / 6.0;
        }
        bounds.grow(left - PIPELINE_PAD, top, right + PIPELINE_PAD, top + 4.0 * R);
        let inner_line = if parts.len() > 1 {
            format!(
                "<path d=\"M{},{}H{}\" stroke=\"{}\" stroke-dasharray=\"4 4\"/>",
                num(left),
                num(y),
                num(right),
                LINE
            )
        } else {
            String::new()
        };
        pipelines.push_str(&format!(
            "<g class=\"pele-wardley-pipeline\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>{}</g>",
            esc_text(&pipeline.node_id),
            num(left - PIPELINE_PAD),
            num(top),
            num(right - left + 2.0 * PIPELINE_PAD),
            num(4.0 * R),
            ROUNDED,
            inner_line
        ));
    }

    let mut anchor_labels: Vec<Option<Label>> = vec![None; data.nodes.len()];
    // Quadrants around a node that a link runs through: 1 up and right, 2 down and right, 4 up and left, 8 down and left.
    let mut taken: Vec<u8> = vec![0; data.nodes.len()];
    let occupy = |taken: &mut [u8], idx: usize, ux: f64, uy: f64| {
        if ux.abs() < 0.25 || uy.abs() < 0.25 {
            return;
        }
        let bit = if ux > 0.0 {
            if uy < 0.0 { 1 } else { 2 }
        } else if uy < 0.0 {
            4
        } else {
            8
        };
        taken[idx] |= bit;
    };

    let mut links = String::new();
    for link in &data.links {
        let (Some(&from_idx), Some(&to_idx)) = (
            pos_by_id.get(link.source.as_str()),
            pos_by_id.get(link.target.as_str()),
        ) else {
            continue;
        };
        if from_idx == to_idx {
            continue;
        }
        // A pipeline component belongs to its parent already.
        if members
            .get(link.target.as_str())
            .map_or(false, |s| s.contains(link.source.as_str()))
        {
            continue;
        }
        let from = placed[from_idx];
        let to = placed[to_idx];
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let distance = dx.hypot(dy);
        if distance < 0.01 {
            continue;
        }
        let ux = dx / distance;
        let uy = dy / distance;
        let start = reach(
            &data.nodes[from_idx],
            &mut anchor_labels[from_idx],
            ux,
            uy,
            small,
            plot_w,
            tight,
            measurer,
        );
        let end = reach(
            &data.nodes[to_idx],
            &mut anchor_labels[to_idx],
            ux,
            uy,
            small,
            plot_w,
            tight,
            measurer,
        );
        if start + end >= distance {
            continue;
        }
        occupy(&mut taken, from_idx, ux, uy);
        occupy(&mut taken, to_idx, -ux, -uy);
        let mut x1 = from.x + ux * start;
        let mut y1 = from.y + uy * start;
        let mut x2 = to.x - ux * end;
        let mut y2 = to.y - uy * end;
        let mut heads = String::new();
        if matches!(link.flow, Some(Flow::Forward) | Some(Flow::Bidirectional)) {
            heads.push_str(&marker("arrow_point", x2, y2, ux, uy, LINE, "none"));
            x2 -= ux * marker_trim("arrow_point");
            y2 -= uy * marker_trim("arrow_point");
        }
        if matches!(link.flow, Some(Flow::Backward) | Some(Flow::Bidirectional)) {
            heads.push_str(&marker("arrow_point", x1, y1, -ux, -uy, LINE, "none"));
            x1 += ux * marker_trim("arrow_point");
            y1 += uy * marker_trim("arrow_point");
        }
        let caption = if let Some(lbl) = link.label.as_deref().filter(|s| !s.is_empty()) {
            let label = text_with(lbl, tiny + 1.0, 200.0, 0);
            // Beside the middle of the line, on its upper side.
            let side = if ux >= 0.0 { 1.0 } else { -1.0 };
            let px = uy * side;
            let py = -ux * side;
            let away = 5.0 + (px.abs() * label.width) / 2.0 + (py.abs() * label.height) / 2.0;
            centered(
                &mut bounds,
                &label,
                (from.x + to.x) / 2.0 + px * away,
                (from.y + to.y) / 2.0 + py * away,
                &format!(" class=\"pele-edge-label\" font-size=\"{}\" fill=\"var(--_m)\"", num(tiny + 1.0)),
            )
        } else {
            String::new()
        };
        links.push_str(&format!(
            "<g class=\"pele-edge{}\" data-id=\"{}-{}\"><path d=\"M{},{}L{},{}\" stroke=\"{}\"{}/>{}{}</g>",
            if link.dashed { " pele-wardley-dashed" } else { "" },
            esc_text(&link.source),
            esc_text(&link.target),
            num(x1),
            num(y1),
            num(x2),
            num(y2),
            LINE,
            if link.dashed { " stroke-dasharray=\"6 6\"" } else { "" },
            heads,
            caption
        ));
    }

    // Evolution: a dashed arrow to where the component is heading, drawn as an open circle.
    let mut trends = String::new();
    for trend in &data.trends {
        let Some(&origin_idx) = pos_by_id.get(trend.node_id.as_str()) else {
            continue;
        };
        let origin = placed[origin_idx];
        let tx = project_x(trend.target_x);
        let ty = project_y(trend.target_y);
        let dx = tx - origin.x;
        let dy = ty - origin.y;
        let distance = dx.hypot(dy);
        let start = if distance > 0.0 {
            reach(
                &data.nodes[origin_idx],
                &mut anchor_labels[origin_idx],
                dx / distance,
                dy / distance,
                small,
                plot_w,
                tight,
                measurer,
            )
        } else {
            0.0
        };
        let arrow = if distance > start + R + 4.0 {
            let ux = dx / distance;
            let uy = dy / distance;
            let ex = tx - ux * (R + 2.0);
            let ey = ty - uy * (R + 2.0);
            format!(
                "<path d=\"M{},{}L{},{}\" stroke=\"{}\" stroke-dasharray=\"4 4\"/>{}",
                num(origin.x + ux * start),
                num(origin.y + uy * start),
                num(ex - ux * marker_trim("arrow_point")),
                num(ey - uy * marker_trim("arrow_point")),
                ACCENT,
                marker("arrow_point", ex, ey, ux, uy, ACCENT, "none")
            )
        } else {
            String::new()
        };
        bounds.grow(tx - R, ty - R, tx + R, ty + R);
        trends.push_str(&format!(
            "<g class=\"pele-wardley-trend\" data-id=\"{}\">{}<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"var(--_bg)\" stroke=\"{}\"/></g>",
            esc_text(&trend.node_id),
            arrow,
            num(tx),
            num(ty),
            num(R),
            ACCENT
        ));
    }

    let mut nodes = String::new();
    let tri = 1.2 * R;
    let dot = 0.7 * R;
    for (idx, node) in data.nodes.iter().enumerate() {
        let x = placed[idx].x;
        let y = placed[idx].y;
        let anchor = node.class_name.as_deref() == Some("anchor");
        let strategy = node.source_strategy;
        let mut shape = String::new();
        if let Some(strat) = strategy {
            let fill = match strat {
                SourceStrategy::Outsource => "var(--_m)",
                SourceStrategy::Buy => "var(--_a)",
                SourceStrategy::Build => "var(--_s)",
                SourceStrategy::Market => "var(--_bg)",
            };
            shape.push_str(&format!(
                "<circle class=\"pele-wardley-{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"{}\"/>",
                strat.as_str(),
                num(x),
                num(y),
                num(2.0 * R),
                fill,
                LINE
            ));
        }
        if strategy == Some(SourceStrategy::Market) {
            let bx = tri * (std::f64::consts::PI / 6.0).cos();
            let by = y + tri * (std::f64::consts::PI / 6.0).sin();
            shape.push_str(&format!(
                "<path d=\"M{},{}L{},{}H{}Z\" fill=\"none\" stroke=\"{}\"/>",
                num(x),
                num(y - tri),
                num(x - bx),
                num(by),
                num(x + bx),
                LINE
            ));
            for (cx, cy) in [(x, y - tri), (x - bx, by), (x + bx, by)] {
                shape.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"var(--_bg)\" stroke=\"{}\"/>",
                    num(cx),
                    num(cy),
                    num(dot),
                    LINE
                ));
            }
        } else if node.is_pipeline_parent == Some(true) {
            shape.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_bg)\" stroke=\"{}\"/>",
                num(x - SQUARE / 2.0),
                num(y - SQUARE / 2.0),
                num(SQUARE),
                num(SQUARE),
                LINE
            ));
        } else if !anchor {
            shape.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"var(--_bg)\" stroke=\"{}\"/>",
                num(x),
                num(y),
                num(R),
                LINE
            ));
        }
        if node.inertia == Some(true) {
            let offset = (if node.is_pipeline_parent == Some(true) {
                SQUARE / 2.0
            } else {
                R
            }) + 10.0
                + if strategy.is_some() { R + 4.0 } else { 0.0 };
            shape.push_str(&format!(
                "<rect class=\"pele-wardley-inertia\" x=\"{}\" y=\"{}\" width=\"4\" height=\"{}\" fill=\"{}\"/>",
                num(x + offset),
                num(y - R),
                num(2.0 * R),
                LINE
            ));
        }
        let ox = node.label_offset_x.map(|v| clamp(v, -5000.0, 5000.0));
        let oy = node.label_offset_y.map(|v| clamp(v, -5000.0, 5000.0));
        let caption = if anchor {
            let label = anchor_labels[idx].take().unwrap_or_else(|| {
                let max_w = if tight { (plot_w / 2.0).max(64.0) } else { 4000.0 };
                text_with(&node.label, small, max_w, style::BOLD)
            });
            centered(
                &mut bounds,
                &label,
                x + ox.unwrap_or(0.0),
                y + oy.unwrap_or(-3.0),
                &format!(" class=\"pele-label\" font-size=\"{}\" font-weight=\"var(--_hw)\"", num(small)),
            )
        } else {
            let label = text(&node.label, small);
            let attrs = format!(" class=\"pele-label\" font-size=\"{}\"", num(small));
            let lift = LABEL_OFFSET + if strategy.is_some() { 10.0 } else { 0.0 };
            if ox.is_some() || oy.is_some() {
                starting(
                    &mut bounds,
                    &label,
                    x + ox.unwrap_or(lift),
                    y + oy.unwrap_or(-lift),
                    &attrs,
                )
            } else if node.in_pipeline == Some(true) {
                centered(&mut bounds, &label, x, y + 2.0 * R + 4.0 + label.height / 2.0, &attrs)
            } else {
                // The label goes up and to the right unless a link leaves that way; then the next free corner.
                let busy = taken[idx];
                let corner = if (busy & 1) == 0 {
                    1
                } else if (busy & 2) == 0 {
                    2
                } else if (busy & 4) == 0 {
                    4
                } else if (busy & 8) == 0 {
                    8
                } else {
                    1
                };
                let right = corner == 1 || corner == 2;
                let up = corner == 1 || corner == 4;
                starting(
                    &mut bounds,
                    &label,
                    if right { x + lift } else { x - lift - label.width },
                    if up { y - lift } else { y + lift + small * 0.75 },
                    &attrs,
                )
            }
        };
        bounds.grow(x - 2.0 * R, y - 2.0 * R, x + 2.0 * R + 20.0, y + 2.0 * R);
        let kind_class = if anchor {
            "anchor"
        } else if node.is_pipeline_parent == Some(true) {
            "pipeline-parent"
        } else if node.in_pipeline == Some(true) {
            "pipeline-component"
        } else {
            "component"
        };
        nodes.push_str(&format!(
            "<g class=\"pele-node pele-wardley-{}\" data-id=\"{}\">{}{}</g>",
            kind_class,
            esc_text(&node.id),
            shape,
            caption
        ));
    }

    let mut annotations = String::new();
    for annotation in &data.annotations {
        let num_str = js_number(annotation.number);
        let label = text_with(&num_str, tiny, 4000.0, style::BOLD);
        let mut marks = String::new();
        for coordinate in &annotation.coordinates {
            let x = project_x(coordinate.x);
            let y = project_y(coordinate.y);
            let r = (label.width / 2.0 + 4.0).max(10.0);
            bounds.grow(x - r, y - r, x + r, y + r);
            marks.push_str(&format!(
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"var(--_bg)\" stroke=\"{}\"/>{}",
                num(x),
                num(y),
                num(r),
                LINE,
                label_svg(
                    &label,
                    x,
                    y,
                    &format!(" font-size=\"{}\" font-weight=\"var(--_hw)\"", num(tiny))
                )
            ));
        }
        annotations.push_str(&format!(
            "<g class=\"pele-wardley-annotation\" data-id=\"{}\">{}</g>",
            esc_text(&num_str),
            marks
        ));
    }

    let mut listed: Vec<_> = data
        .annotations
        .iter()
        .filter(|a| a.text.as_deref().map_or(false, |t| !t.is_empty()))
        .collect();
    listed.sort_by(|a, b| a.number.partial_cmp(&b.number).unwrap_or(Ordering::Equal));
    if !listed.is_empty() {
        let list_max_w = if tight { (plot_w - 20.0).max(80.0) } else { 320.0 };
        let lines: Vec<Label> = listed
            .iter()
            .map(|a| {
                text_with(
                    &format!("{}. {}", js_number(a.number), a.text.as_deref().unwrap_or("")),
                    small,
                    list_max_w,
                    0,
                )
            })
            .collect();
        let mut width: f64 = 0.0;
        let mut height: f64 = 0.0;
        for line in &lines {
            width = width.max(line.width);
            height += line.height;
        }
        width += 20.0;
        height += 14.0;
        // Without a position the list goes under the map, where it covers nothing.
        let (bx, by) = match data.annotations_box {
            Some(box_pos) => (
                clamp(project_x(box_pos.x), 0.0, (plot_w - width).max(0.0)),
                clamp(project_y(box_pos.y), 0.0, (plot_h - height).max(0.0)),
            ),
            None => (0.0, legend_top),
        };
        bounds.grow(bx, by, bx + width, by + height);
        let mut rows = String::new();
        let mut y = by + 7.0;
        for line in &lines {
            rows.push_str(&start_label(
                line,
                bx + 10.0,
                y + line.line_height / 2.0 + small * 0.35,
                &format!(" font-size=\"{}\"", num(small)),
            ));
            y += line.height;
        }
        annotations.push_str(&format!(
            "<g class=\"pele-wardley-annotations\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}/>{}</g>",
            num(bx),
            num(by),
            num(width),
            num(height),
            ROUNDED,
            rows
        ));
    }

    let mut notes = String::new();
    for note in &data.notes {
        let lbl = text_with(&note.text, small, (plot_w * 0.6).min(360.0), style::BOLD);
        notes.push_str(&starting(
            &mut bounds,
            &lbl,
            project_x(note.x),
            project_y(note.y),
            &format!(
                " class=\"pele-wardley-note\" font-size=\"{}\" font-weight=\"var(--_hw)\"",
                num(small)
            ),
        ));
    }

    // Accelerators point right, deaccelerators left.
    let mut forces = String::new();
    let mut draw_force = |name: &str, px: f64, py: f64, forward: bool| {
        let x = project_x(px);
        let y = project_y(py);
        let half = FORCE_HEIGHT / 2.0;
        let tip = if forward { x + FORCE_WIDTH } else { x };
        let tail = if forward { x } else { x + FORCE_WIDTH };
        let neck = if forward {
            x + FORCE_WIDTH - FORCE_HEAD
        } else {
            x + FORCE_HEAD
        };
        let d = format!(
            "M{},{}H{}V{}L{},{}L{},{}V{}H{}Z",
            num(tail),
            num(y - half),
            num(neck),
            num(y - half - 8.0),
            num(tip),
            num(y),
            num(neck),
            num(y + half + 8.0),
            num(y + half),
            num(tail)
        );
        bounds.grow(x, y - half - 8.0, x + FORCE_WIDTH, y + half + 8.0);
        let lbl = text_with(name, tiny + 1.0, 160.0, style::BOLD);
        forces.push_str(&format!(
            "<g class=\"pele-wardley-{}\" data-id=\"{}\"><path d=\"{}\" fill=\"var(--_s)\" stroke=\"{}\"/>{}</g>",
            if forward { "accelerator" } else { "deaccelerator" },
            esc_text(name),
            d,
            LINE,
            centered(
                &mut bounds,
                &lbl,
                x + FORCE_WIDTH / 2.0,
                y + half + 8.0 + 12.0,
                &format!(" font-size=\"{}\" font-weight=\"var(--_hw)\"", num(tiny + 1.0))
            )
        ));
    };
    for a in &data.accelerators {
        draw_force(&a.name, a.x, a.y, true);
    }
    for d in &data.deaccelerators {
        draw_force(&d.name, d.x, d.y, false);
    }

    let title_max_w = if tight { plot_w.max(80.0) } else { 4000.0 };
    let title = layout_label(model.title.as_deref(), false, measurer, size, title_max_w, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let left = bounds.min_x.min(plot_w / 2.0 - title.width / 2.0);
    let right = bounds.max_x.max(plot_w / 2.0 + title.width / 2.0);
    let tx = pad - left;
    let ty = pad + title_height - bounds.min_y;
    let total_width = (right - left + 2.0 * pad).ceil();
    let total_height = (bounds.max_y - bounds.min_y + title_height + 2.0 * pad).ceil();

    let links_group = if links.is_empty() {
        String::new()
    } else {
        format!("<g class=\"pele-edges\" fill=\"none\">{}</g>", links)
    };
    let trends_group = if trends.is_empty() {
        String::new()
    } else {
        format!("<g class=\"pele-wardley-trends\" fill=\"none\">{}</g>", trends)
    };
    let content = format!(
        "{}<g transform=\"translate({},{})\"><g class=\"pele-wardley-frame\" fill=\"none\">{}</g>{}{}{}{}{}{}{}{}{}</g>",
        label_svg(
            &title,
            tx + plot_w / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        num(tx),
        num(ty),
        frame,
        stage_labels,
        axis_labels,
        pipelines,
        links_group,
        trends_group,
        nodes,
        annotations,
        notes,
        forces
    );

    let fixed = config.get("wardley-beta").get("useMaxWidth").as_bool() == Some(false);
    let doc = Document {
        id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
        responsive: options.responsive.unwrap_or(!fixed),
        font_family: options.font_family.as_deref(),
        font_family_mono: options.font_family_mono.as_deref(),
        palette: "",
    };
    let acc = Accessible {
        acc_title: model.acc_title.as_deref(),
        acc_descr: model.acc_descr.as_deref(),
    };

    Rendered {
        svg: svg_document("wardley", total_width, total_height, size, &doc, &acc, &content),
        width: total_width,
        height: total_height,
        links: Vec::new(),
    }
}
