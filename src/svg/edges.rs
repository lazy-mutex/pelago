use super::builder::{esc_text, js_round, label_svg, num, sign};
use super::ink::{declaration, ink};
use crate::layout::layered::ease;
use crate::text::label::Label;
use std::collections::HashMap;

pub const ARROW: f64 = 8.0;

#[derive(Debug, Clone, Default)]
pub struct EdgePath {
    pub d: String,
    pub tokens: Option<Vec<f64>>,
    pub sx: f64,
    pub sy: f64,
    pub sdx: f64,
    pub sdy: f64,
    pub ex: f64,
    pub ey: f64,
    pub edx: f64,
    pub edy: f64,
}

// Marker types are Mermaid's: arrow_point, arrow_circle, arrow_cross; anything else draws nothing.

// The most a corner is rounded by in a stepped route.
const CORNER: f64 = 12.0;
// How far the handles of a wide turn reach along its two legs. A little more than a circle's,
// which makes the turn fuller.
const HANDLE: f64 = 0.62;
// How far an edge runs straight before a marker at its end, where it has the room.
pub const LEAD: f64 = 6.0;

// How far from each side of each node the edges that meet it start to turn: as far as the
// largest marker there needs, for every edge on the side, so that none turns sooner than its
// neighbour and crosses it.
#[derive(Default)]
pub struct SideReach(HashMap<usize, f64>);

impl SideReach {
    pub fn add(&mut self, node: usize, side: i32, kind: &str) {
        let trim = marker_trim(kind);
        if side >= 0 && trim > 0.0 {
            let key = node * 4 + side as usize;
            if trim + LEAD > self.0.get(&key).copied().unwrap_or(0.0) {
                self.0.insert(key, trim + LEAD);
            }
        }
    }

    pub fn of(&self, node: usize, side: i32) -> f64 {
        if side >= 0 {
            self.0.get(&(node * 4 + side as usize)).copied().unwrap_or(0.0)
        } else {
            0.0
        }
    }
}

fn near(ax: f64, ay: f64, bx: f64, by: f64) -> bool {
    (ax - bx).abs() < 0.01 && (ay - by).abs() < 0.01
}

// Match the value parsed from num() output.
fn written(v: f64) -> f64 {
    let n = js_round(v * 100.0);
    if n == 0.0 {
        0.0
    } else {
        n / 100.0
    }
}

struct Route {
    xs: Vec<f64>,
    ys: Vec<f64>,
    axes: Vec<f64>,
    linear: bool,
}

impl Route {
    fn aligned(&self, i: usize) -> bool {
        (self.xs[i] - self.xs[i - 1]).abs() < 0.01 || (self.ys[i] - self.ys[i - 1]).abs() < 0.01
    }

    fn straight(&self, i: usize) -> bool {
        self.linear || self.aligned(i)
    }

    // The way the route heads from point i to point j: along the flow where an S joins them.
    fn unit(&self, i: usize, j: usize, axis: f64) -> (f64, f64) {
        if !self.straight(i.max(j)) {
            let or1 = |v: f64| if v == 0.0 { 1.0 } else { v };
            return if axis == 0.0 { (0.0, or1(sign(self.ys[j] - self.ys[i]))) } else { (or1(sign(self.xs[j] - self.xs[i])), 0.0) };
        }
        let dx = self.xs[j] - self.xs[i];
        let dy = self.ys[j] - self.ys[i];
        let len = dx.hypot(dy);
        let len = if len == 0.0 { 1.0 } else { len };
        (dx / len, dy / len)
    }

    fn length(&self, i: usize) -> f64 {
        (self.xs[i] - self.xs[i - 1]).abs() + (self.ys[i] - self.ys[i - 1]).abs()
    }

    // Whether the route turns a corner at point i, from running one way to running the other.
    fn turns(&self, i: usize) -> bool {
        let last = self.xs.len() - 1;
        !self.linear
            && i > 0
            && i < last
            && self.aligned(i)
            && self.aligned(i + 1)
            && self.length(i) >= 0.01
            && self.length(i + 1) >= 0.01
            && ((self.xs[i] - self.xs[i - 1]).abs() < 0.01) != ((self.xs[i + 1] - self.xs[i]).abs() < 0.01)
    }

