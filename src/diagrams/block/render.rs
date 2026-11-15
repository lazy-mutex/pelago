use std::collections::{HashMap, HashSet};
use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, FRAC_PI_4, PI};

use crate::errors::Result;
use crate::options::{title_room, RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num, sign};
use crate::svg::edges::{
    casings_svg, edge_casing, edge_label_svg, marker, marker_trim, over_fills, EdgePath, Fill, ARROW,
};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::shapes::{draw_shape, shape_filled, shape_inset, shape_size};
use crate::svg::theme::{class_names, resolve_style, rounded, sets_fill, ResolvedStyle};
use crate::svg::title::title_label;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

use super::arrows::{arrow_axis, arrow_mask, arrow_size, draw_arrow, OVERHANG};
use super::db::{Block, BlockDb};
use super::layout::{layout_cells, Cell};

const GAP: f64 = 12.0;
const WRAP: f64 = 200.0;
const CORNER: f64 = 8.0;
const SHAPE_ATTRS: &str = " fill=\"var(--_s)\" stroke=\"var(--_b)\"";
const LINE_ATTRS: &str = " stroke=\"var(--_b)\"";
const OBSTACLE_BUDGET: usize = 2_000_000;
const LABEL_GAP: f64 = 4.0;
const SPOTS: [f64; 5] = [0.5, 0.3, 0.7, 0.15, 0.85];

fn map_shape(type_name: Option<&str>) -> &'static str {
    match type_name {
        Some("square") => "rect",
        Some("round") => "rounded",
        Some("circle") => "circle",
        Some("doublecircle") => "dbl-circ",
        Some("diamond") => "diam",
        Some("hexagon") => "hex",
        Some("stadium") => "stadium",
        Some("subroutine") => "fr-rect",
        Some("cylinder") => "cyl",
        Some("rect_left_inv_arrow") => "odd",
        Some("lean_right") => "lean-r",
        Some("lean_left") => "lean-l",
        Some("trapezoid") => "trap-b",
        Some("inv_trapezoid") => "trap-t",
        _ => "rect",
    }
}

fn is_fixed_shape(shape: &str) -> bool {
    matches!(shape, "circle" | "dbl-circ" | "diam")
}

fn is_wide_shape(shape: &str) -> bool {
    matches!(shape, "stadium" | "hex" | "odd" | "lean-r" | "lean-l" | "trap-b" | "trap-t")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Leaf,
    Arrow,
    Composite,
    Space,
}

#[derive(Clone)]
struct View {
    arena_idx: usize,
    kind: Kind,
    #[allow(dead_code)]
    index: usize,
    shape: &'static str,
    label: Label,
    style: ResolvedStyle,
    mask: u8,
    dx: f64,
    dy: f64,
    bw: f64,
    bh: f64,
}

#[derive(Clone, Copy)]
struct ObstacleBox {
    view_idx: Option<usize>,
    x: f64,
    y: f64,
    bw: f64,
    bh: f64,
}

struct Route {
    edge_idx: usize,
    points: Vec<f64>,
    loop_k: usize,
    label: Label,
    lx: f64,
    ly: f64,
}

fn is_round(v: &View) -> bool {
    v.kind == Kind::Leaf && matches!(v.shape, "circle" | "dbl-circ")
}

fn edge_y(v: &View, cell: &Cell, x: f64, dir_sign: f64) -> f64 {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let dx = (x - cell.x).abs();
    let reach = if is_round(v) {
        (hw * hw - dx * dx).max(0.0).sqrt()
    } else if v.kind == Kind::Leaf && v.shape == "diam" {
        hh * (1.0 - dx / hw).max(0.0)
    } else if v.kind == Kind::Leaf {
        hh - shape_inset(v.shape, v.bw, v.bh, if dir_sign > 0.0 { 2 } else { 0 }, 0.0)
    } else if v.kind == Kind::Arrow && arrow_axis(v.mask) == "x" {
        hh - OVERHANG
    } else {
        hh
    };
    cell.y + dir_sign * reach
}

fn edge_x(v: &View, cell: &Cell, y: f64, dir_sign: f64) -> f64 {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let dy = (y - cell.y).abs();
    let reach = if is_round(v) {
        (hh * hh - dy * dy).max(0.0).sqrt()
    } else if v.kind == Kind::Leaf && v.shape == "diam" {
        hw * (1.0 - dy / hh).max(0.0)
    } else if v.kind == Kind::Leaf {
        hw - shape_inset(v.shape, v.bw, v.bh, if dir_sign > 0.0 { 1 } else { 3 }, 0.0)
    } else if v.kind == Kind::Arrow && arrow_axis(v.mask) == "y" {
        hw - OVERHANG
    } else {
        hw
    };
    cell.x + dir_sign * reach
}

fn border(v: &View, cell: &Cell, tx: f64, ty: f64, out: &mut Vec<f64>) {
    let dx = tx - cell.x;
    let dy = ty - cell.y;
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let mut k = if is_round(v) {
        let len = dx.hypot(dy);
        hw / if len == 0.0 { 1.0 } else { len }
    } else if v.kind == Kind::Leaf && v.shape == "diam" {
        let denom = dx.abs() / hw + dy.abs() / hh;
        1.0 / if denom == 0.0 { 1.0 } else { denom }
    } else {
        let kx = if dx == 0.0 { f64::INFINITY } else { hw / dx.abs() };
        let ky = if dy == 0.0 { f64::INFINITY } else { hh / dy.abs() };
        kx.min(ky)
    };
    if !k.is_finite() {
        k = 0.0;
    }
    out.push(cell.x + dx * k);
    out.push(cell.y + dy * k);
}

