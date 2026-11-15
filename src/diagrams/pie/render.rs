use super::model::PieModel;
use crate::diagrams::common::fit_width::{fit_width, title_room};
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::palette::{palette, Palette};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::series_color;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;
use std::f64::consts::PI;

const RADIUS: f64 = 120.0;
const SWATCH: f64 = 12.0;
const ROW: f64 = 26.0;
const GAP: f64 = 32.0;
const OUTSIDE: f64 = 28.0;

fn to_fixed_0(v: f64) -> String {
    format!("{}", v.round() as i64)
}

fn point(angle: f64, radius: f64) -> String {
    format!("{},{}", num(angle.sin() * radius), num(-angle.cos() * radius))
}

fn slice(start: f64, end: f64, outer: f64, inner: f64) -> String {
    if end - start >= PI * 2.0 - 1e-9 {
        let ring = |r: f64, sweep: u8| -> String {
            format!(
                "M0,{}A{},{} 0 1 {} 0,{}A{},{} 0 1 {} 0,{}Z",
                num(-r),
                num(r),
                num(r),
                sweep,
                num(r),
                num(r),
                num(r),
                sweep,
                num(-r)
            )
        };
        let mut s = ring(outer, 1);
        if inner > 0.0 {
            s.push_str(&ring(inner, 0));
        }
        return s;
    }
    let large = if end - start > PI { 1 } else { 0 };
    if inner <= 0.0 {
        return format!(
            "M0,0L{}A{},{} 0 {} 1 {}Z",
            point(start, outer),
            num(outer),
            num(outer),
            large,
            point(end, outer)
        );
    }
    format!(
        "M{}A{},{} 0 {} 1 {}L{}A{},{} 0 {} 0 {}Z",
        point(start, outer),
        num(outer),
        num(outer),
        large,
        point(end, outer),
        point(end, inner),
        num(inner),
        num(inner),
        large,
        point(start, inner)
    )
}

pub fn render_pie(model: &PieModel, config: &Value, options: &RenderOptions) -> Rendered {
    let pal = palette("pie", config);
    fit_width(options, RADIUS * 2.0, 140.0, |diameter, tight| {
        draw(model, config, options, &pal, diameter / 2.0, tight)
    })
}

struct PlacedOutside {
    x: f64,
    y: f64,
    width: f64,
}

