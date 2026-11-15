use super::position::half_room;
use super::{end, is_dummy, kind, Graph, LNode, LayeredOptions, BEND_SNAP, IN_LINE, LEAN_CLEAR, MIN_PITCH, PORT_PITCH, SNAP, TWIN};

// The ends of edges that may move along one side of a node (0 the side its edges arrive at, 1 the
// other), ordered by their opposite nodes to avoid crossings. `ids` and `room` must start empty.
// `ids` receives the ends; `room` receives their minimum gaps. Returns the sum of the gaps.
// When there is too little room for a full pitch between each pair, the edges that leave side by
// side move closer together, since they have no arrowheads to keep apart. With less room still,
// neighbours that run the same way share a place, which keeps an arrowhead apart from an edge
// that leaves beside it.
fn side_ends(g: &Graph, v: usize, side: usize, span: f64, kinds: &[u8], ids: &mut Vec<usize>, room: &mut Vec<f64>) -> f64 {
    let (start, list, other) = if side == 0 { (&g.pred_start, &g.pred_e, &g.ef) } else { (&g.succ_start, &g.succ_e, &g.et) };
    for k in start[v]..start[v + 1] {
        if kinds[list[k]] != end::HELD {
            ids.push(list[k]);
        }
    }
    if ids.len() < 2 {
        return 0.0;
    }
    ids.sort_by(|&a, &b| g.pos[other[a]].cmp(&g.pos[other[b]]).then(a.cmp(&b)));
    let mut fill = |share: &dyn Fn(bool, bool) -> f64| -> f64 {
        let mut total = 0.0;
        room.clear();
        room.push(0.0);
        for k in 1..ids.len() {
            let same = kinds[ids[k]] == kinds[ids[k - 1]];
            let r = share(same && kinds[ids[k]] == end::LEAVES, same);
            room.push(r);
            total += r;
        }
        total
    };
    let mut total = fill(&|_, _| 1.0);
    if span / total < MIN_PITCH {
        total = fill(&|leaves, _| if leaves { 0.5 } else { 1.0 });
    }
    if span / total < MIN_PITCH {
        total = fill(&|_, same| if same { 0.0 } else { 1.0 });
    }
    if total == 0.0 {
        return 0.0;
    }
    let pitch = PORT_PITCH.min(span / total);
    for r in room.iter_mut().skip(1) {
        *r *= pitch;
    }
    total * pitch
}

// Gives the edges that meet one side of a node places of their own along it, around its middle.
pub(super) fn spread(g: &mut Graph, nodes: &[LNode], efk: &[u8], etk: &[u8]) {
    for v in 0..g.real {
        let span = nodes[v].span.min(g.w[v]);
        if !(span > 0.0) || nodes[v].kind != kind::NODE {
            continue;
        }
        for side in 0..2 {
            let mut ids = Vec::new();
            let mut room = Vec::new();
            let total = side_ends(g, v, side, span, if side == 0 { etk } else { efk }, &mut ids, &mut room);
            if total == 0.0 {
                continue;
            }
            let dx = if side == 0 { &mut g.etd } else { &mut g.efd };
            let mut at = -total / 2.0;
            for k in 0..ids.len() {
                at += room[k];
                dx[ids[k]] = at;
            }
        }
    }
}

fn is_empty(empty: Option<&[u8]>, r: usize) -> bool {
    empty.is_some_and(|e| e[r] == 1)
}

// The segment of an edge that reaches the next point held in place, on the way up from segment
// `e` (side 0) or down: past the bends in ranks that hold nothing else.
pub(super) fn follow(g: &Graph, empty: Option<&[u8]>, mut e: usize, side: usize) -> usize {
    if empty.is_none() {
        return e;
    }
    for _ in 0..100000 {
        let u = if side == 0 { g.ef[e] } else { g.et[e] };
        if g.kind[u] != kind::DUMMY || !is_empty(empty, g.rank[u]) {
            break;
        }
        e = if side == 0 { g.pred_e[g.pred_start[u]] } else { g.succ_e[g.succ_start[u]] };
    }
    e
}

