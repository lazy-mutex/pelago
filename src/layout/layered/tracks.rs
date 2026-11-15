use super::ends::ease;
use super::{kind, Graph, Pieces, CROSSED_SLOPE, IN_LINE, LEVEL_CLEAR, LEVEL_LEAD, LEVEL_TOUCH, MAX_WEIGHED, SAMPLES, TWIN};
use std::cmp::Ordering;
use std::collections::HashSet;

fn cmp(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

struct Tracks<'a> {
    from: &'a [f64],
    to: &'a [f64],
    lo: Vec<f64>,
    hi: Vec<f64>,
    track: &'a mut Vec<i32>,
    levels: &'a mut Vec<i32>,
    curved: bool,
}

impl Tracks<'_> {
    fn within(&self, x: f64, k: usize) -> i32 {
        (x >= self.lo[k] - IN_LINE && x <= self.hi[k] + IN_LINE) as i32
    }

    fn mates(&self, a: usize, b: usize) -> bool {
        (self.from[a] - self.from[b]).abs() < IN_LINE || (self.to[a] - self.to[b]).abs() < IN_LINE
    }

    fn overlap(&self, a: usize, b: usize) -> bool {
        self.lo[a] < self.hi[b] - IN_LINE && self.lo[b] < self.hi[a] - IN_LINE
    }

    // Whether a and b need tracks of their own.
    fn clash(&self, a: usize, b: usize) -> bool {
        self.overlap(a, b) && !self.mates(a, b)
    }

    // Whether one of a and b could cross the other, depending on which is above.
    fn meet(&self, a: usize, b: usize) -> bool {
        self.overlap(a, b) || self.within(self.to[a], b) + self.within(self.from[b], a) + self.within(self.to[b], a) + self.within(self.from[a], b) > 0
    }

    // How often a and b cross with a on the higher track: a's way down through b, and b's way down to itself through a.
    fn crossings(&self, a: usize, b: usize) -> i32 {
        self.within(self.to[a], b) + self.within(self.from[b], a)
    }

    // Returns pairs of list indices, smaller first. Omitted pairs cannot meet or clash.
    fn near_pairs(&self, list: &[usize]) -> Vec<(usize, usize)> {
        let mut by_lo: Vec<usize> = (0..list.len()).collect();
        by_lo.sort_by(|&a, &b| cmp(self.lo[list[a]], self.lo[list[b]]));
        let mut pairs = Vec::new();
        for x in 0..by_lo.len() {
            let i = by_lo[x];
            let reach = self.hi[list[i]] + 4.0 * IN_LINE;
            for &j in by_lo[x + 1..].iter().take_while(|&&j| self.lo[list[j]] <= reach) {
                pairs.push((i.min(j), i.max(j)));
            }
        }
        pairs
    }

    // Gives the pieces of one gap in `list` their tracks, and each the number of tracks that the
    // pieces it has to do with take between them, counting its own from the first of those.
    // Returns the number of tracks. The pieces come without tracks.
    fn lay(&mut self, list: &mut Vec<usize>) -> i32 {
        // A plain order to start from, and to keep where nothing says otherwise: the pieces that
        // reach furthest go first, so that those of one fan nest.
        list.sort_by(|&a, &b| {
            cmp(self.hi[b] - self.lo[b], self.hi[a] - self.lo[a]).then(cmp(self.lo[a], self.lo[b])).then(a.cmp(&b))
        });
        let count = list.len();
        let mut above: Vec<Vec<usize>> = Vec::new();
        let mut group: Vec<usize> = (0..count).collect();
        fn find(group: &mut [usize], mut i: usize) -> usize {
            while group[i] != i {
                group[i] = group[group[i]];
                i = group[i];
            }
            i
        }
        let weighed = count <= MAX_WEIGHED;
        // Avoid storing quadratically many pairs in dense gaps.
        let pairs = if weighed { self.near_pairs(list) } else { Vec::new() };
        let mut clashes: Vec<Vec<usize>> = if weighed { vec![Vec::new(); count] } else { Vec::new() };
        for &(i, j) in &pairs {
            if !self.clash(list[i], list[j]) {
                continue;
            }
            clashes[i].push(j);
            clashes[j].push(i);
        }
        let mut sequence: Vec<usize> = (0..count).collect();
        if weighed {
            // above[j] lists the pieces that should be above list[j]. The order follows as many of
            // these as it can: a piece is placed once all that should be above it are, and when
            // none is free, the one with the fewest still waiting.
            let mut waiting = vec![0i32; count];
            let mut after: Vec<Vec<usize>> = vec![Vec::new(); count];
            above = vec![Vec::new(); count];
            for &(i, j) in &pairs {
                let a = list[i];
                let b = list[j];
                if !self.meet(a, b) {
                    continue;
                }
                let (ri, rj) = (find(&mut group, i), find(&mut group, j));
                group[ri] = rj;
                let ab = self.crossings(a, b);
                let ba = self.crossings(b, a);
                if ab == ba {
                    continue;
                }
                let (first, second) = if ab < ba { (i, j) } else { (j, i) };
                above[second].push(first);
                after[first].push(second);
                waiting[second] += 1;
            }
            let mut placed = vec![false; count];
            sequence.clear();
            for _ in 0..count {
                let mut pick = usize::MAX;
                for i in 0..count {
                    if placed[i] {
                        continue;
                    }
                    if pick == usize::MAX || waiting[i] < waiting[pick] {
                        pick = i;
                    }
                    if waiting[i] == 0 {
                        break;
                    }
                }
                placed[pick] = true;
                sequence.push(pick);
                for &j in &after[pick] {
                    waiting[j] -= 1;
                }
            }
        }
        let mut taken: HashSet<i32> = HashSet::new();
        let mut on_track: Vec<Vec<usize>> = Vec::new();
        let mut rows = 0;
        for &at in &sequence {
            let k = list[at];
            let mut t = 0i32;
            if weighed {
                for &i in &above[at] {
                    if self.track[list[i]] >= t {
                        t = self.track[list[i]] + 1;
                    }
                }
                taken.clear();
                for &i in &clashes[at] {
                    taken.insert(self.track[list[i]]);
                }
                while taken.contains(&t) {
                    t += 1;
                }
            } else {
                while on_track.get(t as usize).is_some_and(|row| row.iter().any(|&other| self.clash(k, other))) {
                    t += 1;
                }
                if on_track.len() <= t as usize {
                    on_track.resize(t as usize + 1, Vec::new());
                }
                on_track[t as usize].push(k);
            }
            self.track[k] = t;
            if t >= rows {
                rows = t + 1;
            }
        }
        let mut first = vec![rows; count];
        let mut last = vec![-1i32; count];
        for i in 0..count {
            let root = find(&mut group, i);
            first[root] = first[root].min(self.track[list[i]]);
            last[root] = last[root].max(self.track[list[i]]);
        }
        for i in 0..count {
            let root = find(&mut group, i);
            self.levels[list[i]] = last[root] - first[root] + 1;
            if self.curved {
                self.track[list[i]] -= first[root];
            }
        }
        rows
    }

    fn sample(&self, k: usize, h: f64, out: &mut [f64]) {
        let c = ease(self.to[k] - self.from[k], h);
        for i in 0..=SAMPLES {
            let t = i as f64 / SAMPLES as f64;
            let u = 1.0 - t;
            out[2 * i] = self.from[k] + (self.to[k] - self.from[k]) * t * t * (3.0 - 2.0 * t);
            out[2 * i + 1] = h * (3.0 * c * t * u * u + 3.0 * (1.0 - c) * t * t * u + t * t * t);
        }
    }

    fn near(&self, a: usize, b: usize, height: f64, limit: f64) -> bool {
        let h = (height / 2.0).max(height - 2.0 * LEVEL_LEAD);
        let mut line_a = [0.0; 2 * SAMPLES + 2];
        let mut line_b = [0.0; 2 * SAMPLES + 2];
        self.sample(a, h, &mut line_a);
        self.sample(b, h, &mut line_b);
        reaches(&line_a, &line_b, limit) || reaches(&line_b, &line_a, limit)
    }
}