fn draw(
    model: &PieModel,
    config: &Value,
    options: &RenderOptions,
    pal: &Palette,
    radius: f64,
    tight: bool,
) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    let pie_cfg = config.get("pie");
    let text_position = pie_cfg
        .get("textPosition")
        .as_f64()
        .filter(|&p| (0.0..=1.0).contains(&p))
        .unwrap_or(0.75);
    let inner = pie_cfg
        .get("donutHole")
        .as_f64()
        .filter(|&h| h > 0.0 && h <= 0.9)
        .map_or(0.0, |h| h * radius);
    let lp_val = pie_cfg.get("legendPosition");
    let placed = if matches!(lp_val, Value::Null) {
        "right".to_string()
    } else {
        lp_val.to_text()
    };
    let legend_position: &str = if tight && (placed == "left" || placed == "right") {
        "bottom"
    } else {
        &placed
    };

    let entries = &model.sections;
    let sum: f64 = entries.iter().map(|(_, v)| *v).sum();

    let legend: Vec<Label> = entries
        .iter()
        .map(|(label, value)| {
            let text = if model.show_data {
                format!("{} [{}]", label, js_number(*value))
            } else {
                label.clone()
            };
            layout_label(Some(&text), false, measurer, small, 4000.0, 0)
        })
        .collect();
    let mut legend_width = 0.0_f64;
    for label in &legend {
        legend_width = legend_width.max(label.width);
    }
    if !legend.is_empty() {
        legend_width += SWATCH + 8.0;
    }
    let legend_height = legend.len() as f64 * ROW;

    let title = layout_label(model.title.as_deref(), false, measurer, size, title_room(options), style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };

    // A slice too thin for its percentage gets the label outside the rim, which needs a margin.
    let shares: Vec<f64> = entries
        .iter()
        .map(|(_, value)| {
            let share = if sum > 0.0 { *value / sum } else { 0.0 };
            if share * 100.0 >= 1.0 && to_fixed_0(share * 100.0) != "0" {
                share
            } else {
                0.0
            }
        })
        .collect();
    let narrow: Vec<bool> = shares
        .iter()
        .map(|&share| {
            if share == 0.0 || share >= 0.5 {
                return false;
            }
            let chord = 2.0 * radius * text_position * (share * PI).sin();
            let pct = format!("{}%", to_fixed_0(share * 100.0));
            chord < measurer.width(&pct, small, 0) + 6.0
        })
        .collect();
    let margin = if narrow.contains(&true) { OUTSIDE } else { 0.0 };

    let side = if matches!(legend_position, "left" | "right" | "center") {
        legend_position
    } else {
        ""
    };
    let stacked = legend_position == "top" || legend_position == "bottom";
    let diameter = (radius + margin) * 2.0;
    let mut width = diameter;
    let mut height = diameter;
    if side == "left" || side == "right" {
        if legend_width > 0.0 {
            width += GAP + legend_width;
        }
        height = height.max(legend_height);
    } else if stacked {
        width = width.max(legend_width);
        if legend_height > 0.0 {
            height += 20.0 + legend_height;
        }
    }
    width = width.max(title.width);

    let cx = pad
        + if side == "left" {
            legend_width + GAP + diameter / 2.0
        } else if side == "right" {
            diameter / 2.0
        } else {
            width / 2.0
        };
    let body_top = pad + title_height;
    let cy = body_top
        + if legend_position == "top" {
            legend_height + 20.0 + diameter / 2.0
        } else if stacked {
            diameter / 2.0
        } else {
            height / 2.0
        };

    let mut slices = String::new();
    let mut labels = String::new();
    let mut outside = String::new();
    let mut angle = 0.0_f64;
    let rim = radius + OUTSIDE / 2.0;
    let mut placed_outside: Vec<PlacedOutside> = Vec::new();
    let clear = |placed_outside: &[PlacedOutside], x: f64, y: f64, w: f64| -> bool {
        placed_outside.iter().all(|other| {
            (x - other.x).abs() >= (w + other.width) / 2.0 + 2.0
                || (y - other.y).abs() >= small + 2.0
        })
    };

    for (index, (label, _)) in entries.iter().enumerate() {
        let share = shares[index];
        if share == 0.0 {
            continue;
        }
        let end = angle + share * PI * 2.0;
        slices.push_str(&format!(
            "<path class=\"pele-slice\" data-id=\"{}\" d=\"{}\" fill=\"{}\"/>",
            esc_text(label),
            slice(angle, end, radius, inner),
            series_color(index as i64)
        ));
        let mid = (angle + end) / 2.0;
        let text = format!("{}%", to_fixed_0(share * 100.0));
        if narrow[index] {
            let w = measurer.width(&text, small, 0);
            let mut at = mid;
            let mut step = 0;
            while step < 360 && !clear(&placed_outside, at.sin() * rim, -at.cos() * rim, w) {
                at += 2.0 / rim;
                step += 1;
            }
            let x = at.sin() * rim;
            let y = -at.cos() * rim;
            placed_outside.push(PlacedOutside { x, y, width: w });
            outside.push_str(&format!(
                "<text x=\"{}\" y=\"{}\">{}</text>",
                num(x),
                num(y + small * 0.35),
                text
            ));
        } else {
            let r = radius * text_position;
            labels.push_str(&format!(
                "<text x=\"{}\" y=\"{}\"{}>{}</text>",
                num(mid.sin() * r),
                num(-mid.cos() * r + small * 0.35),
                pal.ink_fill(index as isize),
                text
            ));
        }
        angle = end;
    }
    if slices.is_empty() {
        slices = format!(
            "<circle class=\"pele-slice\" r=\"{}\" fill=\"none\" stroke=\"var(--_b)\"/>",
            js_number(radius)
        );
    }

    let mut legend_out = String::new();
    let (lx, ly) = if side == "right" {
        (cx + diameter / 2.0 + GAP, body_top + (height - legend_height) / 2.0)
    } else if side == "left" {
        (pad, body_top + (height - legend_height) / 2.0)
    } else if side == "center" {
        (cx - legend_width / 2.0, cy - legend_height / 2.0)
    } else {
        (
            pad + (width - legend_width) / 2.0,
            if legend_position == "top" {
                body_top
            } else {
                body_top + diameter + 20.0
            },
        )
    };

    for (index, label) in legend.iter().enumerate() {
        let y = ly + index as f64 * ROW + ROW / 2.0;
        legend_out.push_str(&format!(
            "<g class=\"pele-legend-item\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"2\" fill=\"{}\"/>{}</g>",
            esc_text(&entries[index].0),
            num(lx),
            num(y - SWATCH / 2.0),
            js_number(SWATCH),
            js_number(SWATCH),
            series_color(index as i64),
            label_svg(label, lx + SWATCH + 8.0 + label.width / 2.0, y, "")
        ));
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (height + title_height + 2.0 * pad).ceil();
    let small_str = js_number(small);
    let mut content = format!(
        "{}<g class=\"pele-slices\" transform=\"translate({},{})\" stroke=\"var(--_bg)\" stroke-width=\"1\" stroke-linejoin=\"round\">{}</g>",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        num(cx),
        num(cy),
        slices
    );
    if !labels.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-slice-labels\" transform=\"translate({},{})\" font-size=\"{}\" text-anchor=\"middle\" fill=\"var(--_bg)\">{}</g>",
            num(cx),
            num(cy),
            small_str,
            labels
        ));
    }
    if !outside.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-slice-labels\" transform=\"translate({},{})\" font-size=\"{}\" text-anchor=\"middle\">{}</g>",
            num(cx),
            num(cy),
            small_str,
            outside
        ));
    }
    if !legend_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-legend\" font-size=\"{}\">{}</g>",
            small_str, legend_out
        ));
    }

    let fixed = pie_cfg.get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "pie",
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
