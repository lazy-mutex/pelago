use super::db::MindmapDb;
use crate::diagrams::common::fit_width::{title_room, turn_to_fit};
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_round, label_svg, num};
use crate::svg::palette::{palette, Palette};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::shapes::{draw_shape, shape_inset, shape_size};
use crate::svg::theme::{class_names, series_color};
use crate::svg::title::title_label;
use crate::text::label::{label_line, layout_label, line_middle, Label};
use crate::text::measurer::MetricsMeasurer;
use crate::value::Value;

// Indexed by Mermaid's node type. A node without delimiters is text on a line, except the root.
const SHAPES: [&str; 7] = ["", "rounded", "rect", "circle", "cloud", "bang", "hex"];
const ROOT_SHAPE: &str = "stadium";

const GAP_X: f64 = 36.0;
const GAP_Y: f64 = 8.0;
// Between two subtrees, of which at least one has children of its own.
const GROUP_GAP: f64 = 18.0;
const BRANCH_GAP_X: f64 = 52.0;
const BRANCH_GAP_Y: f64 = 28.0;
const TEXT_PAD: f64 = 6.0;
const PILL_PAD: f64 = 12.0;
const TINT: f64 = 0.22;
// Running down, each level is set in by this much, and links leave a node this far from its left edge.
const INDENT: f64 = 28.0;
const STEM: f64 = 12.0;
const BEND: f64 = 8.0;
const ROW_GAP: f64 = 10.0;

// A label with each line starting at `left`, for an outline, where centered lines would look ragged.
fn left_label(label: &Label, left: f64, cy: f64) -> String {
    if label.lines.len() < 2 {
        return label_svg(label, left + label.width / 2.0, cy, " class=\"pele-label\"");
    }
    let mut out = String::new();
    let top = cy - label.height / 2.0;
    for k in 0..label.lines.len() {
        out.push_str(&label_svg(
            &label_line(label, k),
            left + label.widths[k] / 2.0,
            top + line_middle(label, k),
            " class=\"pele-label\"",
        ));
    }
    out
}

pub fn render_mindmap(model: &MindmapDb, config: &Value, options: &RenderOptions) -> Rendered {
    let pal = palette("mindmap", config);
    turn_to_fit(options, &|down| draw(model, config, options, &pal, down))
}

fn stack(
    nodes: &[super::db::MindmapNode],
    list: &[usize],
    from: usize,
    to: usize,
    gap: f64,
    extent: &[f64],
    own: &[f64],
    anchor: &[f64],
    rel: &mut [f64],
) -> (f64, f64) {
    let mut y = 0.0_f64;
    let mut last = 0.0_f64;
    for k in from..to {
        let c = list[k];
        rel[c] = y;
        let grouped = !nodes[c].children.is_empty()
            || (k + 1 < to && !nodes[list[k + 1]].children.is_empty());
        last = if grouped { gap.max(GROUP_GAP) } else { gap };
        y += extent[c] + last;
    }
    let stacked = y - last;
    let a = list[from];
    let b = list[to - 1];
    let mid = (rel[a] + own[a] + anchor[a] + rel[b] + own[b] + anchor[b]) / 2.0;
    (mid, stacked)
}