fn reaches(points: &[f64], line: &[f64], limit: f64) -> bool {
    // Allow rounding error in the cheap distance check before calling hypot.
    let bound = limit * limit * (1.0 + 1e-9);
    for i in 0..=SAMPLES {
        let px = points[2 * i];
        let py = points[2 * i + 1];
        for j in 0..SAMPLES {
            let dx = line[2 * j + 2] - line[2 * j];
            let dy = line[2 * j + 3] - line[2 * j + 1];
            let len = dx * dx + dy * dy;
            let t = (((px - line[2 * j]) * dx + (py - line[2 * j + 1]) * dy) / if len == 0.0 { 1.0 } else { len }).min(1.0).max(0.0);
            let ex = px - line[2 * j] - t * dx;
            let ey = py - line[2 * j + 1] - t * dy;
            if ex * ex + ey * ey < bound && ex.hypot(ey) < limit {
                return true;
            }
        }
    }
    false
}

// Gives every piece that does not run straight down a track across its gap, and counts the
// tracks of each gap. Two pieces in one gap share a track only where they cannot be mistaken
// for each other: where they do not overlap, or where they leave from or arrive at the same
// point, as the branches of a tree do. Which of two goes above the other is whichever makes
// them cross less. With `curved`, the tracks are levels for curves to turn onto, and only the
// pieces that would run along or too near another get one: each group of those counts its
// tracks from its own first. `room` is the height of the gap under each rank, and `tall`
// receives the height that a gap whose curves mostly cross each other should have instead of levels.
pub(super) fn assign_tracks(pieces: &mut Pieces, track_count: &mut [i32], curved: bool, room: &[f64], tall: &mut [f64]) {
    let total = pieces.from.len();
    // Gaps in the order their first piece comes in.
    let mut by_rank: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut slot: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for k in 0..total {
        pieces.track.push(-1);
        pieces.levels.push(0);
        if (pieces.from[k] - pieces.to[k]).abs() < IN_LINE {
            continue;
        }
        let at = *slot.entry(pieces.rank[k]).or_insert_with(|| {
            by_rank.push((pieces.rank[k], Vec::new()));
            by_rank.len() - 1
        });
        by_rank[at].1.push(k);
    }
    if by_rank.is_empty() {
        return;
    }
    // The pieces are compared pair by pair, so where each begins and ends across the flow is worked out once.
    let lo: Vec<f64> = (0..total).map(|k| pieces.from[k].min(pieces.to[k])).collect();
    let hi: Vec<f64> = (0..total).map(|k| pieces.from[k].max(pieces.to[k])).collect();
    let wide = &pieces.wide;
    let mut t = Tracks { from: &pieces.from, to: &pieces.to, lo, hi, track: &mut pieces.track, levels: &mut pieces.levels, curved };
    let heads = |t: &Tracks, k: usize| t.from[k] < t.to[k];

    for (r, mut all) in by_rank {
        if !curved {
            track_count[r] = t.lay(&mut all);
            continue;
        }
        if all.len() > MAX_WEIGHED {
            continue;
        }
        // The pieces that turn onto levels: first those that run far across and have another such
        // piece to be told apart from, then any two that would pass too near each other as plain
        // curves.
        let mut far: Vec<usize> = all.iter().copied().filter(|&k| wide[k] == 1).collect();
        t.lay(&mut far);
        let mut level: HashSet<usize> = far.iter().copied().filter(|&k| t.levels[k] > 1).collect();
        let pairs = t.near_pairs(&all);
        for &(i, j) in &pairs {
            let a = all[i];
            let b = all[j];
            if (level.contains(&a) && level.contains(&b)) || !t.clash(a, b) || heads(&t, a) != heads(&t, b) {
                continue;
            }
            if t.near(a, b, room[r], if wide[a] == 1 || wide[b] == 1 { LEVEL_CLEAR } else { LEVEL_TOUCH }) {
                level.insert(a);
                level.insert(b);
            }
        }
        for &k in &far {
            t.track[k] = -1;
            t.levels[k] = 0;
        }
        if level.is_empty() {
            continue;
        }
        let mut crossing = 0;
        let mut alongside = 0;
        let mut beside: HashSet<usize> = HashSet::new();
        for &(i, j) in &pairs {
            let a = all[i];
            let b = all[j];
            if level.contains(&a) && level.contains(&b) {
                if !t.overlap(a, b) {
                    continue;
                }
                if heads(&t, a) != heads(&t, b) {
                    crossing += 1;
                } else if !t.mates(a, b) {
                    alongside += 1;
                }
            } else if (level.contains(&a) || level.contains(&b)) && t.meet(a, b) {
                beside.insert(a);
                beside.insert(b);
            }
        }
        if crossing > alongside {
            for &k in &all {
                tall[r] = tall[r].max((t.hi[k] - t.lo[k]) * CROSSED_SLOPE);
            }
            continue;
        }
        // With them go the short pieces that could cross one of them, so that each has its place
        // among the levels and crosses none.
        let mut members: Vec<usize> = all.iter().copied().filter(|&k| level.contains(&k) || (wide[k] != 1 && beside.contains(&k))).collect();
        t.lay(&mut members);
        track_count[r] = members.iter().map(|&k| t.levels[k]).max().unwrap_or(0).max(0);
    }
}

