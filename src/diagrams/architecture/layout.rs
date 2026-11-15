use super::db::{ArchEdge, ArchGroup, ArchLayoutHint, ArchNode};
use super::parser::{AlignDir, Side};
use std::collections::HashMap;

// Grid placement. Every service and junction takes a cell, an edge `a:R -- L:b` puts b in the
// cell to the right of a, and a group is a solid rectangle of cells, so a group never encloses
// a node that is not in it. Each container (the diagram, then every group) is laid out on its
// own, innermost first, and a finished group is placed in its parent as one block.

pub struct Grid {
    pub col: Vec<i32>,
    pub row: Vec<i32>,
    pub group_col: Vec<i32>,
    pub group_row: Vec<i32>,
    pub group_cols: Vec<i32>,
    pub group_rows: Vec<i32>,
    pub group_parent: Vec<i32>,
    pub group_empty: Vec<bool>,
    pub node_parent: Vec<i32>,
}

const DX: [i32; 4] = [-1, 1, 0, 0];
const DY: [i32; 4] = [0, 0, -1, 1];

pub fn side_index(side: Side) -> usize {
    match side {
        Side::L => 0,
        Side::R => 1,
        Side::T => 2,
        Side::B => 3,
    }
}

// Blocks up to this many cells are found through the cell map; larger ones are compared one by one.
const SMALL: i64 = 64;

struct Placer {
    n: usize,
    parent: Vec<usize>,
    link_a: Vec<usize>,
    link_b: Vec<usize>,
    side_a: Vec<usize>,
    side_b: Vec<usize>,
    item_a: Vec<usize>,
    item_b: Vec<usize>,
    hint_links: Vec<Vec<usize>>,
    edge_links: Vec<Vec<usize>>,
    x: Vec<i32>,
    y: Vec<i32>,
    w: Vec<i32>,
    h: Vec<i32>,
    placed: Vec<bool>,
    order: Vec<usize>,
    cells: HashMap<(i32, i32), usize>,
    large: Vec<usize>,
    reach: HashMap<(i32, i32, i32, i32, i32, i32), i32>,
    first: usize,
}

impl Placer {
    fn anchor(&self, node: usize, item: usize) -> (i32, i32) {
        let mut ax = 0;
        let mut ay = 0;
        let mut cur = node;
        while cur != item {
            ax += self.x[cur];
            ay += self.y[cur];
            cur = self.n + self.parent[cur];
        }
        (ax, ay)
    }

    fn put(&mut self, item: usize, px: i32, py: i32) {
        self.x[item] = px;
        self.y[item] = py;
        self.placed[item] = true;
        self.order.push(item);
        if (self.w[item] as i64) * (self.h[item] as i64) > SMALL {
            self.large.push(item);
            return;
        }
        for cy in py..py + self.h[item] {
            for cx in px..px + self.w[item] {
                self.cells.insert((cx, cy), item);
            }
        }
    }

    fn overlaps(&self, item: usize, rx: i32, ry: i32, rw: i32, rh: i32) -> bool {
        self.x[item] < rx + rw
            && rx < self.x[item] + self.w[item]
            && self.y[item] < ry + rh
            && ry < self.y[item] + self.h[item]
    }

    fn collide(&self, rx: i32, ry: i32, rw: i32, rh: i32) -> Option<usize> {
        if (rw as i64) * (rh as i64) > SMALL {
            for i in self.first..self.order.len() {
                if self.overlaps(self.order[i], rx, ry, rw, rh) {
                    return Some(self.order[i]);
                }
            }
            return None;
        }
        for cy in ry..ry + rh {
            for cx in rx..rx + rw {
                if let Some(&hit) = self.cells.get(&(cx, cy)) {
                    return Some(hit);
                }
            }
        }
        for &item in &self.large {
            if self.overlaps(item, rx, ry, rw, rh) {
                return Some(item);
            }
        }
        None
    }