    fn along(&self, i: usize, ux: f64, uy: f64) -> f64 {
        (self.xs[i] - self.xs[i - 1]).abs() * ux.abs() + (self.ys[i] - self.ys[i - 1]).abs() * uy.abs()
    }
}

// Of the way from the end of a run to where it turns, the part that makes up for a larger
// marker beside this one is kept where it can be, and the rest is at most a quarter of what
// is left, so that ends on one side of a node turn equally far from it.
fn lead(reach: f64, trim: f64, run: f64) -> f64 {
    let room = (reach - LEAD - trim).max(0.0).min(run / 4.0);
    room + (reach - trim - room).min((run - room) / 4.0)
}

struct PathWriter {
    d: String,
    tokens: Vec<f64>,
    at_x: f64,
    at_y: f64,
}

impl PathWriter {
    fn tok(&mut self, v: f64) -> String {
        self.tokens.push(written(v));
        num(v)
    }

    fn line_to(&mut self, x: f64, y: f64) {
        if near(x, y, self.at_x, self.at_y) {
            return;
        }
        self.tokens.extend([76.0, 2.0]);
        let (a, b) = (self.tok(x), self.tok(y));
        self.d.push_str(&format!("L{},{}", a, b));
        self.at_x = x;
        self.at_y = y;
    }

    fn cubic(&mut self, p: [f64; 6]) {
        self.tokens.extend([67.0, 6.0]);
        let t: Vec<String> = p.iter().map(|&v| self.tok(v)).collect();
        self.d.push_str(&format!("C{},{} {},{} {},{}", t[0], t[1], t[2], t[3], t[4], t[5]));
    }

    fn quad(&mut self, p: [f64; 4]) {
        self.tokens.extend([81.0, 4.0]);
        let t: Vec<String> = p.iter().map(|&v| self.tok(v)).collect();
        self.d.push_str(&format!("Q{},{} {},{}", t[0], t[1], t[2], t[3]));
    }
}

pub fn route_path_default(route: &[f64], curve: Option<&str>, start_trim: f64, end_trim: f64) -> EdgePath {
    route_path(
        route,
        curve,
        start_trim,
        end_trim,
        if start_trim > 0.0 { start_trim + LEAD } else { 0.0 },
        if end_trim > 0.0 { end_trim + LEAD } else { 0.0 },
    )
}

