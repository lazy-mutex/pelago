use crate::diagrams::common::fit_text::fit_text;
use crate::diagrams::common::fit_width::{cap_width, title_room};
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::ink::ink_fill;
use crate::svg::palette::palette;
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{class_names, resolve_style, rounded, series_color, sets_fill};
use crate::text::entities::decode_entities;
use crate::text::label::layout_label;
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

use super::format::formatter;
use super::layout::{layout_treemap, BoxItem, Insets};
use super::model::{TreemapModel, TreemapNode};

const HEADER: f64 = 28.0;
const SHORT_HEADER: f64 = 18.0;
const SIDE: f64 = 6.0;
const LINE: f64 = 1.25;

fn title_part(raw: &str) -> String {
    esc(decode_entities(raw).trim()).trim().to_string()
}

fn leaf_count(roots: &[TreemapNode]) -> usize {
    let mut count = 0usize;
    let mut stack: Vec<&TreemapNode> = roots.iter().collect();
    while let Some(node) = stack.pop() {
        if let Some(children) = &node.children {
            stack.extend(children.iter());
        } else if node.value.is_some_and(|v| v.is_finite() && v > 0.0) {
            count += 1;
        }
    }
    count
}

fn corners(attrs: &str, b: &BoxItem<'_>) -> String {
    let cap = b.w.min(b.h) / 4.0;
    let r = rounded(attrs);
    if cap < 4.0 {
        r.replacen("rx:var(--_r)", &format!("rx:min(var(--_r),{}px)", num(cap)), 1)
    } else {
        r
    }
}

fn option(config: &Value, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match config.get("treemap").get(key).as_f64() {
        Some(v) if v >= min => v.min(max),
        _ => fallback,
    }
}

