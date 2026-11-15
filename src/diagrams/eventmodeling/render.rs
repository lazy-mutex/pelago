use std::collections::{HashMap, HashSet};

use crate::diagrams::common::fit_width::turn_to_fit;
use crate::diagrams::common::literal_label::{break_label, left_label_svg, literal_label};
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::edges::{marker, marker_trim};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{series_color, FONT_MONO, ROUNDED};
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

use super::model::EventModelingModel;

const BOX_PAD_X: f64 = 12.0;
const BOX_PAD_Y: f64 = 10.0;
const BOX_MIN_WIDTH: f64 = 96.0;
const BOX_GAP: f64 = 16.0;
const STAGGER: f64 = 80.0;
const LANE_PAD: f64 = 12.0;
const LANE_GAP: f64 = 8.0;
const DATA_COLUMNS: usize = 48;
const NOTE_COLUMNS: usize = 80;
const CARD_GAP: f64 = 8.0;

struct Placed {
    box_idx: usize,
    name: Label,
    data: Vec<String>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Band {
    lane_idx: usize,
    label: Label,
    right: f64,
    y: f64,
    h: f64,
}

fn hard_wrap(text: &str, columns: usize) -> Vec<String> {
    let mut out = Vec::new();
    let decoded = decode_entities(text);
    for line in decoded.split('\n') {
        let chars: Vec<char> = line.chars().collect();
        if chars.len() <= columns {
            out.push(line.to_string());
            continue;
        }
        let mut i = 0usize;
        while i < chars.len() {
            let end = (i + columns).min(chars.len());
            out.push(chars[i..end].iter().collect());
            i = end;
        }
    }
    out
}

fn lines_svg(rows: &[String], x: f64, y: f64, step: f64, attrs: &str) -> String {
    let mut body = String::new();
    for (i, row) in rows.iter().enumerate() {
        if !row.is_empty() {
            body.push_str(&format!(
                "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                num(x),
                num(y + i as f64 * step),
                esc(row)
            ));
        }
    }
    if body.is_empty() {
        String::new()
    } else {
        format!("<text{} xml:space=\"preserve\">{}</text>", attrs, body)
    }
}

pub fn render_event_model(
    model: &EventModelingModel,
    config: &Value,
    options: &RenderOptions,
) -> Rendered {
    let fixed = config.get("eventmodeling").get("useMaxWidth").as_bool() == Some(false);
    let responsive = options.responsive.unwrap_or(!fixed);
    turn_to_fit(options, &|down| draw(model, options, responsive, down))
}

fn draw(
    model: &EventModelingModel,
    options: &RenderOptions,
    responsive: bool,
    down: bool,
) -> Rendered {
    if down {
        return vertical_event_model(model, options, responsive);
    }

    let size = options.font_size.unwrap_or(16.0);
    let name_size = js_round(size * 0.875);
    let small = js_round(size * 0.8125);
    let mono = js_round(size * 0.75);
    let mono_step = js_round(mono * 1.4);
    let small_step = js_round(small * 1.5);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    let widest = |rows: &[String], font_size: f64, st: u8| -> f64 {
        rows.iter()
            .fold(0.0f64, |acc, row| acc.max(measurer.width(row, font_size, st)))
    };

    let mut bands: Vec<Band> = Vec::with_capacity(model.lanes.len());
    let mut label_column = 0.0f64;
    for (lane_idx, lane) in model.lanes.iter().enumerate() {
        let label = layout_label(Some(&lane.label), false, measurer, small, 4000.0, style::BOLD);
        label_column = label_column.max(label.width);
        bands.push(Band {
            lane_idx,
            label,
            right: f64::NEG_INFINITY,
            y: 0.0,
            h: 0.0,
        });
    }
    label_column += 2.0 * LANE_PAD;

    let mut placed: Vec<Placed> = Vec::with_capacity(model.boxes.len());
    for (box_idx, b) in model.boxes.iter().enumerate() {
        let name = layout_label(Some(&b.name), false, measurer, name_size, 4000.0, style::BOLD);
        let data = match &b.data {
            Some(d) if !d.is_empty() => hard_wrap(d, DATA_COLUMNS),
            _ => Vec::new(),
        };
        let w = BOX_MIN_WIDTH.max(name.width.max(widest(&data, mono, style::MONO)) + 2.0 * BOX_PAD_X);
        let h = 2.0 * BOX_PAD_Y + name.height + if !data.is_empty() { 6.0 + data.len() as f64 * mono_step } else { 0.0 };
        let x = if let Some(prev) = placed.last() {
            let stagger = prev.x + (prev.w / 2.0).max(prev.w - STAGGER);
            let prev_lane = model.boxes[prev.box_idx].lane_idx;
            let band_right = bands[b.lane_idx].right;
            if prev_lane == b.lane_idx {
                band_right + BOX_GAP
            } else {
                stagger.max(band_right + BOX_GAP)
            }
        } else {
            0.0
        };
        bands[b.lane_idx].right = x + w;
        bands[b.lane_idx].h = bands[b.lane_idx].h.max(h);
        placed.push(Placed {
            box_idx,
            name,
            data,
            x,
            y: 0.0,
            w,
            h,
        });
    }

    let content_right = placed.iter().fold(0.0f64, |acc, item| acc.max(item.x + item.w));
    let lane_width = label_column + content_right + LANE_PAD;

    let title = layout_label(model.title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let left = pad;
    let top = pad + title_height;

    let small_str = js_number(small);
    let name_size_str = js_number(name_size);
    let mono_str = js_number(mono);

    let mut y = top;
    let mut lanes_svg = String::new();
    for band in &mut bands {
        band.y = y;
        band.h += 2.0 * LANE_PAD;
        let lane_label = &model.lanes[band.lane_idx].label;
        lanes_svg.push_str(&format!(
            "<g class=\"pele-cluster pele-em-lane\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\"{}/>{}</g>",
            esc_text(lane_label),
            num(left),
            num(y),
            num(lane_width),
            num(band.h),
            ROUNDED,
            label_svg(
                &band.label,
                left + LANE_PAD + band.label.width / 2.0,
                y + band.h / 2.0,
                &format!(
                    " class=\"pele-cluster-label\" font-size=\"{}\" font-weight=\"var(--_hw)\" fill=\"var(--_m)\"",
                    small_str
                )
            )
        ));
        y += band.h + LANE_GAP;
    }
    let mut bottom = if !bands.is_empty() { y - LANE_GAP } else { top };

    let mut nodes_svg = String::new();
    for item in &mut placed {
        let b = &model.boxes[item.box_idx];
        let band = &bands[b.lane_idx];
        item.x += left + label_column;
        item.y = band.y + (band.h - item.h) / 2.0;
        let frame = format!(
            "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
            num(item.x),
            num(item.y),
            num(item.w),
            num(item.h)
        );
        let shape = match b.kind.series_slot() {
            None => format!("<rect {} fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}/>", frame, ROUNDED),
            Some(slot) => {
                let col = series_color(slot);
                format!(
                    "<rect {} fill=\"var(--_bg)\"{}/><rect {} fill=\"{}\" fill-opacity=\"0.22\" stroke=\"{}\"{}/>",
                    frame, ROUNDED, frame, col, col, ROUNDED
                )
            }
        };
        let name_y = item.y + BOX_PAD_Y + item.name.height / 2.0;
        let text_svg = format!(
            "{}{}",
            label_svg(
                &item.name,
                item.x + item.w / 2.0,
                name_y,
                &format!(" class=\"pele-label\" font-size=\"{}\" font-weight=\"var(--_hw)\"", name_size_str)
            ),
            lines_svg(
                &item.data,
                item.x + BOX_PAD_X,
                item.y + BOX_PAD_Y + item.name.height + 6.0 + mono_step / 2.0 + mono * 0.35,
                mono_step,
                &format!(" class=\"pele-em-data\" font-family=\"{}\" font-size=\"{}\"", FONT_MONO, mono_str)
            )
        );
        nodes_svg.push_str(&format!(
            "<g class=\"pele-node pele-em-{}{}\" data-id=\"{}\">{}{}</g>",
            b.kind.as_str(),
            if b.reset { " pele-em-reset" } else { "" },
            esc_text(&b.id),
            shape,
            text_svg
        ));
    }

    let mut edges_svg = String::new();
    let trim = marker_trim("arrow_point");
    for rel in &model.relations {
        if rel.source_idx == rel.target_idx {
            continue;
        }
        let s = &placed[rel.source_idx];
        let t = &placed[rel.target_idx];
        let s_box = &model.boxes[rel.source_idx];
        let t_box = &model.boxes[rel.target_idx];
        let (d, head) = if s_box.lane_idx == t_box.lane_idx {
            let forward = t.x >= s.x;
            let sx = if forward { s.x + s.w } else { s.x };
            let tx = if forward { t.x } else { t.x + t.w };
            let sy = s.y + s.h / 2.0;
            let ty = t.y + t.h / 2.0;
            let dir = if tx >= sx { 1.0 } else { -1.0 };
            (
                format!("M{},{}L{},{}", num(sx), num(sy), num(tx - dir * trim), num(ty)),
                marker("arrow_point", tx, ty, dir, 0.0, "var(--_l)", "none"),
            )
        } else {
            let going_down = t.y > s.y;
            let from = s.x.max(t.x);
            let to = (s.x + s.w).min(t.x + t.w);
            let inset = |w: f64| 24.0_f64.min(w / 4.0);
            let sx = if to - from >= 16.0 {
                (from + to) / 2.0
            } else if t.x > s.x {
                s.x + s.w - inset(s.w)
            } else {
                s.x + inset(s.w)
            };
            let tx = if to - from >= 16.0 {
                sx
            } else if t.x > s.x {
                t.x + inset(t.w)
            } else {
                t.x + t.w - inset(t.w)
            };
            let sy = if going_down { s.y + s.h } else { s.y };
            let ty = if going_down { t.y } else { t.y + t.h };
            let dir = if going_down { 1.0 } else { -1.0 };
            let end = ty - dir * trim;
            let mid = (sy + end) / 2.0;
            let path = if sx == tx {
                format!("M{},{}V{}", num(sx), num(sy), num(end))
            } else {
                format!(
                    "M{},{}C{},{} {},{} {},{}",
                    num(sx),
                    num(sy),
                    num(sx),
                    num(mid),
                    num(tx),
                    num(mid),
                    num(tx),
                    num(end)
                )
            };
            (path, marker("arrow_point", tx, ty, 0.0, dir, "var(--_l)", "none"))
        };
        edges_svg.push_str(&format!(
            "<g class=\"pele-edge\" data-id=\"{}-{}\"><path d=\"{}\" fill=\"none\" stroke=\"var(--_l)\"/>{}</g>",
            esc_text(&s_box.id),
            esc_text(&t_box.id),
            d,
            head
        ));
    }

    let mut names: HashMap<&str, &str> = HashMap::new();
    for b in &model.boxes {
        names.insert(b.id.as_str(), b.name.as_str());
    }
    let mut cards_svg = String::new();
    let mut cards_right = 0.0f64;
    let mut add_card = |kind: &str, heading: &str, frame: &str, rows: &[String]| {
        let suffix = names.get(frame).map_or(String::new(), |n| format!(" {}", n));
        let head = layout_label(
            Some(&format!("{} · {}{}", heading, frame, suffix)),
            false,
            measurer,
            small,
            4000.0,
            style::BOLD,
        );
        let w = head.width.max(widest(rows, small, 0)) + 2.0 * BOX_PAD_X;
        let h = 2.0 * BOX_PAD_Y + head.height + rows.len() as f64 * small_step;
        let cy = bottom + if cards_svg.is_empty() && bands.is_empty() { 0.0 } else { CARD_GAP };
        cards_svg.push_str(&format!(
            "<g class=\"pele-em-{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}/>{}{}</g>",
            kind,
            esc_text(frame),
            num(left),
            num(cy),
            num(w),
            num(h),
            ROUNDED,
            label_svg(
                &head,
                left + BOX_PAD_X + head.width / 2.0,
                cy + BOX_PAD_Y + head.height / 2.0,
                &format!(" font-size=\"{}\" font-weight=\"var(--_hw)\"", small_str)
            ),
            lines_svg(
                rows,
                left + BOX_PAD_X,
                cy + BOX_PAD_Y + head.height + small_step / 2.0 + small * 0.35,
                small_step,
                &format!(" font-size=\"{}\"", small_str)
            )
        ));
        bottom = cy + h;
        cards_right = cards_right.max(w);
    };

    for note in &model.notes {
        add_card("note", "Note", &note.frame, &hard_wrap(&note.text, NOTE_COLUMNS));
    }
    for spec in &model.specifications {
        let mut raw_rows = vec![format!("Given  {}", spec.given.join(", "))];
        if !spec.when.is_empty() {
            raw_rows.push(format!("When  {}", spec.when.join(", ")));
        }
        raw_rows.push(format!("Then  {}", spec.then.join(", ")));
        let wrapped: Vec<String> = raw_rows.iter().flat_map(|row| hard_wrap(row, NOTE_COLUMNS)).collect();
        add_card("specification", "Given / When / Then", &spec.frame, &wrapped);
    }

    let width = (if !bands.is_empty() { lane_width } else { 0.0 })
        .max(cards_right)
        .max(title.width);
    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (bottom + pad).ceil();
    let content = format!(
        "{}{}{}{}{}",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        lanes_svg,
        edges_svg,
        nodes_svg,
        cards_svg
    );

    let svg = svg_document(
        "eventmodeling",
        total_width,
        total_height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
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

const RAIL_GAP: f64 = 8.0;
const CARD_MIN: f64 = 120.0;
const ARROW_ROOM: f64 = 20.0;

struct VPlaced {
    y: f64,
    h: f64,
    w: f64,
    index: usize,
    lane: f64,
}

fn vertical_event_model(
    model: &EventModelingModel,
    options: &RenderOptions,
    responsive: bool,
) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let mono = js_round(size * 0.75);
    let pad = options.padding.unwrap_or(8.0);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let room = 160.0_f64.max(options.max_width.unwrap_or(320.0) - 2.0 * pad);

    let mut runs: HashMap<usize, (usize, usize)> = HashMap::new();
    let mut runs_order: Vec<usize> = Vec::new();
    let mut passed: HashSet<usize> = HashSet::new();
    for rel in &model.relations {
        let s = rel.source_idx;
        let t = rel.target_idx;
        if s == t {
            continue;
        }
        if s.abs_diff(t) < 2 {
            passed.insert(s.max(t));
            continue;
        }
        if let Some(run) = runs.get_mut(&t) {
            run.0 = run.0.min(s);
            run.1 = run.1.max(s);
        } else {
            runs.insert(t, (s.min(t), s.max(t)));
            runs_order.push(t);
        }
    }

    runs_order.sort_by_key(|t| runs[t].0);
    let mut rail_of: HashMap<usize, usize> = HashMap::new();
    let mut rail_ends: Vec<usize> = Vec::new();
    for target in runs_order {
        let (from, to) = runs[&target];
        let rail = match rail_ends.iter().position(|&end| end < from) {
            Some(idx) => {
                rail_ends[idx] = to;
                idx
            }
            None => {
                let idx = rail_ends.len();
                rail_ends.push(to);
                idx
            }
        };
        rail_of.insert(target, rail);
    }

    let gutter = 24.0 + RAIL_GAP * (rail_ends.len().saturating_sub(1) as f64);
    let card_width = CARD_MIN.max(room - gutter);
    let text_room = card_width - 2.0 * BOX_PAD_X;
    let title = layout_label(model.title.as_deref(), false, measurer, size, room, style::BOLD);
    let mut y = pad + if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let mut bottom = pad + title.height;
    let mut width = title.width.max(card_width + gutter);
    let mut nodes_svg = String::new();
    let mut edges_svg = String::new();
    let mut cards_svg = String::new();
    let mut placed: Vec<VPlaced> = Vec::with_capacity(model.boxes.len());

    let small_str = js_number(small);
    let mono_str = js_number(mono);
    let make_text = |raw: &str, font_size: f64, st: u8| -> Label {
        literal_label(&decode_entities(raw), measurer, font_size, text_room, st)
    };

    for (index, b) in model.boxes.iter().enumerate() {
        let lane_room = if passed.contains(&index) { text_room - ARROW_ROOM } else { text_room };
        let lane_lbl_str = &model.lanes[b.lane_idx].label;
        let lane = break_label(
            &layout_label(Some(lane_lbl_str), false, measurer, small, lane_room, style::BOLD),
            measurer,
            lane_room,
        );
        let name = layout_label(Some(&b.name), false, measurer, small, text_room - 36.0, style::BOLD);
        let data = b
            .data
            .as_deref()
            .filter(|d| !d.is_empty())
            .map(|d| make_text(d, mono, style::MONO));
        let data_w = data.as_ref().map_or(0.0, |d| d.width);
        let w = card_width
            .max(lane.width + 2.0 * BOX_PAD_X)
            .max(name.width + 2.0 * BOX_PAD_X + 36.0)
            .max(data_w + 2.0 * BOX_PAD_X);
        let head_h = lane.height + 12.0;
        let h = 2.0 * BOX_PAD_Y + name.height + data.as_ref().map_or(0.0, |d| d.height + 6.0);
        let cy = y + head_h;
        let frame = format!("x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"", num(pad), num(cy), num(w), num(h));
        let series_rect = match b.kind.series_slot() {
            None => String::new(),
            Some(slot) => {
                let col = series_color(slot);
                format!("<rect {} fill=\"{}\" fill-opacity=\"0.22\" stroke=\"{}\"{}/>", frame, col, col, ROUNDED)
            }
        };
        let data_svg = match &data {
            None => String::new(),
            Some(d) => {
                let row_strings: Vec<String> = d
                    .lines
                    .iter()
                    .map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>())
                    .collect();
                lines_svg(
                    &row_strings,
                    pad + BOX_PAD_X,
                    cy + BOX_PAD_Y + name.height + 6.0 + d.line_height / 2.0 + mono * 0.35,
                    d.line_height,
                    &format!(" class=\"pele-em-data\" font-family=\"{}\" font-size=\"{}\"", FONT_MONO, mono_str),
                )
            }
        };
        nodes_svg.push_str(&format!(
            "<g class=\"pele-cluster pele-em-lane\" data-id=\"{}\">{}<g class=\"pele-node pele-em-{}{}\" data-id=\"{}\"><rect {} fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}/>{}{}<text class=\"pele-em-frame\" x=\"{}\" y=\"{}\" text-anchor=\"end\" font-size=\"{}\" fill=\"var(--_m)\">{}</text>{}</g></g>",
            esc_text(lane_lbl_str),
            left_label_svg(
                &lane,
                pad + BOX_PAD_X,
                y,
                &format!(" class=\"pele-cluster-label\" font-size=\"{}\" font-weight=\"var(--_hw)\" fill=\"var(--_m)\"", small_str)
            ),
            b.kind.as_str(),
            if b.reset { " pele-em-reset" } else { "" },
            esc_text(&b.id),
            frame,
            ROUNDED,
            series_rect,
            label_svg(
                &name,
                pad + BOX_PAD_X + name.width / 2.0,
                cy + BOX_PAD_Y + name.height / 2.0,
                &format!(" class=\"pele-label\" font-size=\"{}\" font-weight=\"var(--_hw)\"", small_str)
            ),
            num(pad + w - BOX_PAD_X),
            num(cy + BOX_PAD_Y + small),
            mono_str,
            esc(&b.id),
            data_svg
        ));
        placed.push(VPlaced { y: cy, h, w, index, lane: lane.width });
        width = width.max(w + gutter);
        bottom = cy + h;
        y = bottom + 28.0;
    }

    let trim = marker_trim("arrow_point");
    for rel in &model.relations {
        if rel.source_idx == rel.target_idx {
            continue;
        }
        let s = &placed[rel.source_idx];
        let t = &placed[rel.target_idx];
        let going_down = t.y > s.y;
        let dir = if going_down { 1.0 } else { -1.0 };
        let sy = if going_down { s.y + s.h } else { s.y };
        let ty = if going_down { t.y } else { t.y + t.h };
        let (d, head) = if s.index.abs_diff(t.index) == 1 {
            let lower_lane = if going_down { t.lane } else { s.lane };
            let cx = pad + (s.w.min(t.w) / 2.0).max(BOX_PAD_X + lower_lane + 16.0);
            (
                format!("M{},{}V{}", num(cx), num(sy), num(ty - dir * trim)),
                marker("arrow_point", cx, ty, 0.0, dir, "var(--_l)", "none"),
            )
        } else {
            let rail = pad + width - gutter + 12.0 + RAIL_GAP * (rail_of.get(&rel.target_idx).copied().unwrap_or(0) as f64);
            let from = s.y + s.h / 2.0 - 5.0;
            let to = t.y + t.h / 2.0 + 5.0;
            (
                format!("M{},{}H{}V{}H{}", num(pad + s.w), num(from), num(rail), num(to), num(pad + t.w + trim)),
                marker("arrow_point", pad + t.w, to, -1.0, 0.0, "var(--_l)", "none"),
            )
        };
        edges_svg.push_str(&format!(
            "<g class=\"pele-edge\" data-id=\"{}-{}\"><path d=\"{}\" fill=\"none\" stroke=\"var(--_l)\"/>{}</g>",
            esc_text(&model.boxes[rel.source_idx].id),
            esc_text(&model.boxes[rel.target_idx].id),
            d,
            head
        ));
    }

    let mut names: HashMap<&str, &str> = HashMap::new();
    for b in &model.boxes {
        names.insert(b.id.as_str(), b.name.as_str());
    }
    let mut add_card = |kind: &str, heading: &str, frame: &str, raw: &str| {
        let suffix = names.get(frame).map_or(String::new(), |n| format!(" {}", n));
        let head = layout_label(
            Some(&format!("{} · {}{}", heading, frame, suffix)),
            false,
            measurer,
            small,
            text_room,
            style::BOLD,
        );
        let body = make_text(raw, small, 0);
        let w = card_width
            .max(head.width + 2.0 * BOX_PAD_X)
            .max(body.width + 2.0 * BOX_PAD_X);
        let h = 2.0 * BOX_PAD_Y + head.height + body.height;
        let row_strings: Vec<String> = body
            .lines
            .iter()
            .map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect();
        cards_svg.push_str(&format!(
            "<g class=\"pele-em-{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}/>{}{}</g>",
            kind,
            esc_text(frame),
            num(pad),
            num(y),
            num(w),
            num(h),
            ROUNDED,
            label_svg(
                &head,
                pad + BOX_PAD_X + head.width / 2.0,
                y + BOX_PAD_Y + head.height / 2.0,
                &format!(" font-size=\"{}\" font-weight=\"var(--_hw)\"", small_str)
            ),
            lines_svg(
                &row_strings,
                pad + BOX_PAD_X,
                y + BOX_PAD_Y + head.height + body.line_height / 2.0 + small * 0.35,
                body.line_height,
                &format!(" font-size=\"{}\"", small_str)
            )
        ));
        bottom = y + h;
        y = bottom + CARD_GAP;
        width = width.max(w);
    };

    for note in &model.notes {
        add_card("note", "Note", &note.frame, &note.text);
    }
    for spec in &model.specifications {
        let mut rows = vec![format!("Given  {}", spec.given.join(", "))];
        if !spec.when.is_empty() {
            rows.push(format!("When  {}", spec.when.join(", ")));
        }
        rows.push(format!("Then  {}", spec.then.join(", ")));
        add_card("specification", "Given / When / Then", &spec.frame, &rows.join("\n"));
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (bottom + pad).ceil();
    let content = format!(
        "{}{}{}{}",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        edges_svg,
        nodes_svg,
        cards_svg
    );

    let svg = svg_document(
        "eventmodeling",
        total_width,
        total_height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
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
