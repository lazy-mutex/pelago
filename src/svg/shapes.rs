use super::builder::{js_number, num};
use super::theme::rounded;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeSize {
    pub w: f64,
    pub h: f64,
    // Vertical shift of the label from the shape's center.
    pub dy: f64,
}

pub const PAD_X: f64 = 18.0;
pub const PAD_Y: f64 = 12.0;

// Kept clear at each end of a straight side, for the corner.
const CORNER: f64 = 12.0;

const NAMES: &[&str] = &[
    "rect", "rounded", "stadium", "fr-rect", "cyl", "circle", "dbl-circ", "ellipse", "diam", "hex", "odd", "lean-r", "lean-l", "trap-b",
    "trap-t", "text", "sm-circ", "f-circ", "fr-circ", "fork", "notch-rect", "lin-rect", "div-rect", "win-pane", "tri", "flip-tri", "sl-rect",
    "notch-pent", "hourglass", "delay", "h-cyl", "cross-circ", "st-rect", "doc", "flag", "datastore", "folder", "bucket", "console", "browser",
    "person", "bang", "cloud", "brace", "brace-r", "braces", "bolt", "lin-cyl", "curv-trap", "docs", "bow-rect", "tag-rect", "tag-doc",
    "lin-doc", "collapsedGroup",
];

// An unknown shape is drawn as a rectangle.
fn known(shape: &str) -> &str {
    if NAMES.contains(&shape) {
        shape
    } else {
        "rect"
    }
}

// How far an ellipse's outline has fallen away from its bounding box, `t` from the middle of a side `half` long.
fn bulge(t: f64, half: f64, depth: f64) -> f64 {
    let k = (t.abs() / half).min(1.0);
    depth * (1.0 - (1.0 - k * k).sqrt())
}

fn cap_ry(w: f64) -> f64 {
    (w / 14.0).max(6.0).min(12.0)
}

fn sz(w: f64, h: f64, dy: f64) -> ShapeSize {
    ShapeSize { w, h, dy }
}

fn tri_size(tw: f64, th: f64, flip: bool) -> ShapeSize {
    let w = (1.6 * tw + 40.0).max(80.0);
    let h = (th + 10.0) / (1.0 - (tw + 8.0) / w);
    let dy = h / 2.0 - 10.0 - th / 2.0;
    sz(w, h, if flip { -dy } else { dy })
}

