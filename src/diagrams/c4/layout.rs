use std::collections::HashMap;

use crate::svg::edges::ARROW;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, TextMeasurer};

use super::db::{C4Db, C4Shape, ElementId};
use super::route::{BoxRect, HEAD, LABEL_PAD, LANE, NARROW};

const SHAPE_W: f64 = 216.0;
const SHAPE_MIN_H: f64 = 64.0;
const PAD_X: f64 = 14.0;
const PAD_Y: f64 = 12.0;
const GAP_X: f64 = NARROW + LABEL_PAD + 3.0 * ARROW;
const GAP_Y: f64 = 88.0;
pub const GROUP_PAD: f64 = LANE + 8.0;
const GROUP_GAP: f64 = 48.0;
pub const HEAD_LEFT: f64 = 16.0;
pub const HEAD_TOP: f64 = 10.0;

fn lookup_form(key: &str) -> Option<&'static str> {
    Some(match key {
        "person" => "person",
        "box" | "rounded" | "component" => "rect",
        "cylinder" | "database" | "db" => "cyl",
        "queue" | "pipe" => "h-cyl",
        _ => return None,
    })
}

fn form_of(shape: &C4Shape) -> &'static str {
    if let Some(s) = shape.elem.shape.as_deref() {
        if let Some(f) = lookup_form(&s.to_lowercase()) {
            return f;
        }
    }
    if let Some(s) = shape.elem.sprite.as_deref() {
        if let Some(f) = lookup_form(&s.to_lowercase()) {
            return f;
        }
    }
    if let Some(tags) = shape.elem.tags.as_deref() {
        for tag in tags.split(',') {
            if let Some(f) = lookup_form(&tag.trim().to_lowercase()) {
                return f;
            }
        }
    }
    let kind = shape.kind.as_str();
    if kind.ends_with("person") {
        "person"
    } else if kind.ends_with("_db") {
        "cyl"
    } else if kind.ends_with("_queue") {
        "h-cyl"
    } else {
        "rect"
    }
}

pub struct Node {
    pub shape_idx: usize,
    pub rect: BoxRect,
    pub labels: [Label; 4],
    pub text_h: f64,
}

pub struct Group {
    pub boundary_idx: usize,
    pub rect: BoxRect,
    pub kids: Vec<usize>,
    pub nodes: Vec<Node>,
    pub labels: Vec<Label>,
    pub head: f64,
    pub head_w: f64,
    pub head_x: f64,
}

pub struct Layout {
    // Indices into `groups`, parents before children. `order[0]` is `0` (the top-level `global` boundary).
    pub order: Vec<usize>,
    pub groups: Vec<Group>,
    pub boxes: HashMap<ElementId, BoxRect>,
}

fn measure(
    shape_idx: usize,
    box_id: usize,
    shape: &C4Shape,
    measurer: &dyn TextMeasurer,
    size: f64,
    small: f64,
    width: f64,
) -> Node {
    let form = form_of(shape);
    let cap_w = if form == "h-cyl" { 36.0 } else { 0.0 };
    let cap_h = if form == "person" {
        HEAD
    } else if form == "cyl" {
        24.0
    } else {
        0.0
    };
    let inner = width - 2.0 * PAD_X - cap_w;
    let stereotype = format!("«{}»", shape.kind.replace('_', " "));
    let techn_str = shape
        .elem
        .techn
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("[{}]", s));
    let descr_opt = shape.elem.descr.as_deref().filter(|s| !s.is_empty());
    let labels = [
        layout_label(Some(&stereotype), false, measurer, small - 1.0, inner, 0),
        layout_label(Some(&shape.elem.label), false, measurer, size, inner, style::BOLD),
        layout_label(techn_str.as_deref(), false, measurer, small, inner, 0),
        layout_label(descr_opt, false, measurer, small, inner, 0),
    ];
    let mut tw = 0.0f64;
    let mut text_h = if labels[3].height > 0.0 { 4.0 } else { 0.0 };
    for lbl in &labels {
        tw = tw.max(lbl.width);
        text_h += lbl.height;
    }
    Node {
        shape_idx,
        rect: BoxRect {
            id: box_id,
            x: 0.0,
            y: 0.0,
            w: width.max(tw + 2.0 * PAD_X + cap_w),
            h: SHAPE_MIN_H.max(text_h + 2.0 * PAD_Y + cap_h),
            form: form.to_string(),
        },
        labels,
        text_h,
    }
}