// Aligns border ports with the ends on their nodes so edges run straight across the gap.
// Ports are kept `edge_sep` apart while placed, more than ends on a short side get, so
// without this an edge turns outside the border and again in the gap.
// Leaves a rank alone unless every port can move without passing or meeting its neighbors.
pub(super) fn align_ports(g: &Graph, xs: &mut [f64], empty: Option<&[u8]>) {
    for (r, k) in [(0, kind::START_PORT), (g.max_rank, kind::END_PORT)] {
        let s = g.layer_start[r];
        let e_end = g.layer_start[r + 1];
        let mut to: Vec<f64> = Vec::new();
        for i in s..e_end {
            let v = g.order[i];
            let start = k == kind::START_PORT;
            let degree = if start { g.succ_start[v + 1] - g.succ_start[v] } else { g.pred_start[v + 1] - g.pred_start[v] };
            if g.kind[v] != k || degree != 1 {
                break;
            }
            let e = follow(g, empty, if start { g.succ_e[g.succ_start[v]] } else { g.pred_e[g.pred_start[v]] }, if start { 1 } else { 0 });
            let at = if start { xs[g.et[e]] + g.etd[e] } else { xs[g.ef[e]] + g.efd[e] };
            if to.last().is_some_and(|&last| at < last + TWIN) {
                break;
            }
            to.push(at);
        }
        if to.is_empty() || to.len() != e_end - s {
            continue;
        }
        for i in s..e_end {
            xs[g.order[i]] = to[i - s];
        }
    }
}

// How many ends that may move along it each node has on the side its edges arrive at, and on
// the other. An end that has a side to itself keeps to the middle; ends that share one cannot
// all be there, so those may move.
pub(super) fn free_ends(g: &Graph, efk: &[u8], etk: &[u8]) -> [Vec<i32>; 2] {
    let mut free = [vec![0i32; g.n], vec![0i32; g.n]];
    for e in 0..g.ef.len() {
        if etk[e] != end::HELD {
            free[0][g.et[e]] += 1;
        }
        if efk[e] != end::HELD {
            free[1][g.ef[e]] += 1;
        }
    }
    free
}

// How far along the flow the handles of a plain S reach, as a share of the way it travels along
// it. Half gives an even S. One that runs more than eight times as far across as along turns
// sooner and crosses in a straighter line, down to a quarter at sixteen times. The layout uses
// this to tell how near two curves will pass, and the drawing to draw them.
pub fn ease(across: f64, along: f64) -> f64 {
    let across = if across == 0.0 { 1.0 } else { across.abs() };
    ((4.0 * along.abs()) / across).min(0.5).max(0.25)
}

// The place between `lo` and `hi` that is in line with one of two ends, or else nearest the first choice.
fn choose(x: f64, up: (f64, i32), down: (f64, i32), lo: f64, hi: f64) -> f64 {
    let up_first = if up.1 != down.1 { up.1 < down.1 } else { (up.0 - x).abs() <= (down.0 - x).abs() };
    let choices = if up_first { [up.0, down.0] } else { [down.0, up.0] };
    choices.iter().copied().find(|&c| c >= lo && c <= hi).unwrap_or_else(|| lo.max(hi.min(choices[0])))
}