pub fn shape_size(shape: &str, tw: f64, th: f64) -> ShapeSize {
    let h = th + 2.0 * PAD_Y;
    let wide = tw + 2.0 * PAD_X;
    match known(shape) {
        "rect" | "rounded" | "notch-rect" | "datastore" | "tag-rect" => sz(wide.max(2.0 * PAD_X + 12.0), h, 0.0),
        "stadium" => sz(tw + h * 0.5 + 2.0 * PAD_X - 8.0, h, 0.0),
        "fr-rect" => sz(wide + 16.0, h, 0.0),
        "cyl" => {
            let w = wide.max(56.0);
            let ry = cap_ry(w);
            sz(w, h + 2.0 * ry, ry / 2.0)
        }
        "circle" => {
            let d = ((tw * tw + th * th).sqrt() + 16.0).max(44.0);
            sz(d, d, 0.0)
        }
        "dbl-circ" => {
            let d = ((tw * tw + th * th).sqrt() + 26.0).max(54.0);
            sz(d, d, 0.0)
        }
        "ellipse" => sz(tw * 1.3 + 2.0 * PAD_X, th * 1.2 + 2.0 * PAD_Y, 0.0),
        "diam" => {
            let a = tw + 16.0;
            let b = th + 8.0;
            sz(a + 1.6 * b, a / 1.6 + b, 0.0)
        }
        "hex" | "lean-r" | "lean-l" | "trap-b" | "trap-t" => sz(wide + h / 2.0, h, 0.0),
        "odd" | "delay" => sz(wide + h / 4.0, h, 0.0),
        "text" => sz(tw + 8.0, th + 8.0, 0.0),
        "sm-circ" | "f-circ" => sz(14.0, 14.0, 0.0),
        "fr-circ" => sz(18.0, 18.0, 0.0),
        "fork" => sz(72.0, 8.0, 0.0),
        "lin-rect" => sz(wide + 8.0, h, 0.0),
        "div-rect" => sz(wide, h + 8.0, 4.0),
        "win-pane" | "st-rect" => sz(wide + 8.0, h + 8.0, 4.0),
        "tri" => tri_size(tw, th, false),
        "flip-tri" => tri_size(tw, th, true),
        "sl-rect" => sz(wide, h + 10.0, 5.0),
        "notch-pent" => sz(wide, h, 0.0),
        "hourglass" => sz(32.0, 40.0, 0.0),
        "h-cyl" => sz(wide + 2.0 * (h / 4.0).min(12.0), h, 0.0),
        "cross-circ" => sz(44.0, 44.0, 0.0),
        "doc" | "tag-doc" => sz(wide, h + 10.0, -5.0),
        "flag" => sz(wide, h + 16.0, 0.0),
        "folder" => sz(wide.max(64.0), h + 8.0, 4.0),
        "bucket" => sz(wide + 20.0, h + 10.0, 3.0),
        "console" | "browser" => sz(wide.max(72.0), h + 12.0, 6.0),
        "person" => sz(wide.max(56.0), h + 26.0, 13.0),
        "bang" => sz(wide + 28.0, h + 24.0, 0.0),
        "cloud" => sz(wide + 32.0, h + 24.0, 0.0),
        "brace" | "brace-r" => sz(wide + 12.0, h, 0.0),
        "braces" => sz(wide + 24.0, h, 0.0),
        "bolt" => sz(36.0, 56.0, 0.0),
        "lin-cyl" => {
            let w = wide.max(56.0);
            let ry = cap_ry(w);
            sz(w, h + 2.0 * ry + 8.0, ry / 2.0 + 4.0)
        }
        "curv-trap" => sz(wide + h * 0.6, h, 0.0),
        "docs" => sz(wide + 8.0, h + 18.0, -1.0),
        "bow-rect" => sz(wide + h * 0.4, h, 0.0),
        "lin-doc" => sz(wide + 8.0, h + 10.0, -5.0),
        "collapsedGroup" => sz(wide + 8.0, h + 8.0, -2.0),
        _ => unreachable!(),
    }
}

