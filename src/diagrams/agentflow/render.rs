use super::color_slots::{kind_slot, KIND_COUNT};
use super::db::AgentflowDb;
use super::graph::{build_agent_graph, AgentGraph, GraphEdge, GraphNode};
use super::types::LabelType;
use crate::diagrams::flowchart::shapes::canonical_shape;
use crate::errors::Result;
use crate::layout::compound::{
    cedge, cnode, compound_layout, direction, inherit_crossed_directions, shift_layout, CEdge,
    CNode, Dir,
};
use crate::layout::layered::LayeredOptions;
use crate::options::{runs_across, turn_to_fit, LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::cluster::{cluster_title_x, mark_crossings, Crossings};
use crate::svg::edges::{
    casings_svg, edge_casing, edge_label_svg, loop_path, marker, marker_trim, over_fills,
    route_path_default, EdgePath, Fill,
};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::shapes::{draw_shape, inset_route, shape_filled, shape_size, BoxAt, PAD_X, PAD_Y};
use crate::svg::theme::{class_names, resolve_style, rounded, series_color, sets_fill, ResolvedStyle};
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::url::{internal_link, linked, rel_attr};
use crate::value::Value;
use std::collections::HashMap;

// The number of series tokens, which is the palette Mermaid's slot rules are applied to.
pub const PALETTE: usize = 8;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 48.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const DOTS: f64 = 14.0;
const COLLAPSED: &str = "collapsed";

// The side a flow runs toward, with sides numbered clockwise from the top.
fn flow_end(dir: Dir) -> usize {
    match dir {
        Dir::TB => 2,
        Dir::RL => 3,
        Dir::BT => 0,
        Dir::LR => 1,
    }
}

// Builds the graph once per model. Building it also records the model's shape and containment diagnostics.
pub fn agent_graph<'a>(db: &'a mut AgentflowDb, config: &Value) -> Result<&'a AgentGraph> {
    if db.graph.is_none() {
        let curve = config.get("flowchart").get("curve").as_str();
        let graph = build_agent_graph(db, PALETTE, curve);
        db.graph = Some(graph);
    }
    Ok(db.graph.as_ref().unwrap())
}

fn number_option(config: &Value, key: &str, fallback: f64) -> f64 {
    match config.get("agentflow").get(key).as_f64() {
        Some(v) if v > 0.0 => v.min(2000.0),
        _ => fallback,
    }
}

// A palette color as an outline with a light wash of the same color inside.
// A fill the author set replaces the wash and stays opaque.
fn tint(slot: usize, wash: &str, style: &ResolvedStyle) -> String {
    let color = series_color(slot as i64);
    format!(
        " fill=\"{}\"{} stroke=\"{}\"",
        color,
        if sets_fill(&style.shape) {
            String::new()
        } else {
            format!(" fill-opacity=\"{}\"", wash)
        },
        color
    )
}

fn box_at(c: &CNode) -> BoxAt {
    BoxAt {
        x: c.x,
        y: c.y,
        w: c.w,
        h: c.h,
        is_group: c.is_group,
    }
}

fn dot(x: f64, y: f64) -> String {
    format!(
        "M{},{}a1.75,1.75 0 1 0 3.5,0a1.75,1.75 0 1 0 -3.5,0Z",
        num(x - 1.75),
        num(y)
    )
}

struct NodeView<'a> {
    node: &'a GraphNode,
    style: ResolvedStyle,
    label: Label,
    shape: &'static str,
    w: f64,
    h: f64,
    dy: f64,
    loops: usize,
}

struct HeldEdge<'a> {
    edge: &'a GraphEdge,
    label: Label,
    src: usize,
    dst: usize,
}

pub fn render_agentflow(db: &AgentflowDb, config: &Value, options: &RenderOptions) -> Rendered {
    let owned;
    let graph = match &db.graph {
        Some(g) => g,
        None => {
            let mut copy = db.clone();
            let curve = config.get("flowchart").get("curve").as_str();
            owned = build_agent_graph(&mut copy, PALETTE, curve);
            &owned
        }
    };
    if !runs_across(db.direction.as_deref()) {
        return draw(db, graph, config, options, false);
    }
    turn_to_fit(options, &|down| draw(db, graph, config, options, down))
}