// Builds the path for a route of x, y, axis triples, trimming both ends to leave room for markers.
// Between two points that are not in line, a route is one S that leaves and arrives along the
// flow, or a step in a stepped route. Where the route itself turns a corner, a stepped route
// rounds it a little, and any other but a linear one turns as widely as it can: the turn takes
// the whole of a leg that ends at a node and half of a leg it shares with the next turn.
// `start_reach` and `end_reach` say how far from its nodes the edge starts to turn, where the
// other edges on the same side of a node need it to.
pub fn route_path(route: &[f64], curve: Option<&str>, start_trim: f64, end_trim: f64, start_reach: f64, end_reach: f64) -> EdgePath {
    let linear = curve == Some("linear");
    let stepped = matches!(curve, Some("step") | Some("stepBefore") | Some("stepAfter"));
    let mut r = Route { xs: Vec::new(), ys: Vec::new(), axes: Vec::new(), linear };
    for p in route.chunks_exact(3) {
        let (x, y, axis) = (p[0], p[1], p[2]);
        // A stepped route turns at right angles where a smooth one would curve.
        if stepped && !r.xs.is_empty() {
            let px = *r.xs.last().unwrap();
            let py = *r.ys.last().unwrap();
            if (x - px).abs() >= 0.01 && (y - py).abs() >= 0.01 {
                let my = match curve {
                    Some("stepBefore") => py,
                    Some("stepAfter") => y,
                    _ => (py + y) / 2.0,
                };
                let mx = match curve {
                    Some("stepBefore") => px,
                    Some("stepAfter") => x,
                    _ => (px + x) / 2.0,
                };
                let turns = if axis == 0.0 { [px, my, x, my] } else { [mx, py, mx, y] };
                for k in [0, 2] {
                    if near(turns[k], turns[k + 1], px, py) || near(turns[k], turns[k + 1], x, y) {
                        continue;
                    }
                    r.xs.push(turns[k]);
                    r.ys.push(turns[k + 1]);
                    r.axes.push(axis);
                }
            }
        }
        r.xs.push(x);
        r.ys.push(y);
        r.axes.push(axis);
    }
    let mut last = r.xs.len() - 1;
    let (sdx, sdy) = r.unit(0, 1, r.axes[1]);
    let (edx, edy) = r.unit(last - 1, last, r.axes[last]);
    let (sx, sy, ex, ey) = (r.xs[0], r.ys[0], r.xs[last], r.ys[last]);
    r.xs[0] += sdx * start_trim;
    r.ys[0] += sdy * start_trim;
    r.xs[last] -= edx * end_trim;
    r.ys[last] -= edy * end_trim;
    // An edge that curves into a marker runs straight for a little way first, where it has the
    // room, so that the marker does not sit on the bend.
    let arrives = if end_reach > end_trim && (!r.straight(last) || r.turns(last - 1)) { lead(end_reach, end_trim, r.along(last, edx, edy)) } else { 0.0 };
    let leaves = if start_reach > start_trim && (!r.straight(1) || r.turns(1)) { lead(start_reach, start_trim, r.along(1, sdx, sdy)) } else { 0.0 };
    if arrives >= 1.0 {
        r.xs.insert(last, r.xs[last] - edx * arrives);
        r.ys.insert(last, r.ys[last] - edy * arrives);
        r.axes.insert(last, r.axes[last]);
        last += 1;
    }
    if leaves >= 1.0 {
        r.xs.insert(1, r.xs[0] + sdx * leaves);
        r.ys.insert(1, r.ys[0] + sdy * leaves);
        r.axes.insert(1, r.axes[1]);
        last += 1;
    }

    // Keep tokens to avoid parsing d when checking fills.
    let mut w = PathWriter { d: String::new(), tokens: vec![77.0, 2.0], at_x: r.xs[0], at_y: r.ys[0] };
    let (a, b) = (w.tok(r.xs[0]), w.tok(r.ys[0]));
    w.d = format!("M{},{}", a, b);
    for i in 1..=last {
        let x = r.xs[i];
        let y = r.ys[i];
        if !r.straight(i) {
            let down = r.axes[i] == 0.0;
            let along = if down { y - w.at_y } else { x - w.at_x };
            let k = along.abs() * ease(if down { x - w.at_x } else { y - w.at_y }, along);
            let ux = if down { 0.0 } else { sign(along) * k };
            let uy = if down { sign(along) * k } else { 0.0 };
            w.cubic([w.at_x + ux, w.at_y + uy, x - ux, y - uy, x, y]);
            w.at_x = x;
            w.at_y = y;
            continue;
        }
        if !r.turns(i) {
            // A point part of the way along one straight run adds nothing to the path.
            if i < last && r.aligned(i) && r.aligned(i + 1) && (x - r.xs[i - 1]) * (r.xs[i + 1] - x) + (y - r.ys[i - 1]) * (r.ys[i + 1] - y) > 0.0 {
                continue;
            }
            w.line_to(x, y);
            continue;
        }
        // How much of the leg before the turn and of the leg after it the turn takes: all of one
        // that ends at a node, short of the straight run into a marker, and half of one it shares.
        let mut before = if r.turns(i - 1) { r.length(i) / 2.0 } else { r.length(i) };
        let mut after = if r.turns(i + 1) { r.length(i + 1) / 2.0 } else { r.length(i + 1) };
        if stepped {
            before = CORNER.min(before).min(after);
            after = before;
        }
        let (ux, uy) = r.unit(i - 1, i, r.axes[i]);
        let (vx, vy) = r.unit(i, i + 1, r.axes[i + 1]);
        w.line_to(x - ux * before, y - uy * before);
        w.at_x = x + vx * after;
        w.at_y = y + vy * after;
        if stepped {
            w.quad([x, y, w.at_x, w.at_y]);
        } else {
            w.cubic([
                x - ux * before * (1.0 - HANDLE),
                y - uy * before * (1.0 - HANDLE),
                x + vx * after * (1.0 - HANDLE),
                y + vy * after * (1.0 - HANDLE),
                w.at_x,
                w.at_y,
            ]);
        }
    }
    EdgePath { d: w.d, tokens: Some(w.tokens), sx, sy, sdx: -sdx, sdy: -sdy, ex, ey, edx, edy }
}

