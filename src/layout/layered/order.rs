use super::{Graph, LNode, MAX_SWEEPS};
use std::cmp::Ordering;

struct Cross<'a> {
    g: &'a Graph,
    nodes: &'a [LNode],
    order: Vec<usize>,
    pos: Vec<usize>,
    // With offsets, edges that reach one node at different offsets take separate places along it.
    placed: bool,
    place: Vec<usize>,
    // Local offset indices and counts do not depend on node order.
    place_in: Vec<usize>,
    places: Vec<usize>,
    leaving: Vec<usize>,
    mixed: Vec<bool>,
    tree: Vec<u32>,
    targets: Vec<usize>,
    bary: Vec<f64>,
    // Group bookkeeping, for keeping groups contiguous.
    depth: Vec<i32>,
    group_sum: Vec<f64>,
    group_count: Vec<i32>,
    group_first: Vec<usize>,
}

// Sorts a run of edges by offset if they do not all share one. Ties keep their order.
fn by_offset(list: &mut [usize], dx: &[f64]) -> bool {
    if list.iter().all(|&e| dx[e] == dx[list[0]]) {
        return false;
    }
    list.sort_by(|&a, &b| dx[a].partial_cmp(&dx[b]).unwrap_or(Ordering::Equal));
    true
}

impl Cross<'_> {
    fn count(&mut self) -> usize {
        let g = self.g;
        let mut total = 0usize;
        for r in 0..g.max_rank {
            let mut south_len = g.layer_start[r + 2] - g.layer_start[r + 1];
            if self.placed {
                let mut next = 0;
                for i in g.layer_start[r + 1]..g.layer_start[r + 2] {
                    let v = self.order[i];
                    for k in g.pred_start[v]..g.pred_start[v + 1] {
                        self.place[g.pred_e[k]] = next + self.place_in[g.pred_e[k]];
                    }
                    next += self.places[v];
                }
                south_len = next;
            }
            if south_len < 2 {
                continue;
            }
            let mut t = 0;
            for i in g.layer_start[r]..g.layer_start[r + 1] {
                let v = self.order[i];
                let from = g.succ_start[v];
                let to = g.succ_start[v + 1];
                let t0 = t;
                for k in from..to {
                    self.targets[t] = if self.placed { self.place[self.leaving[k]] } else { self.pos[g.et[g.succ_e[k]]] };
                    t += 1;
                }
                if t - t0 < 2 {
                    continue;
                }
                if self.placed && self.mixed[v] {
                    // Sort equal-offset edges by destination to avoid counting shared endpoints as crossings.
                    for a in t0 + 1..t {
                        let item = self.targets[a];
                        let dx = g.efd[self.leaving[from + a - t0]];
                        let mut b = a;
                        while b > t0 && g.efd[self.leaving[from + b - 1 - t0]] == dx && self.targets[b - 1] > item {
                            self.targets[b] = self.targets[b - 1];
                            b -= 1;
                        }
                        self.targets[b] = item;
                    }
                } else {
                    self.targets[t0..t].sort_unstable();
                }
            }
            if t < 2 {
                continue;
            }
            let mut leaves = 1;
            while leaves < south_len {
                leaves <<= 1;
            }
            self.tree[..2 * leaves].fill(0);
            for k in 0..t {
                let mut i = self.targets[k] + leaves;
                self.tree[i] += 1;
                while i > 1 {
                    if i & 1 == 0 {
                        total += self.tree[i + 1] as usize;
                    }
                    i >>= 1;
                    self.tree[i] += 1;
                }
            }
        }
        total
    }

    // Keep groups contiguous, ordering each by its mean key.
    fn arrange(&mut self, r: usize, key: &[f64]) {
        let g = self.g;
        let n = g.n;
        let grp = g.grp.as_ref().unwrap();
        let parent = g.group_parent.as_ref().unwrap();
        let s = g.layer_start[r];
        let e = g.layer_start[r + 1];
        for i in s..e {
            let mut gr = grp[self.order[i]];
            while gr >= 0 {
                self.group_sum[gr as usize] = 0.0;
                self.group_count[gr as usize] = 0;
                gr = parent[gr as usize];
            }
        }
        for i in s..e {
            let v = self.order[i];
            let mut gr = grp[v];
            while gr >= 0 {
                let at = gr as usize;
                if self.group_count[at] == 0 {
                    self.group_first[at] = i;
                }
                self.group_sum[at] += key[v];
                self.group_count[at] += 1;
                gr = parent[at];
            }
        }
        // Compare siblings in their nearest common group; group items use n + group ID.
        let value = |item: usize| if item >= n { self.group_sum[item - n] / self.group_count[item - n] as f64 } else { key[item] };
        let first = |item: usize| if item >= n { self.group_first[item - n] } else { s + self.pos[item] };
        let depth = &self.depth;
        let compare = |&a: &usize, &b: &usize| -> Ordering {
            let mut ia = a;
            let mut ib = b;
            let mut ca = grp[a];
            let mut cb = grp[b];
            let mut da = if ca < 0 { 0 } else { depth[ca as usize] };
            let mut db = if cb < 0 { 0 } else { depth[cb as usize] };
            while da > db {
                ia = n + ca as usize;
                ca = parent[ca as usize];
                da -= 1;
            }
            while db > da {
                ib = n + cb as usize;
                cb = parent[cb as usize];
                db -= 1;
            }
            while ca != cb {
                ia = n + ca as usize;
                ca = parent[ca as usize];
                ib = n + cb as usize;
                cb = parent[cb as usize];
            }
            value(ia).partial_cmp(&value(ib)).unwrap_or(Ordering::Equal).then(first(ia).cmp(&first(ib)))
        };
        let mut row: Vec<usize> = self.order[s..e].to_vec();
        row.sort_by(compare);
        self.order[s..e].copy_from_slice(&row);
        for i in s..e {
            self.pos[self.order[i]] = i - s;
        }
    }

    fn sweep(&mut self, down: bool, bias_right: bool, moving: &mut [usize]) {
        let g = self.g;
        let (start, list, other, other_dx) = if down { (&g.pred_start, &g.pred_e, &g.ef, &g.efd) } else { (&g.succ_start, &g.succ_e, &g.et, &g.etd) };
        for step in 1..=g.max_rank {
            let r = if down { step } else { g.max_rank - step };
            if g.fixed[r] != 0 {
                continue;
            }
            let s = g.layer_start[r];
            let e = g.layer_start[r + 1];
            let mut movable = 0;
            for i in s..e {
                let v = self.order[i];
                let pin = if v < g.real { self.nodes[v].pin } else { 0 };
                if pin != 0 {
                    self.bary[v] = pin as f64 * 1e9;
                    moving[movable] = v;
                    movable += 1;
                    continue;
                }
                let from = start[v];
                let to = start[v + 1];
                if from == to {
                    if g.grp.is_some() {
                        // Preserve relative position when there are no neighbours in the adjacent rank.
                        let across = if down { g.layer_start[r] - g.layer_start[r - 1] } else { g.layer_start[r + 2] - g.layer_start[r + 1] };
                        self.bary[v] = ((i - s) * across) as f64 / (e - s) as f64;
                        continue;
                    }
                    self.bary[v] = -1.0;
                    continue;
                }
                let mut sum = 0.0;
                for k in from..to {
                    let u = other[list[k]];
                    sum += self.pos[u] as f64;
                    if g.offsets && g.w[u] > 0.0 {
                        sum += other_dx[list[k]] / g.w[u];
                    }
                }
                self.bary[v] = sum / (to - from) as f64;
                moving[movable] = v;
                movable += 1;
            }
            if g.grp.is_some() {
                let bary = std::mem::take(&mut self.bary);
                self.arrange(r, &bary);
                self.bary = bary;
                continue;
            }
            if movable < 2 {
                continue;
            }
            // Ties keep their order, or reverse it with `bias_right`.
            let (bary, pos) = (&self.bary, &self.pos);
            moving[..movable].sort_by(|&a, &b| {
                bary[a].partial_cmp(&bary[b]).unwrap_or(Ordering::Equal).then(if bias_right { pos[b].cmp(&pos[a]) } else { pos[a].cmp(&pos[b]) })
            });
            let mut k = 0;
            for i in s..e {
                if self.bary[self.order[i]] != -1.0 {
                    self.order[i] = moving[k];
                    k += 1;
                }
            }
            for i in s..e {
                self.pos[self.order[i]] = i - s;
            }
        }
    }

    // How many pairs of nodes in one rank are not in the order they were given.
    fn out_of_order(&self) -> usize {
        let g = self.g;
        let mut total = 0;
        for r in 0..=g.max_rank {
            for i in g.layer_start[r]..g.layer_start[r + 1] {
                if self.order[i] >= g.real {
                    continue;
                }
                for j in i + 1..g.layer_start[r + 1] {
                    if self.order[j] < self.order[i] {
                        total += 1;
                    }
                }
            }
        }
        total
    }

    fn settle_positions(&mut self) {
        let g = self.g;
        for r in 0..=g.max_rank {
            for i in g.layer_start[r]..g.layer_start[r + 1] {
                self.pos[self.order[i]] = i - g.layer_start[r];
            }
        }
    }

    fn pin_of(&self, v: usize) -> i32 {
        if v < self.g.real {
            self.nodes[v].pin
        } else {
            0
        }
    }

    fn at(&self, e: usize, up: bool) -> f64 {
        let g = self.g;
        let u = if up { g.ef[e] } else { g.et[e] };
        if g.offsets && g.w[u] > 0.0 {
            self.pos[u] as f64 + (if up { g.efd[e] } else { g.etd[e] }) / g.w[u]
        } else {
            self.pos[u] as f64
        }
    }

    // Crossings between a and b, assuming a is left of b.
    fn between(&self, a: usize, b: usize) -> usize {
        let g = self.g;
        let mut c = 0;
        for k in g.pred_start[a]..g.pred_start[a + 1] {
            let x = self.at(g.pred_e[k], true);
            for j in g.pred_start[b]..g.pred_start[b + 1] {
                if x > self.at(g.pred_e[j], true) {
                    c += 1;
                }
            }
        }
        for k in g.succ_start[a]..g.succ_start[a + 1] {
            let x = self.at(g.succ_e[k], false);
            for j in g.succ_start[b]..g.succ_start[b + 1] {
                if x > self.at(g.succ_e[j], false) {
                    c += 1;
                }
            }
        }
        c
    }

    // Adjacent swaps can remove crossings missed by barycenter sweeps.
    fn transpose(&mut self) {
        let g = self.g;
        for _pass in 0..8 {
            let mut improved = false;
            for r in 0..=g.max_rank {
                if g.fixed[r] != 0 {
                    continue;
                }
                let s = g.layer_start[r];
                let mut i = s;
                while i + 1 < g.layer_start[r + 1] {
                    let a = self.order[i];
                    let b = self.order[i + 1];
                    let apart = self.pin_of(a) != self.pin_of(b) || g.grp.as_ref().is_some_and(|grp| grp[a] != grp[b]);
                    if !apart && self.between(b, a) < self.between(a, b) {
                        self.order[i] = b;
                        self.order[i + 1] = a;
                        self.pos[b] = i - s;
                        self.pos[a] = i + 1 - s;
                        improved = true;
                    }
                    i += 1;
                }
            }
            if !improved {
                break;
            }
        }
    }
}

