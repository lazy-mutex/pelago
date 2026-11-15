// Layered (Sugiyama-style) layout of one flat graph. Ranks run down the y axis; callers
// rotate the result for other directions.
//
// What the layout of edges aims for, most important first. Where two aims pull apart, the one
// higher up decides, and each names where it is carried out.
//
// 1. Lines can be told apart. No two touch or run along each other, and none runs through a
//    node or a label. Ends on one side of a node have places of their own (`spread`), and
//    curves that would pass within a few pixels turn at levels of their own (`assign_tracks`).
// 2. Lines cross as little as the order of the nodes allows (`reduce_crossings`, and the order
//    of the levels in `assign_tracks`).
// 3. An edge takes the shortest and simplest way: straight if it can, else one curve, else a
//    curve with a level run. One that passes several ranks runs in line with one of its ends
//    and turns once, at the other (`align_labels`, `straighten`). Ends slide along their sides,
//    and nodes move a little, where that makes an edge straight (`settle`, `snap`).
// 4. An edge meets the middle of a side. An end with a side to itself stays there, and ends
//    that share a side sit around the middle in the order of where they lead. An end moves off
//    the middle only for the aims above (`settle`).
// 5. The drawing is balanced, and what is alike is drawn alike. A label sits midway along the
//    line that shows, the curves either side of it have much the same room to turn in, and a gap
//    whose curves mostly cross has them all as the same plain curve rather than each on a
//    level of its own (`head_room` in `layered`, the crossed gaps in `assign_tracks`).
// 6. An edge leaves and reaches a node square to it, and an arrowhead points straight at the
//    node (`route_path` in svg/edges.rs, which draws the routes).
// 7. Nodes keep the order they were written in, where nothing above says otherwise.
// 8. The drawing stays compact. A gap between ranks grows only to hold the levels in it, or
//    to let curves that cross do so at a clear angle.

mod ends;
mod order;
mod position;
mod rank;
mod tracks;

pub use ends::ease;
pub use rank::{orient, rank};

use ends::{align_labels, align_ports, empty_ranks, free_ends, settle, snap, spread};
use order::reduce_crossings;
use position::position;
use tracks::{assign_tracks, past_labels, straighten};

pub mod kind {
    pub const NODE: u8 = 0;
    pub const DUMMY: u8 = 1;
    pub const LABEL: u8 = 2;
    pub const START_PORT: u8 = 3;
    pub const END_PORT: u8 = 4;
}

#[derive(Debug, Clone)]
pub struct LNode {
    pub w: f64,
    pub h: f64,
    pub kind: u8,
    pub pin: i32,
    // Port's cross-flow destination; NaN if unknown. Fully hinted port ranks keep this order.
    pub hint: f64,
    // Initial cross-flow position; NaN if unknown. Used only when every node in a rank has one.
    pub key: f64,
    // How much of the side that edges meet may be shared out between them; 0 keeps them all at its middle.
    pub span: f64,
    pub x: f64,
    pub y: f64,
    pub rank: i32,
}

