use super::db::RequirementDb;
use super::graph::{build_requirement_graph, GraphEdge, GraphNode};
use crate::chars::is_space;
use crate::layout::compound::{cedge, cnode, compound_layout, direction, CEdge, CNode, Dir};
use crate::layout::layered::LayeredOptions;
use crate::options::{runs_across, turn_to_fit, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num, shared_style, span_style, text_style};
use crate::svg::edges::{edge_label_svg, route_path_default, EdgePath};
use crate::svg::ink::ink_fill;
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{class_names, resolve_style, rounded, ResolvedStyle};
use crate::svg::title::title_label;
use crate::text::label::{layout_label, line_middle, Label, Span};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 56.0;
const PAD_X: f64 = 14.0;
const PAD_Y: f64 = 10.0;
const ROW_GAP: f64 = 2.0;
const COLUMN_GAP: f64 = 12.0;
const MIN_WIDTH: f64 = 120.0;
const NAME_WRAP: f64 = 280.0;
const VALUE_WRAP: f64 = 240.0;
const LOOP: f64 = 26.0;
const CONTAINS_RADIUS: f64 = 6.0;
const ARROW_LENGTH: f64 = 9.0;
const ARROW_HALF: f64 = 4.5;
const END_STEP: f64 = 28.0;
const END_MARGIN: f64 = 14.0;

struct Row {
    key: Label,
    value: Label,
    height: f64,
}

struct NodeView<'a> {
    node: GraphNode<'a>,
    style: ResolvedStyle,
    kind: Label,
    name: Label,
    rows: Vec<Row>,
    key_width: f64,
    head_height: f64,
    w: f64,
    h: f64,
}

struct SideEnd {
    edge_idx: usize,
    at: usize,
    toward: f64,
}

fn strip_default_class(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix("default") {
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            if is_space(c) {
                return chars.as_str();
            }
        }
        rest
    } else {
        s
    }
}

// Adds bold or italic to a whole label after its markdown is read, since emphasis markers only
// toggle styles the label does not start with.
fn restyle(label: Label, bits: u8, measurer: &dyn TextMeasurer) -> Label {
    if bits == 0 || label.lines.is_empty() {
        return label;
    }
    let mut width = 0.0f64;
    let mut widths = Vec::with_capacity(label.lines.len());
    let size = label.size;
    let lines = label
        .lines
        .into_iter()
        .map(|line| {
            let mut total = 0.0;
            let spans = line
                .into_iter()
                .map(|span| {
                    let st = span.style | bits;
                    let w = measurer.width(&span.text, size, st);
                    total += w;
                    Span { text: span.text, style: st, width: w }
                })
                .collect();
            widths.push(total);
            if total > width {
                width = total;
            }
            spans
        })
        .collect();
    Label { lines, widths, width, ..label }
}

// Writes a measured label with its lines starting at x, the first line's box starting at top.
fn start_text(label: &Label, x: f64, top: f64, attrs: &str) -> String {
    let shared = shared_style(label);
    let mut body = String::new();
    for i in 0..label.lines.len() {
        let line = &label.lines[i];
        let middle = top + line_middle(label, i);
        let y = middle + label.size * 0.35;
        let mut first = true;
        for span in line {
            if span.text.is_empty() {
                continue;
            }
            let pos = if first { format!(" x=\"{}\" y=\"{}\"", num(x), num(y)) } else { String::new() };
            body.push_str(&format!("<tspan{}{}>{}</tspan>", pos, span_style(span.style & !shared), esc(&span.text)));
            first = false;
        }
    }
    if body.is_empty() {
        String::new()
    } else {
        format!("<text{} text-anchor=\"start\" xml:space=\"preserve\">{}</text>", text_style(attrs, shared), body)
    }
}

// The containment mark: a circled plus whose rim touches (x, y), where (dx, dy) points at the node.
fn contains_marker(x: f64, y: f64, dx: f64, dy: f64) -> String {
    let r = CONTAINS_RADIUS;
    let cx = x - dx * r;
    let cy = y - dy * r;
    format!(
        "<circle class=\"pele-marker\" cx=\"{}\" cy=\"{}\" r=\"{}\"/><path class=\"pele-marker\" d=\"M{},{}L{},{}M{},{}L{},{}\"/>",
        num(cx),
        num(cy),
        js_number(r),
        num(cx - dx * r),
        num(cy - dy * r),
        num(cx + dx * r),
        num(cy + dy * r),
        num(cx + dy * r),
        num(cy - dx * r),
        num(cx - dy * r),
        num(cy + dx * r),
    )
}

