use super::db::{Direction, TimelineDb};
use crate::chars::is_space;
use crate::diagrams::common::fit::{fit_label, plain_label};
use crate::options::{turn_to_fit, RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::edges::{marker, ARROW};
use crate::svg::palette::{palette, Palette};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{rounded, series_color};
use crate::text::label::Label;
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;

const PAD_X: f64 = 12.0;
const PAD_Y: f64 = 8.0;
const MIN_TEXT: f64 = 152.0;
const MAX_TEXT: f64 = 216.0;
const EVENT_TEXT_TD: f64 = 264.0;
const GAP: f64 = 20.0;
const AXIS: f64 = 20.0;
const SPINE: f64 = 14.0;
const EVENT_GAP: f64 = 8.0;
const SECTION_GAP: f64 = 8.0;
const TAIL: f64 = 12.0;
const ON_COLOR: &str = " fill=\"var(--_bg)\"";
const LINE: &str = " fill=\"none\" stroke=\"var(--_b)\"";

struct Group {
    section: isize,
    periods: Vec<usize>,
}

fn rect_box(x: f64, y: f64, w: f64, h: f64, attrs: &str) -> String {
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
        num(x),
        num(y),
        num(w),
        num(h),
        rounded(attrs)
    )
}

pub fn render_timeline(model: &TimelineDb, config: &Value, options: &RenderOptions) -> Rendered {
    let pal = palette("timeline", config);
    if model.direction == Direction::Td {
        return draw(model, config, options, &pal, true);
    }
    turn_to_fit(options, &|down| draw(model, config, options, &pal, down))
}