    // How far a block must slide along (sx, sy) to be clear.
    fn slide(&mut self, rx: i32, ry: i32, rw: i32, rh: i32, sx: i32, sy: i32) -> i32 {
        let key = (rx, ry, rw, rh, sx, sy);
        let mut k = self.reach.get(&key).copied().unwrap_or(0);
        while let Some(hit) = self.collide(rx + k * sx, ry + k * sy, rw, rh) {
            k = if sx > 0 {
                self.x[hit] + self.w[hit] - rx
            } else if sx < 0 {
                rx + rw - self.x[hit]
            } else if sy > 0 {
                self.y[hit] + self.h[hit] - ry
            } else {
                ry + rh - self.y[hit]
            };
        }
        self.reach.insert(key, k);
        k
    }

    fn follow(&mut self, from: usize, k: usize) {
        let forward = self.item_a[k] == from;
        let to = if forward { self.item_b[k] } else { self.item_a[k] };
        if self.placed[to] {
            return;
        }
        let sp = if forward { self.side_a[k] } else { self.side_b[k] };
        let sq = if forward { self.side_b[k] } else { self.side_a[k] };
        let (pax, pay) = self.anchor(if forward { self.link_a[k] } else { self.link_b[k] }, from);
        let (ax, ay) = self.anchor(if forward { self.link_b[k] } else { self.link_a[k] }, to);
        let mut dx = DX[sp];
        let mut dy = DY[sp];
        let mut shift_x = 0;
        let mut shift_y = 0;
        let facing = DX[sp] + DX[sq] == 0 && DY[sp] + DY[sq] == 0;
        if sp == sq {
            // Like sides: the edge loops around, so the nodes stack across the direction it leaves in.
            dx = if DX[sp] == 0 { 1 } else { 0 };
            dy = 1 - dx;
        } else if !facing {
            // A bend: one step along the side the edge leaves, one step back from the side it enters.
            shift_x = -DX[sq];
            shift_y = -DY[sq];
        }
        let (mut bx, mut by) = if dx != 0 {
            let bx = if dx > 0 { self.x[from] + self.w[from] } else { self.x[from] - self.w[to] };
            let by = self.y[from] + pay + shift_y - ay;
            (bx, by)
        } else {
            let by = if dy > 0 { self.y[from] + self.h[from] } else { self.y[from] - self.h[to] };
            let bx = self.x[from] + pax + shift_x - ax;
            (bx, by)
        };
        if facing {
            // When several placed nodes reach this one through the same pair of sides, line it up
            // with the middle one, not with whichever happened to be visited first.
            if self.edge_links[to].len() > 1 {
                let mut wanted: Vec<i32> = Vec::new();
                for idx in 0..self.edge_links[to].len() {
                    let j = self.edge_links[to][idx];
                    let to_is_b = self.item_b[j] == to;
                    let other = if to_is_b { self.item_a[j] } else { self.item_b[j] };
                    let other_side = if to_is_b { self.side_a[j] } else { self.side_b[j] };
                    let to_side = if to_is_b { self.side_b[j] } else { self.side_a[j] };
                    if !self.placed[other] || other_side != sp || to_side != sq {
                        continue;
                    }
                    let (oax, oay) = self.anchor(if to_is_b { self.link_a[j] } else { self.link_b[j] }, other);
                    let at = if dx != 0 { self.y[other] + oay } else { self.x[other] + oax };
                    let (tax, tay) = self.anchor(if to_is_b { self.link_b[j] } else { self.link_a[j] }, to);
                    wanted.push(at - if dx != 0 { tay } else { tax });
                }
                if wanted.len() > 1 {
                    wanted.sort_unstable();
                    let mid = wanted[(wanted.len() - 1) >> 1];
                    if dx != 0 {
                        by = mid;
                    } else {
                        bx = mid;
                    }
                }
            }
            // Taken: move sideways, to whichever side is nearer, so the edge can still leave straight.
            let px = if dx == 0 { 1 } else { 0 };
            let py = 1 - px;
            let rw = self.w[to];
            let rh = self.h[to];
            let forth = self.slide(bx, by, rw, rh, px, py);
            let back = if forth == 0 { 0 } else { self.slide(bx, by, rw, rh, -px, -py) };
            let near = if forth <= back { forth } else { -back };
            bx += px * near;
            by += py * near;
        } else {
            let rw = self.w[to];
            let rh = self.h[to];
            let far = self.slide(bx, by, rw, rh, dx, dy);
            bx += dx * far;
            by += dy * far;
        }
        self.put(to, bx, by);
    }

