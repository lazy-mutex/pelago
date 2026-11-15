use super::layered::{ease, kind, layered, ledge, lnode, orient, rank, Groups, LEdge, LNode, LayeredOptions, LayeredResult};
use crate::svg::builder::js_round;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    TB,
    BT,
    LR,
    RL,
}

impl Dir {
    pub fn vertical(self) -> bool {
        matches!(self, Dir::TB | Dir::BT)
    }
}

// The direction a diagram names, or top to bottom for anything else.
pub fn direction(dir: Option<&str>) -> Dir {
    match dir {
        Some("BT") => Dir::BT,
        Some("LR") => Dir::LR,
        Some("RL") => Dir::RL,
        _ => Dir::TB,
    }
}

#[derive(Debug, Clone)]
pub struct CNode {
    pub w: f64,
    pub h: f64,
    pub parent: i32,
    pub is_group: bool,
    pub dir: Option<Dir>,
    pub pad_x: f64,
    pub pad_top: f64,
    pub pad_bottom: f64,
    pub min_w: f64,
    // Declaration order; a group takes the smallest value among its members.
    pub seq: f64,
    // How much of a side its edges may spread along, measured across the flow; 0 keeps them at its middle.
    pub span: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct CEdge {
    pub src: usize,
    pub dst: usize,
    pub minlen: i32,
    pub label_w: f64,
    pub label_h: f64,
    // x, y, axis triples; axis is 0 where the edge runs vertically into the point, 1 horizontally.
    pub route: Vec<f64>,
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CompoundResult {
    pub width: f64,
    pub height: f64,
}

// Sides: 0 top, 1 right, 2 bottom, 3 left.
const TOP: u8 = 0;
const BOTTOM: u8 = 2;
const LEFT: u8 = 3;

struct PortEdge {
    e: usize,
    inner: usize,
    out: bool,
    side: u8,
    // Cross-flow position outside the group from a previous layout; NaN if unknown.
    hint: f64,
    px: f64,
    py: f64,
    route: Vec<f64>,
}

struct LevelEdge {
    e: usize,
    a: usize,
    b: usize,
}

struct Level {
    id: i32,
    dir: Dir,
    items: Vec<usize>,
    edges: Vec<LevelEdge>,
    ports: Vec<PortEdge>,
    port_of: HashMap<usize, usize>,
    routes: HashMap<usize, Vec<f64>>,
    x: f64,
    y: f64,
}

const MAX_PORTS: usize = 40000;

// Limit repeated layouts to small graphs.
const MAX_ORDERED_PORTS: usize = 400;
const MAX_ORDERED_EDGES: usize = 500;
// Crossing tolerance before trying independent group ordering.
const FLAT_SLACK: f64 = 1.5;
const FLAT_SPARE: f64 = 4.0;

struct Snapshot {
    nodes: Vec<[f64; 4]>,
    routes: Vec<Vec<f64>>,
    labels: Vec<[f64; 2]>,
    levels: Vec<f64>,
    total: CompoundResult,
}

// Ignore crossings within this distance of a shared node.
const FAN: f64 = 24.0;
// Tolerance for collinear points.
const IN_LINE: f64 = 0.5;
const CURVE_PIECES: usize = 3;
const CORNER_PIECES: usize = 2;
// Allow for a step, an S-curve, or a straight segment plus a rounded corner.
const STEP_PIECES: usize = 3;
// Match the renderer's corner handles.
const HANDLE: f64 = 0.62;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Curved,
    Stepped,
    Linear,
}

pub fn style_of(opt: &LayeredOptions) -> Style {
    if opt.tracks {
        Style::Stepped
    } else if opt.straight {
        Style::Linear
    } else {
        Style::Curved
    }
}

#[derive(Default)]
struct Segments {
    ax: Vec<f64>,
    ay: Vec<f64>,
    bx: Vec<f64>,
    by: Vec<f64>,
    low: Vec<f64>,
    high: Vec<f64>,
    owner: Vec<usize>,
    // Endpoint flags: 1 for the first segment, 2 for the last.
    ends: Vec<u8>,
    top: f64,
    bottom: f64,
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
}

impl Segments {
    fn add(&mut self, ei: usize, part: u8, mut x0: f64, mut y0: f64, x1: f64, y1: f64) {
        let on = x0 == self.to_x && y0 == self.to_y && ((y1 == y0 && self.from_y == y0) || (x1 == x0 && self.from_x == x0));
        if on {
            if let Some(p) = self.owner.len().checked_sub(1) {
                if self.owner[p] == ei && self.ends[p] == part && (x0 - self.from_x) * (x1 - x0) + (y0 - self.from_y) * (y1 - y0) > 0.0 {
                    // Extends the segment before it.
                    for list in [&mut self.ax, &mut self.ay, &mut self.bx, &mut self.by, &mut self.low, &mut self.high] {
                        list.pop();
                    }
                    self.owner.pop();
                    self.ends.pop();
                    x0 = self.from_x;
                    y0 = self.from_y;
                }
            }
        }
        self.from_x = x0;
        self.from_y = y0;
        self.to_x = x1;
        self.to_y = y1;
        self.ends.push(part);
        self.ax.push(x0.min(x1));
        self.bx.push(x0.max(x1));
        self.ay.push(if x0 <= x1 { y0 } else { y1 });
        self.by.push(if x0 <= x1 { y1 } else { y0 });
        let (lo, hi) = (y0.min(y1), y0.max(y1));
        self.low.push(lo);
        self.high.push(hi);
        if lo < self.top {
            self.top = lo;
        }
        if hi > self.bottom {
            self.bottom = hi;
        }
        self.owner.push(ei);
    }
}

// Counts the crossings between the routes as they will be drawn.
pub fn route_crossings(nodes: &[CNode], edges: &[CEdge], style: Style) -> usize {
    let mut s = Segments { top: f64::INFINITY, bottom: f64::NEG_INFINITY, from_x: f64::NAN, from_y: f64::NAN, to_x: f64::NAN, to_y: f64::NAN, ..Default::default() };
    for (ei, e) in edges.iter().enumerate() {
        if e.src == e.dst {
            continue;
        }
        let r = &e.route;
        let n = r.len();
        if n < 6 {
            continue;
        }
        let aligned = |k: usize| (r[k] - r[k - 3]).abs() < 0.01 || (r[k + 1] - r[k - 2]).abs() < 0.01;
        let length = |k: usize| (r[k] - r[k - 3]).abs() + (r[k + 1] - r[k - 2]).abs();
        let turns = |k: usize| {
            style == Style::Curved
                && k > 0
                && k + 3 < n
                && aligned(k)
                && aligned(k + 3)
                && length(k) >= 0.01
                && length(k + 3) >= 0.01
                && ((r[k] - r[k - 3]).abs() < 0.01) != ((r[k + 3] - r[k]).abs() < 0.01)
        };
        let mut at_x = r[0];
        let mut at_y = r[1];
        let mut k = 3;
        while k < n {
            let part = (if k == 3 { 1 } else { 0 }) | (if k == n - 3 { 2 } else { 0 });
            let (x0, y0, x1, y1) = (at_x, at_y, r[k], r[k + 1]);
            at_x = x1;
            at_y = y1;
            if aligned(k) || style == Style::Linear {
                if !turns(k) {
                    s.add(ei, part, x0, y0, x1, y1);
                    k += 3;
                    continue;
                }
                // Adjacent corners share a leg, so each gets half.
                let before = if k >= 6 && turns(k - 3) { length(k) / 2.0 } else { length(k) };
                let after = if turns(k + 3) { length(k + 3) / 2.0 } else { length(k + 3) };
                let ux = (x1 - r[k - 3]) / length(k);
                let uy = (y1 - r[k - 2]) / length(k);
                let vx = (r[k + 3] - x1) / length(k + 3);
                let vy = (r[k + 4] - y1) / length(k + 3);
                let sx = x1 - ux * before;
                let sy = y1 - uy * before;
                s.add(ei, part, x0, y0, sx, sy);
                let c1x = x1 - ux * before * (1.0 - HANDLE);
                let c1y = y1 - uy * before * (1.0 - HANDLE);
                let c2x = x1 + vx * after * (1.0 - HANDLE);
                let c2y = y1 + vy * after * (1.0 - HANDLE);
                at_x = x1 + vx * after;
                at_y = y1 + vy * after;
                let turn = part | (if k + 3 == n - 3 { 2 } else { 0 });
                let mut px = sx;
                let mut py = sy;
                for i in 1..=CORNER_PIECES {
                    let t = i as f64 / CORNER_PIECES as f64;
                    let u = 1.0 - t;
                    let x = u * u * u * sx + 3.0 * u * u * t * c1x + 3.0 * u * t * t * c2x + t * t * t * at_x;
                    let y = u * u * u * sy + 3.0 * u * u * t * c1y + 3.0 * u * t * t * c2y + t * t * t * at_y;
                    s.add(ei, turn, px, py, x, y);
                    px = x;
                    py = y;
                }
                k += 3;
                continue;
            }
            let down = r[k + 2] == 0.0;
            if style == Style::Stepped {
                let mx = if down { x0 } else { (x0 + x1) / 2.0 };
                let my = if down { (y0 + y1) / 2.0 } else { y0 };
                let (nx, ny) = if down { (x1, my) } else { (mx, y1) };
                s.add(ei, part, x0, y0, mx, my);
                s.add(ei, part, mx, my, nx, ny);
                s.add(ei, part, nx, ny, x1, y1);
                k += 3;
                continue;
            }
            // Approximate the rendered S-curve with straight segments.
            let c = if down { ease(x1 - x0, y1 - y0) } else { ease(y1 - y0, x1 - x0) };
            let mut px = x0;
            let mut py = y0;
            for i in 1..=CURVE_PIECES {
                let t = i as f64 / CURVE_PIECES as f64;
                let u = 1.0 - t;
                let across = t * t * (3.0 - 2.0 * t);
                let along = 3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t;
                let x = x0 + (x1 - x0) * if down { across } else { along };
                let y = y0 + (y1 - y0) * if down { along } else { across };
                s.add(ei, part, px, py, x, y);
                px = x;
                py = y;
            }
            k += 3;
        }
    }
    let count = s.owner.len();
    let _ = STEP_PIECES;
    // Count each pair only in the first band shared by both segments.
    let spread = s.bottom > s.top;
    let bands = if spread { (js_round((count as f64).sqrt()) as usize).max(1) } else { 1 };
    let scale = if spread { bands as f64 / (s.bottom - s.top) } else { 0.0 };
    let top = s.top;
    let band_of = |y: f64| (bands - 1).min(((y - top) * scale).floor().max(0.0) as usize);
    let mut first = vec![0usize; count];
    let mut band_start = vec![0usize; bands + 1];
    for p in 0..count {
        first[p] = band_of(s.low[p]);
        for b in first[p]..=band_of(s.high[p]) {
            band_start[b + 1] += 1;
        }
    }
    for b in 0..bands {
        band_start[b + 1] += band_start[b];
    }
    let mut fill = band_start[..bands].to_vec();
    let mut in_band = vec![0usize; band_start[bands]];
    for p in 0..count {
        for b in first[p]..=band_of(s.high[p]) {
            in_band[fill[b]] = p;
            fill[b] += 1;
        }
    }
    // Edges merging into a shared endpoint run do not count as crossings.
    let mut runs = vec![0.0f64; edges.len() * 8];
    let in_line = |r: &[f64], a: usize, b: usize, c: usize| {
        ((r[a] - r[b]).abs() < IN_LINE && (r[a] - r[c]).abs() < IN_LINE) || ((r[a + 1] - r[b + 1]).abs() < IN_LINE && (r[a + 1] - r[c + 1]).abs() < IN_LINE)
    };
    for (ei, e) in edges.iter().enumerate() {
        let r = &e.route;
        if r.len() < 6 {
            continue;
        }
        let mut k = 3;
        while k + 3 < r.len() && in_line(r, 0, k, k + 3) {
            k += 3;
        }
        let last = r.len() - 3;
        let mut j = last - 3;
        while j >= 3 && in_line(r, last, j, j - 3) {
            j -= 3;
        }
        runs[ei * 8..ei * 8 + 8].copy_from_slice(&[r[0], r[1], r[k], r[k + 1], r[j], r[j + 1], r[last], r[last + 1]]);
    }
    let on_run = |ei: usize, end: u8, x: f64, y: f64| {
        let o = ei * 8 + if end == 1 { 0 } else { 4 };
        let x0 = runs[o];
        let y0 = runs[o + 1];
        let dx = runs[o + 2] - x0;
        let dy = runs[o + 3] - y0;
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 { (((x - x0) * dx + (y - y0) * dy) / length).min(1.0).max(0.0) } else { 0.0 };
        (x - x0 - t * dx).hypot(y - y0 - t * dy) < IN_LINE
    };
    let mut total = 0;
    for band in 0..bands {
        let (from, to) = (band_start[band], band_start[band + 1]);
        in_band[from..to].sort_by(|&p, &q| s.ax[p].partial_cmp(&s.ax[q]).unwrap_or(std::cmp::Ordering::Equal).then(p.cmp(&q)));
        for si in from..to {
            let p = in_band[si];
            let dx = s.bx[p] - s.ax[p];
            let dy = s.by[p] - s.ay[p];
            let here = first[p] == band;
            for &q in &in_band[si + 1..to] {
                if s.ax[q] > s.bx[p] {
                    break;
                }
                if s.high[q] < s.low[p] || s.low[q] > s.high[p] || s.owner[p] == s.owner[q] || (!here && first[q] != band) {
                    continue;
                }
                let d1 = dx * (s.ay[q] - s.ay[p]) - dy * (s.ax[q] - s.ax[p]);
                let d2 = dx * (s.by[q] - s.ay[p]) - dy * (s.bx[q] - s.ax[p]);
                // Treat points on the line as one side to count segment-boundary crossings once.
                if (d1 > 0.0) == (d2 > 0.0) {
                    continue;
                }
                let ex = s.bx[q] - s.ax[q];
                let ey = s.by[q] - s.ay[q];
                let d3 = ex * (s.ay[p] - s.ay[q]) - ey * (s.ax[p] - s.ax[q]);
                let d4 = ex * (s.by[p] - s.ay[q]) - ey * (s.bx[p] - s.ax[q]);
                if (d3 > 0.0) == (d4 > 0.0) {
                    continue;
                }
                let ep = &edges[s.owner[p]];
                let eq = &edges[s.owner[q]];
                let at = d3 / (d3 - d4);
                let x = s.ax[p] + at * dx;
                let y = s.ay[p] + at * dy;
                // Exclude fan-in and fan-out at shared nodes.
                let fan = |v: usize, pe: u8, qe: u8| {
                    (s.ends[p] & pe != 0 && s.ends[q] & qe != 0)
                        || ((x - nodes[v].x).abs() < nodes[v].w / 2.0 + FAN && (y - nodes[v].y).abs() < nodes[v].h / 2.0 + FAN)
                        || (on_run(s.owner[p], pe, x, y) && on_run(s.owner[q], qe, x, y))
                };
                if (ep.src == eq.src && fan(ep.src, 1, 1))
                    || (ep.src == eq.dst && fan(ep.src, 1, 2))
                    || (ep.dst == eq.src && fan(ep.dst, 2, 1))
                    || (ep.dst == eq.dst && fan(ep.dst, 2, 2))
                {
                    continue;
                }
                total += 1;
            }
        }
    }
    total
}

// Match Mermaid: groups crossed by edges inherit their parent's direction.
pub fn inherit_crossed_directions(nodes: &mut [CNode], ends: &[(i32, i32)]) {
    let ancestors = |nodes: &[CNode], i: usize| {
        let mut set: Vec<usize> = Vec::new();
        let mut p = nodes[i].parent;
        while p >= 0 && !set.contains(&(p as usize)) {
            set.push(p as usize);
            p = nodes[p as usize].parent;
        }
        set
    };
    for &(src, dst) in ends {
        if src < 0 || dst < 0 || src == dst {
            continue;
        }
        let a = ancestors(nodes, src as usize);
        let b = ancestors(nodes, dst as usize);
        for &g in &a {
            if !b.contains(&g) {
                nodes[g].dir = None;
            }
        }
        for &g in &b {
            if !a.contains(&g) {
                nodes[g].dir = None;
            }
        }
    }
}

pub fn shift_layout(nodes: &mut [CNode], edges: &mut [CEdge], ox: f64, oy: f64) {
    for c in nodes {
        c.x += ox;
        c.y += oy;
    }
    for e in edges {
        for p in e.route.chunks_exact_mut(3) {
            p[0] += ox;
            p[1] += oy;
        }
        e.label_x += ox;
        e.label_y += oy;
    }
}

pub fn cnode(w: f64, h: f64, parent: i32) -> CNode {
    CNode { w, h, parent, is_group: false, dir: None, pad_x: 0.0, pad_top: 0.0, pad_bottom: 0.0, min_w: 0.0, seq: 0.0, span: 0.0, x: 0.0, y: 0.0 }
}

pub fn cedge(src: usize, dst: usize, minlen: i32, label_w: f64, label_h: f64) -> CEdge {
    CEdge { src, dst, minlen, label_w, label_h, ..Default::default() }
}

fn start_side(dir: Dir) -> u8 {
    match dir {
        Dir::TB => TOP,
        Dir::BT => BOTTOM,
        Dir::LR => LEFT,
        Dir::RL => 1,
    }
}

fn opposite(side: u8) -> u8 {
    (side + 2) & 3
}

fn same(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

struct Laid {
    given: Vec<f64>,
    lnodes: Vec<LNode>,
    ledges: Vec<LEdge>,
    size: LayeredResult,
}

// What the flat layout learned, for ordering the ports that leave a group.
struct Far {
    ledge_of: Vec<i32>,
    points: Vec<Vec<f64>>,
    // Last rank occupied by each group.
    last: Vec<i32>,
}

struct Compound<'a> {
    nodes: &'a mut [CNode],
    edges: &'a mut [CEdge],
    opt: &'a LayeredOptions,
    levels: Vec<Level>,
    level_of: HashMap<i32, usize>,
    local: Vec<usize>,
    key: Vec<f64>,
    keyed: bool,
    far: Option<Far>,
    ordered: bool,
    // Reuse unchanged levels across layout attempts.
    laid: HashMap<i32, Vec<Laid>>,
    total: CompoundResult,
}

impl Compound<'_> {
    fn make_level(&mut self, id: i32, dir: Dir, items: Vec<usize>) -> usize {
        self.levels.push(Level { id, dir, items, edges: Vec::new(), ports: Vec::new(), port_of: HashMap::new(), routes: HashMap::new(), x: 0.0, y: 0.0 });
        self.level_of.insert(id, self.levels.len() - 1);
        self.levels.len() - 1
    }

