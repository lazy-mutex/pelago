use super::db::{json_string_array, normalize_text, VennDb};
use super::layout::{find_spot, layout_circles, region_path, Circle, Spot};
use super::types::{OrderMap, VennSubset};
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::ink::{declaration, ink};
use crate::svg::palette::palette;
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{resolve_style, series_color, ResolvedStyle};
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;
use std::collections::{HashMap, HashSet};

const RADIUS: f64 = 100.0;
const MAX_GROWTH: f64 = 1.6;

struct RegionLine {
    id: Option<String>,
    raw: String,
    size: f64,
    base: u8,
    label: Label,
}

struct Region {
    sets: Vec<String>,
    inside: Vec<usize>,
    spot: Spot,
    lines: Vec<RegionLine>,
    style: ResolvedStyle,
    across: bool,
}

// One key for a list of set ids. Ids may hold any character, so they are not simply joined.
fn key_of(ids: &[String]) -> String {
    if ids.len() == 1 {
        ids[0].clone()
    } else {
        json_string_array(ids)
    }
}

pub fn render_venn(model: &VennDb, config: &Value, options: &RenderOptions) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let tiny = js_round(size * 0.8125);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let no_style = ResolvedStyle::default();

    let circles = layout_circles(&model.subsets);
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, c) in circles.iter().enumerate() {
        index.insert(c.id.clone(), i);
    }

    // Styles for the same targets add up, later declarations winning.
    let mut styles: OrderMap<String, OrderMap<String, String>> = OrderMap::new();
    for entry in &model.styles {
        let key = key_of(&entry.targets);
        if !styles.contains_key(&key) {
            styles.insert(key.clone(), OrderMap::new());
        }
        let merged = styles.get_mut(&key).unwrap();
        for (prop, val) in entry.styles.iter() {
            merged.insert(prop.clone(), val.clone());
        }
    }
    let style_of = |key: &str| -> ResolvedStyle {
        let Some(declared) = styles.get(&key.to_string()) else {
            return no_style.clone();
        };
        let list: Vec<String> = declared.iter().map(|(prop, val)| format!("{}:{}", prop, val)).collect();
        resolve_style(&list)
    };

    // Circles that touch each circle; only those can cut into a region of it.
    let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); circles.len()];
    for a in 0..circles.len() {
        for b in a + 1..circles.len() {
            if (circles[a].x - circles[b].x).hypot(circles[a].y - circles[b].y) < circles[a].r + circles[b].r {
                neighbors[a].push(b);
                neighbors[b].push(a);
            }
        }
    }

    // One region for each set and each stated overlap; a later statement about the same sets replaces an earlier one.
    let mut stated: OrderMap<String, VennSubset> = OrderMap::new();
    for subset in &model.subsets {
        stated.insert(key_of(&subset.sets), subset.clone());
    }
    let mut regions: OrderMap<String, Region> = OrderMap::new();
    for (key, subset) in stated.iter() {
        let mut seen_sets: HashSet<String> = HashSet::new();
        let mut sets: Vec<String> = Vec::new();
        for id in &subset.sets {
            if seen_sets.insert(id.clone()) {
                sets.push(id.clone());
            }
        }
        let inside: Vec<usize> = sets.iter().map(|id| index[id]).collect();
        let within: HashSet<usize> = inside.iter().copied().collect();
        let mut smallest = inside[0];
        for &i in &inside {
            if circles[i].r < circles[smallest].r {
                smallest = i;
            }
        }
        let st = style_of(key);
        let text = match &subset.label {
            Some(lbl) => Some(lbl.clone()),
            None if subset.sets.len() == 1 => Some(subset.sets[0].clone()),
            None => None,
        };
        let base = (if st.bold { style::BOLD } else { 0 }) | (if st.italic { style::ITALIC } else { 0 });
        let others: Vec<usize> = neighbors[smallest].iter().copied().filter(|i| !within.contains(i)).collect();
        let mut spot = find_spot(&circles, &inside, &others);
        let across = !others.is_empty() && spot.margin < circles[smallest].r * 0.2;
        if across {
            spot = find_spot(&circles, &inside, &[]);
        }
        let mut lines = Vec::new();
        if let Some(t) = text.filter(|s| !s.is_empty()) {
            let font_size = if sets.len() == 1 { size } else { small };
            let label = layout_label(Some(&t), false, measurer, font_size, 4000.0, base);
            lines.push(RegionLine { id: None, raw: t, size: font_size, base, label });
        }
        regions.insert(key.clone(), Region { sets, inside, spot, lines, style: st, across });
    }
    for node in &model.text_nodes {
        let raw = node.label.clone().unwrap_or_else(|| node.id.clone());
        let key = key_of(&node.sets);
        if let Some(region) = regions.get_mut(&key) {
            let label = layout_label(Some(&raw), false, measurer, tiny, 4000.0, 0);
            region.lines.push(RegionLine {
                id: Some(node.id.clone()),
                raw,
                size: tiny,
                base: 0,
                label,
            });
        }
    }

    let mut largest = 0.0;
    for c in &circles {
        if c.r > largest {
            largest = c.r;
        }
    }
    let mut scale = if largest > 0.0 { RADIUS / largest } else { 1.0 };
    let mut growth = 1.0f64;
    for region in regions.values() {
        if region.spot.margin <= 0.0 {
            continue;
        }
        let mut width = 0.0f64;
        let mut height = 0.0f64;
        for line in &region.lines {
            width = width.max(line.label.width);
            height += line.label.height;
        }
        let need = width.hypot(height) / 2.0 + 4.0;
        growth = growth.max((need / (region.spot.margin * scale)).min(MAX_GROWTH));
    }
    scale *= growth;

    for region in regions.values_mut() {
        let room = (region.spot.margin * scale * 1.8).max(72.0);
        for line in &mut region.lines {
            if line.label.width > room {
                line.label = layout_label(Some(&line.raw), false, measurer, line.size, room, line.base);
            }
        }
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for c in &circles {
        min_x = min_x.min((c.x - c.r) * scale);
        min_y = min_y.min((c.y - c.r) * scale);
        max_x = max_x.max((c.x + c.r) * scale);
        max_y = max_y.max((c.y + c.r) * scale);
    }
    for region in regions.values() {
        let mut wide = 0.0f64;
        let mut tall = 0.0f64;
        for line in &region.lines {
            wide = wide.max(line.label.width);
            tall += line.label.height;
        }
        min_x = min_x.min(region.spot.x * scale - wide / 2.0);
        max_x = max_x.max(region.spot.x * scale + wide / 2.0);
        min_y = min_y.min(region.spot.y * scale - tall / 2.0);
        max_y = max_y.max(region.spot.y * scale + tall / 2.0);
    }
    if circles.is_empty() {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 0.0;
        max_y = 0.0;
    }

    let norm_title = model.title.as_deref().filter(|t| !t.is_empty()).map(normalize_text);
    let title = layout_label(norm_title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = (max_x - min_x).max(title.width);
    let ox = pad + (inner - (max_x - min_x)) / 2.0 - min_x;
    let oy = pad + title_height - min_y;
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let placed: Vec<Circle> = circles
        .iter()
        .map(|c| Circle { id: c.id.clone(), x: c.x * scale + ox, y: c.y * scale + oy, r: c.r * scale })
        .collect();

    let mut sets_svg = String::new();
    for (i, c) in placed.iter().enumerate() {
        let color = series_color(i as i64);
        let shape_attr = regions.get(&c.id).map_or("", |r| r.style.shape.as_str());
        sets_svg.push_str(&format!(
            "<circle class=\"pele-venn-set\" data-id=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" fill-opacity=\"0.2\" stroke=\"{}\"{}/>",
            esc_text(&c.id),
            num(c.x),
            num(c.y),
            num(c.r),
            color,
            color,
            shape_attr
        ));
    }

    let painted: Vec<&Region> = regions.values().filter(|r| r.sets.len() > 1 && !r.style.shape.is_empty()).collect();
    let under = |region: &Region| -> String {
        if region.across {
            return String::new();
        }
        let mut fill = if region.sets.len() == 1 && declaration(&region.style.shape, "fill-opacity").is_some() {
            region.style.shape.clone()
        } else {
            String::new()
        };
        for other in &painted {
            if other.sets.iter().all(|id| region.sets.contains(id)) {
                fill = other.style.shape.clone();
            }
        }
        fill
    };

    let mut overlaps = String::new();
    let mut labels = String::new();
    for region in regions.values() {
        let id = esc_text(&region.sets.join("|"));
        if region.sets.len() > 1 && !region.style.shape.is_empty() {
            let sub_circles: Vec<Circle> = region.inside.iter().map(|&i| placed[i].clone()).collect();
            let d = region_path(&sub_circles, num);
            if !d.is_empty() {
                overlaps.push_str(&format!(
                    "<path class=\"pele-venn-overlap\" data-id=\"{}\" d=\"{}\" fill=\"none\"{}/>",
                    id, d, region.style.shape
                ));
            }
        }
        let total: f64 = region.lines.iter().map(|l| l.label.height).sum();
        if total == 0.0 {
            continue;
        }
        let x = region.spot.x * scale + ox;
        let mut y = region.spot.y * scale + oy - total / 2.0;
        let fill = under(region);
        let mut region_content = String::new();
        for line in &region.lines {
            let own_style;
            let own = match &line.id {
                None => region.style.text.as_str(),
                Some(line_id) => {
                    own_style = style_of(line_id);
                    own_style.text.as_str()
                }
            };
            let color = ink(&fill, own);
            let attrs = match &line.id {
                None => format!(
                    " class=\"pele-label\"{}{}{}",
                    if line.size == size { String::new() } else { format!(" font-size=\"{}\"", js_number(line.size)) },
                    color.map_or_else(String::new, |c| format!(" fill=\"{}\"", c)),
                    own
                ),
                Some(line_id) => format!(
                    " class=\"pele-venn-text\" data-id=\"{}\" font-size=\"{}\" fill=\"{}\"{}",
                    esc_text(line_id),
                    js_number(tiny),
                    color.unwrap_or("var(--_m)"),
                    own
                ),
            };
            region_content.push_str(&label_svg(&line.label, x, y + line.label.height / 2.0, &attrs));
            y += line.label.height;
        }
        labels.push_str(&format!("<g class=\"pele-venn-region\" data-id=\"{}\">{}</g>", id, region_content));
    }

    let mut content = String::new();
    content.push_str(&label_svg(&title, width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\""));
    content.push_str(&format!("<g class=\"pele-venn-sets\">{}</g>", sets_svg));
    if !overlaps.is_empty() {
        content.push_str(&format!("<g class=\"pele-venn-overlaps\">{}</g>", overlaps));
    }
    if !labels.is_empty() {
        content.push_str(&format!("<g class=\"pele-venn-labels\">{}</g>", labels));
    }

    let pal = palette("venn", config);
    let fixed = config.get("venn").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "venn",
        width,
        height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive: options.responsive.unwrap_or(!fixed),
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: &pal.style,
        },
        &Accessible::default(),
        &content,
    );
    Rendered { svg, width, height, links: Vec::new() }
}