fn crosses_box(x1: f64, y1: f64, x2: f64, y2: f64, obs: &ObstacleBox) -> bool {
    let l = obs.x - obs.bw / 2.0 + 1.0;
    let r = obs.x + obs.bw / 2.0 - 1.0;
    let t = obs.y - obs.bh / 2.0 + 1.0;
    let b = obs.y + obs.bh / 2.0 - 1.0;
    if (x1 < l && x2 < l) || (x1 > r && x2 > r) || (y1 < t && y2 < t) || (y1 > b && y2 > b) {
        return false;
    }
    if x1 == x2 || y1 == y2 {
        return true;
    }
    let dx = x2 - x1;
    let dy = y2 - y1;
    let side = |x: f64, y: f64| -> f64 { sign(dx * (y - y1) - dy * (x - x1)) };
    let first = side(l, t);
    first == 0.0 || side(r, t) != first || side(r, b) != first || side(l, b) != first
}

fn hits(points: &[f64], s_idx: usize, t_idx: usize, obstacles: &[ObstacleBox]) -> usize {
    let mut count = 0usize;
    for obs in obstacles {
        if obs.view_idx == Some(s_idx) || obs.view_idx == Some(t_idx) {
            continue;
        }
        let mut i = 0;
        while i + 3 < points.len() {
            if crosses_box(points[i], points[i + 1], points[i + 2], points[i + 3], obs) {
                count += 1;
                break;
            }
            i += 2;
        }
    }
    count
}

fn route_edge(
    views: &[View],
    cells: &[Cell],
    s_idx: usize,
    t_idx: usize,
    shift: f64,
    obstacles: Option<&[ObstacleBox]>,
) -> Vec<f64> {
    let (s, sc) = (&views[s_idx], &cells[s_idx]);
    let (t, tc) = (&views[t_idx], &cells[t_idx]);
    let sl = sc.x - s.bw / 2.0;
    let sr = sc.x + s.bw / 2.0;
    let st = sc.y - s.bh / 2.0;
    let sb = sc.y + s.bh / 2.0;
    let tl = tc.x - t.bw / 2.0;
    let tr = tc.x + t.bw / 2.0;
    let tt = tc.y - t.bh / 2.0;
    let tb = tc.y + t.bh / 2.0;
    let x1 = sl.max(tl);
    let x2 = sr.min(tr);
    let y1 = st.max(tt);
    let y2 = sb.min(tb);
    let over_x = x2 - x1 > 6.0;
    let over_y = y2 - y1 > 6.0;

    if over_x && over_y {
        let s_inner = s.bw * s.bh <= t.bw * t.bh;
        let (inner, ic, outer, oc) = if s_inner {
            (s, sc, t, tc)
        } else {
            (t, tc, s, sc)
        };
        let up = ic.y - inner.bh / 2.0 - (oc.y - outer.bh / 2.0);
        let down = oc.y + outer.bh / 2.0 - (ic.y + inner.bh / 2.0);
        let left = ic.x - inner.bw / 2.0 - (oc.x - outer.bw / 2.0);
        let right = oc.x + outer.bw / 2.0 - (ic.x + inner.bw / 2.0);
        let least = up.min(down).min(left).min(right);
        let points = if least == down {
            vec![ic.x, edge_y(inner, ic, ic.x, 1.0), ic.x, oc.y + outer.bh / 2.0]
        } else if least == up {
            vec![ic.x, edge_y(inner, ic, ic.x, -1.0), ic.x, oc.y - outer.bh / 2.0]
        } else if least == right {
            vec![edge_x(inner, ic, ic.y, 1.0), ic.y, oc.x + outer.bw / 2.0, ic.y]
        } else {
            vec![edge_x(inner, ic, ic.y, -1.0), ic.y, oc.x - outer.bw / 2.0, ic.y]
        };
        return if s_inner {
            points
        } else {
            vec![points[2], points[3], points[0], points[1]]
        };
    }
    if over_x {
        let margin = ((x2 - x1) / 2.0).min(6.0);
        let x = ((x1 + x2) / 2.0 + shift).max(x1 + margin).min(x2 - margin);
        let down = if tc.y > sc.y { 1.0 } else { -1.0 };
        return vec![x, edge_y(s, sc, x, down), x, edge_y(t, tc, x, -down)];
    }
    if over_y {
        let margin = ((y2 - y1) / 2.0).min(6.0);
        let y = ((y1 + y2) / 2.0 + shift).max(y1 + margin).min(y2 - margin);
        let right = if tc.x > sc.x { 1.0 } else { -1.0 };
        return vec![edge_x(s, sc, y, right), y, edge_x(t, tc, y, -right), y];
    }

    let right = if tc.x > sc.x { 1.0 } else { -1.0 };
    let down = if tc.y > sc.y { 1.0 } else { -1.0 };
    let across = vec![
        edge_x(s, sc, sc.y, right),
        sc.y,
        tc.x,
        sc.y,
        tc.x,
        edge_y(t, tc, tc.x, -down),
    ];
    let along = vec![
        sc.x,
        edge_y(s, sc, sc.x, down),
        sc.x,
        tc.y,
        edge_x(t, tc, tc.y, -right),
        tc.y,
    ];
    let wide = (tc.x - sc.x).abs() >= (tc.y - sc.y).abs();
    let (mut best, other) = if wide { (across, along) } else { (along, across) };
    let Some(obstacles) = obstacles else { return best };
    let mut least = hits(&best, s_idx, t_idx, obstacles);
    if least == 0 {
        return best;
    }
    let count = hits(&other, s_idx, t_idx, obstacles);
    if count < least {
        best = other;
        least = count;
    }
    if least == 0 {
        return best;
    }
    let mut direct = Vec::with_capacity(4);
    border(s, sc, tc.x, tc.y, &mut direct);
    border(t, tc, sc.x, sc.y, &mut direct);
    if hits(&direct, s_idx, t_idx, obstacles) < least {
        direct
    } else {
        best
    }
}