    fn level_at(&self, id: i32) -> usize {
        self.level_of[&id]
    }

    // The port by which edge `ei` leaves (`out`) or enters group `item`.
    fn port(&self, item: usize, ei: usize, out: bool) -> &PortEdge {
        let level = &self.levels[self.level_at(item as i32)];
        &level.ports[level.port_of[&(ei * 2 + if out { 0 } else { 1 })]]
    }

    fn set_port_side(&mut self, item: usize, ei: usize, out: bool, side: u8) {
        let at = self.level_at(item as i32);
        let level = &mut self.levels[at];
        let index = level.port_of[&(ei * 2 + if out { 0 } else { 1 })];
        level.ports[index].side = side;
    }

    fn endpoint_below(&self, item: usize, ei: usize, out: bool) -> bool {
        (if out { self.edges[ei].src } else { self.edges[ei].dst }) != item
    }

    fn held(&self, item: usize, ei: usize, out: bool) -> bool {
        self.nodes[item].is_group && self.endpoint_below(item, ei, out)
    }

    fn offset(&self, vertical: bool, sized: bool, item: usize, ei: usize, out: bool) -> f64 {
        if !sized || !self.held(item, ei, out) {
            return 0.0;
        }
        let port = self.port(item, ei, out);
        if vertical {
            port.px - self.nodes[item].w / 2.0
        } else {
            port.py - self.nodes[item].h / 2.0
        }
    }

