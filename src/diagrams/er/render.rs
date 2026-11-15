use super::db::ErDb;
use super::graph::{build_er_graph, ErGraphEdge, ErGraphNode};
use super::markers::{er_marker, MARKER_LENGTH};
use crate::chars::is_space;
use crate::diagrams::common::generics::parse_generic_types;
use crate::layout::compound::{cedge, cnode, compound_layout, direction, shift_layout, CEdge, CNode, Dir};
use crate::layout::layered::LayeredOptions;
use crate::options::{runs_across, turn_to_fit, LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num, shared_style, sign, span_style, text_style};
use crate::svg::cluster::{cluster_title_x, mark_crossings, Crossings};
use crate::svg::edges::{casing, casings_svg, edge_ink, edge_label_svg, fill_under, hollow_on, route_path_default, Fill, Under};
use crate::svg::ink::{ink, ink_fill, paints, ON_DARK, ON_LIGHT};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{class_names, resolve_style, rounded, sets_fill, ResolvedStyle, ROUNDED};
use crate::svg::title::title_label;
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, line_middle, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::url::{internal_link, rel_attr};
use crate::value::Value;
use std::collections::HashMap;

const NODE_SEP: f64 = 48.0;
const EDGE_SEP: f64 = 20.0;
const RANK_SEP: f64 = 64.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 28.0;
const LOOP_BAND: f64 = 38.0;
const MIN_WIDTH: f64 = 96.0;
const BOX_PAD_X: f64 = 18.0;
const BOX_PAD_Y: f64 = 12.0;
const HEAD_PAD_Y: f64 = 9.0;
const CELL_PAD_X: f64 = 12.0;
const CELL_GAP: f64 = 16.0;
const ROW_PAD: f64 = 9.0;
const COMMENT_WRAP: f64 = 260.0;
const NO_WRAP: f64 = 1e6;
const PORT_GAP: f64 = 18.0;
const PORT_MARGIN: f64 = 14.0;
const BOX_ATTRS: &str = " fill=\"var(--_s)\" stroke=\"var(--_b)\"";

fn number_option(config: &Value, key: &str, fallback: f64) -> f64 {
    match config.get("er").get(key).as_f64() {
        Some(v) if v > 0.0 => v.min(2000.0),
        _ => fallback,
    }
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

// Type, name, keys, comment.
type Cells = [Label; 4];

struct Row {
    cells: Cells,
    h: f64,
}

struct NodeView<'a> {
    node: &'a ErGraphNode<'a>,
    style: ResolvedStyle,
    label: Label,
    w: f64,
    h: f64,
    head_h: f64,
    rows: Vec<Row>,
    // Left edge of each column, measured from the left edge of the box.
    cols: Vec<f64>,
    row_size: f64,
    key_size: f64,
    mono: u8,
    // Whether self-relationships loop out of the bottom side rather than the right.
    below: bool,
}

// A relationship from an entity to itself, with the stretch of the side it loops out of.
struct Loop {
    label: Label,
    band: f64,
    at: f64,
}

struct End {
    edge_idx: usize,
    at: usize,
    toward: f64,
}

// Writes a measured label starting at x, with the baseline of its first line at y.
fn text_left(label: &Label, x: f64, mut y: f64, attrs: &str, mono: u8) -> String {
    let lines = &label.lines;
    if lines.is_empty() {
        return String::new();
    }
    if lines.len() == 1 && lines[0].len() == 1 {
        let only = &lines[0][0];
        // The short form collapses repeated spaces in monospace text.
        if only.style == mono && !(mono != 0 && only.text.contains("  ")) {
            return format!("<text{} x=\"{}\" y=\"{}\">{}</text>", text_style(attrs, mono), num(x), num(y), esc(&only.text));
        }
    }
    let shared = shared_style(label);
    let shift = label.size * 0.35;
    let top = y - shift - label.line_height / 2.0;
    let mut body = String::new();
    for i in 0..lines.len() {
        let line = &lines[i];
        let middle = top + line_middle(label, i);
        y = middle + shift;
        let mut sx = x;
        let mut placed = false;
        for span in line {
            if !span.text.is_empty() {
                let pos = if !placed { format!(" x=\"{}\" y=\"{}\"", num(sx), num(y)) } else { String::new() };
                body.push_str(&format!("<tspan{}{}>{}</tspan>", pos, span_style(span.style & !shared), esc(&span.text)));
                placed = true;
            }
            sx += span.width;
        }
    }
    format!("<text{} xml:space=\"preserve\">{}</text>", text_style(attrs, shared), body)
}

