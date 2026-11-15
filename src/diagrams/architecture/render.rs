use super::db::{ArchNode, ArchitectureDb};
use super::icons::builtin_icon;
use super::layout::{place_on_grid, side_index};
use crate::chars::is_space;
use crate::options::{tighten, title_room, RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::edges::{edge_label_svg, marker, marker_trim};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::ROUNDED;
use crate::text::label::{layout_label, Label, Span};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;
use std::collections::HashMap;

const STUB: f64 = 12.0;
const CORNER: f64 = 6.0;
const LABEL_GAP: f64 = 4.0;
const DOT: f64 = 3.0;
const DX: [f64; 4] = [-1.0, 1.0, 0.0, 0.0];
const DY: [f64; 4] = [0.0, 0.0, -1.0, 1.0];

fn setting(config: &Value, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    let section = config.get("architecture");
    if !section.is_object() {
        return fallback;
    }
    match section.get(key).as_f64() {
        Some(v) if v.is_finite() => v.clamp(min, max),
        _ => fallback,
    }
}

// Maps the grid lines in use to consecutive indexes, so empty rows and columns take no room.
fn rank(mut values: Vec<i32>) -> HashMap<i32, usize> {
    values.sort_unstable();
    let mut out = HashMap::new();
    for v in values {
        let next = out.len();
        out.entry(v).or_insert(next);
    }
    out
}

fn icon_slot(name: &str, x: f64, y: f64, side: f64) -> String {
    format!(
        "<svg class=\"pele-icon\" data-icon=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{}</svg>",
        esc_text(name),
        num(x),
        num(y),
        num(side),
        num(side),
        builtin_icon(name)
    )
}

// Cuts a label down to a box, ending what is left with an ellipsis.
fn fit(label: &Label, max_width: f64, max_lines: usize, measurer: &dyn TextMeasurer) -> Label {
    let cut = label.lines.len() > max_lines;
    if !cut && label.width <= max_width {
        return label.clone();
    }
    let dots = measurer.width("…", label.size, 0);
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();
    let mut widest = 0.0f64;
    let take_lines = max_lines.min(label.lines.len());
    for i in 0..take_lines {
        let mut line = label.lines[i].clone();
        let mut width = label.widths[i];
        if width > max_width || (cut && i == max_lines - 1) {
            loop {
                let Some(span) = line.last_mut() else {
                    line.push(Span { text: "…".to_string(), style: 0, width: dots });
                    width = dots;
                    break;
                };
                let rest = width - span.width;
                if rest + dots < max_width {
                    let chars: Vec<char> = span.text.chars().collect();
                    let mut lo = 0usize;
                    let mut hi = chars.len();
                    while lo < hi {
                        let mid = (lo + hi + 1) >> 1;
                        let prefix: String = chars[..mid].iter().collect();
                        if rest + measurer.width(&prefix, label.size, span.style) + dots <= max_width {
                            lo = mid;
                        } else {
                            hi = mid - 1;
                        }
                    }
                    let prefix: String = chars[..lo].iter().collect();
                    span.text = format!("{}…", prefix.trim_end_matches(is_space));
                    span.width = measurer.width(&span.text, label.size, span.style);
                    width = rest + span.width;
                    break;
                }
                line.pop();
                width = rest;
            }
        }
        lines.push(line);
        widths.push(width);
        widest = widest.max(width);
    }
    let height = lines.len() as f64 * label.line_height;
    Label {
        lines,
        widths,
        width: widest,
        height,
        size: label.size,
        line_height: label.line_height,
    }
}

#[derive(Clone, Copy)]
struct Port {
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    gap: f64,
    wire: bool,
}

// An orthogonal route from one port to another, each left along its side's normal.
fn route(s: &Port, e: &Port) -> Vec<f64> {
    let (sx, sy, sdx, sdy) = (s.x, s.y, s.dx, s.dy);
    let (ex, ey, edx, edy) = (e.x, e.y, e.dx, e.dy);
    let s1x = sx + sdx * STUB;
    let s1y = sy + sdy * STUB;
    let e1x = ex + edx * STUB;
    let e1y = ey + edy * STUB;
    if sdx * edx + sdy * edy == 0.0 {
        let cx = if sdx != 0.0 { ex } else { sx };
        let cy = if sdx != 0.0 { sy } else { ey };
        if (cx - sx) * sdx + (cy - sy) * sdy > 0.0 && (cx - ex) * edx + (cy - ey) * edy > 0.0 {
            return vec![sx, sy, cx, cy, ex, ey];
        }
        return if sdx != 0.0 {
            vec![sx, sy, s1x, s1y, s1x, e1y, e1x, e1y, ex, ey]
        } else {
            vec![sx, sy, s1x, s1y, e1x, s1y, e1x, e1y, ex, ey]
        };
    }
    let sideways = sdx != 0.0;
    let offset = if sideways { ey - sy } else { ex - sx };
    if sdx == -edx && sdy == -edy {
        let ahead = (ex - sx) * sdx + (ey - sy) * sdy;
        if ahead > 0.0 {
            if offset.abs() < 0.5 {
                return vec![sx, sy, ex, ey];
            }
            if sideways {
                let lo = if sdx > 0.0 { s.x1 } else { e.x1 };
                let hi = if sdx > 0.0 { e.x0 } else { s.x0 };
                let mx = if lo < hi { (lo + hi) / 2.0 } else { (sx + ex) / 2.0 };
                return vec![sx, sy, mx, sy, mx, ey, ex, ey];
            }
            let lo = if sdy > 0.0 { s.y1 } else { e.y1 };
            let hi = if sdy > 0.0 { e.y0 } else { s.y0 };
            let my = if lo < hi { (lo + hi) / 2.0 } else { (sy + ey) / 2.0 };
            return vec![sx, sy, sx, my, ex, my, ex, ey];
        }
    } else if offset.abs() >= 1.0 {
        if sideways {
            let x = if sdx > 0.0 { sx.max(ex) + STUB } else { sx.min(ex) - STUB };
            return vec![sx, sy, x, sy, x, ey, ex, ey];
        }
        let y = if sdy > 0.0 { sy.max(ey) + STUB } else { sy.min(ey) - STUB };
        return vec![sx, sy, sx, y, ex, y, ex, ey];
    }
    // The straight way is blocked by one of the two nodes: go between them, or around both.
    if sideways {
        let lane = if s.y1 + STUB <= e.y0 {
            (s.y1 + e.y0) / 2.0
        } else if e.y1 + STUB <= s.y0 {
            (e.y1 + s.y0) / 2.0
        } else {
            s.y1.max(e.y1) + STUB
        };
        return vec![sx, sy, s1x, sy, s1x, lane, e1x, lane, e1x, ey, ex, ey];
    }
    let lane = if s.x1 + STUB <= e.x0 {
        (s.x1 + e.x0) / 2.0
    } else if e.x1 + STUB <= s.x0 {
        (e.x1 + s.x0) / 2.0
    } else {
        s.x1.max(e.x1) + STUB
    };
    vec![sx, sy, sx, s1y, lane, s1y, lane, e1y, ex, e1y, ex, ey]
}

// Drops repeated points and joins runs that continue in a straight line.
fn simplify(p: &[f64]) -> Vec<f64> {
    let mut out = vec![p[0], p[1]];
    let mut i = 2;
    while i + 1 < p.len() {
        let x = p[i];
        let y = p[i + 1];
        i += 2;
        let n = out.len();
        let same_x = (x - out[n - 2]).abs() < 0.01;
        let same_y = (y - out[n - 1]).abs() < 0.01;
        if same_x && same_y {
            continue;
        }
        if n >= 4
            && ((same_x && (out[n - 4] - out[n - 2]).abs() < 0.01)
                || (same_y && (out[n - 3] - out[n - 1]).abs() < 0.01))
        {
            out[n - 2] = x;
            out[n - 1] = y;
        } else {
            out.push(x);
            out.push(y);
        }
    }
    out
}

fn trim_end(xs: &mut [f64], ys: &mut [f64], at: usize, toward: usize, amount: f64) {
    let len = (xs[toward] - xs[at]).hypot(ys[toward] - ys[at]);
    let t = amount.min((len - 1.0).max(0.0));
    if len > 0.0 && t > 0.0 {
        xs[at] += ((xs[toward] - xs[at]) / len) * t;
        ys[at] += ((ys[toward] - ys[at]) / len) * t;
    }
}

fn path_data(p: &[f64], start_trim: f64, end_trim: f64) -> String {
    let n = p.len() / 2;
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    for i in 0..n {
        xs.push(p[i * 2]);
        ys.push(p[i * 2 + 1]);
    }
    trim_end(&mut xs, &mut ys, 0, 1, start_trim);
    trim_end(&mut xs, &mut ys, n - 1, n - 2, end_trim);
    let mut d = format!("M{},{}", num(xs[0]), num(ys[0]));
    for i in 1..n - 1 {
        let before = (xs[i] - xs[i - 1]).hypot(ys[i] - ys[i - 1]);
        let after = (xs[i + 1] - xs[i]).hypot(ys[i + 1] - ys[i]);
        let r = CORNER.min(before / 2.0).min(after / 2.0);
        if r < 0.5 {
            d.push_str(&format!("L{},{}", num(xs[i]), num(ys[i])));
            continue;
        }
        let in_x = xs[i] - ((xs[i] - xs[i - 1]) / before) * r;
        let in_y = ys[i] - ((ys[i] - ys[i - 1]) / before) * r;
        let out_x = xs[i] + ((xs[i + 1] - xs[i]) / after) * r;
        let out_y = ys[i] + ((ys[i + 1] - ys[i]) / after) * r;
        d.push_str(&format!(
            "L{},{}Q{},{} {},{}",
            num(in_x),
            num(in_y),
            num(xs[i]),
            num(ys[i]),
            num(out_x),
            num(out_y)
        ));
    }
    d.push_str(&format!("L{},{}", num(xs[n - 1]), num(ys[n - 1])));
    d
}

pub fn render_architecture(model: &ArchitectureDb, config: &Value, options: &RenderOptions) -> Rendered {
    tighten(options, &|tight| draw(model, config, options, tight))
}

fn draw(model: &ArchitectureDb, config: &Value, options: &RenderOptions, tight: bool) -> Rendered {
    let icon_size = setting(config, "iconSize", 48.0, 8.0, 512.0);
    let size = setting(config, "fontSize", options.font_size.unwrap_or(16.0), 4.0, 128.0);
    let group_pad = setting(config, "padding", 20.0, 0.0, 256.0).min(if tight { 12.0 } else { f64::INFINITY });
    let small = js_round(size * 0.875);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let half = icon_size / 2.0;

    let nodes = &model.nodes;
    let groups = &model.groups;
    let n = nodes.len();
    let g_len = groups.len();
    let grid = place_on_grid(nodes, groups, &model.edges, &model.layout_hints);
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, node)| (node.id(), i)).collect();

    // A service is its icon square with the title beneath; a junction takes the same room.
    let mut labels: Vec<Label> = Vec::with_capacity(n);
    let mut box_w = vec![0.0f64; n];
    let mut box_h = vec![0.0f64; n];
    let wrap = if tight { icon_size.max(64.0) } else { (icon_size * 2.25).max(96.0) };
    for i in 0..n {
        let svc_title = match &nodes[i] {
            ArchNode::Service { title, .. } => title.as_deref(),
            ArchNode::Junction { .. } => None,
        };
        let label = layout_label(svc_title, true, measurer, size, wrap, 0);
        box_w[i] = icon_size.max(2.0 * (label.width / 2.0).ceil());
        box_h[i] = icon_size + if label.height > 0.0 { LABEL_GAP + label.height } else { 0.0 };
        labels.push(label);
    }

    let mut col_values: Vec<i32> = Vec::with_capacity(n + g_len);
    let mut row_values: Vec<i32> = Vec::with_capacity(n + g_len);
    for i in 0..n {
        col_values.push(grid.col[i]);
        row_values.push(grid.row[i]);
    }
    for g in 0..g_len {
        if !grid.group_empty[g] {
            continue;
        }
        col_values.push(grid.group_col[g]);
        row_values.push(grid.group_row[g]);
    }
    let col_of = rank(col_values);
    let row_of = rank(row_values);
    let c_len = col_of.len();
    let r_len = row_of.len();
    let mut col_w = vec![0.0f64; c_len];
    let mut row_h = vec![0.0f64; r_len];
    let gap = if tight { (2.0 * STUB).max(24.0) } else { js_round(icon_size * 0.75).max(40.0) };
    let mut gap_x = vec![gap; c_len];
    let mut gap_y = vec![gap; r_len];
    let mut node_col = vec![0usize; n];
    let mut node_row = vec![0usize; n];
    for i in 0..n {
        let c = col_of[&grid.col[i]];
        let r = row_of[&grid.row[i]];
        node_col[i] = c;
        node_row[i] = r;
        col_w[c] = col_w[c].max(box_w[i]);
        row_h[r] = row_h[r].max(box_h[i]);
    }
    for g in 0..g_len {
        if !grid.group_empty[g] {
            continue;
        }
        let c = col_of[&grid.group_col[g]];
        let r = row_of[&grid.group_row[g]];
        col_w[c] = col_w[c].max(icon_size);
        row_h[r] = row_h[r].max(half);
    }

    // An edge label needs the gap its edge crosses to be wide enough to hold it.
    let e_len = model.edges.len();
    let mut edge_labels: Vec<Label> = Vec::with_capacity(e_len);
    let mut ends = vec![-1i32; e_len * 2];
    for k in 0..e_len {
        let edge = &model.edges[k];
        let a = index.get(edge.lhs_id.as_str()).map_or(-1, |&idx| idx as i32);
        let b = index.get(edge.rhs_id.as_str()).map_or(-1, |&idx| idx as i32);
        ends[k * 2] = a;
        ends[k * 2 + 1] = b;
        let label = layout_label(edge.title.as_deref(), true, measurer, small, 160.0, 0);
        if label.width > 0.0 && a >= 0 && b >= 0 {
            let (au, bu) = (a as usize, b as usize);
            if node_col[au] != node_col[bu] {
                let c = node_col[au].min(node_col[bu]);
                gap_x[c] = gap_x[c].max(label.width.ceil() + 28.0);
            } else {
                col_w[node_col[au]] = col_w[node_col[au]].max(2.0 * (label.width / 2.0).ceil() + 8.0);
                if node_row[au] != node_row[bu] {
                    let r = node_row[au].min(node_row[bu]);
                    gap_y[r] = gap_y[r].max(label.height.ceil() + 24.0);
                }
            }
        }
        edge_labels.push(label);
    }

    // Room around each group, kept on the rows and columns at its edges.
    let mut group_labels: Vec<Label> = Vec::with_capacity(g_len);
    let mut c0 = vec![0usize; g_len];
    let mut c1 = vec![0usize; g_len];
    let mut r0 = vec![0usize; g_len];
    let mut r1 = vec![0usize; g_len];
    let mut ins_l = vec![0.0f64; g_len];
    let mut ins_r = vec![0.0f64; g_len];
    let mut ins_t = vec![0.0f64; g_len];
    let mut ins_b = vec![0.0f64; g_len];
    let mut head_w = vec![0.0f64; g_len];
    let mut head_h = vec![0.0f64; g_len];
    let mut col_l = vec![0.0f64; c_len];
    let mut col_r = vec![0.0f64; c_len];
    let mut row_t = vec![0.0f64; r_len];
    let mut row_b = vec![0.0f64; r_len];
    let badge = js_round(size * 1.25);
    for g in 0..g_len {
        c0[g] = col_of[&grid.group_col[g]];
        c1[g] = col_of[&(grid.group_col[g] + grid.group_cols[g] - 1)];
        r0[g] = row_of[&grid.group_row[g]];
        r1[g] = row_of[&(grid.group_row[g] + grid.group_rows[g] - 1)];
        let max_w = if tight { (title_room(options) / 2.0).max(80.0) } else { 320.0 };
        let label = layout_label(groups[g].title.as_deref(), true, measurer, size, max_w, 0);
        let icon = if groups[g].icon.as_deref().is_some_and(|s| !s.is_empty()) { badge } else { 0.0 };
        head_w[g] = label.width + icon + if label.width > 0.0 && icon > 0.0 { 6.0 } else { 0.0 };
        head_h[g] = label.height.max(icon);
        group_labels.push(label);
    }
    for g in (0..g_len).rev() {
        let left_pad = group_pad + ins_l[g];
        let mut right_pad = group_pad + ins_r[g];
        let top_pad = (if head_h[g] > 0.0 { group_pad.max(head_h[g] + 12.0) } else { group_pad }) + ins_t[g];
        let bottom_pad = group_pad + ins_b[g];
        if head_w[g] > 0.0 {
            let mut inner = left_pad + right_pad;
            for c in c0[g]..=c1[g] {
                inner += col_w[c] + if c < c1[g] { col_r[c] + gap_x[c] + col_l[c + 1] } else { 0.0 };
            }
            if head_w[g] + 20.0 > inner {
                right_pad += (head_w[g] + 20.0 - inner).ceil();
            }
        }
        ins_l[g] = left_pad;
        ins_r[g] = right_pad;
        ins_t[g] = top_pad;
        ins_b[g] = bottom_pad;
        col_l[c0[g]] = col_l[c0[g]].max(left_pad);
        col_r[c1[g]] = col_r[c1[g]].max(right_pad);
        row_t[r0[g]] = row_t[r0[g]].max(top_pad);
        row_b[r1[g]] = row_b[r1[g]].max(bottom_pad);
        let p = grid.group_parent[g];
        if p < 0 {
            continue;
        }
        let pu = p as usize;
        if c0[pu] == c0[g] {
            ins_l[pu] = ins_l[pu].max(left_pad);
        }
        if c1[pu] == c1[g] {
            ins_r[pu] = ins_r[pu].max(right_pad);
        }
        if r0[pu] == r0[g] {
            ins_t[pu] = ins_t[pu].max(top_pad);
        }
        if r1[pu] == r1[g] {
            ins_b[pu] = ins_b[pu].max(bottom_pad);
        }
    }

    let mut x_pos = vec![0.0f64; c_len];
    let mut y_pos = vec![0.0f64; r_len];
    let mut inner_w = 0.0f64;
    for c in 0..c_len {
        x_pos[c] = inner_w + col_l[c];
        inner_w = x_pos[c] + col_w[c] + col_r[c] + if c + 1 < c_len { gap_x[c] } else { 0.0 };
    }
    let mut inner_h = 0.0f64;
    for r in 0..r_len {
        y_pos[r] = inner_h + row_t[r];
        inner_h = y_pos[r] + row_h[r] + row_b[r] + if r + 1 < r_len { gap_y[r] } else { 0.0 };
    }
    let mut min_x = 0.0f64;
    let mut min_y = 0.0f64;
    let mut max_x = inner_w;
    let mut max_y = inner_h;

    let left = |g: usize| x_pos[c0[g]] - ins_l[g];
    let right = |g: usize| x_pos[c1[g]] + col_w[c1[g]] + ins_r[g];
    let top = |g: usize| y_pos[r0[g]] - ins_t[g];
    let bottom = |g: usize| y_pos[r1[g]] + row_h[r1[g]] + ins_b[g];
    let center_x = |i: usize| x_pos[node_col[i]] + col_w[node_col[i]] / 2.0;

    let make_port = |i: usize, side: usize, via_group: bool| -> Port {
        let dx = DX[side];
        let dy = DY[side];
        let cx = center_x(i);
        let y = y_pos[node_row[i]];
        let mut p = Port {
            x: cx,
            y: y + half,
            dx,
            dy,
            x0: cx - box_w[i] / 2.0,
            y0: y,
            x1: cx + box_w[i] / 2.0,
            y1: y + box_h[i],
            gap: 0.0,
            wire: false,
        };
        if nodes[i].is_junction() {
            p.x0 = cx;
            p.x1 = cx;
            p.y0 = p.y;
            p.y1 = p.y;
            p.gap = DOT;
        } else if dx != 0.0 {
            p.x = cx + dx * half;
        } else {
            p.y = if dy < 0.0 {
                y
            } else {
                y + box_h[i] + if labels[i].height > 0.0 { 2.0 } else { 0.0 }
            };
        }
        let g = if via_group { grid.node_parent[i] } else { -1 };
        if g >= 0 {
            let gu = g as usize;
            p.x0 = left(gu);
            p.x1 = right(gu);
            p.y0 = top(gu);
            p.y1 = bottom(gu);
            p.gap = 0.0;
            p.wire = nodes[i].is_junction();
            if dx != 0.0 {
                p.x = if dx < 0.0 { p.x0 } else { p.x1 };
            } else {
                p.y = if dy < 0.0 { p.y0 } else { p.y1 };
            }
        }
        p
    };

    let mut edges_out = String::new();
    let mut labels_out = String::new();
    // Vertical runs of the edges, to keep group titles out from under them.
    let mut runs: Vec<f64> = Vec::new();
    let trim_length = marker_trim("arrow_point");
    for k in 0..e_len {
        let edge = &model.edges[k];
        let a = ends[k * 2];
        let b = ends[k * 2 + 1];
        if a < 0 || b < 0 {
            continue;
        }
        let (au, bu) = (a as usize, b as usize);
        let mut s = make_port(au, side_index(edge.lhs_dir), edge.lhs_group);
        let mut e = make_port(bu, side_index(edge.rhs_dir), edge.rhs_group);
        if s.x == e.x && s.y == e.y && s.dx == e.dx {
            // An edge from a side back to the same side: part the two ends so it shows as a loop.
            s.x -= 8.0 * s.dy.abs();
            s.y -= 8.0 * s.dx.abs();
            e.x += 8.0 * s.dy.abs();
            e.y += 8.0 * s.dx.abs();
        }
        let mut routed = route(&s, &e);
        if s.wire {
            routed.splice(0..0, [center_x(au), y_pos[node_row[au]] + half]);
        }
        if e.wire {
            routed.push(center_x(bu));
            routed.push(y_pos[node_row[bu]] + half);
        }
        let points = simplify(&routed);
        if points.len() < 4 {
            continue;
        }
        let raw_id = format!("{}-{}", edge.lhs_id, edge.rhs_id);
        let id = esc_text(&raw_id);
        let start_trim = if edge.lhs_into && !s.wire { trim_length + s.gap } else { 0.0 };
        let end_trim = if edge.rhs_into && !e.wire { trim_length + e.gap } else { 0.0 };
        let lhs_marker = if edge.lhs_into {
            marker("arrow_point", s.x + s.dx * s.gap, s.y + s.dy * s.gap, -s.dx, -s.dy, "var(--_l)", "none")
        } else {
            String::new()
        };
        let rhs_marker = if edge.rhs_into {
            marker("arrow_point", e.x + e.dx * e.gap, e.y + e.dy * e.gap, -e.dx, -e.dy, "var(--_l)", "none")
        } else {
            String::new()
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge\" data-id=\"{}\"><path d=\"{}\"/>{}{}</g>",
            id,
            path_data(&points, start_trim, end_trim),
            lhs_marker,
            rhs_marker
        ));

        let mut best = 0usize;
        let mut best_length = -1.0f64;
        let mut flat = 0usize;
        let mut flat_length = -1.0f64;
        let label = &edge_labels[k];
        let mut i = 0;
        while i + 1 < points.len() {
            min_x = min_x.min(points[i] - 4.0);
            max_x = max_x.max(points[i] + 4.0);
            min_y = min_y.min(points[i + 1] - 4.0);
            max_y = max_y.max(points[i + 1] + 4.0);
            i += 2;
        }
        let mut i = 0;
        while i + 3 < points.len() {
            let horizontal = (points[i + 1] - points[i + 3]).abs() < 0.01;
            let length = (points[i] - points[i + 2]).abs() + (points[i + 1] - points[i + 3]).abs();
            if !horizontal {
                runs.extend([
                    points[i],
                    points[i + 1].min(points[i + 3]),
                    points[i + 1].max(points[i + 3]),
                ]);
            }
            if length > best_length {
                best = i;
                best_length = length;
            }
            if horizontal && length > flat_length {
                flat = i;
                flat_length = length;
            }
            i += 2;
        }
        if label.width > 0.0 {
            let at = if flat_length >= label.width + 8.0 { flat } else { best };
            let mut x = (points[at] + points[at + 2]) / 2.0;
            let mut y = (points[at + 1] + points[at + 3]) / 2.0;
            let w = label.width + 8.0;
            let upright = (points[at] - points[at + 2]).abs() < 0.01;
            // On a short run next to a node the label would land on the node, so it moves off to the side.
            for p in [&s, &e] {
                if x + w / 2.0 <= p.x0
                    || x - w / 2.0 >= p.x1
                    || y + label.height / 2.0 <= p.y0
                    || y - label.height / 2.0 >= p.y1
                {
                    continue;
                }
                if upright {
                    x = if x >= (p.x0 + p.x1) / 2.0 {
                        p.x1 + w / 2.0 + 4.0
                    } else {
                        p.x0 - w / 2.0 - 4.0
                    };
                } else {
                    y = if y >= (p.y0 + p.y1) / 2.0 {
                        p.y1 + label.height / 2.0 + 4.0
                    } else {
                        p.y0 - label.height / 2.0 - 4.0
                    };
                }
            }
            min_x = min_x.min(x - w / 2.0);
            max_x = max_x.max(x + w / 2.0);
            min_y = min_y.min(y - label.height / 2.0);
            max_y = max_y.max(y + label.height / 2.0);
            labels_out.push_str(&edge_label_svg(&raw_id, label, x, y, ""));
        }
    }

    let mut clusters = String::new();
    for g in 0..g_len {
        let group = &groups[g];
        let x = left(g);
        let y = top(g);
        let w = right(g) - x;
        let label = &group_labels[g];
        let mut head = String::new();
        if head_w[g] > 0.0 {
            let band = y + head_h[g] + 12.0;
            let mut crossings: Vec<f64> = Vec::new();
            let mut i = 0;
            while i + 2 < runs.len() {
                if runs[i + 1] < band && runs[i + 2] > y && runs[i] > x && runs[i] < x + w {
                    crossings.push(runs[i]);
                }
                i += 3;
            }
            crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            crossings.push(x + w - 4.0);
            let mut start = x + 10.0;
            for crossing in crossings {
                if crossing - 6.0 - start >= head_w[g] {
                    break;
                }
                start = start.max(crossing + 6.0);
            }
            if start + head_w[g] > x + w - 4.0 {
                start = x + 10.0;
            }
            let cy = y + 6.0 + head_h[g] / 2.0;
            let mut text_x = start;
            if let Some(icon) = group.icon.as_deref().filter(|s| !s.is_empty()) {
                head.push_str(&icon_slot(icon, start, cy - badge / 2.0, badge));
                text_x += badge + 6.0;
            }
            head.push_str(&label_svg(
                label,
                text_x + label.width / 2.0,
                cy,
                " class=\"pele-cluster-label\" fill=\"var(--_m)\"",
            ));
        }
        clusters.push_str(&format!(
            "<g class=\"pele-cluster\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\" fill-opacity=\"0.5\" stroke=\"var(--_b)\"{}/>{}</g>",
            esc_text(&group.id),
            num(x),
            num(y),
            num(w),
            num(bottom(g) - y),
            ROUNDED,
            head
        ));
    }

    let mut nodes_out = String::new();
    let glyph = icon_size * 0.5;
    let text_size = js_round(size * 0.75).max(6.0);
    for i in 0..n {
        let node = &nodes[i];
        let at = format!(
            "data-id=\"{}\" transform=\"translate({},{})\"",
            esc_text(node.id()),
            num(center_x(i)),
            num(y_pos[node_row[i]])
        );
        match node {
            ArchNode::Junction { .. } => {
                nodes_out.push_str(&format!(
                    "<g class=\"pele-node pele-junction\" {}><circle cy=\"{}\" r=\"{}\" fill=\"var(--_l)\"/></g>",
                    at,
                    num(half),
                    js_number(DOT)
                ));
            }
            ArchNode::Service { icon, icon_text, .. } => {
                let mut inner = String::new();
                if let Some(icon_name) = icon.as_deref().filter(|s| !s.is_empty()) {
                    inner = icon_slot(icon_name, -glyph / 2.0, (icon_size - glyph) / 2.0, glyph);
                } else if let Some(txt) = icon_text.as_deref().filter(|s| !s.is_empty()) {
                    let text = layout_label(Some(txt), false, measurer, text_size, icon_size - 8.0, 0);
                    let lh = if text.line_height == 0.0 { 1.0 } else { text.line_height };
                    let max_lines = (((icon_size - 4.0) / lh).floor() as usize).max(1);
                    let fitted = fit(&text, icon_size - 8.0, max_lines, measurer);
                    inner = label_svg(
                        &fitted,
                        0.0,
                        half,
                        &format!(" class=\"pele-icon-text\" font-size=\"{}\"", js_number(text_size)),
                    );
                }
                nodes_out.push_str(&format!(
                    "<g class=\"pele-node pele-service\" {}><rect x=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>{}{}</g>",
                    at,
                    num(-half),
                    num(icon_size),
                    num(icon_size),
                    ROUNDED,
                    inner,
                    label_svg(
                        &labels[i],
                        0.0,
                        icon_size + LABEL_GAP + labels[i].height / 2.0,
                        " class=\"pele-label\"",
                    )
                ));
            }
        }
    }

    let title_max = if tight { title_room(options) } else { 4000.0 };
    let title = layout_label(model.title.as_deref(), false, measurer, size, title_max, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let width = ((max_x - min_x).max(title.width) + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let ox = js_round((width - (max_x - min_x)) / 2.0 - min_x);
    let oy = js_round(pad + title_height - min_y);

    let mut body = String::new();
    if !clusters.is_empty() {
        body.push_str(&format!("<g class=\"pele-clusters\">{}</g>", clusters));
    }
    if !edges_out.is_empty() {
        body.push_str(&format!(
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{}</g>",
            edges_out
        ));
    }
    if !labels_out.is_empty() {
        body.push_str(&format!(
            "<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>",
            js_number(small),
            labels_out
        ));
    }
    if !nodes_out.is_empty() {
        body.push_str(&format!("<g class=\"pele-nodes\">{}</g>", nodes_out));
    }

    let mut content = label_svg(
        &title,
        width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    );
    if !body.is_empty() {
        content.push_str(&format!(
            "<g transform=\"translate({},{})\">{}</g>",
            js_number(ox),
            js_number(oy),
            body
        ));
    }

    let fixed = config.get("architecture").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "architecture",
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