    // Order outgoing ports at the group's last rank; break ties using the next rank.
    fn far_hint(&self, level: &Level, port: &PortEdge) -> f64 {
        let Some(far) = &self.far else { return f64::NAN };
        let le = far.ledge_of[port.e];
        if le < 0 || level.id < 0 || !port.out || port.side != opposite(start_side(level.dir)) {
            return f64::NAN;
        }
        let last = far.last[level.id as usize] as f64;
        let mut inside = self.key[self.edges[port.e].src];
        let mut outside = 0.0;
        for p in far.points[le as usize].chunks_exact(2) {
            if p[0] == last {
                inside = p[1];
            }
            if p[0] == last + 1.0 {
                outside = p[1];
            }
        }
        inside + outside * 1e-6
    }

    fn build(&mut self, at: usize, sized: bool) -> (Vec<LNode>, Vec<LEdge>, Vec<i32>) {
        let vertical = self.levels[at].dir.vertical();
        let start = start_side(self.levels[at].dir);
        let mut lnodes: Vec<LNode> = Vec::new();
        let mut ledges: Vec<LEdge> = Vec::new();
        for i in 0..self.levels[at].items.len() {
            let item = self.levels[at].items[i];
            self.local[item] = i;
            let node = &self.nodes[item];
            let mut ln = if sized { lnode(if vertical { node.w } else { node.h }, if vertical { node.h } else { node.w }, kind::NODE) } else { lnode(0.0, 0.0, kind::NODE) };
            ln.span = if node.is_group { node.span.min(ln.w - 2.0 * node.pad_x) } else { node.span };
            ln.key = if self.keyed { self.key[item] } else { f64::NAN };
            lnodes.push(ln);
        }
        let level = &self.levels[at];
        for le in &level.edges {
            let e = &self.edges[le.e];
            let mut edge = ledge(self.local[le.a], self.local[le.b], e.minlen, 0.0, 0.0);
            if sized {
                edge.label_w = if vertical { e.label_w } else { e.label_h };
                edge.label_h = if vertical { e.label_h } else { e.label_w };
                edge.tail_dx = self.offset(vertical, sized, le.a, le.e, true);
                edge.head_dx = self.offset(vertical, sized, le.b, le.e, false);
                edge.tail_fixed = self.held(le.a, le.e, true);
                edge.head_fixed = self.held(le.b, le.e, false);
            }
            ledges.push(edge);
        }
        let mut dummies = vec![-1i32; level.ports.len()];
        for (i, port) in level.ports.iter().enumerate() {
            if port.side == start || port.side == opposite(start) {
                let d = lnodes.len();
                let mut ln = lnode(0.0, 0.0, if port.side == start { kind::START_PORT } else { kind::END_PORT });
                let far_end = if port.out { self.edges[port.e].dst } else { self.edges[port.e].src };
                ln.key = if self.keyed { self.key[far_end] } else { f64::NAN };
                ln.hint = if port.hint.is_nan() && self.keyed && self.far.is_some() { self.far_hint(level, port) } else { port.hint };
                lnodes.push(ln);
                dummies[i] = d as i32;
                let mut edge = if port.out { ledge(self.local[port.inner], d, 1, 0.0, 0.0) } else { ledge(d, self.local[port.inner], 1, 0.0, 0.0) };
                if port.out {
                    edge.tail_dx = self.offset(vertical, sized, port.inner, port.e, true);
                    edge.tail_fixed = self.held(port.inner, port.e, true);
                } else {
                    edge.head_dx = self.offset(vertical, sized, port.inner, port.e, false);
                    edge.head_fixed = self.held(port.inner, port.e, false);
                }
                ledges.push(edge);
            } else {
                lnodes[self.local[port.inner]].pin = if port.side == TOP || port.side == LEFT { -1 } else { 1 };
            }
        }
        (lnodes, ledges, dummies)
    }