// Returns (final crossings, crossings before grouping or reordering).
pub(super) fn reduce_crossings(g: &mut Graph, nodes: &[LNode]) -> (usize, usize) {
    let order = std::mem::take(&mut g.order);
    let pos = std::mem::take(&mut g.pos);
    let (result, order, pos) = run(g, nodes, order, pos);
    g.order = order;
    g.pos = pos;
    result
}

fn run(g: &Graph, nodes: &[LNode], order: Vec<usize>, pos: Vec<usize>) -> ((usize, usize), Vec<usize>, Vec<usize>) {
    let n = g.n;
    let max_rank = g.max_rank;
    let segments = g.ef.len();
    let widest = (0..=max_rank).map(|r| g.layer_start[r + 1] - g.layer_start[r]).max().unwrap_or(0);
    if widest < 2 {
        return ((0, 0), order, pos);
    }
    let placed = g.offsets;
    let mut place_in = vec![0usize; if placed { segments } else { 0 }];
    let mut places = vec![0usize; if placed { n } else { 0 }];
    let mut leaving = if placed { g.succ_e.clone() } else { Vec::new() };
    let mut mixed = vec![false; if placed { n } else { 0 }];
    if placed {
        let mut arriving = g.pred_e.clone();
        for v in 0..n {
            places[v] = 1;
            if by_offset(&mut arriving[g.pred_start[v]..g.pred_start[v + 1]], &g.etd) {
                for k in g.pred_start[v] + 1..g.pred_start[v + 1] {
                    if g.etd[arriving[k]] != g.etd[arriving[k - 1]] {
                        places[v] += 1;
                    }
                    place_in[arriving[k]] = places[v] - 1;
                }
            }
            if by_offset(&mut leaving[g.succ_start[v]..g.succ_start[v + 1]], &g.efd) {
                mixed[v] = true;
            }
        }
    }
    let mut size = 1;
    while size < (if placed { widest + segments } else { widest }) {
        size <<= 1;
    }
    let groups = g.group_parent.as_ref().map_or(0, |p| p.len());
    let mut depth = vec![0i32; groups];
    if let Some(parent) = &g.group_parent {
        for gr in 0..groups {
            let mut p = gr as i32;
            while p >= 0 {
                depth[gr] += 1;
                p = parent[p as usize];
            }
        }
    }
    let mut c = Cross {
        g,
        nodes,
        order,
        pos,
        placed,
        place: vec![0usize; if placed { segments } else { 0 }],
        place_in,
        places,
        leaving,
        mixed,
        tree: vec![0u32; 2 * size],
        targets: vec![0usize; segments],
        bary: vec![0.0; n],
        depth,
        group_sum: vec![0.0; groups],
        group_count: vec![0; groups],
        group_first: vec![0; groups],
    };

    let declared = c.count();
    let mut moving = vec![0usize; widest];
    let grouped = g.grp.is_some();
    if grouped {
        let mut at = vec![0.0; n];
        for r in 0..=max_rank {
            for i in g.layer_start[r]..g.layer_start[r + 1] {
                at[c.order[i]] = (i - g.layer_start[r]) as f64;
            }
            c.arrange(r, &at);
        }
    }
    let mut best = if grouped { c.count() } else { declared };
    if best == 0 {
        return ((0, declared), c.order, c.pos);
    }
    let best_order = c.order.clone();
    // Require improvements over both the grouped and ungrouped starting orders.
    let start = best.min(declared);

    // The sweeps are run twice from the same start, going down first and going up first. They
    // often end with as few crossings either way but a different order, and then the one that
    // keeps more of the nodes in the order they were given is the less surprising.
    // Only for graphs small enough to be read node by node: a second run costs as much as the first.
    let initial = c.order.clone();
    let small = n <= 600;
    let mut chosen = best_order.clone();
    let mut chosen_out = usize::MAX;
    for attempt in 0..(if small { 2 } else { 1 }) {
        if attempt > 0 {
            c.order.copy_from_slice(&initial);
            c.settle_positions();
        }
        let mut found = start;
        let mut found_order = initial.clone();
        let mut i = 0;
        let mut stale = 0;
        while stale < 4 && i < MAX_SWEEPS {
            c.sweep((i + attempt) % 2 == 0, i % 4 >= 2, &mut moving);
            let crossings = c.count();
            if crossings < found {
                found = crossings;
                found_order.copy_from_slice(&c.order);
                stale = 0;
                if crossings == 0 {
                    break;
                }
            }
            i += 1;
            stale += 1;
        }
        c.order.copy_from_slice(&found_order);
        if small && found > 0 {
            c.settle_positions();
            c.transpose();
            let crossings = c.count();
            if crossings < found {
                found = crossings;
                found_order.copy_from_slice(&c.order);
            }
        }
        let out = if small { c.out_of_order() } else { 0 };
        if attempt == 0 || found < best || (found == best && out < chosen_out) {
            best = found;
            chosen = found_order;
            chosen_out = out;
        }
    }
    c.order.copy_from_slice(&chosen);
    c.settle_positions();
    // The retained grouped order may exceed the ungrouped threshold; recount it.
    if grouped {
        best = c.count();
    }

    // A drawing and its mirror image cross as often too. Of those two, likewise.
    let mirror = small && !nodes.iter().any(|node| node.pin != 0) && chosen_out != 0 && g.fixed[0] == 0 && g.fixed[max_rank] == 0;
    if mirror {
        for r in 0..=max_rank {
            c.order[g.layer_start[r]..g.layer_start[r + 1]].reverse();
        }
        c.settle_positions();
        if c.count() > best || c.out_of_order() >= chosen_out {
            c.order.copy_from_slice(&chosen);
        }
    }
    ((best, declared), c.order, c.pos)
}