fn rows_rects(rects: &mut [&mut BoxRect], per_row: usize, gap_x: f64, gap_y: f64, stretch: bool) -> (f64, f64) {
    let mut width = 0.0f64;
    let mut y = 0.0f64;
    let len = rects.len();
    let mut from = 0;
    while from < len {
        let to = len.min(from + per_row);
        let mut x = 0.0f64;
        let mut row_h = 0.0f64;
        for item in &rects[from..to] {
            row_h = row_h.max(item.h);
        }
        for item in &mut rects[from..to] {
            item.x = x;
            item.y = y;
            if stretch {
                item.h = row_h;
            }
            x += item.w + gap_x;
        }
        width = width.max(x - gap_x);
        y += row_h + gap_y;
        from += per_row;
    }
    (width, if len > 0 { y - gap_y } else { 0.0 })
}

pub fn layout_c4(
    db: &C4Db,
    measurer: &dyn TextMeasurer,
    size: f64,
    small: f64,
    room: Option<f64>,
    gap: f64,
) -> Layout {
    let group_pad = if room.is_none() { GROUP_PAD } else { 12.0 };
    let mut groups: Vec<Group> = Vec::with_capacity(db.boundaries.len());
    for (b_idx, boundary) in db.boundaries.iter().enumerate() {
        let label_room = match room {
            None => 4000.0,
            Some(r) => (r - 2.0 * HEAD_LEFT - 2.0 * GROUP_PAD * 2.0).max(80.0),
        };
        let labels = if boundary.elem.parent.is_some() {
            let type_str = boundary
                .elem
                .elem_type
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| format!("[{}]", s));
            let descr_opt = boundary.elem.descr.as_deref().filter(|s| !s.is_empty());
            vec![
                layout_label(Some(&boundary.elem.label), false, measurer, size, label_room, style::BOLD),
                layout_label(type_str.as_deref(), false, measurer, small, label_room, 0),
                layout_label(descr_opt, false, measurer, small, label_room, 0),
            ]
        } else {
            Vec::new()
        };
        let mut head = 0.0f64;
        let mut head_w = 0.0f64;
        for lbl in &labels {
            head += lbl.height;
            head_w = head_w.max(lbl.width);
        }
        groups.push(Group {
            boundary_idx: b_idx,
            rect: BoxRect {
                id: b_idx,
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
                form: "rect".to_string(),
            },
            kids: Vec::new(),
            nodes: Vec::new(),
            labels,
            head: if head > 0.0 { head + HEAD_TOP + 6.0 + LANE } else { group_pad },
            head_w,
            head_x: HEAD_LEFT,
        });
    }

    for b_idx in 0..db.boundaries.len() {
        if let Some(parent_idx) = db.boundaries[b_idx].elem.parent {
            if parent_idx < groups.len() {
                groups[parent_idx].kids.push(b_idx);
            }
        }
    }

    // A boundary that ends up inside itself is not reachable from the top, as in Mermaid.
    let mut order: Vec<usize> = vec![0];
    let mut depths: HashMap<usize, usize> = HashMap::new();
    depths.insert(0, 0);
    let mut idx = 0;
    while idx < order.len() {
        let g_idx = order[idx];
        let d = depths[&g_idx];
        let kids = groups[g_idx].kids.clone();
        for kid in kids {
            depths.insert(kid, d + 1);
            order.push(kid);
        }
        idx += 1;
    }

    let base_shape_id = db.boundaries.len();
    for (s_idx, shape) in db.shapes.iter().enumerate() {
        if let Some(parent_idx) = shape.elem.parent {
            let depth = depths.get(&parent_idx).copied().unwrap_or(0) as f64;
            let node_width = match room {
                None => SHAPE_W,
                Some(r) => SHAPE_W.min(r - 2.0 * depth * group_pad - 32.0).max(100.0),
            };
            if parent_idx < groups.len() {
                groups[parent_idx].nodes.push(measure(
                    s_idx,
                    base_shape_id + s_idx,
                    shape,
                    measurer,
                    size,
                    small,
                    node_width,
                ));
            }
        }
    }

    let per_row = db.shape_in_row.max(1);
    let groups_per_row = db.boundary_in_row.max(1);
    let mut kids_top: HashMap<usize, f64> = HashMap::new();

    for &g_idx in order.iter().rev() {
        let gap_x = match room {
            None => GAP_X,
            Some(_) => gap,
        };
        let node_count = match room {
            None => per_row,
            Some(r) => {
                let mut widest = 1.0f64;
                for n in &groups[g_idx].nodes {
                    widest = widest.max(n.rect.w);
                }
                let fit = ((r + gap_x - 2.0 * group_pad) / (widest + gap_x)).floor();
                if fit < 1.0 {
                    1
                } else {
                    per_row.min(fit as usize).max(1)
                }
            }
        };
        let (grid_w, grid_h) = {
            let mut node_rects: Vec<&mut BoxRect> =
                groups[g_idx].nodes.iter_mut().map(|n| &mut n.rect).collect();
            rows_rects(&mut node_rects, node_count, gap_x, GAP_Y, true)
        };

        let kids_list = groups[g_idx].kids.clone();
        let kid_count = match room {
            None => groups_per_row,
            Some(r) => {
                let mut widest = 1.0f64;
                for &k_idx in &kids_list {
                    widest = widest.max(groups[k_idx].rect.w);
                }
                let fit = ((r + GROUP_GAP - 2.0 * group_pad) / (widest + GROUP_GAP)).floor();
                if fit < 1.0 {
                    1
                } else {
                    groups_per_row.min(fit as usize).max(1)
                }
            }
        };
        let mut kid_rects_owned: Vec<BoxRect> =
            kids_list.iter().map(|&k_idx| groups[k_idx].rect.clone()).collect();
        let (kids_w, kids_h) = {
            let mut refs: Vec<&mut BoxRect> = kid_rects_owned.iter_mut().collect();
            rows_rects(&mut refs, kid_count, GROUP_GAP, GROUP_GAP, false)
        };
        for (&k_idx, updated) in kids_list.iter().zip(kid_rects_owned) {
            groups[k_idx].rect = updated;
        }

        let between = if grid_h > 0.0 && kids_h > 0.0 { GROUP_GAP } else { 0.0 };
        let frame = if g_idx == 0 { 0.0 } else { group_pad };
        kids_top.insert(g_idx, grid_h + between);
        let head_w = groups[g_idx].head_w;
        let head_space = if g_idx == 0 { 0.0 } else { groups[g_idx].head };
        groups[g_idx].rect.w = grid_w
            .max(kids_w)
            .max(head_w + 2.0 * HEAD_LEFT - 2.0 * frame)
            + 2.0 * frame;
        groups[g_idx].rect.h = head_space + grid_h + between + kids_h + frame;
    }

    let mut boxes: HashMap<ElementId, BoxRect> = HashMap::new();
    for &g_idx in &order {
        let left = groups[g_idx].rect.x + if g_idx == 0 { 0.0 } else { group_pad };
        let top = groups[g_idx].rect.y + if g_idx == 0 { 0.0 } else { groups[g_idx].head };
        for node in &mut groups[g_idx].nodes {
            node.rect.x += left;
            node.rect.y += top;
            boxes.insert(ElementId::Shape(node.shape_idx), node.rect.clone());
        }
        let kt = kids_top[&g_idx];
        let kids = groups[g_idx].kids.clone();
        for k_idx in kids {
            groups[k_idx].rect.x += left;
            groups[k_idx].rect.y += top + kt;
        }
        if g_idx != 0 {
            boxes.insert(ElementId::Boundary(g_idx), groups[g_idx].rect.clone());
        }
    }

    Layout { order, groups, boxes }
}
