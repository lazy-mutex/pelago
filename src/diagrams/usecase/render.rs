use super::graph::{
    build_usecase_graph, EdgePattern, EdgeRelationType, NodeShape, UsecaseGraphEdge,
    UsecaseGraphNode,
};
use super::types::{ActorType, BoundaryType, LabelType, OrdMap, UsecaseModel};
use crate::diagrams::common::icon::icon_svg;
use crate::layout::compound::{
    cnode, compound_layout, direction, shift_layout, CEdge, CNode, Dir,
};
use crate::layout::layered::LayeredOptions;
use crate::options::{runs_across, turn_to_fit, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::cluster::{mark_crossings, Crossings};
use crate::svg::edges::{
    casing, casings_svg, edge_ink, edge_label_svg, fill_under, hollow_on, marker, marker_trim,
    route_path_default, EdgePath, Fill, Under,
};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{class_names, resolve_style, rounded, sets_fill, ResolvedStyle};
use crate::svg::title::title_label;
use crate::text::label::{label_line, layout_label, line_middle, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;
use std::cmp::Ordering;
use std::collections::HashMap;

const NODE_SEP: f64 = 32.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 72.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const FIGURE_W: f64 = 40.0;
const FIGURE_H: f64 = 60.0;
const FIGURE_GAP: f64 = 6.0;
const TAB_PAD: f64 = 6.0;
const CELL_PAD: f64 = 10.0;
const ROW_PAD: f64 = 8.0;
const FOLD: f64 = 9.0;
const EXTENSION: f64 = 12.0;
const PORT_GAP: f64 = 14.0;
const SHAPE_ATTRS: &str = " fill=\"var(--_s)\" stroke=\"var(--_b)\"";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Actor,
    Ellipse,
    Box,
    Json,
    Boundary,
}

struct View<'a> {
    node: &'a UsecaseGraphNode,
    kind: Kind,
    style: ResolvedStyle,
    label: Label,
    stereotype: Label,
    // The size of what is drawn; the layout box is larger when the node carries a self-relationship.
    w: f64,
    h: f64,
    rows: Vec<(Label, Label)>,
    key_w: f64,
    head_h: f64,
    #[allow(dead_code)]
    loops: usize,
}

fn number_option(config: &Value, key: &str, fallback: f64) -> f64 {
    match config.get("usecase").get(key).as_f64() {
        Some(v) if v > 0.0 => v.min(2000.0),
        _ => fallback,
    }
}

// A plain label is shown as written: no tags, no markdown, no `\n` breaks. Its markup characters
// are handed to the label parser as character references, which it turns back into characters.
fn literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&#38;"),
            '<' => out.push_str("&#60;"),
            '>' => out.push_str("&#62;"),
            '\\' => out.push_str("&#92;"),
            _ => out.push(c),
        }
    }
    out
}

fn text_label(
    raw: Option<&str>,
    markdown: bool,
    measurer: &dyn TextMeasurer,
    size: f64,
    wrap: f64,
    base: u8,
) -> Label {
    match raw {
        Some(s) if !s.is_empty() && !markdown => {
            let lit = literal(s);
            layout_label(Some(&lit), false, measurer, size, wrap, base)
        }
        other => layout_label(other, markdown, measurer, size, wrap, base),
    }
}

// The point of a node's outline on one of its four sides (0 top, 1 right, 2 bottom, 3 left),
// `along` away from the middle of that side. Lines meet an actor at its figure, not at its label.
fn anchor(view: &View, c: &CNode, side: u8, along: f64) -> (f64, f64) {
    let sign = if side == 1 || side == 2 { 1.0 } else { -1.0 };
    let upright = side == 1 || side == 3;
    let mut reach = (if upright { view.w } else { view.h }) / 2.0;
    let mut middle = 0.0;
    if view.kind == Kind::Actor {
        if upright {
            reach = FIGURE_W / 2.0 + 3.0;
            middle = (FIGURE_H - view.h) / 2.0;
        } else {
            reach += 2.0;
        }
    } else if view.kind == Kind::Ellipse {
        let span = (if upright { view.h } else { view.w }) / 2.0;
        let ratio = if span == 0.0 { 0.0 } else { along / span };
        reach *= (1.0 - ratio * ratio).max(0.0).sqrt();
    }
    if upright {
        (c.x + sign * reach, c.y + middle + along)
    } else {
        (c.x + along, c.y + sign * reach)
    }
}

