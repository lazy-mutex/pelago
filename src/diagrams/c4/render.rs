use std::collections::{HashMap, HashSet};

use crate::diagrams::common::literal_label::left_label_svg;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::options::{title_room, LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::edges::{casings_svg, edge_casing, marker, marker_trim, over_fills, EdgePath, Fill, ARROW};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::shapes::draw_shape;
use crate::svg::theme::{resolve_style, rounded, ResolvedStyle};
use crate::text::label::layout_label;
use crate::text::measurer::{style, MetricsMeasurer};
use crate::url::{linked, sanitize_url};
use crate::value::Value;

use super::db::C4Db;
use super::layout::{layout_c4, HEAD_LEFT, HEAD_TOP};
use super::route::{crosses, cyl_ry, queue_rx, route, BoxRect, Lanes, HEAD, LABEL_PAD, LANE};

const PAIR_SHIFT: f64 = 7.0;
const NARROW_GAP: f64 = 36.0;
const WIDEN: f64 = 24.0;
const MAX_OFFSET: f64 = 2000.0;
const EDGE: f64 = 8.0;
const SLIDE: [f64; 7] = [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8];
const CARE_BUDGET: usize = 2_000_000;

fn user_style(fill: Option<&str>, stroke: Option<&str>, color: Option<&str>) -> ResolvedStyle {
    let mut declarations = Vec::new();
    if let Some(f) = fill.filter(|s| !s.is_empty()) {
        declarations.push(format!("fill:{}", f));
    }
    if let Some(s) = stroke.filter(|s| !s.is_empty()) {
        declarations.push(format!("stroke:{}", s));
    }
    if let Some(c) = color.filter(|s| !s.is_empty()) {
        declarations.push(format!("color:{}", c));
    }
    resolve_style(&declarations)
}

fn link(
    body: String,
    url: Option<&str>,
    id: &str,
    links: &mut Vec<LinkInfo>,
    options: &RenderOptions,
) -> String {
    let Some(url) = url.filter(|u| !u.is_empty()) else {
        return body;
    };
    let restored = if url.contains("\u{00b6}\u{00df}") {
        url.replace("\u{fb02}\u{00b0}\u{00b0}", "#")
            .replace("\u{fb02}\u{00b0}", "#")
            .replace("\u{00b6}\u{00df}", ";")
    } else {
        url.to_string()
    };
    linked(body, &sanitize_url(&restored), id, links, options, "")
}

fn clamp_offset(value: Option<f64>) -> f64 {
    match value {
        Some(v) if v != 0.0 && !v.is_nan() => v.clamp(-MAX_OFFSET, MAX_OFFSET),
        _ => 0.0,
    }
}

pub fn render_c4(db: &C4Db, config: &Value, options: &RenderOptions) -> Result<Rendered> {
    let draw_mode = |tight: bool| -> Result<Rendered> {
        if !tight {
            return Ok(draw(db, config, options, false, NARROW_GAP)?.0);
        }
        let (mut best, mut squeezed) = draw(db, config, options, true, NARROW_GAP)?;
        let mut gap = NARROW_GAP + WIDEN;
        while squeezed > 0 && gap <= NARROW_GAP + 3.0 * WIDEN {
            let (wider, left) = draw(db, config, options, true, gap)?;
            if left < squeezed {
                best = wider;
                squeezed = left;
            }
            gap += WIDEN;
        }
        Ok(best)
    };

    let loose = draw_mode(false)?;
    let Some(room) = options.max_width.filter(|&w| w > 0.0) else {
        return Ok(loose);
    };
    if loose.width <= room {
        return Ok(loose);
    }
    let tight = draw_mode(true)?;
    Ok(if tight.width < loose.width { tight } else { loose })
}

fn draw(
    db: &C4Db,
    config: &Value,
    options: &RenderOptions,
    tight: bool,
    gap: f64,
) -> Result<(Rendered, usize)> {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.8125);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let mut links: Vec<LinkInfo> = Vec::new();
    let room = tight.then(|| ((options.max_width.unwrap_or(320.0)) - 2.0 * pad).max(80.0));
    let mut layout = layout_c4(db, measurer, size, small, room, gap);
    let root_w = layout.groups[0].rect.w;
    let root_h = layout.groups[0].rect.h;

    let mut min_x = 0.0f64;
    let mut min_y = 0.0f64;
    let mut max_x = root_w;
    let mut max_y = root_h;
    let mut grow = |x0: f64, y0: f64, x1: f64, y1: f64| {
        min_x = min_x.min(x0);
        min_y = min_y.min(y0);
        max_x = max_x.max(x1);
        max_y = max_y.max(y1);
    };

    let mut ends: Vec<BoxRect> = Vec::with_capacity(db.rels.len() * 2);
    let mut pairs: HashMap<usize, HashSet<usize>> = HashMap::new();
    for rel in &db.rels {
        let from = if rel.from_attr.is_some() {
            None
        } else {
            db.get_c4_shape(&rel.from)
        };
        let to = if rel.to_attr.is_some() {
            None
        } else {
            db.get_c4_shape(&rel.to)
        };
        let a = from.and_then(|id| layout.boxes.get(&id)).cloned();
        let b = to.and_then(|id| layout.boxes.get(&id)).cloned();
        let (Some(a), Some(b)) = (a, b) else {
            return Err(PeleError::new(
                format!(
                    "C4 rel \"{}\" -> \"{}\" references an unknown shape or boundary",
                    rel.from, rel.to
                ),
                ErrorCode::Semantic,
            )
            .of("c4"));
        };
        pairs.entry(a.id).or_default().insert(b.id);
        ends.push(a);
        ends.push(b);
    }

    let mut nodes: Vec<BoxRect> = Vec::new();
    let mut taken: Vec<f64> = Vec::new();
    for &g_idx in &layout.order {
        for node in &layout.groups[g_idx].nodes {
            nodes.push(node.rect.clone());
            taken.extend_from_slice(&[
                node.rect.x + EDGE,
                node.rect.y + EDGE,
                node.rect.x + node.rect.w - EDGE,
                node.rect.y + node.rect.h - EDGE,
            ]);
        }
    }

    let careful = db.rels.len() * (nodes.len() + layout.order.len() + 3 * db.rels.len()) <= CARE_BUDGET;
    let covered = |taken: &[f64], x0: f64, y0: f64, x1: f64, y1: f64| -> f64 {
        let mut area = 0.0f64;
        let mut i = 0;
        while i < taken.len() {
            let w = x1.min(taken[i + 2]) - x0.max(taken[i]);
            if w > 0.0 {
                let h = y1.min(taken[i + 3]) - y0.max(taken[i + 1]);
                if h > 0.0 {
                    area += w * h;
                }
            }
            i += 4;
        }
        area
    };

    let mut casings = String::new();
    let mut edges_out = String::new();
    let trim = marker_trim("arrow_point");
    let mut fills: Vec<Fill> = Vec::new();
    for &g_idx in &layout.order {
        if g_idx == 0 {
            continue;
        }
        let b = &layout.groups[g_idx].rect;
        let shape = user_style(db.boundaries[g_idx].elem.bg_color.as_deref(), None, None).shape;
        if paints(&shape) {
            fills.push(Fill {
                bounds: [b.x, b.y, b.x + b.w, b.y + b.h],
                shape,
            });
        }
    }

    let mut lanes = Lanes::default();
    for &g_idx in &layout.order {
        if g_idx != 0 {
            lanes.frame(&layout.groups[g_idx].rect);
        }
    }

    let mut routes = Vec::with_capacity(db.rels.len());
    for (index, rel) in db.rels.iter().enumerate() {
        let a = &ends[index * 2];
        let b = &ends[index * 2 + 1];
        let st = user_style(None, rel.line_color.as_deref(), None);
        let start_arrow = rel.rel_type == "birel" || rel.rel_type == "rel_b";
        let end_arrow = rel.rel_type != "rel_b";
        let reverse = a.id != b.id && pairs.get(&b.id).is_some_and(|s| s.contains(&a.id));
        let r = route(
            a,
            b,
            if reverse { PAIR_SHIFT } else { 0.0 },
            if careful { Some(&nodes) } else { None },
            &mut lanes,
            if start_arrow { trim } else { 0.0 },
            if end_arrow { trim } else { 0.0 },
        );
        grow(r.x0.min(r.x1), r.y0.min(r.y1), r.x0.max(r.x1), r.y0.max(r.y1));
        if start_arrow && careful {
            taken.extend_from_slice(&[r.sx - ARROW, r.sy - ARROW, r.sx + ARROW, r.sy + ARROW]);
        }
        if end_arrow && careful {
            taken.extend_from_slice(&[r.ex - ARROW, r.ey - ARROW, r.ex + ARROW, r.ey + ARROW]);
        }
        let over = over_fills(&r.d, &[r.sx, r.sy, r.ex, r.ey], &fills, &st.line, None);
        let fallback_color = match st.stroke.as_deref().filter(|s| !s.is_empty()) {
            Some(s) => esc(s),
            None => "var(--_l)".to_string(),
        };
        let color = match over {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => fallback_color,
        };
        if over == Some("") {
            let ep = EdgePath {
                d: r.d.clone(),
                tokens: None,
                sx: r.sx,
                sy: r.sy,
                sdx: r.sdx,
                sdy: r.sdy,
                ex: r.ex,
                ey: r.ey,
                edx: r.edx,
                edy: r.edy,
            };
            casings.push_str(&edge_casing(
                &ep,
                if start_arrow { "arrow_point" } else { "none" },
                if end_arrow { "arrow_point" } else { "none" },
                &st.line,
                false,
            ));
        }
        let stroke_attr = match over {
            Some(s) if !s.is_empty() => format!(" stroke=\"{}\"", s),
            _ => String::new(),
        };
        let edge_id = format!("{}-{}", rel.from, rel.to);
        edges_out.push_str(&format!(
            "<g class=\"pele-edge\" data-id=\"{}\"{}{}><path d=\"{}\"/>{}{}</g>",
            esc_text(&edge_id),
            stroke_attr,
            st.line,
            r.d,
            if start_arrow {
                marker("arrow_point", r.sx, r.sy, r.sdx, r.sdy, &color, "none")
            } else {
                String::new()
            },
            if end_arrow {
                marker("arrow_point", r.ex, r.ey, r.edx, r.edy, &color, "none")
            } else {
                String::new()
            },
        ));
        routes.push(r);
    }

    for &g_idx in &layout.order {
        let group = &mut layout.groups[g_idx];
        if !careful || group.head_w == 0.0 {
            continue;
        }
        let top = group.rect.y + HEAD_TOP;
        let bottom = group.rect.y + group.head - LANE;
        let mut fewest = usize::MAX;
        let candidates = [
            HEAD_LEFT,
            (group.rect.w - group.head_w) / 2.0,
            group.rect.w - HEAD_LEFT - group.head_w,
        ];
        for x in candidates {
            let mut count = 0usize;
            for r in &routes {
                if crosses(
                    &r.points,
                    group.rect.x + x - 4.0,
                    top,
                    group.rect.x + x + group.head_w + 4.0,
                    bottom,
                ) {
                    count += 1;
                }
            }
            if count < fewest {
                fewest = count;
                group.head_x = x;
            }
            if count == 0 {
                break;
            }
        }
        taken.extend_from_slice(&[
            group.rect.x + group.head_x - 4.0,
            top,
            group.rect.x + group.head_x + group.head_w + 4.0,
            bottom,
        ]);
    }

    let mut labels_out = String::new();
    for (index, rel) in db.rels.iter().enumerate() {
        let r = &routes[index];
        let id = format!("{}-{}", rel.from, rel.to);
        let st = user_style(None, None, rel.text_color.as_deref());
        let text = if db.c4_type.as_deref() == Some("C4Dynamic") {
            format!("{}: {}", index + 1, rel.label)
        } else {
            rel.label.clone()
        };
        let label = layout_label(Some(&text), false, measurer, small, r.room, 0);
        let techn_str = rel
            .techn
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| format!("[{}]", s));
        let techn = layout_label(techn_str.as_deref(), false, measurer, small, r.room, style::ITALIC);
        let lw = label.width.max(techn.width) + LABEL_PAD;
        let lh = label.height + techn.height;
        if lh == 0.0 {
            continue;
        }

        let beside = r.nx.abs() * (lw / 2.0) + r.ny.abs() * (lh / 2.0) + 2.0;
        let aside = if r.lean != 0.0 {
            (beside - 14.0).min(r.lean).max(0.0)
        } else {
            beside
        };
        let mut dx = r.nx * aside + if tight { 0.0 } else { clamp_offset(rel.offset_x) };
        let mut dy = r.ny * aside + if tight { 0.0 } else { clamp_offset(rel.offset_y) };
        let mut at = 0.5f64;
        if careful {
            let mut least = f64::INFINITY;
            let moved = dx != r.nx * aside || dy != r.ny * aside;
            let passes = if moved { 2 } else { 1 };
            for pass in 0..passes {
                if least <= 0.0 {
                    break;
                }
                let px = if pass != 0 { r.nx * aside } else { dx };
                let py = if pass != 0 { r.ny * aside } else { dy };
                for t in SLIDE {
                    let cx = r.x0 + (r.x1 - r.x0) * t + px;
                    let cy = r.y0 + (r.y1 - r.y0) * t + py;
                    let area = covered(&taken, cx - lw / 2.0, cy - lh / 2.0, cx + lw / 2.0, cy + lh / 2.0);
                    if area < least {
                        least = area;
                        at = t;
                        dx = px;
                        dy = py;
                    }
                    if area == 0.0 {
                        break;
                    }
                }
            }
        }
        let cx = r.x0 + (r.x1 - r.x0) * at + dx;
        let top = r.y0 + (r.y1 - r.y0) * at + dy - lh / 2.0;
        if careful {
            taken.extend_from_slice(&[cx - lw / 2.0, top, cx + lw / 2.0, top + lh]);
        }
        grow(cx - lw / 2.0, top, cx + lw / 2.0, top + lh);
        let descr_tag = match rel.descr.as_deref().filter(|s| !s.is_empty()) {
            Some(d) => format!("<title>{}</title>", esc_text(d)),
            None => String::new(),
        };
        let inner_lbl = format!(
            "{}{}",
            label_svg(&label, cx, top + label.height / 2.0, &st.text),
            label_svg(
                &techn,
                cx,
                top + label.height + techn.height / 2.0,
                &format!(" class=\"pele-c4-techn\" fill=\"var(--_m)\"{}", st.text),
            ),
        );
        labels_out.push_str(&format!(
            "<g class=\"pele-edge-label\" data-id=\"{}\">{}<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" fill=\"var(--_bg)\"/>{}</g>",
            esc_text(&id),
            descr_tag,
            num(cx - lw / 2.0),
            num(top),
            num(lw),
            num(lh),
            link(inner_lbl, rel.link.as_deref(), &id, &mut links, options),
        ));
    }

    let mut clusters = String::new();
    let mut nodes_out = String::new();
    for &g_idx in &layout.order {
        let group = &layout.groups[g_idx];
        if g_idx != 0 {
            let boundary = &db.boundaries[g_idx];
            let st = user_style(
                boundary.elem.bg_color.as_deref(),
                boundary.elem.border_color.as_deref(),
                boundary.elem.font_color.as_deref(),
            );
            let x = group.rect.x + group.head_x;
            let mut y = group.rect.y + HEAD_TOP;
            let color = format!("{}{}", ink_fill(&st.shape, &st.text, None), st.text);
            let muted = format!("{}{}", ink_fill(&st.shape, &st.text, Some("var(--_m)")), st.text);
            let mut text = left_label_svg(
                &group.labels[0],
                x,
                y,
                &format!(" class=\"pele-cluster-label\" font-weight=\"var(--_hw)\"{}", color),
            );
            y += group.labels[0].height;
            text.push_str(&left_label_svg(
                &group.labels[1],
                x,
                y,
                &format!(" class=\"pele-c4-type\" font-size=\"{}\"{}", js_number(small), muted),
            ));
            y += group.labels[1].height;
            text.push_str(&left_label_svg(
                &group.labels[2],
                x,
                y,
                &format!(" class=\"pele-c4-descr\" font-size=\"{}\"{}", js_number(small), muted),
            ));
            let is_node = boundary.node_type.as_deref().is_some_and(|s| !s.is_empty());
            clusters.push_str(&format!(
                "<g class=\"pele-cluster pele-c4-{}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"var(--_b)\"{}{}/>{}</g>",
                if is_node { "node" } else { "boundary" },
                esc_text(&boundary.elem.alias),
                num(group.rect.x),
                num(group.rect.y),
                num(group.rect.w),
                num(group.rect.h),
                if is_node { "" } else { " stroke-dasharray=\"6 4\"" },
                rounded(&st.shape),
                link(
                    text,
                    boundary.elem.link.as_deref(),
                    &boundary.elem.alias,
                    &mut links,
                    options,
                ),
            ));
        }

        for node in &group.nodes {
            let shape = &db.shapes[node.shape_idx];
            let external = shape.kind.starts_with("external");
            let st = user_style(
                shape.elem.bg_color.as_deref(),
                shape.elem.border_color.as_deref(),
                shape.elem.font_color.as_deref(),
            );
            let stroke = format!(
                " stroke=\"var(--_b)\"{}",
                if external { " stroke-dasharray=\"4 3\"" } else { "" }
            );
            let (w, h, form) = (node.rect.w, node.rect.h, node.rect.form.as_str());
            let dx = if form == "h-cyl" { -queue_rx(h) / 2.0 } else { 0.0 };
            let dy = if form == "person" {
                HEAD / 2.0
            } else if form == "cyl" {
                cyl_ry(w) / 2.0
            } else {
                0.0
            };
            let color = format!("{}{}", ink_fill(&st.shape, &st.text, None), st.text);
            let muted = format!("{}{}", ink_fill(&st.shape, &st.text, Some("var(--_m)")), st.text);
            let mut y = dy - node.text_h / 2.0;
            let mut text = label_svg(
                &node.labels[0],
                dx,
                y + node.labels[0].height / 2.0,
                &format!(
                    " class=\"pele-c4-type\" font-size=\"{}\"{}",
                    js_number(small - 1.0),
                    muted
                ),
            );
            y += node.labels[0].height;
            text.push_str(&label_svg(
                &node.labels[1],
                dx,
                y + node.labels[1].height / 2.0,
                &format!(" class=\"pele-label\" font-weight=\"var(--_hw)\"{}", color),
            ));
            y += node.labels[1].height;
            text.push_str(&label_svg(
                &node.labels[2],
                dx,
                y + node.labels[2].height / 2.0,
                &format!(" class=\"pele-c4-techn\" font-size=\"{}\"{}", js_number(small), muted),
            ));
            y += node.labels[2].height + 4.0;
            text.push_str(&label_svg(
                &node.labels[3],
                dx,
                y + node.labels[3].height / 2.0,
                &format!(" class=\"pele-c4-descr\" font-size=\"{}\"{}", js_number(small), color),
            ));

            let shape_svg = draw_shape(
                form,
                w,
                h,
                &format!(
                    " fill=\"var({})\"{}{}",
                    if external { "--_bg" } else { "--_s" },
                    stroke,
                    st.shape
                ),
                &format!("{}{}", stroke, st.line),
            );
            nodes_out.push_str(&format!(
                "<g class=\"pele-node pele-shape-{} pele-c4-{}{}\" data-id=\"{}\" transform=\"translate({},{})\">{}</g>",
                form,
                shape.kind,
                if external { " pele-c4-external" } else { "" },
                esc_text(&shape.elem.alias),
                num(node.rect.x + w / 2.0),
                num(node.rect.y + h / 2.0),
                link(
                    format!("{}{}", shape_svg, text),
                    shape.elem.link.as_deref(),
                    &shape.elem.alias,
                    &mut links,
                    options,
                ),
            ));
        }
    }

    let title_max = if tight { title_room(options) } else { 4000.0 };
    let title = layout_label(db.title.as_deref(), false, measurer, size, title_max, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let inner = (max_x - min_x).max(title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let ox = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let oy = pad + title_height - min_y;

    let fixed = config.get("c4").get("useMaxWidth").as_bool() == Some(false);
    let doc = Document {
        id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
        responsive: options.responsive.unwrap_or(!fixed),
        font_family: options.font_family.as_deref(),
        font_family_mono: options.font_family_mono.as_deref(),
        palette: "",
    };
    let acc = Accessible {
        acc_title: db.acc_title.as_deref(),
        acc_descr: db.acc_descr.as_deref(),
    };
    let content = format!(
        "{}<g transform=\"translate({},{})\">{}{}{}{}{}</g>",
        label_svg(
            &title,
            width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        num(ox),
        num(oy),
        if clusters.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-clusters\">{}</g>", clusters)
        },
        casings_svg(&casings),
        if edges_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{}</g>",
                edges_out
            )
        },
        if nodes_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-nodes\">{}</g>", nodes_out)
        },
        if labels_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>",
                js_number(small),
                labels_out
            )
        },
    );
    let svg = svg_document("c4", width, height, size, &doc, &acc, &content);
    Ok((Rendered { svg, width, height, links }, lanes.squeezed))
}
