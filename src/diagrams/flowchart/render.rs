use super::db::{FlowDb, LabelType};
use super::graph::{build_flow_graph, FlowGraph, GraphEdge, GraphNode};
use super::shapes::canonical_shape;
use crate::layout::compound::{cedge, cnode, compound_layout, direction, inherit_crossed_directions, shift_layout, CEdge, CNode, CompoundResult, Dir};
use crate::layout::layered::LayeredOptions;
use crate::options::{runs_across, tighten, title_room, turn_to_fit, LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::cluster::{cluster_title_x, mark_crossings, struck_title, title_crossed, Crossings};
use crate::svg::edges::{casings_svg, edge_casing, edge_ink, edge_label_svg, fill_under, loop_path, marker, marker_trim, route_path, EdgePath, Fill, SideReach, Under};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::shapes::{draw_shape, end_side, inset_route, shape_filled, shape_has_label, shape_size, shape_span, BoxAt};
use crate::svg::theme::{class_names, resolve_style, rounded, sets_fill, ResolvedStyle};
use crate::svg::title::title_label;
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::url::{image_url, internal_link, linked, rel_attr, safe_url};
use crate::value::Value;
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 48.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const SHAPE_ATTRS: &str = " fill=\"var(--_s)\" stroke=\"var(--_b)\"";
const LINE_ATTRS: &str = " stroke=\"var(--_b)\"";

fn number_option(section: &Value, key: &str, fallback: f64) -> f64 {
    match section.get(key).as_f64() {
        Some(v) if v > 0.0 => v.min(2000.0),
        _ => fallback,
    }
}

fn asset_size(value: Option<f64>, fallback: f64) -> f64 {
    match value {
        Some(v) if v > 0.0 && v <= 4000.0 => v,
        _ => fallback,
    }
}

struct NodeView<'a> {
    node: &'a GraphNode,
    style: ResolvedStyle,
    label: Label,
    shape: &'static str,
    w: f64,
    h: f64,
    dy: f64,
}

fn box_at(c: &CNode) -> BoxAt {
    BoxAt { x: c.x, y: c.y, w: c.w, h: c.h, is_group: c.is_group }
}

// A diagram type that is a flowchart with a different layout, which may draw its top-level groups itself.
pub struct FlowVariant {
    pub kind: &'static str,
    pub prepare: fn(&mut FlowGraph),
    pub layout: fn(&mut [CNode], &mut [CEdge], Dir, &LayeredOptions, bool) -> CompoundResult,
    pub draw_group: Option<fn(&CNode, &Label, &ResolvedStyle, &str, &str, Dir) -> String>,
    // The part of a draw_group group that takes its fill, as [left, top, right, bottom].
    pub filled_box: Option<fn(&CNode, Dir) -> [f64; 4]>,
}

pub fn render_flowchart(db: &FlowDb, config: &Value, options: &RenderOptions) -> Rendered {
    render_flowchart_variant(db, config, options, None)
}

pub fn render_flowchart_variant(db: &FlowDb, config: &Value, options: &RenderOptions, variant: Option<&FlowVariant>) -> Rendered {
    let fitted = |down: bool| tighten(options, &|tight| draw(db, config, options, variant, down, tight));
    if runs_across(db.direction.as_deref()) {
        turn_to_fit(options, &fitted)
    } else {
        fitted(false)
    }
}