pub fn marker_trim(kind: &str) -> f64 {
    match kind {
        "arrow_point" => ARROW - 1.0,
        "arrow_circle" => ARROW,
        "arrow_cross" => 4.0,
        _ => 0.0,
    }
}

// Draws a marker whose tip is at (x, y), pointing along (dx, dy).
pub fn marker(kind: &str, x: f64, y: f64, dx: f64, dy: f64, color: &str, stroke: &str) -> String {
    let nx = -dy;
    let ny = dx;
    match kind {
        "arrow_point" => {
            let bx = x - dx * ARROW;
            let by = y - dy * ARROW;
            let half = ARROW * 0.42;
            format!(
                "<path class=\"pele-marker\" d=\"M{},{}L{},{}L{},{}Z\" fill=\"{}\" stroke=\"{}\"/>",
                num(x), num(y), num(bx + nx * half), num(by + ny * half), num(bx - nx * half), num(by - ny * half), color, stroke
            )
        }
        "arrow_circle" => {
            format!("<circle class=\"pele-marker\" cx=\"{}\" cy=\"{}\" r=\"3.5\" fill=\"{}\" stroke=\"{}\"/>", num(x - dx * 4.0), num(y - dy * 4.0), color, stroke)
        }
        "arrow_cross" => {
            let cx = x - dx * 4.0;
            let cy = y - dy * 4.0;
            let a = 3.5;
            let ux = (dx + nx) * a;
            let uy = (dy + ny) * a;
            let vx = (dx - nx) * a;
            let vy = (dy - ny) * a;
            format!(
                "<path class=\"pele-marker\" d=\"M{},{}L{},{}M{},{}L{},{}\"/>",
                num(cx - ux), num(cy - uy), num(cx + ux), num(cy + uy), num(cx - vx), num(cy - vy), num(cx + vx), num(cy + vy)
            )
        }
        _ => String::new(),
    }
}