// `turned` draws a diagram that runs across as one that runs down.
fn draw(
    db: &AgentflowDb,
    graph: &AgentGraph,
    config: &Value,
    options: &RenderOptions,
    turned: bool,
) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let edge_size = js_round(size * 0.875);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let wrap_width = number_option(config, "wrappingWidth", 120.0);
    let min_width = number_option(config, "minNodeWidth", 120.0);
    let pad = options.padding.unwrap_or(8.0);

    let mut index: HashMap<&str, usize> = HashMap::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(node.id.as_str(), i);
    }

    let mut views: Vec<NodeView<'_>> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());

    for node in &graph.nodes {
        let declarations: Vec<&str> = node
            .css_compiled_styles
            .iter()
            .chain(&node.css_styles)
            .map(String::as_str)
            .collect();
        let resolved = resolve_style(&declarations);
        let text_style =
            (if resolved.bold { style::BOLD } else { 0 }) | (if resolved.italic { style::ITALIC } else { 0 });
        let markdown = node.label_type == LabelType::Markdown;
        let font_size = resolved.font_size.unwrap_or(size);
        let view: NodeView<'_>;
        let mut c: CNode;
        if node.is_group {
            let label = layout_label(
                node.label.as_deref(),
                markdown,
                measurer,
                font_size,
                4000.0,
                text_style,
            );
            view = NodeView {
                node,
                style: resolved,
                label,
                shape: "group",
                w: 0.0,
                h: 0.0,
                dy: 0.0,
                loops: 0,
            };
            c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.dir = node
                .dir
                .as_deref()
                .filter(|d| !d.is_empty())
                .map(|d| direction(Some(d)));
            c.pad_x = GROUP_PAD;
            c.pad_top = if view.label.height > 0.0 {
                view.label.height + 16.0
            } else {
                GROUP_PAD
            };
            c.pad_bottom = GROUP_PAD;
            c.min_w = view.label.width + 2.0 * GROUP_PAD - 12.0;
        } else if node.shape == "collapsedGroup" {
            let label = layout_label(
                node.label.as_deref(),
                markdown,
                measurer,
                font_size,
                wrap_width,
                text_style,
            );
            let w = label.width.max(min_width) + 2.0 * PAD_X;
            let h = label.height + 2.0 * PAD_Y + DOTS;
            view = NodeView {
                node,
                style: resolved,
                label,
                shape: COLLAPSED,
                w,
                h,
                dy: -DOTS / 2.0,
                loops: 0,
            };
            c = cnode(w, h, -1);
        } else {
            let shape = canonical_shape(Some(&node.shape));
            let label = layout_label(
                node.label.as_deref(),
                markdown,
                measurer,
                font_size,
                wrap_width,
                text_style,
            );
            // A diamond grows twice as fast as its label, so it keeps the label's own width.
            let tw = if shape == "diam" {
                label.width
            } else {
                label.width.max(min_width)
            };
            let s = shape_size(shape, tw, label.height);
            view = NodeView {
                node,
                style: resolved,
                label,
                shape,
                w: s.w,
                h: s.h,
                dy: s.dy,
                loops: 0,
            };
            c = cnode(s.w, s.h, -1);
        }
        c.seq = views.len() as f64;
        views.push(view);
        cnodes.push(c);
    }

    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node
            .parent_id
            .as_deref()
            .and_then(|pid| index.get(pid))
            .map_or(-1, |&idx| idx as i32);
    }
    let at_id = |id: &str| index.get(id).map_or(-1, |&idx| idx as i32);
    let ends: Vec<(i32, i32)> = graph
        .edges
        .iter()
        .map(|edge| (at_id(&edge.start), at_id(&edge.end)))
        .collect();
    inherit_crossed_directions(&mut cnodes, &ends);

    let root_dir = if turned {
        Dir::TB
    } else {
        direction(db.direction.as_deref())
    };
    // The direction of the level a node sits in.
    let flow_of = |cnodes: &[CNode], i: usize| -> Dir {
        let mut dir: Option<Dir> = None;
        let mut p = cnodes[i].parent;
        let mut hops = 0;
        while p >= 0 && dir.is_none() && hops < 10000 {
            dir = cnodes[p as usize].dir;
            p = cnodes[p as usize].parent;
            hops += 1;
        }
        dir.unwrap_or(root_dir)
    };
    let flows_sideways = |cnodes: &[CNode], i: usize| -> bool {
        matches!(flow_of(cnodes, i), Dir::LR | Dir::RL)
    };

    let mut depth = vec![-1i32; cnodes.len()];
    for i in 0..cnodes.len() {
        let mut chain: Vec<usize> = Vec::new();
        let mut j = i as i32;
        while j >= 0 && depth[j as usize] < 0 && chain.len() <= cnodes.len() {
            chain.push(j as usize);
            j = cnodes[j as usize].parent;
        }
        let mut d = if j >= 0 { depth[j as usize] + 1 } else { 0 };
        for &node_idx in chain.iter().rev() {
            depth[node_idx] = d;
            d += 1;
        }
    }
    let contains = |cnodes: &[CNode], group: usize, i: usize| -> bool {
        let mut j = i as i32;
        let mut d = depth[i];
        while d > depth[group] && j >= 0 {
            j = cnodes[j as usize].parent;
            d -= 1;
        }
        j == group as i32 && i != group
    };

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    // Edges between a node and a container around it. The layout has no place for them, so they
    // are drawn afterwards, between the node and the container's border.
    let mut held: Vec<HeldEdge<'_>> = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        let label = layout_label(
            Some(&edge.label),
            edge.label_type == LabelType::Markdown,
            measurer,
            edge_size,
            wrap_width,
            0,
        );
        if contains(&cnodes, src, dst) || contains(&cnodes, dst, src) {
            held.push(HeldEdge {
                edge,
                label,
                src,
                dst,
            });
            continue;
        }
        if src == dst {
            // A self-loop sits beside the node, across the flow, and the node's box grows to hold it.
            views[src].loops += 1;
            if flows_sideways(&cnodes, src) {
                cnodes[src].h +=
                    2.0 * (LOOP + if label.height > 0.0 { label.height + 8.0 } else { 0.0 });
            } else {
                cnodes[src].w +=
                    2.0 * (LOOP + if label.width > 0.0 { label.width + 12.0 } else { 0.0 });
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

    let title = layout_label(db.title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = layout.width.max(title.width);
    let ox = pad + (inner - layout.width) / 2.0;
    let oy = pad + title_height;
    shift_layout(&mut cnodes, &mut cedges, ox, oy);
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();
    let mut links: Vec<LinkInfo> = Vec::new();

    // An edge between a node and a container around it runs straight between the node and the
    // container's border: out along the flow, or in against it. If another node is in the way it
    // takes a side instead, and failing that the far end.
    let mut held_paths: Vec<EdgePath> = Vec::with_capacity(held.len());
    let mut held_labels: Vec<f64> = Vec::with_capacity(held.len() * 2);
    let mut held_routes: Vec<[f64; 6]> = Vec::with_capacity(held.len());
    let careful = held.len().saturating_mul(cnodes.len()) < 2_000_000;
    for h in &held {
        let out = contains(&cnodes, h.dst, h.src);
        let outer = if out { h.dst } else { h.src };
        let i = if out { h.src } else { h.dst };
        let g = &cnodes[outer];
        let n = &cnodes[i];
        let end = flow_end(g.dir.unwrap_or_else(|| flow_of(&cnodes, outer)));
        let first = if out { end } else { (end + 2) & 3 };
        let toward = |side: usize| -> [f64; 6] {
            let upright = (side & 1) == 0;
            let sgn = if side == 0 || side == 3 { -1.0 } else { 1.0 };
            let nx = if upright { n.x } else { n.x + (sgn * n.w) / 2.0 };
            let ny = if upright { n.y + (sgn * n.h) / 2.0 } else { n.y };
            let bx = if upright { n.x } else { g.x + (sgn * g.w) / 2.0 };
            let by = if upright { g.y + (sgn * g.h) / 2.0 } else { n.y };
            let axis = if upright { 0.0 } else { 1.0 };
            if out {
                [nx, ny, axis, bx, by, axis]
            } else {
                [bx, by, axis, nx, ny, axis]
            }
        };
        let blocked = |r: &[f64; 6]| -> bool {
            let min_x = r[0].min(r[3]) - 1.0;
            let max_x = r[0].max(r[3]) + 1.0;
            let min_y = r[1].min(r[4]) - 1.0;
            let max_y = r[1].max(r[4]) + 1.0;
            for (k, c) in cnodes.iter().enumerate() {
                if k != i
                    && !c.is_group
                    && c.x + c.w / 2.0 > min_x
                    && c.x - c.w / 2.0 < max_x
                    && c.y + c.h / 2.0 > min_y
                    && c.y - c.h / 2.0 < max_y
                {
                    return true;
                }
            }
            false
        };
        let mut route = toward(first);
        if careful && blocked(&route) {
            for side in [(first + 1) & 3, (first + 3) & 3, (first + 2) & 3] {
                let other = toward(side);
                if !blocked(&other) {
                    route = other;
                    break;
                }
            }
        }
        held_routes.push(route);
        held_labels.push((route[0] + route[3]) / 2.0);
        held_labels.push((route[1] + route[4]) / 2.0);
        let mut drawn_route = route;
        inset_route(
            &mut drawn_route,
            if out { 0 } else { 3 },
            views[i].shape,
            views[i].w,
            views[i].h,
            &box_at(n),
        );
        held_paths.push(route_path_default(
            &drawn_route,
            Some("linear"),
            0.0,
            marker_trim(&h.edge.arrow_type_end),
        ));
    }

    let mut crossings: Crossings = HashMap::new();
    let any_group = cnodes.iter().any(|c| c.is_group);
    if any_group {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
        for route in &held_routes {
            mark_crossings(&mut crossings, route);
        }
    }

    let mut clusters = String::new();
    let mut nodes_out = String::new();
    let mut fills: Vec<Fill> = Vec::new();
    for i in 0..views.len() {
        let view = &views[i];
        let c = &cnodes[i];
        let node = view.node;
        let x = c.x;
        let y = c.y;
        let raw_css = match node.css_classes.strip_prefix("default") {
            Some(rest) => rest.strip_prefix(crate::chars::is_space).unwrap_or(rest),
            None => node.css_classes.as_str(),
        };
        let classes = class_names(raw_css);
        let id = esc_text(&node.id);
        if c.is_group {
            if paints(&view.style.shape) {
                fills.push(Fill {
                    bounds: [x - c.w / 2.0, y - c.h / 2.0, x + c.w / 2.0, y + c.h / 2.0],
                    shape: view.style.shape.clone(),
                });
            }
            let title_x = cluster_title_x(c, view.label.width, &crossings, GROUP_PAD);
            let rect_attrs = rounded(&format!(
                "{}{}",
                tint(node.color_index.unwrap_or(KIND_COUNT), "0.06", &view.style),
                view.style.shape
            ));
            let label_attrs = format!(
                " class=\"pele-cluster-label\"{}{}",
                ink_fill(&view.style.shape, &view.style.text, Some("var(--_m)")),
                view.style.text
            );
            clusters.push_str(&format!(
                "<g class=\"pele-cluster pele-flow{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>{}</g>",
                classes,
                id,
                num(x - c.w / 2.0),
                num(y - c.h / 2.0),
                num(c.w),
                num(c.h),
                rect_attrs,
                label_svg(&view.label, title_x, y - c.h / 2.0 + 8.0 + view.label.height / 2.0, &label_attrs)
            ));
            continue;
        }

        let w = view.w;
        let h = view.h;
        let mut kind = String::new();
        let shape_svg = if view.shape == COLLAPSED {
            let slot = node.color_index.unwrap_or(KIND_COUNT);
            let color = series_color(slot as i64);
            let line = h / 2.0 - DOTS - 2.0;
            let dots = line + (DOTS + 2.0) / 2.0;
            let rect_attrs = rounded(&format!(
                "{}{}",
                tint(slot, "0.06", &view.style),
                view.style.shape
            ));
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/><path d=\"M{},{}H{}\" fill=\"none\" stroke=\"{}\" stroke-dasharray=\"3 3\"{}/><path class=\"pele-collapsed-mark\" d=\"{}{}{}\" fill=\"{}\" stroke=\"none\"/>",
                num(-w / 2.0),
                num(-h / 2.0),
                num(w),
                num(h),
                rect_attrs,
                num(-w / 2.0 + 10.0),
                num(line),
                num(w / 2.0 - 10.0),
                color,
                view.style.line,
                dot(-9.0, dots),
                dot(0.0, dots),
                dot(9.0, dots),
                color
            )
        } else {
            let node_kind = node.kind.as_deref().unwrap_or("task");
            let slot = kind_slot(node_kind).unwrap_or(1);
            kind = format!(" pele-kind-{}", node_kind);
            draw_shape(
                view.shape,
                w,
                h,
                &format!("{}{}", tint(slot, "0.14", &view.style), view.style.shape),
                &format!(" stroke=\"{}\"{}", series_color(slot as i64), view.style.line),
            )
        };

        let ink = if view.shape == COLLAPSED || shape_filled(view.shape) {
            ink_fill(&view.style.shape, &view.style.text, None)
        } else {
            String::new()
        };
        let mut text = label_svg(
            &view.label,
            0.0,
            view.dy,
            &format!(" class=\"pele-label\"{}{}", ink, view.style.text),
        );
        let has_internal = classes.split_whitespace().any(|cls| cls == "internal-link");
        if has_internal && options.links != Some(false) {
            if let Some(lbl) = node.label.as_deref().filter(|l| !l.is_empty()) {
                // The label is a note name, but it still ends up in an href, so it gets the same check as a URL.
                let name = decode_entities(lbl);
                let safe = internal_link(&name, options);
                let target = esc(&name);
                let href_attr = if safe {
                    format!(" href=\"{}\"{}", target, rel_attr(options))
                } else {
                    String::new()
                };
                text = format!(
                    "<a class=\"internal-link\"{} data-href=\"{}\">{}</a>",
                    href_attr, target, text
                );
                links.push(LinkInfo {
                    id: node.id.clone(),
                    href: name,
                    internal: true,
                });
            }
        }
        let mut body = format!("{}{}", shape_svg, text);
        if let Some(tooltip) = node.tooltip.as_deref().filter(|t| !t.is_empty()) {
            body = format!("<title>{}</title>{}", esc_text(tooltip), body);
        }
        if let Some(link) = node.link.as_deref().filter(|l| !l.is_empty()) {
            let target_attr = node
                .link_target
                .as_deref()
                .filter(|t| !t.is_empty())
                .map_or(String::new(), |t| format!(" target=\"{}\"", esc(t)));
            body = linked(body, link, &node.id, &mut links, options, &target_attr);
        }
        nodes_out.push_str(&format!(
            "<g class=\"pele-node pele-shape-{}{}{}\" data-id=\"{}\" transform=\"translate({},{})\">{}</g>",
            view.shape,
            kind,
            classes,
            id,
            num(x),
            num(y),
            body
        ));
    }

    let mut casings = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();

    let mut put = |edge: &GraphEdge, label: &Label, path: &EdgePath, x: f64, y: f64| {
        let resolved = if !edge.style.is_empty() || !edge.css_compiled_styles.is_empty() {
            let declarations: Vec<&str> = edge
                .css_compiled_styles
                .iter()
                .chain(&edge.style)
                .map(String::as_str)
                .collect();
            Some(resolve_style(&declarations))
        } else {
            None
        };
        let line = resolved.as_ref().map_or("", |s| s.line.as_str());
        let end_type = edge.arrow_type_end.as_str();
        let over = over_fills(&path.d, &[path.ex, path.ey], &fills, line, path.tokens.as_deref());
        let color = match over {
            Some(ink) if !ink.is_empty() => ink.to_string(),
            _ => resolved
                .as_ref()
                .and_then(|s| s.stroke.as_deref())
                .filter(|s| !s.is_empty())
                .map_or_else(|| "var(--_l)".to_string(), esc),
        };
        let kind = if edge.thickness.as_deref() == Some("dotted") {
            "reference"
        } else if end_type == "arrow_cross" {
            "failure"
        } else {
            "sequence"
        };
        let stripped_classes = edge
            .classes
            .replace("edge-thickness-normal", "")
            .replace("edge-pattern-solid", "")
            .replace("flowchart-link", "");
        let classes = class_names(&stripped_classes);
        if over == Some("") {
            casings.push_str(&edge_casing(path, "none", end_type, line, false));
        }
        let over_stroke = match over {
            Some(ink) if !ink.is_empty() => format!(" stroke=\"{}\"", ink),
            _ => String::new(),
        };
        let dash = if edge.thickness.as_deref() == Some("dotted") {
            " stroke-dasharray=\"3 4\""
        } else {
            ""
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge pele-edge-{}{}\" data-id=\"{}\"{}{}><path d=\"{}\"{}/>{}</g>",
            kind,
            classes,
            esc_text(&edge.id),
            over_stroke,
            line,
            path.d,
            dash,
            marker(end_type, path.ex, path.ey, path.edx, path.edy, &color, "none")
        ));
        if label.width > 0.0 {
            let text_attrs = resolved.as_ref().map_or("", |s| s.text.as_str());
            labels_out.push_str(&edge_label_svg(&edge.id, label, x, y, text_attrs));
        }
    };

    let mut loop_seen = vec![0usize; views.len()];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let end_type = edge.arrow_type_end.as_str();
        let src = cedges[i].src;
        let dst = cedges[i].dst;
        let path: EdgePath;
        let label_x: f64;
        let label_y: f64;
        if src == dst {
            let view = &views[src];
            let k = loop_seen[src];
            loop_seen[src] += 1;
            let ce = &mut cedges[i];
            let (found, at) = loop_path(
                cnodes[src].x,
                cnodes[src].y,
                view.w,
                view.h,
                k as f64,
                flows_sideways(&cnodes, src),
                LOOP,
                marker_trim(end_type),
                ce.label_w,
                ce.label_h,
            );
            ce.label_x = at.x;
            ce.label_y = at.y;
            label_x = at.x;
            label_y = at.y;
            path = found;
        } else {
            let mut route = cedges[i].route.clone();
            if route.len() < 6 {
                continue;
            }
            let last = route.len() - 3;
            inset_route(
                &mut route,
                0,
                views[src].shape,
                views[src].w,
                views[src].h,
                &box_at(&cnodes[src]),
            );
            inset_route(
                &mut route,
                last,
                views[dst].shape,
                views[dst].w,
                views[dst].h,
                &box_at(&cnodes[dst]),
            );
            path = route_path_default(&route, edge.curve.as_deref(), 0.0, marker_trim(end_type));
            label_x = cedges[i].label_x;
            label_y = cedges[i].label_y;
        }
        put(edge, label, &path, label_x, label_y);
    }
    for (i, h) in held.iter().enumerate() {
        put(
            h.edge,
            &h.label,
            &held_paths[i],
            held_labels[2 * i],
            held_labels[2 * i + 1],
        );
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
            js_number(edge_size),
            labels_out
        ));
    }
    content.push_str(&format!("<g class=\"pele-nodes\">{}</g>", nodes_out));

    let _ = views.first().map(|v| v.loops);
    let fixed = config.get("agentflow").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "agentflow",
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
            acc_title: db.acc_title.as_deref(),
            acc_descr: db.acc_descr.as_deref(),
        },
        &content,
    );

    Rendered {
        svg,
        width,
        height,
        links,
    }
}