// An open arrowhead with its tip at (x, y), pointing along (dx, dy).
fn arrow_marker(x: f64, y: f64, dx: f64, dy: f64) -> String {
    let bx = x - dx * ARROW_LENGTH;
    let by = y - dy * ARROW_LENGTH;
    format!(
        "<path class=\"pele-marker\" d=\"M{},{}L{},{}L{},{}\" stroke-linejoin=\"round\"/>",
        num(bx - dy * ARROW_HALF),
        num(by + dx * ARROW_HALF),
        num(x),
        num(y),
        num(bx + dy * ARROW_HALF),
        num(by - dx * ARROW_HALF),
    )
}

// The layout attaches every edge to the middle of a node's side. Ends that share a side are
// spread along it, in the order of where they head, so their markers do not sit on each other.
fn spread_ends(cnodes: &[CNode], cedges: &mut [CEdge], views: &[NodeView<'_>]) {
    let mut sides: HashMap<usize, Vec<SideEnd>> = HashMap::new();
    for (edge_idx, edge) in cedges.iter().enumerate() {
        let route = &edge.route;
        if edge.src == edge.dst || route.len() < 6 {
            continue;
        }
        for at in [0, route.len() - 3] {
            let node = if at == 0 { edge.src } else { edge.dst };
            let c = &cnodes[node];
            let dx = route[at] - c.x;
            let dy = route[at + 1] - c.y;
            let next = if at == 0 { 3 } else { at - 3 };
            let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
                if dy < 0.0 { 0 } else { 2 }
            } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
                if dx < 0.0 { 3 } else { 1 }
            } else {
                continue;
            };
            let key = node * 4 + side;
            let toward = route[if side % 2 == 0 { next } else { next + 1 }];
            sides.entry(key).or_default().push(SideEnd { edge_idx, at, toward });
        }
    }
    for (key, mut list) in sides {
        if list.len() < 2 {
            continue;
        }
        let node = key >> 2;
        let vertical = key % 2 == 1;
        let span = (if vertical { views[node].h } else { views[node].w }) - 2.0 * END_MARGIN;
        let step = 0.0f64.max(END_STEP.min(span / (list.len() - 1) as f64));
        list.sort_by(|a, b| a.toward.partial_cmp(&b.toward).unwrap_or(std::cmp::Ordering::Equal));
        let denom = (list.len() - 1) as f64 / 2.0;
        for (j, end) in list.iter().enumerate() {
            let offset = (j as f64 - denom) * step;
            if vertical {
                cedges[end.edge_idx].route[end.at + 1] = cnodes[node].y + offset;
            } else {
                cedges[end.edge_idx].route[end.at] = cnodes[node].x + offset;
            }
        }
    }
}

pub fn render_requirement(model: &RequirementDb, config: &Value, options: &RenderOptions) -> Rendered {
    if !runs_across(Some(&model.direction)) {
        return draw(model, config, options, false);
    }
    turn_to_fit(options, &|down| draw(model, config, options, down))
}

