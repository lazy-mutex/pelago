use super::db::ClassDb;
use super::graph::{build_class_graph, EdgePattern, GraphEdge, GraphNode, NodeShape};
use super::markers::{class_marker, class_marker_trim};
use super::members::ClassMember;
use super::types::ClassNode;
use crate::chars::is_space;
use crate::diagrams::common::generics::parse_generic_types;
use crate::layout::components::wrap_components;
use crate::layout::compound::{cnode, compound_layout, direction, CEdge, CNode, Dir};
use crate::layout::layered::LayeredOptions;
use crate::options::{runs_across, tighten, turn_to_fit, LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::cluster::{cluster_title_x, mark_crossings, struck_title, title_crossed, Crossings};
use crate::svg::edges::{edge_label_svg, route_path_default, EdgePath};
use crate::svg::ink::ink_fill;
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::shapes::{end_side, BoxAt};
use crate::svg::theme::{class_names, resolve_style, rounded, ResolvedStyle, FONT_MONO, ROUNDED};
use crate::svg::title::with_title;
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::url::linked;
use crate::value::Value;
use std::collections::HashMap;

const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 56.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const PAD_X: f64 = 12.0;
const TITLE_PAD: f64 = 8.0;
const SECTION_PAD: f64 = 6.0;
const EMPTY_SECTION: f64 = 10.0;
const MIN_WIDTH: f64 = 64.0;
const FOLD: f64 = 8.0;
const LINE: &str = "var(--_l)";

fn is_valid_target(s: &str) -> bool {
    matches!(s, "_self" | "_blank" | "_parent" | "_top")
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn strip_default_class(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix("default") {
        if let Some(first) = rest.chars().next() {
            if is_space(first) {
                return &rest[first.len_utf8()..];
            }
        }
        return rest;
    }
    s
}

fn number_option(conf: &Value, key: &str, fallback: f64) -> f64 {
    match conf.get(key).as_f64() {
        Some(v) if v > 0.0 => v.min(2000.0),
        _ => fallback,
    }
}

fn box_at(c: &CNode) -> BoxAt {
    BoxAt { x: c.x, y: c.y, w: c.w, h: c.h, is_group: c.is_group }
}

#[derive(Clone)]
struct Line {
    text: String,
    width: f64,
    classifier: String,
}

struct View<'a> {
    node: &'a GraphNode,
    style: ResolvedStyle,
    label: Label,
    w: f64,
    h: f64,
    loops: usize,
    annotations: Vec<String>,
    attributes: Vec<Line>,
    methods: Vec<Line>,
    title_h: f64,
    attributes_h: f64,
    font_size: f64,
}

#[derive(Clone, Copy)]
struct End {
    edge_idx: usize,
    at: usize,
    toward: f64,
    order: usize,
    room: f64,
    side: usize,
    // Whether the text of this end goes on the right of, or below, its line.
    after: bool,
}

struct Card {
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    parent: i32,
}

struct KeptMap {
    entries: Vec<(String, String)>,
}

impl KeptMap {
    fn new() -> Self {
        Self { entries: Vec::new() }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn set(&mut self, prop: String, decl: String) {
        if let Some(existing) = self.entries.iter_mut().find(|(k, _)| *k == prop) {
            existing.1 = decl;
        } else {
            self.entries.push((prop, decl));
        }
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn member_line(member: &ClassMember, measurer: &dyn TextMeasurer, size: f64, mono: bool) -> Line {
    let details = member.get_display_details();
    let decoded = decode_entities(&details.display_text);
    let text = js_trim(&decoded).to_string();
    let st = (if member.classifier == "*" { style::ITALIC } else { 0 })
        | (if mono { style::MONO } else { 0 });
    let width = measurer.width(&text, size, st);
    Line {
        text,
        width,
        classifier: member.classifier.clone(),
    }
}

fn base_view<'a>(
    node: &'a GraphNode,
    style: ResolvedStyle,
    label: Label,
    w: f64,
    h: f64,
    font_size: f64,
) -> View<'a> {
    View {
        node,
        style,
        label,
        w,
        h,
        loops: 0,
        annotations: Vec::new(),
        attributes: Vec::new(),
        methods: Vec::new(),
        title_h: 0.0,
        attributes_h: 0.0,
        font_size,
    }
}

pub fn render_class(db: &ClassDb, config: &Value, options: &RenderOptions) -> Rendered {
    let fitted = |down: bool| tighten(options, &|tight| draw(db, config, options, down, tight));
    if runs_across(Some(&db.direction)) {
        turn_to_fit(options, &fitted)
    } else {
        fitted(false)
    }
}

// `turned` draws a diagram that runs across as one that runs down. `tight` gives up the room
// that keeps a namespace's name clear of an edge coming in beside it.
fn draw(db: &ClassDb, config: &Value, options: &RenderOptions, turned: bool, tight: bool) -> Rendered {
    let conf = config.get("class");
    let hierarchical = conf.get("hierarchicalNamespaces").as_bool() != Some(false);
    let graph = build_class_graph(db, hierarchical);
    let hide_empty = conf.get("hideEmptyMembersBox").as_bool() == Some(true);
    let monospaced = false;
    let is_mono = |st: &ResolvedStyle| -> bool {
        monospaced && !st.text.contains("\"font-family:") && !st.text.contains(";font-family:")
    };
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let tiny = js_round(size * 0.8);
    let tiny_height = js_round(tiny * 1.4);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let card_width = |raw: &str| -> f64 {
        if raw.is_empty() {
            0.0
        } else {
            measurer.width(&decode_entities(raw), tiny, 0)
        }
    };
    let root_dir = if turned { Dir::TB } else { direction(Some(&db.direction)) };

    // A class may carry the declarations of many classDefs many times over. Each distinct declaration
    // is checked once, and only the last value of each property that has an effect is resolved.
    let mut effect: HashMap<String, String> = HashMap::new();
    let mut from_defs: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut kept = KeptMap::new();

    let keep_decl = |decl: &str, effect: &mut HashMap<String, String>, kept: &mut KeptMap| {
        let prop = if let Some(cached) = effect.get(decl) {
            cached.clone()
        } else {
            let one = resolve_style(&[decl]);
            let computed = if one.shape.is_empty() && one.text.is_empty() {
                String::new()
            } else if let Some(colon) = decl.find(':') {
                js_trim(&decl[..colon]).to_lowercase()
            } else {
                String::new()
            };
            effect.insert(decl.to_string(), computed.clone());
            computed
        };
        if !prop.is_empty() {
            kept.set(prop, decl.to_string());
        }
    };

    let mut class_style = |cls: &ClassNode| -> ResolvedStyle {
        kept.clear();
        if let Some(base) = from_defs.get(&cls.css_classes) {
            for (prop, decl) in base {
                kept.set(prop.clone(), decl.clone());
            }
        } else {
            for name in cls.css_classes.split(' ') {
                if let Some(def) = db.style_classes.get(name) {
                    for s in &def.styles {
                        for part in s.split(',') {
                            keep_decl(part, &mut effect, &mut kept);
                        }
                    }
                }
            }
            from_defs.insert(cls.css_classes.clone(), kept.entries.clone());
        }
        for s in &cls.styles {
            keep_decl(s, &mut effect, &mut kept);
        }
        if kept.is_empty() {
            let empty: [&str; 0] = [];
            resolve_style(&empty)
        } else {
            let vals: Vec<&str> = kept.entries.iter().map(|(_, v)| v.as_str()).collect();
            resolve_style(&vals)
        }
    };

    let mut children: Vec<i32> = vec![0; graph.nodes.len()];
    for node in &graph.nodes {
        if node.parent >= 0 {
            children[node.parent as usize] += 1;
        }
    }

    // Namespaces that an edge may cross into are made wide enough for the title to sit beside it.
    let mut crossed: Vec<bool> = vec![false; graph.nodes.len()];
    for edge in &graph.edges {
        if edge.source < 0 || edge.target < 0 {
            continue;
        }
        let a = graph.nodes[edge.source as usize].parent;
        let b = graph.nodes[edge.target as usize].parent;
        if a == b {
            continue;
        }
        let mut g = a;
        while g >= 0 && !crossed[g as usize] {
            crossed[g as usize] = true;
            g = graph.nodes[g as usize].parent;
        }
        let mut g = b;
        while g >= 0 && !crossed[g as usize] {
            crossed[g as usize] = true;
            g = graph.nodes[g as usize].parent;
        }
    }

    let empty_style_slice: [&str; 0] = [];
    let default_style = resolve_style(&empty_style_slice);

    let mut views: Vec<View<'_>> = Vec::with_capacity(graph.nodes.len());
    let mut cnodes: Vec<CNode> = Vec::with_capacity(graph.nodes.len());

    for (i, node) in graph.nodes.iter().enumerate() {
        let mut view: View<'_>;
        let mut c: CNode;
        if node.is_group {
            let label = layout_label(Some(&node.label), false, measurer, size, 4000.0, 0);
            view = base_view(node, default_style.clone(), label, 0.0, 0.0, size);
            if children[i] > 0 {
                c = cnode(0.0, 0.0, -1);
                c.is_group = true;
                c.pad_x = GROUP_PAD;
                c.pad_top = if view.label.height > 0.0 {
                    view.label.height + 16.0
                } else {
                    GROUP_PAD
                };
                c.pad_bottom = GROUP_PAD;
                c.min_w = view.label.width
                    + 2.0 * GROUP_PAD
                    - 12.0
                    + if crossed[i] && !tight {
                        (view.label.width + 24.0).min(240.0)
                    } else {
                        0.0
                    };
            } else {
                view.w = (view.label.width + 2.0 * GROUP_PAD).max(80.0);
                view.h = view.label.height + 2.0 * GROUP_PAD;
                c = cnode(view.w, view.h, -1);
            }
        } else if let Some(ref cls) = node.class_node {
            let st = class_style(cls);
            let font_size = st.font_size.unwrap_or(size);
            let member_size = js_round(font_size * 0.875);
            let line_height = js_round(member_size * 1.5);
            // The generic type is escaped so that `<T>` is not read as a tag.
            let generic = if !cls.generic_type.is_empty() {
                format!(
                    "&lt;{}&gt;",
                    parse_generic_types(&cls.generic_type)
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                )
            } else {
                String::new()
            };
            let full_label = format!("{}{}", cls.label, generic);
            let label = layout_label(
                Some(&full_label),
                false,
                measurer,
                font_size,
                4000.0,
                style::BOLD | if st.italic { style::ITALIC } else { 0 },
            );
            let annotations: Vec<String> = cls
                .annotations
                .iter()
                .map(|a| format!("«{}»", js_trim(&decode_entities(a))))
                .collect();
            let mono_flag = is_mono(&st);
            let attributes: Vec<Line> = cls
                .members
                .iter()
                .map(|m| member_line(m, measurer, member_size, mono_flag))
                .collect();
            let methods: Vec<Line> = cls
                .methods
                .iter()
                .map(|m| member_line(m, measurer, member_size, mono_flag))
                .collect();
            let mut inner = label.width;
            let mut annotations_h = 0.0;
            for a in &annotations {
                inner = inner.max(measurer.width(a, tiny, 0));
                annotations_h += tiny_height;
            }
            for line in &attributes {
                inner = inner.max(line.width);
            }
            for line in &methods {
                inner = inner.max(line.width);
            }
            let sections = !(hide_empty && attributes.is_empty() && methods.is_empty());
            let section_h = |count: usize| -> f64 {
                if !sections {
                    0.0
                } else if count > 0 {
                    2.0 * SECTION_PAD + count as f64 * line_height
                } else {
                    EMPTY_SECTION
                }
            };
            let title_h =
                2.0 * TITLE_PAD + annotations_h + label.height.max(js_round(font_size * 1.5));
            let attributes_h = section_h(attributes.len());
            let w = (inner + 2.0 * PAD_X).max(MIN_WIDTH);
            let h = title_h + attributes_h + section_h(methods.len());
            view = View {
                node,
                style: st,
                label,
                w,
                h,
                loops: 0,
                annotations,
                attributes,
                methods,
                title_h,
                attributes_h,
                font_size,
            };
            c = cnode(view.w, view.h, -1);
        } else if node.shape == NodeShape::Note {
            let label = layout_label(Some(&node.label), true, measurer, small, 260.0, 0);
            let w = (label.width + 2.0 * PAD_X).max(40.0);
            let h = label.height + 2.0 * TITLE_PAD;
            view = base_view(node, default_style.clone(), label, w, h, small);
            c = cnode(view.w, view.h, -1);
        } else {
            let label = layout_label(Some(&node.label), false, measurer, small, 200.0, 0);
            let w = label.width.max(12.0) + 4.0;
            let h = label.height.max(12.0);
            view = base_view(node, default_style.clone(), label, w, h, small);
            c = cnode(view.w, view.h, -1);
        }
        c.parent = node.parent;
        c.seq = i as f64;
        views.push(view);
        cnodes.push(c);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut drawn: Vec<&GraphEdge> = Vec::new();
    let mut edge_labels: Vec<Label> = Vec::new();
    let mut start_room: Vec<f64> = Vec::new();
    let mut end_room: Vec<f64> = Vec::new();
    // Edges leave one side of a class and arrive at the other, so a class with many of them
    // is widened until they, and the text at their ends, fit side by side.
    let mut crowd: Vec<f64> = vec![0.0; views.len() * 4];
    let mut meet = |item: usize, slot: usize, room: f64| {
        crowd[item * 4 + slot] += 1.0;
        if room > crowd[item * 4 + slot + 1] {
            crowd[item * 4 + slot + 1] = room;
        }
    };

    for edge in &graph.edges {
        if edge.source < 0 || edge.target < 0 {
            continue;
        }
        let src = edge.source as usize;
        let dst = edge.target as usize;
        let label = layout_label(edge.label.as_deref(), true, measurer, small, 200.0, 0);
        let start_w = card_width(&edge.start_label_right);
        let end_w = card_width(&edge.end_label_left);
        if src == dst {
            views[src].loops += 1;
            cnodes[src].w += 2.0 * (LOOP + if label.width > 0.0 { label.width + 12.0 } else { 0.0 });
        } else {
            meet(src, 0, start_w);
            meet(dst, 2, end_w);
        }
        start_room.push(start_w);
        end_room.push(end_w);
        drawn.push(edge);
        let label_w = if label.width > 0.0 { label.width + 12.0 } else { 0.0 };
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

    if matches!(root_dir, Dir::TB | Dir::BT) {
        for i in 0..views.len() {
            if views[i].node.class_node.is_none() {
                continue;
            }
            let mut need = 0.0f64;
            for slot in [0usize, 2usize] {
                let count = crowd[i * 4 + slot];
                let room = crowd[i * 4 + slot + 1];
                if count > 1.0 {
                    need = need.max(
                        (count - 1.0) * (if room > 0.0 { room + 24.0 } else { 14.0 }) + 24.0,
                    );
                }
            }
            need = need.min(480.0);
            if need > views[i].w {
                cnodes[i].w += need - views[i].w;
                views[i].w = need;
            }
        }
    }

    let lettered = (0..drawn.len()).any(|i| start_room[i] > 0.0 || end_room[i] > 0.0);
    // Cardinalities at both ends of a vertical edge need room for their height between the ranks.
    let mut tall_cards = 0.0f64;
    if matches!(root_dir, Dir::TB | Dir::BT) {
        let card_height = |raw: &str| -> f64 { if raw.is_empty() { 0.0 } else { tiny_height } };
        for edge in &drawn {
            tall_cards = tall_cards.max(
                card_height(&edge.start_label_right) + card_height(&edge.end_label_left)
                    - 2.0 * tiny_height,
            );
        }
    }

    let layout = compound_layout(
        &mut cnodes,
        &mut cedges,
        root_dir,
        &LayeredOptions {
            node_sep: number_option(conf, "nodeSpacing", NODE_SEP),
            edge_sep: EDGE_SEP,
            rank_sep: number_option(
                conf,
                "rankSpacing",
                if lettered { RANK_SEP + 16.0 } else { RANK_SEP },
            ) + tall_cards,
            port_sep: 20.0,
            apart: true,
            ..Default::default()
        },
    );

    // Every edge reaches the middle of a side. Where several share a side they are spread along it,
    // in the order of where they come from. An end with text runs straight for the length of the text,
    // which sits beside it on the side the line then bends toward, away from the neighbors that bend
    // the other way. Neighbors are set apart far enough for the text between them.
    let mut sides_order: Vec<usize> = Vec::new();
    let mut sides: HashMap<usize, Vec<End>> = HashMap::new();
    let mut cards: Vec<Card> = Vec::new();

    let mut attach_end = |cedges: &mut [CEdge],
                          edge_idx: usize,
                          at: usize,
                          item: usize,
                          room: f64,
                          order: usize|
     -> Option<End> {
        let c = &cnodes[item];
        let side_i32 = end_side(&cedges[edge_idx].route, at, &box_at(c));
        if side_i32 < 0 {
            return None;
        }
        let side = side_i32 as usize;
        let slack = (c.w - views[item].w) / 2.0;
        let route = &mut cedges[edge_idx].route;
        if side == 1 {
            route[at] -= slack;
        } else if side == 3 {
            route[at] += slack;
        }
        let across = side & 1;
        let toward = route[(if at == 0 { 3 } else { at - 3 }) + across];
        let bend = toward - route[at + across];
        let end = End {
            edge_idx,
            at,
            toward,
            order,
            room,
            side,
            after: if across != 0 { bend > 0.0 } else { bend >= 0.0 },
        };
        let key = item * 4 + side;
        if let Some(list) = sides.get_mut(&key) {
            list.push(end);
        } else {
            sides_order.push(key);
            sides.insert(key, vec![end]);
        }
        Some(end)
    };

    let mut starts: Vec<Option<End>> = Vec::with_capacity(cedges.len());
    let mut ends: Vec<Option<End>> = Vec::with_capacity(cedges.len());
    for i in 0..cedges.len() {
        let routed = cedges[i].src != cedges[i].dst && cedges[i].route.len() >= 6;
        if routed {
            let src = cedges[i].src;
            let dst = cedges[i].dst;
            let last_at = cedges[i].route.len() - 3;
            let s = attach_end(&mut cedges, i, 0, src, start_room[i], i);
            let e = attach_end(&mut cedges, i, last_at, dst, end_room[i], i);
            starts.push(s);
            ends.push(e);
        } else {
            starts.push(None);
            ends.push(None);
        }
    }

    // Whether a point lies on the border of one of the namespaces around a class.
    let on_border = |cnodes: &[CNode], item: usize, x: f64, y: f64| -> bool {
        let mut g = cnodes[item].parent;
        let mut hops = 0usize;
        while g >= 0 && hops < 10000 {
            let c = &cnodes[g as usize];
            let dx = (x - c.x).abs();
            let dy = (y - c.y).abs();
            if ((dy - c.h / 2.0).abs() < 0.5 && dx <= c.w / 2.0 + 0.5)
                || ((dx - c.w / 2.0).abs() < 0.5 && dy <= c.h / 2.0 + 0.5)
            {
                return true;
            }
            g = c.parent;
            hops += 1;
        }
        false
    };

    let mut gaps: Vec<f64> = Vec::new();
    for key in sides_order {
        let list = sides.get_mut(&key).unwrap();
        if list.len() < 2 {
            continue;
        }
        let item = key >> 2;
        let across = key & 1;
        let length = if across != 0 { views[item].h } else { views[item].w };
        list.sort_by(|a, b| {
            a.toward
                .partial_cmp(&b.toward)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.order.cmp(&b.order))
        });
        gaps.resize(list.len(), 0.0);
        let mut total = 0.0;
        for j in 0..list.len() - 1 {
            let a = &list[j];
            let b = &list[j + 1];
            let mut gap = if across != 0 { 20.0 } else { 24.0 };
            if a.room > 0.0 && a.after {
                gap += if across != 0 { tiny_height + 6.0 } else { a.room };
            }
            if b.room > 0.0 && !b.after {
                gap += if across != 0 { tiny_height + 6.0 } else { b.room };
            }
            gaps[j] = gap;
            total += gap;
        }
        let scale = (1.0f64).min((length - 24.0) / total);
        if scale <= 0.0 {
            continue;
        }
        let mut at_pos = (if across != 0 { cnodes[item].y } else { cnodes[item].x })
            - (total * scale) / 2.0;
        for j in 0..list.len() {
            let edge_idx = list[j].edge_idx;
            let end_idx = list[j].at;
            let route = &mut cedges[edge_idx].route;
            // An edge that crosses the border of the class's namespace close by runs straight to it:
            // the point where it crosses moves into line with its end.
            let step: isize = if end_idx == 0 { 3 } else { -3 };
            let first: isize = end_idx as isize + step;
            let mut border: isize = -1;
            let mut k = first;
            while k >= 0 && (k as usize) < route.len() {
                let ku = k as usize;
                let fu = first as usize;
                if (route[ku + across] - route[fu + across]).abs() >= 0.5 {
                    break;
                }
                if on_border(&cnodes, item, route[ku], route[ku + 1]) {
                    border = k;
                    break;
                }
                k += step;
            }
            if border >= 0 && (route[first as usize + across] - at_pos).abs() <= 24.0 {
                let mut k = first;
                while k != border + step {
                    route[k as usize + across] = at_pos;
                    k += step;
                }
            }
            route[end_idx + across] = at_pos;
            at_pos += gaps[j] * scale;
        }
    }

    let straighten = |route: &mut Vec<f64>, end_opt: Option<End>, room: f64, first: bool| {
        let Some(end) = end_opt else {
            return;
        };
        if room <= 0.0 {
            return;
        }
        let across = end.side & 1;
        let at = if first { 0 } else { route.len() - 3 };
        let along = at + 1 - across;
        let neighbor = if first { 3 } else { at - 3 };
        let reach = (route[neighbor + 1 - across] - route[along]).abs();
        let length = (if across != 0 { room + 10.0 } else { tiny_height + 6.0 }).min(reach * 0.35);
        if length < 4.0 {
            return;
        }
        let mut point = [route[at], route[at + 1], route[at + 2]];
        point[1 - across] += (if end.side == 0 || end.side == 3 { -1.0 } else { 1.0 }) * length;
        let insert_at = if first { 3 } else { at };
        route.splice(insert_at..insert_at, point);
    };

    for i in 0..cedges.len() {
        straighten(&mut cedges[i].route, starts[i], start_room[i], true);
        straighten(&mut cedges[i].route, ends[i], end_room[i], false);
    }

    let mut min_x = 0.0f64;
    let mut min_y = 0.0f64;
    let mut max_x = layout.width;
    let mut max_y = layout.height;

    let mut add_card = |parent: i32,
                        raw: &str,
                        width: f64,
                        x: f64,
                        y: f64,
                        side: usize,
                        after: bool| {
        if raw.is_empty() {
            return;
        }
        let h = tiny_height;
        let mut cx = x;
        let mut cy = y;
        if (side & 1) != 0 {
            cx += (if side == 3 { -1.0 } else { 1.0 }) * (5.0 + width / 2.0);
            cy += (if after { 1.0 } else { -1.0 }) * (7.0 + h / 2.0);
        } else {
            cx += (if after { 1.0 } else { -1.0 }) * (9.0 + width / 2.0);
            cy += (if side == 0 { -1.0 } else { 1.0 }) * (3.0 + h / 2.0);
        }
        cards.push(Card {
            text: decode_entities(raw),
            x: cx,
            y: cy,
            w: width,
            h,
            parent,
        });
        min_x = min_x.min(cx - width / 2.0);
        max_x = max_x.max(cx + width / 2.0);
        min_y = min_y.min(cy - h / 2.0);
        max_y = max_y.max(cy + h / 2.0);
    };

    let mut loop_seen: HashMap<usize, i32> = HashMap::new();
    let mut loop_at: Vec<i32> = vec![0; cedges.len()];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let src = cedges[i].src;
        let dst = cedges[i].dst;
        if src == dst {
            let k = loop_seen.get(&src).copied().unwrap_or(0);
            loop_seen.insert(src, k + 1);
            loop_at[i] = k;
            let c = &cnodes[src];
            let view = &views[src];
            let x = c.x + view.w / 2.0;
            let spread = (view.h / 2.0 - 4.0).min(8.0 + k as f64 * 6.0);
            add_card(
                src as i32,
                &edge.start_label_right,
                start_room[i],
                x,
                c.y - spread,
                1,
                false,
            );
            add_card(
                dst as i32,
                &edge.end_label_left,
                end_room[i],
                x,
                c.y + spread,
                1,
                true,
            );
            cedges[i].label_x =
                c.x + view.w / 2.0 + LOOP + k as f64 * 8.0 + 4.0 + cedges[i].label_w / 2.0;
            cedges[i].label_y = c.y;
        } else if cedges[i].route.len() >= 6 {
            let last = cedges[i].route.len() - 3;
            let r0 = cedges[i].route[0];
            let r1 = cedges[i].route[1];
            let rl0 = cedges[i].route[last];
            let rl1 = cedges[i].route[last + 1];
            let from = starts[i];
            let to = ends[i];
            add_card(
                src as i32,
                &edge.start_label_right,
                start_room[i],
                r0,
                r1,
                from.map_or(2, |f| f.side),
                from.map_or(true, |f| f.after),
            );
            add_card(
                dst as i32,
                &edge.end_label_left,
                end_room[i],
                rl0,
                rl1,
                to.map_or(0, |t| t.side),
                to.map_or(true, |t| t.after),
            );
        }
    }

    if let Some(available) = options.max_width {
        if available > 0.0 && max_x - min_x + 2.0 * pad > available {
            let base_len = cnodes.len();
            for item in &cards {
                let mut cn = cnode(item.w, item.h, item.parent);
                cn.x = item.x;
                cn.y = item.y;
                cnodes.push(cn);
            }
            let wrapped = wrap_components(
                &mut cnodes,
                &mut cedges,
                (available - 2.0 * pad).max(1.0),
                number_option(conf, "nodeSpacing", NODE_SEP),
            );
            if let Some(res) = wrapped {
                for (i, item) in cards.iter_mut().enumerate() {
                    item.x = cnodes[base_len + i].x;
                    item.y = cnodes[base_len + i].y;
                }
                min_x = 0.0;
                min_y = 0.0;
                max_x = res.width;
                max_y = res.height;
            }
            cnodes.truncate(base_len);
        }
    }

    let ox = pad - min_x;
    let oy = pad - min_y;
    let width = (max_x - min_x + 2.0 * pad).ceil();
    let height = (max_y - min_y + 2.0 * pad).ceil();
    let mut links: Vec<LinkInfo> = Vec::new();

    let mut crossings: Crossings = HashMap::new();
    if !graph.nodes.is_empty() && graph.nodes[0].is_group {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
    }

    let mut clusters = String::new();
    let mut struck_titles = String::new();
    let mut nodes_out = String::new();

    for i in 0..views.len() {
        let view = &views[i];
        let c = &cnodes[i];
        let node = view.node;
        let x = c.x + ox;
        let y = c.y + oy;
        let id = esc_text(&node.id);
        let w = if c.is_group { c.w } else { view.w };
        let h = if c.is_group { c.h } else { view.h };
        if node.is_group {
            let tx = if c.is_group {
                cluster_title_x(c, view.label.width, &crossings, GROUP_PAD)
            } else {
                c.x
            };
            let ty = y - h / 2.0 + 8.0 + view.label.height / 2.0;
            let title_attrs = " class=\"pele-cluster-label\" fill=\"var(--_m)\"";
            let struck = c.is_group && title_crossed(c, view.label.width, tx, &crossings);
            if struck {
                struck_titles.push_str(&struck_title(
                    &view.label,
                    tx + ox,
                    ty,
                    title_attrs,
                    &id,
                    &cnodes,
                    i,
                ));
            }
            clusters.push_str(&format!(
                "<g class=\"pele-cluster pele-namespace\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\" fill-opacity=\"0.5\" stroke=\"var(--_b)\"{}/>{}</g>",
                id,
                num(x - w / 2.0),
                num(y - h / 2.0),
                num(w),
                num(h),
                ROUNDED,
                if struck {
                    String::new()
                } else {
                    label_svg(&view.label, tx + ox, ty, title_attrs)
                }
            ));
            continue;
        }
        let at_attr = format!(" transform=\"translate({},{})\"", num(x), num(y));
        if let Some(ref cls) = node.class_node {
            nodes_out.push_str(&class_svg(
                view,
                cls,
                &id,
                &at_attr,
                size,
                tiny,
                tiny_height,
                &is_mono,
                &mut links,
                options,
            ));
        } else if node.shape == NodeShape::Note {
            let r = w / 2.0;
            let top = -h / 2.0;
            nodes_out.push_str(&format!(
                "<g class=\"pele-node pele-note\" data-id=\"{}\"{}><path d=\"M{},{}H{}L{},{}V{}H{}Z\" fill=\"var(--_a)\" stroke=\"var(--_b)\" stroke-linejoin=\"round\"/><path d=\"M{},{}V{}H{}\" fill=\"none\" stroke=\"var(--_b)\" stroke-linejoin=\"round\"/>{}</g>",
                id,
                at_attr,
                num(-r),
                num(top),
                num(r - FOLD),
                num(r),
                num(top + FOLD),
                num(-top),
                num(-r),
                num(r - FOLD),
                num(top),
                num(top + FOLD),
                num(r),
                label_svg(
                    &view.label,
                    0.0,
                    0.0,
                    &format!(" class=\"pele-label\" font-size=\"{}\"", js_number(small))
                )
            ));
        } else {
            nodes_out.push_str(&format!(
                "<g class=\"pele-node pele-interface\" data-id=\"{}\"{}>{}</g>",
                id,
                at_attr,
                label_svg(
                    &view.label,
                    0.0,
                    0.0,
                    &format!(" class=\"pele-label\" font-size=\"{}\"", js_number(small))
                )
            ));
        }
    }

    let mut edges_out = String::new();
    let mut labels_out = String::new();
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let start_type = edge.arrow_type_start;
        let end_type = edge.arrow_type_end;
        let start_trim = class_marker_trim(start_type);
        let end_trim = class_marker_trim(end_type);
        let path: EdgePath;
        if cedges[i].src == cedges[i].dst {
            let src = cedges[i].src;
            let c = &cnodes[src];
            let view = &views[src];
            let k = loop_at[i] as f64;
            let x = c.x + ox + view.w / 2.0;
            let y = c.y + oy;
            let spread = (view.h / 2.0 - 4.0).min(8.0 + k * 6.0);
            let reach = LOOP + k * 8.0;
            path = EdgePath {
                d: format!(
                    "M{},{}C{},{} {},{} {},{}",
                    num(x + start_trim),
                    num(y - spread),
                    num(x + reach + start_trim),
                    num(y - spread),
                    num(x + reach + end_trim),
                    num(y + spread),
                    num(x + end_trim),
                    num(y + spread)
                ),
                tokens: None,
                sx: x,
                sy: y - spread,
                sdx: -1.0,
                sdy: 0.0,
                ex: x,
                ey: y + spread,
                edx: -1.0,
                edy: 0.0,
            };
        } else {
            let route = &mut cedges[i].route;
            let mut k = 0;
            while k < route.len() {
                route[k] += ox;
                route[k + 1] += oy;
                k += 3;
            }
            path = route_path_default(route, None, start_trim, end_trim);
        }
        let attrs = match edge.pattern {
            EdgePattern::Dashed => " stroke-dasharray=\"6 4\"",
            EdgePattern::Dotted => " stroke-dasharray=\"2 4\"",
            EdgePattern::Solid => "",
        };
        let edge_cls = if edge.pattern == EdgePattern::Dotted {
            " pele-note-edge"
        } else {
            " pele-relation"
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge{}\" data-id=\"{}\"><path d=\"{}\"{}/>{}{}</g>",
            edge_cls,
            esc_text(&edge.id),
            path.d,
            attrs,
            class_marker(start_type, path.sx, path.sy, path.sdx, path.sdy, LINE),
            class_marker(end_type, path.ex, path.ey, path.edx, path.edy, LINE)
        ));
        if label.width > 0.0 {
            let x = cedges[i].label_x + ox;
            let y = cedges[i].label_y + oy;
            labels_out.push_str(&edge_label_svg(&edge.id, label, x, y, ""));
        }
    }

    let mut cards_out = String::new();
    for item in &cards {
        cards_out.push_str(&format!(
            "<text class=\"pele-cardinality\" x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
            num(item.x + ox),
            num(item.y + oy + tiny * 0.35),
            esc(&item.text)
        ));
    }

    let mut content = String::new();
    if !clusters.is_empty() {
        content.push_str(&format!("<g class=\"pele-clusters\">{}</g>", clusters));
    }
    if !edges_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"{}\" stroke-linecap=\"round\">{}</g>",
            LINE, edges_out
        ));
    }
    if !struck_titles.is_empty() {
        content.push_str(&format!("<g class=\"pele-cluster-titles\">{}</g>", struck_titles));
    }
    if !labels_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edge-labels\" font-size=\"{}\">{}</g>",
            js_number(small),
            labels_out
        ));
    }
    if !cards_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-cardinalities\" font-size=\"{}\" fill=\"var(--_m)\">{}</g>",
            js_number(tiny),
            cards_out
        ));
    }
    content.push_str(&format!("<g class=\"pele-nodes\">{}</g>", nodes_out));

    let titled = with_title(db.title.as_deref(), measurer, size, pad, width, height, content);
    let fixed = conf.get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "class",
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