fn trace(p: &[f64], mut start_trim: f64, mut end_trim: f64) -> EdgePath {
    let n = p.len();
    let sx = p[0];
    let sy = p[1];
    let ex = p[n - 2];
    let ey = p[n - 1];
    let mut ax = p[2] - sx;
    let mut ay = p[3] - sy;
    let first = ax.hypot(ay);
    let f_div = if first == 0.0 { 1.0 } else { first };
    ax /= f_div;
    ay /= f_div;
    let mut bx = ex - p[n - 4];
    let mut by = ey - p[n - 3];
    let last = bx.hypot(by);
    let l_div = if last == 0.0 { 1.0 } else { last };
    bx /= l_div;
    by /= l_div;
    if n == 4 && start_trim + end_trim > first {
        let scale = first / (start_trim + end_trim);
        start_trim *= scale;
        end_trim *= scale;
    }
    start_trim = start_trim.min(first);
    end_trim = end_trim.min(last);
    let mut d = format!("M{},{}", num(sx + ax * start_trim), num(sy + ay * start_trim));
    if n == 6 {
        let r = CORNER.min(first - start_trim).min(last - end_trim).max(0.0);
        d.push_str(&format!(
            "L{},{}Q{},{} {},{}",
            num(p[2] - ax * r),
            num(p[3] - ay * r),
            num(p[2]),
            num(p[3]),
            num(p[2] + bx * r),
            num(p[3] + by * r),
        ));
    }
    d.push_str(&format!("L{},{}", num(ex - bx * end_trim), num(ey - by * end_trim)));
    EdgePath {
        d,
        tokens: None,
        sx,
        sy,
        sdx: -ax,
        sdy: -ay,
        ex,
        ey,
        edx: bx,
        edy: by,
    }
}

fn loop_arc(v: &View, cell: &Cell, k: usize) -> [f64; 5] {
    let hw = v.bw / 2.0;
    let hh = v.bh / 2.0;
    let r = (10.0 + 5.0 * ((k - 1) as f64)).min(hw).min(hh).max(1.0);
    if is_round(v) {
        let px = cell.x + hw * FRAC_1_SQRT_2;
        let py = cell.y - hw * FRAC_1_SQRT_2;
        let half = 2.0 * (r / (2.0 * hw)).min(1.0).asin();
        let angle = |at: f64| -> f64 { (cell.y - hw * at.sin() - py).atan2(cell.x + hw * at.cos() - px) };
        return [px, py, r, angle(FRAC_PI_4 + half), angle(FRAC_PI_4 - half)];
    }
    if v.kind == Kind::Leaf && v.shape == "diam" {
        let slope = hh.atan2(hw);
        return [cell.x + hw / 2.0, cell.y - hh / 2.0, r, slope + PI, slope];
    }
    [cell.x + hw, cell.y - hh, r, PI, FRAC_PI_2]
}

fn loop_path_arc(v: &View, cell: &Cell, k: usize, start_trim: f64, end_trim: f64) -> EdgePath {
    let [px, py, r, a1, a2] = loop_arc(v, cell, k);
    let from = a1 + start_trim / r;
    let to = a2 - end_trim / r;
    let mut sweep = (to - from) % (2.0 * PI);
    if sweep <= 0.0 {
        sweep += 2.0 * PI;
    }
    let x1 = px + r * from.cos();
    let y1 = py + r * from.sin();
    let x2 = px + r * to.cos();
    let y2 = py + r * to.sin();
    let sx = px + r * a1.cos();
    let sy = py + r * a1.sin();
    let ex = px + r * a2.cos();
    let ey = py + r * a2.sin();
    let sl = (sx - x1).hypot(sy - y1);
    let el = (ex - x2).hypot(ey - y2);
    EdgePath {
        d: format!(
            "M{},{}A{},{} 0 {} 1 {},{}",
            num(x1),
            num(y1),
            num(r),
            num(r),
            if sweep > PI { 1 } else { 0 },
            num(x2),
            num(y2),
        ),
        tokens: None,
        sx,
        sy,
        sdx: if sl > 0.0 { (sx - x1) / sl } else { a1.sin() },
        sdy: if sl > 0.0 { (sy - y1) / sl } else { -a1.cos() },
        ex,
        ey,
        edx: if el > 0.0 { (ex - x2) / el } else { -a2.sin() },
        edy: if el > 0.0 { (ey - y2) / el } else { a2.cos() },
    }
}

fn covers(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    s_idx: usize,
    t_idx: usize,
    obstacles: Option<&[ObstacleBox]>,
) -> bool {
    let Some(obstacles) = obstacles else { return false };
    for obs in obstacles {
        if obs.view_idx != Some(s_idx)
            && obs.view_idx != Some(t_idx)
            && (x - obs.x).abs() < (w + obs.bw) / 2.0
            && (y - obs.y).abs() < (h + obs.bh) / 2.0
        {
            return true;
        }
    }
    false
}

