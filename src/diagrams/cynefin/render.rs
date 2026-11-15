use std::collections::HashMap;

use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::edges::{marker, marker_trim};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{series_color, ROUNDED};
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

use super::boundaries::{
    backward, cliff, curve_path, ellipse_path, forward, resolve_seed, wave, WaveOptions, FOLD,
    HORIZON,
};
use super::model::CynefinModel;
use super::parser::DomainName;

const QUADRANTS: [(DomainName, usize, usize, i64); 4] = [
    (DomainName::Complex, 0, 0, 0),
    (DomainName::Complicated, 1, 0, 1),
    (DomainName::Chaotic, 0, 1, 3),
    (DomainName::Clear, 1, 1, 2),
];
const CONFUSION_COLOR: i64 = 4;

fn meta(name: DomainName) -> (&'static str, &'static str, &'static str) {
    match name {
        DomainName::Complex => ("Complex", "Probe → Sense → Respond", "Emergent Practices"),
        DomainName::Complicated => ("Complicated", "Sense → Analyse → Respond", "Good Practices"),
        DomainName::Clear => ("Clear", "Sense → Categorise → Respond", "Best Practices"),
        DomainName::Chaotic => ("Chaotic", "Act → Sense → Respond", "Novel Practices"),
        DomainName::Confusion => ("Confusion", "", "Disorder"),
    }
}

const MAX_CONFUSION_ITEMS: usize = 3;
const ITEM_MAX_WIDTH: f64 = 200.0;
const ITEM_PAD_X: f64 = 10.0;
const ITEM_PAD_Y: f64 = 4.0;
const ITEM_GAP: f64 = 6.0;
const HEAD_GAP: f64 = 10.0;
const MARGIN: f64 = 20.0;
const CLEARANCE: f64 = 10.0;

#[derive(Clone)]
struct Badge {
    text: String,
    label: Label,
    w: f64,
    h: f64,
    overflow: bool,
}

#[derive(Clone)]
struct Block {
    name: DomainName,
    title: Label,
    subtitles: Vec<Label>,
    badges: Vec<Badge>,
    w: f64,
    h: f64,
    cx: f64,
    cy: f64,
}

fn setting(config: &Value, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match config.get("cynefin").get(key).as_f64() {
        Some(v) if v >= min && v <= max => v,
        _ => fallback,
    }
}