fn draw(
    model: &TimelineDb,
    config: &Value,
    options: &RenderOptions,
    pal: &Palette,
    vertical: bool,
) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let small_str = js_number(small);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let multicolor = config.get("timeline").get("disableMulticolor").as_bool() != Some(true);
    let periods = &model.periods;
    let has_sections = !model.sections.is_empty();

    // Mermaid draws nothing for periods that come before the first section; here they lead, in a neutral color.
    let mut groups: Vec<Group> = (0..model.sections.len())
        .map(|section| Group {
            section: section as isize,
            periods: Vec::new(),
        })
        .collect();
    let mut leading = Group {
        section: -1,
        periods: Vec::new(),
    };
    for (i, period) in periods.iter().enumerate() {
        if period.section_index >= 0 && (period.section_index as usize) < groups.len() {
            groups[period.section_index as usize].periods.push(i);
        } else {
            leading.periods.push(i);
        }
    }
    if !leading.periods.is_empty() {
        groups.insert(0, leading);
    }

    // Columns widen, up to a point, for a word that would not fit; past that the word is cut.
    // Running down, the two columns share the width the host has.
    let room = match options.max_width {
        Some(mw) if vertical && mw > 0.0 => mw - 2.0 * pad,
        _ => f64::INFINITY,
    };
    let taken = AXIS + SPINE * 2.0 + 4.0 * PAD_X;
    let period_text = js_round((room - taken) * 0.38).max(80.0).min(MIN_TEXT);
    let event_text = (room - taken - period_text).max(110.0).min(EVENT_TEXT_TD);
    let mut text_w = period_text;
    let event_text_w = |tw: f64| if vertical { event_text } else { tw };

    let mut period_labels: Vec<Label> = periods
        .iter()
        .map(|p| plain_label(Some(&p.text), measurer, size, text_w, 0))
        .collect();
    let mut event_labels: Vec<Vec<Label>> = periods
        .iter()
        .map(|p| {
            p.events
                .iter()
                .map(|e| plain_label(Some(e), measurer, small, event_text_w(text_w), 0))
                .collect()
        })
        .collect();

    let mut widest = 0.0f64;
    for label in &period_labels {
        widest = widest.max(label.width);
    }
    if !vertical {
        for labels in &event_labels {
            for label in labels {
                widest = widest.max(label.width);
            }
        }
    }
    let mut overflow = widest > text_w;
    if vertical {
        for labels in &event_labels {
            for label in labels {
                overflow = overflow || label.width > event_text;
            }
        }
    }
    if overflow {
        let cap = if room.is_infinite() {
            MAX_TEXT
        } else {
            period_text
        };
        text_w = widest.ceil().max(period_text).min(cap);
        period_labels = periods
            .iter()
            .map(|p| fit_label(Some(&p.text), measurer, size, text_w, 0))
            .collect();
        let etw = event_text_w(text_w);
        event_labels = periods
            .iter()
            .map(|p| {
                p.events
                    .iter()
                    .map(|e| fit_label(Some(e), measurer, small, etw, 0))
                    .collect()
            })
            .collect();
    }

    let line_height = js_round(size * 1.5);
    let small_line_height = js_round(small * 1.5);
    let col_w = text_w + 2.0 * PAD_X;
    let event_w = event_text_w(text_w) + 2.0 * PAD_X;
    let chip_height = |label: &Label| label.height.max(line_height) + 2.0 * PAD_Y;
    let event_height = |label: &Label| label.height.max(small_line_height) + 2.0 * PAD_Y;

    let series_of = |group: &Group, period_idx: usize| -> Option<isize> {
        if group.section >= 0 {
            Some(group.section)
        } else if has_sections {
            None
        } else if multicolor {
            Some(period_idx as isize)
        } else {
            Some(0)
        }
    };
    let color_of = |group: &Group, period_idx: usize| -> String {
        match series_of(group, period_idx) {
            None => "var(--_m)".to_string(),
            Some(s) => series_color(s as i64),
        }
    };
    let on_color = |series: Option<isize>| -> String {
        if let Some(s) = series {
            let fill = pal.ink_fill(s);
            if !fill.is_empty() {
                return fill;
            }
        }
        ON_COLOR.to_string()
    };

    let has_events = event_labels.iter().any(|labels| !labels.is_empty());
    let mut slots = 0usize;
    for group in &groups {
        slots += group.periods.len().max(1);
    }

    let pitch = col_w + GAP;
    let drawn = !periods.is_empty();
    let body_w = if vertical {
        col_w
            + if has_events {
                AXIS + SPINE * 2.0 + event_w
            } else if drawn {
                AXIS + TAIL
            } else {
                0.0
            }
    } else if slots > 0 {
        slots as f64 * pitch - GAP + if drawn { TAIL + ARROW } else { 0.0 }
    } else {
        0.0
    };
    let title = fit_label(
        model.title.as_deref(),
        measurer,
        size,
        body_w.max(320.0),
        style::BOLD,
    );
    let title_h = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let width = body_w.max(title.width);
    let left = pad + (width - body_w) / 2.0;
    let top = pad + title_h;

    let section_label = |group: &Group, w: f64| -> Label {
        fit_label(
            Some(&model.sections[group.section as usize]),
            measurer,
            size,
            w - 2.0 * PAD_X,
            style::BOLD,
        )
    };
    let header = |group: &Group, label: &Label, x: f64, y: f64, w: f64, h: f64| -> String {
        let fill_attr = format!(" fill=\"{}\"", series_color(group.section as i64));
        let lbl_attr = format!(
            " font-weight=\"var(--_hw)\"{}",
            on_color(Some(group.section))
        );
        format!(
            "{}{}",
            rect_box(x, y, w, h, &fill_attr),
            label_svg(label, x + w / 2.0, y + h / 2.0, &lbl_attr)
        )
    };
    let period_svg = |i: usize, group: &Group, color: &str, x: f64, y: f64, h: f64| -> String {
        let fill_attr = format!(" fill=\"{}\"", color);
        let lbl_attr = on_color(series_of(group, i));
        format!(
            "{}{}",
            rect_box(x, y, col_w, h, &fill_attr),
            label_svg(&period_labels[i], x + col_w / 2.0, y + h / 2.0, &lbl_attr)
        )
    };
    let event_label_attr = format!(" font-size=\"{}\"", small_str);
    let event_svg = |label: &Label, color: &str, x: f64, y: f64, h: f64| -> String {
        let fill_attr = format!(" fill=\"{}\" fill-opacity=\"0.16\"", color);
        format!(
            "<g class=\"pele-node pele-event\">{}{}</g>",
            rect_box(x, y, event_w, h, &fill_attr),
            label_svg(label, x + event_w / 2.0, y + h / 2.0, &event_label_attr)
        )
    };
    let dot_svg = |x: f64, y: f64, color: &str| -> String {
        format!(
            "<circle class=\"pele-dot\" cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"{}\" stroke=\"var(--_bg)\" stroke-width=\"2\"/>",
            num(x),
            num(y),
            color
        )
    };
    let open_group = |group: &Group| -> String {
        if group.section >= 0 {
            format!(
                "<g class=\"pele-cluster pele-section\" data-id=\"{}\">",
                esc_text(model.sections[group.section as usize].trim_matches(is_space))
            )
        } else {
            String::new()
        }
    };
    let open_period = |i: usize| -> String {
        format!(
            "<g class=\"pele-node pele-period\" data-id=\"{}\">",
            esc_text(periods[i].text.trim_matches(is_space))
        )
    };

    let mut body = String::new();
    let height: f64;

    if vertical {
        let axis_x = left + col_w + AXIS;
        let spine_x = axis_x + SPINE;
        let events_x = spine_x + SPINE;
        let mut y = top;
        for group in &groups {
            body.push_str(&open_group(group));
            if group.section >= 0 {
                let label = section_label(group, body_w);
                let h = chip_height(&label);
                body.push_str(&header(group, &label, left, y, body_w, h));
                y += h + SECTION_GAP + 4.0;
            }
            for &i in &group.periods {
                let color = color_of(group, i);
                let h = chip_height(&period_labels[i]);
                // A period and its first event share one center line, so the line between them runs straight.
                let first = if !event_labels[i].is_empty() {
                    event_height(&event_labels[i][0])
                } else {
                    h
                };
                let cy = y + h.max(first) / 2.0;
                let mut d = format!("M{},{}H{}", num(left + col_w), num(cy), num(axis_x));
                let mut cards = String::new();
                let mut ey = cy - first / 2.0;
                let mut last = cy;
                for label in &event_labels[i] {
                    let eh = event_height(label);
                    last = ey + eh / 2.0;
                    d.push_str(&format!("M{},{}H{}", num(spine_x), num(last), num(events_x)));
                    cards.push_str(&event_svg(label, &color, events_x, ey, eh));
                    ey += eh + EVENT_GAP;
                }
                if !cards.is_empty() {
                    d.push_str(&format!(
                        "M{},{}H{}V{}",
                        num(axis_x),
                        num(cy),
                        num(spine_x),
                        num(last)
                    ));
                }
                body.push_str(&format!(
                    "{}{}<path class=\"pele-edge\" d=\"{}\"{}/>{}{}</g>",
                    open_period(i),
                    period_svg(i, group, &color, left, cy - h / 2.0, h),
                    d,
                    LINE,
                    dot_svg(axis_x, cy, &color),
                    cards
                ));
                y = (cy + h / 2.0).max(ey - EVENT_GAP) + GAP;
            }
            if group.periods.is_empty() {
                y += GAP - SECTION_GAP - 4.0;
            }
            if group.section >= 0 {
                body.push_str("</g>");
            }
        }
        if !groups.is_empty() {
            y -= GAP;
        }
        if drawn {
            let tip = y + TAIL + ARROW;
            body = format!(
                "<path class=\"pele-axis\" d=\"M{},{}V{}\" fill=\"none\" stroke=\"var(--_l)\"/>{}{}",
                num(axis_x),
                num(top),
                num(tip - ARROW + 1.0),
                marker("arrow_point", axis_x, tip, 0.0, 1.0, "var(--_l)", "none"),
                body
            );
            y = tip;
        }
        height = y + pad;
    } else {
        let mut header_h = 0.0f64;
        let section_labels: Vec<Option<Label>> = groups
            .iter()
            .map(|group| {
                if group.section < 0 {
                    return None;
                }
                let label = section_label(group, group.periods.len().max(1) as f64 * pitch - GAP);
                header_h = header_h.max(chip_height(&label));
                Some(label)
            })
            .collect();
        let mut chip_h = 0.0f64;
        for label in &period_labels {
            chip_h = chip_h.max(chip_height(label));
        }
        let periods_y = top + if has_sections {
            header_h + SECTION_GAP
        } else {
            0.0
        };
        let axis_y = periods_y + chip_h + if chip_h > 0.0 { AXIS } else { 0.0 };
        let events_y = axis_y + AXIS;
        let mut bottom = if drawn {
            axis_y + TAIL
        } else {
            top + header_h
        };
        let mut slot = 0usize;
        for (g, group) in groups.iter().enumerate() {
            let span = group.periods.len().max(1);
            let gx = left + slot as f64 * pitch;
            body.push_str(&open_group(group));
            if let Some(ref label) = section_labels[g] {
                body.push_str(&header(
                    group,
                    label,
                    gx,
                    top,
                    span as f64 * pitch - GAP,
                    header_h,
                ));
            }
            for (k, &i) in group.periods.iter().enumerate() {
                let color = color_of(group, i);
                let x = gx + k as f64 * pitch;
                let cx = x + col_w / 2.0;
                let end_y = if !event_labels[i].is_empty() {
                    events_y
                } else {
                    axis_y
                };
                let mut d = format!("M{},{}V{}", num(cx), num(periods_y + chip_h), num(end_y));
                let mut cards = String::new();
                let mut ey = events_y;
                for event_label in &event_labels[i] {
                    let eh = event_height(event_label);
                    if ey > events_y {
                        d.push_str(&format!("M{},{}V{}", num(cx), num(ey - EVENT_GAP), num(ey)));
                    }
                    cards.push_str(&event_svg(event_label, &color, x, ey, eh));
                    ey += eh + EVENT_GAP;
                }
                bottom = bottom.max(ey - EVENT_GAP);
                body.push_str(&format!(
                    "{}{}<path class=\"pele-edge\" d=\"{}\"{}/>{}{}</g>",
                    open_period(i),
                    period_svg(i, group, &color, x, periods_y, chip_h),
                    d,
                    LINE,
                    dot_svg(cx, axis_y, &color),
                    cards
                ));
            }
            if group.section >= 0 {
                body.push_str("</g>");
            }
            slot += span;
        }
        if drawn {
            let tip = left + body_w;
            body = format!(
                "<path class=\"pele-axis\" d=\"M{},{}H{}\" fill=\"none\" stroke=\"var(--_l)\"/>{}{}",
                num(left),
                num(axis_y),
                num(tip - ARROW + 1.0),
                marker("arrow_point", tip, axis_y, 1.0, 0.0, "var(--_l)", "none"),
                body
            );
        }
        height = bottom + pad;
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = height.max(pad * 2.0).ceil();
    let content = format!(
        "{}{}",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        body
    );

    let fixed = config.get("timeline").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "timeline",
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
        &Accessible {
            acc_title: model.acc_title.as_deref(),
            acc_descr: model.acc_descr.as_deref(),
        },
        &content,
    );
    Rendered {
        svg,
        width: total_width,
        height: total_height,
        links: Vec::new(),
    }
}