fn label_spot(
    p: &[f64],
    at: usize,
    w: f64,
    h: f64,
    s_idx: usize,
    t_idx: usize,
    obstacles: Option<&[ObstacleBox]>,
) -> f64 {
    if obstacles.is_none() {
        return 0.5;
    }
    for spot in SPOTS {
        let x = p[at] + (p[at + 2] - p[at]) * spot;
        let y = p[at + 1] + (p[at + 3] - p[at + 1]) * spot;
        if !covers(x, y, w, h, s_idx, t_idx, obstacles) {
            return spot;
        }
    }
    0.5
}

fn stack_labels(
    stack: &[usize],
    routes: &mut [Route],
    legs: &HashMap<usize, [f64; 4]>,
    s_idx: usize,
    t_idx: usize,
    obstacles: Option<&[ObstacleBox]>,
) -> f64 {
    let first_leg = legs[&stack[0]];
    let vertical = first_leg[0] == first_leg[2];
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    let mut need = -LABEL_GAP;
    let mut crowded = false;
    for a in 0..stack.len() {
        let leg = legs[&stack[a]];
        if if vertical { leg[0] != leg[2] } else { leg[1] != leg[3] } {
            return 0.0;
        }
        lo = lo.max(if vertical { leg[1].min(leg[3]) } else { leg[0].min(leg[2]) });
        hi = hi.min(if vertical { leg[1].max(leg[3]) } else { leg[0].max(leg[2]) });
        let p = &routes[stack[a]];
        need += (if vertical { p.label.height } else { p.label.width + 8.0 }) + LABEL_GAP;
        for &b_route_idx in &stack[..a] {
            let q = &routes[b_route_idx];
            if (p.lx - q.lx).abs() < (p.label.width + q.label.width) / 2.0 + 8.0
                && (p.ly - q.ly).abs() < (p.label.height + q.label.height) / 2.0
            {
                crowded = true;
            }
        }
    }
    if !crowded {
        return 0.0;
    }
    if need > hi - lo - 2.0 * ARROW {
        return need - (hi - lo - 2.0 * ARROW);
    }
    let mut along = Vec::with_capacity(stack.len());
    let mut next = (lo + hi - need) / 2.0;
    for &i in stack {
        let r = &routes[i];
        let sz = if vertical { r.label.height } else { r.label.width + 8.0 };
        let at = next + sz / 2.0;
        if covers(
            if vertical { r.lx } else { at },
            if vertical { at } else { r.ly },
            r.label.width + 8.0,
            r.label.height,
            s_idx,
            t_idx,
            obstacles,
        ) {
            return 0.0;
        }
        along.push(at);
        next += sz + LABEL_GAP;
    }
    for (k, &i) in stack.iter().enumerate() {
        if vertical {
            routes[i].ly = along[k];
        } else {
            routes[i].lx = along[k];
        }
    }
    0.0
}

fn widen(
    parents: &[Option<usize>],
    cells: &mut [Cell],
    s_idx: usize,
    t_idx: usize,
    vertical: bool,
    short: f64,
    base: &mut HashMap<(usize, bool), Vec<f64>>,
) -> bool {
    let mut chain = HashSet::new();
    let mut cur = Some(s_idx);
    while let Some(v) = cur {
        chain.insert(v);
        cur = parents[v];
    }
    let mut tb_opt: Option<usize> = None;
    let mut box_opt = Some(t_idx);
    while let Some(bx) = box_opt {
        if chain.contains(&bx) {
            break;
        }
        tb_opt = Some(bx);
        box_opt = parents[bx];
    }
    let (Some(box_idx), Some(tb)) = (box_opt, tb_opt) else {
        return false;
    };
    if box_idx == s_idx {
        return false;
    }
    let mut sb = s_idx;
    while parents[sb] != Some(box_idx) {
        let Some(p) = parents[sb] else { return false };
        sb = p;
    }
    let at = if vertical {
        if cells[sb].row == cells[tb].row {
            return false;
        }
        cells[sb].row.max(cells[tb].row) - 1
    } else {
        let (l, r) = if cells[sb].col < cells[tb].col {
            (sb, tb)
        } else {
            (tb, sb)
        };
        if cells[r].col < cells[l].col + cells[l].span {
            return false;
        }
        cells[r].col - 1
    };
    let gaps = if vertical {
        &mut cells[box_idx].row_gaps
    } else {
        &mut cells[box_idx].col_gaps
    };
    let orig = base.entry((box_idx, vertical)).or_insert_with(|| gaps.clone());
    if gaps.len() <= at {
        gaps.resize(at + 1, 0.0);
    }
    let orig_val = orig.get(at).copied().unwrap_or(0.0);
    gaps[at] = gaps[at].max(orig_val + short);
    true
}

pub fn render_block(db: &BlockDb, config: &Value, options: &RenderOptions) -> Result<Rendered> {
    let loose = draw(db, config, options, false)?;
    let Some(room) = options.max_width.filter(|&w| w > 0.0) else {
        return Ok(loose);
    };
    if loose.width <= room {
        return Ok(loose);
    }
    let tight = draw(db, config, options, true)?;
    Ok(if tight.width < loose.width { tight } else { loose })
}