// Each command stores its character code, argument count, and arguments.
fn tokenize(d: &str) -> Vec<f64> {
    let b = d.as_bytes();
    let mut out: Vec<f64> = Vec::new();
    let mut at: Option<usize> = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_uppercase() && c != b'E' {
            at = Some(out.len() + 1);
            out.push(c as f64);
            out.push(0.0);
            i += 1;
        } else if c == b'-' || c == b'.' || c.is_ascii_digit() {
            let mut j = i + 1;
            let mut dot = c == b'.';
            while j < b.len() {
                let k = b[j];
                let exp = (k == b'e' || k == b'E') && j + 1 < b.len();
                let exp_sign = (k == b'-' || k == b'+') && matches!(b[j - 1], b'e' | b'E');
                if k.is_ascii_digit() || exp || exp_sign {
                    j += 1;
                } else if k == b'.' && !dot {
                    dot = true;
                    j += 1;
                } else {
                    break;
                }
            }
            if let (Some(at), Ok(v)) = (at, d[i..j].parse::<f64>()) {
                out.push(v);
                out[at] += 1.0;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

// Samples curves and arcs; NaN separates subpaths.
fn sample(tokens: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut x = 0.0;
    let mut y = 0.0;
    let mut i = 0;
    while i + 1 < tokens.len() {
        let cmd = tokens[i] as u8;
        let count = tokens[i + 1] as usize;
        let s = i + 2;
        let e = (s + count).min(tokens.len());
        i = e;
        match cmd {
            b'H' | b'V' => {
                for &v in &tokens[s..e] {
                    if cmd == b'H' {
                        x = v;
                    } else {
                        y = v;
                    }
                    out.extend([x, y]);
                }
            }
            b'C' | b'Q' => {
                let cubic = cmd == b'C';
                let step = if cubic { 6 } else { 4 };
                let mut k = s;
                while k + step <= e {
                    let (x1, y1, x2, y2) = (tokens[k], tokens[k + 1], tokens[k + 2], tokens[k + 3]);
                    let x3 = if cubic { tokens[k + 4] } else { x2 };
                    let y3 = if cubic { tokens[k + 5] } else { y2 };
                    for n in 1..=8 {
                        let t = n as f64 * 0.125;
                        // Preserve the original De Casteljau operation order to keep rounding unchanged.
                        let mut ax = x + (x1 - x) * t;
                        let mut ay = y + (y1 - y) * t;
                        let mut bx = x1 + (x2 - x1) * t;
                        let mut by = y1 + (y2 - y1) * t;
                        if cubic {
                            let cx = x2 + (x3 - x2) * t;
                            let cy = y2 + (y3 - y2) * t;
                            ax += (bx - ax) * t;
                            ay += (by - ay) * t;
                            bx += (cx - bx) * t;
                            by += (cy - by) * t;
                        }
                        out.extend([ax + (bx - ax) * t, ay + (by - ay) * t]);
                    }
                    x = x3;
                    y = y3;
                    k += step;
                }
            }
            b'A' => {
                let mut k = s;
                while k + 7 <= e {
                    arc(&mut out, x, y, tokens[k], tokens[k + 1], tokens[k + 3], tokens[k + 4], tokens[k + 5], tokens[k + 6]);
                    x = tokens[k + 5];
                    y = tokens[k + 6];
                    k += 7;
                }
            }
            _ => {
                let mut k = s;
                while k + 2 <= e {
                    if cmd == b'M' && k == s && !out.is_empty() {
                        out.extend([f64::NAN, f64::NAN]);
                    }
                    x = tokens[k];
                    y = tokens[k + 1];
                    out.extend([x, y]);
                    k += 2;
                }
            }
        }
    }
    out
}

// Samples an unrotated arc from (x1, y1), after SVG's conversion from end points to a center.
#[allow(clippy::too_many_arguments)]
fn arc(out: &mut Vec<f64>, x1: f64, y1: f64, mut rx: f64, mut ry: f64, large: f64, sweep: f64, x2: f64, y2: f64) {
    let hx = (x1 - x2) / 2.0;
    let hy = (y1 - y2) / 2.0;
    if rx == 0.0 || ry == 0.0 || (hx == 0.0 && hy == 0.0) {
        out.extend([x2, y2]);
        return;
    }
    let scale = ((hx * hx) / (rx * rx) + (hy * hy) / (ry * ry)).sqrt().max(1.0);
    rx *= scale;
    ry *= scale;
    let k = (if large == sweep { -1.0 } else { 1.0 })
        * ((rx * rx * ry * ry - rx * rx * hy * hy - ry * ry * hx * hx) / (rx * rx * hy * hy + ry * ry * hx * hx)).max(0.0).sqrt();
    let cx = (k * rx * hy) / ry + (x1 + x2) / 2.0;
    let cy = (-k * ry * hx) / rx + (y1 + y2) / 2.0;
    let from = ((y1 - cy) / ry).atan2((x1 - cx) / rx);
    let mut turn = ((y2 - cy) / ry).atan2((x2 - cx) / rx) - from;
    if sweep != 0.0 && turn < 0.0 {
        turn += 2.0 * std::f64::consts::PI;
    } else if sweep == 0.0 && turn > 0.0 {
        turn -= 2.0 * std::f64::consts::PI;
    }
    for i in 1..=8 {
        let a = from + (turn * i as f64) / 8.0;
        out.extend([cx + rx * a.cos(), cy + ry * a.sin()]);
    }
}

// Test segment overlap with the box using Liang-Barsky clipping.
#[allow(clippy::too_many_arguments)]
fn clips(x: f64, y: f64, dx: f64, dy: f64, l: f64, t: f64, r: f64, b: f64) -> bool {
    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    for (p, q) in [(-dx, x - l), (dx, r - x), (-dy, y - t), (dy, b - y)] {
        if p == 0.0 {
            if q < 0.0 {
                hi = -1.0;
            }
        } else if p < 0.0 {
            lo = lo.max(q / p);
        } else {
            hi = hi.min(q / p);
        }
    }
    lo < hi
}

fn enters(points: &[f64], l: f64, t: f64, r: f64, b: f64) -> bool {
    let mut k = 0;
    while k + 3 < points.len() {
        let (x, y, x2, y2) = (points[k], points[k + 1], points[k + 2], points[k + 3]);
        k += 2;
        if (x < l && x2 < l) || (x > r && x2 > r) || (y < t && y2 < t) || (y > b && y2 > b) {
            continue;
        }
        if clips(x, y, x2 - x, y2 - y, l, t, r, b) {
            return true;
        }
    }
    false
}

// Fill bounds are [left, top, right, bottom].
#[derive(Debug, Clone)]
pub struct Fill {
    pub bounds: [f64; 4],
    pub shape: String,
}

pub enum Under<'a> {
    // Off every fill.
    None,
    // Crosses a fill's border.
    Border,
    // The innermost fill holding the whole edge and its marker tips.
    Fill(&'a Fill),
}

pub fn fill_under<'a>(d: &str, tips: &[f64], fills: &'a [Fill], tokens: Option<&[f64]>) -> Under<'a> {
    if fills.is_empty() {
        return Under::None;
    }
    let points = match tokens {
        Some(t) => sample(t),
        None => sample(&tokenize(d)),
    };
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for pair in points.chunks_exact(2).chain(tips.chunks_exact(2)) {
        // Comparisons with NaN are false, so subpath separators are skipped.
        if pair[0] < min_x {
            min_x = pair[0];
        }
        if pair[0] > max_x {
            max_x = pair[0];
        }
        if pair[1] < min_y {
            min_y = pair[1];
        }
        if pair[1] > max_y {
            max_y = pair[1];
        }
    }
    let mut holder = None;
    let mut least = f64::INFINITY;
    for fill in fills {
        let [l, t, r, b] = fill.bounds;
        if max_x < l + 0.5 || min_x > r - 0.5 || max_y < t + 0.5 || min_y > b - 0.5 {
            continue;
        }
        let mut touched = enters(&points, l + 0.5, t + 0.5, r - 0.5, b - 0.5);
        for tip in tips.chunks_exact(2) {
            if touched {
                break;
            }
            touched = tip[0] > l + 0.5 && tip[0] < r - 0.5 && tip[1] > t + 0.5 && tip[1] < b - 0.5;
        }
        if !touched {
            continue;
        }
        if min_x < l || max_x > r || min_y < t || max_y > b {
            return Under::Border;
        }
        let area = (r - l) * (b - t);
        if area < least {
            least = area;
            holder = Some(fill);
        }
    }
    holder.map_or(Under::None, Under::Fill)
}

// The fill's ink for an edge on it, or '' for a casing when unknown or when the line style sets a stroke.
pub fn edge_ink(fill: &Fill, line: &str) -> &'static str {
    if declaration(line, "stroke").is_some() {
        ""
    } else {
        ink(&fill.shape, "").unwrap_or("")
    }
}