// How much of a side edges may spread over.
fn side_length(view: &View, side: u8) -> f64 {
    let upright = side == 1 || side == 3;
    if view.kind == Kind::Actor {
        return if upright {
            FIGURE_H * 0.5
        } else {
            FIGURE_W * 0.6
        };
    }
    let length = if upright { view.h } else { view.w };
    if view.kind == Kind::Ellipse {
        length * 0.6
    } else {
        length - 16.0
    }
}

struct End {
    edge_idx: usize,
    at: usize,
    toward: f64,
}

// The layout brings every edge that ends on one side of a node to the middle of that side. This
// spaces them along the side in the order they fan out, and puts each on the node's outline.
fn attach_ends(cedges: &mut [CEdge], cnodes: &[CNode], views: &[View]) {
    let mut sides: OrdMap<(usize, u8), Vec<End>> = OrdMap::new();
    let mut add = |node_idx: usize, edge_idx: usize, route: &[f64], at: usize, next: usize| {
        let c = &cnodes[node_idx];
        if c.is_group {
            return;
        }
        let dx = route[at] - c.x;
        let dy = route[at + 1] - c.y;
        let side: u8 = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
            if dy < 0.0 {
                0
            } else {
                2
            }
        } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
            if dx < 0.0 {
                3
            } else {
                1
            }
        } else {
            return;
        };
        let toward = route[next + if (side & 1) != 0 { 1 } else { 0 }];
        let end = End {
            edge_idx,
            at,
            toward,
        };
        if let Some(list) = sides.get_mut(&(node_idx, side)) {
            list.push(end);
        } else {
            sides.insert((node_idx, side), vec![end]);
        }
    };

    for (edge_idx, e) in cedges.iter().enumerate() {
        if e.src == e.dst || e.route.len() < 6 {
            continue;
        }
        let last = e.route.len() - 3;
        add(e.src, edge_idx, &e.route, 0, 3);
        add(e.dst, edge_idx, &e.route, last, last - 3);
    }

    for &((node_idx, side), ref ends_unsorted) in sides.iter() {
        let mut ends: Vec<&End> = ends_unsorted.iter().collect();
        let view = &views[node_idx];
        let step = if ends.len() > 1 {
            PORT_GAP.min(side_length(view, side) / (ends.len() - 1) as f64)
        } else {
            0.0
        };
        ends.sort_by(|a, b| a.toward.partial_cmp(&b.toward).unwrap_or(Ordering::Equal));
        let mid = (ends.len() - 1) as f64 / 2.0;
        for (i, end) in ends.into_iter().enumerate() {
            let (x, y) = anchor(view, &cnodes[node_idx], side, (i as f64 - mid) * step);
            cedges[end.edge_idx].route[end.at] = x;
            cedges[end.edge_idx].route[end.at + 1] = y;
        }
    }
}

// The hollow triangle of a generalization, with its tip at (x, y).
fn extension(x: f64, y: f64, dx: f64, dy: f64, color: &str) -> String {
    let bx = x - dx * EXTENSION;
    let by = y - dy * EXTENSION;
    let half = EXTENSION * 0.5;
    format!(
        "<path class=\"pele-marker\" d=\"M{},{}L{},{}L{},{}Z\" fill=\"var(--_bg)\" stroke=\"{}\" stroke-linejoin=\"round\"/>",
        num(x),
        num(y),
        num(bx - dy * half),
        num(by + dx * half),
        num(bx + dy * half),
        num(by - dx * half),
        color
    )
}

fn trim(kind: &str) -> f64 {
    if kind == "extension" {
        EXTENSION
    } else {
        marker_trim(kind)
    }
}

fn mark(kind: &str, x: f64, y: f64, dx: f64, dy: f64, color: &str, stroke: &str) -> String {
    if kind == "extension" {
        extension(x, y, dx, dy, color)
    } else {
        marker(kind, x, y, dx, dy, color, stroke)
    }
}