// `turned` draws a diagram that runs across as one that runs down.
fn draw(model: &RequirementDb, config: &Value, options: &RenderOptions, turned: bool) -> Rendered {
    let graph = build_requirement_graph(model);
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut views: Vec<NodeView<'_>> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());
    for node in graph.nodes {
        let resolved = resolve_style(node.css_styles);
        let base = (if resolved.bold { style::BOLD } else { 0 }) | (if resolved.italic { style::ITALIC } else { 0 });
        let name_size = resolved.font_size.unwrap_or(size);
        let body_size = resolved.font_size.unwrap_or(small);
        let kind_str = match node.requirement {
            Some(req) => format!("«{}»", req.req_type),
            None => "«Element»".to_string(),
        };
        let kind = restyle(layout_label(Some(&kind_str), false, measurer, body_size, 4000.0, 0), base, measurer);
        let name = restyle(layout_label(Some(node.id), true, measurer, name_size, NAME_WRAP, 0), base | style::BOLD, measurer);
        let fields: Vec<(&str, &str)> = if let Some(req) = node.requirement {
            vec![
                ("ID", req.requirement_id.as_str()),
                ("Text", req.text.as_str()),
                ("Risk", req.risk.as_str()),
                ("Verification", req.verify_method.as_str()),
            ]
        } else {
            let elem = node.element.unwrap();
            vec![("Type", elem.elem_type.as_str()), ("Doc Ref", elem.doc_ref.as_str())]
        };
        let mut rows: Vec<Row> = Vec::new();
        let mut key_width = 0.0f64;
        let mut value_width = 0.0f64;
        let mut body_height = 0.0f64;
        for (label_str, text) in fields {
            if text.is_empty() {
                continue;
            }
            let key = restyle(layout_label(Some(label_str), false, measurer, body_size, 4000.0, 0), base, measurer);
            let value = restyle(layout_label(Some(text), true, measurer, body_size, VALUE_WRAP, 0), base, measurer);
            let height = key.height.max(value.height);
            key_width = key_width.max(key.width);
            value_width = value_width.max(value.width);
            body_height += height + ROW_GAP;
            rows.push(Row { key, value, height });
        }
        if !rows.is_empty() {
            body_height += 2.0 * PAD_Y - ROW_GAP;
        }
        let head_height = 2.0 * PAD_Y + kind.height + name.height;
        let row_w = if !rows.is_empty() {
            key_width + COLUMN_GAP + value_width + 2.0 * PAD_X
        } else {
            0.0
        };
        let w = MIN_WIDTH.max(kind.width.max(name.width) + 2.0 * PAD_X).max(row_w);
        // A requirement and an element may share a name; relationships then go to the requirement.
        index.entry(node.id).or_insert(views.len());
        let mut c = cnode(w, head_height + body_height, -1);
        c.seq = views.len() as f64;
        views.push(NodeView {
            node,
            style: resolved,
            kind,
            name,
            rows,
            key_width,
            head_height,
            w,
            h: head_height + body_height,
        });
        cnodes.push(c);
    }

    let root_dir = if turned { Dir::TB } else { direction(Some(graph.direction)) };
    let sideways = matches!(root_dir, Dir::LR | Dir::RL);
    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    for edge in &graph.edges {
        let Some(&src) = edge.start.as_deref().and_then(|s| index.get(s)) else { continue };
        let Some(&dst) = edge.end.as_deref().and_then(|s| index.get(s)) else { continue };
        let label = layout_label(Some(&format!("«{}»", edge.rel_type)), false, measurer, small, 4000.0, 0);
        if src == dst {
            // A self-loop sits beside the node, across the flow, and the node's box grows to hold it.
            if sideways {
                cnodes[src].h += 2.0 * (LOOP + label.height + 8.0);
            } else {
                cnodes[src].w += 2.0 * (LOOP + label.width + 12.0);
            }
        }
        drawn.push(edge);
        cedges.push(cedge(src, dst, 1, label.width + 12.0, label.height + 4.0));
        edge_labels.push(label);
    }

    let layout = compound_layout(
        &mut cnodes,
        &mut cedges,
        root_dir,
        &LayeredOptions {
            node_sep: NODE_SEP,
            edge_sep: EDGE_SEP,
            rank_sep: RANK_SEP,
            port_sep: 20.0,
            ..Default::default()
        },
    );

    spread_ends(&cnodes, &mut cedges, &views);

    let title = title_label(model.title.as_deref(), measurer, size, 4000.0);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = layout.width.max(title.width);
    let ox = pad + (inner - layout.width) / 2.0;
    let oy = pad + title_height;
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();

    let mut nodes_out = String::new();
    for i in 0..views.len() {
        let view = &views[i];
        let (w, h) = (view.w, view.h);
        let st = &view.style;
        let color = ink_fill(&st.shape, &st.text, None);
        let muted = if color.is_empty() { " fill=\"var(--_m)\"" } else { color.as_str() };
        let left = -w / 2.0;
        let top = -h / 2.0;
        let mut body = format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>{}{}",
            num(left),
            num(top),
            num(w),
            num(h),
            rounded(&st.shape),
            label_svg(
                &view.kind,
                0.0,
                top + PAD_Y + view.kind.height / 2.0,
                &format!(" class=\"pele-stereotype\" font-size=\"{}\"{}{}", js_number(view.kind.size), muted, st.text),
            ),
            label_svg(
                &view.name,
                0.0,
                top + PAD_Y + view.kind.height + view.name.height / 2.0,
                &format!(
                    " class=\"pele-label\" font-size=\"{}\" font-weight=\"var(--_hw)\"{}{}",
                    js_number(view.name.size),
                    color,
                    st.text,
                ),
            ),
        );
        if !view.rows.is_empty() {
            let divider = top + view.head_height;
            body.push_str(&format!(
                "<path class=\"pele-divider\" d=\"M{},{}H{}\" stroke=\"var(--_b)\"{}/>",
                num(left),
                num(divider),
                num(-left),
                st.line,
            ));
            let mut y = divider + PAD_Y;
            let mut rows_svg = String::new();
            for row in &view.rows {
                rows_svg.push_str(&start_text(&row.key, left + PAD_X, y, &format!(" class=\"pele-field\"{}{}", muted, st.text)));
                rows_svg.push_str(&start_text(
                    &row.value,
                    left + PAD_X + view.key_width + COLUMN_GAP,
                    y,
                    &format!(" class=\"pele-value\"{}{}", color, st.text),
                ));
                y += row.height + ROW_GAP;
            }
            body.push_str(&format!("<g class=\"pele-fields\" font-size=\"{}\">{}</g>", js_number(view.rows[0].key.size), rows_svg));
        }
        nodes_out.push_str(&format!(
            "<g class=\"pele-node {}{}\" data-id=\"{}\" transform=\"translate({},{})\">{}</g>",
            if view.node.requirement.is_some() { "pele-requirement" } else { "pele-element" },
            class_names(strip_default_class(&view.node.css_classes)),
            esc_text(view.node.id),
            num(cnodes[i].x + ox),
            num(cnodes[i].y + oy),
            body,
        ));
    }

    let mut edges_out = String::new();
    let mut labels_out = String::new();
    let mut loop_seen: HashMap<usize, usize> = HashMap::new();
    for i in 0..cedges.len() {
        let ce = &cedges[i];
        let edge = drawn[i];
        let label = &edge_labels[i];
        let start_trim = if edge.contains { 2.0 * CONTAINS_RADIUS } else { 0.0 };
        let path: EdgePath;
        let mut label_x = ce.label_x + ox;
        let mut label_y = ce.label_y + oy;
        if ce.src == ce.dst {
            let view = &views[ce.src];
            let cx = cnodes[ce.src].x + ox;
            let cy = cnodes[ce.src].y + oy;
            let k = loop_seen.get(&ce.src).copied().unwrap_or(0);
            loop_seen.insert(ce.src, k + 1);
            let half = (if sideways { view.w } else { view.h }) / 2.0;
            let spread = (half - 4.0).min(12.0 + k as f64 * 6.0);
            let reach = LOOP + k as f64 * 8.0;
            // Local frame: `out` points away from the node, `along` runs along its side.
            let at = |out: f64, along: f64| -> String {
                if sideways {
                    format!("{},{}", num(cx + along), num(cy + view.h / 2.0 + out))
                } else {
                    format!("{},{}", num(cx + view.w / 2.0 + out), num(cy + along))
                }
            };
            // A cubic whose control points are this far out peaks at `reach`.
            let pull = reach / 0.75;
            path = EdgePath {
                d: format!(
                    "M{}C{} {} {}",
                    at(start_trim, -spread),
                    at(pull, -spread),
                    at(pull, spread),
                    at(0.0, spread),
                ),
                tokens: None,
                sx: if sideways { cx - spread } else { cx + view.w / 2.0 },
                sy: if sideways { cy + view.h / 2.0 } else { cy - spread },
                sdx: if sideways { 0.0 } else { -1.0 },
                sdy: if sideways { -1.0 } else { 0.0 },
                ex: if sideways { cx + spread } else { cx + view.w / 2.0 },
                ey: if sideways { cy + view.h / 2.0 } else { cy + spread },
                edx: if sideways { 0.0 } else { -1.0 },
                edy: if sideways { -1.0 } else { 0.0 },
            };
            if sideways {
                label_x = cx;
                label_y = cy + view.h / 2.0 + reach + 4.0 + ce.label_h / 2.0;
            } else {
                label_x = cx + view.w / 2.0 + reach + 4.0 + ce.label_w / 2.0;
                label_y = cy;
            }
        } else {
            let mut route = ce.route.clone();
            for p in route.chunks_exact_mut(3) {
                p[0] += ox;
                p[1] += oy;
            }
            path = route_path_default(&route, None, start_trim, 0.0);
        }
        let mark = if edge.contains {
            contains_marker(path.sx, path.sy, path.sdx, path.sdy)
        } else {
            arrow_marker(path.ex, path.ey, path.edx, path.edy)
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge pele-relationship-{}\" data-id=\"{}\"><path d=\"{}\"{}/>{}</g>",
            if edge.contains { "contains" } else { "arrow" },
            esc_text(&edge.id),
            path.d,
            if edge.contains { "" } else { " stroke-dasharray=\"5 4\"" },
            mark,
        ));
        labels_out.push_str(&edge_label_svg(&edge.id, label, label_x, label_y, ""));
    }

    let fixed = config.get("requirement").get("useMaxWidth").as_bool() == Some(false);
    let content = format!(
        "{}{}{}<g class=\"pele-nodes\">{}</g>",
        label_svg(&title, width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\""),
        if edges_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{}</g>", edges_out)
        },
        if labels_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-edge-labels\" font-size=\"{}\" fill=\"var(--_m)\">{}</g>", js_number(small), labels_out)
        },
        nodes_out,
    );

    let svg = svg_document(
        "requirement",
        width,
        height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive: options.responsive.unwrap_or(!fixed),
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: "",
        },
        &Accessible {
            acc_title: model.acc_title.as_deref(),
            acc_descr: model.acc_descr.as_deref(),
        },
        &content,
    );

    Rendered { svg, width, height, links: Vec::new() }
}
