use super::{is_dummy, kind, Graph, LayeredOptions};

// Half the room a node, bend or port takes across the flow, with its share of the gap beside it.
// A port on a group's border is a point that an edge passes through, and needs no more than a bend.
pub(super) fn half_room(g: &Graph, opt: &LayeredOptions, v: usize) -> f64 {
    g.w[v] / 2.0 + (if g.kind[v] == kind::NODE { opt.node_sep } else { opt.edge_sep }) / 2.0
}

// Brandes-Köpf horizontal coordinate assignment: four aligned layouts, balanced.
pub(super) fn position(g: &mut Graph, opt: &LayeredOptions) -> Vec<f64> {
    let n = g.n;
    let max_rank = g.max_rank;
    let segments = g.ef.len();

    // Neighbours in left-to-right order, so the medians can be read off directly.
    {
        let key: Vec<usize> = (0..segments).map(|e| g.pos[g.et[e]]).collect();
        for v in 0..n {
            g.succ_e[g.succ_start[v]..g.succ_start[v + 1]].sort_by_key(|&e| (key[e], e));
        }
        let key: Vec<usize> = (0..segments).map(|e| g.pos[g.ef[e]]).collect();
        for v in 0..n {
            g.pred_e[g.pred_start[v]..g.pred_start[v + 1]].sort_by_key(|&e| (key[e], e));
        }
    }
    let g = &*g;
    let gap: Vec<f64> = (0..n).map(|i| half_room(g, opt, i)).collect();
    let dummy = |v: usize| is_dummy(g.kind[v]);

    // An edge between two real nodes may not cross a dummy-to-dummy segment; mark those that would.
    let mut conflict = vec![false; segments];
    for r in 1..=max_rank {
        let prev_len = g.layer_start[r] - g.layer_start[r - 1];
        let s = g.layer_start[r];
        let end = g.layer_start[r + 1];
        let mut k0 = 0usize;
        let mut scan = s;
        for i in s..end {
            let v = g.order[i];
            let mut inner = None;
            if dummy(v) {
                let u = g.ef[g.pred_e[g.pred_start[v]]];
                if dummy(u) {
                    inner = Some(u);
                }
            }
            let k1 = inner.map_or(prev_len, |u| g.pos[u]);
            if inner.is_some() || i == end - 1 {
                for j in scan..=i {
                    let w = g.order[j];
                    for k in g.pred_start[w]..g.pred_start[w + 1] {
                        let e = g.pred_e[k];
                        let up = g.pos[g.ef[e]];
                        if (up < k0 || k1 < up) && !(dummy(w) && dummy(g.ef[e])) {
                            conflict[e] = true;
                        }
                    }
                }
                scan = i + 1;
                k0 = k1;
            }
        }
    }

    let mut results: Vec<Vec<f64>> = Vec::with_capacity(4);
    let mut root = vec![0usize; n];
    let mut align = vec![0usize; n];
    // Offset of each node from its block's root, so that aligned edges meet at their attachment points.
    let mut shift = vec![0.0; n];
    let mut e_from = vec![0usize; n];
    let mut e_to = vec![0usize; n];
    let mut e_w = vec![0.0; n];
    let mut indeg = vec![0usize; n];
    let mut out_start = vec![0usize; n + 1];
    let mut out_e = vec![0usize; n];
    let mut topo = vec![0usize; n];
    let mut x = vec![0.0f64; n];

    for variant in 0..4 {
        let up = variant < 2;
        let right = variant & 1 == 1;
        let (start, list, other, own_dx, other_dx) =
            if up { (&g.pred_start, &g.pred_e, &g.ef, &g.etd, &g.efd) } else { (&g.succ_start, &g.succ_e, &g.et, &g.efd, &g.etd) };
        for i in 0..n {
            root[i] = i;
            align[i] = i;
            shift[i] = 0.0;
            indeg[i] = 0;
            out_start[i + 1] = 0;
            x[i] = 0.0;
        }

        for step in 0..=max_rank {
            let r = if up { step } else { max_rank - step };
            let s = g.layer_start[r];
            let end = g.layer_start[r + 1];
            let adjacent = if up { r as i64 - 1 } else { r as i64 + 1 };
            let adjacent_last =
                if adjacent >= 0 && adjacent <= max_rank as i64 { (g.layer_start[adjacent as usize + 1] - g.layer_start[adjacent as usize]) as i64 - 1 } else { 0 };
            let mut prev_idx: i64 = -1;
            for step_i in 0..end - s {
                let i = if right { end - 1 - step_i } else { s + step_i };
                let v = g.order[i];
                let from = start[v];
                let d = start[v + 1] - from;
                if d == 0 {
                    continue;
                }
                let lo = from + ((d - 1) >> 1);
                let hi = from + (d >> 1);
                for pass in 0..2 {
                    if pass == 1 && lo == hi {
                        break;
                    }
                    let e = list[if (pass == 0) != right { lo } else { hi }];
                    let w = other[e];
                    let idx = if right { adjacent_last - g.pos[w] as i64 } else { g.pos[w] as i64 };
                    if align[v] == v && prev_idx < idx && !conflict[e] {
                        align[w] = v;
                        root[v] = root[w];
                        align[v] = root[v];
                        shift[v] = shift[w] + other_dx[e] - own_dx[e];
                        prev_idx = idx;
                    }
                }
            }
        }

        let mut block_edges = 0;
        for r in 0..=max_rank {
            let s = g.layer_start[r];
            let end = g.layer_start[r + 1];
            for i in s + 1..end {
                let a = if right { g.order[i] } else { g.order[i - 1] };
                let b = if right { g.order[i - 1] } else { g.order[i] };
                let sep = gap[a] + gap[b];
                e_from[block_edges] = root[a];
                e_to[block_edges] = root[b];
                e_w[block_edges] = if right { sep - shift[a] + shift[b] } else { sep + shift[a] - shift[b] };
                indeg[root[b]] += 1;
                out_start[root[a] + 1] += 1;
                block_edges += 1;
            }
        }
        for i in 0..n {
            out_start[i + 1] += out_start[i];
        }
        let mut fill = out_start[..n].to_vec();
        for e in 0..block_edges {
            out_e[fill[e_from[e]]] = e;
            fill[e_from[e]] += 1;
        }

        let mut count = 0;
        for i in 0..n {
            if root[i] == i && indeg[i] == 0 {
                topo[count] = i;
                count += 1;
            }
        }
        let mut q = 0;
        while q < count {
            let v = topo[q];
            q += 1;
            for k in out_start[v]..out_start[v + 1] {
                let e = out_e[k];
                let w = e_to[e];
                if x[v] + e_w[e] > x[w] {
                    x[w] = x[v] + e_w[e];
                }
                indeg[w] -= 1;
                if indeg[w] == 0 {
                    topo[count] = w;
                    count += 1;
                }
            }
        }
        for q in (0..count).rev() {
            let v = topo[q];
            let mut min = f64::INFINITY;
            for k in out_start[v]..out_start[v + 1] {
                let e = out_e[k];
                min = min.min(x[e_to[e]] - e_w[e]);
            }
            if min != f64::INFINITY && min > x[v] {
                x[v] = min;
            }
        }
        results.push((0..n).map(|i| (if right { -x[root[i]] } else { x[root[i]] }) + shift[i]).collect());
    }

    let mut narrowest = 0;
    let mut best_width = f64::INFINITY;
    let mut mins = [0.0; 4];
    let mut maxs = [0.0; 4];
    for k in 0..4 {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..n {
            let half = g.w[i] / 2.0;
            lo = lo.min(results[k][i] - half);
            hi = hi.max(results[k][i] + half);
        }
        mins[k] = lo;
        maxs[k] = hi;
        if hi - lo < best_width {
            best_width = hi - lo;
            narrowest = k;
        }
    }
    for k in 0..4 {
        let delta = if k & 1 == 0 { mins[narrowest] - mins[k] } else { maxs[narrowest] - maxs[k] };
        if delta != 0.0 {
            for v in results[k].iter_mut() {
                *v += delta;
            }
        }
    }

    // The middle of the two middle layouts.
    (0..n)
        .map(|i| {
            let mut four = [results[0][i], results[1][i], results[2][i], results[3][i]];
            four.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            (four[1] + four[2]) / 2.0
        })
        .collect()
}
