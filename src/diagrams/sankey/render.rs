use std::collections::HashMap;

use crate::chars::is_word;
use crate::errors::{PeleError, Result};
use crate::options::{fit_width, title_room, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{resolve_style, series_color};
use crate::text::entities::decode_entities;
use crate::text::label::layout_label;
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

use super::db::SankeyDb;
use super::layout::{layout_sankey, Alignment, LayoutOptions};

const LABEL_GAP: f64 = 6.0;
const VALUE_GAP: f64 = 15.0;
const LABEL_ROOM: f64 = 1.8;
const CAREFUL: usize = 300;

fn to_base36(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::with_capacity(7);
    while n > 0 {
        buf.push(DIGITS[(n % 36) as usize] as char);
        n /= 36;
    }
    buf.iter().rev().collect()
}

// Two 32-bit hashes, so different gradients practically never share an id.
fn content_id(text: &str) -> String {
    let mut a: u32 = 0x811c_9dc5;
    let mut b: u32 = 5381;
    for c in text.encode_utf16() {
        let cu = c as u32;
        a = (a ^ cu).wrapping_mul(0x0100_0193);
        b = b.wrapping_mul(33).wrapping_add(cu);
    }
    format!("{}{}", to_base36(a), to_base36(b))
}

fn number_option(conf: &Value, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match conf.get(key).as_f64() {
        Some(v) if v >= min => v.min(max),
        _ => fallback,
    }
}

fn text_option(conf: &Value, key: &str) -> String {
    match conf.get(key) {
        Value::Str(s) => s.clone(),
        Value::Num(n) if n.is_finite() => js_number(*n),
        _ => String::new(),
    }
}

// Two decimals, as Mermaid shows values; numbers too large for that are left as they are.
fn rounded_str(value: f64) -> String {
    let r = if value.abs() < 1e15 {
        js_round(value * 100.0) / 100.0
    } else {
        value
    };
    js_number(r)
}

// A colour from the configuration, as an inline style that overrides the series colour.
fn fill_style(color: &Value) -> String {
    match color.as_str() {
        Some(c) => resolve_style(&[format!("fill:{}", c)]).shape,
        None => String::new(),
    }
}

#[derive(Clone)]
struct LabelBox {
    value: String,
    width: f64,
    after: bool,
    left: f64,
    y: f64,
}

pub fn render_sankey(model: &SankeyDb, config: &Value, options: &RenderOptions) -> Result<Rendered> {
    let sankey_val = config.get("sankey");
    let empty_obj = Value::object();
    let conf = if sankey_val.is_object() { sankey_val } else { &empty_obj };
    let natural = number_option(conf, "width", 600.0, 1.0, 10000.0);
    let mut err: Option<PeleError> = None;
    let rendered = fit_width(options, natural, 160.0, |chart_width, _| {
        if err.is_some() {
            return Rendered { svg: String::new(), width: 0.0, height: 0.0, links: Vec::new() };
        }
        match draw(model, conf, config, options, chart_width, natural) {
            Ok(r) => r,
            Err(e) => {
                err = Some(e);
                Rendered { svg: String::new(), width: 0.0, height: 0.0, links: Vec::new() }
            }
        }
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(rendered)
}

fn draw(
    model: &SankeyDb,
    conf: &Value,
    root_config: &Value,
    options: &RenderOptions,
    chart_width: f64,
    natural: f64,
) -> Result<Rendered> {
    let size = options.font_size.unwrap_or(16.0);
    let least = js_round(size * 0.625);
    // Labels shrink with a chart narrowed to fit, as they would if the whole picture were scaled down.
    let mut small = least.max(js_round(size * 0.875 * (chart_width / natural).min(1.0)));
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    let chart_height = number_option(conf, "height", 400.0, 1.0, 10000.0);
    let node_width = number_option(conf, "nodeWidth", 10.0, 1.0, 200.0).min(chart_width);
    let show_values = conf.get("showValues").as_bool() != Some(false);
    let prefix = decode_entities(&text_option(conf, "prefix"));
    let suffix = decode_entities(&text_option(conf, "suffix"));
    let outlined = conf.get("labelStyle").as_str() == Some("outlined");
    let alignment = match conf.get("nodeAlignment").as_str() {
        Some("left") => Alignment::Left,
        Some("right") => Alignment::Right,
        Some("center") => Alignment::Center,
        _ => Alignment::Justify,
    };
    let link_color = conf.get("linkColor").as_str().unwrap_or("gradient");
    let node_padding = number_option(conf, "nodePadding", 12.0, 0.0, 200.0) + if show_values { VALUE_GAP } else { 0.0 };

    let lay = |label_size: f64| {
        layout_sankey(
            model,
            &LayoutOptions {
                width: chart_width,
                height: chart_height,
                node_width,
                node_padding,
                label_height: label_size * 1.15,
                alignment,
            },
        )
    };
    let mut layout = lay(small)?;
    // Labels also shrink with the most crowded column, so each keeps about LABEL_ROOM lines.
    let mut rows: HashMap<usize, usize> = HashMap::new();
    let mut fullest = 1usize;
    for node in &layout.nodes {
        let count = rows.get(&node.layer).copied().unwrap_or(0) + 1;
        rows.insert(node.layer, count);
        if count > fullest {
            fullest = count;
        }
    }
    let crowded = least.max(js_round(chart_height / fullest as f64 / LABEL_ROOM));
    if crowded < small {
        small = crowded;
        layout = lay(small)?;
    }
    let nodes = layout.nodes;
    let links = layout.links;
    let line_height = small * 1.5;

    let mut custom: HashMap<String, String> = HashMap::new();
    if let Value::Obj(entries) = conf.get("nodeColors") {
        for (id, color) in entries {
            let st = fill_style(color);
            if !st.is_empty() {
                custom.insert(id.clone(), st);
            }
        }
    }
    let node_style = |i: usize| -> &str {
        custom.get(&model.nodes[i].id).map(|s| s.as_str()).unwrap_or("")
    };
    // Match the whole fill to preserve semicolons in escaped entities.
    let node_color = |i: usize| -> String {
        let s = node_style(i);
        s.strip_prefix(" style=\"fill:")
            .and_then(|rest| rest.strip_suffix(";\""))
            .filter(|inner| !inner.contains(['\n', '\r']))
            .map(|inner| inner.to_string())
            .unwrap_or_else(|| series_color(i as i64))
    };

    // Labels sit beside the bars, on the side that faces the middle of the chart unless that
    // side is crowded.
    let mut central = 0usize;
    let mut most = 0.0f64;
    for node in &nodes {
        if node.value > most {
            most = node.value;
            central = node.layer;
        }
    }
    let mut min_x = 0.0f64;
    let mut max_x = chart_width;
    let mut min_y = 0.0f64;
    let mut max_y = chart_height;
    let mut labels: Vec<LabelBox> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let name = decode_entities(&model.nodes[i].id);
            let value = if show_values {
                format!("{}{}{}", prefix, rounded_str(node.value), suffix)
            } else {
                String::new()
            };
            let full_text = if !value.is_empty() {
                format!("{} {}", name, value)
            } else {
                name
            };
            let width = measurer.width(&full_text, small, 0);
            let after = if outlined {
                node.layer >= central
            } else {
                node.x < chart_width / 2.0
            };
            let x = if after {
                node.x + node_width + LABEL_GAP
            } else {
                node.x - LABEL_GAP
            };
            let y = node.y + node.height / 2.0;
            min_x = min_x.min(if after { x } else { x - width });
            max_x = max_x.max(if after { x + width } else { x });
            min_y = min_y.min(y - small * 0.75);
            max_y = max_y.max(y + small * 0.75);
            LabelBox {
                value,
                width,
                after,
                left: if after { x } else { x - width },
                y,
            }
        })
        .collect();

    // A label moves at most one line; retain two bands on each side.
    let mut bands: HashMap<i64, Vec<usize>> = HashMap::new();
    if nodes.len() <= CAREFUL {
        for (idx, label) in labels.iter().enumerate() {
            let band = (label.y / line_height).floor() as i64;
            for key in [band - 2, band - 1, band, band + 1, band + 2] {
                bands.entry(key).or_default().push(idx);
            }
        }
    }

    let overlaps_label = |self_idx: usize, left: f64, y: f64, width: f64, labels_slice: &[LabelBox]| -> bool {
        let band = (y / line_height).floor() as i64;
        let Some(nearby) = bands.get(&band) else {
            return false;
        };
        nearby.iter().any(|&other_idx| {
            if other_idx == self_idx {
                return false;
            }
            let other = &labels_slice[other_idx];
            (other.y - y).abs() < line_height
                && left < other.left + other.width + LABEL_GAP
                && left + width > other.left - LABEL_GAP
        })
    };

    let mut labels_out = String::new();
    let mut nodes_out = String::new();
    for (i, node) in nodes.iter().enumerate() {
        let id = &model.nodes[i].id;
        let name = decode_entities(id);
        let height = node.height.max(1.0);
        nodes_out.push_str(&format!(
            "<rect class=\"pele-sankey-node\" data-id=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"{}/>",
            esc_text(id),
            num(node.x),
            num(node.y),
            num(node_width),
            num(height),
            num((height / 2.0).min(2.0)),
            series_color(i as i64),
            node_style(i),
        ));

        let cur_width = labels[i].width;
        let cur_left = labels[i].left;
        let cur_y = labels[i].y;
        let cur_after = labels[i].after;
        // Try the other side, then an adjacent line when both sides are crowded.
        if nodes.len() <= CAREFUL && overlaps_label(i, cur_left, cur_y, cur_width, &labels) {
            'place: for dy in [0.0, -line_height, line_height] {
                for after in [cur_after, !cur_after] {
                    let left = if after {
                        node.x + node_width + LABEL_GAP
                    } else {
                        node.x - LABEL_GAP - cur_width
                    };
                    let y = cur_y + dy;
                    if overlaps_label(i, left, y, cur_width, &labels)
                        || left < min_x
                        || left + cur_width > max_x
                        || y - small * 0.75 < min_y
                        || y + small * 0.75 > max_y
                    {
                        continue;
                    }
                    if nodes.iter().enumerate().any(|(j, bar)| {
                        j != i
                            && left < bar.x + node_width + LABEL_GAP
                            && left + cur_width > bar.x - LABEL_GAP
                            && y - small * 0.75 < bar.y + bar.height
                            && y + small * 0.75 > bar.y
                    }) {
                        continue;
                    }
                    // A label stays nearer its own bar than any other in the column, so it can't name that one.
                    let own = (node.y - y).max(y - node.y - node.height).max(0.0);
                    if nodes.iter().enumerate().any(|(j, bar)| {
                        j != i
                            && bar.x == node.x
                            && (bar.y - y).max(y - bar.y - bar.height).max(0.0) <= own
                    }) {
                        continue;
                    }
                    labels[i].left = left;
                    labels[i].y = y;
                    labels[i].after = after;
                    break 'place;
                }
            }
        }

        let after = labels[i].after;
        let y = labels[i].y;
        let value = &labels[i].value;
        let x = if after {
            node.x + node_width + LABEL_GAP
        } else {
            node.x - LABEL_GAP
        };
        let position = format!(
            " x=\"{}\" y=\"{}\" text-anchor=\"{}\"",
            num(x),
            num(y + small * 0.35),
            if after { "start" } else { "end" }
        );
        let text = if !value.is_empty() {
            format!(
                "{} <tspan class=\"pele-sankey-value\" fill=\"var(--_m)\">{}</tspan>",
                esc(&name),
                esc(value)
            )
        } else {
            esc(&name)
        };
        labels_out.push_str(&format!("<g class=\"pele-label\" data-id=\"{}\">", esc_text(id)));
        if outlined {
            let val_part = if !value.is_empty() {
                format!(" {}", esc(value))
            } else {
                String::new()
            };
            labels_out.push_str(&format!(
                "<text{} fill=\"none\" stroke=\"var(--_bg)\" stroke-width=\"3\" stroke-linejoin=\"round\">{}{}</text>",
                position,
                esc(&name),
                val_part
            ));
        }
        labels_out.push_str(&format!("<text{}>{}</text></g>", position, text));
    }

    let fixed_link_style = if link_color == "gradient" || link_color == "source" || link_color == "target" {
        String::new()
    } else {
        resolve_style(&[format!("fill:{}", link_color)]).shape
    };
    // Without an idPrefix diagrams share gradient ids, and Chrome then uses a hidden or differently
    // themed copy's gradient. Other characters would end the url() and void the whole fill.
    let id_prefix = options.id_prefix.as_deref().unwrap_or("");
    let gradient = link_color == "gradient"
        && !id_prefix.is_empty()
        && id_prefix.chars().all(|c| is_word(c) || c == '-');
    let mut gradients: Vec<(String, String)> = Vec::new();
    let mut gradient_idx: HashMap<String, usize> = HashMap::new();
    let mut links_out = String::new();
    for link in &links {
        let end = if link_color == "target" { link.target } else { link.source };
        let half = link.thickness.max(1.0) / 2.0;
        let x0 = nodes[link.source].x + node_width;
        let x1 = nodes[link.target].x;
        let style_part = if !fixed_link_style.is_empty() {
            fixed_link_style.as_str()
        } else {
            node_style(end)
        };
        let mut paint = format!("fill=\"{}\"{}", series_color(end as i64), style_part);
        if gradient {
            // Ids come from the gradient itself, so diagrams sharing a prefix agree on them. The colour
            // after the url() is the fallback.
            let src_col = node_color(link.source);
            let tgt_col = node_color(link.target);
            let stops = format!(
                "x1=\"{}\" x2=\"{}\"><stop offset=\"0\" stop-color=\"{}\"/><stop offset=\"1\" stop-color=\"{}\"/>",
                num(x0),
                num(x1),
                src_col,
                tgt_col
            );
            let id = format!("{}-sankey-{}", id_prefix, content_id(&stops));
            let def_el = format!(
                "<linearGradient id=\"{}\" gradientUnits=\"userSpaceOnUse\" {}</linearGradient>",
                id, stops
            );
            if let Some(&pos) = gradient_idx.get(&id) {
                gradients[pos].1 = def_el;
            } else {
                gradient_idx.insert(id.clone(), gradients.len());
                gradients.push((id.clone(), def_el));
            }
            paint = format!("fill=\"url(#{}) {}\"", id, src_col);
        }
        let mid = num((x0 + x1) / 2.0);
        let from = esc_text(&model.nodes[link.source].id);
        let to = esc_text(&model.nodes[link.target].id);
        links_out.push_str(&format!(
            "<path class=\"pele-sankey-link\" data-id=\"{}-&gt;{}\" d=\"M{},{}C{},{} {},{} {},{}L{},{}C{},{} {},{} {},{}Z\" {}><title>{} → {}: {}{}{}</title></path>",
            from,
            to,
            num(x0),
            num(link.source_y - half),
            mid,
            num(link.source_y - half),
            mid,
            num(link.target_y - half),
            num(x1),
            num(link.target_y - half),
            num(x1),
            num(link.target_y + half),
            mid,
            num(link.target_y + half),
            mid,
            num(link.source_y + half),
            num(x0),
            num(link.source_y + half),
            paint,
            from,
            to,
            esc(&prefix),
            rounded_str(link.value),
            esc(&suffix),
        ));
    }

    let title = layout_label(
        model.title.as_deref(),
        false,
        measurer,
        size,
        title_room(options),
        style::BOLD,
    );
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = (max_x - min_x).max(title.width);
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();

    let defs_svg = if !gradients.is_empty() {
        let mut s = String::from("<defs>");
        for (_, def_el) in &gradients {
            s.push_str(def_el);
        }
        s.push_str("</defs>");
        s
    } else {
        String::new()
    };

    let content = format!(
        "{}<g transform=\"translate({},{})\">{}<g class=\"pele-sankey-links\" fill-opacity=\"0.4\">{}</g><g class=\"pele-sankey-nodes\">{}</g><g class=\"pele-labels\" font-size=\"{}\">{}</g></g>",
        label_svg(&title, width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\""),
        num(ox),
        num(oy),
        defs_svg,
        links_out,
        nodes_out,
        js_number(small),
        labels_out,
    );

    let fixed_max = root_config.get("sankey").get("useMaxWidth").as_bool() == Some(false);
    let responsive = options.responsive.unwrap_or(!fixed_max);
    let doc = Document {
        id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
        responsive,
        font_family: options.font_family.as_deref(),
        font_family_mono: options.font_family_mono.as_deref(),
        palette: "",
    };
    let acc = Accessible {
        acc_title: model.acc_title.as_deref(),
        acc_descr: model.acc_descr.as_deref(),
    };
    let svg = svg_document("sankey", width, height, size, &doc, &acc, &content);

    Ok(Rendered { svg, width, height, links: Vec::new() })
}
