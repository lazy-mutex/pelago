use super::compound::{CEdge, CNode, CompoundResult, Dir};
use super::layered::{kind, ledge, lnode, orient, rank, LEdge, LNode, LayeredOptions};
use crate::svg::builder::js_round;
use std::collections::HashSet;

// Layout for swimlanes. Every top-level group is a lane: a band that runs the length of the
// diagram along the flow. Ranks are shared by all lanes, so a step sits level with the steps
// that happen at the same time in other lanes, and each node stays inside its own lane.

const LANE_HEAD: f64 = 34.0;
pub const LANE_PAD: f64 = 24.0;
const SWEEPS: usize = 8;
const MAX_DUMMIES: usize = 20000;
const MAX_HOPS: usize = 10000;
const ROUTE_BUDGET: usize = 2_000_000;
// Parallel lines closer than this are drawn as one.
const NEAR: f64 = 4.0;
const ROUNDS: usize = 3;
const RELAY: usize = 8;
const TURN: f64 = 20.0;
// The room a lane keeps beside a handoff along its margin, and how much of it is between the
// handoff and the lane's items.
const MARGIN: f64 = 20.0;
const LEG: f64 = 14.0;
const CLOSE: f64 = 3.0;

fn js_sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

struct Placed {
    width: f64,
    height: f64,
    routed: Vec<u8>,
    margins: Vec<u8>,
    work: usize,
}

