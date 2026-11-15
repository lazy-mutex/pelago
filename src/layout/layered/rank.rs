use super::{kind, LEdge, LNode};

// Reverses the edges that close a cycle, found by depth-first search in declaration order.
pub fn orient(nodes: &[LNode], edges: &mut [LEdge]) {
    let n = nodes.len();
    let m = edges.len();
    let mut start = vec![0usize; n + 1];
    for e in edges.iter_mut() {
        e.reversed = nodes[e.head].kind == kind::START_PORT || nodes[e.tail].kind == kind::END_PORT;
        if !e.reversed {
            start[e.tail + 1] += 1;
        }
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut out = vec![0usize; start[n]];
    {
        let mut fill = start[..n].to_vec();
        for i in 0..m {
            if !edges[i].reversed {
                out[fill[edges[i].tail]] = i;
                fill[edges[i].tail] += 1;
            }
        }
    }
    let mut state = vec![0u8; n];
    let mut cursor = start[..n].to_vec();
    let mut stack: Vec<usize> = Vec::new();
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        stack.push(root);
        state[root] = 1;
        while let Some(&v) = stack.last() {
            if cursor[v] < start[v + 1] {
                let e = &mut edges[out[cursor[v]]];
                cursor[v] += 1;
                let w = e.head;
                if state[w] == 1 {
                    e.reversed = true;
                } else if state[w] == 0 {
                    state[w] = 1;
                    stack.push(w);
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
}

pub fn rank(nodes: &mut [LNode], edges: &[LEdge], step: i32) {
    let n = nodes.len();
    let m = edges.len();
    let mut from = vec![0usize; m];
    let mut to = vec![0usize; m];
    let mut len = vec![0i32; m];
    let mut out_start = vec![0usize; n + 1];
    let mut in_start = vec![0usize; n + 1];
    for (i, e) in edges.iter().enumerate() {
        let (a, b) = if e.reversed { (e.head, e.tail) } else { (e.tail, e.head) };
        from[i] = a;
        to[i] = b;
        len[i] = e.minlen.max(1) * step;
        out_start[a + 1] += 1;
        in_start[b + 1] += 1;
    }
    for i in 0..n {
        out_start[i + 1] += out_start[i];
        in_start[i + 1] += in_start[i];
    }
    let mut out_e = vec![0usize; m];
    let mut in_e = vec![0usize; m];
    {
        let mut of = out_start[..n].to_vec();
        let mut inf = in_start[..n].to_vec();
        for i in 0..m {
            out_e[of[from[i]]] = i;
            of[from[i]] += 1;
            in_e[inf[to[i]]] = i;
            inf[to[i]] += 1;
        }
    }

    let mut r = vec![0i32; n];
    // Start ports go above every node later, so they don't push their targets down a rank.
    for i in 0..n {
        if nodes[i].kind == kind::START_PORT {
            r[i] = -step;
        }
    }
    let mut indeg: Vec<usize> = (0..n).map(|i| in_start[i + 1] - in_start[i]).collect();
    let mut topo: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut q = 0;
    while q < topo.len() {
        let v = topo[q];
        q += 1;
        for k in out_start[v]..out_start[v + 1] {
            let ei = out_e[k];
            let w = to[ei];
            if r[v] + len[ei] > r[w] {
                r[w] = r[v] + len[ei];
            }
            indeg[w] -= 1;
            if indeg[w] == 0 {
                topo.push(w);
            }
        }
    }

    // Longest-path ranking leaves loosely attached nodes far from their neighbours.
    // Move each node toward the side that has more edges.
    for _pass in 0..8 {
        let mut changed = false;
        for &v in topo.iter().rev() {
            let ins = in_start[v + 1] - in_start[v];
            let outs = out_start[v + 1] - out_start[v];
            if ins == 0 && outs == 0 {
                continue;
            }
            let lo = (in_start[v]..in_start[v + 1]).map(|k| r[from[in_e[k]]] + len[in_e[k]]).max();
            let hi = (out_start[v]..out_start[v + 1]).map(|k| r[to[out_e[k]]] - len[out_e[k]]).min();
            let mut next = r[v];
            if outs > ins {
                if let Some(hi) = hi {
                    next = hi;
                }
            } else if ins > outs {
                if let Some(lo) = lo {
                    next = lo;
                }
            }
            if next != r[v] {
                r[v] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut min = i32::MAX;
    let mut max = i32::MIN;
    let mut start_ports = false;
    for i in 0..n {
        let k = nodes[i].kind;
        if k == kind::START_PORT {
            start_ports = true;
        }
        if k == kind::START_PORT || k == kind::END_PORT {
            continue;
        }
        min = min.min(r[i]);
        max = max.max(r[i]);
    }
    if min == i32::MAX {
        min = 0;
        max = 0;
    }
    for i in 0..n {
        if nodes[i].kind == kind::START_PORT {
            r[i] = min - step;
        } else if nodes[i].kind == kind::END_PORT {
            r[i] = max + step;
        }
    }
    if start_ports {
        min -= step;
    }
    for i in 0..n {
        nodes[i].rank = r[i] - min;
    }
}
