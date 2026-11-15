use super::db::StateDb;
use super::graph::{StateEdge, StateNode};
use crate::layout::compound::{cnode, compound_layout, direction, CEdge, CNode, Dir};
use crate::layout::layered::LayeredOptions;
use crate::options::{runs_across, turn_to_fit, LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num, sign};
use crate::svg::edges::{
    casings_svg, edge_casing, edge_label_svg, marker, marker_trim, over_fills, route_path,
    EdgePath, Fill, SideReach,
};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::shapes::{end_side, BoxAt};
use crate::svg::theme::{class_names, resolve_style, sets_fill, ResolvedStyle};
use crate::svg::title::with_title;
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::url::{linked, safe_url};
use crate::value::Value;
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 44.0;
const GROUP_PAD: f64 = 16.0;
const REGION_PAD: f64 = 10.0;
const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 10.0;
const LOOP: f64 = 26.0;
const FOLD: f64 = 8.0;
const NOTE_WRAP: f64 = 320.0;
const CORNER: f64 = 10.0;
const BAR: f64 = 8.0;
const SURFACE: &str = " fill=\"var(--_s)\" stroke=\"var(--_b)\"";
const RULE: &str = " fill=\"none\" stroke=\"var(--_b)\"";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(usize)]
enum K {
    State = 0,
    Described = 1,
    Start = 2,
    End = 3,
    Choice = 4,
    Fork = 5,
    Join = 6,
    Note = 7,
    Composite = 8,
    Region = 9,
    Hidden = 10,
}

fn kind_from_shape(shape: Option<&str>) -> K {
    match shape {
        Some("rectWithTitle") => K::Described,
        Some("stateStart") => K::Start,
        Some("stateEnd") => K::End,
        Some("choice") => K::Choice,
        Some("fork") => K::Fork,
        Some("join") => K::Join,
        Some("note") => K::Note,
        Some("divider") => K::Hidden,
        _ => K::State,
    }
}

const CLASSES: [&str; 9] = [
    "pele-node pele-state",
    "pele-node pele-state",
    "pele-node pele-state-start",
    "pele-node pele-state-end",
    "pele-node pele-state-choice",
    "pele-node pele-state-fork",
    "pele-node pele-state-join",
    "pele-note",
    "pele-cluster pele-state-composite",
];

struct View<'a> {
    // Absent for the unseen helpers the layout needs.
    node: Option<&'a StateNode>,
    kind: K,
    style: Option<ResolvedStyle>,
    label: Label,
    lines: Vec<Label>,
    w: f64,
    h: f64,
    title_h: f64,
}

fn vertical(dir: Option<Dir>) -> bool {
    matches!(dir, Some(Dir::TB | Dir::BT))
}

fn number_option(config: &Value, key: &str, fallback: f64) -> f64 {
    match config.get("state").get(key).as_f64() {
        Some(v) if v > 0.0 => v.min(2000.0),
        _ => fallback,
    }
}

// Mermaid keeps the quotes of a click statement's strings and strips them when it draws.
fn unquote(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut from = 0;
    let mut to = bytes.len();
    while from < to && bytes[from] == b'"' {
        from += 1;
    }
    while to > from && bytes[to - 1] == b'"' {
        to -= 1;
    }
    &text[from..to]
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64, rest: &str) -> String {
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}/>",
        num(x),
        num(y),
        num(w),
        num(h),
        rest
    )
}

fn group(dir: Dir, pad_x: f64, pad_top: f64, pad_bottom: f64) -> CNode {
    let mut c = cnode(0.0, 0.0, -1);
    c.is_group = true;
    c.dir = Some(dir);
    c.pad_x = pad_x;
    c.pad_top = pad_top;
    c.pad_bottom = pad_bottom;
    c
}

fn box_at(c: &CNode) -> BoxAt {
    BoxAt { x: c.x, y: c.y, w: c.w, h: c.h, is_group: c.is_group }
}