// Markers with the page color inside them, with the fill's color there instead.
pub fn hollow_on(markers: &str, fill: &Fill) -> String {
    match declaration(&fill.shape, "fill").map(str::trim).filter(|c| !c.is_empty()) {
        Some(color) => markers.replace(" fill=\"var(--_bg)\"", &format!(" fill=\"{}\"", color)),
        None => markers.to_string(),
    }
}

// The ink of the innermost fill that holds the whole edge, `Some("")` for a casing where it crosses a border, or `None` off fills.
pub fn over_fills(d: &str, tips: &[f64], fills: &[Fill], line: &str, tokens: Option<&[f64]>) -> Option<&'static str> {
    match fill_under(d, tips, fills, tokens) {
        Under::None => None,
        Under::Border => Some(""),
        Under::Fill(fill) => Some(edge_ink(fill, line)),
    }
}

const CASE: f64 = 4.0;

fn plain_length(width: &str) -> bool {
    let digits = width.strip_suffix("px").unwrap_or(width);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

// A wider page-colored stroke under an edge, as faint as it; markers drop their class so marker styles skip it.
pub fn casing(d: &str, markers: &str, line: &str, thick: bool) -> String {
    let decl = |prop: &str| declaration(line, prop).map(str::trim).filter(|v| !v.is_empty());
    let stroke = decl("stroke");
    if stroke == Some("none") || stroke == Some("transparent") {
        return String::new();
    }
    let width = if thick { Some("2.5") } else { decl("stroke-width") };
    let mut attrs = String::new();
    let mut style = String::new();
    if let Some(width) = width {
        if plain_length(width) {
            let n = super::theme::leading_float(width).unwrap_or(1.0);
            if n != 1.0 {
                attrs = format!(" stroke-width=\"{}\"", num(n + CASE));
            }
        } else {
            style.push_str(&format!("stroke-width:calc({} + {}px);", width, CASE));
        }
    }
    for prop in ["opacity", "stroke-opacity"] {
        if let Some(value) = decl(prop) {
            style.push_str(&format!("{}:{};", prop, value));
        }
    }
    let body = format!("<path d=\"{}\"/>{}", d, markers.replace(" class=\"pele-marker\"", ""));
    if attrs.is_empty() && style.is_empty() {
        body
    } else {
        let style = if style.is_empty() { String::new() } else { format!(" style=\"{}\"", style) };
        format!("<g{}{}>{}</g>", attrs, style, body)
    }
}

pub fn edge_casing(path: &EdgePath, start_type: &str, end_type: &str, line: &str, thick: bool) -> String {
    let markers = marker(start_type, path.sx, path.sy, path.sdx, path.sdy, "var(--_bg)", "var(--_bg)")
        + &marker(end_type, path.ex, path.ey, path.edx, path.edy, "var(--_bg)", "var(--_bg)");
    casing(&path.d, &markers, line, thick)
}

pub fn casings_svg(casings: &str) -> String {
    if casings.is_empty() {
        String::new()
    } else {
        format!(
            "<g class=\"pele-casings\" fill=\"none\" stroke=\"var(--_bg)\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" pointer-events=\"none\">{}</g>",
            num(1.0 + CASE),
            casings
        )
    }
}

// An edge label on a patch of the background, so the line does not run through the text.
pub fn edge_label_svg(id: &str, label: &Label, x: f64, y: f64, attrs: &str) -> String {
    let w = label.width + 8.0;
    let h = label.height;
    format!(
        "<g class=\"pele-edge-label\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" fill=\"var(--_bg)\"/>{}</g>",
        esc_text(id),
        num(x - w / 2.0),
        num(y - h / 2.0),
        num(w),
        num(h),
        label_svg(label, x, y, attrs)
    )
}

pub struct LoopLabel {
    pub x: f64,
    pub y: f64,
}

// A self-loop beside a node of drawn size w by h: out of the side that lies across the flow and
// back in. `k` counts the loops already on the node, so each one reaches a little further.
// Also places the edge's label beyond the loop.
#[allow(clippy::too_many_arguments)]
pub fn loop_path(cx: f64, cy: f64, w: f64, h: f64, k: f64, sideways: bool, base: f64, trim: f64, label_w: f64, label_h: f64) -> (EdgePath, LoopLabel) {
    let half = (if sideways { w } else { h }) / 2.0;
    let spread = (half - 4.0).min(8.0 + k * 6.0);
    let reach = base + k * 8.0;
    let len = reach.hypot(spread);
    let len = if len == 0.0 { 1.0 } else { len };
    // Local frame: `out` points away from the node, `along` runs along its side.
    let at = |out: f64, along: f64| {
        if sideways {
            format!("{},{}", num(cx + along), num(cy + h / 2.0 + out))
        } else {
            format!("{},{}", num(cx + w / 2.0 + out), num(cy + along))
        }
    };
    let tx = (reach / len) * trim;
    let ty = (spread / len) * trim;
    let label = if sideways {
        LoopLabel { x: cx, y: cy + h / 2.0 + reach + 4.0 + label_h / 2.0 }
    } else {
        LoopLabel { x: cx + w / 2.0 + reach + 4.0 + label_w / 2.0, y: cy }
    };
    let path = EdgePath {
        d: format!("M{}C{} {} {}", at(0.0, -spread), at(reach, -spread * 2.0), at(reach, spread * 2.0), at(tx, spread + ty)),
        tokens: None,
        sx: if sideways { cx - spread } else { cx + w / 2.0 },
        sy: if sideways { cy + h / 2.0 } else { cy - spread },
        sdx: if sideways { 0.0 } else { -1.0 },
        sdy: if sideways { -1.0 } else { 0.0 },
        ex: if sideways { cx + spread } else { cx + w / 2.0 },
        ey: if sideways { cy + h / 2.0 } else { cy + spread },
        edx: if sideways { -spread / len } else { -reach / len },
        edy: if sideways { -reach / len } else { -spread / len },
    };
    (path, label)
}