    fn lay_out(&mut self, id: i32, mut lnodes: Vec<LNode>, mut ledges: Vec<LEdge>) -> (Vec<LNode>, Vec<LEdge>, LayeredResult) {
        if !self.ordered {
            let size = layered(&mut lnodes, &mut ledges, self.opt);
            return (lnodes, ledges, size);
        }
        let mut given: Vec<f64> = Vec::with_capacity(lnodes.len() * 7 + ledges.len() * 9);
        for ln in &lnodes {
            given.extend([ln.w, ln.h, ln.kind as f64, ln.pin as f64, ln.hint, ln.key, ln.span]);
        }
        for le in &ledges {
            given.extend([le.tail as f64, le.head as f64, le.minlen as f64, le.label_w, le.label_h, le.tail_dx, le.head_dx, le.tail_fixed as u8 as f64, le.head_fixed as u8 as f64]);
        }
        let past = self.laid.entry(id).or_default();
        for p in past.iter() {
            if p.given.len() == given.len() && p.given.iter().zip(&given).all(|(&a, &b)| same(a, b)) {
                return (p.lnodes.clone(), p.ledges.clone(), p.size);
            }
        }
        let size = layered(&mut lnodes, &mut ledges, self.opt);
        past.push(Laid { given, lnodes: lnodes.clone(), ledges: ledges.clone(), size });
        (lnodes, ledges, size)
    }