#[derive(Debug, Clone)]
pub struct LEdge {
    pub tail: usize,
    pub head: usize,
    pub minlen: i32,
    pub label_w: f64,
    pub label_h: f64,
    pub tail_dx: f64,
    pub head_dx: f64,
    // An end that is held where its offset puts it, and takes no part in sharing out a node's side.
    pub tail_fixed: bool,
    pub head_fixed: bool,
    pub reversed: bool,
    pub points: Vec<f64>,
    pub label_x: f64,
    pub label_y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Groups {
    // Maps nodes to groups, and groups to parents. Both use -1 for none.
    pub of: Vec<i32>,
    pub parent: Vec<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct LayeredOptions {
    pub node_sep: f64,
    pub edge_sep: f64,
    pub rank_sep: f64,
    pub port_sep: f64,
    // Route edges at right angles, each on a track of its own across the gap between two ranks.
    pub tracks: bool,
    // Draw every edge straight from point to point: none is given a level to turn onto.
    pub straight: bool,
    // The caller sets the ends of the edges on one side of a node apart itself, so two edges
    // between the same two nodes need no bend to keep them from being drawn as one.
    pub apart: bool,
    // The room that the marker at the head of an edge takes. A rank of labels whose edges all
    // head the same way sits half of this nearer their tails, so that each label is midway along
    // the line that shows, and the curves either side of it have the same room to turn in.
    pub head_room: f64,
    // Keep each group contiguous within ranks.
    pub groups: Option<Groups>,
    // Compute cross-flow positions in `x` without full routing.
    pub order_only: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LayeredResult {
    pub width: f64,
    pub height: f64,
    // With `order_only`: final crossings and crossings before grouping or reordering.
    pub crossings: usize,
    pub declared: usize,
}

pub fn lnode(w: f64, h: f64, kind: u8) -> LNode {
    LNode { w, h, kind, pin: 0, hint: f64::NAN, key: f64::NAN, span: 0.0, x: 0.0, y: 0.0, rank: 0 }
}

pub fn ledge(tail: usize, head: usize, minlen: i32, label_w: f64, label_h: f64) -> LEdge {
    LEdge {
        tail,
        head,
        minlen,
        label_w,
        label_h,
        tail_dx: 0.0,
        head_dx: 0.0,
        tail_fixed: false,
        head_fixed: false,
        reversed: false,
        points: Vec::new(),
        label_x: 0.0,
        label_y: 0.0,
    }
}

pub(crate) const MAX_SWEEPS: usize = 24;

// An edge gets a bend point in every rank it skips. Past this many, the longest edges are
// drawn as one direct curve instead, which keeps huge graphs from exhausting time and memory.
const MAX_DUMMIES: usize = 20000;

// The room between two edges that meet one side of a node, and the least that keeps their arrowheads apart.
pub(crate) const PORT_PITCH: f64 = 16.0;
pub(crate) const MIN_PITCH: f64 = 8.0;

// Where edges are not given levels, a gap between ranks grows to keep the edges that cross it at
// least this steep, up to this many times its usual height.
const MIN_SLOPE: f64 = 0.125;
const MAX_GAP: f64 = 3.0;

// Tracks: the room kept clear between a rank and the nearest track, the room between two tracks
// when the gap has to grow to hold them, and the most they spread to when it has room to spare.
const TRACK_MARGIN: f64 = 18.0;
const TRACK_PITCH: f64 = 10.0;
const TRACK_SPREAD: f64 = 14.0;
// A curve is one plain S unless it runs more than this many times as far across as the gap is
// high, and so does another that it would run along. Then each turns onto a level of its own
// to run across on. The levels of a gap keep this far from its ranks, and from each other.
pub(crate) const LEVEL_WIDE: f64 = 2.0;
const LEVEL_MARGIN: f64 = 28.0;
const LEVEL_PITCH: f64 = 12.0;
// Two plain curves that would pass too near each other turn at levels of their own as well:
// nearer than LEVEL_CLEAR where one of them runs far across, and otherwise nearer than
// LEVEL_TOUCH. LEVEL_LEAD is the room a plain curve is taken to keep straight at each end of its gap.
pub(crate) const LEVEL_LEAD: f64 = 12.0;
pub(crate) const LEVEL_CLEAR: f64 = 8.0;
pub(crate) const LEVEL_TOUCH: f64 = 4.0;
// Where the curves of a gap cross each other more often than they run alongside, each curve is
// one plain S, and the gap is made this many times as tall as its widest curve runs across.
pub(crate) const CROSSED_SLOPE: f64 = 0.5;
// How far out of line two points may be and still count as in line.
pub(crate) const IN_LINE: f64 = 0.5;
// How far apart two edges may be at both ends and still count as twins.
pub(crate) const TWIN: f64 = 1.0;
// A node, with whatever it is already in line with, moves this far at most to bring one more of
// its edges into line.
pub(crate) const SNAP: f64 = 12.0;
// Bends with no node in line with them move this far at most to bring an edge into line.
pub(crate) const BEND_SNAP: f64 = 24.0;
// An end that cannot be brought into line leans towards where its edge goes, but keeps this far
// from the ends of the part of its side that edges spread along.
pub(crate) const LEAN_CLEAR: f64 = 40.0;
// Past this many edges across one gap, tracks are given out in a plain order.
pub(crate) const MAX_WEIGHED: usize = 400;
pub(crate) const SAMPLES: usize = 16;

pub(crate) mod end {
    pub const HELD: u8 = 0;
    pub const LEAVES: u8 = 1;
    pub const ARRIVES: u8 = 2;
}

pub(crate) fn is_dummy(k: u8) -> bool {
    k == kind::DUMMY || k == kind::LABEL
}

// The pieces of edges that each cross one gap: where they leave the rank above and reach the
// one below, the rank above, the bend they end at (or -1 at the edge's last node), and the
// track they run across on (or -1 when they have none). Where only some pieces are given a
// level to turn onto, `wide` marks those that run far across, `levels` is how many tracks the
// group a piece belongs to takes, or 0 for a piece that needs none, and each piece's track
// counts from its group's first. A piece that is not `wide` curves from the level above its own.
#[derive(Default)]
pub(crate) struct Pieces {
    pub from: Vec<f64>,
    pub to: Vec<f64>,
    pub rank: Vec<usize>,
    pub lower: Vec<i32>,
    pub track: Vec<i32>,
    pub levels: Vec<i32>,
    pub wide: Vec<u8>,
}

pub(crate) struct Graph {
    pub n: usize,
    pub real: usize,
    pub max_rank: usize,
    pub w: Vec<f64>,
    pub kind: Vec<u8>,
    pub rank: Vec<usize>,
    // Segment endpoints, and where each segment attaches on them.
    pub ef: Vec<usize>,
    pub et: Vec<usize>,
    pub efd: Vec<f64>,
    pub etd: Vec<f64>,
    pub succ_start: Vec<usize>,
    pub succ_e: Vec<usize>,
    pub pred_start: Vec<usize>,
    pub pred_e: Vec<usize>,
    pub layer_start: Vec<usize>,
    pub order: Vec<usize>,
    pub pos: Vec<usize>,
    pub offsets: bool,
    // Ranks whose order must remain fixed.
    pub fixed: Vec<u8>,
    pub grp: Option<Vec<i32>>,
    pub group_parent: Option<Vec<i32>>,
}

impl Graph {
    fn set_positions(&mut self) {
        for r in 0..=self.max_rank {
            let s = self.layer_start[r];
            for i in s..self.layer_start[r + 1] {
                self.pos[self.order[i]] = i - s;
            }
        }
    }

    // Sorts a rank by `key`, unless a node in it has none. Ties keep their order.
    fn sort_rank(&mut self, r: usize, key: &dyn Fn(usize) -> f64) -> bool {
        let s = self.layer_start[r];
        let e = self.layer_start[r + 1];
        if e - s < 2 || self.order[s..e].iter().any(|&v| key(v).is_nan()) {
            return false;
        }
        self.order[s..e].sort_by(|&a, &b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
        true
    }
}

pub fn layered(nodes: &mut [LNode], edges: &mut [LEdge], opt: &LayeredOptions) -> LayeredResult {
    let real = nodes.len();
    if real == 0 {
        return LayeredResult::default();
    }
    let m = edges.len();
    if real == 1 && m == 0 && nodes[0].kind == kind::NODE {
        let node = &mut nodes[0];
        node.rank = 0;
        node.x = node.w / 2.0;
        node.y = node.h / 2.0;
        return LayeredResult { width: node.w, height: node.h, ..Default::default() };
    }

    // Every edge gets a middle rank of its own when labels or parallel edges need the room.
    let mut step: usize = 1;
    if m > 0 {
        let mut seen = std::collections::HashSet::new();
        for e in edges.iter() {
            let key = e.tail.min(e.head) * real + e.tail.max(e.head);
            if e.label_w > 0.0 || !seen.insert(key) {
                step = 2;
                break;
            }
        }
    }

    orient(nodes, edges);
    rank(nodes, edges, step as i32);

    let mut span = vec![0usize; m];
    let mut dummies = 0usize;
    for (ei, e) in edges.iter().enumerate() {
        span[ei] = (nodes[e.head].rank - nodes[e.tail].rank).unsigned_abs() as usize;
        if span[ei] > 1 {
            dummies += span[ei] - 1;
        }
    }
    let mut direct = vec![false; m];
    if dummies > MAX_DUMMIES {
        let mut longest: Vec<usize> = (0..m).filter(|&ei| span[ei] > 1).collect();
        longest.sort_by(|&a, &b| span[b].cmp(&span[a]).then(a.cmp(&b)));
        for ei in longest {
            if dummies <= MAX_DUMMIES {
                break;
            }
            direct[ei] = true;
            dummies -= span[ei] - 1;
        }
    }

    let n = real + dummies;
    let mut w = vec![0.0; n];
    let mut h = vec![0.0; n];
    let mut kinds = vec![0u8; n];
    let mut ranks = vec![0usize; n];
    let mut max_rank = 0usize;
    for (i, node) in nodes.iter().enumerate() {
        w[i] = node.w;
        h[i] = node.h;
        kinds[i] = node.kind;
        ranks[i] = node.rank as usize;
        max_rank = max_rank.max(ranks[i]);
    }

    // The graph that gets ordered and positioned: every edge cut into rank-to-rank segments.
    let segments: usize = (0..m).filter(|&ei| !direct[ei]).map(|ei| span[ei]).sum();
    let mut ef = vec![0usize; segments];
    let mut et = vec![0usize; segments];
    let mut efd = vec![0.0; segments];
    let mut etd = vec![0.0; segments];
    let mut first_dummy = vec![-1i32; m];
    // Which way the edges with a label in each rank head: 1 down, 2 up, 3 both.
    let mut heads = vec![0u8; max_rank + 1];
    let mut label_node = vec![-1i32; m];
    let spans = nodes.iter().any(|node| node.span > 0.0 && node.kind == kind::NODE);
    // What each segment's end is to the node it meets, for the ends that may move along the node's side.
    let mut efk = vec![0u8; segments];
    let mut etk = vec![0u8; segments];
    let mut first_seg = vec![-1i32; m];
    let mut last_seg = vec![-1i32; m];
    let mut key = vec![f64::NAN; n];
    for i in 0..real {
        key[i] = nodes[i].key;
    }
    let mut offsets = false;
    {
        let mut next = real;
        let mut ce = 0;
        for ei in 0..m {
            if direct[ei] {
                continue;
            }
            let e = &edges[ei];
            let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
            let (a_dx, b_dx) = if e.reversed { (e.head_dx, e.tail_dx) } else { (e.tail_dx, e.head_dx) };
            if a_dx != 0.0 || b_dx != 0.0 {
                offsets = true;
            }
            let ra = ranks[a] as i64;
            let rb = ranks[b] as i64;
            let mut label_rank: i64 = -1;
            if e.label_w > 0.0 {
                // The label goes halfway along. An edge that passes ranks of nodes on its way has its
                // label in one of those, beside the nodes, where it takes no height of its own: the one
                // halfway, or of the two either side of halfway, the one nearer the node the edge starts
                // from, which is the lower when the edge runs against the flow. An edge between two
                // neighbouring ranks has its label in the rank between them.
                label_rank = ra + ((rb - ra) >> 1);
                let toward = if e.reversed { 1 } else { -1 };
                if step == 2 && rb - ra >= 4 {
                    if label_rank & 1 == 1 {
                        label_rank += toward;
                    }
                } else if step == 2 && label_rank & 1 == 0 {
                    label_rank += toward;
                }
            }
            let mut prev = a;
            if rb - ra > 1 {
                first_dummy[ei] = next as i32;
            }
            first_seg[ei] = ce as i32;
            if spans && !(if e.reversed { e.head_fixed } else { e.tail_fixed }) {
                efk[ce] = if e.reversed { end::ARRIVES } else { end::LEAVES };
            }
            for r in ra + 1..rb {
                let d = next;
                next += 1;
                ranks[d] = r as usize;
                key[d] = key[a] + ((key[b] - key[a]) * (r - ra) as f64) / (rb - ra) as f64;
                if r == label_rank {
                    w[d] = e.label_w;
                    h[d] = e.label_h;
                    kinds[d] = kind::LABEL;
                    label_node[ei] = d as i32;
                    heads[r as usize] |= if e.reversed { 2 } else { 1 };
                } else {
                    kinds[d] = kind::DUMMY;
                }
                ef[ce] = prev;
                et[ce] = d;
                efd[ce] = if prev == a { a_dx } else { 0.0 };
                ce += 1;
                prev = d;
            }
            ef[ce] = prev;
            et[ce] = b;
            efd[ce] = if prev == a { a_dx } else { 0.0 };
            etd[ce] = b_dx;
            if spans && !(if e.reversed { e.tail_fixed } else { e.head_fixed }) {
                etk[ce] = if e.reversed { end::LEAVES } else { end::ARRIVES };
            }
            last_seg[ei] = ce as i32;
            ce += 1;
        }
    }

    // Bends belong to the innermost group containing both edge endpoints.
    let mut grp: Option<Vec<i32>> = None;
    if let Some(groups) = &opt.groups {
        let parent = &groups.parent;
        let mut g = vec![0i32; n];
        g[..real].copy_from_slice(&groups.of[..real]);
        let depth = |mut gr: i32| {
            let mut d = 0;
            while gr >= 0 {
                d += 1;
                gr = parent[gr as usize];
            }
            d
        };
        let lca = |mut x: i32, mut y: i32| {
            let mut dx = depth(x);
            let mut dy = depth(y);
            while dx > dy {
                x = parent[x as usize];
                dx -= 1;
            }
            while dy > dx {
                y = parent[y as usize];
                dy -= 1;
            }
            while x != y {
                x = parent[x as usize];
                y = parent[y as usize];
            }
            x
        };
        for ei in 0..m {
            if first_dummy[ei] < 0 {
                continue;
            }
            let e = &edges[ei];
            let gr = lca(groups.of[e.tail], groups.of[e.head]);
            let d0 = first_dummy[ei] as usize;
            for slot in &mut g[d0..d0 + span[ei] - 1] {
                *slot = gr;
            }
        }
        grp = Some(g);
    }

    let mut succ_start = vec![0usize; n + 1];
    let mut pred_start = vec![0usize; n + 1];
    for e in 0..segments {
        succ_start[ef[e] + 1] += 1;
        pred_start[et[e] + 1] += 1;
    }
    for i in 0..n {
        succ_start[i + 1] += succ_start[i];
        pred_start[i + 1] += pred_start[i];
    }
    let mut succ_e = vec![0usize; segments];
    let mut pred_e = vec![0usize; segments];
    {
        let mut sf = succ_start[..n].to_vec();
        let mut pf = pred_start[..n].to_vec();
        for e in 0..segments {
            succ_e[sf[ef[e]]] = e;
            sf[ef[e]] += 1;
            pred_e[pf[et[e]]] = e;
            pf[et[e]] += 1;
        }
    }

    let mut layer_start = vec![0usize; max_rank + 2];
    for i in 0..n {
        layer_start[ranks[i] + 1] += 1;
    }
    for r in 0..=max_rank {
        layer_start[r + 1] += layer_start[r];
    }
    let mut order = vec![0usize; n];

    // Initial order: depth-first from the top so that subtrees start out contiguous.
    {
        let mut by_rank = vec![0usize; n];
        let mut fill = layer_start[..=max_rank].to_vec();
        for i in 0..n {
            by_rank[fill[ranks[i]]] = i;
            fill[ranks[i]] += 1;
        }
        fill.copy_from_slice(&layer_start[..=max_rank]);
        let mut visited = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();
        for k in 0..n {
            if visited[by_rank[k]] {
                continue;
            }
            stack.push(by_rank[k]);
            while let Some(v) = stack.pop() {
                if visited[v] {
                    continue;
                }
                visited[v] = true;
                order[fill[ranks[v]]] = v;
                fill[ranks[v]] += 1;
                for j in (succ_start[v]..succ_start[v + 1]).rev() {
                    let t = et[succ_e[j]];
                    if !visited[t] {
                        stack.push(t);
                    }
                }
            }
        }
    }

    let mut g = Graph {
        n,
        real,
        max_rank,
        w,
        kind: kinds,
        rank: ranks,
        ef,
        et,
        efd,
        etd,
        succ_start,
        succ_e,
        pred_start,
        pred_e,
        layer_start,
        order,
        pos: vec![0usize; n],
        offsets,
        fixed: vec![0u8; max_rank + 1],
        grp,
        group_parent: opt.groups.as_ref().map(|gr| gr.parent.clone()),
    };

    for r in 0..=max_rank {
        g.sort_rank(r, &|v| key[v]);
    }
    if nodes.iter().any(|node| node.pin != 0) {
        for r in 0..=max_rank {
            let (s, e) = (g.layer_start[r], g.layer_start[r + 1]);
            g.order[s..e].sort_by_key(|&v| if v < real { nodes[v].pin } else { 0 });
        }
    }
    // Preserve port order to avoid crossings outside the group.
    {
        let hint = |v: usize| if v < real && (nodes[v].kind == kind::START_PORT || nodes[v].kind == kind::END_PORT) { nodes[v].hint } else { f64::NAN };
        for r in [0, max_rank] {
            if g.sort_rank(r, &hint) {
                g.fixed[r] = 1;
            }
        }
    }
    g.set_positions();

    let mut crossings = 0;
    let mut declared = 0;
    if segments > 0 && max_rank > 0 {
        (crossings, declared) = reduce_crossings(&mut g, nodes);
        g.set_positions();
    }
    if opt.order_only {
        // Points are (rank, cross-flow position) pairs.
        let at = position(&mut g, opt);
        for i in 0..real {
            nodes[i].x = at[i];
        }
        for ei in 0..m {
            let e = &mut edges[ei];
            let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
            let mut pts = vec![g.rank[a] as f64, at[a]];
            if !direct[ei] && first_dummy[ei] >= 0 {
                let d0 = first_dummy[ei] as usize;
                for d in d0..d0 + span[ei] - 1 {
                    pts.extend([g.rank[d] as f64, at[d]]);
                }
            }
            pts.extend([g.rank[b] as f64, at[b]]);
            e.points = pts;
        }
        return LayeredResult { width: 0.0, height: 0.0, crossings, declared };
    }

    if spans {
        spread(&mut g, nodes, &efk, &etk);
    }

    let mut xs = position(&mut g, opt);

    let empty = empty_ranks(&g);
    let empty_ref = empty.as_deref();
    if spans {
        let free = free_ends(&g, &efk, &etk);
        align_labels(&g, nodes, &mut xs, &free, opt, empty_ref);
        settle(&mut g, nodes, &mut xs, &efk, &etk, &free, empty_ref);
        snap(&g, &mut xs, empty_ref, opt);
        for ei in 0..m {
            if direct[ei] {
                continue;
            }
            let e = &mut edges[ei];
            let a_dx = g.efd[first_seg[ei] as usize];
            let b_dx = g.etd[last_seg[ei] as usize];
            e.tail_dx = if e.reversed { b_dx } else { a_dx };
            e.head_dx = if e.reversed { a_dx } else { b_dx };
        }
    }

    align_ports(&g, &mut xs, empty_ref);

    // A rank that holds nothing but the bends of edges is only there because some other edge has a
    // label or a twin. Its bends move onto the straight line between the ends of their edges, and
    // one that gets there is left out of the route, so that the edge is drawn as a single curve.
    let mut on_line = vec![false; n];
    if let Some(empty) = empty_ref {
        if !opt.tracks {
            straighten(&g, &mut xs, empty, &first_dummy, &span, if opt.apart { 0.0 } else { opt.edge_sep }, &mut on_line);
        }
    }
    if !opt.tracks {
        past_labels(&g, &mut xs, &first_dummy, &span, opt.edge_sep, &mut on_line);
    }
    let is_empty = |r: usize| empty_ref.is_some_and(|e| e[r] != 0);
    // An edge is cut where it passes a rank that holds something, and each piece crosses one gap.
    // With `tracks`, every piece that does not run straight gets a track to run across on at
    // right angles. Otherwise, where edges spread along their nodes, a piece gets one only if it
    // needs a level to turn onto. `piece_start` gives every edge's run of pieces.
    let mut piece_start = vec![0usize; m + 1];
    let mut pieces: Option<Pieces> = if opt.tracks || (spans && !opt.straight) { Some(Pieces::default()) } else { None };
    let mut track_count = vec![0i32; max_rank + 1];
    if let Some(pieces) = pieces.as_mut() {
        for ei in 0..m {
            piece_start[ei] = pieces.from.len();
            if direct[ei] {
                continue;
            }
            let bends = if first_dummy[ei] < 0 { 0..0 } else { first_dummy[ei] as usize..first_dummy[ei] as usize + span[ei] - 1 };
            // An edge that keeps a bend to stay clear of its twin is drawn through the bend, as it is.
            // So is one that has a bend left out in a rank of labels: its curve crosses that rank.
            if !opt.tracks && bends.clone().any(|d| is_empty(g.rank[d]) != on_line[d]) {
                continue;
            }
            let fs = first_seg[ei] as usize;
            let ls = last_seg[ei] as usize;
            let mut x = xs[g.ef[fs]] + g.efd[fs];
            let mut rank = g.rank[g.ef[fs]];
            for d in bends {
                if is_empty(g.rank[d]) {
                    continue;
                }
                pieces.from.push(x);
                pieces.to.push(xs[d]);
                pieces.rank.push(rank);
                pieces.lower.push(d as i32);
                x = xs[d];
                rank = g.rank[d];
            }
            pieces.from.push(x);
            pieces.to.push(xs[g.et[ls]] + g.etd[ls]);
            pieces.rank.push(rank);
            pieces.lower.push(-1);
        }
        piece_start[m] = pieces.from.len();
    }

    // Ranks that hold only border ports or their pass-through dummies need little room of their own.
    let port_only = |r: usize, k: u8| {
        let mut found = false;
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            let kd = g.kind[g.order[i]];
            if kd == k {
                found = true;
            } else if kd != kind::DUMMY {
                return false;
            }
        }
        found
    };
    let start_ranks = if port_only(0, kind::START_PORT) { step } else { 0 };
    let end_ranks = if port_only(max_rank, kind::END_PORT) { step } else { 0 };
    // The longest way across that any edge travels in the gap below each rank.
    let mut run = vec![0.0f64; max_rank + 1];
    for e in 0..segments {
        let across = (xs[g.ef[e]] + g.efd[e] - xs[g.et[e]] - g.etd[e]).abs();
        let r = g.rank[g.ef[e]];
        if across > run[r] {
            run[r] = across;
        }
    }
    let stepf = step as f64;
    let usual = |r: usize| (if r < start_ranks || r + end_ranks >= max_rank { opt.port_sep } else { opt.rank_sep }) / stepf;
    // The room under each rank. Edges that are not cut into pieces need more where they run far across.
    let mut gap_under = vec![0.0f64; max_rank + 1];
    for r in 0..max_rank {
        gap_under[r] = if pieces.is_some() { usual(r) } else { usual(r).max(((MAX_GAP * opt.rank_sep) / stepf).min(run[r] * MIN_SLOPE)) };
    }
    // A rank between two ranks of nodes, which holds labels and bends only.
    let mut between = vec![true; max_rank + 1];
    for i in 0..real {
        between[g.rank[i]] = false;
    }
    for r in 1..max_rank {
        if opt.tracks || !between[r] || (heads[r] != 1 && heads[r] != 2) {
            continue;
        }
        let down = heads[r] == 1;
        let shift = (opt.head_room / 2.0).min(gap_under[if down { r - 1 } else { r }] / 2.0);
        gap_under[r - 1] += if down { -shift } else { shift };
        gap_under[r] += if down { shift } else { -shift };
    }
    // The rank that the pieces leaving each rank arrive at, past any that hold only bends, and
    // the room between the two, which grows to hold the gap's tracks or levels: these keep
    // `margin` from the ranks, and between `least_pitch` and `most_pitch` from each other.
    let margin = if opt.tracks { TRACK_MARGIN } else { LEVEL_MARGIN };
    let least_pitch = if opt.tracks { TRACK_PITCH } else { LEVEL_PITCH };
    let most_pitch = if opt.tracks { TRACK_SPREAD } else { LEVEL_PITCH };
    let mut below = vec![0usize; max_rank + 1];
    if let Some(pieces) = pieces.as_mut() {
        let mut room = vec![0.0f64; max_rank + 1];
        for r in 0..max_rank {
            if is_empty(r) {
                continue;
            }
            let mut next = r + 1;
            room[r] = gap_under[r];
            while next < max_rank && is_empty(next) {
                room[r] += gap_under[next];
                next += 1;
            }
            below[r] = next;
        }
        for k in 0..pieces.from.len() {
            pieces.wide.push(((pieces.from[k] - pieces.to[k]).abs() > LEVEL_WIDE * room[pieces.rank[k]]) as u8);
        }
        let mut tall = vec![0.0f64; max_rank + 1];
        assign_tracks(pieces, &mut track_count, !opt.tracks, &room, &mut tall);
        for r in 0..max_rank {
            if track_count[r] > 0 {
                gap_under[r] += (2.0 * margin + (track_count[r] - 1) as f64 * least_pitch - room[r]).max(0.0);
            } else if tall[r] > 0.0 {
                gap_under[r] += ((MAX_GAP * opt.rank_sep).min(tall[r]) - room[r]).max(0.0);
            }
        }
    }
    let mut band_top = vec![0.0f64; max_rank + 1];
    let mut band_bottom = vec![0.0f64; max_rank + 1];
    let mut y = 0.0;
    for r in 0..=max_rank {
        let mut tallest = 0.0f64;
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            tallest = tallest.max(h[g.order[i]]);
        }
        band_top[r] = y;
        band_bottom[r] = y + tallest;
        y += tallest + gap_under[r];
    }
    let height = y;
    // The part of its rank that an edge runs straight through at a bend or label. A bare bend
    // beside labels keeps to the middle half, which leaves the curves either side of it more room
    // and still keeps them clear of the labels' corners.
    let inset = |d: usize| if g.kind[d] == kind::DUMMY && between[g.rank[d]] && !opt.tracks { (band_bottom[g.rank[d]] - band_top[g.rank[d]]) / 4.0 } else { 0.0 };
    // Where a piece runs across its gap, or would on another track of its group: its tracks sit
    // in the middle of the gap, the first at the top.
    let track_y = |pieces: &Pieces, piece: usize, on_track: i32| {
        let r = pieces.rank[piece];
        let top = band_bottom[r];
        let bottom = band_top[below[r]];
        let count = if opt.tracks { track_count[r] } else { pieces.levels[piece] } as f64;
        let pitch = if count > 1.0 { most_pitch.min((bottom - top - 2.0 * margin) / (count - 1.0)) } else { 0.0 };
        (top + bottom) / 2.0 + (on_track as f64 - (count - 1.0) / 2.0) * pitch
    };

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for i in 0..n {
        let half = g.w[i] / 2.0;
        min_x = min_x.min(xs[i] - half);
        max_x = max_x.max(xs[i] + half);
    }
    // Where edges spread along their nodes, a rank whose nodes only send edges on stands on one
    // line, and a rank whose nodes only receive them hangs from one, so that the edges of nodes of
    // different heights set out from, or arrive at, the same level. In any other rank the nodes
    // are centered.
    let mut arrives = vec![false; max_rank + 1];
    let mut leaves = vec![false; max_rank + 1];
    for i in 0..real {
        if g.kind[i] != kind::NODE || !spans {
            continue;
        }
        if g.pred_start[i + 1] > g.pred_start[i] {
            arrives[g.rank[i]] = true;
        }
        if g.succ_start[i + 1] > g.succ_start[i] {
            leaves[g.rank[i]] = true;
        }
    }
    for i in 0..real {
        let node = &mut nodes[i];
        let r = node.rank as usize;
        node.x = xs[i] - min_x;
        node.y = if g.kind[i] == kind::NODE && leaves[r] && !arrives[r] {
            band_bottom[r] - node.h / 2.0
        } else if g.kind[i] == kind::NODE && arrives[r] && !leaves[r] {
            band_top[r] + node.h / 2.0
        } else {
            (band_top[r] + band_bottom[r]) / 2.0
        };
    }

    for ei in 0..m {
        let e = &mut edges[ei];
        let first = &nodes[if e.reversed { e.head } else { e.tail }];
        let last = &nodes[if e.reversed { e.tail } else { e.head }];
        let mut pts: Vec<f64> = vec![first.x + if e.reversed { e.head_dx } else { e.tail_dx }, first.y + first.h / 2.0];
        let d0 = first_dummy[ei];
        match &pieces {
            Some(pieces) if piece_start[ei + 1] > piece_start[ei] => {
                for k in piece_start[ei]..piece_start[ei + 1] {
                    if pieces.track[k] >= 0 && (opt.tracks || pieces.wide[k] == 1) {
                        let across = track_y(pieces, k, pieces.track[k]);
                        pts.extend([pieces.from[k] - min_x, across, pieces.to[k] - min_x, across]);
                    } else if pieces.track[k] > 0 {
                        // A short piece among levels runs straight until it is past the level before its
                        // own, and turns in one S from there, so that it crosses none of them.
                        pts.extend([pieces.from[k] - min_x, track_y(pieces, k, pieces.track[k] - 1)]);
                    }
                    if pieces.lower[k] < 0 {
                        continue;
                    }
                    let d = pieces.lower[k] as usize;
                    pts.extend([xs[d] - min_x, band_top[g.rank[d]] + inset(d)]);
                    if band_bottom[g.rank[d]] > band_top[g.rank[d]] {
                        pts.extend([xs[d] - min_x, band_bottom[g.rank[d]] - inset(d)]);
                    }
                }
            }
            _ if d0 >= 0 => {
                let d0 = d0 as usize;
                for d in d0..d0 + span[ei] - 1 {
                    if on_line[d] {
                        continue;
                    }
                    let x = xs[d] - min_x;
                    let top = band_top[g.rank[d]];
                    let bottom = band_bottom[g.rank[d]];
                    pts.extend([x, top + inset(d)]);
                    if bottom > top {
                        pts.extend([x, bottom - inset(d)]);
                    }
                }
            }
            _ => {}
        }
        pts.extend([last.x + if e.reversed { e.tail_dx } else { e.head_dx }, last.y - last.h / 2.0]);
        if e.reversed {
            let pairs: Vec<[f64; 2]> = pts.chunks_exact(2).rev().map(|p| [p[0], p[1]]).collect();
            pts = pairs.into_iter().flatten().collect();
        }
        let ln = label_node[ei];
        if ln >= 0 {
            let ln = ln as usize;
            e.label_x = xs[ln] - min_x;
            e.label_y = (band_top[g.rank[ln]] + band_bottom[g.rank[ln]]) / 2.0;
        } else if e.label_w > 0.0 {
            e.label_x = (pts[0] + pts[pts.len() - 2]) / 2.0;
            e.label_y = (pts[1] + pts[pts.len() - 1]) / 2.0;
        }
        e.points = pts;
    }

    LayeredResult { width: max_x - min_x, height, crossings: 0, declared: 0 }
}
