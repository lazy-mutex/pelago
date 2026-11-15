use super::compound::{CEdge, CNode, CompoundResult};
use std::collections::HashMap;

struct Part {
    node_indices: Vec<usize>,
    edge_indices: Vec<usize>,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Part {
    fn new() -> Self {
        Self {
            node_indices: Vec::new(),
            edge_indices: Vec::new(),
            left: f64::INFINITY,
            top: f64::INFINITY,
            right: f64::NEG_INFINITY,
            bottom: f64::NEG_INFINITY,
        }
    }

    fn include(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.left = self.left.min(x - w / 2.0);
        self.top = self.top.min(y - h / 2.0);
        self.right = self.right.max(x + w / 2.0);
        self.bottom = self.bottom.max(y + h / 2.0);
    }
}

// Moves disconnected parts into rows without changing the layout inside a part.
pub fn wrap_components(nodes: &mut [CNode], edges: &mut [CEdge], width: f64, gap: f64) -> Option<CompoundResult> {
    let n = nodes.len();
    if n == 0 {
        return None;
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut join = |a: usize, b: usize| {
        let ra = root(&mut parent, a);
        let rb = root(&mut parent, b);
        parent[ra] = rb;
    };
    for i in 0..n {
        if nodes[i].parent >= 0 {
            join(i, nodes[i].parent as usize);
        }
    }
    for e in edges.iter() {
        join(e.src, e.dst);
    }

    // Preserve insertion order of parts as in JS Map.
    let mut order: Vec<usize> = Vec::new();
    let mut parts: HashMap<usize, Part> = HashMap::new();
    for i in 0..n {
        let id = root(&mut parent, i);
        if !parts.contains_key(&id) {
            order.push(id);
            parts.insert(id, Part::new());
        }
        let part = parts.get_mut(&id).unwrap();
        part.node_indices.push(i);
        part.include(nodes[i].x, nodes[i].y, nodes[i].w, nodes[i].h);
    }
    if order.len() < 2 {
        return None;
    }
    for (ei, e) in edges.iter().enumerate() {
        let id = root(&mut parent, e.src);
        let part = parts.get_mut(&id).unwrap();
        part.edge_indices.push(ei);
        let mut i = 0;
        while i + 1 < e.route.len() {
            part.include(e.route[i], e.route[i + 1], 0.0, 0.0);
            i += 3;
        }
        if e.label_w > 0.0 {
            part.include(e.label_x, e.label_y, e.label_w, e.label_h);
        }
    }

    let mut x = 0.0f64;
    let mut y = 0.0f64;
    let mut row_height = 0.0f64;
    let mut right = 0.0f64;
    let mut from = f64::INFINITY;
    let mut to = f64::NEG_INFINITY;
    let mut shifts: Vec<(f64, f64)> = Vec::with_capacity(order.len());
    for &id in &order {
        let part = &parts[&id];
        let w = part.right - part.left;
        let h = part.bottom - part.top;
        if x > 0.0 && x + w > width {
            y += row_height + gap;
            x = 0.0;
            row_height = 0.0;
        }
        shifts.push((x - part.left, y - part.top));
        from = from.min(part.left);
        to = to.max(part.right);
        right = right.max(x + w);
        row_height = row_height.max(h);
        x += w + gap;
    }
    if right >= to - from {
        return None;
    }
    for (idx, &id) in order.iter().enumerate() {
        let (dx, dy) = shifts[idx];
        let part = &parts[&id];
        for &ni in &part.node_indices {
            nodes[ni].x += dx;
            nodes[ni].y += dy;
        }
        for &ei in &part.edge_indices {
            let e = &mut edges[ei];
            let mut i = 0;
            while i + 1 < e.route.len() {
                e.route[i] += dx;
                e.route[i + 1] += dy;
                i += 3;
            }
            e.label_x += dx;
            e.label_y += dy;
        }
    }
    Some(CompoundResult { width: right, height: y + row_height })
}