// Brings the labels and bends of each edge into line with an end of the edge, so that it runs
// straight from that end and turns once, at the other: with the end that has fewer edges beside
// it on its node, which is the one nearer the middle of its side, or else with the other, and
// as near to that as what is beside them in their ranks leaves room. The other end may then move
// along its node to meet the line. Bends in ranks that hold nothing else are left to `straighten`.
// An edge with no one line that all of its labels and bends have room on has each brought into
// line with the nearest point above or below that is held in place.
pub(super) fn align_labels(g: &Graph, nodes: &[LNode], xs: &mut [f64], free: &[Vec<i32>; 2], opt: &LayeredOptions, empty: Option<&[u8]>) {
    let sep = opt.edge_sep;
    let placed = |v: usize| g.kind[v] == kind::LABEL || (g.kind[v] == kind::DUMMY && !is_empty(empty, g.rank[v]));
    // Where segment `e` ends on the way up (side 0) or down, and how many ends share that side of
    // the node there: none for a point that is not on a node.
    let end_of = |xs: &[f64], e: usize, side: usize| -> (f64, i32) {
        let u = if side == 0 { g.ef[e] } else { g.et[e] };
        let x = if side == 0 { xs[u] + g.efd[e] } else { xs[u] + g.etd[e] };
        (x, if u < g.real && nodes[u].kind == kind::NODE { free[1 - side][u] } else { 0 })
    };
    // The room the neighbours in its rank leave a label or bend: a full gap from a node or between
    // two labels, half of one between a label and a line that passes it.
    let room = |xs: &[f64], v: usize| -> (f64, f64) {
        let s = g.layer_start[g.rank[v]];
        let i = s + g.pos[v];
        let clear = |other: usize| {
            g.w[other] / 2.0
                + g.w[v] / 2.0
                + if g.kind[other] == kind::NODE {
                    opt.node_sep / 2.0 + sep / 2.0
                } else if g.kind[other] == g.kind[v] {
                    sep
                } else {
                    sep / 2.0
                }
        };
        (
            if i > s { xs[g.order[i - 1]] + clear(g.order[i - 1]) } else { f64::NEG_INFINITY },
            if i + 1 < g.layer_start[g.rank[v] + 1] { xs[g.order[i + 1]] - clear(g.order[i + 1]) } else { f64::INFINITY },
        )
    };
    let up_seg = |v: usize| g.pred_e[g.pred_start[v]];
    let down_seg = |v: usize| g.succ_e[g.succ_start[v]];
    let mut done = vec![false; g.n];
    let mut chain: Vec<usize> = Vec::new();
    for r in 1..g.max_rank {
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            if !placed(g.order[i]) || done[g.order[i]] {
                continue;
            }
            // The edge's labels and bends from its first node to its last, and the room they share.
            let mut top = g.order[i];
            while is_dummy(g.kind[g.ef[up_seg(top)]]) {
                top = g.ef[up_seg(top)];
            }
            let mut bottom = top;
            let mut lo = f64::NEG_INFINITY;
            let mut hi = f64::INFINITY;
            chain.clear();
            let mut d = top;
            while is_dummy(g.kind[d]) {
                bottom = d;
                if placed(d) {
                    done[d] = true;
                    chain.push(d);
                    let (least, most) = room(xs, d);
                    lo = lo.max(least);
                    hi = hi.min(most);
                }
                d = g.et[down_seg(d)];
            }
            if lo <= hi {
                let x = choose(xs[chain[0]], end_of(xs, up_seg(top), 0), end_of(xs, down_seg(bottom), 1), lo, hi);
                for &d in &chain {
                    xs[d] = x;
                }
                continue;
            }
            for &d in &chain {
                let (least, most) = room(xs, d);
                if least > most {
                    continue;
                }
                xs[d] = choose(xs[d], end_of(xs, follow(g, empty, up_seg(d), 0), 0), end_of(xs, follow(g, empty, down_seg(d), 1), 1), least, most);
            }
        }
    }
}

struct Side {
    v: usize,
    side: usize,
    span: f64,
    ids: Vec<usize>,
    room: Vec<f64>,
    all: Vec<usize>,
}