// Mermaid reads `~` pairs as angle brackets. They are written as entities so the label parser
// does not take `<int>` for a tag.
fn cell_text(raw: &str) -> String {
    let converted = parse_generic_types(raw);
    if converted == raw {
        converted
    } else {
        converted.replace('<', "&lt;").replace('>', "&gt;")
    }
}

pub fn render_er(db: &ErDb, config: &Value, options: &RenderOptions) -> Rendered {
    if !runs_across(Some(&db.direction)) {
        return draw(db, config, options, false);
    }
    turn_to_fit(options, &|down| draw(db, config, options, down))
}

// `turned` draws a diagram that runs across as one that runs down.
fn draw(db: &ErDb, config: &Value, options: &RenderOptions, turned: bool) -> Rendered {
    let graph = build_er_graph(db);
    let size = options.font_size.unwrap_or(16.0);
    let small_size = js_round(size * 0.875);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    let mut index: HashMap<&str, usize> = HashMap::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(node.id, i);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&ErGraphEdge> = Vec::new();
    let mut all_loops: Vec<Loop> = Vec::new();
    let mut node_loops: Vec<Vec<usize>> = vec![Vec::new(); graph.nodes.len()];
    let mut loop_of: Vec<Option<usize>> = Vec::new();
    for edge in &graph.edges {
        let Some(&src) = index.get(edge.start.as_str()) else { continue };
        let Some(&dst) = index.get(edge.end.as_str()) else { continue };
        let label = layout_label(Some(&edge.label), true, measurer, small_size, 200.0, 0);
        let loop_idx = if src == dst {
            let li = all_loops.len();
            all_loops.push(Loop { label: label.clone(), band: 0.0, at: 0.0 });
            node_loops[src].push(li);
            Some(li)
        } else {
            None
        };
        loop_of.push(loop_idx);
        drawn.push(edge);
        let lw = if label.width > 0.0 { label.width + 12.0 } else { 0.0 };
        let lh = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
        cedges.push(cedge(src, dst, 1, lw, lh));
        edge_labels.push(label);
    }

    // The direction of the level a node is laid out in: its nearest subgraph that sets one, or the diagram's.
    let horizontal = |start_idx: usize| -> bool {
        let mut at = start_idx;
        // Subgraphs can name each other as members, so the walk up is bounded.
        for _ in 0..graph.nodes.len() {
            let Some(parent) = graph.nodes[at].parent_id.as_deref() else { break };
            let Some(&up) = index.get(parent) else { break };
            if let Some(dir) = graph.nodes[up].dir.filter(|d| !d.is_empty()) {
                return dir == "LR" || dir == "RL";
            }
            at = up;
        }
        db.direction == "LR" || db.direction == "RL"
    };

    let mut views: Vec<NodeView<'_>> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        let resolved = if !node.css_styles.is_empty() {
            let mut combined = Vec::with_capacity(node.css_compiled_styles.len() + node.css_styles.len());
            combined.extend(node.css_compiled_styles.iter().map(String::as_str));
            combined.extend(node.css_styles.iter().map(String::as_str));
            resolve_style(&combined)
        } else {
            resolve_style(&node.css_compiled_styles)
        };
        let base = (if resolved.bold { style::BOLD } else { 0 }) | (if resolved.italic { style::ITALIC } else { 0 });
        let mono = 0u8;
        let name_size = resolved.font_size.unwrap_or(size);
        let row_size = resolved.font_size.unwrap_or(small_size);
        let key_size = resolved.font_size.unwrap_or_else(|| js_round(size * 0.75));
        let label = if node.is_group {
            layout_label(Some(node.label), true, measurer, name_size, 4000.0, base)
        } else {
            let display = if node.alias.is_empty() { node.label } else { node.alias };
            layout_label(Some(display), true, measurer, name_size, 240.0, base)
        };
        let own = &node_loops[i];
        // Loops leave by a side the layout keeps free of other edges: the right when ranks run down,
        // the bottom when they run across.
        let below = !own.is_empty() && !node.is_group && horizontal(i);
        let mut view = NodeView {
            node,
            style: resolved,
            label,
            w: 0.0,
            h: 0.0,
            head_h: 0.0,
            rows: Vec::new(),
            cols: Vec::new(),
            row_size,
            key_size,
            mono,
            below,
        };
        let mut reach = 0.0f64;
        let mut bands = 0.0f64;
        for &li in own {
            let lp = &mut all_loops[li];
            lp.band = LOOP_BAND.max(if below { lp.label.width + 12.0 } else { lp.label.height + 6.0 });
            bands += lp.band;
            reach = reach.max(if below { lp.label.height } else { lp.label.width });
        }
        let min_w = MIN_WIDTH.max(if below { bands + 8.0 } else { 0.0 });
        let mut c: CNode;
        if node.is_group {
            c = cnode(0.0, 0.0, -1);
            c.is_group = true;
            c.dir = node.dir.filter(|d| !d.is_empty()).map(|d| direction(Some(d)));
            c.pad_x = GROUP_PAD;
            c.pad_top = if view.label.height > 0.0 { view.label.height + 16.0 } else { GROUP_PAD };
            c.pad_bottom = GROUP_PAD;
            c.min_w = (view.label.width + 2.0 * GROUP_PAD - 12.0).max(MIN_WIDTH);
        } else {
            let name_h = view.label.height.max(js_round(name_size * 1.5));
            if node.attributes.is_empty() {
                view.w = (view.label.width + 2.0 * BOX_PAD_X).max(min_w);
                view.h = (name_h + 2.0 * BOX_PAD_Y).max(if below { 0.0 } else { bands + 8.0 });
            } else {
                let row_line = js_round(row_size * 1.5);
                let mut widths = [0.0f64; 4];
                for attribute in node.attributes {
                    let type_str = cell_text(&attribute.attr_type);
                    let name_str = cell_text(&attribute.name);
                    let keys_str = attribute.keys.join(", ");
                    let comment_str = cell_text(&attribute.comment);
                    let cells: Cells = [
                        layout_label(Some(&type_str), true, measurer, row_size, NO_WRAP, base | mono),
                        layout_label(Some(&name_str), true, measurer, row_size, NO_WRAP, base | mono),
                        layout_label(Some(&keys_str), false, measurer, key_size, NO_WRAP, base | mono),
                        layout_label(Some(&comment_str), true, measurer, row_size, COMMENT_WRAP, base),
                    ];
                    let mut h = row_line;
                    for k in 0..4 {
                        if cells[k].width > widths[k] {
                            widths[k] = cells[k].width;
                        }
                        if cells[k].height > h {
                            h = cells[k].height;
                        }
                    }
                    view.rows.push(Row { cells, h: h + ROW_PAD });
                    view.h += h + ROW_PAD;
                }
                let mut columns = 0usize;
                let mut inner = 0.0f64;
                for &w in &widths {
                    if w == 0.0 {
                        continue;
                    }
                    inner += w + if columns > 0 { CELL_GAP } else { 0.0 };
                    columns += 1;
                }
                view.w = (inner + 2.0 * CELL_PAD_X)
                    .max(view.label.width + 2.0 * BOX_PAD_X)
                    .max(min_w);
                // A box wider than its table shares the spare width between the columns.
                let spare = if columns > 0 {
                    (view.w - inner - 2.0 * CELL_PAD_X) / columns as f64
                } else {
                    0.0
                };
                let mut x = CELL_PAD_X;
                for &w in &widths {
                    view.cols.push(x);
                    if w > 0.0 {
                        x += w + spare + CELL_GAP;
                    }
                }
                view.head_h = name_h + 2.0 * HEAD_PAD_Y;
                view.h += view.head_h;
            }
            // The layout keeps a node centered in its box, so room for loops is added on both sides.
            let room = if !own.is_empty() {
                2.0 * (MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach)
            } else {
                0.0
            };
            c = if below {
                cnode(view.w, view.h + room, -1)
            } else {
                cnode(view.w + room, view.h, -1)
            };
        }
        c.seq = views.len() as f64;
        views.push(view);
        cnodes.push(c);
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        cnodes[i].parent = node
            .parent_id
            .as_deref()
            .and_then(|p| index.get(p))
            .map(|&idx| idx as i32)
            .unwrap_or(-1);
    }

    let root_dir = if turned { Dir::TB } else { direction(Some(&db.direction)) };
    let layout = compound_layout(
        &mut cnodes,
        &mut cedges,
        root_dir,
        &LayeredOptions {
            node_sep: number_option(config, "nodeSpacing", NODE_SEP),
            edge_sep: EDGE_SEP,
            rank_sep: number_option(config, "rankSpacing", RANK_SEP),
            port_sep: 24.0,
            ..Default::default()
        },
    );

    spread_ends(&mut cedges, &cnodes, &views);

    // A subgraph gets its size from the layout, so its loops hang outside it and may widen the diagram.
    let mut layout_width = layout.width;
    for (i, own) in node_loops.iter().enumerate() {
        if own.is_empty() || !cnodes[i].is_group {
            continue;
        }
        let c = &cnodes[i];
        let mut reach = 0.0f64;
        for &li in own {
            reach = reach.max(all_loops[li].label.width);
        }
        layout_width = layout_width.max(c.x + c.w / 2.0 + MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach);
    }

    let title = title_label(db.title.as_deref(), measurer, size, 4000.0);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = layout_width.max(title.width);
    let ox = pad + (inner - layout_width) / 2.0;
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
    let mut fills: Vec<Fill> = Vec::new();
    let mut nodes_out = String::new();
    for i in 0..views.len() {
        let c = &cnodes[i];
        let node = views[i].node;
        let classes = class_names(strip_default_class(&node.css_classes));
        if c.is_group {
            views[i].w = c.w;
            views[i].h = c.h;
            let view = &views[i];
            let x = c.x - c.w / 2.0;
            let y = c.y - c.h / 2.0;
            let title_x = cluster_title_x(c, view.label.width, &crossings, GROUP_PAD);
            if paints(&view.style.shape) {
                fills.push(Fill {
                    bounds: [x, y, x + c.w, y + c.h],
                    shape: view.style.shape.clone(),
                });
            }
            clusters.push_str(&format!(
                "<g class=\"pele-cluster{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\"{} stroke=\"var(--_b)\"{}/>{}</g>",
                classes,
                esc_text(node.id),
                num(x),
                num(y),
                num(c.w),
                num(c.h),
                if sets_fill(&view.style.shape) { "" } else { " fill-opacity=\"0.5\"" },
                rounded(&view.style.shape),
                label_svg(
                    &view.label,
                    title_x,
                    y + 8.0 + view.label.height / 2.0,
                    &format!(
                        " class=\"pele-cluster-label\"{}{}",
                        ink_fill(&view.style.shape, &view.style.text, Some("var(--_m)")),
                        view.style.text,
                    ),
                ),
            ));
            continue;
        }

        let view = &views[i];
        let w = view.w;
        let h = view.h;
        let x0 = -w / 2.0;
        let y0 = -h / 2.0;
        let text = &view.style.text;
        let color = ink_fill(&view.style.shape, &view.style.text, None);
        let muted = if color.is_empty() { " fill=\"var(--_m)\"" } else { color.as_str() };
        let shape = format!("x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"", num(x0), num(y0), num(w), num(h));
        let name_cy = if !view.rows.is_empty() { y0 + view.head_h / 2.0 } else { 0.0 };
        let mut name = label_svg(&view.label, 0.0, name_cy, &format!(" class=\"pele-label\"{}{}", color, text));
        let raw_display = if node.alias.is_empty() { node.label } else { node.alias };
        let display = decode_entities(raw_display);
        if format!("{} ", classes).contains(" internal-link ") && !name.is_empty() && options.links != Some(false) {
            // The name ends up in an href, so it gets the same check as a URL.
            let safe = internal_link(&display, options);
            let target = esc(&display);
            let href_attr = if safe {
                format!(" href=\"{}\"{}", target, rel_attr(options))
            } else {
                String::new()
            };
            name = format!("<a class=\"internal-link\"{} data-href=\"{}\">{}</a>", href_attr, target, name);
            links.push(LinkInfo { id: node.label.to_string(), href: display, internal: true });
        }

        let body = if view.rows.is_empty() {
            format!("<rect {}{}/>{}", shape, rounded(&format!("{}{}", BOX_ATTRS, view.style.shape)), name)
        } else {
            let row_line = js_round(view.row_size * 1.5);
            let line_color = view.style.stroke.as_deref().map(esc).unwrap_or_else(|| "var(--_b)".to_string());
            let mut stripes = String::new();
            let mut cells = String::new();
            let mut y = y0 + view.head_h;
            for (r, row) in view.rows.iter().enumerate() {
                if r % 2 == 0 {
                    let span = format!("x=\"{}\" width=\"{}\"", num(x0), num(w));
                    if r == view.rows.len() - 1 {
                        // The last row follows the box's rounded bottom corners: a rounded rect, squared off on top.
                        stripes.push_str(&format!(
                            "<rect {} y=\"{}\" height=\"{}\"{}/><rect {} y=\"{}\" height=\"{}\"/>",
                            span,
                            num(y),
                            num(row.h),
                            ROUNDED,
                            span,
                            num(y),
                            num(row.h / 2.0),
                        ));
                    } else {
                        stripes.push_str(&format!("<rect {} y=\"{}\" height=\"{}\"/>", span, num(y), num(row.h)));
                    }
                }
                let baseline = y + ROW_PAD / 2.0 + row_line / 2.0 + view.row_size * 0.35;
                let [ty_cell, attr_cell, keys_cell, comment_cell] = &row.cells;
                cells.push_str(&text_left(
                    ty_cell,
                    x0 + view.cols[0],
                    baseline,
                    &format!(" class=\"pele-er-type\"{}{}", muted, text),
                    view.mono,
                ));
                cells.push_str(&text_left(
                    attr_cell,
                    x0 + view.cols[1],
                    baseline,
                    &format!(" class=\"pele-er-name\"{}{}", color, text),
                    view.mono,
                ));
                cells.push_str(&text_left(
                    keys_cell,
                    x0 + view.cols[2],
                    baseline,
                    &format!(" class=\"pele-er-keys\" font-size=\"{}\"{}{}", js_number(view.key_size), muted, text),
                    view.mono,
                ));
                cells.push_str(&text_left(
                    comment_cell,
                    x0 + view.cols[3],
                    baseline,
                    &format!(" class=\"pele-er-comment\"{}{}", muted, text),
                    0,
                ));
                y += row.h;
            }
            let divider_y = num(y0 + view.head_h);
            // Keep row shading light on light fills and dark on dark fills to preserve text contrast.
            let tone = ink(&view.style.shape, "");
            let shade = if tone == Some(ON_LIGHT) {
                ON_DARK
            } else if tone == Some(ON_DARK) {
                ON_LIGHT
            } else {
                "var(--_bg)"
            };
            format!(
                "<rect {}{}/><g class=\"pele-er-stripes\" fill=\"{}\"{}>{}</g><path class=\"pele-er-divider\" d=\"M{},{}H{}\" stroke=\"{}\"/><rect {} fill=\"none\" stroke=\"var(--_b)\"{}/>{}<g class=\"pele-er-attributes\" font-size=\"{}\">{}</g>",
                shape,
                rounded(&format!("{}{}", BOX_ATTRS, view.style.shape)),
                shade,
                if view.style.shape.contains("fill:") { " opacity=\"0.6\"" } else { "" },
                stripes,
                num(x0),
                divider_y,
                num(x0 + w),
                line_color,
                shape,
                rounded(&view.style.line),
                name,
                js_number(view.row_size),
                cells,
            )
        };
        nodes_out.push_str(&format!(
            "<g class=\"pele-node pele-entity{}\" data-id=\"{}\" transform=\"translate({},{})\">{}</g>",
            classes,
            esc_text(node.label),
            num(c.x),
            num(c.y),
            body,
        ));
    }

    // Self-relationships share the side they loop out of, squeezed together if it is too short for them.
    for (i, own) in node_loops.iter().enumerate() {
        if own.is_empty() {
            continue;
        }
        let view = &views[i];
        let mut total = 0.0f64;
        for &li in own {
            total += all_loops[li].band;
        }
        let side_len = if view.below { view.w } else { view.h };
        let scale = 1.0f64.min((side_len - 8.0).max(0.0) / total);
        let mut at = (-total * scale) / 2.0;
        for &li in own {
            let lp = &mut all_loops[li];
            lp.band *= scale;
            lp.at = at + lp.band / 2.0;
            at += lp.band;
        }
    }

    let mut edges_out = String::new();
    let mut casings = String::new();
    let mut labels_out = String::new();
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let d: String;
        let mut start: String;
        let mut end: String;
        if let Some(li) = loop_of[i] {
            let lp = &all_loops[li];
            let src = cedges[i].src;
            let c = &cnodes[src];
            let view = &views[src];
            let spread = (lp.band / 2.0 - 6.0).min(13.0).max(1.0);
            let far = MARKER_LENGTH + LOOP * 0.75 + 6.0;
            if view.below {
                let x = c.x + lp.at;
                let y = c.y + view.h / 2.0;
                let turn = num(y + MARKER_LENGTH);
                let out = num(y + MARKER_LENGTH + LOOP);
                d = format!(
                    "M{},{}V{}C{},{} {},{} {},{}V{}",
                    num(x - spread),
                    num(y),
                    turn,
                    num(x - spread),
                    out,
                    num(x + spread),
                    out,
                    num(x + spread),
                    turn,
                    num(y),
                );
                start = er_marker(edge.arrow_type_start, x - spread, y, 0.0, -1.0);
                end = er_marker(edge.arrow_type_end, x + spread, y, 0.0, -1.0);
                cedges[i].label_x = x;
                cedges[i].label_y = y + far + label.height / 2.0;
            } else {
                let x = c.x + view.w / 2.0;
                let y = c.y + lp.at;
                let turn = num(x + MARKER_LENGTH);
                let out = num(x + MARKER_LENGTH + LOOP);
                d = format!(
                    "M{},{}H{}C{},{} {},{} {},{}H{}",
                    num(x),
                    num(y - spread),
                    turn,
                    out,
                    num(y - spread),
                    out,
                    num(y + spread),
                    turn,
                    num(y + spread),
                    num(x),
                );
                start = er_marker(edge.arrow_type_start, x, y - spread, -1.0, 0.0);
                end = er_marker(edge.arrow_type_end, x, y + spread, -1.0, 0.0);
                cedges[i].label_x = x + far + label.width / 2.0;
                cedges[i].label_y = y;
            }
        } else {
            let mut route = straighten(&cedges[i].route);
            if route.len() < 6 {
                continue;
            }
            let src = cedges[i].src;
            let dst = cedges[i].dst;
            inset(&mut route, 0, &views[src], &cnodes[src]);
            let last_pt = route.len() - 3;
            inset(&mut route, last_pt, &views[dst], &cnodes[dst]);
            // A curve stops short of each box so that the marker sits on a straight run of line.
            let dx = (route[last_pt] - route[0]).abs();
            let dy = (route[last_pt + 1] - route[1]).abs();
            let direct = route.len() == 6 && (dx < 0.01 || dy < 0.01 || dx.hypot(dy) <= 3.0 * MARKER_LENGTH);
            let stub = if direct { 0.0 } else { MARKER_LENGTH };
            let path = route_path_default(&route, None, stub, stub);
            d = if stub > 0.0 {
                format!("M{},{}L{}L{},{}", num(path.sx), num(path.sy), &path.d[1..], num(path.ex), num(path.ey))
            } else {
                path.d
            };
            start = er_marker(edge.arrow_type_start, path.sx, path.sy, path.sdx, path.sdy);
            end = er_marker(edge.arrow_type_end, path.ex, path.ey, path.edx, path.edy);
        }
        let under = fill_under(&d, &[], &fills, None);
        let over: Option<&'static str> = match under {
            Under::None => None,
            Under::Border => Some(""),
            Under::Fill(fill) => Some(edge_ink(fill, "")),
        };
        if over == Some("") {
            casings.push_str(&casing(&d, &format!("{}{}", start, end), "", false));
        }
        if let (Under::Fill(fill), Some(ink_str)) = (under, over) {
            if !ink_str.is_empty() {
                start = hollow_on(&start, fill);
                end = hollow_on(&end, fill);
            }
        }
        let stroke_attr = match over.filter(|s| !s.is_empty()) {
            Some(s) => format!(" stroke=\"{}\"", s),
            None => String::new(),
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge pele-er-{}\" data-id=\"{}\"{}><path d=\"{}\"{}/>{}{}</g>",
            if edge.dashed { "non-identifying" } else { "identifying" },
            esc_text(&edge.id),
            stroke_attr,
            d,
            if edge.dashed { " stroke-dasharray=\"4 3\"" } else { "" },
            start,
            end,
        ));
        if label.width > 0.0 {
            labels_out.push_str(&edge_label_svg(&edge.id, label, cedges[i].label_x, cedges[i].label_y, ""));
        }
    }

    let fixed = config.get("er").get("useMaxWidth").as_bool() == Some(false);
    let content = format!(
        "{}{}{}{}{}<g class=\"pele-nodes\">{}</g>",
        label_svg(&title, width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\""),
        if clusters.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-clusters\">{}</g>", clusters)
        },
        casings_svg(&casings),
        if edges_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{}</g>", edges_out)
        },
        if labels_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>", js_number(small_size), labels_out)
        },
        nodes_out,
    );

    let svg = svg_document(
        "er",
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

    Rendered { svg, width, height, links }
}