fn figure(view: &View) -> String {
    let node = view.node;
    let shape = &view.style.shape;
    let line = format!(" fill=\"none\" stroke=\"var(--_b)\"{}", view.style.line);
    let mut head = -21.0;
    let mut radius = 9.0;
    let mut out = match node.actor_type {
        Some(ActorType::Hollow) => {
            radius = 8.0;
            format!(
                "<circle cy=\"-21\" r=\"8\" fill=\"var(--_bg)\" stroke=\"var(--_b)\"{shape}/><path d=\"M-20,-9H20V0H5.5L20,15.5L12,25.5L0,12L-12,25.5L-20,15.5L-5.5,0H-20Z\" fill=\"var(--_bg)\" stroke=\"var(--_b)\" stroke-linejoin=\"round\"{shape}/>"
            )
        }
        Some(ActorType::Awesome) => {
            format!(
                "<path d=\"M0,-30A11,11 0 1 1 0,-8A11,11 0 1 1 0,-30ZM-20,25C-20,9 -12,-3 0,-3C12,-3 20,9 20,25C20,28 18,30 15,30H-15C-18,30 -20,28 -20,25Z\" fill=\"var(--_b)\"{shape}/>"
            )
        }
        Some(ActorType::Icon) => {
            head = -14.0;
            radius = 12.0;
            let rect_attrs = rounded(&format!("{}{}", SHAPE_ATTRS, shape));
            format!(
                "<rect x=\"-22\" y=\"-26\" width=\"44\" height=\"44\"{}/>{}",
                rect_attrs,
                icon_svg(node.icon.as_deref().unwrap_or(""), -14.0, -18.0, 28.0)
            )
        }
        _ => {
            format!(
                "<circle cy=\"-21\" r=\"9\"{SHAPE_ATTRS}{shape}/><path d=\"M0,-12V10M-14,-3H14M0,10L-12,30M0,10L12,30\"{line}/>"
            )
        }
    };
    if node.business == Some(true) {
        // The slash of a business actor: a chord across the lower right of the head.
        out.push_str(&format!(
            "<path class=\"pele-business\" d=\"M{},{}L{},{}\"{line}/>",
            num(0.12 * radius),
            num(head + 0.99 * radius),
            num(0.92 * radius),
            num(head - 0.39 * radius)
        ));
    }
    out
}

fn strip_default_node_classes(raw: &str) -> &str {
    raw.strip_prefix("default ")
        .or_else(|| raw.strip_prefix("default"))
        .unwrap_or(raw)
}

fn strip_default_edge_classes(raw: &str) -> &str {
    let Some(rest) = raw.strip_prefix("default relationship relationship-") else {
        return raw;
    };
    let after_word = match rest.find(char::is_whitespace) {
        Some(idx) => &rest[idx..],
        None => "",
    };
    after_word
        .strip_prefix(|c: char| c.is_whitespace())
        .unwrap_or(after_word)
}

pub fn render_usecase(
    model: &UsecaseModel,
    config: &Value,
    options: &RenderOptions,
) -> Rendered {
    if !runs_across(Some(model.direction.as_str())) {
        return draw(model, config, options, false);
    }
    turn_to_fit(options, &|down| draw(model, config, options, down))
}