    fn layout(&mut self, c: usize, g_len: usize, kids: &[Vec<usize>]) {
        let items = &kids[c];
        if items.is_empty() {
            return;
        }
        self.order.clear();
        let mut starts: Vec<usize> = Vec::new();
        for &start in items {
            if self.placed[start] {
                continue;
            }
            self.first = self.order.len();
            starts.push(self.first);
            self.cells.clear();
            self.large.clear();
            self.reach.clear();
            self.put(start, 0, 0);
            // Declared alignments are followed as soon as a node is placed, ahead of any edge.
            let mut hint_at = self.first;
            let mut edge_at = self.first;
            let mut link_at = 0usize;
            loop {
                if hint_at < self.order.len() {
                    let from = self.order[hint_at];
                    hint_at += 1;
                    let len = self.hint_links[from].len();
                    for idx in 0..len {
                        let k = self.hint_links[from][idx];
                        self.follow(from, k);
                    }
                    continue;
                }
                if edge_at >= self.order.len() {
                    break;
                }
                let from = self.order[edge_at];
                if link_at >= self.edge_links[from].len() {
                    edge_at += 1;
                    link_at = 0;
                    continue;
                }
                let k = self.edge_links[from][link_at];
                link_at += 1;
                self.follow(from, k);
            }
        }
        starts.push(self.order.len());

        // Unconnected parts go side by side, wrapping into rows that keep the whole roughly square.
        let count = starts.len() - 1;
        let mut min_x = vec![0i32; count];
        let mut min_y = vec![0i32; count];
        let mut part_w = vec![0i32; count];
        let mut part_h = vec![0i32; count];
        let mut area: i64 = 0;
        let mut widest: i32 = 0;
        for p in 0..count {
            let mut x0 = i32::MAX;
            let mut y0 = i32::MAX;
            let mut x1 = i32::MIN;
            let mut y1 = i32::MIN;
            for i in starts[p]..starts[p + 1] {
                let item = self.order[i];
                x0 = x0.min(self.x[item]);
                y0 = y0.min(self.y[item]);
                x1 = x1.max(self.x[item] + self.w[item]);
                y1 = y1.max(self.y[item] + self.h[item]);
            }
            min_x[p] = x0;
            min_y[p] = y0;
            part_w[p] = x1 - x0;
            part_h[p] = y1 - y0;
            area += (part_w[p] as i64) * (part_h[p] as i64);
            widest = widest.max(part_w[p]);
        }
        let limit = widest.max(((area as f64 * 2.0).sqrt().ceil()) as i32);
        let mut cx = 0;
        let mut cy = 0;
        let mut shelf = 0;
        let mut width = 0;
        for p in 0..count {
            if cx > 0 && cx + part_w[p] > limit {
                cy += shelf;
                cx = 0;
                shelf = 0;
            }
            for i in starts[p]..starts[p + 1] {
                let item = self.order[i];
                self.x[item] += cx - min_x[p];
                self.y[item] += cy - min_y[p];
            }
            cx += part_w[p];
            shelf = shelf.max(part_h[p]);
            width = width.max(cx);
        }
        if c < g_len {
            self.w[self.n + c] = width;
            self.h[self.n + c] = cy + shelf;
        }
    }
}