fn draw(db: &BlockDb, config: &Value, options: &RenderOptions, tight: bool) -> Result<Rendered> {
    let size = options.font_size.unwrap_or(16.0);
    let edge_size = js_round(size * 0.875);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let base_gap = match config.get("block").get("padding").as_f64() {
        Some(c) if c >= 0.0 => c.min(200.0),
        _ => GAP,
    };
    let gap = if tight { base_gap.min(6.0) } else { base_gap };
    let wrap = if tight { 80.0 } else { WRAP };

    let style_of = |block: &Block| -> ResolvedStyle {
        let mut declarations = Vec::new();
        let mut add = |styles: &[String]| {
            for st in styles {
                for part in st.split(';') {
                    declarations.push(part.to_string());
                }
            }
        };
        if block.classes.is_empty() {
            if let Some(def) = db.classes.get("default") {
                add(&def.styles);
            }
        } else {
            for name in &block.classes {
                if let Some(def) = db.classes.get(name) {
                    add(&def.styles);
                }
            }
        }
        if !block.styles.is_empty() {
            add(&block.styles);
        }
        resolve_style(&declarations)
    };

    let make_view = |arena_idx: usize, index: usize| -> (View, Cell) {
        let block = &db.arena[arena_idx];
        let btype = block.block_type.as_deref();
        let st = if btype == Some("space") {
            resolve_style(&[] as &[&str])
        } else {
            style_of(block)
        };
        let text_style = (if st.bold { style::BOLD } else { 0 }) | (if st.italic { style::ITALIC } else { 0 });
        let font_size = st.font_size.unwrap_or(size);
        let mut kind = Kind::Leaf;
        let mut shape: &'static str = "";
        let label: Label;
        let mut nw = 0.0f64;
        let mut nh = 0.0f64;
        let mut head = 0.0f64;
        let mut mask = 0u8;
        let mut dx = 0.0f64;
        let mut dy = 0.0f64;

        if btype == Some("space") {
            kind = Kind::Space;
            label = layout_label(None, false, measurer, font_size, wrap, 0);
        } else if btype == Some("composite") {
            kind = Kind::Composite;
            let max_w = if tight { title_room(options) } else { 4000.0 };
            label = layout_label(block.label.as_deref(), false, measurer, font_size, max_w, text_style);
            head = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
            nw = label.width + 2.0 * gap;
            nh = head + 2.0 * gap;
        } else if btype == Some("block_arrow") {
            kind = Kind::Arrow;
            label = layout_label(block.label.as_deref(), false, measurer, font_size, wrap, text_style);
            let mut blank = true;
            for line in &label.lines {
                for part in line {
                    if !part.text.trim().is_empty() {
                        blank = false;
                    }
                }
            }
            mask = arrow_mask(&block.directions);
            let s = if blank {
                arrow_size(mask, 0.0, 0.0)
            } else {
                arrow_size(mask, label.width, label.height)
            };
            nw = s.w;
            nh = s.h;
            dx = s.dx;
            dy = s.dy;
        } else {
            shape = map_shape(btype);
            label = layout_label(block.label.as_deref(), false, measurer, font_size, wrap, text_style);
            let s = shape_size(shape, label.width, label.height);
            nw = s.w;
            nh = if shape == "cyl" {
                s.h.max(label.height + 40.0)
            } else {
                s.h
            };
            dy = s.dy;
        }

        let span = if kind == Kind::Space {
            1
        } else {
            block.width_in_columns.unwrap_or(1).max(1) as usize
        };
        let cell = Cell {
            children: Vec::new(),
            columns: if kind == Kind::Composite {
                block.columns.unwrap_or(-1)
            } else {
                -1
            },
            span,
            space: kind == Kind::Space,
            head,
            nw,
            nh,
            ..Default::default()
        };
        let view = View {
            arena_idx,
            kind,
            index,
            shape,
            label,
            style: st,
            mask,
            dx,
            dy,
            bw: 0.0,
            bh: 0.0,
        };
        (view, cell)
    };

    let (root_view, mut root_cell) = make_view(0, 0);
    root_cell.nw = 0.0;
    root_cell.nh = 0.0;

    let mut views = vec![root_view];
    let mut cells = vec![root_cell];
    let mut parents: Vec<Option<usize>> = vec![None];
    let mut by_id: HashMap<String, usize> = HashMap::new();

    let mut idx = 0usize;
    while idx < views.len() {
        if views[idx].kind == Kind::Composite {
            let child_arena_indices = db.arena[views[idx].arena_idx].children.clone();
            for c_arena_idx in child_arena_indices {
                let v_index = views.len();
                let (v, c) = make_view(c_arena_idx, v_index);
                if v.kind != Kind::Space {
                    by_id.insert(db.arena[c_arena_idx].id.clone(), v_index);
                }
                cells[idx].children.push(v_index);
                parents.push(Some(idx));
                views.push(v);
                cells.push(c);
            }
        }
        idx += 1;
    }

    let widths: Vec<f64> = cells.iter().map(|c| c.nw).collect();
    let mut obstacles: Vec<ObstacleBox> = Vec::new();
    let mut titles: Vec<ObstacleBox> = Vec::new();

    let place = |views: &mut [View],
                 cells: &mut [Cell],
                 obstacles: &mut Vec<ObstacleBox>,
                 titles: &mut Vec<ObstacleBox>|
     -> Result<(f64, f64)> {
        for (i, c) in cells.iter_mut().enumerate() {
            c.nw = widths[i];
        }
        let content = layout_cells(cells, gap)?;
        obstacles.clear();
        titles.clear();
        for (i, v) in views.iter_mut().enumerate() {
            let c = &cells[i];
            if v.kind == Kind::Leaf && is_fixed_shape(v.shape) {
                v.bw = c.nw;
                v.bh = c.nh;
            } else if v.kind == Kind::Arrow {
                v.bw = if c.span > 1 { c.w } else { c.nw };
                v.bh = c.nh;
            } else {
                v.bw = c.w;
                v.bh = if v.kind == Kind::Leaf && is_wide_shape(v.shape) {
                    c.nh
                } else {
                    c.h
                };
            }
            if v.kind == Kind::Leaf && v.shape == "cyl" {
                v.dy = (v.bw / 14.0).clamp(6.0, 12.0) / 2.0;
            }
            if v.kind == Kind::Leaf || v.kind == Kind::Arrow {
                obstacles.push(ObstacleBox {
                    view_idx: Some(i),
                    x: c.x,
                    y: c.y,
                    bw: v.bw,
                    bh: v.bh,
                });
            } else if c.head > 0.0 {
                titles.push(ObstacleBox {
                    view_idx: None,
                    x: c.x,
                    y: c.y - c.h / 2.0 + (c.head + gap) / 2.0 + 2.0,
                    bw: v.label.width,
                    bh: v.label.height,
                });
            }
        }
        Ok(content)
    };

    let mut content = place(&mut views, &mut cells, &mut obstacles, &mut titles)?;

    let order_len = views.len();
    let mut pairs: HashMap<usize, usize> = HashMap::new();
    let mut labelled: HashSet<usize> = HashSet::new();
    let mut ends: Vec<(usize, usize)> = Vec::new();
    let mut drawn_edge_indices: Vec<usize> = Vec::new();

    for (e_idx, edge) in db.edges.iter().enumerate() {
        let Some(&s_idx) = edge.start.as_deref().and_then(|id| by_id.get(id)) else {
            continue;
        };
        let Some(&t_idx) = edge.end.as_deref().and_then(|id| by_id.get(id)) else {
            continue;
        };
        let key = s_idx.min(t_idx) * order_len + s_idx.max(t_idx);
        *pairs.entry(key).or_insert(0) += 1;
        if edge.label.as_deref().is_some_and(|l| !l.is_empty()) {
            labelled.insert(key);
        }
        ends.push((s_idx, t_idx));
        drawn_edge_indices.push(e_idx);
    }

    let check_obstacles = drawn_edge_indices.len() * obstacles.len() <= OBSTACLE_BUDGET;
    let mut routes: Vec<Route> = Vec::new();

    let settle = |allow_grow: bool,
                  views: &[View],
                  cells: &mut [Cell],
                  obstacles: &[ObstacleBox],
                  titles: &[ObstacleBox],
                  routes: &mut Vec<Route>|
     -> bool {
        let mut seen: HashMap<usize, usize> = HashMap::new();
        // Keeps insertion order of keys in `stacks`, matching JS `Map.values()`.
        let mut stacks: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut stack_keys: Vec<usize> = Vec::new();
        let mut legs: HashMap<usize, [f64; 4]> = HashMap::new();
        routes.clear();
        let obs_opt = check_obstacles.then_some(obstacles);
        for (i, &e_idx) in drawn_edge_indices.iter().enumerate() {
            let (s_idx, t_idx) = ends[i];
            let key = s_idx.min(t_idx) * order_len + s_idx.max(t_idx);
            let total_pair = pairs.get(&key).copied().unwrap_or(1);
            let k = seen.get(&key).copied().unwrap_or(0);
            seen.insert(key, k + 1);
            let label = layout_label(
                db.edges[e_idx].label.as_deref(),
                false,
                measurer,
                edge_size,
                WRAP,
                0,
            );
            let points: Vec<f64>;
            let lx: f64;
            let ly: f64;
            if s_idx == t_idx {
                let [px, py, r, _, _] = loop_arc(&views[s_idx], &cells[s_idx], k + 1);
                points = Vec::new();
                lx = px + r + 6.0 + label.width / 2.0;
                ly = py - r / 2.0;
            } else {
                let step = if labelled.contains(&key) {
                    js_round(edge_size * 1.5) + 2.0
                } else {
                    14.0
                };
                let shift = ((k as f64) - ((total_pair - 1) as f64) / 2.0) * step;
                points = route_edge(views, cells, s_idx, t_idx, shift, obs_opt);
                let n = points.len();
                let first_leg = (points[2] - points[0]).abs() + (points[3] - points[1]).abs();
                let last_leg = (points[n - 2] - points[n - 4]).abs() + (points[n - 1] - points[n - 3]).abs();
                let at = if n == 6 && last_leg > first_leg { 2 } else { 0 };
                let spot = if label.width > 0.0 {
                    label_spot(
                        &points,
                        at,
                        label.width + 8.0,
                        label.height,
                        s_idx,
                        t_idx,
                        obs_opt,
                    )
                } else {
                    0.5
                };
                lx = points[at] + (points[at + 2] - points[at]) * spot;
                ly = points[at + 1] + (points[at + 3] - points[at + 1]) * spot;
                if label.width > 0.0 && total_pair > 1 {
                    legs.insert(i, [points[at], points[at + 1], points[at + 2], points[at + 3]]);
                    let entry = stacks.entry(key).or_insert_with(|| {
                        stack_keys.push(key);
                        Vec::new()
                    });
                    entry.push(i);
                }
            }
            routes.push(Route {
                edge_idx: e_idx,
                points,
                loop_k: if s_idx == t_idx { k + 1 } else { 0 },
                label,
                lx,
                ly,
            });
        }

        let combined: Option<Vec<ObstacleBox>> = check_obstacles.then(|| {
            let mut all = Vec::with_capacity(obstacles.len() + titles.len());
            all.extend_from_slice(obstacles);
            all.extend_from_slice(titles);
            all
        });
        let mut base: HashMap<(usize, bool), Vec<f64>> = HashMap::new();
        let mut grown = false;
        for key in &stack_keys {
            let stack = &stacks[key];
            if stack.len() < 2 {
                continue;
            }
            let (s_idx, t_idx) = ends[stack[0]];
            let short = stack_labels(stack, routes, &legs, s_idx, t_idx, combined.as_deref());
            let leg0 = legs[&stack[0]];
            let vertical = leg0[0] == leg0[2];
            if allow_grow
                && short > 0.0
                && (vertical || !tight)
                && widen(&parents, cells, s_idx, t_idx, vertical, short.ceil(), &mut base)
            {
                grown = true;
            }
        }
        grown
    };

    let overlap = |routes: &[Route]| -> f64 {
        let mut area = 0.0f64;
        for i in 0..routes.len() {
            let p = &routes[i];
            for q in &routes[i + 1..] {
                let pw = (p.label.width + 8.0) / 2.0;
                let qw = (q.label.width + 8.0) / 2.0;
                let w = (p.lx + pw).min(q.lx + qw) - (p.lx - pw).max(q.lx - qw);
                let h = (p.ly + p.label.height / 2.0).min(q.ly + q.label.height / 2.0)
                    - (p.ly - p.label.height / 2.0).max(q.ly - q.label.height / 2.0);
                if p.label.width > 0.0 && q.label.width > 0.0 && w > 0.0 && h > 0.0 {
                    area += w * h;
                }
            }
        }
        area
    };

    let bound = |content: (f64, f64), views: &[View], cells: &[Cell], routes: &[Route]| -> [f64; 4] {
        let mut min_x = 0.0f64;
        let mut min_y = 0.0f64;
        let mut max_x = content.0;
        let mut max_y = content.1;
        let mut grow_ext = |x: f64, y: f64, w: f64, h: f64| {
            min_x = min_x.min(x - w / 2.0);
            max_x = max_x.max(x + w / 2.0);
            min_y = min_y.min(y - h / 2.0);
            max_y = max_y.max(y + h / 2.0);
        };
        for (i, r) in routes.iter().enumerate() {
            if r.loop_k > 0 {
                let s_idx = ends[i].0;
                let [px, py, rad, _, _] = loop_arc(&views[s_idx], &cells[s_idx], r.loop_k);
                grow_ext(px, py, 2.0 * rad + 2.0, 2.0 * rad + 2.0);
            }
            if r.label.width > 0.0 {
                grow_ext(r.lx, r.ly, r.label.width + 8.0, r.label.height);
            }
        }
        [min_x, min_y, max_x, max_y]
    };

    let can_grow = drawn_edge_indices.len() * drawn_edge_indices.len() <= OBSTACLE_BUDGET;
    if settle(can_grow, &views, &mut cells, &obstacles, &titles, &mut routes) {
        let before = overlap(&routes);
        let ext0 = bound(content, &views, &cells, &routes);
        let narrow = ext0[2] - ext0[0];
        let mut grown = true;
        for round in 0..3 {
            if !grown {
                break;
            }
            content = place(&mut views, &mut cells, &mut obstacles, &mut titles)?;
            grown = settle(round < 2, &views, &mut cells, &obstacles, &titles, &mut routes);
        }
        let ext1 = bound(content, &views, &cells, &routes);
        let wide = ext1[2] - ext1[0];
        let room = options.max_width.filter(|&w| w > 0.0).unwrap_or(f64::INFINITY);
        if overlap(&routes) > before - 1.0 || (wide > narrow && (wide + 2.0 * pad).ceil() > room) {
            for c in &mut cells {
                c.row_gaps.clear();
                c.col_gaps.clear();
            }
            content = place(&mut views, &mut cells, &mut obstacles, &mut titles)?;
            settle(false, &views, &mut cells, &obstacles, &titles, &mut routes);
        }
    }

    let [min_x, min_y, max_x, max_y] = bound(content, &views, &cells, &routes);

    let title_max = if tight { title_room(options) } else { 4000.0 };
    let title = title_label(db.title.as_deref(), measurer, size, title_max);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = (max_x - min_x).max(title.width);
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;
    for c in &mut cells {
        c.x += ox;
        c.y += oy;
    }
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();

    let mut clusters = String::new();
    let mut nodes_out = String::new();
    let mut fills: Vec<Fill> = Vec::new();
    for i in 1..views.len() {
        let v = &views[i];
        if v.kind == Kind::Space {
            continue;
        }
        let c = &cells[i];
        let block = &db.arena[v.arena_idx];
        let classes = class_names(&block.classes.join(" "));
        let id = esc_text(&block.id);
        if v.kind == Kind::Composite {
            let top = c.y - c.h / 2.0;
            if paints(&v.style.shape) {
                fills.push(Fill {
                    bounds: [c.x - c.w / 2.0, top, c.x + c.w / 2.0, top + c.h],
                    shape: v.style.shape.clone(),
                });
            }
            clusters.push_str(&format!(
                "<g class=\"pele-cluster{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\"{} stroke=\"var(--_b)\"{}/>{}</g>",
                classes,
                id,
                num(c.x - c.w / 2.0),
                num(top),
                num(c.w),
                num(c.h),
                if sets_fill(&v.style.shape) { "" } else { " fill-opacity=\"0.5\"" },
                rounded(&v.style.shape),
                label_svg(
                    &v.label,
                    c.x,
                    top + (c.head + gap) / 2.0 + 2.0,
                    &format!(
                        " class=\"pele-cluster-label\"{}{}",
                        ink_fill(&v.style.shape, &v.style.text, Some("var(--_m)")),
                        v.style.text
                    ),
                ),
            ));
            continue;
        }
        let arrow = v.kind == Kind::Arrow;
        let rewrapped;
        let lbl = if c.span > 1
            && v.label.width > 0.0
            && v.label.lines.len() > 1
            && v.bw > c.nw
            && (arrow || !is_fixed_shape(v.shape))
        {
            let text_style =
                (if v.style.bold { style::BOLD } else { 0 }) | (if v.style.italic { style::ITALIC } else { 0 });
            let room = v.label.width + (v.bw - c.nw);
            rewrapped = layout_label(block.label.as_deref(), false, measurer, v.label.size, room, text_style);
            &rewrapped
        } else {
            &v.label
        };
        let shape_svg = if arrow {
            draw_arrow(v.mask, v.bw, v.bh, &format!("{}{}", SHAPE_ATTRS, v.style.shape))
        } else {
            draw_shape(
                v.shape,
                v.bw,
                v.bh,
                &format!("{}{}", SHAPE_ATTRS, v.style.shape),
                &format!("{}{}", LINE_ATTRS, v.style.line),
            )
        };
        let ink_attr = if arrow || shape_filled(v.shape) {
            ink_fill(&v.style.shape, &v.style.text, None)
        } else {
            String::new()
        };
        nodes_out.push_str(&format!(
            "<g class=\"pele-node pele-shape-{}{}\" data-id=\"{}\" transform=\"translate({},{})\">{}{}</g>",
            if arrow { "block-arrow" } else { v.shape },
            classes,
            id,
            num(c.x),
            num(c.y),
            shape_svg,
            label_svg(lbl, v.dx, v.dy, &format!(" class=\"pele-label\"{}{}", ink_attr, v.style.text)),
        ));
    }

    let mut casings = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();
    for (i, r) in routes.iter().enumerate() {
        let edge = &db.edges[r.edge_idx];
        let start_type = edge.arrow_type_start.as_deref().unwrap_or("");
        let end_type = edge.arrow_type_end.as_deref().unwrap_or("");
        let path = if r.loop_k > 0 {
            let s_idx = ends[i].0;
            loop_path_arc(
                &views[s_idx],
                &cells[s_idx],
                r.loop_k,
                marker_trim(start_type),
                marker_trim(end_type),
            )
        } else {
            let mut shifted = r.points.clone();
            let mut k = 0;
            while k < shifted.len() {
                shifted[k] += ox;
                shifted[k + 1] += oy;
                k += 2;
            }
            trace(&shifted, marker_trim(start_type), marker_trim(end_type))
        };
        let over = over_fills(&path.d, &[path.sx, path.sy, path.ex, path.ey], &fills, "", None);
        let thick = edge.thickness.as_deref() == Some("thick");
        if over == Some("") {
            casings.push_str(&edge_casing(&path, start_type, end_type, "", thick));
        }
        let id = esc_text(&edge.id);
        let stroke_attr = match over {
            Some(s) if !s.is_empty() => format!(" stroke=\"{}\"", s),
            _ => String::new(),
        };
        let m_color = over.filter(|s| !s.is_empty()).unwrap_or("var(--_l)");
        edges_out.push_str(&format!(
            "<g class=\"pele-edge\" data-id=\"{}\"{}><path d=\"{}\"{}{}/>{}{}</g>",
            id,
            stroke_attr,
            path.d,
            if thick { " stroke-width=\"2.5\"" } else { "" },
            if edge.pattern.as_deref() == Some("dotted") {
                " stroke-dasharray=\"3 4\""
            } else {
                ""
            },
            marker(start_type, path.sx, path.sy, path.sdx, path.sdy, m_color, "none"),
            marker(end_type, path.ex, path.ey, path.edx, path.edy, m_color, "none"),
        ));
        if r.label.width > 0.0 {
            let x = r.lx + ox;
            let y = r.ly + oy;
            labels_out.push_str(&edge_label_svg(&edge.id, &r.label, x, y, ""));
        }
    }

    let fixed = config.get("block").get("useMaxWidth").as_bool() == Some(false);
    let doc = Document {
        id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
        responsive: options.responsive.unwrap_or(!fixed),
        font_family: options.font_family.as_deref(),
        font_family_mono: options.font_family_mono.as_deref(),
        palette: "",
    };
    let acc = Accessible {
        acc_title: db.acc_title.as_deref(),
        acc_descr: db.acc_descr.as_deref(),
    };
    let content_svg = format!(
        "{}{}{}{}<g class=\"pele-nodes\">{}</g>{}",
        label_svg(
            &title,
            width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        if clusters.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-clusters\">{}</g>", clusters)
        },
        casings_svg(&casings),
        if edges_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{}</g>",
                edges_out
            )
        },
        nodes_out,
        if labels_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>",
                js_number(edge_size),
                labels_out
            )
        },
    );
    let svg = svg_document("block", width, height, size, &doc, &acc, &content_svg);
    Ok(Rendered {
        svg,
        width,
        height,
        links: Vec::new(),
    })
}