fn straighten(route: &[f64]) -> Vec<f64> {
    if route.len() < 3 {
        return route.to_vec();
    }
    let mut out = route[..3].to_vec();
    let mut k = 3;
    while k < route.len() {
        let prev = out.len() - 3;
        let flat = |axis: usize| -> bool {
            (out[prev + axis] - route[k + axis]).abs() < 0.01 && (route[k + axis] - route[k + 3 + axis]).abs() < 0.01
        };
        if k + 3 < route.len() && (flat(0) || flat(1)) {
            k += 3;
            continue;
        }
        out.extend_from_slice(&route[k..k + 3]);
        k += 3;
    }
    out
}

// The layout brings every edge that ends on one side of a box to the middle of that side, where
// their markers would cover each other. This spaces them along the side, in the order they fan out.
fn spread_ends(cedges: &mut [CEdge], cnodes: &[CNode], views: &[NodeView<'_>]) {
    let mut sides: HashMap<usize, Vec<End>> = HashMap::new();
    for (edge_idx, e) in cedges.iter().enumerate() {
        if e.src == e.dst || e.route.len() < 6 {
            continue;
        }
        let last = e.route.len() - 3;
        for (node, at, next) in [(e.src, 0, 3), (e.dst, last, last - 3)] {
            let c = &cnodes[node];
            let dx = e.route[at] - c.x;
            let dy = e.route[at + 1] - c.y;
            let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
                if dy < 0.0 { 0 } else { 2 }
            } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
                if dx < 0.0 { 3 } else { 1 }
            } else {
                continue;
            };
            let key = node * 4 + side;
            let toward = e.route[next + if (side & 1) != 0 { 1 } else { 0 }];
            sides.entry(key).or_default().push(End { edge_idx, at, toward });
        }
    }
    for (key, mut ends) in sides {
        if ends.len() < 2 {
            continue;
        }
        let node = key >> 2;
        let vertical = (key & 1) == 1;
        let (box_w, box_h) = if cnodes[node].is_group {
            (cnodes[node].w, cnodes[node].h)
        } else {
            (views[node].w, views[node].h)
        };
        let length = if vertical { box_h } else { box_w };
        let step = PORT_GAP.min((length - 2.0 * PORT_MARGIN) / (ends.len() - 1) as f64).max(0.0);
        let center = if vertical { cnodes[node].y } else { cnodes[node].x };
        ends.sort_by(|a, b| a.toward.partial_cmp(&b.toward).unwrap_or(std::cmp::Ordering::Equal));
        let denom = (ends.len() - 1) as f64 / 2.0;
        let coord = if vertical { 1 } else { 0 };
        for (i, end) in ends.iter().enumerate() {
            cedges[end.edge_idx].route[end.at + coord] = center + (i as f64 - denom) * step;
        }
    }
}

// An entity with a self-relationship is laid out larger than it is drawn; this moves a route end
// from the side of that larger box onto the entity's own side.
fn inset(route: &mut [f64], at: usize, view: &NodeView<'_>, c: &CNode) {
    if c.is_group {
        return;
    }
    let dx = route[at] - c.x;
    let dy = route[at + 1] - c.y;
    if c.w != view.w && (dx.abs() - c.w / 2.0).abs() < 0.5 {
        route[at] -= (sign(dx) * (c.w - view.w)) / 2.0;
    }
    if c.h != view.h && (dy.abs() - c.h / 2.0).abs() < 0.5 {
        route[at + 1] -= (sign(dy) * (c.h - view.h)) / 2.0;
    }
}