pub fn place_on_grid(
    nodes: &[ArchNode],
    groups: &[ArchGroup],
    edges: &[ArchEdge],
    hints: &[ArchLayoutHint],
) -> Grid {
    let n = nodes.len();
    let g_len = groups.len();
    let root = g_len;
    let mut node_index: HashMap<&str, usize> = HashMap::new();
    let mut group_index: HashMap<&str, usize> = HashMap::new();
    // Items are nodes (0..n-1) then groups (n..n+g_len-1). Containers are groups (0..g_len-1) then root.
    let mut parent = vec![root; n + g_len];
    let mut depth = vec![0i32; g_len + 1];
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); g_len + 1];

    for (g, group) in groups.iter().enumerate() {
        group_index.insert(&group.id, g);
        let p = group.parent.as_deref().and_then(|pid| group_index.get(pid).copied());
        if let Some(p) = p.filter(|&p| p != g) {
            parent[n + g] = p;
            depth[g] = depth[p] + 1;
        } else {
            depth[g] = 1;
        }
    }
    for (i, node) in nodes.iter().enumerate() {
        node_index.insert(node.id(), i);
        if let Some(p) = node.parent().and_then(|pid| group_index.get(pid).copied()) {
            parent[i] = p;
        }
        kids[parent[i]].push(i);
    }
    for g in 0..g_len {
        kids[parent[n + g]].push(n + g);
    }

    let mut placer = Placer {
        n,
        parent,
        link_a: Vec::new(),
        link_b: Vec::new(),
        side_a: Vec::new(),
        side_b: Vec::new(),
        item_a: Vec::new(),
        item_b: Vec::new(),
        hint_links: vec![Vec::new(); n + g_len],
        edge_links: vec![Vec::new(); n + g_len],
        x: vec![0; n + g_len],
        y: vec![0; n + g_len],
        w: vec![1; n + g_len],
        h: vec![1; n + g_len],
        placed: vec![false; n + g_len],
        order: Vec::new(),
        cells: HashMap::new(),
        large: Vec::new(),
        reach: HashMap::new(),
        first: 0,
    };

    let mut add_link = |a: usize, b: usize, sa: usize, sb: usize, is_hint: bool| {
        if a == b {
            return;
        }
        let mut ia = a;
        let mut ib = b;
        let mut ca = placer.parent[a];
        let mut cb = placer.parent[b];
        while ca != cb {
            if depth[ca] >= depth[cb] {
                ia = n + ca;
                ca = placer.parent[ia];
            } else {
                ib = n + cb;
                cb = placer.parent[ib];
            }
        }
        let k = placer.link_a.len();
        placer.link_a.push(a);
        placer.link_b.push(b);
        placer.side_a.push(sa);
        placer.side_b.push(sb);
        placer.item_a.push(ia);
        placer.item_b.push(ib);
        let lists = if is_hint { &mut placer.hint_links } else { &mut placer.edge_links };
        lists[ia].push(k);
        lists[ib].push(k);
    };

    for hint in hints {
        let row = hint.direction == AlignDir::Row;
        for i in 1..hint.members.len() {
            if let (Some(&a), Some(&b)) = (
                node_index.get(hint.members[i - 1].as_str()),
                node_index.get(hint.members[i].as_str()),
            ) {
                add_link(a, b, if row { 1 } else { 3 }, if row { 0 } else { 2 }, true);
            }
        }
    }
    // An edge between two like sides says least about where its ends go, so those come last.
    for like_sides in [false, true] {
        for edge in edges {
            if (edge.lhs_dir == edge.rhs_dir) != like_sides {
                continue;
            }
            if let (Some(&a), Some(&b)) = (
                node_index.get(edge.lhs_id.as_str()),
                node_index.get(edge.rhs_id.as_str()),
            ) {
                add_link(a, b, side_index(edge.lhs_dir), side_index(edge.rhs_dir), false);
            }
        }
    }

    for c in (0..g_len).rev() {
        placer.layout(c, g_len, &kids);
    }
    placer.layout(root, g_len, &kids);

    let mut group_col = vec![0i32; g_len];
    let mut group_row = vec![0i32; g_len];
    let mut group_parent = vec![0i32; g_len];
    let mut group_empty = vec![false; g_len];
    for g in 0..g_len {
        let p = placer.parent[n + g];
        group_parent[g] = if p == root { -1 } else { p as i32 };
        group_col[g] = placer.x[n + g] + if p == root { 0 } else { group_col[p] };
        group_row[g] = placer.y[n + g] + if p == root { 0 } else { group_row[p] };
        if kids[g].is_empty() {
            group_empty[g] = true;
        }
    }
    let mut col = vec![0i32; n];
    let mut row = vec![0i32; n];
    let mut node_parent = vec![0i32; n];
    for i in 0..n {
        let p = placer.parent[i];
        node_parent[i] = if p == root { -1 } else { p as i32 };
        col[i] = placer.x[i] + if p == root { 0 } else { group_col[p] };
        row[i] = placer.y[i] + if p == root { 0 } else { group_row[p] };
    }
    Grid {
        col,
        row,
        group_col,
        group_row,
        group_cols: placer.w[n..].to_vec(),
        group_rows: placer.h[n..].to_vec(),
        group_parent,
        group_empty,
        node_parent,
    }
}