// Once the nodes have their places, moves an end along its side when that lets its edge run
// straight: when the point the edge runs to, or some part of the side it runs to, is in line
// with the end's own side. Any other end leans towards the point its edge runs to, where its
// side is long enough, and an end with a side to itself stays in the middle. On a group, the
// ends keep clear of the points where other edges pass through its border.
pub(super) fn settle(g: &mut Graph, nodes: &[LNode], xs: &[f64], efk: &[u8], etk: &[u8], free: &[Vec<i32>; 2], empty: Option<&[u8]>) {
    // End order and spacing stay fixed across passes; held ends divide free ends into runs.
    let mut sides: Vec<Side> = Vec::new();
    for v in 0..g.real {
        let span = nodes[v].span.min(g.w[v]);
        if !(span > 0.0) || nodes[v].kind != kind::NODE {
            continue;
        }
        for side in 0..2 {
            let mut ids = Vec::new();
            let mut room = Vec::new();
            side_ends(g, v, side, span, if side == 0 { etk } else { efk }, &mut ids, &mut room);
            if ids.is_empty() {
                continue;
            }
            if ids.len() == 1 {
                room.clear();
                room.push(0.0);
            }
            let (start, list, other) = if side == 0 { (&g.pred_start, &g.pred_e, &g.ef) } else { (&g.succ_start, &g.succ_e, &g.et) };
            let mut all: Vec<usize> = (start[v]..start[v + 1]).map(|k| list[k]).collect();
            all.sort_by(|&a, &b| g.pos[other[a]].cmp(&g.pos[other[b]]).then(a.cmp(&b)));
            sides.push(Side { v, side, span, ids, room, all });
        }
    }
    // The offsets are changed while the rest of the graph is read.
    let mut efd = std::mem::take(&mut g.efd);
    let mut etd = std::mem::take(&mut g.etd);
    // Where the end of segment `e` on node `v` should be, from the middle of its side. The edge is
    // followed up from the segment (side 0) or down, past the bends in ranks that hold nothing
    // else, to where it next comes to a point that is held in place or to another node's side.
    let wanted = |efd: &[f64], etd: &[f64], e: usize, side: usize, v: usize, span: f64, again: bool| -> f64 {
        if free[side][v] < 2 {
            return 0.0;
        }
        let e = follow(g, empty, e, side);
        let u = if side == 0 { g.ef[e] } else { g.et[e] };
        let point = if side == 0 { xs[u] + efd[e] } else { xs[u] + etd[e] };
        // Once the other end has its place, this one comes into line with it if it can.
        if again && (point - xs[v]).abs() <= span / 2.0 + IN_LINE {
            return (-span / 2.0).max((span / 2.0).min(point - xs[v]));
        }
        // The part of the other side that its end may move along, if it may move.
        let mut middle = point;
        let mut width = 0.0;
        if u < g.real && nodes[u].kind == kind::NODE && (if side == 0 { efk } else { etk })[e] != end::HELD && free[1 - side][u] >= 2 {
            width = nodes[u].span.min(g.w[u]).max(0.0);
            if width > 0.0 {
                middle = xs[u];
            }
        }
        let lo = (xs[v] - span / 2.0).max(middle - width / 2.0);
        let hi = (xs[v] + span / 2.0).min(middle + width / 2.0);
        // Two sides with no part in line: the end leans towards the other, if its side is long enough.
        if lo > hi + IN_LINE {
            let lean = (span / 2.0 - LEAN_CLEAR).max(0.0);
            return (-lean).max(lean.min(point - xs[v]));
        }
        // Both ends settle on one place in the part the two sides share: nearer the middle of the narrower.
        let place = (xs[v] * width + middle * span) / (width + span);
        lo.max(hi.min(place)) - xs[v]
    };
    // The ends of an edge follow each other, so the nodes are gone over until none moves: a few
    // times at most, in practice.
    let mut moved = true;
    let mut pass = 0;
    while moved && pass < 6 {
        moved = false;
        for s in &sides {
            let (v, side, span) = (s.v, s.side, s.span);
            let kinds = if side == 0 { etk } else { efk };
            let dx_of = |efd: &[f64], etd: &[f64], e: usize| if side == 0 { etd[e] } else { efd[e] };
            let mut k = 0;
            let mut lo = -span / 2.0;
            for i in 0..=s.all.len() {
                let held = i < s.all.len() && kinds[s.all[i]] == end::HELD;
                if i < s.all.len() && !held {
                    continue;
                }
                // Free ends since the last held end occupy ids[k..k + size).
                let mut size = 0;
                let mut j = i;
                while j > 0 && kinds[s.all[j - 1]] != end::HELD {
                    size += 1;
                    j -= 1;
                }
                let hi = if held { dx_of(&efd, &etd, s.all[i]) - PORT_PITCH } else { span / 2.0 };
                if size > 0 {
                    // Isotonic fitting preserves order and spacing by pooling neighbours that are too close.
                    let mut sum: Vec<f64> = Vec::new();
                    let mut count: Vec<usize> = Vec::new();
                    let mut before = 0.0;
                    for j in 0..size {
                        if j > 0 {
                            before += s.room[k + j];
                        }
                        sum.push(wanted(&efd, &etd, s.ids[k + j], side, v, span, pass > 0) - before);
                        count.push(1);
                        // Keep ends together when spacing forced them to share a place.
                        let mut together = j > 0 && s.room[k + j] == 0.0;
                        while sum.len() > 1 && (together || sum[sum.len() - 2] / count[count.len() - 2] as f64 > sum[sum.len() - 1] / count[count.len() - 1] as f64) {
                            together = false;
                            let top = sum.pop().unwrap();
                            let pooled = count.pop().unwrap();
                            *sum.last_mut().unwrap() += top;
                            *count.last_mut().unwrap() += pooled;
                        }
                    }
                    let total = before;
                    let mut j = 0;
                    before = 0.0;
                    for b in 0..sum.len() {
                        // Center the run if it cannot fit between held ends.
                        let level = if hi - total < lo { (lo + hi - total) / 2.0 } else { lo.max((hi - total).min(sum[b] / count[b] as f64)) };
                        for _ in 0..count[b] {
                            if j > 0 {
                                before += s.room[k + j];
                            }
                            let id = s.ids[k + j];
                            let dx = if side == 0 { &mut etd } else { &mut efd };
                            if (dx[id] - level - before).abs() > 0.01 {
                                moved = true;
                            }
                            dx[id] = level + before;
                            j += 1;
                        }
                    }
                    k += size;
                }
                if held {
                    lo = dx_of(&efd, &etd, s.all[i]) + PORT_PITCH;
                }
            }
        }
        pass += 1;
    }
    g.efd = efd;
    g.etd = etd;
}