#[allow(clippy::too_many_arguments)]
fn class_svg(
    view: &View<'_>,
    cls: &ClassNode,
    id: &str,
    at: &str,
    base_size: f64,
    tiny: f64,
    tiny_height: f64,
    is_mono: &dyn Fn(&ResolvedStyle) -> bool,
    links: &mut Vec<LinkInfo>,
    options: &RenderOptions,
) -> String {
    let w = view.w;
    let h = view.h;
    let st = &view.style;
    let left = -w / 2.0;
    let top = -h / 2.0;
    let member_size = js_round(view.font_size * 0.875);
    let line_height = js_round(member_size * 1.5);
    let mut body = format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>",
        num(left),
        num(top),
        num(w),
        num(h),
        rounded(&st.shape)
    );
    let ink = ink_fill(&st.shape, &st.text, None);
    let first = top + view.title_h;
    let second = first + view.attributes_h;
    if h > view.title_h {
        let stroke = match &st.stroke {
            Some(s) if !s.is_empty() => format!(" style=\"stroke:{}\"", esc(s)),
            _ => String::new(),
        };
        body.push_str(&format!(
            "<path class=\"pele-divider\" d=\"M{},{}H{}M{},{}H{}\" fill=\"none\" stroke=\"var(--_b)\"{}/>",
            num(left),
            num(first),
            num(-left),
            num(left),
            num(second),
            num(-left),
            stroke
        ));
    }
    let mut y = top + TITLE_PAD;
    let annotation_fill = if ink.is_empty() { " fill=\"var(--_m)\"" } else { ink.as_str() };
    for annotation in &view.annotations {
        body.push_str(&format!(
            "<text class=\"pele-annotation\" x=\"0\" y=\"{}\" text-anchor=\"middle\" font-size=\"{}\"{}>{}</text>",
            num(y + tiny_height / 2.0 + tiny * 0.35),
            js_number(tiny),
            annotation_fill,
            esc(annotation)
        ));
        y += tiny_height;
    }
    let title_size = if view.font_size == base_size {
        String::new()
    } else {
        format!(" font-size=\"{}\"", num(view.font_size))
    };
    body.push_str(&label_svg(
        &view.label,
        0.0,
        (y + first - TITLE_PAD) / 2.0,
        &format!(
            " class=\"pele-label\"{} font-weight=\"var(--_hw)\"{}{}",
            title_size, ink, st.text
        ),
    ));

    let format_lines = |list: &[Line], kind: &str, from: f64| -> String {
        let mut out = String::new();
        let mut baseline = from + SECTION_PAD + line_height / 2.0 + member_size * 0.35;
        for line in list {
            let mark = if line.classifier == "$" {
                let st_underline = if !st.text.is_empty() {
                    format!("{}text-decoration:underline\"", &st.text[..st.text.len() - 1])
                } else {
                    " style=\"text-decoration:underline\"".to_string()
                };
                format!(" pele-static\"{}", st_underline)
            } else if line.classifier == "*" {
                format!(" pele-abstract\" font-style=\"italic\"{}", st.text)
            } else {
                format!("\"{}", st.text)
            };
            out.push_str(&format!(
                "<text class=\"pele-member pele-{}{} x=\"{}\" y=\"{}\">{}</text>",
                kind,
                mark,
                num(left + PAD_X),
                num(baseline),
                esc(&line.text)
            ));
            baseline += line_height;
        }
        out
    };

    if !view.attributes.is_empty() || !view.methods.is_empty() {
        let mono_attr = if is_mono(st) {
            format!(" font-family=\"{}\"", FONT_MONO)
        } else {
            String::new()
        };
        body.push_str(&format!(
            "<g class=\"pele-members\" font-size=\"{}\"{}{}>{}{}</g>",
            js_number(member_size),
            ink,
            mono_attr,
            format_lines(&view.attributes, "attribute", first),
            format_lines(&view.methods, "method", second)
        ));
    }

    if let Some(ref tip) = cls.tooltip {
        if !tip.is_empty() {
            body = format!("<title>{}</title>{}", esc_text(tip), body);
        }
    }
    if let Some(ref link_url) = cls.link {
        if !link_url.is_empty() {
            let target_attr = match cls.link_target.as_deref() {
                Some(tgt) if is_valid_target(tgt) => format!(" target=\"{}\"", tgt),
                _ => String::new(),
            };
            body = linked(body, link_url, &cls.id, links, options, &target_attr);
        }
    }

    format!(
        "<g class=\"pele-node pele-class{}\" data-id=\"{}\"{}>{}</g>",
        class_names(strip_default_class(&cls.css_classes)),
        id,
        at,
        body
    )
}
