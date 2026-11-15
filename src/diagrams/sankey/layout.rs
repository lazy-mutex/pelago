use std::cmp::Ordering;

use crate::errors::{ErrorCode, PeleError, Result};

use super::db::SankeyDb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Justify,
    Left,
    Right,
    Center,
}

pub struct LayoutOptions {
    pub width: f64,
    pub height: f64,
    pub node_width: f64,
    pub node_padding: f64,
    // Height of a label line; thin nodes are kept at least this far apart where there is room.
    pub label_height: f64,
    pub alignment: Alignment,
}

#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub layer: usize,
    // Total flow through the node, in the diagram's units.
    pub value: f64,
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct PlacedLink {
    pub source: usize,
    pub target: usize,
    pub value: f64,
    pub thickness: f64,
    // Vertical centers of the band where it leaves its source and where it enters its target.
    pub source_y: f64,
    pub target_y: f64,
}

pub struct SankeyLayout {
    pub nodes: Vec<PlacedNode>,
    pub links: Vec<PlacedLink>,
}

const PASSES: usize = 6;

fn cmp_f64_then_idx(va: f64, vb: f64, a: usize, b: usize) -> Ordering {
    let diff = va - vb;
    if diff < 0.0 {
        Ordering::Less
    } else if diff > 0.0 {
        Ordering::Greater
    } else {
        a.cmp(&b)
    }
}

fn sort_indices(list: &mut [usize], compare: impl Fn(usize, usize) -> Ordering) {
    if list.len() > 8 {
        list.sort_by(|&a, &b| compare(a, b));
        return;
    }
    for i in 1..list.len() {
        let item = list[i];
        let mut j = i;
        while j > 0 && compare(list[j - 1], item) == Ordering::Greater {
            list[j] = list[j - 1];
            j -= 1;
        }
        list[j] = item;
    }
}