// Moves the bends in ranks that hold only bends onto the straight line between the nearest points
// of their edges that are held in place, and marks the ones that can be left out of the route.
// `first_dummy` and `span` give each edge's run of bends, one in every rank. `sep` is the room
// kept between the bends of twin edges, or 0 to leave those out as well.
pub(super) fn straighten(g: &Graph, xs: &mut [f64], empty: &[u8], first_dummy: &[i32], span: &[usize], sep: f64, on_line: &mut [bool]) {
    let mut want = vec![0.0; xs.len()];
    // The points that each bend's edge runs between, across the ranks that hold only bends.
    let mut from = vec![0.0; xs.len()];
    let mut to = vec![0.0; xs.len()];
    for ei in 0..first_dummy.len() {
        if first_dummy[ei] < 0 {
            continue;
        }
        let d0 = first_dummy[ei] as usize;
        let end = d0 + span[ei] - 1;
        // A bend's rank is its place in the run, counted from the rank after the edge's first node.
        let base = g.rank[d0];
        let mut d = d0;
        while d < end {
            if empty[base + d - d0] == 0 {
                d += 1;
                continue;
            }
            let mut last = d;
            while last + 1 < end && empty[base + last + 1 - d0] != 0 {
                last += 1;
            }
            let above = g.pred_e[g.pred_start[d]];
            let below = g.succ_e[g.succ_start[last]];
            let x0 = xs[g.ef[above]] + g.efd[above];
            let x1 = xs[g.et[below]] + g.etd[below];
            let steps = (last - d + 2) as f64;
            for k in d..=last {
                want[k] = x0 + ((x1 - x0) * (k - d + 1) as f64) / steps;
                from[k] = x0;
                to[k] = x1;
            }
            d = last + 1;
        }
    }

    // Nothing in such a rank is in a bend's way, so each one goes onto its line and out of the
    // route. Only twins stay: bends of edges that run between the same two points, which would
    // otherwise be drawn as one. They take places a little apart, around the line.
    let mut twins: Vec<usize> = Vec::new();
    for r in 1..g.max_rank {
        if empty[r] == 0 {
            continue;
        }
        let mut row: Vec<usize> = g.order[g.layer_start[r]..g.layer_start[r + 1]].to_vec();
        row.sort_by(|&a, &b| cmp(from[a], from[b]).then(cmp(to[a], to[b])).then(a.cmp(&b)));
        let mut i = 0;
        while i < row.len() {
            twins.clear();
            let mut j = i;
            while j < row.len() && (from[row[j]] - from[row[i]]).abs() < TWIN && (to[row[j]] - to[row[i]]).abs() < TWIN {
                twins.push(row[j]);
                j += 1;
            }
            // The twins keep the order the rank gave them, so that two edges do not swap sides on the way.
            twins.sort_by_key(|&d| g.pos[d]);
            for (k, &d) in twins.iter().enumerate() {
                xs[d] = want[d] + (k as f64 - (twins.len() - 1) as f64 / 2.0) * sep;
                if twins.len() == 1 || sep == 0.0 {
                    on_line[d] = true;
                }
            }
            i = j;
        }
    }
}