// `turned` draws a flowchart that runs across as one that runs down. `tight` sets nodes closer together.
fn draw(db: &FlowDb, config: &Value, options: &RenderOptions, variant: Option<&FlowVariant>, turned: bool, tight: bool) -> Rendered {
    let group_pad = if tight { 12.0 } else { GROUP_PAD };
    let flow = match variant.map(|v| config.get(v.kind)).filter(|v| v.is_object()) {
        Some(own) => own,
        None => config.get("flowchart"),
    };
    let curve = flow.get("curve").as_str();
    let mut graph = build_flow_graph(db, curve);
    if let Some(v) = variant {
        (v.prepare)(&mut graph);
    }
    let size = options.font_size.unwrap_or(16.0);
    let edge_size = js_round(size * 0.875);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let wrap_width = number_option(flow, "wrappingWidth", 200.0).min(if variant.is_some() && tight { 100.0 } else { f64::INFINITY });
    let pad = options.padding.unwrap_or(8.0);

    let index: HashMap<&str, usize> = graph.nodes.iter().enumerate().map(|(i, node)| (node.id.as_str(), i)).collect();
    let mut views: Vec<NodeView> = Vec::new();
    let mut cnodes: Vec<CNode> = Vec::new();

    for node in &graph.nodes {
        let declarations: Vec<&str> = node.css_compiled_styles.iter().chain(&node.css_styles).map(String::as_str).collect();
        let resolved = resolve_style(&declarations);
        let text_style = (if resolved.bold { style::BOLD } else { 0 }) | (if resolved.italic { style::ITALIC } else { 0 });
        let markdown = node.label_type == LabelType::Markdown;
        let font_size = resolved.font_size.unwrap_or(size);
        let view;
        let mut c;
        if node.is_group {
            let group_max_w = if variant.is_some() && tight { (title_room(options) / 2.0).max(80.0) } else { 4000.0 };
            let label = layout_label(node.label.as_deref(), markdown, measurer, font_size, group_max_w, text_style);
            c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.dir = node.dir.as_deref().map(|d| direction(Some(d)));
            c.pad_x = group_pad;
            c.pad_top = if label.height > 0.0 { label.height + 16.0 } else { GROUP_PAD };
            c.pad_bottom = group_pad;
            c.min_w = label.width + 2.0 * group_pad - 12.0;
            view = NodeView { node, style: resolved, label, shape: "group", w: 0.0, h: 0.0, dy: 0.0 };
        } else {
            let shape = canonical_shape(Some(&node.shape));
            let asset = node.img.is_some() || node.icon.is_some();
            let text = if asset || shape_has_label(shape) { node.label.as_deref() } else { None };
            let label = layout_label(text, markdown, measurer, font_size, wrap_width, text_style);
            let (w, h, mut dy);
            if asset {
                let side = if node.img.is_some() { 80.0 } else { 48.0 };
                let aw = asset_size(node.asset_width, side);
                let ah = asset_size(node.asset_height, side);
                w = aw.max(label.width + 8.0);
                h = ah + if label.height > 0.0 { label.height + 6.0 } else { 0.0 };
                dy = if node.pos.as_deref() == Some("t") { -ah / 2.0 } else { ah / 2.0 };
                if label.height == 0.0 {
                    dy = 0.0;
                }
            } else {
                let s = shape_size(shape, label.width, label.height);
                w = s.w;
                h = s.h;
                dy = s.dy;
            }
            c = cnode(w, h, -1);
            view = NodeView { node, style: resolved, label, shape, w, h, dy };
        }
        c.seq = views.len() as f64;
        views.push(view);
        cnodes.push(c);
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node.parent_id.as_deref().and_then(|p| index.get(p)).map_or(-1, |&p| p as i32);
    }
    let at = |id: &str| index.get(id).map_or(-1, |&i| i as i32);
    let ends: Vec<(i32, i32)> = graph.edges.iter().map(|edge| (at(&edge.start), at(&edge.end))).collect();
    inherit_crossed_directions(&mut cnodes, &ends);

    let root_dir = if turned { Dir::TB } else { direction(db.direction.as_deref()) };
    let flows_sideways = |cnodes: &[CNode], i: usize| {
        let mut dir = None;
        let mut p = cnodes[i].parent;
        let mut hops = 0;
        while p >= 0 && dir.is_none() && hops < 10000 {
            dir = cnodes[p as usize].dir;
            p = cnodes[p as usize].parent;
            hops += 1;
        }
        matches!(dir.unwrap_or(root_dir), Dir::LR | Dir::RL)
    };
    // The edges on one side of a node spread along it; a picture or an icon keeps them at its middle.
    for i in 0..views.len() {
        let view = &views[i];
        if view.node.is_group {
            cnodes[i].span = f64::INFINITY;
        } else if view.node.img.is_none() && view.node.icon.is_none() {
            cnodes[i].span = shape_span(view.shape, view.w, view.h, flows_sideways(&cnodes, i));
        }
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    let mut loops = vec![0usize; views.len()];
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(edge.start.as_str()), index.get(edge.end.as_str())) else { continue };
        let label = layout_label(Some(&edge.label), edge.label_type == LabelType::Markdown, measurer, edge_size, wrap_width, 0);
        if src == dst {
            // A self-loop sits beside the node, across the flow, and the node's box grows to hold it.
            loops[src] += 1;
            if flows_sideways(&cnodes, src) {
                cnodes[src].h += 2.0 * (LOOP + if label.height > 0.0 { label.height + 8.0 } else { 0.0 });
            } else {
                cnodes[src].w += 2.0 * (LOOP + if label.width > 0.0 { label.width + 12.0 } else { 0.0 });
            }
        }
        drawn.push(edge);
        cedges.push(cedge(
            src,
            dst,
            edge.minlen.map_or(1, |l| l as i32),
            if label.width > 0.0 { label.width + 12.0 } else { 0.0 },
            if label.height > 0.0 { label.height + 4.0 } else { 0.0 },
        ));
        edge_labels.push(label);
    }

    let layout_options = LayeredOptions {
        node_sep: number_option(flow, "nodeSpacing", NODE_SEP) * if tight { 0.5 } else { 1.0 },
        edge_sep: EDGE_SEP,
        rank_sep: number_option(flow, "rankSpacing", RANK_SEP) * if tight { 0.75 } else { 1.0 },
        port_sep: 20.0,
        tracks: curve.is_some_and(|c| c.starts_with("step")),
        straight: curve == Some("linear"),
        head_room: marker_trim("arrow_point"),
        ..Default::default()
    };
    let layout = match variant {
        Some(v) => (v.layout)(&mut cnodes, &mut cedges, root_dir, &layout_options, tight),
        None => compound_layout(&mut cnodes, &mut cedges, root_dir, &layout_options),
    };

    let title = title_label(db.title.as_deref(), measurer, size, if tight { title_room(options) } else { 4000.0 });
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = layout.width.max(title.width);
    let ox = pad + (inner - layout.width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();
    let mut links: Vec<LinkInfo> = Vec::new();

    let mut crossings: Crossings = HashMap::new();
    if cnodes.iter().any(|c| c.is_group) {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
    }

    let mut clusters = String::new();
    let mut struck_titles = String::new();
    let mut nodes_out = String::new();
    let mut fills: Vec<Fill> = Vec::new();
    for i in 0..views.len() {
        let view = &views[i];
        let c = &cnodes[i];
        let node = view.node;
        let (x, y) = (c.x, c.y);
        let css = node.css_classes.strip_prefix("default").map_or(node.css_classes.as_str(), |rest| rest.strip_prefix(char::is_whitespace).unwrap_or(rest));
        let classes = class_names(css);
        let id = esc_text(&node.id);
        if c.is_group && c.parent < 0 {
            if let Some(draw_group) = variant.and_then(|v| v.draw_group) {
                if paints(&view.style.shape) {
                    if let Some(filled_box) = variant.and_then(|v| v.filled_box) {
                        fills.push(Fill { bounds: filled_box(c, root_dir), shape: view.style.shape.clone() });
                    }
                }
                clusters.push_str(&draw_group(c, &view.label, &view.style, &classes, &id, root_dir));
                continue;
            }
        }
        if c.is_group {
            if paints(&view.style.shape) {
                fills.push(Fill { bounds: [x - c.w / 2.0, y - c.h / 2.0, x + c.w / 2.0, y + c.h / 2.0], shape: view.style.shape.clone() });
            }
            let title_x = cluster_title_x(c, view.label.width, &crossings, group_pad);
            let title_y = y - c.h / 2.0 + 8.0 + view.label.height / 2.0;
            let struck = title_crossed(c, view.label.width, title_x, &crossings);
            // A title drawn over the edges sits on a halo in the theme's colors.
            let title_attrs = format!(
                " class=\"pele-cluster-label\"{}{}",
                if struck { " fill=\"var(--_m)\"".to_string() } else { ink_fill(&view.style.shape, &view.style.text, Some("var(--_m)")) },
                view.style.text
            );
            if struck {
                struck_titles.push_str(&struck_title(&view.label, title_x, title_y, &title_attrs, &id, &cnodes, i));
            }
            clusters.push_str(&format!(
                "<g class=\"pele-cluster{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\"{} stroke=\"var(--_b)\"{}/>{}</g>",
                classes,
                id,
                num(x - c.w / 2.0),
                num(y - c.h / 2.0),
                num(c.w),
                num(c.h),
                if sets_fill(&view.style.shape) { "" } else { " fill-opacity=\"0.5\"" },
                rounded(&view.style.shape),
                if struck { String::new() } else { label_svg(&view.label, title_x, title_y, &title_attrs) }
            ));
            continue;
        }

        let (w, h) = (view.w, view.h);
        let body_shape;
        let mut ink = String::new();
        if let Some(img) = &node.img {
            let aw = asset_size(node.asset_width, 80.0);
            let ah = asset_size(node.asset_height, 80.0);
            let top = if view.label.height > 0.0 { if node.pos.as_deref() == Some("t") { h / 2.0 - ah } else { -h / 2.0 } } else { -ah / 2.0 };
            let url = safe_url(img);
            body_shape = match image_url(&url, options) {
                Some(src) => format!(
                    "<image href=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"{}\"/>",
                    esc(src),
                    num(-aw / 2.0),
                    num(top),
                    num(aw),
                    num(ah),
                    if node.constraint.as_deref() == Some("on") { "xMidYMid meet" } else { "none" }
                ),
                None => String::new(),
            };
        } else if let Some(icon) = &node.icon {
            let side = asset_size(node.asset_height, 48.0);
            let top = if view.label.height > 0.0 { if node.pos.as_deref() == Some("t") { h / 2.0 - side } else { -h / 2.0 } } else { -side / 2.0 };
            let frame = match node.shape.as_str() {
                "icon" => String::new(),
                "iconCircle" => format!("<circle cy=\"{}\" r=\"{}\"{}{}/>", num(top + side / 2.0), num(side / 2.0), SHAPE_ATTRS, view.style.shape),
                shape => format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{}{}/>",
                    num(-side / 2.0),
                    num(top),
                    num(side),
                    num(side),
                    if shape == "iconRounded" { 8 } else { 0 },
                    SHAPE_ATTRS,
                    view.style.shape
                ),
            };
            let glyph = side * 0.6;
            // No icon set is bundled; the frame is drawn with room for one.
            body_shape = format!(
                "{}<svg class=\"pele-icon\" data-icon=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"></svg>",
                frame,
                esc_text(icon),
                num(-glyph / 2.0),
                num(top + (side - glyph) / 2.0),
                num(glyph),
                num(glyph)
            );
        } else {
            body_shape = draw_shape(view.shape, w, h, &format!("{}{}", SHAPE_ATTRS, view.style.shape), &format!("{}{}", LINE_ATTRS, view.style.line));
            if shape_filled(view.shape) {
                ink = ink_fill(&view.style.shape, &view.style.text, None);
            }
        }

        let mut text = label_svg(&view.label, 0.0, view.dy, &format!(" class=\"pele-label\"{}{}", ink, view.style.text));
        let has_internal = classes.split_whitespace().any(|c| c == "internal-link");
        if has_internal && options.links != Some(false) {
            if let Some(lbl) = node.label.as_deref().filter(|l| !l.is_empty()) {
                let name = decode_entities(lbl);
                let safe = internal_link(&name, options);
                let target = esc(&name);
                let href_attr = if safe { format!(" href=\"{}\"{}", target, rel_attr(options)) } else { String::new() };
                text = format!("<a class=\"internal-link\"{} data-href=\"{}\">{}</a>", href_attr, target, text);
                links.push(LinkInfo { id: node.id.clone(), href: name, internal: true });
            }
        }
        let mut body = body_shape + &text;
        if let Some(tooltip) = node.tooltip.as_deref().filter(|t| !t.is_empty()) {
            body = format!("<title>{}</title>{}", esc_text(tooltip), body);
        }
        if let Some(link) = node.link.as_deref().filter(|l| !l.is_empty()) {
            let target = node.link_target.as_deref().filter(|t| !t.is_empty()).map_or(String::new(), |t| format!(" target=\"{}\"", esc(t)));
            body = linked(body, link, &node.id, &mut links, options, &target);
        }
        nodes_out.push_str(&format!(
            "<g class=\"pele-node pele-shape-{}{}\" data-id=\"{}\" transform=\"translate({},{})\">{}</g>",
            view.shape,
            classes,
            id,
            num(x),
            num(y),
            body
        ));
    }

    let mut reach = SideReach::default();
    for (i, ce) in cedges.iter().enumerate() {
        if ce.src == ce.dst || drawn[i].thickness.as_deref() == Some("invisible") || ce.route.len() < 6 {
            continue;
        }
        reach.add(ce.src, end_side(&ce.route, 0, &box_at(&cnodes[ce.src])), &drawn[i].arrow_type_start);
        reach.add(ce.dst, end_side(&ce.route, ce.route.len() - 3, &box_at(&cnodes[ce.dst])), &drawn[i].arrow_type_end);
    }

    let mut casings = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();
    let mut loop_seen = vec![0usize; views.len()];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let resolved = if !edge.style.is_empty() || !edge.css_compiled_styles.is_empty() {
            let declarations: Vec<&str> = edge.css_compiled_styles.iter().chain(&edge.style).map(String::as_str).collect();
            Some(resolve_style(&declarations))
        } else {
            None
        };
        let line = resolved.as_ref().map_or("", |s| s.line.as_str());
        let invisible = edge.thickness.as_deref() == Some("invisible");
        if !invisible {
            let start_type = edge.arrow_type_start.as_str();
            let end_type = edge.arrow_type_end.as_str();
            let (src, dst) = (cedges[i].src, cedges[i].dst);
            let path: EdgePath;
            if src == dst {
                let view = &views[src];
                let k = loop_seen[src];
                loop_seen[src] += 1;
                let ce = &mut cedges[i];
                let (found, at) = loop_path(cnodes[src].x, cnodes[src].y, view.w, view.h, k as f64, flows_sideways(&cnodes, src), LOOP, marker_trim(end_type), ce.label_w, ce.label_h);
                ce.label_x = at.x;
                ce.label_y = at.y;
                path = found;
            } else {
                let mut route = cedges[i].route.clone();
                if route.len() < 6 {
                    continue;
                }
                let last = route.len() - 3;
                let (a, b) = (box_at(&cnodes[src]), box_at(&cnodes[dst]));
                let leaves = reach.of(src, end_side(&route, 0, &a));
                let arrives = reach.of(dst, end_side(&route, last, &b));
                inset_route(&mut route, 0, views[src].shape, views[src].w, views[src].h, &a);
                inset_route(&mut route, last, views[dst].shape, views[dst].w, views[dst].h, &b);
                path = route_path(&route, edge.curve.as_deref(), marker_trim(start_type), marker_trim(end_type), leaves, arrives);
            }
            // The ink of the innermost fill that holds the whole edge, '' for a casing where it
            // crosses a border, or nothing off fills.
            let over: Option<&str> = match fill_under(&path.d, &[path.sx, path.sy, path.ex, path.ey], &fills, path.tokens.as_deref()) {
                Under::None => None,
                Under::Border => Some(""),
                Under::Fill(fill) => Some(edge_ink(fill, line)),
            };
            let color = match over {
                Some(ink) if !ink.is_empty() => ink.to_string(),
                _ => resolved.as_ref().and_then(|s| s.stroke.as_deref()).filter(|s| !s.is_empty()).map_or("var(--_l)".to_string(), esc),
            };
            let attrs = match edge.thickness.as_deref() {
                Some("thick") => " stroke-width=\"2.5\"",
                Some("dotted") => " stroke-dasharray=\"3 4\"",
                _ => "",
            };
            let classes = class_names(&edge.classes.replace("edge-thickness-normal", "").replace("edge-pattern-solid", "").replace("flowchart-link", ""));
            if over == Some("") {
                casings.push_str(&edge_casing(&path, start_type, end_type, line, edge.thickness.as_deref() == Some("thick")));
            }
            edges_out.push_str(&format!(
                "<g class=\"pele-edge{}\" data-id=\"{}\"{}{}><path d=\"{}\"{}/>{}{}</g>",
                classes,
                esc_text(&edge.id),
                match over {
                    Some(ink) if !ink.is_empty() => format!(" stroke=\"{}\"", ink),
                    _ => String::new(),
                },
                line,
                path.d,
                attrs,
                marker(start_type, path.sx, path.sy, path.sdx, path.sdy, &color, "none"),
                marker(end_type, path.ex, path.ey, path.edx, path.edy, &color, "none")
            ));
        }
        if label.width > 0.0 {
            let text_attrs = resolved.as_ref().map_or("", |s| s.text.as_str());
            labels_out.push_str(&edge_label_svg(&edge.id, label, cedges[i].label_x, cedges[i].label_y, text_attrs));
        }
    }

    let mut content = label_svg(&title, width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\"");
    if !clusters.is_empty() {
        content.push_str(&format!("<g class=\"pele-clusters\">{}</g>", clusters));
    }
    content.push_str(&casings_svg(&casings));
    if !edges_out.is_empty() {
        content.push_str(&format!("<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{}</g>", edges_out));
    }
    if !struck_titles.is_empty() {
        content.push_str(&format!("<g class=\"pele-cluster-titles\">{}</g>", struck_titles));
    }
    if !labels_out.is_empty() {
        content.push_str(&format!("<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>", js_number(edge_size), labels_out));
    }
    content.push_str(&format!("<g class=\"pele-nodes\">{}</g>", nodes_out));

    let _ = loops;
    let fixed = flow.get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        variant.map_or("flowchart", |v| v.kind),
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
        &Accessible { acc_title: db.acc_title.as_deref(), acc_descr: db.acc_descr.as_deref() },
        &content,
    );
    Rendered { svg, width, height, links }
}