// Places nodes in columns by their distance from a source, sizes them by their flow, and
// then lets each column settle next to its neighbours so that the bands cross less.
pub fn layout_sankey(model: &SankeyDb, options: &LayoutOptions) -> Result<SankeyLayout> {
    let n = model.nodes.len();
    let mut links: Vec<PlacedLink> = model
        .links
        .iter()
        .map(|link| PlacedLink {
            source: link.source.index,
            target: link.target.index,
            value: if link.value.is_finite() && link.value > 0.0 { link.value } else { 0.0 },
            thickness: 0.0,
            source_y: 0.0,
            target_y: 0.0,
        })
        .collect();

    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, link) in links.iter().enumerate() {
        outgoing[link.source].push(k);
        incoming[link.target].push(k);
    }

    // Longest distance from a source, in topological order; what is left over lies on a cycle.
    let mut depth = vec![0usize; n];
    let mut pending = vec![0usize; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    for i in 0..n {
        pending[i] = incoming[i].len();
        if pending[i] == 0 {
            order.push(i);
        }
    }
    let mut q = 0usize;
    while q < order.len() {
        let i = order[q];
        for &k in &outgoing[i] {
            let t = links[k].target;
            if depth[t] < depth[i] + 1 {
                depth[t] = depth[i] + 1;
            }
            pending[t] -= 1;
            if pending[t] == 0 {
                order.push(t);
            }
        }
        q += 1;
    }
    if order.len() < n {
        // Every node left over has a predecessor that is left over too; walking back finds the loop.
        let mut stuck = 0usize;
        while pending[stuck] == 0 {
            stuck += 1;
        }
        let mut seen = vec![false; n];
        while !seen[stuck] {
            seen[stuck] = true;
            for &k in &incoming[stuck] {
                if pending[links[k].source] > 0 {
                    stuck = links[k].source;
                    break;
                }
            }
        }
        return Err(PeleError::new(
            format!(
                "Sankey diagrams cannot have circular links, and \"{}\" is part of one.",
                model.nodes[stuck].id
            ),
            ErrorCode::Semantic,
        )
        .of("sankey"));
    }

    let mut layers = 1usize;
    for i in 0..n {
        if depth[i] + 1 > layers {
            layers = depth[i] + 1;
        }
    }
    let mut reach = vec![0usize; n];
    for &i in order.iter().rev() {
        for &k in &outgoing[i] {
            let r = reach[links[k].target] + 1;
            if reach[i] < r {
                reach[i] = r;
            }
        }
    }

    // Flows are scaled to the largest one, so that sums of huge values stay finite.
    let mut largest = 0.0f64;
    for link in &links {
        if link.value > largest {
            largest = link.value;
        }
    }
    let unit = if largest > 0.0 { 1.0 / largest } else { 0.0 };

    let mut nodes: Vec<PlacedNode> = Vec::with_capacity(n);
    let mut flow = vec![0.0f64; n];
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); layers];
    let step = if layers > 1 {
        (options.width - options.node_width) / (layers - 1) as f64
    } else {
        0.0
    };
    for i in 0..n {
        let layer = match options.alignment {
            Alignment::Left => depth[i],
            Alignment::Right => layers - 1 - reach[i],
            Alignment::Center => {
                let mut l = depth[i];
                if incoming[i].is_empty() && !outgoing[i].is_empty() {
                    l = layers;
                    for &k in &outgoing[i] {
                        let d = depth[links[k].target] - 1;
                        if d < l {
                            l = d;
                        }
                    }
                }
                l
            }
            Alignment::Justify => {
                if !outgoing[i].is_empty() {
                    depth[i]
                } else {
                    layers - 1
                }
            }
        };
        let mut into = 0.0f64;
        let mut out = 0.0f64;
        let mut value = 0.0f64;
        let mut value_out = 0.0f64;
        for &k in &incoming[i] {
            into += links[k].value * unit;
            value += links[k].value;
        }
        for &k in &outgoing[i] {
            out += links[k].value * unit;
            value_out += links[k].value;
        }
        flow[i] = into.max(out);
        nodes.push(PlacedNode {
            layer,
            value: value.max(value_out),
            x: layer as f64 * step,
            y: 0.0,
            height: 0.0,
        });
        columns[layer].push(i);
    }

    let mut tallest = 1usize;
    for column in &columns {
        if column.len() > tallest {
            tallest = column.len();
        }
    }
    let gap = if tallest > 1 {
        options.node_padding.min(options.height / (tallest - 1) as f64)
    } else {
        options.node_padding
    };
    let mut scale = f64::INFINITY;
    for column in &columns {
        let mut sum = 0.0f64;
        for &i in column {
            sum += flow[i];
        }
        if sum > 0.0 {
            let candidate = (options.height - (column.len().saturating_sub(1) as f64) * gap) / sum;
            scale = scale.min(candidate);
        }
    }
    if !scale.is_finite() || scale < 0.0 {
        scale = 0.0;
    }

    // A thin node still needs room for its label. Where every column can afford it, each node
    // claims at least a label's height, and the flows are scaled down to make that fit.
    let mut claim = (options.label_height - gap).max(0.0);
    let fits = |s: f64, claim_val: f64| -> bool {
        for column in &columns {
            if column.is_empty() {
                continue;
            }
            let mut extent = (column.len() as f64 - 1.0) * gap;
            for &i in column {
                extent += (flow[i] * s).max(claim_val);
            }
            if extent > options.height + 1e-6 {
                return false;
            }
        }
        true
    };
    if claim > 0.0 && !fits(scale, claim) {
        if fits(0.0, claim) {
            let mut low = 0.0f64;
            for _ in 0..40 {
                let mid = (low + scale) / 2.0;
                if fits(mid, claim) {
                    low = mid;
                } else {
                    scale = mid;
                }
            }
            scale = low;
        } else {
            claim = 0.0;
        }
    }

    // The room a node keeps clear above and below itself.
    let mut margin = vec![0.0f64; n];
    for link in &mut links {
        link.thickness = link.value * unit * scale;
    }
    for column in &columns {
        let mut y = 0.0f64;
        for &i in column {
            nodes[i].height = flow[i] * scale;
            margin[i] = (claim - nodes[i].height).max(0.0) / 2.0;
            nodes[i].y = y + margin[i];
            y += nodes[i].height + 2.0 * margin[i] + gap;
        }
        // Spare room is shared out between the nodes of a column.
        let spare = (options.height - y + gap) / (column.len() + 1) as f64;
        for (k, &i) in column.iter().enumerate() {
            nodes[i].y += spare * (k + 1) as f64;
        }
    }

    let center_of = |nodes_slice: &[PlacedNode], idx: usize| -> f64 {
        nodes_slice[idx].y + nodes_slice[idx].height / 2.0
    };

    let mut source_offset = vec![0.0f64; links.len()];
    let mut target_offset = vec![0.0f64; links.len()];

    let stack_out = |_i: usize,
                     outgoing_list: &mut [usize],
                     nodes_slice: &[PlacedNode],
                     links_slice: &[PlacedLink],
                     src_off: &mut [f64]| {
        let mut y = 0.0f64;
        sort_indices(outgoing_list, |a, b| {
            cmp_f64_then_idx(
                center_of(nodes_slice, links_slice[a].target),
                center_of(nodes_slice, links_slice[b].target),
                a,
                b,
            )
        });
        for &k in outgoing_list.iter() {
            src_off[k] = y + links_slice[k].thickness / 2.0;
            y += links_slice[k].thickness;
        }
    };

    let stack_in = |i: usize,
                    incoming_list: &mut [usize],
                    nodes_slice: &[PlacedNode],
                    links_slice: &[PlacedLink],
                    tgt_off: &mut [f64]| {
        let _ = i;
        let mut y = 0.0f64;
        sort_indices(incoming_list, |a, b| {
            cmp_f64_then_idx(
                center_of(nodes_slice, links_slice[a].source),
                center_of(nodes_slice, links_slice[b].source),
                a,
                b,
            )
        });
        for &k in incoming_list.iter() {
            tgt_off[k] = y + links_slice[k].thickness / 2.0;
            y += links_slice[k].thickness;
        }
    };

    let stack_all = |outgoing_all: &mut [Vec<usize>],
                     incoming_all: &mut [Vec<usize>],
                     nodes_slice: &[PlacedNode],
                     links_slice: &[PlacedLink],
                     src_off: &mut [f64],
                     tgt_off: &mut [f64]| {
        for i in 0..n {
            stack_out(i, &mut outgoing_all[i], nodes_slice, links_slice, src_off);
            stack_in(i, &mut incoming_all[i], nodes_slice, links_slice, tgt_off);
        }
    };

    let settle = |column: &mut [usize],
                  pull: f64,
                  forward: bool,
                  outgoing_all: &mut [Vec<usize>],
                  incoming_all: &mut [Vec<usize>],
                  nodes_slice: &mut [PlacedNode],
                  links_slice: &[PlacedLink],
                  src_off: &mut [f64],
                  tgt_off: &mut [f64]| {
        for &i in column.iter() {
            if forward {
                if incoming_all[i].is_empty() {
                    continue;
                }
                stack_in(i, &mut incoming_all[i], nodes_slice, links_slice, tgt_off);
            } else {
                if outgoing_all[i].is_empty() {
                    continue;
                }
                stack_out(i, &mut outgoing_all[i], nodes_slice, links_slice, src_off);
            }
            let list = if forward { &incoming_all[i] } else { &outgoing_all[i] };
            let mut weighted = 0.0f64;
            let mut weight = 0.0f64;
            let mut plain = 0.0f64;
            for &k in list {
                let level = if forward {
                    nodes_slice[links_slice[k].source].y + src_off[k] - tgt_off[k]
                } else {
                    nodes_slice[links_slice[k].target].y + tgt_off[k] - src_off[k]
                };
                weighted += level * links_slice[k].thickness;
                weight += links_slice[k].thickness;
                let other = if forward { links_slice[k].source } else { links_slice[k].target };
                plain += center_of(nodes_slice, other) - nodes_slice[i].height / 2.0;
            }
            let wanted = if weight > 0.0 {
                weighted / weight
            } else {
                plain / list.len() as f64
            };
            nodes_slice[i].y += (wanted - nodes_slice[i].y) * pull;
        }
        sort_indices(column, |a, b| {
            cmp_f64_then_idx(center_of(nodes_slice, a), center_of(nodes_slice, b), a, b)
        });
        let mut y = 0.0f64;
        for &i in column.iter() {
            if nodes_slice[i].y - margin[i] < y {
                nodes_slice[i].y = y + margin[i];
            }
            y = nodes_slice[i].y + nodes_slice[i].height + margin[i] + gap;
        }
        y = options.height;
        for &i in column.iter().rev() {
            if nodes_slice[i].y + nodes_slice[i].height + margin[i] > y {
                nodes_slice[i].y = y - nodes_slice[i].height - margin[i];
            }
            y = nodes_slice[i].y - margin[i] - gap;
        }
    };

    stack_all(
        &mut outgoing,
        &mut incoming,
        &nodes,
        &links,
        &mut source_offset,
        &mut target_offset,
    );
    for pass in 0..PASSES {
        let pull = 0.99f64.powi(pass as i32);
        if layers >= 2 {
            for l in (0..=layers - 2).rev() {
                settle(
                    &mut columns[l],
                    pull,
                    false,
                    &mut outgoing,
                    &mut incoming,
                    &mut nodes,
                    &links,
                    &mut source_offset,
                    &mut target_offset,
                );
            }
        }
        stack_all(
            &mut outgoing,
            &mut incoming,
            &nodes,
            &links,
            &mut source_offset,
            &mut target_offset,
        );
        for l in 1..layers {
            settle(
                &mut columns[l],
                pull,
                true,
                &mut outgoing,
                &mut incoming,
                &mut nodes,
                &links,
                &mut source_offset,
                &mut target_offset,
            );
        }
        stack_all(
            &mut outgoing,
            &mut incoming,
            &nodes,
            &links,
            &mut source_offset,
            &mut target_offset,
        );
    }

    for k in 0..links.len() {
        links[k].source_y = nodes[links[k].source].y + source_offset[k];
        links[k].target_y = nodes[links[k].target].y + target_offset[k];
    }

    Ok(SankeyLayout { nodes, links })
}
