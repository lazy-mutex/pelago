use super::common::fold;
use super::{RailroadModel, RailroadNode};
use crate::options::{tighten, title_room, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{FONT_MONO, ROUNDED};
use crate::text::entities::decode_entities;
use crate::text::label::layout_label;
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Terminal,
    NonTerminal,
    Special,
    Sequence,
    Choice,
    Bypass,
    Loop,
    Folded,
}

#[derive(Clone)]
struct RBox {
    kind: Kind,
    width: f64,
    up: f64,
    down: f64,
    kids: Vec<RBox>,
    text: String,
    id: String,
    reach: f64,
}

struct Metrics<'a> {
    measurer: &'a dyn TextMeasurer,
    size: f64,
    padding: f64,
    height: f64,
    gap: f64,
    sep: f64,
    arc: f64,
}

const LEAD: f64 = 12.0;
const HEAD_GAP: f64 = 6.0;
const RULE_GAP: f64 = 18.0;
const COUNT_SIZE: f64 = 12.0;
const ARROW: f64 = 7.0;

fn option(config: &Value, key: &str, fallback: f64, max: f64) -> f64 {
    match config.get("railroad").get(key).as_f64() {
        Some(v) if v.is_finite() && v >= 0.0 => v.min(max),
        Some(v) if v.is_infinite() && v.is_sign_positive() => max,
        _ => fallback,
    }
}