// Distance from the bounding box to the outline on a side: 0 top, 1 right, 2 bottom, 3 left.
// `t` is how far along the side from its middle, to the right or down.
pub fn shape_inset(shape: &str, w: f64, h: f64, side: i32, t: f64) -> f64 {
    let odd = side & 1 != 0;
    match shape {
        "stadium" => {
            if odd {
                bulge(t, h / 2.0, h / 2.0)
            } else {
                0.0
            }
        }
        "cyl" | "lin-cyl" => {
            if odd {
                0.0
            } else {
                bulge(t, w / 2.0, cap_ry(w))
            }
        }
        "circle" | "dbl-circ" | "ellipse" | "sm-circ" | "f-circ" | "fr-circ" | "cross-circ" => {
            if odd {
                bulge(t, h / 2.0, w / 2.0)
            } else {
                bulge(t, w / 2.0, h / 2.0)
            }
        }
        "diam" => {
            if odd {
                t.abs() * w / h
            } else {
                t.abs() * h / w
            }
        }
        "hex" => {
            if odd {
                t.abs() / 2.0
            } else {
                0.0
            }
        }
        "odd" => {
            if side == 3 {
                h / 4.0
            } else {
                0.0
            }
        }
        "lean-r" | "lean-l" | "trap-b" | "trap-t" => {
            if odd {
                h / 8.0
            } else {
                0.0
            }
        }
        "tri" | "flip-tri" | "bolt" => {
            if odd {
                w / 4.0
            } else {
                0.0
            }
        }
        "sl-rect" => {
            if side == 0 {
                5.0
            } else {
                0.0
            }
        }
        "hourglass" => {
            if odd {
                w / 2.0
            } else {
                0.0
            }
        }
        "doc" | "docs" | "tag-doc" | "lin-doc" => {
            if side == 2 {
                10.0
            } else {
                0.0
            }
        }
        "flag" => {
            if odd {
                0.0
            } else {
                8.0
            }
        }
        "bucket" => {
            if odd {
                h / 12.0
            } else {
                0.0
            }
        }
        "bang" => 3.0,
        "cloud" => {
            if odd {
                2.0
            } else {
                1.0
            }
        }
        "bow-rect" => {
            if side == 1 {
                h * 0.2
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

// How much of a side of the shape edges may spread along, when the flow runs down or, if `sideways`, across.
pub fn shape_span(shape: &str, w: f64, h: f64, sideways: bool) -> f64 {
    let along = if sideways { h } else { w };
    let flat = along - 2.0 * CORNER;
    // Straight along the top and bottom except for `cut` at each end, and not straight at the sides.
    let across = |cut: f64| if sideways { 0.0 } else { w - 2.0 * cut - 2.0 * CORNER };
    let span = match shape {
        "rect" | "rounded" | "fr-rect" | "text" | "notch-rect" | "lin-rect" | "div-rect" | "win-pane" | "notch-pent" | "datastore"
        | "console" | "browser" | "tag-rect" => flat,
        "stadium" => {
            if sideways {
                h * 0.6
            } else {
                w - h
            }
        }
        "cyl" => {
            if sideways {
                h - 2.0 * CORNER - 12.0
            } else {
                w * 0.6
            }
        }
        "lin-cyl" => {
            if sideways {
                h - 2.0 * CORNER - 20.0
            } else {
                w * 0.6
            }
        }
        "circle" | "dbl-circ" | "ellipse" | "sm-circ" | "f-circ" | "fr-circ" | "cross-circ" | "diam" => along * 0.6,
        "hex" => {
            if sideways {
                h * 0.5
            } else {
                w - h / 2.0 - CORNER
            }
        }
        "odd" => across(0.0),
        "lean-r" | "lean-l" | "trap-b" | "trap-t" => across(h / 4.0),
        "fork" => along - 8.0,
        "st-rect" => along - 2.0 * CORNER - 8.0,
        _ => 0.0,
    };
    span.max(0.0)
}

pub fn shape_has_label(shape: &str) -> bool {
    !matches!(shape, "sm-circ" | "f-circ" | "fr-circ" | "fork" | "hourglass" | "cross-circ" | "bolt")
}

// Whether the shape takes a style's fill.
pub fn shape_filled(shape: &str) -> bool {
    !matches!(shape, "text" | "brace" | "brace-r" | "braces")
}

fn path(d: &str, a: &str) -> String {
    format!("<path d=\"{}\"{}/>", d, a)
}

fn poly(points: &[f64], a: &str) -> String {
    let mut d = String::new();
    for (i, pair) in points.chunks(2).enumerate() {
        if i > 0 {
            d.push(' ');
        }
        d.push_str(&num(pair[0]));
        d.push(',');
        d.push_str(&num(pair[1]));
    }
    format!("<polygon points=\"{}\"{}/>", d, a)
}

fn rect(w: f64, h: f64, a: &str, rx: &str) -> String {
    format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}{}/>", num(-w / 2.0), num(-h / 2.0), num(w), num(h), rx, a)
}

fn brace(x: f64, h: f64, dir: f64, line: &str) -> String {
    let y = h / 2.0;
    let k = 6.0 * dir;
    format!(
        "<path d=\"M{},{}q{},0 {},6V-6q0,6 {},6q{},0 {},6V{}q0,6 {},6\" fill=\"none\"{}/>",
        num(x),
        num(-y),
        num(k),
        num(k),
        num(k),
        num(-k),
        num(-k),
        num(y - 6.0),
        num(-k),
        line
    )
}

fn wave(w: f64, h: f64, a: &str) -> String {
    let x = w / 2.0;
    let top = -h / 2.0;
    let base = h / 2.0 - 10.0;
    let q = w / 4.0;
    format!("<path d=\"M{},{}H{}V{}q{},-12 {},0t{},0Z\"{}/>", num(-x), num(top), num(x), num(base), num(-q), num(-2.0 * q), num(-2.0 * q), a)
}

const BARE: &str = " fill=\"none\" stroke=\"none\"";

// Draws the shape centered on the origin. `a` holds the fill and outline attributes, `line` the outline alone.
pub fn draw_shape(shape: &str, w: f64, h: f64, a: &str, line: &str) -> String {
    let x = w / 2.0;
    let y = h / 2.0;
    let lean = h / 4.0;
    match known(shape) {
        "rect" => rect(w, h, &rounded(a), ""),
        "rounded" => rect(w, h, a, &format!(" rx=\"{}\"", num((h / 2.0).min(12.0)))),
        "stadium" => rect(w, h, a, &format!(" rx=\"{}\"", num(h / 2.0))),
        "fr-rect" => {
            let x = w / 2.0 - 8.0;
            rect(w, h, &rounded(a), "") + &format!("<path d=\"M{},{}V{}M{},{}V{}\" fill=\"none\"{}/>", num(-x), num(-y), num(y), num(x), num(-y), num(y), line)
        }
        "cyl" => {
            let ry = cap_ry(w);
            let top = -y + ry;
            let bottom = y - ry;
            format!(
                "<path d=\"M{},{}A{},{} 0 0 1 {},{}V{}A{},{} 0 0 1 {},{}Z\"{}/>",
                num(-x), num(top), num(x), num(ry), num(x), num(top), num(bottom), num(x), num(ry), num(-x), num(bottom), a
            ) + &format!("<path d=\"M{},{}A{},{} 0 0 0 {},{}\" fill=\"none\"{}/>", num(-x), num(top), num(x), num(ry), num(x), num(top), line)
        }
        "circle" | "sm-circ" => format!("<circle r=\"{}\"{}/>", num(x), a),
        "dbl-circ" => format!("<circle r=\"{}\"{}/><circle r=\"{}\" fill=\"none\"{}/>", num(x), a, num(x - 5.0), line),
        "ellipse" => format!("<ellipse rx=\"{}\" ry=\"{}\"{}/>", num(x), num(y), a),
        "diam" => poly(&[0.0, -y, x, 0.0, 0.0, y, -x, 0.0], a),
        "hex" => poly(&[-x + lean, -y, x - lean, -y, x, 0.0, x - lean, y, -x + lean, y, -x, 0.0], a),
        "odd" => poly(&[-x, -y, x, -y, x, y, -x, y, -x + lean, 0.0], a),
        "lean-r" => poly(&[-x + lean, -y, x, -y, x - lean, y, -x, y], a),
        "lean-l" => poly(&[-x, -y, x - lean, -y, x, y, -x + lean, y], a),
        "trap-b" => poly(&[-x + lean, -y, x - lean, -y, x, y, -x, y], a),
        "trap-t" => poly(&[-x, -y, x, -y, x - lean, y, -x + lean, y], a),
        "text" => rect(w, h, BARE, ""),
        "f-circ" => format!("<circle r=\"{}\" fill=\"var(--_l)\"{}/>", num(x), line),
        "fr-circ" => format!("<circle r=\"{}\"{}/><circle r=\"{}\" fill=\"var(--_l)\"{}/>", num(x), a, num(x - 4.0), line),
        "fork" => rect(w, h, &format!(" fill=\"var(--_l)\"{}", line), " rx=\"2\""),
        "notch-rect" => poly(&[-x + 10.0, -y, x, -y, x, y, -x, y, -x, -y + 10.0], a),
        "lin-rect" => rect(w, h, &rounded(a), "") + &format!("<path d=\"M{},{}V{}\" fill=\"none\"{}/>", num(-x + 8.0), num(-y), num(y), line),
        "div-rect" => rect(w, h, &rounded(a), "") + &format!("<path d=\"M{},{}H{}\" fill=\"none\"{}/>", num(-x), num(-y + 8.0), num(x), line),
        "win-pane" => {
            rect(w, h, &rounded(a), "")
                + &format!("<path d=\"M{},{}V{}M{},{}H{}\" fill=\"none\"{}/>", num(-x + 8.0), num(-y), num(y), num(-x), num(-y + 8.0), num(x), line)
        }
        "tri" => poly(&[0.0, -y, x, y, -x, y], a),
        "flip-tri" => poly(&[-x, -y, x, -y, 0.0, y], a),
        "sl-rect" => poly(&[-x, -y + 10.0, x, -y, x, y, -x, y], a),
        "notch-pent" => poly(&[-x + 10.0, -y, x - 10.0, -y, x, -y + 10.0, x, y, -x, y, -x, -y + 10.0], a),
        "hourglass" => poly(&[-x, -y, x, -y, -x, y, x, y], a),
        "delay" => {
            let r = y;
            format!("<path d=\"M{},{}H{}A{},{} 0 0 1 {},{}H{}Z\"{}/>", num(-x), num(-r), num(x - r), num(r), num(r), num(x - r), num(r), num(-x), a)
        }
        "h-cyl" => {
            let ry = y;
            let rx = (h / 4.0).min(12.0);
            let l = -x + rx;
            let r = x - rx;
            format!(
                "<path d=\"M{},{}H{}A{},{} 0 0 1 {},{}H{}A{},{} 0 0 1 {},{}Z\"{}/>",
                num(l), num(-ry), num(r), num(rx), num(ry), num(r), num(ry), num(l), num(rx), num(ry), num(l), num(-ry), a
            ) + &format!("<path d=\"M{},{}A{},{} 0 0 0 {},{}\" fill=\"none\"{}/>", num(r), num(-ry), num(rx), num(ry), num(r), num(ry), line)
        }
        "cross-circ" => {
            let k = x * std::f64::consts::FRAC_1_SQRT_2;
            format!("<circle r=\"{}\"{}/>", num(x), a)
                + &format!("<path d=\"M{},{}L{},{}M{},{}L{},{}\" fill=\"none\"{}/>", num(-k), num(-k), num(k), num(k), num(k), num(-k), num(-k), num(k), line)
        }
        "st-rect" => {
            let mut out = String::new();
            for i in 0..3 {
                let i = i as f64;
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
                    num(-x + (2.0 - i) * 4.0),
                    num(-y + i * 4.0),
                    num(w - 8.0),
                    num(h - 8.0),
                    rounded(a)
                ));
            }
            out
        }
        "doc" => wave(w, h, a),
        "flag" => {
            let y = h / 2.0 - 8.0;
            let q = w / 4.0;
            format!(
                "<path d=\"M{},{}q{},-10 {},0t{},0V{}q{},10 {},0t{},0Z\"{}/>",
                num(-x), num(-y), num(q), num(2.0 * q), num(2.0 * q), num(y), num(-q), num(-2.0 * q), num(-2.0 * q), a
            )
        }
        "datastore" => {
            rect(w, h, &a.replace("stroke=\"var(--_b)\"", "stroke=\"none\""), "")
                + &format!("<path d=\"M{},{}H{}M{},{}H{}\" fill=\"none\"{}/>", num(-x), num(-y), num(x), num(-x), num(y), num(x), line)
        }
        "folder" => {
            let tab = (w * 0.4).min(48.0);
            path(
                &format!("M{},{}H{}L{},{}H{}V{}H{}Z", num(-x), num(-y), num(-x + tab), num(-x + tab + 8.0), num(-y + 8.0), num(x), num(y), num(-x)),
                a,
            )
        }
        "bucket" => {
            let ry = 5.0;
            let taper = h / 6.0;
            path(
                &format!(
                    "M{},{}A{},{} 0 0 1 {},{}L{},{}A{},{} 0 0 1 {},{}Z",
                    num(-x), num(-y + ry), num(x), num(ry), num(x), num(-y + ry), num(x - taper), num(y - ry), num(x - taper), num(ry), num(-x + taper), num(y - ry)
                ),
                a,
            ) + &format!("<path d=\"M{},{}A{},{} 0 0 0 {},{}\" fill=\"none\"{}/>", num(-x), num(-y + ry), num(x), num(ry), num(x), num(-y + ry), line)
        }
        "console" => {
            rect(w, h, &rounded(a), "")
                + &format!(
                    "<path d=\"M{},{}H{}M{},{}l3,2.5l-3,2.5M{},{}h4\" fill=\"none\"{}/>",
                    num(-x), num(-y + 12.0), num(x), num(-x + 7.0), num(-y + 4.0), num(-x + 13.0), num(-y + 9.0), line
                )
        }
        "browser" => {
            let mut dots = String::new();
            for i in 0..3 {
                dots.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"1.5\" fill=\"none\"{}/>", num(-x + 8.0 + i as f64 * 7.0), num(-y + 6.0), line));
            }
            rect(w, h, &rounded(a), "") + &format!("<path d=\"M{},{}H{}\" fill=\"none\"{}/>", num(-x), num(-y + 12.0), num(x), line) + &dots
        }
        "person" => {
            let top = -y;
            format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"12\"{}/>", num(-x), num(top + 26.0), num(w), num(h - 26.0), a)
                + &format!("<circle cy=\"{}\" r=\"13\"{}/>", num(top + 14.0), a)
        }
        "bang" => {
            let spikes = 14;
            let mut pts = Vec::with_capacity(spikes * 4);
            for i in 0..spikes * 2 {
                let angle = (std::f64::consts::PI * i as f64) / spikes as f64 - std::f64::consts::PI / 2.0;
                let k = if i % 2 == 0 { 1.0 } else { 0.82 };
                pts.push(angle.cos() * x * k);
                pts.push(angle.sin() * y * k);
            }
            poly(&pts, a)
        }
        "cloud" => {
            let r = y * 0.55;
            path(
                &format!(
                    "M{},{}A{},{} 0 0 1 {},{}A{},{} 0 0 1 {},{}A{},{} 0 0 1 {},{}A{},{} 0 0 1 {},{}Z",
                    num(-x + r), num(y), num(r), num(r), num(-x + r * 0.6), num(-y * 0.1),
                    num(x * 0.42), num(y * 0.7), num(-x * 0.1), num(-y * 0.72),
                    num(x * 0.4), num(y * 0.62), num(x * 0.62), num(-y * 0.42),
                    num(r * 1.05), num(r * 1.05), num(x - r), num(y)
                ),
                a,
            )
        }
        "brace" => rect(w, h, BARE, "") + &brace(-x + 12.0, h, -1.0, line),
        "brace-r" => rect(w, h, BARE, "") + &brace(x - 12.0, h, 1.0, line),
        "braces" => rect(w, h, BARE, "") + &brace(-x + 12.0, h, -1.0, line) + &brace(x - 12.0, h, 1.0, line),
        "bolt" => poly(&[w * 0.15, -y, -x, h * 0.08, -w * 0.05, h * 0.08, -w * 0.15, y, x, -h * 0.08, w * 0.05, -h * 0.08], a),
        "lin-cyl" => {
            let ry = cap_ry(w);
            let top = -y + ry;
            let bottom = y - ry;
            let arc = |at: f64| format!("M{},{}A{},{} 0 0 0 {},{}", num(-x), num(at), num(x), num(ry), num(x), num(at));
            path(
                &format!(
                    "M{},{}A{},{} 0 0 1 {},{}V{}A{},{} 0 0 1 {},{}Z",
                    num(-x), num(top), num(x), num(ry), num(x), num(top), num(bottom), num(x), num(ry), num(-x), num(bottom)
                ),
                a,
            ) + &format!("<path d=\"{}{}\" fill=\"none\"{}/>", arc(top), arc(top + 8.0), line)
        }
        "curv-trap" => {
            let k = h * 0.3;
            path(
                &format!("M{},{}H{}A{},{} 0 0 1 {},{}H{}L{},0Z", num(-x + k), num(-y), num(x - k), num(k), num(y), num(x - k), num(y), num(-x + k), num(-x)),
                a,
            )
        }
        "docs" => {
            let mut out = String::new();
            for i in 0..3 {
                out.push_str(&format!(
                    "<g transform=\"translate({},{})\">{}</g>",
                    js_number(((1 - i) * 4) as f64),
                    js_number(((i - 1) * 4) as f64),
                    wave(w - 8.0, h - 8.0, a)
                ));
            }
            out
        }
        "bow-rect" => {
            let k = h * 0.2;
            path(
                &format!(
                    "M{},{}H{}A{},{} 0 0 0 {},{}H{}A{},{} 0 0 1 {},{}Z",
                    num(-x + k), num(-y), num(x), num(k), num(y), num(x), num(y), num(-x + k), num(k), num(y), num(-x + k), num(-y)
                ),
                a,
            )
        }
        "tag-rect" => rect(w, h, &rounded(a), "") + &format!("<path d=\"M{},{}L{},{}\" fill=\"none\"{}/>", num(x - 12.0), num(y), num(x), num(y - 12.0), line),
        "tag-doc" => wave(w, h, a) + &format!("<path d=\"M{},{}L{},{}\" fill=\"none\"{}/>", num(x - 12.0), num(y - 14.0), num(x), num(y - 26.0), line),
        "lin-doc" => wave(w, h, a) + &format!("<path d=\"M{},{}V{}\" fill=\"none\"{}/>", num(-x + 8.0), num(-y), num(y - 6.0), line),
        "collapsedGroup" => {
            format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>", num(-x + 6.0), num(-y + 8.0), num(w - 12.0), num(h - 8.0), rounded(a))
                + &format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>", num(-x), num(-y), num(w), num(h - 4.0), rounded(a))
        }
        _ => unreachable!(),
    }
}