// In a rank that holds labels and bends but no nodes, a bend is there to take its edge straight
// through between the labels. Where the edge would pass clear of every label anyway, the bend
// is left out, and the edge turns in the whole height between the ranks on either side.
pub(super) fn past_labels(g: &Graph, xs: &mut [f64], first_dummy: &[i32], span: &[usize], clear: f64, on_line: &mut [bool]) {
    let mut labelled = vec![false; g.max_rank + 1];
    let mut any = false;
    for r in 1..g.max_rank {
        let mut labels = false;
        let mut others = false;
        for i in g.layer_start[r]..g.layer_start[r + 1] {
            let k = g.kind[g.order[i]];
            if k == kind::LABEL {
                labels = true;
            } else if k != kind::DUMMY {
                others = true;
            }
        }
        if labels && !others {
            labelled[r] = true;
            any = true;
        }
    }
    if !any {
        return;
    }
    for ei in 0..first_dummy.len() {
        if first_dummy[ei] < 0 {
            continue;
        }
        let d0 = first_dummy[ei] as usize;
        let end = d0 + span[ei] - 1;
        for d in d0..end {
            if g.kind[d] != kind::DUMMY || !labelled[g.rank[d]] {
                continue;
            }
            // The points on either side that the edge is held at, past any bends already left out.
            let mut first = d;
            let mut last = d;
            while first > d0 && on_line[first - 1] {
                first -= 1;
            }
            while last + 1 < end && on_line[last + 1] {
                last += 1;
            }
            let above = g.pred_e[g.pred_start[first]];
            let below = g.succ_e[g.succ_start[last]];
            let from = xs[g.ef[above]] + g.efd[above];
            let to = xs[g.et[below]] + g.etd[below];
            let lo = from.min(to) - clear;
            let hi = from.max(to) + clear;
            let r = g.rank[d];
            let blocked = (g.layer_start[r]..g.layer_start[r + 1]).any(|i| {
                let label = g.order[i];
                g.kind[label] == kind::LABEL && xs[label] + g.w[label] / 2.0 > lo && xs[label] - g.w[label] / 2.0 < hi
            });
            if blocked {
                continue;
            }
            on_line[d] = true;
            // The bends left out between the two points lie on the line between them.
            let steps = (last - first + 2) as f64;
            for k in first..=last {
                xs[k] = from + ((to - from) * (k - first + 1) as f64) / steps;
            }
        }
    }
}
