use crate::svg::builder::js_round;

use super::model::TreemapNode;

#[derive(Debug, Clone)]
pub struct BoxItem<'a> {
    pub node: &'a TreemapNode,
    pub parent: Option<usize>,
    pub series: usize,
    pub value: f64,
    pub children: Vec<usize>,
    pub headed: bool,
    pub header: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Insets {
    pub gap: f64,
    pub header: f64,
    pub short: f64,
    pub side: f64,
}

pub struct Layout<'a> {
    pub arena: Vec<BoxItem<'a>>,
    pub placed: Vec<usize>,
}

fn new_box<'a>(node: &'a TreemapNode, parent: Option<usize>, series: usize) -> BoxItem<'a> {
    let value = match (&node.children, node.value) {
        (None, Some(v)) if v.is_finite() && v > 0.0 => v,
        _ => 0.0,
    };
    BoxItem {
        node,
        parent,
        series,
        value,
        children: Vec::new(),
        headed: false,
        header: 0.0,
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    }
}

fn clamp(v: f64, low: f64, high: f64) -> f64 {
    if v >= low {
        v.min(high)
    } else {
        low
    }
}

#[allow(clippy::too_many_arguments)]
fn squarify(
    arena: &mut [BoxItem<'_>],
    shown: &[usize],
    total: f64,
    mut x: f64,
    mut y: f64,
    mut w: f64,
    mut h: f64,
    gap: f64,
) {
    let mut rest = total;
    let mut from = 0usize;
    while from < shown.len() {
        let scale = (w * h) / rest;
        let side = w.min(h);
        let mut sum = arena[shown[from]].value * scale;
        let largest = sum;
        let mut worst = ((side * side * largest) / (sum * sum)).max((sum * sum) / (side * side * largest));
        let mut to = from + 1;
        while to < shown.len() {
            let smallest = arena[shown[to]].value * scale;
            let next = sum + smallest;
            let ratio = ((side * side * largest) / (next * next)).max((next * next) / (side * side * smallest));
            if ratio > worst {
                break;
            }
            worst = ratio;
            sum = next;
            to += 1;
        }

        let thickness = sum / side;
        let mut offset = 0.0;
        for i in from..to {
            let b = &mut arena[shown[i]];
            let length = if i == to - 1 { side - offset } else { (b.value * scale) / thickness };
            let bx = if w >= h { x } else { x + offset };
            let by = if w >= h { y + offset } else { y };
            let bw = if w >= h { thickness } else { length };
            let bh = if w >= h { length } else { thickness };
            let gx = if js_round(bx + bw - gap / 2.0) > js_round(bx + gap / 2.0) { gap / 2.0 } else { 0.0 };
            let gy = if js_round(by + bh - gap / 2.0) > js_round(by + gap / 2.0) { gap / 2.0 } else { 0.0 };
            b.x = js_round(bx + gx);
            b.y = js_round(by + gy);
            b.w = js_round(bx + bw - gx) - b.x;
            b.h = js_round(by + bh - gy) - b.y;
            offset += length;
            rest -= b.value;
        }
        if w >= h {
            x += thickness;
            w -= thickness;
        } else {
            y += thickness;
            h -= thickness;
        }
        from = to;
    }
}

fn place(
    arena: &mut [BoxItem<'_>],
    placed: &mut Vec<usize>,
    children: &[usize],
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    gap: f64,
) {
    let mut shown: Vec<usize> = children.iter().copied().filter(|&idx| arena[idx].value > 0.0).collect();
    shown.sort_by(|&a, &b| {
        arena[b]
            .value
            .partial_cmp(&arena[a].value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut total = 0.0;
    for &idx in &shown {
        total += arena[idx].value;
    }
    if shown.is_empty() || !total.is_finite() {
        return;
    }
    let w = w.max(0.0);
    let h = h.max(0.0);
    if w > 0.0 && h > 0.0 {
        squarify(arena, &shown, total, x - gap / 2.0, y - gap / 2.0, w + gap, h + gap, gap);
    }
    for idx in shown {
        let b = &mut arena[idx];
        let x0 = clamp(b.x, x, x + w);
        let y0 = clamp(b.y, y, y + h);
        b.w = clamp(b.x + b.w, x0, x + w) - x0;
        b.h = clamp(b.y + b.h, y0, y + h) - y0;
        b.x = x0;
        b.y = y0;
        placed.push(idx);
    }
}

pub fn layout_treemap<'a>(roots: &'a [TreemapNode], width: f64, height: f64, insets: Insets) -> Layout<'a> {
    let mut arena: Vec<BoxItem<'a>> = roots
        .iter()
        .enumerate()
        .map(|(index, node)| new_box(node, None, index))
        .collect();
    let top: Vec<usize> = (0..roots.len()).collect();

    let mut i = 0usize;
    while i < arena.len() {
        if let Some(children) = &arena[i].node.children {
            let series = arena[i].series;
            let mut child_indices = Vec::with_capacity(children.len());
            for child_node in children {
                let idx = arena.len();
                arena.push(new_box(child_node, Some(i), series));
                child_indices.push(idx);
            }
            arena[i].children = child_indices;
        }
        i += 1;
    }

    for idx in (0..arena.len()).rev() {
        if let Some(p) = arena[idx].parent {
            arena[p].value += arena[idx].value;
        }
    }

    let Insets { gap, header, short, side } = insets;
    let mut placed = Vec::new();
    place(&mut arena, &mut placed, &top, 0.0, 0.0, width, height, gap);

    let mut p_idx = 0usize;
    while p_idx < placed.len() {
        let b_idx = placed[p_idx];
        p_idx += 1;
        if arena[b_idx].children.is_empty() {
            continue;
        }
        let b_x = arena[b_idx].x;
        let b_y = arena[b_idx].y;
        let b_w = arena[b_idx].w;
        let b_h = arena[b_idx].h;
        let top_h = if b_h - header - side >= 12.0 { header } else { short };
        let headed = b_w > 24.0_f64.max(2.0 * side) && b_h - top_h - side >= 8.0;
        arena[b_idx].headed = headed;
        arena[b_idx].header = if headed { top_h } else { 0.0 };
        let children = arena[b_idx].children.clone();
        if headed {
            place(&mut arena, &mut placed, &children, b_x + side, b_y + top_h, b_w - 2.0 * side, b_h - top_h - side, gap);
        } else {
            place(&mut arena, &mut placed, &children, b_x, b_y, b_w, b_h, gap);
        }
    }

    Layout { arena, placed }
}