// A node's layout box.
#[derive(Debug, Clone, Copy)]
pub struct BoxAt {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub is_group: bool,
}

// The side of a node's layout box that a route end lies on: 0 top, 1 right, 2 bottom, 3 left, or -1 for none.
pub fn end_side(route: &[f64], at: usize, c: &BoxAt) -> i32 {
    let dx = route[at] - c.x;
    let dy = route[at + 1] - c.y;
    if (dy.abs() - c.h / 2.0).abs() < 0.5 {
        return if dy < 0.0 { 0 } else { 2 };
    }
    if (dx.abs() - c.w / 2.0).abs() < 0.5 {
        return if dx < 0.0 { 3 } else { 1 };
    }
    -1
}

// Moves a route end from a node's layout box onto the outline of the shape drawn in it.
pub fn inset_route(route: &mut [f64], at: usize, shape: &str, w: f64, h: f64, c: &BoxAt) {
    if c.is_group {
        return;
    }
    let side = end_side(route, at, c);
    if side == 0 || side == 2 {
        let amount = shape_inset(shape, w, h, side, route[at] - c.x) + (c.h - h) / 2.0;
        route[at + 1] += if side == 0 { amount } else { -amount };
    } else if side >= 0 {
        let amount = shape_inset(shape, w, h, side, route[at + 1] - c.y) + (c.w - w) / 2.0;
        route[at] += if side == 3 { amount } else { -amount };
    }
}