    fn place(&mut self) {
        // Rank parents first to determine which group sides edges cross.
        for at in 0..self.levels.len() {
            for i in 0..self.levels[at].ports.len() {
                let port = &self.levels[at].ports[i];
                let (inner, e, out, side) = (port.inner, port.e, port.out, port.side);
                if self.held(inner, e, out) {
                    self.set_port_side(inner, e, out, side);
                }
            }
            if self.levels[at].edges.is_empty() {
                continue;
            }
            let needed = self.levels[at].edges.iter().any(|le| {
                let e = &self.edges[le.e];
                (self.nodes[le.a].is_group && e.src != le.a) || (self.nodes[le.b].is_group && e.dst != le.b)
            });
            if !needed {
                continue;
            }
            let (mut lnodes, mut ledges, _) = self.build(at, false);
            orient(&lnodes, &mut ledges);
            rank(&mut lnodes, &ledges, 1);
            let start = start_side(self.levels[at].dir);
            for i in 0..self.levels[at].edges.len() {
                let le = &self.levels[at].edges[i];
                let (a, b, ei) = (le.a, le.b, le.e);
                let (src, dst) = (self.edges[ei].src, self.edges[ei].dst);
                let a_above = !ledges[i].reversed;
                if self.nodes[a].is_group && src != a {
                    self.set_port_side(a, ei, true, if a_above { opposite(start) } else { start });
                }
                if self.nodes[b].is_group && dst != b {
                    self.set_port_side(b, ei, false, if a_above { start } else { opposite(start) });
                }
            }
        }

        // Lay out children first so their sizes are known.
        for at in (0..self.levels.len()).rev() {
            let id = self.levels[at].id;
            let dir = self.levels[at].dir;
            let vertical = dir.vertical();
            let flip = dir == Dir::BT || dir == Dir::RL;
            let (built_nodes, built_edges, dummies) = self.build(at, true);
            let (lnodes, ledges, size) = self.lay_out(id, built_nodes, built_edges);
            let content_w = if vertical { size.width } else { size.height };
            let content_h = if vertical { size.height } else { size.width };
            let (pad_x, pad_top, pad_bottom, min_w) = if id >= 0 {
                let g = &self.nodes[id as usize];
                (g.pad_x, g.pad_top, g.pad_bottom, g.min_w)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };
            let box_w = (content_w + 2.0 * pad_x).max(min_w);
            let box_h = content_h + pad_top + pad_bottom;
            let off_x = (box_w - content_w) / 2.0;
            let off_y = pad_top;
            let tx = |lx: f64, ly: f64| off_x + if vertical { lx } else if flip { size.height - ly } else { ly };
            let ty = |lx: f64, ly: f64| off_y + if vertical { if flip { size.height - ly } else { ly } } else { lx };
            let axis = if vertical { 0.0 } else { 1.0 };
            let convert = |points: &[f64]| -> Vec<f64> { points.chunks_exact(2).flat_map(|p| [tx(p[0], p[1]), ty(p[0], p[1]), axis]).collect() };

            for i in 0..self.levels[at].items.len() {
                let item = self.levels[at].items[i];
                self.nodes[item].x = tx(lnodes[i].x, lnodes[i].y);
                self.nodes[item].y = ty(lnodes[i].x, lnodes[i].y);
            }
            for i in 0..self.levels[at].edges.len() {
                let ei = self.levels[at].edges[i].e;
                let edge = &ledges[i];
                self.levels[at].routes.insert(ei, convert(&edge.points));
                let e = &mut self.edges[ei];
                if e.label_w > 0.0 {
                    e.label_x = tx(edge.label_x, edge.label_y);
                    e.label_y = ty(edge.label_x, edge.label_y);
                }
            }
            let mut k = self.levels[at].edges.len();
            for i in 0..self.levels[at].ports.len() {
                let port = &self.levels[at].ports[i];
                let (inner, pe, out, side) = (port.inner, port.e, port.out, port.side);
                let onto_border = |x: f64, y: f64| match side {
                    TOP => (x, 0.0),
                    BOTTOM => (x, box_h),
                    LEFT => (0.0, y),
                    _ => (box_w, y),
                };
                if dummies[i] >= 0 {
                    let mut route = convert(&ledges[k].points);
                    k += 1;
                    let from = if out { route.len() - 3 } else { 0 };
                    let (bx, by) = onto_border(route[from], route[from + 1]);
                    if out {
                        route.extend([bx, by, axis]);
                    } else {
                        route.splice(0..0, [bx, by, axis]);
                    }
                    let port = &mut self.levels[at].ports[i];
                    port.px = bx;
                    port.py = by;
                    port.route = route;
                    continue;
                }
                let item = &self.nodes[inner];
                let mut ix = item.x;
                let mut iy = item.y;
                if self.held(inner, pe, out) {
                    let below = self.port(inner, pe, out);
                    ix += below.px - item.w / 2.0;
                    iy += below.py - item.h / 2.0;
                } else {
                    match side {
                        TOP => iy -= item.h / 2.0,
                        BOTTOM => iy += item.h / 2.0,
                        LEFT => ix -= item.w / 2.0,
                        _ => ix += item.w / 2.0,
                    }
                }
                let (bx, by) = onto_border(ix, iy);
                let a = if side == TOP || side == BOTTOM { 0.0 } else { 1.0 };
                let port = &mut self.levels[at].ports[i];
                port.px = bx;
                port.py = by;
                port.route = if out { vec![ix, iy, a, bx, by, a] } else { vec![bx, by, a, ix, iy, a] };
            }

            if id >= 0 {
                self.nodes[id as usize].w = box_w;
                self.nodes[id as usize].h = box_h;
            } else {
                self.total = CompoundResult { width: box_w, height: box_h };
            }
        }

        for at in 0..self.levels.len() {
            let (ox, oy) = (self.levels[at].x, self.levels[at].y);
            for i in 0..self.levels[at].items.len() {
                let item = self.levels[at].items[i];
                let node = &mut self.nodes[item];
                node.x += ox;
                node.y += oy;
                if node.is_group {
                    let (x, y) = (node.x - node.w / 2.0, node.y - node.h / 2.0);
                    let child = self.level_at(item as i32);
                    self.levels[child].x = x;
                    self.levels[child].y = y;
                }
            }
        }

        fn append(route: &mut Vec<f64>, part: &[f64], ox: f64, oy: f64) {
            for p in part.chunks_exact(3) {
                let x = p[0] + ox;
                let y = p[1] + oy;
                let len = route.len();
                if len >= 3 && (route[len - 3] - x).abs() < 0.01 && (route[len - 2] - y).abs() < 0.01 {
                    continue;
                }
                route.extend([x, y, p[2]]);
            }
        }

        for at in 0..self.levels.len() {
            let level = &self.levels[at];
            for le in &level.edges {
                let (src, dst) = (self.edges[le.e].src, self.edges[le.e].dst);
                let mut route: Vec<f64> = Vec::new();
                let mut item = src;
                while self.nodes[item].parent != level.id {
                    let lv = &self.levels[self.level_at(self.nodes[item].parent)];
                    append(&mut route, &lv.ports[lv.port_of[&(le.e * 2)]].route, lv.x, lv.y);
                    item = self.nodes[item].parent as usize;
                }
                append(&mut route, &level.routes[&le.e], level.x, level.y);
                let mut inward: Vec<usize> = Vec::new();
                let mut item = dst;
                while self.nodes[item].parent != level.id {
                    inward.push(self.level_at(self.nodes[item].parent));
                    item = self.nodes[item].parent as usize;
                }
                for &lv in inward.iter().rev() {
                    let lv = &self.levels[lv];
                    append(&mut route, &lv.ports[lv.port_of[&(le.e * 2 + 1)]].route, lv.x, lv.y);
                }
                let e = &mut self.edges[le.e];
                e.route = route;
                if e.label_w > 0.0 {
                    e.label_x += level.x;
                    e.label_y += level.y;
                }
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        let mut places = Vec::new();
        for level in &self.levels {
            places.extend([level.x, level.y]);
            for port in &level.ports {
                places.extend([port.px, port.py]);
            }
        }
        Snapshot {
            nodes: self.nodes.iter().map(|c| [c.x, c.y, c.w, c.h]).collect(),
            routes: self.edges.iter().map(|e| e.route.clone()).collect(),
            labels: self.edges.iter().map(|e| [e.label_x, e.label_y]).collect(),
            levels: places,
            total: self.total,
        }
    }

    fn restore(&mut self, s: Snapshot) {
        for (c, at) in self.nodes.iter_mut().zip(&s.nodes) {
            [c.x, c.y, c.w, c.h] = *at;
        }
        for ((e, route), label) in self.edges.iter_mut().zip(s.routes).zip(&s.labels) {
            e.route = route;
            [e.label_x, e.label_y] = *label;
        }
        let mut k = 0;
        for level in &mut self.levels {
            level.x = s.levels[k];
            level.y = s.levels[k + 1];
            k += 2;
            for port in &mut level.ports {
                port.px = s.levels[k];
                port.py = s.levels[k + 1];
                k += 2;
            }
        }
        self.total = s.total;
    }
}

// Lays out a graph whose nodes may be nested in groups. Each group is laid out on its own,
// in its own direction, and then placed in its parent as a single box. An edge that crosses
// a group's border passes through a port on the side facing its other end.
pub fn compound_layout(nodes: &mut [CNode], edges: &mut [CEdge], root_dir: Dir, opt: &LayeredOptions) -> CompoundResult {
    let n = nodes.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut root_items: Vec<usize> = Vec::new();
    for i in 0..n {
        let p = nodes[i].parent;
        if p >= 0 && (p as usize) < n && p as usize != i && nodes[p as usize].is_group {
            children[p as usize].push(i);
        } else {
            nodes[i].parent = -1;
            root_items.push(i);
        }
    }
    let mut c = Compound {
        nodes,
        edges,
        opt,
        levels: Vec::new(),
        level_of: HashMap::new(),
        local: vec![0; n],
        key: vec![f64::NAN; n],
        keyed: false,
        far: None,
        ordered: false,
        laid: HashMap::new(),
        total: CompoundResult::default(),
    };
    c.make_level(-1, root_dir, root_items);
    let mut depth = vec![0usize; n];
    let mut placed = vec![false; n];
    fn descend(c: &mut Compound, children: &[Vec<usize>], depth: &mut [usize], placed: &mut [bool], from: usize) {
        let mut q = from;
        while q < c.levels.len() {
            let (id, dir) = (c.levels[q].id, c.levels[q].dir);
            for i in 0..c.levels[q].items.len() {
                let item = c.levels[q].items[i];
                placed[item] = true;
                depth[item] = if id < 0 { 0 } else { depth[id as usize] + 1 };
                if c.nodes[item].is_group {
                    let child_dir = c.nodes[item].dir.unwrap_or(dir);
                    c.make_level(item as i32, child_dir, children[item].clone());
                }
            }
            q += 1;
        }
    }
    descend(&mut c, &children, &mut depth, &mut placed, 0);
    // Groups that contain each other are unreachable from the root; lift them out.
    for i in 0..n {
        if placed[i] {
            continue;
        }
        let former = c.nodes[i].parent as usize;
        children[former].retain(|&child| child != i);
        c.nodes[i].parent = -1;
        c.levels[0].items.push(i);
        placed[i] = true;
        depth[i] = 0;
        if c.nodes[i].is_group {
            let from = c.levels.len();
            let dir = c.nodes[i].dir.unwrap_or(root_dir);
            c.make_level(i as i32, dir, children[i].clone());
            descend(&mut c, &children, &mut depth, &mut placed, from);
        }
    }

    for q in (1..c.levels.len()).rev() {
        let seq = c.levels[q].items.iter().map(|&item| c.nodes[item].seq).fold(f64::INFINITY, f64::min);
        if seq != f64::INFINITY {
            c.nodes[c.levels[q].id as usize].seq = seq;
        }
    }
    for q in 0..c.levels.len() {
        let nodes = &*c.nodes;
        c.levels[q].items.sort_by(|&a, &b| nodes[a].seq.partial_cmp(&nodes[b].seq).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
    }

    // Every group border an edge crosses costs a port. Past the budget, an edge is drawn between
    // the groups that hold its ends, at the level they share.
    let up = |nodes: &[CNode], v: usize| nodes[v].parent as usize;
    let mut ports = 0usize;
    for ei in 0..c.edges.len() {
        let mut a = c.edges[ei].src;
        let mut b = c.edges[ei].dst;
        let mut crossed = 0;
        while depth[a] > depth[b] {
            a = up(c.nodes, a);
            crossed += 1;
        }
        while depth[b] > depth[a] {
            b = up(c.nodes, b);
            crossed += 1;
        }
        while a != b && c.nodes[a].parent != c.nodes[b].parent {
            a = up(c.nodes, a);
            b = up(c.nodes, b);
            crossed += 2;
        }
        if a == b {
            continue;
        }
        if ports + crossed > MAX_PORTS {
            c.edges[ei].src = a;
            c.edges[ei].dst = b;
        } else {
            ports += crossed;
        }
    }

    let mut loops: Vec<usize> = Vec::new();
    let mut stray: Vec<usize> = Vec::new();
    for ei in 0..c.edges.len() {
        c.edges[ei].route = Vec::new();
        let (src, dst) = (c.edges[ei].src, c.edges[ei].dst);
        if src == dst {
            loops.push(ei);
            continue;
        }
        let mut a = src;
        let mut b = dst;
        let mut a_path: Vec<usize> = Vec::new();
        let mut b_path: Vec<usize> = Vec::new();
        while depth[a] > depth[b] {
            a_path.push(a);
            a = up(c.nodes, a);
        }
        while depth[b] > depth[a] {
            b_path.push(b);
            b = up(c.nodes, b);
        }
        if a == b {
            stray.push(ei);
            continue;
        }
        while c.nodes[a].parent != c.nodes[b].parent {
            a_path.push(a);
            b_path.push(b);
            a = up(c.nodes, a);
            b = up(c.nodes, b);
        }
        let shared = c.level_at(c.nodes[a].parent);
        c.levels[shared].edges.push(LevelEdge { e: ei, a, b });
        for (path, out) in [(a_path, true), (b_path, false)] {
            for inner in path {
                let at = c.level_at(c.nodes[inner].parent);
                let level = &mut c.levels[at];
                level.ports.push(PortEdge { e: ei, inner, out, side: TOP, hint: f64::NAN, px: 0.0, py: 0.0, route: Vec::new() });
                level.port_of.insert(ei * 2 + if out { 0 } else { 1 }, level.ports.len() - 1);
            }
        }
    }

    c.ordered = ports > 1 && ports <= MAX_ORDERED_PORTS && c.edges.len() <= MAX_ORDERED_EDGES;

    // Use a flat layout to coordinate node and port order across groups.
    let mut flat_crossings = 0usize;
    let flat = c.levels.len() > 1 && c.ordered && c.levels.iter().all(|level| level.dir == root_dir);
    if flat {
        let vertical = root_dir.vertical();
        let mut leaf = vec![-1i32; n];
        let mut group = vec![-1i32; n];
        let mut lnodes: Vec<LNode> = Vec::new();
        let mut of: Vec<i32> = Vec::new();
        let mut parent: Vec<i32> = Vec::new();
        for i in 0..n {
            if c.nodes[i].is_group {
                group[i] = parent.len() as i32;
                parent.push(-1);
            }
        }
        for i in 0..n {
            let p = c.nodes[i].parent;
            let in_group = if p >= 0 { group[p as usize] } else { -1 };
            if c.nodes[i].is_group {
                parent[group[i] as usize] = in_group;
            } else {
                leaf[i] = lnodes.len() as i32;
                let node = &c.nodes[i];
                lnodes.push(lnode(if vertical { node.w } else { node.h }, if vertical { node.h } else { node.w }, kind::NODE));
                of.push(in_group);
            }
        }
        let mut ledges: Vec<LEdge> = Vec::new();
        let mut ledge_of = vec![-1i32; c.edges.len()];
        for (ei, e) in c.edges.iter().enumerate() {
            if e.src == e.dst || leaf[e.src] < 0 || leaf[e.dst] < 0 {
                continue;
            }
            ledge_of[ei] = ledges.len() as i32;
            ledges.push(ledge(
                leaf[e.src] as usize,
                leaf[e.dst] as usize,
                e.minlen,
                if vertical { e.label_w } else { e.label_h },
                if vertical { e.label_h } else { e.label_w },
            ));
        }
        let flat_opt = LayeredOptions { groups: Some(Groups { of, parent }), order_only: true, ..opt.clone() };
        let flat_layout = layered(&mut lnodes, &mut ledges, &flat_opt);
        flat_crossings = flat_layout.crossings.min(flat_layout.declared);
        let mut sum = vec![0.0; n];
        let mut count = vec![0usize; n];
        let mut last = vec![-1i32; n];
        for i in 0..n {
            if leaf[i] < 0 {
                continue;
            }
            let ln = &lnodes[leaf[i] as usize];
            c.key[i] = ln.x;
            let mut p = c.nodes[i].parent;
            while p >= 0 {
                sum[p as usize] += c.key[i];
                count[p as usize] += 1;
                last[p as usize] = last[p as usize].max(ln.rank);
                p = c.nodes[p as usize].parent;
            }
        }
        for i in 0..n {
            if count[i] > 0 {
                c.key[i] = sum[i] / count[i] as f64;
            }
        }
        c.far = Some(Far { ledge_of, points: ledges.into_iter().map(|le| le.points).collect(), last });
    }

    // The flat layout ignores group boxes, so independent ordering may yield fewer crossings.
    let style = style_of(opt);
    c.keyed = flat;
    c.place();
    let mut crossings: Option<usize> = None;
    if flat {
        let mut found = route_crossings(c.nodes, c.edges, style);
        if found as f64 > flat_crossings as f64 * FLAT_SLACK + FLAT_SPARE {
            let first = c.snapshot();
            c.keyed = false;
            c.place();
            let own = route_crossings(c.nodes, c.edges, style);
            if own >= found {
                c.keyed = true;
                c.restore(first);
            } else {
                found = own;
            }
        }
        crossings = Some(found);
    }

    // Refine port order using routes outside each group. Skip this if independent ordering won;
    // it rarely helps enough to justify another layout.
    if c.keyed == flat && c.ordered {
        let mut again = false;
        for at in 0..c.levels.len() {
            let id = c.levels[at].id;
            if id < 0 {
                continue;
            }
            let (bx0, by0, bw, bh) = {
                let b = &c.nodes[id as usize];
                (b.x, b.y, b.w, b.h)
            };
            let vertical = c.levels[at].dir.vertical();
            let end = opposite(start_side(c.levels[at].dir));
            let (lx, ly) = (c.levels[at].x, c.levels[at].y);
            let mut seen: Vec<[f64; 2]> = Vec::new();
            for i in 0..c.levels[at].ports.len() {
                let port = &c.levels[at].ports[i];
                if port.side != end {
                    continue;
                }
                // Skip the straight run out of the port to find where the edge turns.
                let route = &c.edges[port.e].route;
                let bx = lx + port.px;
                let by = ly + port.py;
                let mut found: Option<usize> = None;
                let mut k = 0;
                while k < route.len() && (found.is_none() || port.out) {
                    if (route[k] - bx).abs() < 0.01 && (route[k + 1] - by).abs() < 0.01 {
                        found = Some(k);
                    }
                    k += 3;
                }
                let Some(from) = found else { continue };
                let a = if vertical { 0 } else { 1 };
                let step: i64 = if port.out { 3 } else { -3 };
                let mut next = from as i64 + step;
                while next >= 0 && (next as usize) < route.len() && (route[next as usize + a] - route[from + a]).abs() < IN_LINE {
                    next += step;
                }
                if next < 0 || next as usize >= route.len() {
                    continue;
                }
                let hint = route[next as usize + a] - if vertical { bx0 - bw / 2.0 } else { by0 - bh / 2.0 };
                let along = if vertical { port.px } else { port.py };
                c.levels[at].ports[i].hint = hint;
                seen.push([along, hint]);
            }
            seen.sort_by(|p, q| p[0].partial_cmp(&q[0]).unwrap_or(std::cmp::Ordering::Equal));
            if seen.windows(2).any(|pair| pair[0][1] > pair[1][1]) {
                again = true;
            }
        }
        if again {
            let before = crossings.unwrap_or_else(|| route_crossings(c.nodes, c.edges, style));
            if before > 0 {
                let first = c.snapshot();
                c.place();
                if route_crossings(c.nodes, c.edges, style) >= before {
                    c.restore(first);
                }
            }
        }
    }

    for &ei in &stray {
        let (a, b) = (&c.nodes[c.edges[ei].src], &c.nodes[c.edges[ei].dst]);
        let (ax, ay, bx, by) = (a.x, a.y, b.x, b.y);
        let e = &mut c.edges[ei];
        e.route = vec![ax, ay, 0.0, bx, by, 0.0];
        e.label_x = (ax + bx) / 2.0;
        e.label_y = (ay + by) / 2.0;
    }
    for &ei in &loops {
        let node = &c.nodes[c.edges[ei].src];
        let (x, y) = (node.x + node.w / 2.0 + 28.0, node.y);
        let e = &mut c.edges[ei];
        e.label_x = x + e.label_w / 2.0;
        e.label_y = y;
    }
    let _ = HashSet::<usize>::new();
    c.total
}