// `down` draws the map as an outline: the root at the top, and each level set in under its parent.
// It is far narrower than branches spreading to both sides, and as tall as it needs to be.
fn draw(
    model: &MindmapDb,
    config: &Value,
    options: &RenderOptions,
    pal: &Palette,
    down: bool,
) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let mindmap_cfg = config.get("mindmap");
    let wrap = mindmap_cfg
        .get("maxNodeWidth")
        .as_f64()
        .filter(|&v| v > 0.0)
        .map_or(200.0, |v| v.min(2000.0));

    let nodes = &model.nodes;
    let n = nodes.len();
    let mut parent = vec![0usize; n];
    let mut depth = vec![0usize; n];
    for i in 0..n {
        for &child_id in &nodes[i].children {
            parent[child_id] = i;
            depth[child_id] = depth[i] + 1;
        }
    }

    // An outline has the width the host gives it, less what its level is set in.
    let room = match options.max_width.filter(|&mw| down && mw > 0.0) {
        Some(mw) => mw - 2.0 * pad,
        None => f64::INFINITY,
    };

    let mut labels = Vec::with_capacity(n);
    let mut shapes: Vec<&'static str> = Vec::with_capacity(n);
    let mut w = vec![0.0_f64; n];
    let mut h = vec![0.0_f64; n];
    let mut dy = vec![0.0_f64; n];
    // Where links meet the node, measured down from its top: its middle.
    let mut anchor = vec![0.0_f64; n];

    for i in 0..n {
        let node = &nodes[i];
        let fit = wrap.min(72.0_f64.max(room - depth[i] as f64 * INDENT - 2.0 * TEXT_PAD));
        let label = layout_label(Some(&node.descr), true, measurer, size, fit, 0);
        let tw = label.width;
        let th = label.height.max(js_round(size * 1.5));
        let raw_shape = SHAPES.get(node.node_type as usize).copied().unwrap_or("");
        let mut shape = if !raw_shape.is_empty() {
            raw_shape
        } else if i == 0 {
            ROOT_SHAPE
        } else {
            ""
        };
        // In an outline the root heads a column, and the trunk drops from under its left end. A circle
        // or a cloud would stand wide of that, so the root is a pill there unless it is a box already.
        if down && i == 0 && shape != "rect" && shape != "rounded" {
            shape = ROOT_SHAPE;
        }
        shapes.push(shape);
        if shape.is_empty() {
            // A node with no shape of its own is plain text when it ends a branch, and text in a
            // tinted pill when the branch goes on from it, which sets the levels apart.
            let pill = !node.children.is_empty();
            w[i] = tw + 2.0 * if pill { PILL_PAD } else { TEXT_PAD };
            h[i] = th + if pill { 6.0 } else { 0.0 };
            anchor[i] = h[i] / 2.0;
        } else {
            let s = shape_size(shape, tw, th);
            w[i] = s.w;
            h[i] = s.h;
            dy[i] = s.dy;
            anchor[i] = s.h / 2.0;
        }
        labels.push(label);
    }

    // Each subtree takes a band of height `extent`. `rel` is a subtree's offset among its siblings,
    // `own` the node's offset inside its band, and `kids` the offset of its children's bands.
    let mut extent = vec![0.0_f64; n];
    let mut rel = vec![0.0_f64; n];
    let mut own = vec![0.0_f64; n];
    let mut kids = vec![0.0_f64; n];

    // Nodes are in depth-first order, so going backwards visits children before their parent.
    for i in (1..n).rev() {
        let children = &nodes[i].children;
        if children.is_empty() {
            extent[i] = h[i];
            continue;
        }
        let (mid, stacked) = stack(
            nodes,
            children,
            0,
            children.len(),
            GAP_Y,
            &extent,
            &own,
            &anchor,
            &mut rel,
        );
        let top = mid - anchor[i];
        let low = 0.0_f64.min(top);
        extent[i] = stacked.max(top + h[i]) - low;
        own[i] = top - low;
        kids[i] = -low;
    }

    let mut x = vec![0.0_f64; n];
    let mut y = vec![0.0_f64; n];
    let mut band = vec![0.0_f64; n];
    let mut side = vec![0i8; n];
    // Running down: how far the trunk under each node has been drawn.
    let mut trunk = vec![0.0_f64; n];

    if down {
        // Nodes are in depth-first order, which is the order of an outline's rows.
        let mut row = 0.0_f64;
        for i in 0..n {
            if i > 0 && parent[i] == 0 {
                row += ROW_GAP;
            }
            x[i] = depth[i] as f64 * INDENT + w[i] / 2.0;
            y[i] = row;
            row += h[i] + ROW_GAP;
        }
    } else if n > 0 {
        // The first branches go right and the rest left, split where the two sides are closest in height.
        let branches = &nodes[0].children;
        let count = branches.len();
        let mut total = 0.0_f64;
        for &b_id in branches {
            total += extent[b_id];
        }
        let mut split = count;
        let mut best = f64::INFINITY;
        let mut right = 0.0_f64;
        for k in 1..count {
            right += extent[branches[k - 1]];
            let diff =
                (2.0 * right - total + (2.0 * k as f64 - count as f64) * BRANCH_GAP_Y).abs();
            if diff < best {
                best = diff;
                split = k;
            }
        }
        let mut from = 0;
        let mut to = split;
        let mut dir = 1i8;
        while from < count {
            let (middle, _) = stack(
                nodes,
                branches,
                from,
                to,
                BRANCH_GAP_Y,
                &extent,
                &own,
                &anchor,
                &mut rel,
            );
            for k in from..to {
                let c = branches[k];
                band[c] = rel[c] - middle;
                side[c] = dir;
            }
            from = to;
            to = count;
            dir = -1;
        }
        y[0] = -h[0] / 2.0;
        for i in 0..n {
            if i > 0 {
                y[i] = band[i] + own[i];
            }
            for &c in &nodes[i].children {
                if i > 0 {
                    side[c] = side[i];
                    band[c] = band[i] + kids[i] + rel[c];
                }
                x[c] = x[i]
                    + side[c] as f64
                        * (w[i] / 2.0 + if i > 0 { GAP_X } else { BRANCH_GAP_X } + w[c] / 2.0);
            }
        }
    }

    let mut min_x = 0.0_f64;
    let mut max_x = 0.0_f64;
    let mut min_y = 0.0_f64;
    let mut max_y = 0.0_f64;
    let mut out = String::new();
    let mut edges = String::new();
    let mut body = String::new();
    let mut color = String::new();

    let flush = |out: &mut String, edges: &mut String, body: &mut String, color: &str, section: Option<usize>| {
        if body.is_empty() {
            return;
        }
        out.push_str(&format!(
            "<g class=\"pele-branch pele-section-{}\"><g class=\"pele-edges\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\">{}</g>{}</g>",
            section.unwrap_or(0),
            color,
            edges,
            body
        ));
        edges.clear();
        body.clear();
    };

    // The root is drawn last, on top of the links that leave it.
    for k in 1..=n {
        let i = k % n;
        let node = &nodes[i];
        let shape = shapes[i];
        let label = &labels[i];
        let cx = x[i];
        let top = y[i];
        min_x = min_x.min(cx - w[i] / 2.0);
        max_x = max_x.max(cx + w[i] / 2.0);
        min_y = min_y.min(top);
        max_y = max_y.max(top + h[i]);

        if i == 0 || parent[i] == 0 {
            flush(&mut out, &mut edges, &mut body, &color, nodes[k - 1].section);
        }
        if i > 0 && down {
            if parent[i] == 0 {
                color = series_color(node.section.unwrap_or(0) as i64);
            }
            let p = parent[i];
            // The link drops from under its parent and turns in to the node's left. It starts where
            // the link to the sibling above turned in, so each stretch of a shared trunk has one color.
            // Under the root's rounded end, the trunk starts where that end has come down to the bottom.
            let stem = if p == 0 {
                STEM.max(h[0] / 2.0).min(INDENT - BEND)
            } else {
                STEM
            };
            let x1 = x[p] - w[p] / 2.0 + stem.min(w[p] / 2.0);
            let y1 = if trunk[p] != 0.0 {
                trunk[p]
            } else {
                y[p] + h[p]
            };
            let left = cx - w[i] / 2.0;
            let x2 = left
                + if !shape.is_empty() {
                    shape_inset(shape, w[i], h[i], 3, 0.0)
                } else {
                    0.0
                };
            let y2 = top + anchor[i];
            let bend = 0.0_f64.max(BEND.min(x2 - x1).min(y2 - y1));
            // The next link carries on from where this one leaves the trunk, so the trunk has no gap.
            trunk[p] = y2 - bend;
            edges.push_str(&format!(
                "<path class=\"pele-edge\" d=\"M{},{}V{}Q{},{} {},{}H{}\"/>",
                num(x1),
                num(y1),
                num(y2 - bend),
                num(x1),
                num(y2),
                num(x1 + bend),
                num(y2),
                num(x2)
            ));
        } else if i > 0 {
            if parent[i] == 0 {
                color = series_color(node.section.unwrap_or(0) as i64);
            }
            let p = parent[i];
            let dir = side[i] as f64;
            let from = if side[i] > 0 { 1 } else { 3 };
            let p_inset = if !shapes[p].is_empty() {
                shape_inset(shapes[p], w[p], h[p], from, 0.0)
            } else {
                0.0
            };
            let x1 = x[p] + dir * (w[p] / 2.0 - p_inset);
            let y1 = y[p] + anchor[p];
            let near = cx - (dir * w[i]) / 2.0;
            let i_inset = if !shape.is_empty() {
                dir * shape_inset(shape, w[i], h[i], 4 - from, 0.0)
            } else {
                0.0
            };
            let x2 = near + i_inset;
            let y2 = top + anchor[i];
            let mid = num((x1 + x2) / 2.0);
            edges.push_str(&format!(
                "<path class=\"pele-edge\" d=\"M{},{}C{},{} {},{} {},{}\"/>",
                num(x1),
                num(y1),
                mid,
                num(y1),
                mid,
                num(y2),
                num(x2),
                num(y2)
            ));
        }

        let pill = shape.is_empty() && !node.children.is_empty();
        // Every node off the root is filled with a tint of its branch's color and outlined in it,
        // whether its shape was given in the source or is the pill of a node that branches.
        // The root is neutral.
        let tint = format!(
            " fill=\"{}\" fill-opacity=\"{}\" stroke=\"{}\"",
            color, TINT, color
        );
        let inner = if !shape.is_empty() {
            if i > 0 {
                let stroke_only = format!(" stroke=\"{}\"", color);
                draw_shape(shape, w[i], h[i], &tint, &stroke_only)
            } else {
                draw_shape(
                    shape,
                    w[i],
                    h[i],
                    " fill=\"var(--_s)\" stroke=\"var(--_b)\"",
                    " stroke=\"var(--_b)\"",
                )
            }
        } else if pill {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"{}/>",
                num(-w[i] / 2.0),
                num(-h[i] / 2.0),
                num(w[i]),
                num(h[i]),
                num(h[i] / 2.0),
                tint
            )
        } else {
            String::new()
        };

        let shape_class = if !shape.is_empty() {
            shape
        } else if pill {
            "branch"
        } else {
            "text"
        };
        let root_class = if i > 0 { "" } else { " pele-root" };
        let extra_classes = class_names(node.class.as_deref().unwrap_or(""));
        let label_markup = if down && shape.is_empty() {
            left_label(label, -label.width / 2.0, dy[i])
        } else {
            label_svg(label, 0.0, dy[i], " class=\"pele-label\"")
        };
        let group = format!(
            "<g class=\"pele-node pele-shape-{}{}{}\" data-id=\"{}\" transform=\"translate({},{})\">{}{}</g>",
            shape_class,
            root_class,
            extra_classes,
            esc_text(&node.node_id),
            num(cx),
            num(top + h[i] / 2.0),
            inner,
            label_markup
        );
        if i > 0 {
            body.push_str(&group);
        } else {
            out.push_str(&group);
        }
    }

    let title = title_label(model.title.as_deref(), measurer, size, title_room(options));
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let width = (max_x - min_x).max(title.width);
    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let content = format!(
        "{}<g transform=\"translate({},{})\">{}</g>",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        num(pad - min_x + (width - max_x + min_x) / 2.0),
        num(pad + title_height - min_y),
        out
    );

    let fixed = mindmap_cfg.get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "mindmap",
        total_width,
        total_height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive: options.responsive.unwrap_or(!fixed),
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: &pal.style,
        },
        &Accessible::default(),
        &content,
    );

    Rendered {
        svg,
        width: total_width,
        height: total_height,
        links: Vec::new(),
    }
}