// `turned` draws a diagram that runs across as one that runs down.
fn draw(
    model: &UsecaseModel,
    config: &Value,
    options: &RenderOptions,
    turned: bool,
) -> Rendered {
    let graph = build_usecase_graph(model);
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let tiny = js_round(size * 0.8125);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let root_dir = if turned {
        Dir::TB
    } else {
        direction(Some(model.direction.as_str()))
    };
    let sideways = matches!(root_dir, Dir::LR | Dir::RL);

    let mut index: HashMap<String, usize> = HashMap::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(node.id.clone(), i);
    }

    let mut views: Vec<View> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());

    for node in &graph.nodes {
        let style = if !node.css_styles.is_empty() {
            let mut combined = node.css_compiled_styles.clone();
            combined.extend(node.css_styles.iter().cloned());
            resolve_style(&combined)
        } else {
            resolve_style(&node.css_compiled_styles)
        };
        let base = (if style.bold { style::BOLD } else { 0 })
            | (if style.italic { style::ITALIC } else { 0 });
        let font_size = style.font_size.unwrap_or(size);
        let markdown = node.label_type == LabelType::Markdown;
        let stereo_raw = node
            .stereotype
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| format!("«{}»", s));
        let stereotype = text_label(stereo_raw.as_deref(), false, measurer, tiny, 200.0, base);
        let mut view = View {
            node,
            kind: Kind::Box,
            style,
            label: stereotype.clone(),
            stereotype,
            w: 0.0,
            h: 0.0,
            rows: Vec::new(),
            key_w: 0.0,
            head_h: 0.0,
            loops: 0,
        };
        let mut c: CNode;
        if node.is_group {
            view.kind = Kind::Boundary;
            view.label = text_label(Some(&node.label), markdown, measurer, font_size, 4000.0, base);
            let title_h = view.label.height;
            c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.pad_x = GROUP_PAD;
            c.pad_bottom = GROUP_PAD;
            c.pad_top = if node.boundary_type == Some(BoundaryType::Package) {
                title_h + 2.0 * TAB_PAD + 16.0
            } else if title_h > 0.0 {
                title_h + 16.0
            } else {
                GROUP_PAD
            };
            c.min_w = view.label.width + 2.0 * GROUP_PAD;
        } else {
            if let Some(ref json_rows) = node.json_rows {
                view.kind = Kind::Json;
                view.label = text_label(
                    Some(&node.label),
                    false,
                    measurer,
                    font_size,
                    4000.0,
                    base | style::BOLD,
                );
                let mut value_w: f64 = 0.0;
                for row in json_rows {
                    let key = text_label(Some(&row.key), false, measurer, small, 4000.0, base);
                    let value = text_label(Some(&row.value), false, measurer, small, 260.0, base);
                    view.key_w = view.key_w.max(key.width);
                    value_w = value_w.max(value.width);
                    view.h += key
                        .height
                        .max(value.height)
                        .max(js_round(small * 1.5))
                        + ROW_PAD;
                    view.rows.push((key, value));
                }
                // The root of an empty object has a value and no key, and then there is no key column.
                if view.key_w > 0.0 {
                    view.key_w += 2.0 * CELL_PAD;
                }
                view.head_h = view.label.height.max(js_round(font_size * 1.5)) + ROW_PAD;
                let rows_w = if !view.rows.is_empty() {
                    view.key_w + value_w + 2.0 * CELL_PAD
                } else {
                    0.0
                };
                view.w = (view.label.width + 2.0 * CELL_PAD).max(rows_w).max(72.0);
                view.h += view.head_h;
            } else if node.actor_type.is_some() {
                view.kind = Kind::Actor;
                view.label =
                    text_label(Some(&node.label), markdown, measurer, font_size, 140.0, base);
                view.w = FIGURE_W.max(view.label.width).max(view.stereotype.width);
                view.h = FIGURE_H + FIGURE_GAP + view.stereotype.height + view.label.height;
            } else {
                let lbl_size = if node.shape == NodeShape::Note {
                    small
                } else {
                    font_size
                };
                view.label =
                    text_label(Some(&node.label), markdown, measurer, lbl_size, 180.0, base);
                let tw = view.label.width.max(view.stereotype.width);
                let th = view.label.height + view.stereotype.height;
                if matches!(
                    node.shape,
                    NodeShape::UsecaseEllipse | NodeShape::UsecaseBusiness
                ) {
                    view.kind = Kind::Ellipse;
                    // An oval needs more room around its text than a box, and a business oval room for its slash.
                    let extra = if node.business == Some(true) { 16.0 } else { 4.0 };
                    view.w = (tw + 2.0 * node.padding + extra).max(88.0);
                    view.h = (th + 28.0).max(48.0);
                } else if node.shape == NodeShape::Note {
                    view.w = tw + 2.0 * node.padding + 6.0;
                    view.h = th + 18.0;
                } else {
                    view.w = (tw + 2.0 * node.padding + 12.0).max(80.0);
                    view.h = (th + 24.0).max(44.0);
                }
            }
            c = cnode(view.w, view.h, -1);
        }
        c.seq = views.len() as f64;
        views.push(view);
        cnodes.push(c);
    }

    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node
            .parent_id
            .as_deref()
            .and_then(|pid| index.get(pid).copied())
            .map_or(-1, |idx| idx as i32);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&UsecaseGraphEdge> = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(&edge.start), index.get(&edge.end)) else {
            continue;
        };
        let semantic = matches!(
            edge.relationship_type,
            EdgeRelationType::Include | EdgeRelationType::Extend
        );
        let semantic_text = if semantic {
            edge.label.as_deref().map(|l| format!("«{}»", l))
        } else {
            None
        };
        let raw_lbl = if semantic {
            semantic_text.as_deref()
        } else {
            edge.label.as_deref()
        };
        let label = text_label(
            raw_lbl,
            edge.label_type == Some(LabelType::Markdown),
            measurer,
            small,
            200.0,
            0,
        );
        if src == dst {
            // A self-relationship loops out across the flow, and the node's box grows to hold it.
            views[src].loops += 1;
            if sideways {
                cnodes[src].h +=
                    2.0 * (LOOP + if label.height > 0.0 { label.height + 8.0 } else { 0.0 });
            } else {
                cnodes[src].w +=
                    2.0 * (LOOP + if label.width > 0.0 { label.width + 12.0 } else { 0.0 });
            }
        }
        drawn.push(edge);
        cedges.push(CEdge {
            src,
            dst,
            minlen: edge.minlen as i32,
            label_w: if label.width > 0.0 {
                label.width + 12.0
            } else {
                0.0
            },
            label_h: if label.height > 0.0 {
                label.height + 4.0
            } else {
                0.0
            },
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        });
        edge_labels.push(label);
    }

    let layout = compound_layout(
        &mut cnodes,
        &mut cedges,
        root_dir,
        &LayeredOptions {
            node_sep: number_option(config, "nodeSpacing", NODE_SEP),
            edge_sep: EDGE_SEP,
            rank_sep: number_option(config, "rankSpacing", RANK_SEP),
            port_sep: 20.0,
            ..Default::default()
        },
    );

    let title = title_label(model.title.as_deref(), measurer, size, 4000.0);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = layout.width.max(title.width);
    let ox = pad + (inner - layout.width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();

    // Where edges cross each boundary's top border, so that a title can keep out of their way.
    let mut crossings: Crossings = HashMap::new();
    for e in &cedges {
        mark_crossings(&mut crossings, &e.route);
    }
    attach_ends(&mut cedges, &cnodes, &views);

    let mut clusters = String::new();
    let mut fills: Vec<Fill> = Vec::new();
    let mut nodes_out = String::new();

    for i in 0..views.len() {
        let view = &views[i];
        let c = &cnodes[i];
        let node = view.node;
        let classes = class_names(strip_default_node_classes(&node.css_classes));
        let id = esc_text(&node.id);
        let text_attrs = &view.style.text;
        let shape = &view.style.shape;
        let color = ink_fill(shape, text_attrs, None);
        let muted = if color.is_empty() {
            " fill=\"var(--_m)\""
        } else {
            color.as_str()
        };
        let w = view.w;
        let h = view.h;

        if view.kind == Kind::Boundary {
            let x = c.x - c.w / 2.0;
            let y = c.y - c.h / 2.0;
            if paints(shape) {
                fills.push(Fill {
                    bounds: [x, y, x + c.w, y + c.h],
                    shape: shape.clone(),
                });
            }
            let box_attrs = format!(
                " fill=\"var(--_a)\"{} stroke=\"var(--_b)\"{}",
                if sets_fill(shape) {
                    ""
                } else {
                    " fill-opacity=\"0.5\""
                },
                shape
            );
            let title_attrs = format!(" class=\"pele-cluster-label\"{}{}", muted, text_attrs);
            let body = if node.boundary_type == Some(BoundaryType::Package) {
                let tab_w = (view.label.width + 2.0 * TAB_PAD + 8.0).min(c.w);
                let tab_h = view.label.height + 2.0 * TAB_PAD;
                format!(
                    "<rect class=\"pele-boundary-tab\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{box_attrs}/><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{box_attrs}/>{}",
                    num(x),
                    num(y),
                    num(tab_w),
                    num(tab_h),
                    num(x),
                    num(y + tab_h),
                    num(c.w),
                    num(c.h - tab_h),
                    label_svg(&view.label, x + tab_w / 2.0, y + tab_h / 2.0, &title_attrs)
                )
            } else {
                format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>{}",
                    num(x),
                    num(y),
                    num(c.w),
                    num(c.h),
                    rounded(&box_attrs),
                    label_svg(
                        &view.label,
                        title_x(c, view.label.width, &crossings),
                        y + 8.0 + view.label.height / 2.0,
                        &title_attrs
                    )
                )
            };
            clusters.push_str(&format!(
                "<g class=\"pele-cluster pele-boundary{}\" data-id=\"{}\">{}</g>",
                classes, id, body
            ));
            continue;
        }

        let (kind_cls, body) = if view.kind == Kind::Actor {
            let top = -h / 2.0;
            let stereotype_y = top + FIGURE_H + FIGURE_GAP + view.stereotype.height / 2.0;
            let b = format!(
                "<g class=\"pele-actor-figure\" transform=\"translate(0,{})\">{}</g>{}{}",
                num(top + FIGURE_H / 2.0),
                figure(view),
                label_svg(
                    &view.stereotype,
                    0.0,
                    stereotype_y,
                    &format!(" class=\"pele-stereotype\" font-size=\"{}\" fill=\"var(--_m)\"", js_number(tiny))
                ),
                label_svg(
                    &view.label,
                    0.0,
                    h / 2.0 - view.label.height / 2.0,
                    &format!(" class=\"pele-label\"{}", text_attrs)
                )
            );
            ("pele-actor", b)
        } else if view.kind == Kind::Json {
            let x0 = -w / 2.0;
            let mut y = -h / 2.0 + view.head_h;
            let mut cells = String::new();
            let mut lines = format!("M{},{}H{}", num(x0), num(y), num(x0 + w));
            if view.key_w > 0.0 {
                lines.push_str(&format!("M{},{}V{}", num(x0 + view.key_w), num(y), num(h / 2.0)));
            }
            for (key, value) in &view.rows {
                let row_h = key
                    .height
                    .max(value.height)
                    .max(js_round(small * 1.5))
                    + ROW_PAD;
                cells.push_str(&label_svg(
                    key,
                    x0 + CELL_PAD + key.width / 2.0,
                    y + row_h / 2.0,
                    &format!(" class=\"pele-json-key\"{}", muted),
                ));
                cells.push_str(&left_label(
                    value,
                    x0 + view.key_w + CELL_PAD,
                    y + row_h / 2.0,
                    &format!(" class=\"pele-json-value\"{}{}", color, text_attrs),
                ));
                y += row_h;
            }
            let b = format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/><path class=\"pele-json-lines\" d=\"{}\" fill=\"none\" stroke=\"var(--_b)\"{}/>{}<g font-size=\"{}\">{}</g>",
                num(x0),
                num(-h / 2.0),
                num(w),
                num(h),
                rounded(&format!("{}{}", SHAPE_ATTRS, shape)),
                lines,
                view.style.line,
                label_svg(
                    &view.label,
                    0.0,
                    -h / 2.0 + view.head_h / 2.0,
                    &format!(" class=\"pele-label\" font-weight=\"var(--_hw)\"{}{}", color, text_attrs)
                ),
                js_number(small),
                cells
            );
            ("pele-json", b)
        } else {
            let label_y = view.stereotype.height / 2.0;
            let (k_cls, outline) = if view.kind == Kind::Ellipse {
                let mut out = format!(
                    "<ellipse rx=\"{}\" ry=\"{}\"{SHAPE_ATTRS}{shape}/>",
                    num(w / 2.0),
                    num(h / 2.0)
                );
                if node.business == Some(true) {
                    // The slash of a business use case, cutting off the right end of the oval beside the text.
                    let from = w / 2.0 - 15.0;
                    let to = w / 2.0 - 5.0;
                    let arc = |at: f64| -> f64 {
                        let ratio = at / (w / 2.0);
                        (h / 2.0) * (1.0 - ratio * ratio).max(0.0).sqrt()
                    };
                    out.push_str(&format!(
                        "<path class=\"pele-business\" d=\"M{},{}L{},{}\" fill=\"none\" stroke=\"var(--_b)\"{}/>",
                        num(from),
                        num(arc(from)),
                        num(to),
                        num(-arc(to)),
                        view.style.line
                    ));
                }
                ("pele-usecase", out)
            } else if node.shape == NodeShape::Note {
                let r = w / 2.0;
                let b_half = h / 2.0;
                let out = format!(
                    "<path d=\"M{},{}H{}L{},{}V{}H{}Z\" fill=\"var(--_a)\" stroke=\"var(--_b)\" stroke-linejoin=\"round\"{}\
                    /><path d=\"M{},{}V{}H{}\" fill=\"none\" stroke=\"var(--_b)\" stroke-linejoin=\"round\"{}/>",
                    num(-r),
                    num(-b_half),
                    num(r - FOLD),
                    num(r),
                    num(FOLD - b_half),
                    num(b_half),
                    num(-r),
                    shape,
                    num(r - FOLD),
                    num(-b_half),
                    num(FOLD - b_half),
                    num(r),
                    view.style.line
                );
                ("pele-note", out)
            } else {
                let out = format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
                    num(-w / 2.0),
                    num(-h / 2.0),
                    num(w),
                    num(h),
                    rounded(&format!("{}{}", SHAPE_ATTRS, shape))
                );
                ("pele-usecase", out)
            };
            let note_font = if node.shape == NodeShape::Note {
                format!(" font-size=\"{}\"", js_number(small))
            } else {
                String::new()
            };
            let b = format!(
                "{}{}{}",
                outline,
                label_svg(
                    &view.stereotype,
                    0.0,
                    -view.label.height / 2.0,
                    &format!(" class=\"pele-stereotype\" font-size=\"{}\"{}", js_number(tiny), muted)
                ),
                label_svg(
                    &view.label,
                    0.0,
                    label_y,
                    &format!(" class=\"pele-label\"{}{}{}", note_font, color, text_attrs)
                )
            );
            (k_cls, b)
        };
        nodes_out.push_str(&format!(
            "<g class=\"pele-node {}{}\" data-id=\"{}\" transform=\"translate({},{})\">{}</g>",
            kind_cls,
            classes,
            id,
            num(c.x),
            num(c.y),
            body
        ));
    }

    let mut casings = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();
    let mut loop_seen: HashMap<usize, usize> = HashMap::new();

    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let edge_style = if !edge.style.is_empty() || !edge.css_compiled_styles.is_empty() {
            let mut combined = edge.css_compiled_styles.clone();
            combined.extend(edge.style.iter().cloned());
            Some(resolve_style(&combined))
        } else {
            None
        };
        let start_type = edge.arrow_type_start;
        let end_type = edge.arrow_type_end;

        let path: EdgePath = if cedges[i].src == cedges[i].dst {
            let src_idx = cedges[i].src;
            let c = &cnodes[src_idx];
            let view = &views[src_idx];
            let k = loop_seen.get(&src_idx).copied().unwrap_or(0);
            loop_seen.insert(src_idx, k + 1);
            let side: u8 = if sideways { 2 } else { 1 };
            let spread = (side_length(view, side) / 2.0).min(8.0 + k as f64 * 6.0);
            let reach = LOOP + k as f64 * 8.0;
            let (ax, ay) = anchor(view, c, side, -spread);
            let (bx, by) = anchor(view, c, side, spread);
            let hypot = reach.hypot(spread);
            let len = if hypot == 0.0 { 1.0 } else { hypot };
            let ux = if sideways { spread / len } else { reach / len };
            let uy = if sideways { reach / len } else { spread / len };
            let st = trim(start_type);
            let et = trim(end_type);
            // Both ends leave the node leaning away from each other, and the curve joins them outside it.
            let out_pt = |x: f64, y: f64, sign: f64, far: f64| -> String {
                let factor = if far != 0.0 { 1.0 } else { 0.0 };
                if sideways {
                    format!("{},{}", num(x + sign * spread * factor), num(y + far))
                } else {
                    format!("{},{}", num(x + far), num(y + sign * spread * factor))
                }
            };
            let sx = if sideways { ax - ux * st } else { ax + ux * st };
            let sy = if sideways { ay + uy * st } else { ay - uy * st };
            let ex = bx + ux * et;
            let ey = by + uy * et;
            if sideways {
                cedges[i].label_x = c.x;
                cedges[i].label_y = ay.max(by) + reach * 0.75 + 6.0 + label.height / 2.0;
            } else {
                cedges[i].label_x = ax.max(bx) + reach * 0.75 + 8.0 + label.width / 2.0;
                cedges[i].label_y = c.y;
            }
            EdgePath {
                d: format!(
                    "M{},{}C{} {} {},{}",
                    num(sx),
                    num(sy),
                    out_pt(ax, ay, -1.0, reach),
                    out_pt(bx, by, 1.0, reach),
                    num(ex),
                    num(ey)
                ),
                tokens: None,
                sx: ax,
                sy: ay,
                sdx: if sideways { ux } else { -ux },
                sdy: if sideways { -uy } else { uy },
                ex: bx,
                ey: by,
                edx: -ux,
                edy: -uy,
            }
        } else {
            if cedges[i].route.len() < 6 {
                continue;
            }
            route_path_default(&cedges[i].route, None, trim(start_type), trim(end_type))
        };

        let style_line = edge_style.as_ref().map_or("", |s| s.line.as_str());
        let (under_fill, over): (Option<&Fill>, Option<&str>) = match fill_under(
            &path.d,
            &[path.sx, path.sy, path.ex, path.ey],
            &fills,
            path.tokens.as_deref(),
        ) {
            Under::None => (None, None),
            Under::Border => (None, Some("")),
            Under::Fill(fill) => (Some(fill), Some(edge_ink(fill, style_line))),
        };
        let color = match over {
            Some(ink) if !ink.is_empty() => ink.to_string(),
            _ => edge_style
                .as_ref()
                .and_then(|s| s.stroke.as_deref())
                .filter(|s| !s.is_empty())
                .map_or_else(|| "var(--_l)".to_string(), esc),
        };
        if over == Some("") {
            let bg = "var(--_bg)";
            let bg_marks = mark(start_type, path.sx, path.sy, path.sdx, path.sdy, bg, bg)
                + &mark(end_type, path.ex, path.ey, path.edx, path.edy, bg, bg);
            casings.push_str(&casing(&path.d, &bg_marks, style_line, false));
        }
        // Mermaid animates these dashes. A still picture keeps the dashes.
        let dashes = if edge.internal {
            "2 3"
        } else if edge.pattern == EdgePattern::Dotted {
            "5 4"
        } else if edge.animate {
            "9 5"
        } else {
            ""
        };
        let marks = mark(
            start_type, path.sx, path.sy, path.sdx, path.sdy, &color, "none",
        ) + &mark(end_type, path.ex, path.ey, path.edx, path.edy, &color, "none");
        let rendered_marks = match (under_fill, over) {
            (Some(fill), Some(ink)) if !ink.is_empty() => hollow_on(&marks, fill),
            _ => marks,
        };
        let stroke_attr = match over {
            Some(ink) if !ink.is_empty() => format!(" stroke=\"{}\"", ink),
            _ => String::new(),
        };
        let dash_attr = if dashes.is_empty() {
            String::new()
        } else {
            format!(" stroke-dasharray=\"{}\"", dashes)
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge pele-{}{}\" data-id=\"{}\"{}{}><path d=\"{}\"{}/>{}</g>",
            edge.relationship_type.as_str(),
            class_names(strip_default_edge_classes(&edge.classes)),
            esc_text(&edge.id),
            stroke_attr,
            style_line,
            path.d,
            dash_attr,
            rendered_marks
        ));
        if label.width > 0.0 {
            let semantic = matches!(
                edge.relationship_type,
                EdgeRelationType::Include | EdgeRelationType::Extend
            );
            let lbl_attrs = format!(
                "{}{}",
                if semantic { " fill=\"var(--_m)\"" } else { "" },
                edge_style.as_ref().map_or("", |s| s.text.as_str())
            );
            labels_out.push_str(&edge_label_svg(
                &edge.id,
                label,
                cedges[i].label_x,
                cedges[i].label_y,
                &lbl_attrs,
            ));
        }
    }

    let mut content = label_svg(
        &title,
        width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    );
    if !clusters.is_empty() {
        content.push_str(&format!("<g class=\"pele-clusters\">{}</g>", clusters));
    }
    content.push_str(&casings_svg(&casings));
    if !edges_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{}</g>",
            edges_out
        ));
    }
    if !labels_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>",
            js_number(small),
            labels_out
        ));
    }
    content.push_str(&format!("<g class=\"pele-nodes\">{}</g>", nodes_out));

    let fixed = config.get("usecase").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "usecase",
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
            acc_title: Some(model.acc_title.as_str()).filter(|s| !s.is_empty()),
            acc_descr: Some(model.acc_descr.as_str()).filter(|s| !s.is_empty()),
        },
        &content,
    );

    Rendered {
        svg,
        width,
        height,
        links: Vec::new(),
    }
}