pub fn lane_layout(
    nodes: &mut [CNode],
    edges: &mut [CEdge],
    dir: Dir,
    opt: &LayeredOptions,
    lane_pad: Option<f64>,
) -> CompoundResult {
    let lane_pad = lane_pad.unwrap_or(LANE_PAD);
    let n = nodes.len();
    if n == 0 {
        return CompoundResult { width: 0.0, height: 0.0 };
    }
    let sideways = matches!(dir, Dir::LR | Dir::RL);
    let flipped = matches!(dir, Dir::BT | Dir::RL);

    // Lanes in the order they were written. The graph lists groups last-declared first.
    let mut lanes: Vec<usize> = Vec::new();
    for i in (0..n).rev() {
        if nodes[i].parent < 0 && nodes[i].is_group {
            lanes.push(i);
        }
    }
    let lane_count = lanes.len().max(1);
    let mut lane_slot = vec![-1i32; n];
    for (k, &id) in lanes.iter().enumerate() {
        lane_slot[id] = k as i32;
    }
    let mut has_child = vec![false; n];
    for i in 0..n {
        if nodes[i].parent >= 0 {
            has_child[nodes[i].parent as usize] = true;
        }
    }

    // Room a lane keeps around its items, so that the groups drawn around them stay inside it.
    let mut lane_pads = vec![lane_pad; lane_count];
    let mut end_pad = lane_pad;

    // What gets ranked and placed: nodes, and groups that are empty and so act as nodes.
    let mut item_of = vec![-1i32; n];
    let mut real: Vec<usize> = Vec::new();
    for i in 0..n {
        if lane_slot[i] >= 0 {
            continue;
        }
        if !nodes[i].is_group || !has_child[i] {
            item_of[i] = real.len() as i32;
            real.push(i);
        }
    }

    let mut item_lane = Vec::with_capacity(real.len());
    for &i in &real {
        let mut t = i;
        let mut room = lane_pad;
        let mut hops = 0;
        while nodes[t].parent >= 0 && hops < MAX_HOPS {
            t = nodes[t].parent as usize;
            if lane_slot[t] < 0 {
                room += nodes[t].pad_x.max(nodes[t].pad_top).max(nodes[t].pad_bottom);
            }
            hops += 1;
        }
        let lane = lane_slot[t].max(0) as usize;
        if room > lane_pads[lane] {
            lane_pads[lane] = room;
        }
        if room > end_pad {
            end_pad = room;
        }
        item_lane.push(lane as i32);
    }

    let along = |c: &CNode| if sideways { c.w } else { c.h };
    let cross = |c: &CNode| if sideways { c.h } else { c.w };

    let mut lnodes: Vec<LNode> = real.iter().map(|&i| lnode(cross(&nodes[i]), along(&nodes[i]), kind::NODE)).collect();
    let mut ledges: Vec<LEdge> = Vec::new();
    let mut edge_of: Vec<usize> = Vec::new();
    let mut step: i32 = 1;
    {
        let mut seen: HashSet<u64> = HashSet::new();
        let rlen = real.len() as u64;
        for (ei, e) in edges.iter().enumerate() {
            let a = item_of[e.src];
            let b = item_of[e.dst];
            if a < 0 || b < 0 || a == b {
                continue;
            }
            let (au, bu) = (a as usize, b as usize);
            let key = (au.min(bu) as u64) * rlen + (au.max(bu) as u64);
            if e.label_w > 0.0 || seen.contains(&key) {
                step = 2;
            }
            seen.insert(key);
            ledges.push(ledge(
                au,
                bu,
                e.minlen,
                if sideways { e.label_h } else { e.label_w },
                if sideways { e.label_w } else { e.label_h },
            ));
            edge_of.push(ei);
        }
    }
    orient(&mut lnodes, &mut ledges);
    rank(&mut lnodes, &ledges, step);

    let m = ledges.len();

    // Lays the ranked items out and routes the relations. A plain relation is a handoff that takes
    // a simple route of its own, and so keeps no room through the ranks it passes.
    let place = |nodes: &mut [CNode], edges: &mut [CEdge], plain: &[u8], margins: &[u8]| -> Placed {
        let pad: Vec<f64> = lane_pads
            .iter()
            .enumerate()
            .map(|(lane, &room)| if margins[lane] != 0 { room.max(MARGIN) } else { room })
            .collect();
        let mut used = vec![0u8; lane_count];
        // Items: the real ones, then a dummy for every rank an edge passes through.
        let mut dummies: usize = 0;
        let mut span = vec![0i32; m];
        for k in 0..m {
            span[k] = (lnodes[ledges[k].head].rank - lnodes[ledges[k].tail].rank).abs();
            if plain[k] == 0 {
                dummies += (span[k] - 1).max(0) as usize;
            }
        }
        let direct = dummies > MAX_DUMMIES;
        let total = real.len() + if direct { 0 } else { dummies };
        let mut rank_arr = vec![0i32; total];
        let mut lane_arr = vec![0i32; total];
        let mut size_arr = vec![0.0f64; total];
        let mut depth_arr = vec![0.0f64; total];
        let mut dummy_arr = vec![0u8; total];
        let mut max_rank: i32 = 0;
        for k in 0..real.len() {
            rank_arr[k] = lnodes[k].rank;
            lane_arr[k] = item_lane[k];
            size_arr[k] = lnodes[k].w;
            depth_arr[k] = lnodes[k].h;
            if rank_arr[k] > max_rank {
                max_rank = rank_arr[k];
            }
        }
        let mut first_dummy = vec![-1i32; m];
        let mut label_item = vec![-1i32; m];
        let mut links: Vec<usize> = Vec::new();
        {
            let mut next = real.len();
            for k in 0..m {
                let e = &ledges[k];
                let a = if e.reversed { e.head } else { e.tail };
                let b = if e.reversed { e.tail } else { e.head };
                if direct || span[k] < 2 || plain[k] != 0 {
                    links.push(a);
                    links.push(b);
                    continue;
                }
                let ra = rank_arr[a];
                let rb = rank_arr[b];
                let mut label_rank: i32 = -1;
                if e.label_w > 0.0 {
                    label_rank = ra + ((rb - ra) >> 1);
                    if step == 2 && (label_rank & 1) == 0 {
                        label_rank -= 1;
                    }
                }
                first_dummy[k] = next as i32;
                let mut prev = a;
                for r in (ra + 1)..rb {
                    let d = next;
                    next += 1;
                    rank_arr[d] = r;
                    // The edge crosses from its first lane to its last one evenly along the way.
                    lane_arr[d] = lane_arr[a]
                        + js_round(((lane_arr[b] - lane_arr[a]) as f64 * (r - ra) as f64) / (rb - ra) as f64) as i32;
                    dummy_arr[d] = 1;
                    if r == label_rank {
                        size_arr[d] = e.label_w;
                        depth_arr[d] = e.label_h;
                        label_item[k] = d as i32;
                    }
                    links.push(prev);
                    links.push(d);
                    prev = d;
                }
                links.push(prev);
                links.push(b);
            }
        }

        let mut up_start = vec![0usize; total + 1];
        let mut down_start = vec![0usize; total + 1];
        let mut k = 0;
        while k < links.len() {
            down_start[links[k] + 1] += 1;
            up_start[links[k + 1] + 1] += 1;
            k += 2;
        }
        for i in 0..total {
            up_start[i + 1] += up_start[i];
            down_start[i + 1] += down_start[i];
        }
        let mut up = vec![0usize; links.len() / 2];
        let mut down = vec![0usize; links.len() / 2];
        {
            let mut uf = up_start[..total].to_vec();
            let mut df = down_start[..total].to_vec();
            let mut k = 0;
            while k < links.len() {
                let u = links[k];
                let v = links[k + 1];
                down[df[u]] = v;
                df[u] += 1;
                up[uf[v]] = u;
                uf[v] += 1;
                k += 2;
            }
        }

        // Order within each rank: by lane first, then by where the item's neighbours sit.
        let num_ranks = (max_rank + 1) as usize;
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); num_ranks];
        for i in 0..total {
            rows[rank_arr[i] as usize].push(i);
        }
        let mut pos = vec![0.0f64; total];
        let mut key = vec![0.0f64; total];
        let settle = |row: &mut Vec<usize>, pos: &mut [f64], key: &[f64]| {
            row.sort_by(|&a, &b| {
                lane_arr[a]
                    .cmp(&lane_arr[b])
                    .then_with(|| key[a].total_cmp(&key[b]))
                    .then_with(|| a.cmp(&b))
            });
            for (k, &item) in row.iter().enumerate() {
                pos[item] = k as f64;
            }
        };
        for row in &mut rows {
            for (k, &item) in row.iter().enumerate() {
                key[item] = k as f64;
            }
            settle(row, &mut pos, &key);
        }
        for sweep in 0..SWEEPS {
            if max_rank <= 0 {
                break;
            }
            let downward = (sweep & 1) == 0;
            for s in 1..=(max_rank as usize) {
                let r_idx = if downward { s } else { (max_rank as usize) - s };
                let (start, list) = if downward { (&up_start, &up) } else { (&down_start, &down) };
                for &i in &rows[r_idx] {
                    let mut sum = 0.0;
                    let count = start[i + 1] - start[i];
                    for k in start[i]..start[i + 1] {
                        sum += pos[list[k]];
                    }
                    key[i] = if count > 0 { sum / count as f64 } else { pos[i] };
                }
                settle(&mut rows[r_idx], &mut pos, &key);
            }
        }

        // The items of one rank that share a lane sit side by side as a run.
        let gap = |a: usize, b: usize| -> f64 {
            if dummy_arr[a] != 0 || dummy_arr[b] != 0 {
                opt.edge_sep
            } else {
                opt.node_sep
            }
        };
        let each_run = |row: &[usize], mut visit: Box<dyn FnMut(&[usize], usize, usize) + '_>| {
            let mut from = 0;
            while from < row.len() {
                let mut to = from;
                while to + 1 < row.len() && lane_arr[row[to + 1]] == lane_arr[row[from]] {
                    to += 1;
                }
                visit(row, from, to);
                from = to + 1;
            }
        };
        let run_size = |row: &[usize], from: usize, to: usize| -> f64 {
            let mut size = size_arr[row[from]];
            for k in from..to {
                size += gap(row[k], row[k + 1]) + size_arr[row[k + 1]];
            }
            size
        };

        // Lane breadth: enough for its fullest rank, and for its title.
        let mut breadth = vec![0.0f64; lane_count];
        let mut head = if !lanes.is_empty() { LANE_HEAD } else { 0.0 };
        for (k, &lane_id) in lanes.iter().enumerate() {
            let lane = &nodes[lane_id];
            breadth[k] = (lane.min_w - 2.0 * pad[k]).max(0.0);
            if lane.pad_top > head {
                head = lane.pad_top;
            }
        }
        for row in &rows {
            let mut from = 0;
            while from < row.len() {
                let mut to = from;
                while to + 1 < row.len() && lane_arr[row[to + 1]] == lane_arr[row[from]] {
                    to += 1;
                }
                let size = run_size(row, from, to);
                let l = lane_arr[row[from]] as usize;
                if size > breadth[l] {
                    breadth[l] = size;
                }
                from = to + 1;
            }
        }
        let mut lane_start = vec![0.0f64; lane_count + 1];
        for k in 0..lane_count {
            lane_start[k + 1] = lane_start[k] + breadth[k] + 2.0 * pad[k];
        }

        // Across the flow: each lane's items start centered, then line up with their neighbours in
        // the same lane as far as the lane and the items beside them allow.
        let mut c_pos = vec![0.0f64; total];
        let mut want = vec![0.0f64; total];
        for row in &rows {
            each_run(
                row,
                Box::new(|row, from, to| {
                    let lane = lane_arr[row[from]] as usize;
                    let mut at = (lane_start[lane] + pad[lane] + (lane_start[lane + 1] - pad[lane]) - run_size(row, from, to)) / 2.0;
                    for k in from..=to {
                        c_pos[row[k]] = at + size_arr[row[k]] / 2.0;
                        if k < to {
                            at += size_arr[row[k]] + gap(row[k], row[k + 1]);
                        }
                    }
                }),
            );
        }
        for pass in 0..4 {
            if max_rank <= 0 {
                break;
            }
            let downward = (pass & 1) == 0;
            for s in 1..=(max_rank as usize) {
                let r_idx = if downward { s } else { (max_rank as usize) - s };
                let (start, list) = if downward { (&up_start, &up) } else { (&down_start, &down) };
                for &i in &rows[r_idx] {
                    let mut sum = 0.0;
                    let mut count = 0usize;
                    for k in start[i]..start[i + 1] {
                        if lane_arr[list[k]] != lane_arr[i] {
                            continue;
                        }
                        sum += c_pos[list[k]];
                        count += 1;
                    }
                    want[i] = if count > 0 { sum / count as f64 } else { c_pos[i] };
                }
                let row = &rows[r_idx];
                each_run(
                    row,
                    Box::new(|row, from, to| {
                        let lane = lane_arr[row[from]] as usize;
                        let mut floor = lane_start[lane] + pad[lane];
                        for k in from..=to {
                            let i = row[k];
                            c_pos[i] = want[i].max(floor + size_arr[i] / 2.0);
                            if k < to {
                                floor = c_pos[i] + size_arr[i] / 2.0 + gap(i, row[k + 1]);
                            }
                        }
                        let mut ceil = lane_start[lane + 1] - pad[lane];
                        for k in (from..=to).rev() {
                            let i = row[k];
                            c_pos[i] = c_pos[i].min(ceil - size_arr[i] / 2.0);
                            if k > from {
                                ceil = c_pos[i] - size_arr[i] / 2.0 - gap(row[k - 1], i);
                            }
                        }
                    }),
                );
            }
        }

        // Along the flow: one band per rank, after the lane titles.
        let mut band_start = vec![0.0f64; num_ranks];
        let mut band_end = vec![0.0f64; num_ranks];
        let full = opt.rank_sep / step as f64;
        let mut sep = vec![full; num_ranks];
        if plain.contains(&1) {
            sep.fill(full / CLOSE);
            for k in 0..m {
                let e = &ledges[k];
                let a = if e.reversed { e.head } else { e.tail };
                let b = if e.reversed { e.tail } else { e.head };
                if plain[k] != 0 {
                    if e.label_h <= 0.0 || span[k] > step {
                        continue;
                    }
                    let room = full.min((e.label_h + 2.0 * TURN - depth_arr[a].min(depth_arr[b]) / 2.0) / span[k] as f64);
                    for r in rank_arr[a]..rank_arr[b] {
                        if room > sep[r as usize] {
                            sep[r as usize] = room;
                        }
                    }
                    continue;
                }
                let d0 = first_dummy[k];
                if d0 < 0 || span[k] <= step * e.minlen {
                    for r in rank_arr[a]..rank_arr[b] {
                        sep[r as usize] = full;
                    }
                    continue;
                }
                for d in (d0 - 1)..(d0 + span[k] - 1) {
                    let u = if d < d0 { a } else { d as usize };
                    let v = if d + 1 < d0 + span[k] - 1 { (d + 1) as usize } else { b };
                    if lane_arr[u] != lane_arr[v] || (c_pos[u] - c_pos[v]).abs() >= 0.5 {
                        sep[rank_arr[u] as usize] = full;
                    }
                }
            }
        }
        let mut at = head + end_pad;
        for r in 0..num_ranks {
            let mut depth = 0.0f64;
            for &i in &rows[r] {
                if depth_arr[i] > depth {
                    depth = depth_arr[i];
                }
            }
            band_start[r] = at;
            band_end[r] = at + depth;
            at += depth + if r + 1 == num_ranks { 0.0 } else { sep[r] };
        }
        let length = at + end_pad;
        let breadth_total = lane_start[lane_count];
        let flip = |a: f64| -> f64 { if flipped { head + length - a } else { a } };
        let px = |c: f64, a: f64| -> f64 { if sideways { flip(a) } else { c } };
        let py = |c: f64, a: f64| -> f64 { if sideways { c } else { flip(a) } };
        let axis = if sideways { 1.0 } else { 0.0 };

        for k in 0..real.len() {
            let node = &mut nodes[real[k]];
            let r = rank_arr[k] as usize;
            let a = (band_start[r] + band_end[r]) / 2.0;
            node.x = px(c_pos[k], a);
            node.y = py(c_pos[k], a);
        }
        for (k, &lane_id) in lanes.iter().enumerate() {
            let lane = &mut nodes[lane_id];
            let c = (lane_start[k] + lane_start[k + 1]) / 2.0;
            let size = lane_start[k + 1] - lane_start[k];
            lane.x = if sideways { length / 2.0 } else { c };
            lane.y = if sideways { c } else { length / 2.0 };
            lane.w = if sideways { length } else { size };
            lane.h = if sideways { size } else { length };
            lane.pad_top = head;
        }

        // Groups inside a lane are drawn around their members, deepest first.
        let mut depth_of = vec![0i32; n];
        let mut inner: Vec<usize> = Vec::new();
        for i in 0..n {
            if !nodes[i].is_group || lane_slot[i] >= 0 || item_of[i] >= 0 {
                continue;
            }
            let mut d = 0i32;
            let mut p = nodes[i].parent;
            while p >= 0 && (d as usize) < MAX_HOPS {
                d += 1;
                p = nodes[p as usize].parent;
            }
            depth_of[i] = d;
            inner.push(i);
        }
        inner.sort_by(|&a, &b| depth_of[b].cmp(&depth_of[a]).then_with(|| a.cmp(&b)));
        let mut bbox = vec![[f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY]; n];
        let grow = |bbox: &mut [[f64; 4]], g: usize, cx: f64, cy: f64, cw: f64, ch: f64| {
            if cx - cw / 2.0 < bbox[g][0] {
                bbox[g][0] = cx - cw / 2.0;
            }
            if cy - ch / 2.0 < bbox[g][1] {
                bbox[g][1] = cy - ch / 2.0;
            }
            if cx + cw / 2.0 > bbox[g][2] {
                bbox[g][2] = cx + cw / 2.0;
            }
            if cy + ch / 2.0 > bbox[g][3] {
                bbox[g][3] = cy + ch / 2.0;
            }
        };
        for &i in &real {
            let p = nodes[i].parent;
            if p >= 0 && lane_slot[p as usize] < 0 {
                grow(&mut bbox, p as usize, nodes[i].x, nodes[i].y, nodes[i].w, nodes[i].h);
            }
        }
        for &g in &inner {
            if bbox[g][0] == f64::INFINITY {
                continue;
            }
            let (b0, b1, b2, b3) = (bbox[g][0], bbox[g][1], bbox[g][2], bbox[g][3]);
            let w = (b2 - b0 + 2.0 * nodes[g].pad_x).max(nodes[g].min_w);
            nodes[g].x = (b0 + b2) / 2.0;
            nodes[g].w = w;
            nodes[g].y = (b1 - nodes[g].pad_top + b3 + nodes[g].pad_bottom) / 2.0;
            nodes[g].h = b3 - b1 + nodes[g].pad_top + nodes[g].pad_bottom;
            let p = nodes[g].parent;
            if p >= 0 && lane_slot[p as usize] < 0 {
                grow(&mut bbox, p as usize, nodes[g].x, nodes[g].y, nodes[g].w, nodes[g].h);
            }
        }

        for e in edges.iter_mut() {
            e.route.clear();
        }
        for k in 0..m {
            let e = &ledges[k];
            let out = &mut edges[edge_of[k]];
            let a = if e.reversed { e.head } else { e.tail };
            let b = if e.reversed { e.tail } else { e.head };
            let ra = rank_arr[a] as usize;
            let rb = rank_arr[b] as usize;
            let mut pts: Vec<f64> = vec![c_pos[a], band_end[ra] - (band_end[ra] - band_start[ra] - depth_arr[a]) / 2.0];
            let d0 = first_dummy[k];
            if d0 >= 0 {
                for d in (d0 as usize)..(d0 + span[k] - 1) as usize {
                    let rd = rank_arr[d] as usize;
                    pts.push(c_pos[d]);
                    pts.push(band_start[rd]);
                    if band_end[rd] > band_start[rd] {
                        pts.push(c_pos[d]);
                        pts.push(band_end[rd]);
                    }
                }
            }
            pts.push(c_pos[b]);
            pts.push(band_start[rb] + (band_end[rb] - band_start[rb] - depth_arr[b]) / 2.0);
            let mut route: Vec<f64> = Vec::with_capacity((pts.len() / 2) * 3);
            let mut p = 0;
            while p < pts.len() {
                route.push(px(pts[p], pts[p + 1]));
                route.push(py(pts[p], pts[p + 1]));
                route.push(axis);
                p += 2;
            }
            if e.reversed && route.len() >= 6 {
                let mut i = 0usize;
                let mut j = route.len() - 3;
                while i < j {
                    route.swap(i, j);
                    route.swap(i + 1, j + 1);
                    i += 3;
                    if j < 3 {
                        break;
                    }
                    j -= 3;
                }
            }
            let label = label_item[k];
            if label >= 0 {
                let rl = rank_arr[label as usize] as usize;
                let mid = (band_start[rl] + band_end[rl]) / 2.0;
                out.label_x = px(c_pos[label as usize], mid);
                out.label_y = py(c_pos[label as usize], mid);
            } else {
                let last = route.len() - 3;
                out.label_x = (route[0] + route[last]) / 2.0;
                out.label_y = (route[1] + route[last + 1]) / 2.0;
            }
            out.route = route;
        }

        // An edge that starts or ends at a lane or a group runs straight to its border.
        for e in edges.iter_mut() {
            if !e.route.is_empty() || e.src == e.dst {
                continue;
            }
            let (ax, ay) = border(&nodes[e.src], &nodes[e.dst]);
            let (bx, by) = border(&nodes[e.dst], &nodes[e.src]);
            let flat = if (bx - ax).abs() > (by - ay).abs() { 1.0 } else { 0.0 };
            e.route = vec![ax, ay, flat, bx, by, flat];
            e.label_x = (ax + bx) / 2.0;
            e.label_y = (ay + by) / 2.0;
        }

        // Try single-corner and lane-margin routes for cross-lane handoffs.
        let mut routed = vec![0u8; m];
        let route_points: usize = edges.iter().map(|edge| edge.route.len() / 3).sum();
        let work = m * (real.len() + edges.len() + route_points);
        if work <= ROUTE_BUDGET {
            let amid = |i: usize| -> f64 {
                let r = rank_arr[i] as usize;
                (band_start[r] + band_end[r]) / 2.0
            };
            let mut labels: Vec<[f64; 2]> = edges
                .iter()
                .map(|e| [if sideways { e.label_y } else { e.label_x }, flip(if sideways { e.label_x } else { e.label_y })])
                .collect();
            let overlaps = |c0: f64, a0: f64, c1: f64, a1: f64, lc: f64, la: f64, cw: f64, ah: f64| -> bool {
                c1 > lc - cw / 2.0 && c0 < lc + cw / 2.0 && a1 > la - ah / 2.0 && a0 < la + ah / 2.0
            };
            let clear = |edges: &[CEdge], labels: &[[f64; 2]], c0: f64, a0: f64, c1: f64, a1: f64, src: isize, dst: isize, ei: usize| -> bool {
                for i in 0..real.len() {
                    if i as isize != src
                        && i as isize != dst
                        && overlaps(c0, a0, c1, a1, c_pos[i], amid(i), size_arr[i] + 8.0, depth_arr[i] + 8.0)
                    {
                        return false;
                    }
                }
                for (i, e) in edges.iter().enumerate() {
                    if i != ei
                        && e.label_w > 0.0
                        && e.src != e.dst
                        && overlaps(
                            c0,
                            a0,
                            c1,
                            a1,
                            labels[i][0],
                            labels[i][1],
                            if sideways { e.label_h } else { e.label_w },
                            if sideways { e.label_w } else { e.label_h },
                        )
                    {
                        return false;
                    }
                }
                true
            };
            let crossings = |edges: &[CEdge], x0: f64, y0: f64, x1: f64, y1: f64, ei: usize| -> usize {
                let mut count = 0usize;
                for (i, edge) in edges.iter().enumerate() {
                    if i == ei {
                        continue;
                    }
                    let route = &edge.route;
                    let mut j = 3;
                    while j < route.len() {
                        if route[j - 3].max(route[j]) > x0
                            && route[j - 3].min(route[j]) < x1
                            && route[j - 2].max(route[j + 1]) > y0
                            && route[j - 2].min(route[j + 1]) < y1
                        {
                            count += 1;
                            break;
                        }
                        j += 3;
                    }
                }
                count
            };
            let meets = |edges: &[CEdge], x: f64, y: f64, ei: usize| -> usize {
                let mut count = 0usize;
                for (i, edge) in edges.iter().enumerate() {
                    let route = &edge.route;
                    if i == ei || route.is_empty() {
                        continue;
                    }
                    let last = route.len() - 3;
                    if ((route[0] - x).abs() < 1.0 && (route[1] - y).abs() < 1.0)
                        || ((route[last] - x).abs() < 1.0 && (route[last + 1] - y).abs() < 1.0)
                    {
                        count += 1;
                    }
                }
                count
            };
            let retraces = |edges: &[CEdge], x0: f64, y0: f64, x1: f64, y1: f64, ei: usize| -> bool {
                let upright = (x1 - x0).abs() < 0.5;
                let (lo, hi) = if upright { (y0.min(y1), y0.max(y1)) } else { (x0.min(x1), x0.max(x1)) };
                let line = if upright { x0 } else { y0 };
                for (i, other) in edges.iter().enumerate() {
                    if i == ei || other.src == edges[ei].src || other.dst == edges[ei].dst {
                        continue;
                    }
                    let route = &other.route;
                    let mut j = 3;
                    while j < route.len() {
                        let u_off = if upright { 0 } else { 1 };
                        let v_off = if upright { 1 } else { 0 };
                        let u0 = route[j - 3 + u_off];
                        let u1 = route[j + u_off];
                        if (u0 - line).abs() >= NEAR || (u1 - line).abs() >= NEAR {
                            j += 3;
                            continue;
                        }
                        let v0 = route[j - 3 + v_off];
                        let v1 = route[j + v_off];
                        if hi.min(v0.max(v1)) - lo.max(v0.min(v1)) > 1.0 {
                            return true;
                        }
                        j += 3;
                    }
                }
                false
            };

            struct Around {
                pts: Vec<f64>,
                cost: f64,
                lane: usize,
            }

            for k in 0..m {
                let e = &ledges[k];
                let ei = edge_of[k];
                let a = e.tail;
                let b = e.head;
                if lane_arr[a] == lane_arr[b] || (c_pos[b] - c_pos[a]).abs() < 1.0 {
                    continue;
                }
                let (ac, bc, aa, ba) = (c_pos[a], c_pos[b], amid(a), amid(b));
                let (dc, da) = (js_sign(bc - ac), js_sign(ba - aa));

                let try_attempt = |edges: &mut [CEdge], labels: &mut [[f64; 2]], routed: &mut [u8], pts: &[f64], margin: f64| -> bool {
                    let mut p = 2;
                    while p < pts.len() {
                        if !clear(
                            edges,
                            labels,
                            pts[p - 2].min(pts[p]) - margin,
                            pts[p - 1].min(pts[p + 1]) - margin,
                            pts[p - 2].max(pts[p]) + margin,
                            pts[p - 1].max(pts[p + 1]) + margin,
                            a as isize,
                            b as isize,
                            ei,
                        ) {
                            return false;
                        }
                        if retraces(edges, px(pts[p - 2], pts[p - 1]), py(pts[p - 2], pts[p - 1]), px(pts[p], pts[p + 1]), py(pts[p], pts[p + 1]), ei) {
                            return false;
                        }
                        p += 2;
                    }
                    let mut lc = 0.0;
                    let mut la = 0.0;
                    let mut longest = -1.0;
                    let out_label_w = edges[ei].label_w;
                    let out_label_h = edges[ei].label_h;
                    if out_label_w > 0.0 {
                        let cw = e.label_w;
                        let ah = e.label_h;
                        let mut p = 2;
                        while p < pts.len() {
                            let cc = (pts[p - 2] + pts[p]) / 2.0;
                            let ca = (pts[p - 1] + pts[p + 1]) / 2.0;
                            let across = (pts[p] - pts[p - 2]).abs();
                            let along = (pts[p + 1] - pts[p - 1]).abs();
                            let run = across + along;
                            if run <= longest || (if across > 0.0 { across < cw + 8.0 } else { along < ah + 8.0 }) {
                                p += 2;
                                continue;
                            }
                            if cc - cw / 2.0 < 0.0 || cc + cw / 2.0 > breadth_total || ca - ah / 2.0 < 0.0 || ca + ah / 2.0 > length {
                                p += 2;
                                continue;
                            }
                            if !clear(edges, labels, cc - cw / 2.0, ca - ah / 2.0, cc + cw / 2.0, ca + ah / 2.0, -1, -1, ei) {
                                p += 2;
                                continue;
                            }
                            let lx = px(cc, ca);
                            let ly = py(cc, ca);
                            if crossings(edges, lx - out_label_w / 2.0, ly - out_label_h / 2.0, lx + out_label_w / 2.0, ly + out_label_h / 2.0, ei) > 0 {
                                p += 2;
                                continue;
                            }
                            lc = cc;
                            la = ca;
                            longest = run;
                            p += 2;
                        }
                        if longest < 0.0 {
                            return false;
                        }
                    }
                    let mut way = vec![pts[0], pts[1]];
                    let mut p = 2;
                    while p < pts.len() {
                        let dx = pts[p] - pts[p - 2];
                        let dy = pts[p + 1] - pts[p - 1];
                        let run = dx.abs() + dy.abs();
                        let first = if p > 2 { TURN } else { 0.0 };
                        let last = if p + 2 < pts.len() { TURN } else { 0.0 };
                        if run > first + last + 1.0 {
                            if first > 0.0 {
                                way.push(pts[p - 2] + (dx / run) * first);
                                way.push(pts[p - 1] + (dy / run) * first);
                            }
                            if last > 0.0 {
                                way.push(pts[p] - (dx / run) * last);
                                way.push(pts[p + 1] - (dy / run) * last);
                            }
                        }
                        way.push(pts[p]);
                        way.push(pts[p + 1]);
                        p += 2;
                    }
                    let out = &mut edges[ei];
                    out.route.clear();
                    let mut p = 0;
                    while p < way.len() {
                        let crosswise = if p == 0 { way[0] != way[2] } else { way[p] != way[p - 2] };
                        out.route.push(px(way[p], way[p + 1]));
                        out.route.push(py(way[p], way[p + 1]));
                        out.route.push(if crosswise { 1.0 - axis } else { axis });
                        p += 2;
                    }
                    if out.label_w > 0.0 {
                        out.label_x = px(lc, la);
                        out.label_y = py(lc, la);
                        labels[ei] = [lc, la];
                    }
                    routed[k] = 1;
                    true
                };

                if try_attempt(&mut *edges, &mut labels, &mut routed, &[ac + dc * size_arr[a] / 2.0, aa, bc, aa, bc, ba - da * depth_arr[b] / 2.0], 2.0) {
                    continue;
                }
                if try_attempt(&mut *edges, &mut labels, &mut routed, &[ac, aa + da * depth_arr[a] / 2.0, ac, ba, bc - dc * size_arr[b] / 2.0, ba], 2.0) {
                    continue;
                }
                if work > ROUTE_BUDGET / RELAY {
                    continue;
                }
                let mut around: Vec<Around> = Vec::new();
                let cand_lanes = if lane_arr[a] < lane_arr[b] {
                    [lane_arr[b] as usize, lane_arr[a] as usize]
                } else {
                    [lane_arr[a] as usize, lane_arr[b] as usize]
                };
                for lane in cand_lanes {
                    let inset = (pad[lane] - LEG).max((pad[lane] / 2.0).min(MARGIN - LEG));
                    for ch in [lane_start[lane] + inset, lane_start[lane + 1] - inset] {
                        if (ch - ac).abs() < size_arr[a] / 2.0 + 4.0 || (ch - bc).abs() < size_arr[b] / 2.0 + 4.0 {
                            continue;
                        }
                        let pts = vec![
                            ac + js_sign(ch - ac) * size_arr[a] / 2.0,
                            aa,
                            ch,
                            aa,
                            ch,
                            ba,
                            bc + js_sign(ch - bc) * size_arr[b] / 2.0,
                            ba,
                        ];
                        let mut cost =
                            (meets(edges, px(pts[0], pts[1]), py(pts[0], pts[1]), ei) + meets(edges, px(pts[6], pts[7]), py(pts[6], pts[7]), ei)) as f64
                                * 1e6;
                        let mut p = 2;
                        while p < pts.len() {
                            let (x0, y0) = (px(pts[p - 2], pts[p - 1]), py(pts[p - 2], pts[p - 1]));
                            let (x1, y1) = (px(pts[p], pts[p + 1]), py(pts[p], pts[p + 1]));
                            cost += crossings(edges, x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1), ei) as f64 * 1e6
                                + (x1 - x0).abs()
                                + (y1 - y0).abs();
                            p += 2;
                        }
                        around.push(Around { pts, cost, lane });
                    }
                }
                around.sort_by(|p, q| p.cost.total_cmp(&q.cost));
                for way in &around {
                    if !try_attempt(&mut *edges, &mut labels, &mut routed, &way.pts, 1.0) {
                        continue;
                    }
                    used[way.lane] = 1;
                    break;
                }
            }
        }

        Placed {
            width: if sideways { length } else { breadth_total },
            height: if sideways { breadth_total } else { length },
            routed,
            margins: used,
            work,
        }
    };

    let mut plain = vec![0u8; m];
    let mut margins = vec![0u8; lane_count];
    let mut placed = place(nodes, edges, &plain, &margins);
    for round in 0..ROUNDS {
        if placed.work > ROUTE_BUDGET / RELAY {
            break;
        }
        let next: Vec<u8> = if round == 0 {
            placed.routed.clone()
        } else {
            plain.iter().enumerate().map(|(k, &on)| on & placed.routed[k]).collect()
        };
        if next == plain && placed.margins == margins {
            break;
        }
        plain = next;
        margins = placed.margins.clone();
        placed = place(nodes, edges, &plain, &margins);
    }
    if plain.iter().enumerate().any(|(k, &on)| on != 0 && placed.routed[k] == 0) {
        placed = place(nodes, edges, &vec![0u8; m], &vec![0u8; lane_count]);
    }
    CompoundResult { width: placed.width, height: placed.height }
}

// The point where the line from one box's center to another's leaves the first box. When the
// other center is inside the box, the point is on the nearest side instead.
fn border(from: &CNode, to: &CNode) -> (f64, f64) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let hw = (from.w / 2.0).max(0.5);
    let hh = (from.h / 2.0).max(0.5);
    if dx.abs() < hw && dy.abs() < hh {
        let to_side = hw - dx.abs();
        let to_end = hh - dy.abs();
        return if to_side < to_end {
            (from.x + if dx < 0.0 { -hw } else { hw }, to.y)
        } else {
            (to.x, from.y + if dy < 0.0 { -hh } else { hh })
        };
    }
    let scale = (if dx != 0.0 { hw / dx.abs() } else { f64::INFINITY }).min(if dy != 0.0 { hh / dy.abs() } else { f64::INFINITY });
    (from.x + dx * scale, from.y + dy * scale)
}