// Brings edges that are nearly straight into line, by moving a node or bend a little across the
// flow along with everything it is in line with already, where the nodes beside them leave room.
// What is in line stays in line, so each move leaves one more edge straight.
pub(super) fn snap(g: &Graph, xs: &mut [f64], empty: Option<&[u8]>, opt: &LayeredOptions) {
    let n = g.n;
    let passed = |u: usize| g.kind[u] == kind::DUMMY && is_empty(empty, g.rank[u]);
    // Each edge from one point that is held in place to the next, past the bends in ranks that hold nothing else.
    let mut a: Vec<usize> = Vec::new();
    let mut b: Vec<usize> = Vec::new();
    let mut da: Vec<f64> = Vec::new();
    let mut db: Vec<f64> = Vec::new();
    for e in 0..g.ef.len() {
        if passed(g.ef[e]) {
            continue;
        }
        let f = follow(g, empty, e, 1);
        a.push(g.ef[e]);
        b.push(g.et[f]);
        da.push(g.efd[e]);
        db.push(g.etd[f]);
    }
    let off = |xs: &[f64], k: usize| xs[b[k]] + db[k] - xs[a[k]] - da[k];
    // The nodes in line with each other move as one.
    let mut block: Vec<usize> = (0..n).collect();
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    fn join(block: &mut [usize], members: &mut [Vec<usize>], x: usize, y: usize) {
        let mut keep = block[x];
        let mut drop = block[y];
        if keep == drop {
            return;
        }
        if members[keep].len() < members[drop].len() {
            std::mem::swap(&mut keep, &mut drop);
        }
        let moved = std::mem::take(&mut members[drop]);
        for &v in &moved {
            block[v] = keep;
        }
        members[keep].extend(moved);
    }
    let mut near: Vec<usize> = Vec::new();
    for k in 0..a.len() {
        let d = off(xs, k).abs();
        if d < 0.01 {
            join(&mut block, &mut members, a[k], b[k]);
        } else if d <= BEND_SNAP {
            near.push(k);
        }
    }
    if near.is_empty() {
        return;
    }
    near.sort_by(|&x, &y| off(xs, x).abs().partial_cmp(&off(xs, y).abs()).unwrap_or(std::cmp::Ordering::Equal).then(x.cmp(&y)));
    let gap = |v: usize| half_room(g, opt, v);
    // Whether every node of a block can move by `shift` and keep its room from those beside it.
    let free = |xs: &[f64], block: &[usize], members: &[Vec<usize>], id: usize, shift: f64| -> bool {
        for &v in &members[id] {
            let s = g.layer_start[g.rank[v]];
            let e_end = g.layer_start[g.rank[v] + 1];
            let i = s + g.pos[v];
            if i > s && block[g.order[i - 1]] != id && xs[v] + shift - xs[g.order[i - 1]] < gap(v) + gap(g.order[i - 1]) - 0.01 {
                return false;
            }
            if i + 1 < e_end && block[g.order[i + 1]] != id && xs[g.order[i + 1]] - xs[v] - shift < gap(v) + gap(g.order[i + 1]) - 0.01 {
                return false;
            }
        }
        true
    };
    for &k in &near {
        let d = off(xs, k);
        if d.abs() < 0.01 {
            join(&mut block, &mut members, a[k], b[k]);
            continue;
        }
        if d.abs() > BEND_SNAP || block[a[k]] == block[b[k]] {
            continue;
        }
        // The smaller of the two blocks moves, if it can; else the other.
        let upper = block[a[k]];
        let lower = block[b[k]];
        let first = if members[upper].len() <= members[lower].len() { upper } else { lower };
        for id in [first, if first == upper { lower } else { upper }] {
            let shift = if id == upper { d } else { -d };
            if d.abs() > SNAP && !members[id].iter().all(|&v| g.kind[v] == kind::DUMMY) {
                continue;
            }
            if !free(xs, &block, &members, id, shift) {
                continue;
            }
            for &v in &members[id] {
                xs[v] += shift;
            }
            join(&mut block, &mut members, a[k], b[k]);
            break;
        }
    }
}

// Marks the ranks that hold nothing but the bends of edges, if there are any.
pub(super) fn empty_ranks(g: &Graph) -> Option<Vec<u8>> {
    let mut empty = vec![0u8; g.max_rank + 1];
    let mut any = false;
    for r in 1..g.max_rank {
        let s = g.layer_start[r];
        let e = g.layer_start[r + 1];
        if e > s && g.order[s..e].iter().all(|&v| g.kind[v] == kind::DUMMY) {
            empty[r] = 1;
            any = true;
        }
    }
    any.then_some(empty)
}