// Text is literal here: a terminal such as "<br>" or "*" is itself, not markup. Mermaid's `#35;`
// codes still stand for characters, and escapes that would be invisible are written out.
fn shown(text: &str) -> String {
    let s = if text.contains("¶ß") {
        decode_entities(&text.replace('&', "&amp;"))
    } else {
        text.to_string()
    };
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

fn even(value: f64) -> f64 {
    (value / 2.0).ceil() * 2.0
}

fn leaf(kind: Kind, text: String, id: String, text_style: u8, m: &Metrics) -> RBox {
    let half = m.height / 2.0;
    let width = even(m.measurer.width(&text, m.size, text_style) + 2.0 * m.padding).max(m.height);
    RBox { kind, width, up: half, down: half, kids: Vec::new(), text, id, reach: 0.0 }
}

fn count_label(min: f64, max: f64) -> String {
    if max.is_infinite() && min <= 1.0 {
        return String::new();
    }
    let fmt = |v: f64| -> String {
        if v.is_finite() {
            js_number(v)
        } else {
            "∞".to_string()
        }
    };
    if min == max {
        format!("{}×", fmt(min))
    } else if max.is_infinite() {
        format!("≥{}×", fmt(min))
    } else if min == 0.0 {
        format!("≤{}×", fmt(max))
    } else {
        format!("{}–{}×", fmt(min), fmt(max))
    }
}

fn below(offset: f64, above: &RBox, branch: &RBox, m: &Metrics) -> f64 {
    (offset + above.down + m.sep + branch.up).max(2.0 * m.arc)
}

fn bypass(kid: RBox, m: &Metrics) -> RBox {
    let reach = (kid.up + m.sep).max(2.0 * m.arc);
    RBox {
        kind: Kind::Bypass,
        width: kid.width + 4.0 * m.arc,
        up: reach,
        down: kid.down,
        kids: vec![kid],
        text: String::new(),
        id: String::new(),
        reach,
    }
}

fn node_parts(node: &RailroadNode) -> Vec<&RailroadNode> {
    match node {
        RailroadNode::Sequence(items) | RailroadNode::Choice(items) => items.iter().collect(),
        RailroadNode::Optional(inner) | RailroadNode::Repetition { element: inner, .. } => vec![inner.as_ref()],
        RailroadNode::Terminal(_) | RailroadNode::NonTerminal(_) | RailroadNode::Special(_) => Vec::new(),
    }
}

fn measure(root: &RailroadNode, m: &Metrics, room: f64) -> RBox {
    let mut budgets: HashMap<usize, f64> = HashMap::new();
    if room.is_finite() {
        let mut pending: Vec<(&RailroadNode, f64)> = vec![(root, room)];
        let mut i = 0;
        while i < pending.len() {
            let (node, width) = pending[i];
            budgets.insert(node as *const RailroadNode as usize, width);
            let around = match node {
                RailroadNode::Choice(_) | RailroadNode::Optional(_) => 4.0 * m.arc,
                RailroadNode::Repetition { min, .. } => (if *min == 0.0 { 6.0 } else { 2.0 }) * m.arc,
                RailroadNode::Sequence(_) => 6.0 * m.arc,
                _ => 0.0,
            };
            for child in node_parts(node) {
                pending.push((child, (width - around).max(40.0)));
            }
            i += 1;
        }
    }

    fold(root, node_parts, |node, mut kids: Vec<RBox>| {
        let room = budgets.get(&(node as *const RailroadNode as usize)).copied().unwrap_or(f64::INFINITY);
        match node {
            RailroadNode::Terminal(val) => {
                let text = shown(val);
                let display = if !text.is_empty() && text.bytes().all(|b| b == b' ') {
                    "␣".repeat(text.len())
                } else {
                    text
                };
                leaf(Kind::Terminal, display, String::new(), style::MONO, m)
            }
            RailroadNode::NonTerminal(name) => leaf(Kind::NonTerminal, shown(name), name.clone(), 0, m),
            RailroadNode::Special(txt) => leaf(Kind::Special, format!("? {} ?", shown(txt)), String::new(), style::ITALIC, m),
            RailroadNode::Sequence(_) => {
                let mut width = (kids.len().saturating_sub(1) as f64) * m.gap;
                let mut up = 0.0f64;
                let mut down = 0.0f64;
                for kid in &kids {
                    width += kid.width;
                    up = up.max(kid.up);
                    down = down.max(kid.down);
                }
                if width <= room || kids.len() < 2 {
                    return RBox { kind: Kind::Sequence, width, up, down, kids, text: String::new(), id: String::new(), reach: 0.0 };
                }
                let mut rows: Vec<RBox> = Vec::new();
                let mut row: Vec<RBox> = Vec::new();
                let mut row_w = 0.0f64;
                let flush = |rows: &mut Vec<RBox>, row: &mut Vec<RBox>, row_w: &mut f64| {
                    let mut u = 0.0f64;
                    let mut d = 0.0f64;
                    for kid in row.iter() {
                        u = u.max(kid.up);
                        d = d.max(kid.down);
                    }
                    rows.push(RBox {
                        kind: Kind::Sequence,
                        width: *row_w,
                        up: u,
                        down: d,
                        kids: std::mem::take(row),
                        text: String::new(),
                        id: String::new(),
                        reach: 0.0,
                    });
                    *row_w = 0.0;
                };
                let row_limit = (room - 6.0 * m.arc).max(40.0);
                for kid in &kids {
                    if !row.is_empty() && row_w + m.gap + kid.width > row_limit {
                        flush(&mut rows, &mut row, &mut row_w);
                    }
                    row_w += (if !row.is_empty() { m.gap } else { 0.0 }) + kid.width;
                    row.push(kid.clone());
                }
                flush(&mut rows, &mut row, &mut row_w);
                if rows.len() < 2 {
                    return RBox { kind: Kind::Sequence, width, up, down, kids, text: String::new(), id: String::new(), reach: 0.0 };
                }
                let mut folded_width = 0.0f64;
                let mut offset = 0.0f64;
                for i in 0..rows.len() {
                    folded_width = folded_width.max(rows[i].width);
                    if i > 0 {
                        offset += rows[i - 1].down + m.sep.max(2.0 * m.arc) + rows[i].up;
                    }
                }
                let first_up = rows[0].up;
                let last_down = rows.last().unwrap().down;
                RBox {
                    kind: Kind::Folded,
                    width: folded_width + 6.0 * m.arc,
                    up: first_up,
                    down: offset + last_down,
                    kids: rows,
                    text: String::new(),
                    id: String::new(),
                    reach: 0.0,
                }
            }
            RailroadNode::Choice(_) => {
                let mut inner = 0.0f64;
                let mut offset = 0.0f64;
                for i in 0..kids.len() {
                    inner = inner.max(kids[i].width);
                    if i > 0 {
                        offset = below(offset, &kids[i - 1], &kids[i], m);
                    }
                }
                let up = kids.first().map_or(0.0, |k| k.up);
                let down = kids.last().map_or(0.0, |k| offset + k.down);
                RBox {
                    kind: Kind::Choice,
                    width: inner + 4.0 * m.arc,
                    up,
                    down,
                    kids,
                    text: String::new(),
                    id: String::new(),
                    reach: 0.0,
                }
            }
            RailroadNode::Optional(_) => bypass(kids.pop().unwrap(), m),
            RailroadNode::Repetition { min, max, .. } => {
                let kid = kids.pop().unwrap();
                if *min == 1.0 && *max == 1.0 {
                    return kid;
                }
                let text = count_label(*min, *max);
                let reach = (kid.down + m.sep).max(2.0 * m.arc);
                let label_w = if text.is_empty() { 0.0 } else { even(m.measurer.width(&text, COUNT_SIZE, 0)) };
                let loop_box = RBox {
                    kind: Kind::Loop,
                    width: kid.width.max(label_w) + 2.0 * m.arc,
                    up: kid.up,
                    down: reach + if text.is_empty() { ARROW / 2.0 + 1.0 } else { COUNT_SIZE + 6.0 },
                    kids: vec![kid],
                    text,
                    id: String::new(),
                    reach,
                };
                if *min == 0.0 {
                    bypass(loop_box, m)
                } else {
                    loop_box
                }
            }
        }
    })
}

pub fn render_railroad(model: &RailroadModel, config: &Value, options: &RenderOptions) -> Rendered {
    tighten(options, &|tight| draw(model, config, options, tight))
}

fn draw(model: &RailroadModel, config: &Value, options: &RenderOptions, tight: bool) -> Rendered {
    let base = options.font_size.unwrap_or(16.0);
    let font_size = option(config, "fontSize", 0.0, 200.0);
    let size = if font_size > 0.0 { font_size } else { js_round(base * 0.875) };
    let head_size = if font_size > 0.0 { font_size } else { base };
    let raw_padding = match config.get("railroad").get("padding").as_f64() {
        Some(v) if v.is_finite() && v >= 0.0 => v.min(200.0),
        Some(v) if v.is_infinite() && v.is_sign_positive() => 200.0,
        _ => -1.0,
    };
    let line_height = js_round(size * 1.5);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let m = Metrics {
        measurer,
        size,
        padding: if raw_padding < 0.0 { 12.0 } else { raw_padding },
        height: even(line_height + if raw_padding < 0.0 { 9.0 } else { 2.0 * raw_padding }),
        gap: option(config, "horizontalSeparation", 16.0, 500.0).min(if tight { 12.0 } else { f64::INFINITY }),
        sep: option(config, "verticalSeparation", 10.0, 500.0),
        arc: option(config, "arcRadius", 10.0, 200.0).min(if tight { 6.0 } else { f64::INFINITY }),
    };
    let radius = if config.get("railroad").get("showMarkers").as_bool() == Some(false) {
        0.0
    } else {
        option(config, "markerRadius", 4.0, 50.0)
    };
    let stroke_width = option(config, "strokeWidth", 0.0, 20.0);
    let stroke = if stroke_width > 0.0 {
        format!(" stroke-width=\"{}\"", num(stroke_width))
    } else {
        String::new()
    };
    let pad = options.padding.unwrap_or(8.0);
    let arc = m.arc;

    let a_str = num(arc);
    let turn = |sweep: u8, dx: &str, dy: &str| format!("a{a},{a} 0 0 {sweep} {dx}{a},{dy}{a}", a = a_str);
    let east_to_south = turn(1, "", "");
    let south_to_east = turn(0, "", "");
    let east_to_north = turn(0, "", "-");
    let north_to_east = turn(1, "", "-");
    let loop_down = turn(0, "-", "");
    let loop_up = turn(0, "-", "-");

    let title = layout_label(model.title.as_deref(), false, m.measurer, base, if tight { title_room(options) } else { 4000.0 }, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let head_height = js_round(head_size * 1.5);
    let text_shift = size * 0.35;
    let box_top = m.height / 2.0;

    let mut rules_svg = String::new();
    let mut width = title.width;
    let mut top = pad + title_height;
    let mut bottom = pad + title.height;

    for rule in &model.rules {
        let room = if tight {
            (options.max_width.unwrap_or(320.0) - 2.0 * pad - 4.0 * radius - 2.0 * LEAD - 1.0).max(80.0)
        } else {
            f64::INFINITY
        };
        let rbox = measure(&rule.definition, &m, room);
        let name = shown(&rule.name);
        let baseline = top + head_height + HEAD_GAP + rbox.up;
        let start = pad + 2.0 * radius;
        let left = start + LEAD;
        let end = left + rbox.width + LEAD;

        let mut track = String::new();
        let mut arrows = String::new();
        let mut nodes = String::new();

        let add_line = |track: &mut String, x1: f64, y: f64, x2: f64| {
            if x2 > x1 {
                track.push_str(&format!("M{},{}H{}", num(x1), num(y), num(x2)));
            }
        };

        add_line(&mut track, start, baseline, left);
        add_line(&mut track, left + rbox.width, baseline, end);

        let mut stack: Vec<(&RBox, f64, f64)> = vec![(&rbox, left, baseline)];
        while let Some((b, x, y)) = stack.pop() {
            let kids = &b.kids;
            let right = x + b.width;
            match b.kind {
                Kind::Sequence => {
                    let mut kx = right;
                    for i in (0..kids.len()).rev() {
                        kx -= kids[i].width;
                        stack.push((&kids[i], kx, y));
                        if i > 0 {
                            add_line(&mut track, kx - m.gap, y, kx);
                        }
                        kx -= m.gap;
                    }
                }
                Kind::Folded => {
                    let row_start = x + 2.0 * arc;
                    let from = stack.len();
                    let mut offset = 0.0;
                    add_line(&mut track, x, y, row_start);
                    for i in 0..kids.len() {
                        let kid = &kids[i];
                        let ky = y + offset;
                        stack.push((kid, row_start, ky));
                        if i + 1 < kids.len() {
                            let next = &kids[i + 1];
                            let next_offset = offset + kid.down + m.sep.max(2.0 * arc) + next.up;
                            let mid = y + offset + kid.down + m.sep.max(2.0 * arc) / 2.0;
                            track.push_str(&format!(
                                "M{},{}H{}V{}H{}V{}H{}",
                                num(row_start + kid.width),
                                num(ky),
                                num(right - 2.0 * arc),
                                num(mid),
                                num(x + arc),
                                num(y + next_offset),
                                num(row_start)
                            ));
                            arrows.push_str(&format!(
                                "M{},{}l{},{}l{},{}z",
                                num(row_start - ARROW),
                                num(y + next_offset - ARROW / 2.0),
                                js_number(ARROW),
                                js_number(ARROW / 2.0),
                                js_number(-ARROW),
                                js_number(ARROW / 2.0)
                            ));
                            offset = next_offset;
                        } else {
                            track.push_str(&format!(
                                "M{},{}H{}V{}H{}",
                                num(row_start + kid.width),
                                num(ky),
                                num(right - arc),
                                num(y),
                                num(right)
                            ));
                        }
                    }
                    stack[from..].reverse();
                }
                Kind::Choice => {
                    let inner = b.width - 4.0 * arc;
                    let rail_in = x + arc;
                    let rail_out = right - arc;
                    let from = stack.len();
                    let mut offset = 0.0;
                    for i in 0..kids.len() {
                        let kid = &kids[i];
                        if i > 0 {
                            offset = below(offset, &kids[i - 1], kid, &m);
                        }
                        let ky = y + offset;
                        let kx = x + 2.0 * arc + (inner - kid.width) / 2.0;
                        let last = i + 1 == kids.len();
                        if i == 0 {
                            add_line(&mut track, x, y, kx);
                            add_line(&mut track, kx + kid.width, y, right);
                        } else {
                            if last {
                                track.push_str(&format!("M{},{}{}V{}", num(x), num(y), east_to_south, num(ky - arc)));
                            } else {
                                track.push_str(&format!("M{},{}", num(rail_in), num(ky - arc)));
                            }
                            track.push_str(&south_to_east);
                            if kx > rail_in + arc {
                                track.push_str(&format!("H{}", num(kx)));
                            }
                            track.push_str(&format!("M{},{}", num(kx + kid.width), num(ky)));
                            if kx > rail_in + arc {
                                track.push_str(&format!("H{}", num(rail_out - arc)));
                            }
                            track.push_str(&east_to_north);
                            if last {
                                track.push_str(&format!("V{}{}", num(y + arc), north_to_east));
                            }
                        }
                        stack.push((kid, kx, ky));
                    }
                    stack[from..].reverse();
                }
                Kind::Bypass => {
                    let kid = &kids[0];
                    let kx = x + 2.0 * arc;
                    add_line(&mut track, x, y, kx);
                    add_line(&mut track, kx + kid.width, y, right);
                    track.push_str(&format!(
                        "M{},{}{}V{}{}H{}{east_to_south}V{}{south_to_east}",
                        num(x),
                        num(y),
                        east_to_north,
                        num(y - b.reach + arc),
                        north_to_east,
                        num(right - 2.0 * arc),
                        num(y - arc)
                    ));
                    stack.push((kid, kx, y));
                }
                Kind::Loop => {
                    let kid = &kids[0];
                    let kx = x + (b.width - kid.width) / 2.0;
                    let loop_y = y + b.reach;
                    let middle = x + b.width / 2.0;
                    add_line(&mut track, x, y, kx);
                    add_line(&mut track, kx + kid.width, y, right);
                    track.push_str(&format!(
                        "M{},{}{}V{}{}H{}{east_to_north}V{}{loop_up}",
                        num(x + arc),
                        num(y),
                        loop_down,
                        num(loop_y - arc),
                        south_to_east,
                        num(right - arc),
                        num(y + arc)
                    ));
                    arrows.push_str(&format!(
                        "M{},{}l{},{}v{}z",
                        num(middle - ARROW / 2.0),
                        num(loop_y),
                        js_number(ARROW),
                        js_number(-ARROW / 2.0),
                        js_number(ARROW)
                    ));
                    if !b.text.is_empty() {
                        nodes.push_str(&format!(
                            "<text class=\"pele-edge-label pele-count\" x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"var(--_m)\">{}</text>",
                            num(middle),
                            num(loop_y + COUNT_SIZE + 3.0),
                            js_number(COUNT_SIZE),
                            esc(&b.text)
                        ));
                    }
                    stack.push((kid, kx, y));
                }
                Kind::Terminal | Kind::NonTerminal | Kind::Special => {
                    let text = &b.text;
                    let space = if text.starts_with(' ') || text.ends_with(' ') || text.contains("  ") {
                        " xml:space=\"preserve\""
                    } else {
                        ""
                    };
                    let shape = format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"", num(x), num(y - box_top), num(b.width), num(m.height));
                    let lbl = format!("<text x=\"{}\" y=\"{}\"{}", num(x + b.width / 2.0), num(y + text_shift), space);
                    if b.kind == Kind::Terminal {
                        nodes.push_str(&format!(
                            "<g class=\"pele-node pele-terminal\">{} rx=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>{} font-family=\"{}\">{}</text></g>",
                            shape,
                            num(box_top),
                            stroke,
                            lbl,
                            FONT_MONO,
                            esc(text)
                        ));
                    } else if b.kind == Kind::NonTerminal {
                        nodes.push_str(&format!(
                            "<g class=\"pele-node pele-nonterminal\" data-id=\"{}\">{} fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}{}/>{}>{}</text></g>",
                            esc_text(&b.id),
                            shape,
                            stroke,
                            ROUNDED,
                            lbl,
                            esc(text)
                        ));
                    } else {
                        let inner_txt = &text[2..text.len() - 2];
                        nodes.push_str(&format!(
                            "<g class=\"pele-node pele-special\">{} fill=\"var(--_bg)\" stroke=\"var(--_b)\" stroke-dasharray=\"4 3\"{}{}/>{} font-style=\"italic\"><tspan fill=\"var(--_m)\">? </tspan>{}<tspan fill=\"var(--_m)\"> ?</tspan></text></g>",
                            shape,
                            stroke,
                            ROUNDED,
                            lbl,
                            esc(inner_txt)
                        ));
                    }
                }
            }
        }

        let markers = if radius > 0.0 {
            format!(
                "<circle class=\"pele-marker pele-start\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"var(--_l)\"/><circle class=\"pele-marker pele-end\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"var(--_l)\"/>",
                num(pad + radius),
                num(baseline),
                num(radius),
                num(end + radius),
                num(baseline),
                num(radius)
            )
        } else {
            String::new()
        };

        rules_svg.push_str(&format!(
            "<g class=\"pele-rule\" data-id=\"{}\"><text class=\"pele-rule-name\" x=\"{}\" y=\"{}\" font-size=\"{}\" font-weight=\"var(--_hw)\" text-anchor=\"start\">{}</text><path class=\"pele-edge pele-track\" d=\"{}\" fill=\"none\" stroke=\"var(--_l)\"{}/>{}{}{}</g>",
            esc_text(&rule.name),
            num(pad),
            num(top + head_height / 2.0 + head_size * 0.35),
            num(head_size),
            esc(&name),
            track,
            stroke,
            if !arrows.is_empty() {
                format!("<path class=\"pele-marker\" d=\"{}\" fill=\"var(--_l)\"/>", arrows)
            } else {
                String::new()
            },
            markers,
            nodes
        ));
        width = width.max(end + 2.0 * radius - pad).max(m.measurer.width(&name, head_size, style::BOLD));
        bottom = baseline + rbox.down;
        top = bottom + m.sep + RULE_GAP;
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (bottom + pad).ceil();
    let mut content = label_svg(&title, total_width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\"");
    if !rules_svg.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-rules\" transform=\"translate(0.5,0.5)\" font-size=\"{}\" text-anchor=\"middle\">{}</g>",
            num(size),
            rules_svg
        ));
    }

    let fixed = config.get("railroad").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "railroad",
        total_width,
        total_height,
        base,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive: options.responsive.unwrap_or(!fixed),
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: "",
        },
        &Accessible { acc_title: model.acc_title.as_deref(), acc_descr: model.acc_descr.as_deref() },
        &content,
    );
    Rendered { svg, width: total_width, height: total_height, links: Vec::new() }
}