// A boundary's title sits in the middle of its top edge. Where edges enter through that edge it
// moves into the stretch between them that is nearest the middle, or the widest if none has room.
fn title_x(c: &CNode, width: f64, crossings: &Crossings) -> f64 {
    let Some(xs) = crossings.get(&(js_round(c.y - c.h / 2.0) as i64)) else {
        return c.x;
    };
    if width == 0.0 {
        return c.x;
    }
    let left = c.x - c.w / 2.0 + 6.0;
    let right = c.x + c.w / 2.0 - 6.0;
    let mut cuts: Vec<f64> = xs
        .iter()
        .copied()
        .filter(|&x| x > left && x < right)
        .collect();
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    cuts.push(right);
    let need = width / 2.0 + 6.0;
    let mut best = c.x;
    let mut best_off = f64::INFINITY;
    let mut widest = 0.0;
    let mut from = left;
    for cut in cuts {
        if cut - from >= 2.0 * need {
            let x = c.x.max(from + need).min(cut - need);
            if (x - c.x).abs() < best_off {
                best_off = (x - c.x).abs();
                best = x;
            }
        } else if best_off == f64::INFINITY && cut - from > widest {
            widest = cut - from;
            best = (from + cut) / 2.0;
        }
        from = cut;
    }
    best
}

fn left_label(label: &Label, x: f64, cy: f64, attrs: &str) -> String {
    if label.lines.len() < 2 {
        return label_svg(label, x + label.width / 2.0, cy, attrs);
    }
    let mut out = String::new();
    for i in 0..label.lines.len() {
        out.push_str(&label_svg(
            &label_line(label, i),
            x + label.widths[i] / 2.0,
            cy - label.height / 2.0 + line_middle(label, i),
            attrs,
        ));
    }
    out
}