pub fn render_treemap(model: &TreemapModel, config: &Value, options: &RenderOptions) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let tiny = js_round(size * 0.75);
    let least = js_round(size * 0.5625);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let treemap_cfg = config.get("treemap");
    let pad = options
        .padding
        .unwrap_or_else(|| option(config, "diagramPadding", 8.0, 0.0, 2000.0));

    let pal = palette("treemap", config);
    let fixed_width = treemap_cfg.get("useMaxWidth").as_bool() == Some(false);
    let responsive = options.responsive.unwrap_or(!fixed_width);

    let width = js_round(cap_width(
        options,
        option(config, "nodeWidth", 64.0, 1.0, 400.0) * 10.0,
        200.0,
        2.0 * pad,
    ));
    let height = js_round(option(config, "nodeHeight", 40.0, 1.0, 400.0) * 10.0);
    let show_values = treemap_cfg.get("showValues").as_bool() != Some(false);
    let format = formatter(treemap_cfg.get("valueFormat").as_str().unwrap_or(","));

    let leaves_n = leaf_count(&model.roots).max(1) as f64;
    let crowd = (((width * height) / leaves_n / 800.0).log10())
        .max(0.0)
        .min(1.0);
    let layout = layout_treemap(
        &model.roots,
        width,
        height,
        Insets {
            gap: option(config, "padding", js_round(2.0 + 2.0 * crowd), 0.0, 200.0),
            header: HEADER,
            short: SHORT_HEADER,
            side: js_round(3.0 + 3.0 * crowd),
        },
    );

    let title = layout_label(
        model.title.as_deref(),
        false,
        measurer,
        size,
        title_room(options),
        style::BOLD,
    );
    let title_height = if title.height > 0.0 { title.height + 8.0 } else { 0.0 };
    let left = pad + ((title.width - width) / 2.0).max(0.0);
    let top = pad + title_height;

    let mut sections = String::new();
    let mut leaves = String::new();
    let empty_styles: &[String] = &[];

    for &b_idx in &layout.placed {
        let b = &layout.arena[b_idx];
        let node = b.node;
        let resolved = resolve_style(node.css_compiled_styles.as_deref().unwrap_or(empty_styles));
        let color = series_color(b.series as i64);
        let x = left + b.x;
        let y = top + b.y;
        let decoded_name = decode_entities(&node.name);
        let name = decoded_name.trim();
        let value = if show_values { format(b.value) } else { String::new() };
        let group = format!("data-id=\"{}\">", esc_text(&node.name));

        let own = title_part(&node.name);
        let mut path_parts: Vec<String> = Vec::new();
        let mut p_opt = b.parent;
        while let Some(p_idx) = p_opt {
            let p = &layout.arena[p_idx];
            if p.headed {
                break;
            }
            let sec = title_part(&p.node.name);
            if !sec.is_empty() {
                path_parts.push(sec);
            }
            p_opt = p.parent;
        }
        path_parts.reverse();
        if !own.is_empty() {
            path_parts.push(own.clone());
        }
        let full = path_parts.join(" › ");
        let tooltip = |drawn: Option<&str>| -> String {
            if full.is_empty() || (drawn == Some(name) && full == own) {
                String::new()
            } else {
                format!("<title>{}</title>", full)
            }
        };

        let rect = format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"",
            num(x),
            num(y),
            num(b.w),
            num(b.h),
            color
        );
        let fixed_font = resolved.font_size;
        let text_style = (if resolved.bold { style::BOLD } else { 0 })
            | (if resolved.italic { style::ITALIC } else { 0 });
        let ink = if sets_fill(&resolved.shape) {
            ink_fill(&resolved.shape, &resolved.text, None)
        } else {
            pal.ink_fill(b.series as isize)
        };

        if node.children.is_some() {
            let mut text_svg = String::new();
            let mut drawn: Option<String> = None;
            if b.headed {
                let short = b.header < HEADER;
                let label_size = fixed_font.unwrap_or(if short { tiny } else { small });
                let value_size = fixed_font.unwrap_or(tiny);
                let room = b.w - 2.0 * SIDE - 4.0;
                let value_width = if !value.is_empty() {
                    measurer.width(&value, value_size, text_style)
                } else {
                    0.0
                };
                let with_value = !short && value_width > 0.0 && room - value_width - 12.0 >= 3.0 * label_size;
                let label = if label_size * LINE < b.header {
                    fit_text(
                        name,
                        if with_value { room - value_width - 12.0 } else { room },
                        label_size,
                        label_size,
                        measurer,
                        text_style | style::BOLD,
                    )
                } else {
                    None
                };
                let baseline = y + b.header / 2.0 + 1.0;
                if let Some(lbl) = label {
                    text_svg.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-weight=\"var(--_hw)\"{}{}{}>{}</text>",
                        num(x + SIDE + 2.0),
                        num(baseline + label_size * 0.35),
                        if short { format!(" font-size=\"{}\"", js_number(label_size)) } else { String::new() },
                        ink,
                        resolved.text,
                        esc(&lbl.text)
                    ));
                    drawn = Some(lbl.text);
                }
                if with_value && value_size * LINE <= HEADER {
                    text_svg.push_str(&format!(
                        "<text class=\"pele-value\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"end\"{}{}>{}</text>",
                        num(x + b.w - SIDE - 2.0),
                        num(baseline + value_size * 0.35),
                        js_number(tiny),
                        if ink.is_empty() { " fill=\"var(--_m)\"" } else { &ink },
                        resolved.text,
                        esc(&value)
                    ));
                }
            }
            let tint = if sets_fill(&resolved.shape) { "" } else { " fill-opacity=\"0.12\"" };
            let shape = if b.headed {
                format!(
                    "{}{}/>",
                    rect,
                    corners(&format!("{} stroke=\"{}\"{}", tint, color, resolved.shape), b)
                )
            } else {
                format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\"/>",
                    num(x),
                    num(y),
                    num(b.w),
                    num(b.h)
                )
            };
            sections.push_str(&format!(
                "<g class=\"pele-node pele-section{}\" {}{}{}{}</g>",
                class_names(node.class_selector.as_deref().unwrap_or("")),
                group,
                tooltip(drawn.as_deref()),
                shape,
                text_svg
            ));
            continue;
        }

        let mut text_svg = String::new();
        let room = b.w - 8.0;
        let smallest = fixed_font.unwrap_or(least);
        let tallest = match fixed_font {
            None => size.min(((b.h - 4.0) / LINE).floor()),
            Some(f) if f * LINE <= b.h - 4.0 => f,
            Some(_) => 0.0,
        };
        let label = if tallest >= smallest {
            fit_text(name, room, tallest, smallest, measurer, text_style)
        } else {
            None
        };
        if let Some(ref lbl) = label {
            let cx = num(x + b.w / 2.0);
            let cy = y + b.h / 2.0;
            let value_size = fixed_font.unwrap_or_else(|| least.max(js_round(lbl.size * 0.8)));
            let lines = (lbl.size + value_size) * LINE;
            if !value.is_empty() && lines <= b.h - 4.0 && measurer.width(&value, value_size, text_style) <= room {
                let first = cy - lines / 2.0 + (lbl.size * LINE) / 2.0;
                let second = cy + lines / 2.0 - (value_size * LINE) / 2.0;
                text_svg = format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\"{}{}>{}</text><text class=\"pele-value\" x=\"{}\" y=\"{}\" font-size=\"{}\"{}{}>{}</text>",
                    cx,
                    num(first + lbl.size * 0.35),
                    num(lbl.size),
                    ink,
                    resolved.text,
                    esc(&lbl.text),
                    cx,
                    num(second + value_size * 0.35),
                    num(value_size),
                    ink,
                    resolved.text,
                    esc(&value)
                );
            } else {
                text_svg = format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\"{}{}>{}</text>",
                    cx,
                    num(cy + lbl.size * 0.35),
                    num(lbl.size),
                    ink,
                    resolved.text,
                    esc(&lbl.text)
                );
            }
        }
        leaves.push_str(&format!(
            "<g class=\"pele-node pele-leaf{}\" {}{}{}{}/>{}</g>",
            class_names(node.class_selector.as_deref().unwrap_or("")),
            group,
            tooltip(label.as_ref().map(|l| l.text.as_str())),
            rect,
            corners(&resolved.shape, b),
            text_svg
        ));
    }

    let total_width = (width.max(title.width) + 2.0 * pad).ceil();
    let total_height = (height + title_height + 2.0 * pad).ceil();
    let mut content = label_svg(
        &title,
        total_width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    );
    if !sections.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-sections\" font-size=\"{}\">{}</g>",
            js_number(small),
            sections
        ));
    }
    if !leaves.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-leaves\" text-anchor=\"middle\" fill=\"var(--_bg)\">{}</g>",
            leaves
        ));
    }

    let svg = svg_document(
        "treemap",
        total_width,
        total_height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive,
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: &pal.style,
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