pub fn render_cynefin(model: &CynefinModel, config: &Value, options: &RenderOptions) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.8125);
    let item_size = js_round(size * 0.875);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let cynefin_cfg = config.get("cynefin");
    let show_descriptions = cynefin_cfg.get("showDomainDescriptions").as_bool() != Some(false);
    let amplitude = setting(config, "boundaryAmplitude", 8.0, 0.0, 50.0);
    let id_prefix = options.id_prefix.as_deref().unwrap_or("pele");
    let seed = resolve_seed(Some(setting(config, "seed", 0.0, -1e15, 1e15)), id_prefix);
    let item_line = js_round(item_size * 1.5);

    let fixed_width = cynefin_cfg.get("useMaxWidth").as_bool() == Some(false);
    let responsive = options.responsive.unwrap_or(!fixed_width);

    let make_badge = |text: String, overflow: bool| -> Badge {
        let label = layout_label(Some(&text), false, measurer, item_size, ITEM_MAX_WIDTH, 0);
        let w = label.width + 2.0 * ITEM_PAD_X;
        let h = label.height.max(item_line) + 2.0 * ITEM_PAD_Y;
        Badge { text, label, w, h, overflow }
    };

    let make_block = |name: DomainName| -> Block {
        let (m0, m1, m2) = meta(name);
        let title = layout_label(Some(m0), false, measurer, size, 4000.0, style::BOLD);
        let mut subtitles = Vec::new();
        if show_descriptions {
            for s_txt in [m1, m2] {
                if !s_txt.is_empty() {
                    subtitles.push(layout_label(Some(s_txt), false, measurer, small, 4000.0, style::ITALIC));
                }
            }
        }
        let empty_items = Vec::new();
        let items = model.domains.get(&name).map_or(&empty_items, |d| &d.items);
        let shown = if name == DomainName::Confusion && items.len() > MAX_CONFUSION_ITEMS {
            MAX_CONFUSION_ITEMS
        } else {
            items.len()
        };
        let mut badges = Vec::new();
        for item in items.iter().take(shown) {
            badges.push(make_badge(item.label.clone(), false));
        }
        if shown < items.len() {
            badges.push(make_badge(format!("+{} more", items.len() - shown), true));
        }
        let mut w = title.width;
        let mut h = title.height;
        for s in &subtitles {
            w = w.max(s.width);
            h += s.height;
        }
        if !badges.is_empty() {
            h += HEAD_GAP - ITEM_GAP;
        }
        for b in &badges {
            w = w.max(b.w);
            h += ITEM_GAP + b.h;
        }
        Block { name, title, subtitles, badges, w, h, cx: 0.0, cy: 0.0 }
    };

    let mut quadrants: Vec<Block> = QUADRANTS.iter().map(|&(name, _, _, _)| make_block(name)).collect();
    let mut confusion = make_block(DomainName::Confusion);
    let rx = 88.0_f64.max(confusion.w * 0.71 + 8.0);
    let ry = 64.0_f64.max(confusion.h * 0.71 + 6.0);

    let mut quad_w = setting(config, "width", 640.0, 100.0, 10000.0) / 2.0;
    let mut quad_h = setting(config, "height", 440.0, 100.0, 10000.0) / 2.0;
    for b in &quadrants {
        quad_w = quad_w.max(b.w + 2.0 * MARGIN);
        quad_h = quad_h.max(b.h + 2.0 * MARGIN);
    }
    for b in &quadrants {
        let dx = (quad_w - b.w) / 2.0;
        let dy = (quad_h - b.h) / 2.0;
        if (dx / (rx + CLEARANCE)).powi(2) + (dy / (ry + CLEARANCE)).powi(2) >= 1.0 {
            continue;
        }
        let wider = b.w + 2.0 * (rx + CLEARANCE);
        let taller = b.h + 2.0 * (ry + CLEARANCE);
        if wider * quad_h <= quad_w * taller {
            quad_w = quad_w.max(wider);
        } else {
            quad_h = quad_h.max(taller);
        }
    }

    let title = layout_label(model.title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let width = 2.0 * quad_w;
    let height = 2.0 * quad_h;
    let x0 = pad;
    let y0 = pad + title_height;
    let cx = x0 + quad_w;
    let cy = y0 + quad_h;
    let right = x0 + width;
    let bottom = y0 + height;

    let fold = wave(y0, cy, cx, seed, amplitude, WaveOptions { segments: 4, pin_end: true, ..FOLD });
    let west = wave(x0, cx, cy, seed + 100.0, amplitude, WaveOptions { segments: 4, pin_end: true, ..HORIZON });
    let east = wave(cx, right, cy, seed + 200.0, amplitude, WaveOptions { segments: 4, pin_start: true, ..HORIZON });
    let drop = cliff(cx, cy, bottom, width * 0.03);
    let at = |curve: &[f64], end: bool| -> String {
        if end {
            format!("{},{}", num(curve[curve.len() - 2]), num(curve[curve.len() - 1]))
        } else {
            format!("{},{}", num(curve[0]), num(curve[1]))
        }
    };

    let region_path = |name: DomainName| -> String {
        match name {
            DomainName::Complex => format!("M{},{}L{}{}{}Z", num(x0), num(y0), at(&fold, false), forward(&fold), backward(&west)),
            DomainName::Complicated => format!("M{}L{},{}L{}{}{}Z", at(&fold, false), num(right), num(y0), at(&east, true), backward(&east), backward(&fold)),
            DomainName::Chaotic => format!("M{}{}{}L{},{}Z", at(&west, false), forward(&west), forward(&drop), num(x0), num(bottom)),
            DomainName::Clear => format!("M{}{}L{},{}L{}{}Z", at(&east, false), forward(&east), num(right), num(bottom), at(&drop, true), backward(&drop)),
            DomainName::Confusion => String::new(),
        }
    };

    let mut fills = String::new();
    for &(name, _, _, color) in &QUADRANTS {
        fills.push_str(&format!(
            "<path class=\"pele-cynefin-region\" data-id=\"{}\" d=\"{}\" fill=\"{}\" fill-opacity=\"0.14\"/>",
            name.as_str(),
            region_path(name),
            series_color(color)
        ));
    }
    let boundaries = format!(
        "<g class=\"pele-cynefin-boundaries\" fill=\"none\" stroke=\"var(--_b)\" stroke-dasharray=\"6 3\"><path d=\"{}\"/><path d=\"{}\"/><path d=\"{}\"/><path class=\"pele-cynefin-cliff\" d=\"{}\" stroke=\"var(--_l)\" stroke-width=\"2\" stroke-dasharray=\"none\" stroke-linecap=\"round\"/></g>",
        curve_path(&fold),
        curve_path(&west),
        curve_path(&east),
        curve_path(&drop)
    );
    let oval = ellipse_path(cx, cy, rx, ry);
    let ellipse = format!(
        "<path d=\"{}\" fill=\"var(--_bg)\"/><path class=\"pele-cynefin-region\" data-id=\"confusion\" d=\"{}\" fill=\"{}\" fill-opacity=\"0.18\" stroke=\"var(--_b)\" stroke-dasharray=\"4 2\"/>",
        oval,
        oval,
        series_color(CONFUSION_COLOR)
    );

    let small_str = js_number(small);
    let item_size_str = js_number(item_size);
    let draw_block = |b: &Block| -> String {
        let mut y = b.cy - b.h / 2.0;
        let mut out = label_svg(&b.title, b.cx, y + b.title.height / 2.0, " class=\"pele-label\" font-weight=\"var(--_hw)\"");
        y += b.title.height;
        for s in &b.subtitles {
            out.push_str(&label_svg(
                s,
                b.cx,
                y + s.height / 2.0,
                &format!(" class=\"pele-cynefin-subtitle\" font-size=\"{}\" font-style=\"italic\" fill=\"var(--_m)\"", small_str),
            ));
            y += s.height;
        }
        y += HEAD_GAP - ITEM_GAP;
        for item in &b.badges {
            y += ITEM_GAP;
            out.push_str(&format!(
                "<g class=\"pele-node{}\"{}><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}{}/>{}</g>",
                if item.overflow { " pele-cynefin-overflow" } else { "" },
                if item.overflow { String::new() } else { format!(" data-id=\"{}\"", esc_text(&item.text)) },
                num(b.cx - item.w / 2.0),
                num(y),
                num(item.w),
                num(item.h),
                if item.overflow { " stroke-dasharray=\"3 2\"" } else { "" },
                ROUNDED,
                label_svg(
                    &item.label,
                    b.cx,
                    y + item.h / 2.0,
                    &format!(" class=\"pele-label\" font-size=\"{}\"{}", item_size_str, if item.overflow { " fill=\"var(--_m)\"" } else { "" })
                )
            ));
            y += item.h;
        }
        format!("<g class=\"pele-cynefin-domain\" data-id=\"{}\">{}</g>", b.name.as_str(), out)
    };

    let mut blocks: HashMap<DomainName, Block> = HashMap::new();
    let mut domains_svg = String::new();
    for (index, &(name, col, row, _)) in QUADRANTS.iter().enumerate() {
        let b = &mut quadrants[index];
        b.cx = x0 + quad_w * (col as f64 + 0.5);
        b.cy = y0 + quad_h * (row as f64 + 0.5);
        domains_svg.push_str(&draw_block(b));
        blocks.insert(name, b.clone());
    }
    confusion.cx = cx;
    confusion.cy = cy;
    domains_svg.push_str(&draw_block(&confusion));
    blocks.insert(DomainName::Confusion, confusion);

    let inside = |b: &Block, x: f64, y: f64| -> bool {
        if b.name == DomainName::Confusion {
            ((x - cx) / (rx + 4.0)).powi(2) + ((y - cy) / (ry + 4.0)).powi(2) <= 1.0
        } else {
            (x - b.cx).abs() <= b.w / 2.0 + 8.0 && (y - b.cy).abs() <= b.h / 2.0 + 8.0
        }
    };

    let mut edges = String::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for transition in &model.transitions {
        let Some(from) = blocks.get(&transition.from) else { continue };
        let Some(to) = blocks.get(&transition.to) else { continue };
        if from.name == to.name {
            continue;
        }
        let f_str = transition.from.as_str();
        let t_str = transition.to.as_str();
        let key = if f_str < t_str {
            format!("{} {}", f_str, t_str)
        } else {
            format!("{} {}", t_str, f_str)
        };
        let repeat = seen.get(&key).copied().unwrap_or(0);
        seen.insert(key, repeat + 1);

        let dx = to.cx - from.cx;
        let dy = to.cy - from.cy;
        let len = {
            let h = dx.hypot(dy);
            if h == 0.0 { 1.0 } else { h }
        };
        let mx = (from.cx + to.cx) / 2.0;
        let my = (from.cy + to.cy) / 2.0;
        let outward = if -dy * (mx - cx) + dx * (my - cy) >= 0.0 { 1.0 } else { -1.0 };
        let nx = (-dy / len) * outward;
        let ny = (dx / len) * outward;
        let diagonal = from.name != DomainName::Confusion
            && to.name != DomainName::Confusion
            && dx.abs() > 1.0
            && dy.abs() > 1.0;
        let bow = (if diagonal {
            2.0 * ((rx * nx).hypot(ry * ny) + 16.0)
        } else {
            len * 0.15
        }) + repeat as f64 * 56.0;
        let px = mx + nx * bow;
        let py = my + ny * bow;
        let point = |t: f64| -> (f64, f64) {
            let u = 1.0 - t;
            (
                u * u * from.cx + 2.0 * u * t * px + t * t * to.cx,
                u * u * from.cy + 2.0 * u * t * py + t * t * to.cy,
            )
        };
        let crossing = |b: &Block, mut lo: f64, mut hi: f64, leaving: bool| -> f64 {
            for _ in 0..16 {
                let mid = (lo + hi) / 2.0;
                let (ptx, pty) = point(mid);
                if inside(b, ptx, pty) == leaving {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) / 2.0
        };
        const STEPS: usize = 48;
        let mut step_i = 1usize;
        while step_i <= STEPS {
            let (ptx, pty) = point(step_i as f64 / STEPS as f64);
            if !inside(from, ptx, pty) {
                break;
            }
            step_i += 1;
        }
        if step_i > STEPS {
            continue;
        }
        let t0 = crossing(from, (step_i - 1) as f64 / STEPS as f64, step_i as f64 / STEPS as f64, true);
        while step_i <= STEPS {
            let (ptx, pty) = point(step_i as f64 / STEPS as f64);
            if inside(to, ptx, pty) {
                break;
            }
            step_i += 1;
        }
        let t1 = if step_i > STEPS {
            1.0
        } else {
            crossing(to, (step_i - 1) as f64 / STEPS as f64, step_i as f64 / STEPS as f64, false)
        };

        let (sx, sy) = point(t0);
        let (mut ex, mut ey) = point(t1);
        let blossom = |a: f64, b: f64, c: f64| -> f64 {
            (1.0 - t0) * (1.0 - t1) * a + ((1.0 - t0) * t1 + t0 * (1.0 - t1)) * b + t0 * t1 * c
        };
        let qx = blossom(from.cx, px, to.cx);
        let qy = blossom(from.cy, py, to.cy);
        let el = {
            let h = (ex - qx).hypot(ey - qy);
            if h == 0.0 { 1.0 } else { h }
        };
        let ux = (ex - qx) / el;
        let uy = (ey - qy) / el;
        let head = marker("arrow_point", ex, ey, ux, uy, "var(--_l)", "none");
        let trim = marker_trim("arrow_point");
        ex -= ux * trim;
        ey -= uy * trim;

        let mut text_svg = String::new();
        if let Some(ref tr_label) = transition.label {
            let label = layout_label(Some(tr_label), false, measurer, small, ITEM_MAX_WIDTH, 0);
            let (lx, ly) = point((t0 + t1) / 2.0);
            let away = (nx.abs() * label.width) / 2.0 + (ny.abs() * label.height) / 2.0 + 6.0;
            let half = label.width / 2.0 + 4.0;
            let tx = (lx + nx * away).max(x0 + half).min((right - half).max(x0 + half));
            text_svg = label_svg(
                &label,
                tx,
                ly + ny * away,
                &format!(" class=\"pele-edge-label\" font-size=\"{}\"", small_str),
            );
        }
        edges.push_str(&format!(
            "<g class=\"pele-edge\" data-id=\"{}-{}\"><path d=\"M{},{}Q{},{} {},{}\" fill=\"none\" stroke=\"var(--_l)\"/>{}{}</g>",
            f_str,
            t_str,
            num(sx),
            num(sy),
            num(qx),
            num(qy),
            num(ex),
            num(ey),
            head,
            text_svg
        ));
    }

    let total_width = (width.max(title.width) + 2.0 * pad).ceil();
    let total_height = (height + title_height + 2.0 * pad).ceil();
    let mut content = label_svg(
        &title,
        x0 + width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    );
    content.push_str(&format!("<g class=\"pele-cynefin-regions\">{}</g>", fills));
    content.push_str(&boundaries);
    content.push_str(&ellipse);
    content.push_str(&domains_svg);
    if !edges.is_empty() {
        content.push_str(&format!("<g class=\"pele-edges\">{}</g>", edges));
    }

    let svg = svg_document(
        "cynefin",
        total_width,
        total_height,
        size,
        &Document {
            id_prefix,
            responsive,
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: "",
        },
        &Accessible { acc_title: model.acc_title.as_deref(), acc_descr: model.acc_descr.as_deref() },
        &content,
    );

    Rendered {
        svg,
        width: total_width,
        height: total_height,
        links: Vec::new(),
    }
}