fn strip_mermaid_classes(raw: &str) -> String {
    raw.split(' ')
        .filter(|&w| {
            !matches!(
                w,
                "statediagram-state"
                    | "statediagram-cluster-alt"
                    | "statediagram-cluster"
                    | "statediagram-note"
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn render_state(db: &StateDb, config: &Value, options: &RenderOptions) -> Rendered {
    let dir = db.direction();
    if !runs_across(Some(dir)) {
        return draw(db, config, options, false);
    }
    turn_to_fit(options, &|down| draw(db, config, options, down))
}

// `turned` draws a diagram that runs across as one that runs down.
fn draw(db: &StateDb, config: &Value, options: &RenderOptions, turned: bool) -> Rendered {
    let graph = &db.graph;
    // State diagrams follow the flowchart curve.
    let curve = config.get("flowchart").get("curve").as_str();
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let wrap_width = number_option(config, "wrappingWidth", 200.0);
    let pad = options.padding.unwrap_or(8.0);
    let root_dir = if turned { Dir::TB } else { direction(Some(db.direction())) };
    let no_label = layout_label(None, false, measurer, size, 0.0, 0);
    let text = |raw: Option<&str>, font_size: f64, wrap: f64, st: u8| -> Label {
        layout_label(raw, true, measurer, font_size, wrap, st)
    };

    let mut index: HashMap<String, usize> = HashMap::new();
    let mut views: Vec<View<'_>> = Vec::new();
    let mut cnodes: Vec<CNode> = Vec::new();

    for node in &graph.nodes {
        if node.shape.as_deref() == Some("noteGroup") {
            continue;
        }
        let kind = if node.is_group {
            if node.shape.as_deref() == Some("divider") {
                K::Region
            } else {
                K::Composite
            }
        } else {
            kind_from_shape(node.shape.as_deref())
        };
        let idx = views.len();
        views.push(View {
            node: Some(node),
            kind,
            style: None,
            label: no_label.clone(),
            lines: Vec::new(),
            w: 0.0,
            h: 0.0,
            title_h: 0.0,
        });
        index.insert(node.id.clone(), idx);
    }
    let states = views.len();

    let mut logical: Vec<i32> = Vec::with_capacity(states);
    for i in 0..states {
        let parent_id = views[i].node.unwrap().parent_id.as_deref();
        let parent = parent_id.and_then(|pid| index.get(pid).copied());
        let p_val = match parent {
            Some(p) if p != i && views[p].node.unwrap().is_group => p as i32,
            _ => -1,
        };
        logical.push(p_val);
    }

    // Composites that are redeclared inside each other form a ring with no way in from the root.
    // One member of each ring moves to the root, which leaves a tree.
    {
        let mut kids: Vec<Vec<usize>> = vec![Vec::new(); states];
        let mut queue: Vec<usize> = Vec::new();
        let mut placed: Vec<bool> = vec![false; states];
        for i in 0..states {
            if logical[i] == -1 {
                placed[i] = true;
                queue.push(i);
            } else {
                kids[logical[i] as usize].push(i);
            }
        }
        let mut done = 0usize;
        let settle = |done: &mut usize, queue: &mut Vec<usize>, placed: &mut [bool], kids: &[Vec<usize>]| {
            while *done < queue.len() {
                let cur = queue[*done];
                *done += 1;
                for &kid in &kids[cur] {
                    if placed[kid] {
                        continue;
                    }
                    placed[kid] = true;
                    queue.push(kid);
                }
            }
        };
        settle(&mut done, &mut queue, &mut placed, &kids);
        let mut seen: Vec<i32> = vec![-1; states];
        for i in 0..states {
            if placed[i] {
                continue;
            }
            let mut ring = i;
            while seen[ring] != i as i32 {
                seen[ring] = i as i32;
                ring = logical[ring] as usize;
            }
            logical[ring] = -1;
            placed[ring] = true;
            queue.push(ring);
            settle(&mut done, &mut queue, &mut placed, &kids);
        }
    }

    let mut owners: Vec<i32> = vec![-1; states];
    for i in 0..states {
        let note_for = views[i].node.unwrap().note_for.as_deref();
        if let Some(owner) = note_for.and_then(|nf| index.get(nf).copied()) {
            owners[i] = owner as i32;
            logical[i] = logical[owner];
        }
    }

    // As in Mermaid, a composite that sets no direction runs top to bottom whatever surrounds it.
    let inner_dir = |views: &[View<'_>], composite: usize| -> Dir {
        direction(views[composite].node.unwrap().dir.as_deref())
    };
    // Whether the flow around a state runs top to bottom or bottom to top.
    let across = |views: &[View<'_>], logical: &[i32], i: usize| -> bool {
        let d = if logical[i] == -1 {
            root_dir
        } else {
            inner_dir(views, logical[i] as usize)
        };
        vertical(Some(d))
    };

    let mut degree: Vec<i32> = vec![0; states];
    let mut noted: Vec<bool> = vec![false; states];
    // When each state first takes part in a transition. Layout follows this order rather than
    // the order of declaration, so a state declared up front does not become the top of the flow.
    let mut appear: Vec<f64> = vec![f64::INFINITY; states];
    let mut appeared = 0usize;
    let mut counts: Vec<i32> = vec![0; 2 * states];
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(&edge.start), index.get(&edge.end)) else {
            continue;
        };
        if edge.pattern == Some("dashed") {
            let target = if views[src].kind == K::Note { dst } else { src };
            noted[target] = true;
            continue;
        }
        counts[2 * src] += 1;
        degree[src] = degree[src].max(counts[2 * src]);
        counts[2 * dst + 1] += 1;
        degree[dst] = degree[dst].max(counts[2 * dst + 1]);
        if appear[src] == f64::INFINITY {
            appear[src] = appeared as f64;
            appeared += 1;
        }
        if appear[dst] == f64::INFINITY {
            appear[dst] = appeared as f64;
            appeared += 1;
        }
    }

    for i in 0..states {
        let is_across = across(&views, &logical, i);
        let dir_i = inner_dir(&views, i);
        let node = views[i].node.unwrap();
        let resolved = if !node.css_styles.is_empty() {
            let mut combined = node.css_compiled_styles.clone();
            combined.extend(node.css_styles.iter().cloned());
            resolve_style(&combined)
        } else {
            resolve_style(&node.css_compiled_styles)
        };
        let text_style = (if resolved.bold { style::BOLD } else { 0 })
            | (if resolved.italic { style::ITALIC } else { 0 });
        let font_size = resolved.font_size.unwrap_or(size);
        views[i].style = Some(resolved);

        let mut c_opt: Option<CNode> = None;
        match views[i].kind {
            K::Composite => {
                let lbl = text(node.label.as_deref(), font_size, 4000.0, text_style);
                let title_h = if lbl.height > 0.0 { lbl.height + 12.0 } else { 0.0 };
                let mut g = group(dir_i, GROUP_PAD, title_h + GROUP_PAD, GROUP_PAD);
                g.min_w = lbl.width + 2.0 * GROUP_PAD;
                views[i].label = lbl;
                views[i].title_h = title_h;
                c_opt = Some(g);
            }
            K::Region => {
                c_opt = Some(group(dir_i, REGION_PAD, REGION_PAD, REGION_PAD));
            }
            K::Start => {
                views[i].w = 14.0;
                views[i].h = 14.0;
            }
            K::End => {
                views[i].w = 18.0;
                views[i].h = 18.0;
            }
            K::Choice => {
                views[i].w = 30.0;
                views[i].h = 30.0;
            }
            K::Fork | K::Join => {
                let length = clamp(40.0 * degree[i] as f64 + 16.0, 72.0, 320.0);
                views[i].w = if is_across { length } else { BAR };
                views[i].h = if is_across { BAR } else { length };
            }
            K::Hidden => {}
            K::Note => {
                let lbl = text(node.label.as_deref(), small, wrap_width.max(NOTE_WRAP), 0);
                views[i].w = lbl.width + 24.0;
                views[i].h = lbl.height + 16.0;
                views[i].label = lbl;
            }
            K::State | K::Described => {
                let lbl = text(node.label.as_deref(), font_size, wrap_width, text_style);
                let mut width = lbl.width;
                let mut height = lbl.height + 2.0 * PAD_Y;
                views[i].label = lbl;
                if views[i].kind == K::Described {
                    height += 14.0;
                    for line in &node.description {
                        let line_lbl = text(
                            Some(if line.is_empty() { " " } else { line.as_str() }),
                            small,
                            wrap_width,
                            text_style,
                        );
                        if line_lbl.width > width {
                            width = line_lbl.width;
                        }
                        height += line_lbl.height;
                        views[i].lines.push(line_lbl);
                    }
                }
                views[i].w = (width + 2.0 * PAD_X).max(56.0);
                views[i].h = height;
            }
        }
        let mut c = c_opt.unwrap_or_else(|| cnode(views[i].w, views[i].h, -1));
        // The transitions on one side of a state's box spread along it, clear of its corners.
        if views[i].kind == K::State || views[i].kind == K::Described {
            c.span = (if is_across { views[i].w } else { views[i].h }) - 24.0;
        }
        cnodes.push(c);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut drawn: Vec<&StateEdge> = Vec::new();
    // The transitions each state makes to itself: how many, their widest label, and their labels' height.
    let mut loop_count: Vec<i32> = vec![0; states];
    let mut loop_width: Vec<f64> = vec![0.0; states];
    let mut loop_height: Vec<f64> = vec![0.0; states];
    // A transition between a composite and a state inside it runs between that state and a point
    // just inside the composite, and is then drawn on to the composite's border.
    let mut anchors: HashMap<usize, usize> = HashMap::new();

    let encloses = |cnodes: &[CNode], logical: &[i32], composite: usize, item: usize| -> bool {
        if !cnodes[composite].is_group {
            return false;
        }
        let mut p = logical[item];
        while p != -1 {
            if p as usize == composite {
                return true;
            }
            p = logical[p as usize];
        }
        false
    };

    for edge in &graph.edges {
        let (Some(&raw_src), Some(&raw_dst)) = (index.get(&edge.start), index.get(&edge.end)) else {
            continue;
        };
        let mut src = raw_src;
        let mut dst = raw_dst;
        let label = text(Some(&edge.label), small, wrap_width, 0);
        let label_w = if label.width > 0.0 { label.width + 12.0 } else { 0.0 };
        if src == dst {
            loop_count[src] += 1;
            loop_width[src] = loop_width[src].max(label_w);
            loop_height[src] += label.height;
        } else if encloses(&cnodes, &logical, src, dst) {
            anchors.insert(cedges.len(), src);
            appear.push(-1.0);
            let idx = views.len();
            views.push(View {
                node: None,
                kind: K::Hidden,
                style: None,
                label: no_label.clone(),
                lines: Vec::new(),
                w: 0.0,
                h: 0.0,
                title_h: 0.0,
            });
            cnodes.push(cnode(0.0, 0.0, src as i32));
            src = idx;
        } else if encloses(&cnodes, &logical, dst, src) {
            anchors.insert(cedges.len(), dst);
            appear.push(f64::INFINITY);
            let idx = views.len();
            views.push(View {
                node: None,
                kind: K::Hidden,
                style: None,
                label: no_label.clone(),
                lines: Vec::new(),
                w: 0.0,
                h: 0.0,
                title_h: 0.0,
            });
            cnodes.push(cnode(0.0, 0.0, dst as i32));
            dst = idx;
        }
        drawn.push(edge);
        let label_h = if label.height > 0.0 { label.height + 4.0 } else { 0.0 };
        edge_labels.push(label);
        cedges.push(CEdge {
            src,
            dst,
            minlen: 1,
            label_w,
            label_h,
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        });
    }

    // A state with notes, or a composite with self-transitions, sits in an unseen group of its
    // own: the notes go beside the state, across the flow, without disturbing its transitions.
    for i in 0..states {
        if owners[i] != -1 {
            continue;
        }
        let is_across = across(&views, &logical, i);
        // Loops hang off the right, or below when the flow is horizontal, with their labels beyond them.
        let mut room = 0.0;
        if loop_count[i] > 0 {
            let reach = LOOP + (loop_count[i] - 1) as f64 * 8.0;
            room = if is_across {
                reach + loop_width[i]
            } else {
                reach * 0.75 + 4.0 + loop_height[i]
            };
            if !cnodes[i].is_group {
                if is_across {
                    cnodes[i].w += 2.0 * room;
                    cnodes[i].h = cnodes[i].h.max(loop_height[i]);
                } else {
                    cnodes[i].h += 2.0 * room;
                    cnodes[i].w = cnodes[i].w.max(loop_width[i]);
                }
                room = 0.0;
            }
        }
        if !noted[i] && room == 0.0 {
            cnodes[i].parent = logical[i];
            continue;
        }
        let mut c = if is_across {
            group(Dir::LR, room, 0.0, 0.0)
        } else {
            group(Dir::TB, 0.0, room, room)
        };
        c.parent = logical[i];
        appear.push(appear[i]);
        let helper = views.len();
        views.push(View {
            node: None,
            kind: K::Hidden,
            style: None,
            label: no_label.clone(),
            lines: Vec::new(),
            w: 0.0,
            h: 0.0,
            title_h: 0.0,
        });
        cnodes.push(c);
        cnodes[i].parent = helper as i32;
    }
    for i in 0..states {
        if owners[i] != -1 {
            cnodes[i].parent = cnodes[owners[i] as usize].parent;
        }
    }

    {
        // Numbers the tree so that, at every level, initial states come first and the rest follow
        // in the order the transitions introduce them.
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); cnodes.len()];
        let mut top: Vec<usize> = Vec::new();
        for i in 0..cnodes.len() {
            let p = cnodes[i].parent;
            if p >= 0 {
                members[p as usize].push(i);
            } else {
                top.push(i);
            }
        }
        let by_flow = |a: &usize, b: &usize| -> std::cmp::Ordering {
            let init_a = if views[*a].kind == K::Start { 0 } else { 1 };
            let init_b = if views[*b].kind == K::Start { 0 } else { 1 };
            init_a
                .cmp(&init_b)
                .then_with(|| {
                    if appear[*a] == appear[*b] {
                        std::cmp::Ordering::Equal
                    } else if appear[*a] < appear[*b] {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                })
                .then_with(|| a.cmp(b))
        };
        let mut seq = 0usize;
        top.sort_by(by_flow);
        top.reverse();
        let mut pending = top;
        while let Some(i) = pending.pop() {
            cnodes[i].seq = seq as f64;
            seq += 1;
            let inside = &mut members[i];
            inside.sort_by(by_flow);
            for k in (0..inside.len()).rev() {
                pending.push(inside[k]);
            }
        }
    }

    let layout = compound_layout(
        &mut cnodes,
        &mut cedges,
        root_dir,
        &LayeredOptions {
            node_sep: number_option(config, "nodeSpacing", NODE_SEP),
            edge_sep: EDGE_SEP,
            rank_sep: number_option(config, "rankSpacing", RANK_SEP),
            port_sep: 20.0,
            tracks: matches!(curve, Some("step" | "stepBefore" | "stepAfter")),
            straight: curve == Some("linear"),
            head_room: marker_trim("arrow_point"),
            ..Default::default()
        },
    );

    let width = (layout.width + 2.0 * pad).ceil();
    let height = (layout.height + 2.0 * pad).ceil();
    let mut links: Vec<LinkInfo> = Vec::new();

    // Route points by rounded y, to find the edges that cross a composite's title bar.
    let mut crossings: HashMap<i64, Vec<f64>> = HashMap::new();
    for e in &cedges {
        let route = &e.route;
        if route.len() > 6 {
            let mut k = 3;
            while k < route.len() - 3 {
                let key = js_round(route[k + 1]) as i64;
                crossings.entry(key).or_default().push(route[k]);
                k += 3;
            }
        }
    }

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); cnodes.len()];
    let mut order: Vec<usize> = Vec::new();
    for i in 0..cnodes.len() {
        let p = cnodes[i].parent;
        if p >= 0 {
            children[p as usize].push(i);
        } else {
            order.push(i);
        }
    }
    let roots = order.clone();
    let mut q = 0usize;
    while q < order.len() {
        let cur = order[q];
        q += 1;
        for &child in &children[cur] {
            order.push(child);
        }
    }

    let mut clusters = String::new();
    let mut fills: Vec<Fill> = Vec::new();
    let mut dividers = String::new();
    let mut nodes_out = String::new();
    let mut notes_out = String::new();

    // Draws the dashed lines between the regions among a composite's members.
    let mut divide = |items: &[usize], left: f64, top: f64, right: f64, bottom: f64| {
        let regions: Vec<usize> = items.iter().copied().filter(|&i| views[i].kind == K::Region).collect();
        if regions.len() < 2 {
            return;
        }
        let lo = |i: usize, x_axis: bool| -> f64 {
            if x_axis {
                cnodes[i].x - cnodes[i].w / 2.0
            } else {
                cnodes[i].y - cnodes[i].h / 2.0
            }
        };
        let hi = |i: usize, x_axis: bool| -> f64 {
            if x_axis {
                cnodes[i].x + cnodes[i].w / 2.0
            } else {
                cnodes[i].y + cnodes[i].h / 2.0
            }
        };
        for x_axis in [true, false] {
            let mut sorted = regions.clone();
            sorted.sort_by(|&a, &b| {
                lo(a, x_axis)
                    .partial_cmp(&lo(b, x_axis))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(&b))
            });
            let mut apart = true;
            for k in 1..sorted.len() {
                if hi(sorted[k - 1], x_axis) > lo(sorted[k], x_axis) + 0.5 {
                    apart = false;
                }
            }
            if !apart {
                continue;
            }
            for k in 1..sorted.len() {
                let at = num((hi(sorted[k - 1], x_axis) + lo(sorted[k], x_axis)) / 2.0 + pad);
                if x_axis {
                    dividers.push_str(&format!(
                        "<path class=\"pele-divider\" d=\"M{},{}V{}\"/>",
                        at,
                        num(top),
                        num(bottom)
                    ));
                } else {
                    dividers.push_str(&format!(
                        "<path class=\"pele-divider\" d=\"M{},{}H{}\"/>",
                        num(left),
                        at,
                        num(right)
                    ));
                }
            }
            return;
        }
        // Regions that sit neither side by side nor in a stack each get an outline.
        for i in regions {
            let c = &cnodes[i];
            dividers.push_str(&rect(
                c.x - c.w / 2.0 + pad,
                c.y - c.h / 2.0 + pad,
                c.w,
                c.h,
                &format!(" class=\"pele-divider\" rx=\"{}\"", js_number(CORNER)),
            ));
        }
    };

    divide(&roots, pad / 2.0, pad / 2.0, width - pad / 2.0, height - pad / 2.0);

    for &i in &order {
        let view = &views[i];
        let (Some(node), Some(ref st)) = (view.node, &view.style) else {
            continue;
        };
        if view.kind >= K::Region {
            continue;
        }
        let c = &cnodes[i];
        let x = c.x + pad;
        let y = c.y + pad;
        let w = view.w;
        let h = view.h;
        let ink = ink_fill(&st.shape, &st.text, None);
        let label_attrs = format!(" class=\"pele-label\"{}{}", ink, st.text);
        let mut body: String;

        match view.kind {
            K::Composite => {
                let left = x - c.w / 2.0;
                let top = y - c.h / 2.0;
                if paints(&st.shape) {
                    fills.push(Fill {
                        bounds: [left, top, left + c.w, top + c.h],
                        shape: st.shape.clone(),
                    });
                }
                body = rect(
                    left,
                    top,
                    c.w,
                    c.h,
                    &format!(
                        " rx=\"{}\" fill=\"var(--_a)\"{} stroke=\"var(--_b)\"{}",
                        js_number(CORNER),
                        if sets_fill(&st.shape) { "" } else { " fill-opacity=\"0.5\"" },
                        st.shape
                    ),
                );
                if view.title_h > 0.0 {
                    body.push_str(&format!(
                        "<path d=\"M{},{}H{}\"{}{}/>{}",
                        num(left),
                        num(top + view.title_h),
                        num(left + c.w),
                        RULE,
                        st.line,
                        label_svg(
                            &view.label,
                            title_x(c, view.label.width, &crossings) + pad,
                            top + view.title_h / 2.0,
                            &format!(" class=\"pele-cluster-label\"{}{}", ink, st.text),
                        )
                    ));
                }
                divide(&children[i], left, top + view.title_h, left + c.w, top + c.h);
            }
            K::Start => {
                body = format!("<circle r=\"7\" fill=\"var(--_l)\"{}/>", st.shape);
            }
            K::End => {
                body = format!(
                    "<circle r=\"8.5\" fill=\"var(--_bg)\" stroke=\"var(--_l)\"{}/><circle r=\"5\" fill=\"var(--_l)\"{}/>",
                    st.line, st.shape
                );
            }
            K::Choice => {
                body = format!(
                    "<polygon points=\"0,{} {},0 0,{} {},0\"{}{}/>",
                    num(-h / 2.0),
                    num(w / 2.0),
                    num(h / 2.0),
                    num(-w / 2.0),
                    SURFACE,
                    st.shape
                );
            }
            K::Fork | K::Join => {
                body = rect(
                    -w / 2.0,
                    -h / 2.0,
                    w,
                    h,
                    &format!(" rx=\"2\" fill=\"var(--_l)\"{}", st.shape),
                );
            }
            K::Note => {
                let r = num(w / 2.0);
                let b = num(h / 2.0);
                let fold = num(w / 2.0 - FOLD);
                let corner = num(FOLD - h / 2.0);
                body = format!(
                    "<path d=\"M-{r},-{b}H{fold}L{r},{corner}V{b}H-{r}Z\" fill=\"var(--_bg)\" stroke=\"var(--_b)\"/><path d=\"M{fold},-{b}V{corner}H{r}\"{RULE}/>{}",
                    label_svg(
                        &view.label,
                        0.0,
                        0.0,
                        &format!(" class=\"pele-label\" font-size=\"{}\" fill=\"var(--_m)\"", js_number(small)),
                    )
                );
            }
            K::State | K::Described | K::Region | K::Hidden => {
                body = rect(
                    -w / 2.0,
                    -h / 2.0,
                    w,
                    h,
                    &format!(" rx=\"{}\"{}{}", num(CORNER.min(h / 2.0)), SURFACE, st.shape),
                );
                if view.kind == K::State {
                    body.push_str(&label_svg(&view.label, 0.0, 0.0, &label_attrs));
                } else {
                    let mut at = PAD_Y - h / 2.0 + view.label.height;
                    body.push_str(&format!(
                        "<path d=\"M{},{}H{}\"{}{}/>{}",
                        num(-w / 2.0),
                        num(at + 6.0),
                        num(w / 2.0),
                        RULE,
                        st.line,
                        label_svg(&view.label, 0.0, at - view.label.height / 2.0, &label_attrs)
                    ));
                    at += 14.0;
                    for line in &view.lines {
                        body.push_str(&label_svg(
                            line,
                            0.0,
                            at + line.height / 2.0,
                            &format!(
                                " class=\"pele-state-description\" font-size=\"{}\"{}{}",
                                js_number(small),
                                ink,
                                st.text
                            ),
                        ));
                        at += line.height;
                    }
                }
            }
        }

        if let Some(link) = db.links.get(&node.id) {
            // Entity codes are resolved first, so the URL is checked in the form a browser would follow.
            let tooltip = decode_entities(unquote(&link.tooltip));
            if !tooltip.is_empty() {
                body = format!("<title>{}</title>{}", esc(&tooltip), body);
            }
            body = linked(
                body,
                &safe_url(unquote(&link.url)),
                &node.id,
                &mut links,
                options,
                " target=\"_blank\"",
            );
        }

        let extra_classes = class_names(&strip_mermaid_classes(&node.css_classes));
        let open = format!(
            "<g class=\"{}{}\" data-id=\"{}\"",
            CLASSES[view.kind as usize],
            extra_classes,
            esc_text(&node.id)
        );
        if view.kind == K::Composite {
            clusters.push_str(&format!("{}>{}</g>", open, body));
        } else if view.kind == K::Note {
            notes_out.push_str(&format!(
                "{} transform=\"translate({},{})\">{}</g>",
                open,
                num(x),
                num(y),
                body
            ));
        } else {
            nodes_out.push_str(&format!(
                "{} transform=\"translate({},{})\">{}</g>",
                open,
                num(x),
                num(y),
                body
            ));
        }
    }

    // Moves a route end from the layout box onto the drawn shape, and spreads the ends along a bar.
    let attach = |route: &mut [f64], at: usize, i: usize| {
        let c = &cnodes[i];
        if c.is_group {
            return;
        }
        let view = &views[i];
        let bar = view.kind == K::Fork || view.kind == K::Join;
        let other = if at == 0 { 3 } else { at - 3 };
        // 0 is the x axis and 1 the y axis; the end sits on a side that faces along `axis`.
        for axis in [1usize, 0usize] {
            let extent = if axis != 0 { c.h } else { c.w };
            let drawn_extent = if axis != 0 { view.h } else { view.w };
            let breadth = if axis != 0 { view.w } else { view.h };
            let center = (if axis != 0 { c.y } else { c.x }) + pad;
            let cross = (if axis != 0 { c.x } else { c.y }) + pad;
            let delta = route[at + axis] - center;
            if (delta.abs() - extent / 2.0).abs() >= 0.5 {
                continue;
            }
            route[at + axis] -= (sign(delta) * (extent - drawn_extent)) / 2.0;
            if bar && breadth > drawn_extent {
                route[at + 1 - axis] = clamp(
                    route[other + 1 - axis],
                    cross - breadth / 2.0 + 6.0,
                    cross + breadth / 2.0 - 6.0,
                );
            }
            return;
        }
    };

    let mut reach = SideReach::default();
    for i in 0..cedges.len() {
        let ce = &cedges[i];
        if ce.src == ce.dst || ce.route.len() < 6 || drawn[i].pattern == Some("dashed") {
            continue;
        }
        reach.add(
            ce.dst,
            end_side(&ce.route, ce.route.len() - 3, &box_at(&cnodes[ce.dst])),
            "arrow_point",
        );
    }

    let mut casings = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();
    let mut loop_seen: Vec<i32> = vec![0; states];
    let mut loop_text: Vec<f64> = vec![0.0; states];

    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let note = edge.pattern == Some("dashed");
        let end_type = if note { "none" } else { "arrow_point" };
        let path: EdgePath;

        if cedges[i].src == cedges[i].dst {
            let src = cedges[i].src;
            let a = &cnodes[src];
            let view = &views[src];
            let k = loop_seen[src] as f64;
            loop_seen[src] += 1;
            // The loop hangs off a side the flow does not use: the right, or below when the flow is horizontal.
            let side = across(&views, &logical, src);
            let w = if a.is_group { a.w } else { view.w };
            let h = if a.is_group { a.h } else { view.h };
            let out = (if side { w } else { h }) / 2.0;
            let spread = (((if side { h } else { w }) / 2.0 - 4.0).min(8.0 + k * 6.0)).max(2.0);
            let loop_reach = LOOP + k * 8.0;
            let hyp = loop_reach.hypot(spread);
            let trim = marker_trim(end_type) / if hyp == 0.0 { 1.0 } else { hyp };
            // Offsets are given for a loop on the right and turned a quarter for one below.
            let px = |dx: f64, dy: f64| -> f64 { a.x + pad + if side { out + dx } else { -dy } };
            let py = |dx: f64, dy: f64| -> f64 { a.y + pad + if side { dy } else { out + dx } };
            let at = |dx: f64, dy: f64| -> String { format!("{},{}", num(px(dx, dy)), num(py(dx, dy))) };

            let raw_edx = px(-loop_reach, -spread) - px(0.0, 0.0);
            let raw_edy = py(-loop_reach, -spread) - py(0.0, 0.0);
            let len = raw_edx.hypot(raw_edy);
            let len = if len == 0.0 { 1.0 } else { len };
            path = EdgePath {
                d: format!(
                    "M{}C{} {} {}",
                    at(0.0, -spread),
                    at(loop_reach, -2.0 * spread),
                    at(loop_reach, 2.0 * spread),
                    at(loop_reach * trim, spread + spread * trim)
                ),
                tokens: None,
                sx: 0.0,
                sy: 0.0,
                sdx: 0.0,
                sdy: 0.0,
                ex: px(0.0, spread),
                ey: py(0.0, spread),
                edx: raw_edx / len,
                edy: raw_edy / len,
            };
            // Labels stack beyond the outermost loop, in the order of their transitions.
            let furthest = LOOP + (loop_count[src] - 1) as f64 * 8.0;
            let below = loop_text[src] + label.height / 2.0;
            loop_text[src] += label.height;
            cedges[i].label_x = if side {
                a.x + out + furthest + 4.0 + cedges[i].label_w / 2.0
            } else {
                a.x
            };
            cedges[i].label_y = if side {
                a.y - loop_height[src] / 2.0 + below
            } else {
                a.y + out + furthest * 0.75 + 4.0 + below
            };
        } else {
            let src = cedges[i].src;
            let dst = cedges[i].dst;
            let mut route = cedges[i].route.clone();
            let leaves = reach.of(src, end_side(&route, 0, &box_at(&cnodes[src])));
            let arrives = reach.of(
                dst,
                end_side(&route, route.len().saturating_sub(3), &box_at(&cnodes[dst])),
            );
            let mut k = 0;
            while k < route.len() {
                route[k] += pad;
                route[k + 1] += pad;
                k += 3;
            }
            if route.len() >= 3 {
                attach(&mut route, 0, src);
                let last_at = route.len() - 3;
                attach(&mut route, last_at, dst);
            }
            if let Some(&composite) = anchors.get(&i) {
                if route.len() >= 6 {
                    let g = &cnodes[composite];
                    let inward = views[src].kind == K::Hidden;
                    let at = if inward { 0 } else { route.len() - 3 };
                    let near = matches!(g.dir, Some(Dir::TB | Dir::LR)) == inward;
                    let point = if vertical(g.dir) {
                        [
                            route[at],
                            g.y + pad + if near { views[composite].title_h - g.h / 2.0 } else { g.h / 2.0 },
                            0.0,
                        ]
                    } else {
                        [
                            g.x + pad + if near { -g.w / 2.0 } else { g.w / 2.0 },
                            route[at + 1],
                            1.0,
                        ]
                    };
                    if inward {
                        let mut next = Vec::with_capacity(route.len() + 3);
                        next.extend_from_slice(&point);
                        next.extend_from_slice(&route);
                        route = next;
                    } else {
                        route.extend_from_slice(&point);
                    }
                }
            }
            path = route_path(&route, curve, 0.0, marker_trim(end_type), leaves, arrives);
        }

        let over = over_fills(&path.d, &[path.ex, path.ey], &fills, "", path.tokens.as_deref());
        if over == Some("") {
            casings.push_str(&edge_casing(&path, "none", end_type, "", false));
        }
        let over_ink = over.filter(|s| !s.is_empty());
        let kind_attr = if note {
            format!(
                "pele-note-edge\" stroke=\"{}\" stroke-dasharray=\"3 4",
                over_ink.unwrap_or("var(--_b)")
            )
        } else if let Some(ink_col) = over_ink {
            format!("pele-transition\" stroke=\"{}", ink_col)
        } else {
            "pele-transition".to_string()
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge {}\" data-id=\"{}\"><path d=\"{}\"/>{}</g>",
            kind_attr,
            esc_text(&edge.id),
            path.d,
            marker(
                end_type,
                path.ex,
                path.ey,
                path.edx,
                path.edy,
                over_ink.unwrap_or("var(--_l)"),
                "none"
            )
        ));
        if label.width > 0.0 {
            let x = cedges[i].label_x + pad;
            let y = cedges[i].label_y + pad;
            labels_out.push_str(&edge_label_svg(&edge.id, label, x, y, ""));
        }
    }

    let mut content = String::new();
    if !clusters.is_empty() {
        content.push_str(&format!("<g class=\"pele-clusters\">{}</g>", clusters));
    }
    if !dividers.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-dividers\" stroke-dasharray=\"4 4\"{}>{}</g>",
            RULE, dividers
        ));
    }
    content.push_str(&casings_svg(&casings));
    if !edges_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{}</g>",
            edges_out
        ));
    }
    if !labels_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>",
            js_number(small),
            labels_out
        ));
    }
    content.push_str(&format!("<g class=\"pele-nodes\">{}{}</g>", nodes_out, notes_out));

    let titled = with_title(db.title.as_deref(), measurer, size, pad, width, height, content);
    let fixed = config.get("state").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "state",
        titled.width,
        titled.height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive: options.responsive.unwrap_or(!fixed),
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: "",
        },
        &Accessible {
            acc_title: db.acc_title.as_deref(),
            acc_descr: db.acc_descr.as_deref(),
        },
        &titled.content,
    );

    Rendered {
        svg,
        width: titled.width,
        height: titled.height,
        links,
    }
}

// Picks where a composite's title sits in its title bar so that edges entering from above do not cross it.
fn title_x(c: &CNode, width: f64, crossings: &HashMap<i64, Vec<f64>>) -> f64 {
    let Some(xs) = crossings.get(&(js_round(c.y - c.h / 2.0) as i64)) else {
        return c.x;
    };
    if width == 0.0 {
        return c.x;
    }
    let left = c.x - c.w / 2.0;
    let half = width / 2.0;
    let mut best = c.x;
    let mut best_gap = -1.0;
    for x in [c.x, left + GROUP_PAD + half, left + c.w - GROUP_PAD - half] {
        let mut gap = f64::INFINITY;
        for &cx in xs {
            if cx > left && cx < left + c.w {
                gap = gap.min((cx - x).abs() - half);
            }
        }
        if gap >= 6.0 {
            return x;
        }
        if gap > best_gap + 0.5 {
            best_gap = gap;
            best = x;
        }
    }
    best
}
