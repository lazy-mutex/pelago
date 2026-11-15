use super::actors::{actor_kind, actor_size, draw_actor, is_figure, ActorSize};
use super::db::SeqDb;
use super::types::{linetype, placement, SeqActor, SeqMessage, SeqMessageBody};
use crate::diagrams::common::literal_label::break_label;
use crate::options::{tighten, title_room, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::edges::{marker, ARROW};
use crate::svg::ink::ink;
use crate::svg::root::{Accessible, Document, svg_document};
use crate::svg::theme::{class_names, resolve_style, rounded, ROUNDED};
use crate::svg::title::title_label;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Head {
    None,
    Arrow,
    Cross,
    Open,
    HalfSolid,
    HalfStick,
}

#[derive(Clone, Copy)]
struct LineStyle {
    dotted: bool,
    head: Head,
    // Where the heads are: at the receiver, at the sender only (the reverse arrows), or at both.
    at_start: bool,
    at_end: bool,
    bottom: bool,
}

const fn make_style(dotted: bool, head: Head, ends: u8, bottom: bool) -> Option<LineStyle> {
    Some(LineStyle {
        dotted,
        head,
        at_start: ends != 1,
        at_end: ends != 0,
        bottom,
    })
}

const fn build_styles() -> [Option<LineStyle>; 64] {
    let mut s = [None; 64];
    s[linetype::SOLID as usize] = make_style(false, Head::Arrow, 1, false);
    s[linetype::DOTTED as usize] = make_style(true, Head::Arrow, 1, false);
    s[linetype::SOLID_CROSS as usize] = make_style(false, Head::Cross, 1, false);
    s[linetype::DOTTED_CROSS as usize] = make_style(true, Head::Cross, 1, false);
    s[linetype::SOLID_OPEN as usize] = make_style(false, Head::None, 1, false);
    s[linetype::DOTTED_OPEN as usize] = make_style(true, Head::None, 1, false);
    s[linetype::SOLID_POINT as usize] = make_style(false, Head::Open, 1, false);
    s[linetype::DOTTED_POINT as usize] = make_style(true, Head::Open, 1, false);
    s[linetype::BIDIRECTIONAL_SOLID as usize] = make_style(false, Head::Arrow, 2, false);
    s[linetype::BIDIRECTIONAL_DOTTED as usize] = make_style(true, Head::Arrow, 2, false);

    let mut pass = 0;
    while pass < 2 {
        let dotted = pass == 1;
        let d: usize = if dotted { 10 } else { 0 };
        s[linetype::SOLID_TOP as usize + d] = make_style(dotted, Head::HalfSolid, 1, false);
        s[linetype::SOLID_BOTTOM as usize + d] = make_style(dotted, Head::HalfSolid, 1, true);
        s[linetype::STICK_TOP as usize + d] = make_style(dotted, Head::HalfStick, 1, false);
        s[linetype::STICK_BOTTOM as usize + d] = make_style(dotted, Head::HalfStick, 1, true);
        s[linetype::SOLID_ARROW_TOP_REVERSE as usize + d] =
            make_style(dotted, Head::HalfSolid, 0, false);
        s[linetype::SOLID_ARROW_BOTTOM_REVERSE as usize + d] =
            make_style(dotted, Head::HalfSolid, 0, true);
        s[linetype::STICK_ARROW_TOP_REVERSE as usize + d] =
            make_style(dotted, Head::HalfStick, 0, false);
        s[linetype::STICK_ARROW_BOTTOM_REVERSE as usize + d] =
            make_style(dotted, Head::HalfStick, 0, true);
        pass += 1;
    }
    s
}

const STYLES: [Option<LineStyle>; 64] = build_styles();

fn frame_kind(kind: i32) -> Option<&'static str> {
    match kind {
        linetype::LOOP_START => Some("loop"),
        linetype::ALT_START => Some("alt"),
        linetype::OPT_START => Some("opt"),
        linetype::PAR_START | linetype::PAR_OVER_START => Some("par"),
        linetype::CRITICAL_START => Some("critical"),
        linetype::BREAK_START => Some("break"),
        linetype::RECT_START => Some("rect"),
        _ => None,
    }
}

const MSG_PAD: f64 = 12.0;
const SELF_W: f64 = 28.0;
const SELF_H: f64 = 22.0;
const ACT_W: f64 = 10.0;
const ACT_STEP: f64 = 4.0;
const ACT_MIN: f64 = 14.0;
const NOTE_GAP: f64 = 10.0;
const NOTE_OVER: f64 = 16.0;
const FRAME_PAD: f64 = 20.0;
const FRAME_NEST: f64 = 10.0;
const TAB_H: f64 = 20.0;
const FRAME_TEXT: f64 = 240.0;
const BADGE_H: f64 = 16.0;
const CENTRAL_R: f64 = 4.0;
const DASH: &str = " stroke-dasharray=\"4 4\"";

pub struct Numbering {
    // The number each message line carries, whether or not it is shown. NaN for everything else.
    pub index: Vec<f64>,
    pub shown: Vec<bool>,
    #[allow(dead_code)]
    pub visible: bool,
}

fn line_style(kind: Option<i32>) -> Option<LineStyle> {
    let k = kind?;
    if k < 0 {
        return None;
    }
    STYLES.get(k as usize).copied().flatten()
}

pub fn is_line_message(kind: Option<i32>) -> bool {
    line_style(kind).is_some()
}

// Counts message lines the way Mermaid's renderer does while it draws.
pub fn number_messages(messages: &[SeqMessage], always: bool) -> Numbering {
    let mut index = vec![f64::NAN; messages.len()];
    let mut shown = vec![false; messages.len()];
    let mut current = 1.0f64;
    let mut step = 1.0f64;
    let mut visible = false;
    for (i, m) in messages.iter().enumerate() {
        if m.kind == Some(linetype::AUTONUMBER) {
            if let SeqMessageBody::Numbering(ref spec) = m.message {
                if let Some(s) = spec.start.filter(|&n| n != 0.0 && !n.is_nan()) {
                    current = s;
                }
                if let Some(st) = spec.step.filter(|&n| n != 0.0 && !n.is_nan()) {
                    step = st;
                }
                visible = spec.visible;
            }
        } else if is_line_message(m.kind) {
            index[i] = current;
            shown[i] = visible || always;
            current = js_round((current + step) * 100.0) / 100.0;
        }
    }
    Numbering {
        index,
        shown,
        visible,
    }
}

struct Column<'a> {
    id: &'a str,
    actor: &'a SeqActor,
    kind: &'a str,
    label: Label,
    size: ActorSize,
    x: f64,
    create_at: isize,
    destroy_at: isize,
    top: f64,
    life_start: f64,
    life_end: f64,
    activations: Vec<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum V {
    Line,
    SelfLoop,
    Note,
}

#[derive(Clone)]
struct View {
    kind: V,
    a: usize,
    b: usize,
    label: Label,
    x: f64,
    w: f64,
}

#[derive(Clone)]
struct FrameView {
    kind: &'static str,
    x0: f64,
    x1: f64,
    top: f64,
    title: Label,
    tab_w: f64,
    fill: String,
    dividers: Vec<usize>,
    divider_y: Vec<f64>,
    divider_labels: Vec<Label>,
}

struct BoxRun {
    box_idx: usize,
    first: usize,
    last: usize,
    title: Label,
    grow: f64,
}

fn option(conf: &Value, key: &str, fallback: f64) -> f64 {
    match conf.get(key).as_f64() {
        Some(v) if (0.0..=2000.0).contains(&v) => v,
        _ => fallback,
    }
}

fn half(x: f64, y: f64, dir: f64, up: f64, stick: bool, color: &str) -> String {
    let bx = num(x - dir * ARROW);
    if stick {
        format!(
            "<path class=\"pele-marker\" d=\"M{},{}L{},{}\"/>",
            num(x),
            num(y),
            bx,
            num(y + up * 5.0)
        )
    } else {
        format!(
            "<path class=\"pele-marker\" d=\"M{},{}L{},{}V{}Z\" fill=\"{}\" stroke=\"none\"/>",
            num(x),
            num(y),
            bx,
            num(y + up * 4.5),
            num(y),
            color
        )
    }
}

// Draws the head whose tip is at (x, y) and points along dir. `up` is the side a half head sits on.
fn head(kind: Head, x: f64, y: f64, dir: f64, up: f64, color: &str) -> String {
    match kind {
        Head::Arrow => marker("arrow_point", x, y, dir, 0.0, color, "none"),
        Head::Cross => marker("arrow_cross", x, y, dir, 0.0, color, "none"),
        Head::Open => format!(
            "<path class=\"pele-marker\" d=\"M{},{}L{},{}L{},{}\" stroke-linejoin=\"round\"/>",
            num(x - dir * ARROW),
            num(y - 4.5),
            num(x),
            num(y),
            num(x - dir * ARROW),
            num(y + 4.5)
        ),
        Head::HalfSolid => half(x, y, dir, up, false, color),
        Head::HalfStick => half(x, y, dir, up, true, color),
        Head::None => String::new(),
    }
}

fn trim(kind: Head) -> f64 {
    match kind {
        Head::Arrow => ARROW - 1.0,
        Head::Cross => 4.0,
        _ => 0.0,
    }
}

pub fn render_sequence(db: &SeqDb, config: &Value, options: &RenderOptions) -> Rendered {
    tighten(options, &|tight| draw(db, config, options, tight, 0.0, true))
}

fn draw(
    db: &SeqDb,
    config: &Value,
    options: &RenderOptions,
    tight: bool,
    hold: f64,
    by_need: bool,
) -> Rendered {
    let conf = config.get("sequence");
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let tiny = js_round(size * 0.75);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let mirror = conf.get("mirrorActors").as_bool() != Some(false);
    let right_angles = conf.get("rightAngles").as_bool() == Some(true);
    let actor_margin = option(conf, "actorMargin", 40.0).min(if tight { 8.0 } else { f64::INFINITY });
    let min_w = option(conf, "width", 64.0).min(if tight { 48.0 } else { f64::INFINITY });
    let min_h = option(conf, "height", js_round(size * 1.5) + 20.0);
    let wrap_w = option(conf, "width", 160.0).min(if tight {
        (32.0f64).max(
            (options.max_width.unwrap_or(320.0) - 2.0 * pad) / (db.actors.len().max(1) as f64)
                - actor_margin
                - 24.0,
        )
    } else {
        f64::INFINITY
    });
    // Mermaid's default of 35 corresponds to Pele's default gap. Other values move it by the difference.
    let msg_gap = (4.0f64).max(14.0 + option(conf, "messageMargin", 35.0) - 35.0);
    let note_pad = option(conf, "noteMargin", 10.0).min(if tight { 6.0 } else { f64::INFINITY });
    let box_pad = option(conf, "boxMargin", 10.0).min(if tight { 6.0 } else { f64::INFINITY });
    let align: i32 = match conf.get("messageAlign").as_str() {
        Some("left") => -1,
        Some("right") => 1,
        _ => 0,
    };

    let msg_pad = if tight { 6.0 } else { MSG_PAD };
    let note_over = if tight { 8.0 } else { NOTE_OVER };
    let frame_pad = if tight { 12.0 } else { FRAME_PAD };
    let frame_nest = if tight { 6.0 } else { FRAME_NEST };
    let messages = &db.messages;
    let count = messages.len();
    let numbering = number_messages(
        messages,
        conf.get("showSequenceNumbers").as_bool() == Some(true),
    );
    let text = |raw: Option<&str>, font_size: f64, wrap: bool, width: f64| -> Label {
        let label = layout_label(
            raw,
            false,
            measurer,
            font_size,
            if tight || wrap { width } else { f64::INFINITY },
            0,
        );
        if tight {
            break_label(&label, measurer, (120.0f64).max(width))
        } else {
            label
        }
    };
    let message_text =
        |raw: &str, wrap: bool, width: f64| -> Label { text(Some(raw), small, wrap, width) };

    let used: Option<HashSet<&str>> = if conf.get("hideUnusedParticipants").as_bool() == Some(true)
    {
        let mut s = HashSet::new();
        for m in messages {
            if let Some(ref f) = m.from {
                s.insert(f.as_str());
            }
            if let Some(ref t) = m.to {
                s.insert(t.as_str());
            }
        }
        Some(s)
    } else {
        None
    };

    let mut next_line = vec![-1isize; count + 1];
    for i in (0..count).rev() {
        next_line[i] = if is_line_message(messages[i].kind) {
            i as isize
        } else {
            next_line[i + 1]
        };
    }

    let mut cols: Vec<Column> = Vec::new();
    let mut col_of: HashMap<&str, usize> = HashMap::new();
    for (id, actor) in db.actors.iter() {
        if let Some(ref u) = used {
            if !u.contains(id.as_str()) {
                continue;
            }
        }
        let kind = actor_kind(&actor.kind);
        let label = text(Some(&actor.description), size, actor.wrap, wrap_w);
        let created = db.created_actors.get(id.as_str()).copied();
        let destroyed = db.destroyed_actors.get(id.as_str()).copied();
        let mut create_at = match created {
            None => -1,
            Some(c) => next_line[c.min(count)],
        };
        if create_at >= 0 && messages[create_at as usize].to.as_deref() != Some(id.as_str()) {
            create_at = -1;
        }
        let mut destroy_at = match destroyed {
            None => -1,
            Some(d) => next_line[d.min(count)],
        };
        if destroy_at >= 0
            && messages[destroy_at as usize].to.as_deref() != Some(id.as_str())
            && messages[destroy_at as usize].from.as_deref() != Some(id.as_str())
        {
            destroy_at = -1;
        }
        col_of.insert(id.as_str(), cols.len());
        let size_val = actor_size(kind, &label, min_w, min_h, tight.then_some(8.0));
        cols.push(Column {
            id: id.as_str(),
            actor,
            kind,
            label,
            size: size_val,
            x: 0.0,
            create_at,
            destroy_at,
            top: 0.0,
            life_start: 0.0,
            life_end: -1.0,
            activations: Vec::new(),
        });
    }
    let col_count = cols.len();
    let last: isize = col_count as isize - 1;

    // Widths between lifelines. Every message and note says how far apart the lifelines it touches must be.
    let mut needs: HashMap<usize, f64> = HashMap::new();
    let mut need_keys: Vec<usize> = Vec::new();
    let mut need = |lo: isize, hi: isize, width: f64| {
        if lo < 0 || hi > last {
            return;
        }
        let key = (lo as usize) * col_count + (hi as usize);
        let prev = needs.get(&key).copied().unwrap_or(0.0);
        if prev < width {
            if !needs.contains_key(&key) {
                need_keys.push(key);
            }
            needs.insert(key, width);
        }
    };
    let badge_width = |i: usize| -> f64 {
        if numbering.shown[i] {
            BADGE_H.max(measurer.width(&js_number(numbering.index[i]), tiny, 0) + 8.0)
        } else {
            0.0
        }
    };

    let mut views: Vec<Option<View>> = vec![None; count];
    for i in 0..count {
        let m = &messages[i];
        let a = m.from.as_deref().and_then(|f| col_of.get(f).copied());
        let b = m.to.as_deref().and_then(|t| col_of.get(t).copied());
        let (Some(a), Some(b), SeqMessageBody::Text(ref raw_msg)) = (a, b, &m.message) else {
            continue;
        };
        if m.kind == Some(linetype::NOTE) {
            let label = text(Some(raw_msg), small, m.wrap, wrap_w);
            let w = label.width + 2.0 * note_pad;
            if m.placement == Some(placement::LEFTOF) {
                need(a as isize - 1, a as isize, w + 2.0 * NOTE_GAP + ACT_W);
            } else if m.placement == Some(placement::RIGHTOF) {
                need(a as isize, a as isize + 1, w + 2.0 * NOTE_GAP + ACT_W);
            } else if a == b {
                need(a as isize - 1, a as isize, w / 2.0 + NOTE_GAP);
                need(a as isize, a as isize + 1, w / 2.0 + NOTE_GAP);
            } else {
                need(
                    a.min(b) as isize,
                    a.max(b) as isize,
                    w - 2.0 * note_over,
                );
            }
            views[i] = Some(View {
                kind: V::Note,
                a,
                b,
                label,
                x: 0.0,
                w,
            });
        } else if is_line_message(m.kind) {
            let label = message_text(raw_msg, m.wrap, wrap_w);
            let badge = badge_width(i);
            if a == b {
                need(
                    a as isize,
                    a as isize + 1,
                    SELF_W + 8.0 + label.width + 16.0 + ACT_W + badge,
                );
                views[i] = Some(View {
                    kind: V::SelfLoop,
                    a,
                    b,
                    label,
                    x: 0.0,
                    w: 0.0,
                });
            } else {
                let created = if cols[b].create_at == i as isize {
                    cols[b].size.w / 2.0
                } else {
                    0.0
                };
                let central = if m.central_connection.unwrap_or(0) != 0 {
                    4.0 * CENTRAL_R
                } else {
                    0.0
                };
                let lw = if tight {
                    label.width.min((16.0f64).max(wrap_w - 16.0))
                } else {
                    label.width
                };
                need(
                    a.min(b) as isize,
                    a.max(b) as isize,
                    lw + 2.0 * msg_pad + ACT_W + badge + created + central,
                );
                views[i] = Some(View {
                    kind: V::Line,
                    a,
                    b,
                    label,
                    x: 0.0,
                    w: 0.0,
                });
            }
        }
    }

    let initial_labels: Vec<Option<Label>> =
        views.iter().map(|v| v.as_ref().map(|vw| vw.label.clone())).collect();
    let mut sized: Vec<Option<HashMap<u64, Label>>> = vec![None; count];

    let mut runs: Vec<BoxRun> = Vec::new();
    for (i, col) in cols.iter().enumerate() {
        if let Some(box_idx) = col.actor.box_idx {
            if let Some(run) = runs.last_mut().filter(|r| r.box_idx == box_idx && r.last + 1 == i) {
                run.last = i;
            } else {
                let b = &db.boxes[box_idx];
                runs.push(BoxRun {
                    box_idx,
                    first: i,
                    last: i,
                    title: text(b.name.as_deref(), small, b.wrap, wrap_w),
                    grow: 0.0,
                });
            }
        }
    }

    let gap_count = col_count.saturating_sub(1);
    let mut gaps = vec![0.0f64; gap_count];
    for i in 0..gap_count {
        let left = &cols[i];
        let right = &cols[i + 1];
        let boxed = if left.actor.box_idx != right.actor.box_idx {
            (if left.actor.box_idx.is_some() { box_pad } else { 0.0 })
                + (if right.actor.box_idx.is_some() { box_pad } else { 0.0 })
        } else {
            0.0
        };
        gaps[i] = (left.size.w / 2.0 + right.size.w / 2.0 + actor_margin + boxed).max(
            needs
                .get(&(i * col_count + i + 1))
                .copied()
                .unwrap_or(0.0),
        );
    }

    // A message across several lifelines widens each gap it crosses by an equal share of what is missing.
    let mut spans: Vec<(usize, usize, f64)> = Vec::new();
    for &key in &need_keys {
        let width = needs[&key];
        let lo = key / col_count;
        let hi = key % col_count;
        if hi - lo > 1 {
            spans.push((lo, hi, width));
        }
    }
    spans.sort_by(|p, q| ((p.1 - p.0).cmp(&(q.1 - q.0))).then_with(|| p.0.cmp(&q.0)));
    for (lo, hi, width) in spans {
        let total: f64 = gaps[lo..hi].iter().sum();
        if total >= width {
            continue;
        }
        let share = (width - total) / ((hi - lo) as f64);
        for g in &mut gaps[lo..hi] {
            *g += share;
        }
    }

    let place = |cols: &mut [Column], gaps: &[f64]| {
        for i in 1..cols.len() {
            cols[i].x = cols[i - 1].x + gaps[i - 1];
        }
    };
    place(&mut cols, &gaps);
    for run in &mut runs {
        let inner = cols[run.last].x + cols[run.last].size.w / 2.0
            - (cols[run.first].x - cols[run.first].size.w / 2.0);
        run.grow = (0.0f64).max((run.title.width - inner) / 2.0);
        if run.grow == 0.0 {
            continue;
        }
        if run.first > 0 {
            gaps[run.first - 1] += run.grow;
        }
        if run.last < gap_count {
            gaps[run.last] += run.grow;
        }
    }

    let box_edges = |cols: &[Column], run: &BoxRun| -> (f64, f64) {
        (
            cols[run.first].x - cols[run.first].size.w / 2.0 - box_pad - run.grow,
            cols[run.last].x + cols[run.last].size.w / 2.0 + box_pad + run.grow,
        )
    };

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut frames: Vec<Option<FrameView>> = vec![None; count];

    let mut do_layout = |cols: &mut [Column],
                         gaps: &[f64],
                         views: &mut [Option<View>],
                         frames: &mut [Option<FrameView>],
                         min_x_out: &mut f64,
                         max_x_out: &mut f64| {
        place(cols, gaps);
        let mut lo_x = f64::INFINITY;
        let mut hi_x = f64::NEG_INFINITY;
        let mut extend = |x0: f64, x1: f64| {
            if x0 < lo_x {
                lo_x = x0;
            }
            if x1 > hi_x {
                hi_x = x1;
            }
        };
        for col in cols.iter() {
            extend(col.x - col.size.w / 2.0, col.x + col.size.w / 2.0);
        }
        for run in &runs {
            let (bx0, bx1) = box_edges(cols, run);
            extend(bx0, bx1);
        }
        for f in frames.iter_mut() {
            *f = None;
        }

        // Horizontal extents of every block, from what it holds.
        let mut open: Vec<(usize, FrameView)> = Vec::new();
        let span_all = |cols: &[Column]| -> (f64, f64) {
            if cols.is_empty() {
                (0.0, 120.0)
            } else {
                (cols[0].x - frame_pad, cols[cols.len() - 1].x + frame_pad)
            }
        };
        for i in 0..count {
            let m = &messages[i];
            let m_kind = m.kind;
            if let Some(fk) = m_kind.and_then(frame_kind) {
                let fill = match (fk, &m.message) {
                    ("rect", SeqMessageBody::Text(s)) => s.clone(),
                    _ => String::new(),
                };
                open.push((
                    i,
                    FrameView {
                        kind: fk,
                        x0: f64::INFINITY,
                        x1: f64::NEG_INFINITY,
                        top: 0.0,
                        title: Label::default(),
                        tab_w: 0.0,
                        fill,
                        dividers: vec![i],
                        divider_y: Vec::new(),
                        divider_labels: Vec::new(),
                    },
                ));
            } else if matches!(
                m_kind,
                Some(linetype::ALT_ELSE | linetype::PAR_AND | linetype::CRITICAL_OPTION)
            ) {
                if let Some((_, top_frame)) = open.last_mut() {
                    top_frame.dividers.push(i);
                }
            } else if matches!(
                m_kind,
                Some(
                    linetype::LOOP_END
                        | linetype::ALT_END
                        | linetype::OPT_END
                        | linetype::PAR_END
                        | linetype::CRITICAL_END
                        | linetype::BREAK_END
                        | linetype::RECT_END
                )
            ) {
                let Some((start_idx, mut frame)) = open.pop() else {
                    continue;
                };
                if frame.x0 > frame.x1 {
                    let (sx0, sx1) = span_all(cols);
                    frame.x0 = sx0;
                    frame.x1 = sx1;
                }
                if frame.kind != "rect" {
                    frame.tab_w = measurer.width(frame.kind, tiny, style::BOLD) + 14.0;
                    // A block grows to the right to give its texts up to FRAME_TEXT of width before they wrap.
                    for (k, &div_idx) in frame.dividers.iter().enumerate() {
                        let raw = match &messages[div_idx].message {
                            SeqMessageBody::Text(s) if !s.is_empty() => Some(s.as_str()),
                            _ => None,
                        };
                        let tab = if k == 0 { frame.tab_w + 8.0 } else { 0.0 };
                        let room = frame.x1 - frame.x0 - 16.0 - tab;
                        let max_room = room.max(if tight {
                            FRAME_TEXT.min(wrap_w)
                        } else {
                            FRAME_TEXT
                        });
                        let label = match raw {
                            Some(r) => text(Some(&format!("[{}]", r)), small, true, max_room),
                            None => Label::default(),
                        };
                        if label.width > room {
                            frame.x1 += label.width - room;
                        }
                        if k == 0 {
                            frame.title = label;
                        } else {
                            frame.divider_labels.push(label);
                        }
                    }
                }
                extend(frame.x0, frame.x1);
                if let Some((_, parent)) = open.last_mut() {
                    parent.x0 = parent.x0.min(frame.x0 - frame_nest);
                    parent.x1 = parent.x1.max(frame.x1 + frame_nest);
                }
                frames[start_idx] = Some(frame.clone());
                frames[i] = Some(frame);
            } else if let Some(ref mut view) = views[i] {
                let ax = cols[view.a].x;
                let bx = cols[view.b].x;
                let b_create_at = cols[view.b].create_at;
                let b_w = cols[view.b].size.w;
                let (mut x0, mut x1);
                match view.kind {
                    V::Note => {
                        if m.placement == Some(placement::LEFTOF) {
                            view.x = ax - NOTE_GAP - view.w;
                        } else if m.placement == Some(placement::RIGHTOF) {
                            view.x = ax + NOTE_GAP;
                        } else if view.a == view.b {
                            view.x = ax - view.w / 2.0;
                        } else {
                            let lo = ax.min(bx) - note_over;
                            let hi = ax.max(bx) + note_over;
                            if tight || m.wrap {
                                if let SeqMessageBody::Text(ref raw_msg) = m.message {
                                    view.label = text(
                                        Some(raw_msg),
                                        small,
                                        true,
                                        wrap_w.max(hi - lo - 2.0 * note_pad),
                                    );
                                }
                            }
                            view.w = (hi - lo).max(view.label.width + 2.0 * note_pad);
                            view.x = (lo + hi - view.w) / 2.0;
                        }
                        x0 = view.x;
                        x1 = view.x + view.w;
                    }
                    V::SelfLoop => {
                        x0 = ax;
                        x1 = ax + ACT_W + SELF_W + 8.0 + view.label.width + badge_width(i);
                    }
                    V::Line => {
                        x0 = ax.min(bx);
                        x1 = ax.max(bx);
                        if tight || m.wrap {
                            let room = wrap_w.max(x1 - x0 - 2.0 * msg_pad - ACT_W);
                            let bits = room.to_bits();
                            let wrap_bits = wrap_w.to_bits();
                            if sized[i].is_none() {
                                if bits == wrap_bits {
                                    view.label = initial_labels[i].clone().unwrap_or_default();
                                } else {
                                    let mut map = HashMap::new();
                                    if let Some(ref init) = initial_labels[i] {
                                        map.insert(wrap_bits, init.clone());
                                    }
                                    sized[i] = Some(map);
                                }
                            }
                            if let Some(ref mut known) = sized[i] {
                                if let Some(cached) = known.get(&bits) {
                                    view.label = cached.clone();
                                } else if let SeqMessageBody::Text(ref raw_msg) = m.message {
                                    let lbl = message_text(raw_msg, true, room);
                                    known.insert(bits, lbl.clone());
                                    view.label = lbl;
                                }
                            }
                        }
                        if b_create_at == i as isize {
                            if bx > ax {
                                x1 += b_w / 2.0;
                            } else {
                                x0 -= b_w / 2.0;
                            }
                        }
                    }
                }
                extend(x0, x1);
                if let Some((_, top_frame)) = open.last_mut() {
                    top_frame.x0 = top_frame.x0.min(x0 - frame_pad);
                    top_frame.x1 = top_frame.x1.max(x1 + frame_pad);
                }
            }
        }
        // Any unclosed frame stays stored at its start index as in JS.
        for (start_idx, open_frame) in open {
            frames[start_idx] = Some(open_frame);
        }
        *min_x_out = lo_x;
        *max_x_out = hi_x;
    };

    do_layout(
        &mut cols,
        &gaps,
        &mut views,
        &mut frames,
        &mut min_x,
        &mut max_x,
    );

    // Widen gaps only if it reduces message wrapping.
    let mut grown = false;
    let mut uneven = false;
    if tight {
        if let Some(max_width_opt) = options.max_width {
            let spare = max_width_opt.floor() - (max_x - min_x + 2.0 * pad).ceil() - hold;
            let mut stretch = vec![0i32; col_count];
            let mut want = vec![0.0f64; gap_count];
            let mut wrapped: Vec<usize> = Vec::new();
            let mut lines = 0usize;
            if spare > 0.0 {
                for (i, view_opt) in views.iter().enumerate() {
                    let Some(ref view) = view_opt else {
                        continue;
                    };
                    if view.kind != V::Line {
                        continue;
                    }
                    let SeqMessageBody::Text(ref raw_msg) = messages[i].message else {
                        continue;
                    };
                    let natural = message_text(raw_msg, false, f64::INFINITY);
                    if view.label.lines.len() <= natural.lines.len() {
                        continue;
                    }
                    wrapped.push(i);
                    lines += view.label.lines.len();
                    let lo = view.a.min(view.b);
                    let hi = view.a.max(view.b);
                    stretch[lo] += 1;
                    stretch[hi] -= 1;
                    let short = (natural.width - (cols[hi].x - cols[lo].x - 2.0 * msg_pad - ACT_W))
                        / ((hi - lo) as f64);
                    for w in &mut want[lo..hi] {
                        *w = w.max(short);
                    }
                }
            }
            let mut active = 0i32;
            let mut crossed = 0i32;
            for i in 0..gap_count {
                active += stretch[i];
                stretch[i] = if active > 0 { 1 } else { 0 };
                crossed += stretch[i];
            }
            if crossed > 0 {
                let before = gaps.clone();
                let mut spread = |share: &dyn Fn(usize) -> f64,
                                  cols: &mut [Column],
                                  gaps: &mut [f64],
                                  views: &mut [Option<View>],
                                  frames: &mut [Option<FrameView>],
                                  min_x_out: &mut f64,
                                  max_x_out: &mut f64|
                 -> Vec<usize> {
                    for g in 0..gaps.len() {
                        gaps[g] = before[g] + share(g);
                    }
                    do_layout(cols, gaps, views, frames, min_x_out, max_x_out);
                    wrapped
                        .iter()
                        .map(|&idx| views[idx].as_ref().unwrap().label.lines.len())
                        .collect()
                };
                let even = |g: usize| -> f64 { (stretch[g] as f64 * spare) / (crossed as f64) };
                let mut counts = spread(
                    &even,
                    &mut cols,
                    &mut gaps,
                    &mut views,
                    &mut frames,
                    &mut min_x,
                    &mut max_x,
                );
                let missing: f64 = want.iter().sum();
                // Allow for rounding error.
                let needs_fn = |g: usize| -> f64 { (want[g] * (spare - 0.01)) / missing };
                // Reducing wrapping requires at least one gap wider than the even split.
                if by_need
                    && crossed > 1
                    && missing > 0.0
                    && (0..gap_count).any(|g| needs_fn(g) > even(g))
                {
                    let first = counts;
                    counts = spread(
                        &needs_fn,
                        &mut cols,
                        &mut gaps,
                        &mut views,
                        &mut frames,
                        &mut min_x,
                        &mut max_x,
                    );
                    let sum_counts: usize = counts.iter().sum();
                    let sum_first: usize = first.iter().sum();
                    uneven = sum_counts < sum_first
                        && counts.iter().zip(&first).all(|(&n, &f)| n <= f);
                    if !uneven {
                        counts = spread(
                            &even,
                            &mut cols,
                            &mut gaps,
                            &mut views,
                            &mut frames,
                            &mut min_x,
                            &mut max_x,
                        );
                    }
                }
                let final_sum: usize = counts.iter().sum();
                grown = final_sum < lines;
                if !grown {
                    gaps.clone_from(&before);
                    do_layout(
                        &mut cols,
                        &gaps,
                        &mut views,
                        &mut frames,
                        &mut min_x,
                        &mut max_x,
                    );
                }
            }
        }
    }

    // Vertical placement, top to bottom, drawing as it goes.
    let mut box_title_h = 0.0f64;
    for run in &runs {
        box_title_h = box_title_h.max(run.title.height);
    }
    let box_top = if !runs.is_empty() {
        box_pad + if box_title_h > 0.0 { box_title_h + 4.0 } else { 0.0 }
    } else {
        0.0
    };
    let mut row_h = 0.0f64;
    for col in &cols {
        if col.create_at < 0 {
            row_h = row_h.max(col.size.h);
        }
    }
    let head_bottom = box_top + row_h;
    for col in &mut cols {
        col.top = head_bottom - col.size.h;
        col.life_start = head_bottom;
    }

    let xs: Vec<f64> = cols.iter().map(|col| col.x).collect();
    // Whether a lifeline runs between two x positions, where text would be struck through.
    let crosses = |x0: f64, x1: f64| -> bool {
        let mut lo = 0usize;
        let mut hi = xs.len();
        while lo < hi {
            let mid = (lo + hi) >> 1;
            if xs[mid] <= x0 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo < xs.len() && xs[lo] < x1
    };

    let run_bounds: Vec<(f64, f64, &str)> = runs
        .iter()
        .map(|run| {
            let (bx0, bx1) = box_edges(&cols, run);
            (bx0, bx1, db.boxes[run.box_idx].fill.as_str())
        })
        .collect();

    let mut stack: Vec<FrameView> = Vec::new();
    let mut inks: HashMap<String, Option<&'static str>> = HashMap::new();
    let mut ink_of = |fill: Option<&str>| -> Option<&'static str> {
        let f = fill.filter(|s| !s.is_empty())?;
        if let Some(&cached) = inks.get(f) {
            return cached;
        }
        let resolved = ink(&resolve_style(&[format!("fill:{}", f)]).shape, "");
        inks.insert(f.to_string(), resolved);
        resolved
    };
    let ink_on = |ink_of: &mut dyn FnMut(Option<&str>) -> Option<&'static str>,
                  fill: Option<&str>|
     -> String {
        match ink_of(fill) {
            Some(color) => format!(" fill=\"{}\"", color),
            None => String::new(),
        }
    };

    // Prefer the innermost rect's fill, then a containing box's.
    let fill_under = |stack: &[FrameView], l: f64, r: f64| -> Option<String> {
        for f in stack.iter().rev() {
            if f.kind == "rect" {
                return Some(f.fill.clone());
            }
        }
        for &(x0, x1, bfill) in &run_bounds {
            if l >= x0 && r <= x1 {
                return Some(bfill.to_string());
            }
        }
        None
    };

    let backed = |label: &Label,
                  cx: f64,
                  cy: f64,
                  is_frame: bool,
                  stack: &[FrameView],
                  ink_of: &mut dyn FnMut(Option<&str>) -> Option<&'static str>|
     -> String {
        if label.width == 0.0 {
            return String::new();
        }
        let l = cx - label.width / 2.0;
        let plate = crosses(l, l + label.width);
        let back = if plate {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" fill=\"var(--_bg)\" stroke=\"none\"/>",
                num(l - 3.0),
                num(cy - label.height / 2.0 + 2.0),
                num(label.width + 6.0),
                num(label.height - 4.0)
            )
        } else {
            String::new()
        };
        let fill = if plate {
            String::new()
        } else {
            let f = fill_under(stack, l, l + label.width);
            ink_on(ink_of, f.as_deref())
        };
        let attrs = if is_frame {
            format!(
                " class=\"pele-frame-label\"{} stroke=\"none\"",
                if fill.is_empty() {
                    " fill=\"var(--_m)\""
                } else {
                    &fill
                }
            )
        } else {
            fill
        };
        format!("{}{}", back, label_svg(label, cx, cy, &attrs))
    };

    let left_edge = |col: &Column, extra: usize| -> f64 {
        if col.activations.len() + extra > 0 {
            col.x - ACT_W / 2.0
        } else {
            col.x
        }
    };
    let right_edge = |col: &Column, extra: usize| -> f64 {
        let depth = col.activations.len() + extra;
        if depth > 0 {
            col.x + ACT_W / 2.0 + ((depth - 1) as f64) * ACT_STEP
        } else {
            col.x
        }
    };

    // Shaded blocks and activations close from the inside out. They are written in reverse, so that
    // the inner ones end up on top.
    let mut rects_out: Vec<String> = Vec::new();
    let mut activations_out: Vec<String> = Vec::new();
    let mut frames_out = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();
    let mut notes_out = String::new();
    let mut actors_out = String::new();
    let mut crosses_out = String::new();

    fn extend_bounds(min_x: &mut f64, max_x: &mut f64, x0: f64, x1: f64) {
        if x0 < *min_x {
            *min_x = x0;
        }
        if x1 > *max_x {
            *max_x = x1;
        }
    }

    let end_activation = |col: &mut Column,
                          end: f64,
                          activations_out: &mut Vec<String>,
                          min_x: &mut f64,
                          max_x: &mut f64|
     -> f64 {
        let start = col.activations.pop().unwrap();
        let bottom = end.max(start + ACT_MIN);
        let x = col.x - ACT_W / 2.0 + (col.activations.len() as f64) * ACT_STEP;
        if x < *min_x {
            *min_x = x;
        }
        if x + ACT_W > *max_x {
            *max_x = x + ACT_W;
        }
        activations_out.push(format!(
            "<rect class=\"pele-activation\" data-id=\"{}\" x=\"{}\" y=\"{}\" width=\"{ACT_W}\" height=\"{}\" rx=\"2\"/>",
            esc_text(col.id),
            num(x),
            num(start),
            num(bottom - start)
        ));
        bottom
    };

    let badge = |i: usize, cx: f64, cy: f64, w: f64| -> String {
        format!(
            "<g class=\"pele-sequence-number\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{BADGE_H}\" rx=\"{}\" fill=\"var(--_l)\" stroke=\"none\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"{}\" fill=\"var(--_bg)\" stroke=\"none\">{}</text></g>",
            num(cx - w / 2.0),
            num(cy - BADGE_H / 2.0),
            num(w),
            js_number(BADGE_H / 2.0),
            num(cx),
            num(cy + tiny * 0.35),
            js_number(tiny),
            esc(&js_number(numbering.index[i]))
        )
    };

    let mut y = head_bottom + 6.0;
    for i in 0..count {
        let m = &messages[i];
        let m_kind = m.kind;
        let view = views[i].as_ref();
        let frame = frames[i].as_ref();

        if m_kind == Some(linetype::ACTIVE_START) {
            if let Some(&col_idx) = m.from.as_deref().and_then(|f| col_of.get(f)) {
                cols[col_idx].activations.push(y);
            }
        } else if m_kind == Some(linetype::ACTIVE_END) {
            if let Some(&col_idx) = m.from.as_deref().and_then(|f| col_of.get(f)) {
                if !cols[col_idx].activations.is_empty() {
                    let btm = end_activation(
                        &mut cols[col_idx],
                        y,
                        &mut activations_out,
                        &mut min_x,
                        &mut max_x,
                    );
                    y = y.max(btm);
                }
            }
        } else if let Some(f) = frame.filter(|f| f.dividers[0] == i) {
            let mut f = f.clone();
            if f.kind == "rect" {
                f.top = y + 8.0;
                y = f.top;
            } else {
                f.top = y + 12.0;
                y = f.top + TAB_H.max(f.title.height + 2.0);
            }
            stack.push(f);
        } else if frame.is_some() {
            if let Some(frame) = stack.pop() {
                let bottom = y + 12.0;
                let x = num(frame.x0);
                let w = num(frame.x1 - frame.x0);
                if frame.kind == "rect" {
                    let fill = if frame.fill.is_empty() {
                        String::new()
                    } else {
                        resolve_style(&[format!("fill:{}", frame.fill)]).shape
                    };
                    let attr_in = format!(
                        "{}{}",
                        if fill.is_empty() {
                            " fill-opacity=\"0.5\""
                        } else {
                            ""
                        },
                        fill
                    );
                    rects_out.push(format!(
                        "<rect class=\"pele-rect\" x=\"{x}\" y=\"{}\" width=\"{w}\" height=\"{}\"{}/>",
                        num(frame.top),
                        num(bottom - frame.top),
                        rounded(&attr_in)
                    ));
                } else {
                    let mut inner = format!(
                        "<rect x=\"{x}\" y=\"{}\" width=\"{w}\" height=\"{}\" fill=\"none\"{ROUNDED}/>",
                        num(frame.top),
                        num(bottom - frame.top)
                    );
                    for k in 0..frame.divider_y.len() {
                        let dy = frame.divider_y[k];
                        let label = &frame.divider_labels[k];
                        inner.push_str(&format!(
                            "<path d=\"M{x},{}H{}\"{DASH}/>{}",
                            num(dy),
                            num(frame.x1),
                            backed(
                                label,
                                frame.x0 + 8.0 + label.width / 2.0,
                                dy + 2.0 + label.height / 2.0,
                                true,
                                &stack,
                                &mut ink_of,
                            )
                        ));
                    }
                    inner.push_str(&format!(
                        "<rect x=\"{x}\" y=\"{}\" width=\"{}\" height=\"{TAB_H}\" fill=\"var(--_s)\"{ROUNDED}/><text class=\"pele-frame-kind\" x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"{}\" font-weight=\"var(--_hw)\" fill=\"var(--_m)\" stroke=\"none\">{}</text>{}",
                        num(frame.top),
                        num(frame.tab_w),
                        num(frame.x0 + frame.tab_w / 2.0),
                        num(frame.top + TAB_H / 2.0 + tiny * 0.35),
                        js_number(tiny),
                        frame.kind,
                        backed(
                            &frame.title,
                            frame.x0 + frame.tab_w + 8.0 + frame.title.width / 2.0,
                            frame.top + TAB_H.max(frame.title.height) / 2.0,
                            true,
                            &stack,
                            &mut ink_of,
                        )
                    ));
                    frames_out.push_str(&format!(
                        "<g class=\"pele-frame pele-frame-{}\">{}</g>",
                        frame.kind, inner
                    ));
                }
                y = bottom;
            }
        } else if matches!(
            m_kind,
            Some(linetype::ALT_ELSE | linetype::PAR_AND | linetype::CRITICAL_OPTION)
        ) {
            if let Some(current) = stack.last_mut() {
                let idx = current.divider_y.len();
                let lh = current
                    .divider_labels
                    .get(idx)
                    .map_or(0.0, |l| if l.height > 0.0 { l.height } else { 0.0 });
                current.divider_y.push(y + 12.0);
                y += 12.0 + lh;
            }
        } else if let Some(view) = view.filter(|v| v.kind == V::Note) {
            let a = &cols[view.a];
            let h = view.label.height + 2.0 * (4.0f64).max(note_pad - 4.0);
            let top = y + 10.0;
            let mut x = view.x;
            if m.placement == Some(placement::RIGHTOF) {
                x += right_edge(a, 0) - a.x;
            } else if m.placement == Some(placement::LEFTOF) {
                x += left_edge(a, 0) - a.x;
            }
            extend_bounds(&mut min_x, &mut max_x, x, x + view.w);
            notes_out.push_str(&format!(
                "<g class=\"pele-note\" data-id=\"i{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{ROUNDED}/>{}</g>",
                esc_text(&m.id),
                num(x),
                num(top),
                num(view.w),
                num(h),
                label_svg(
                    &view.label,
                    x + view.w / 2.0,
                    top + h / 2.0,
                    " class=\"pele-label\" stroke=\"none\" fill=\"var(--_fg)\""
                )
            ));
            y = top + h;
        } else if let Some(view) = view {
            let st = line_style(m_kind).unwrap();
            let va = view.a;
            let vb = view.b;
            let label = &view.label;
            let bw = badge_width(i);
            // The activation a `+` starts belongs to this message, so the arrow stops at its edge.
            let mut starting = 0usize;
            for next_m in &messages[i + 1..] {
                let t_next = next_m.kind;
                if t_next == Some(linetype::ACTIVE_START) {
                    if next_m.from == m.to {
                        starting += 1;
                    }
                } else if !matches!(
                    t_next,
                    Some(
                        linetype::ACTIVE_END
                            | linetype::CENTRAL_CONNECTION
                            | linetype::CENTRAL_CONNECTION_REVERSE
                    )
                ) {
                    break;
                }
            }
            let up = if st.bottom { 1.0 } else { -1.0 };
            let attrs = if st.dotted { DASH } else { "" };
            let cc = m.central_connection.unwrap_or(0);
            let circle_to =
                cc == linetype::CENTRAL_CONNECTION || cc == linetype::CENTRAL_CONNECTION_DUAL;
            let circle_from = cc == linetype::CENTRAL_CONNECTION_REVERSE
                || cc == linetype::CENTRAL_CONNECTION_DUAL;
            let tail_at_end = st.at_start && !st.at_end;
            let head_trim = trim(st.head);
            let mut inner = String::new();
            let line_y: f64;
            let line_color: Option<&'static str>;

            if view.kind == V::SelfLoop {
                let ax = cols[va].x;
                let top = y + msg_gap + (0.0f64).max((label.height - SELF_H) / 2.0);
                let bottom = top + SELF_H;
                let mut x0 = if circle_from {
                    ax + CENTRAL_R
                } else {
                    right_edge(&cols[va], 0)
                };
                let mut x1 = if circle_to {
                    ax + CENTRAL_R
                } else {
                    right_edge(&cols[va], starting)
                };
                let out = x0.max(x1) + SELF_W + bw / 2.0;
                let under_fill = fill_under(&stack, ax, out);
                line_color = ink_of(under_fill.as_deref());
                let color = line_color.unwrap_or("var(--_l)");
                let circle = |cx: f64, cy: f64| -> String {
                    format!(
                        "<circle class=\"pele-central\" cx=\"{}\" cy=\"{}\" r=\"{CENTRAL_R}\" fill=\"{color}\" stroke=\"none\"/>",
                        num(cx),
                        num(cy)
                    )
                };
                if circle_from {
                    inner.push_str(&circle(ax, top));
                }
                if circle_to {
                    inner.push_str(&circle(ax, bottom));
                }
                if bw > 0.0 {
                    inner.push_str(&badge(
                        i,
                        if tail_at_end { x1 } else { x0 },
                        if tail_at_end { bottom } else { top },
                        bw,
                    ));
                    if tail_at_end {
                        x1 += bw / 2.0;
                    } else {
                        x0 += bw / 2.0;
                    }
                }
                // The loop leaves to the right and comes back heading left, which turns a half head over.
                let heads = format!(
                    "{}{}",
                    if st.at_start {
                        head(st.head, x0, top, -1.0, up, color)
                    } else {
                        String::new()
                    },
                    if st.at_end {
                        head(st.head, x1, bottom, -1.0, -up, color)
                    } else {
                        String::new()
                    }
                );
                let from = num(x0 + if st.at_start { head_trim } else { 0.0 });
                let to = num(x1 + if st.at_end { head_trim } else { 0.0 });
                let d = if right_angles {
                    format!("M{from},{}H{}V{}H{to}", num(top), num(out), num(bottom))
                } else {
                    format!(
                        "M{from},{}H{}A6,6 0 0 1 {},{}V{}A6,6 0 0 1 {},{}H{to}",
                        num(top),
                        num(out - 6.0),
                        num(out),
                        num(top + 6.0),
                        num(bottom - 6.0),
                        num(out - 6.0),
                        num(bottom)
                    )
                };
                inner = format!("<path d=\"{d}\"{attrs}/>{}{}", heads, inner);
                extend_bounds(&mut min_x, &mut max_x, ax, out + 8.0 + label.width);
                if label.width > 0.0 {
                    let lbl_fill = fill_under(&stack, out + 8.0, out + 8.0 + label.width);
                    let lbl_attrs = ink_on(&mut ink_of, lbl_fill.as_deref());
                    labels_out.push_str(&format!(
                        "<g class=\"pele-edge-label\" data-id=\"i{}\">{}</g>",
                        esc_text(&m.id),
                        label_svg(
                            label,
                            out + 8.0 + label.width / 2.0,
                            (top + bottom) / 2.0,
                            &lbl_attrs
                        )
                    ));
                }
                // A label taller than the loop reaches below it, so the next row starts under the label.
                line_y = bottom.max((top + bottom) / 2.0 + label.height / 2.0);
            } else {
                let ax = cols[va].x;
                let bx = cols[vb].x;
                let dir = if bx > ax { 1.0 } else { -1.0 };
                let creating = cols[vb].create_at == i as isize;
                let under_fill = fill_under(&stack, ax.min(bx), ax.max(bx));
                line_color = ink_of(under_fill.as_deref());
                let color = line_color.unwrap_or("var(--_l)");
                let circle = |cx: f64, cy: f64| -> String {
                    format!(
                        "<circle class=\"pele-central\" cx=\"{}\" cy=\"{}\" r=\"{CENTRAL_R}\" fill=\"{color}\" stroke=\"none\"/>",
                        num(cx),
                        num(cy)
                    )
                };
                let b_h = cols[vb].size.h;
                let b_w = cols[vb].size.w;
                let b_kind = cols[vb].kind;
                line_y = y
                    + msg_gap
                    + (if label.height > 0.0 {
                        label.height + 2.0
                    } else {
                        6.0
                    })
                    .max(if creating {
                        b_h / 2.0 - msg_gap + 6.0
                    } else {
                        0.0
                    });
                let mut x0 = if dir > 0.0 {
                    right_edge(&cols[va], 0)
                } else {
                    left_edge(&cols[va], 0)
                };
                let mut x1 = if dir > 0.0 {
                    left_edge(&cols[vb], starting)
                } else {
                    right_edge(&cols[vb], starting)
                };
                if creating {
                    cols[vb].top = line_y - b_h / 2.0;
                    cols[vb].life_start = cols[vb].top + b_h;
                    x1 = bx - dir * if is_figure(b_kind) { 18.0 } else { b_w / 2.0 + 1.0 };
                }
                if circle_from {
                    inner.push_str(&circle(ax, line_y));
                    x0 = ax + dir * CENTRAL_R;
                }
                if circle_to {
                    inner.push_str(&circle(bx, line_y));
                    x1 = bx - dir * CENTRAL_R;
                }
                if bw > 0.0 {
                    // The number sits on the lifeline at the tail of the arrow, past a connection circle if there is one.
                    if tail_at_end {
                        let cx = if circle_to {
                            x1 - dir * (bw / 2.0 + 2.0)
                        } else {
                            x1
                        };
                        inner.push_str(&badge(i, cx, line_y, bw));
                        x1 = cx - (dir * bw) / 2.0;
                    } else {
                        let cx = if circle_from {
                            x0 + dir * (bw / 2.0 + 2.0)
                        } else {
                            x0
                        };
                        inner.push_str(&badge(i, cx, line_y, bw));
                        x0 = cx + (dir * bw) / 2.0;
                    }
                }
                let heads = format!(
                    "{}{}",
                    if st.at_start {
                        head(st.head, x0, line_y, -dir, up * dir, color)
                    } else {
                        String::new()
                    },
                    if st.at_end {
                        head(st.head, x1, line_y, dir, up * dir, color)
                    } else {
                        String::new()
                    }
                );
                let from = x0 + if st.at_start { dir * head_trim } else { 0.0 };
                let to = x1 - if st.at_end { dir * head_trim } else { 0.0 };
                inner = format!(
                    "<path d=\"M{},{}H{}\"{attrs}/>{}{}",
                    num(from),
                    num(line_y),
                    num(to),
                    heads,
                    inner
                );
                if label.width > 0.0 {
                    let l = x0.min(x1);
                    let r = x0.max(x1);
                    let cx = if align < 0 {
                        l + msg_pad + label.width / 2.0
                    } else if align > 0 {
                        r - msg_pad - label.width / 2.0
                    } else {
                        (l + r) / 2.0
                    };
                    extend_bounds(&mut min_x, &mut max_x, cx - label.width / 2.0, cx + label.width / 2.0);
                    labels_out.push_str(&format!(
                        "<g class=\"pele-edge-label\" data-id=\"i{}\">{}</g>",
                        esc_text(&m.id),
                        backed(
                            label,
                            cx,
                            line_y - 4.0 - label.height / 2.0,
                            false,
                            &stack,
                            &mut ink_of,
                        )
                    ));
                }
                if creating {
                    y = line_y + b_h / 2.0 - 6.0;
                }
            }
            let stroke_attr = match line_color {
                Some(lc) => format!(" stroke=\"{}\"", lc),
                None => String::new(),
            };
            edges_out.push_str(&format!(
                "<g class=\"pele-edge pele-message\" data-id=\"i{}\"{}>{}</g>",
                esc_text(&m.id),
                stroke_attr,
                inner
            ));
            y = y.max(line_y);
            if cols[va].destroy_at == i as isize {
                let cx = cols[va].x;
                let cy = line_y + 16.0;
                cols[va].life_end = cy;
                crosses_out.push_str(&format!(
                    "<path class=\"pele-destroy\" data-id=\"{}\" d=\"M{},{}L{},{}M{},{}L{},{}\"/>",
                    esc_text(cols[va].id),
                    num(cx - 6.0),
                    num(cy - 6.0),
                    num(cx + 6.0),
                    num(cy + 6.0),
                    num(cx + 6.0),
                    num(cy - 6.0),
                    num(cx - 6.0),
                    num(cy + 6.0)
                ));
                y = y.max(cy + 6.0);
            }
            if vb != va && cols[vb].destroy_at == i as isize {
                let cx = cols[vb].x;
                let cy = line_y + 16.0;
                cols[vb].life_end = cy;
                crosses_out.push_str(&format!(
                    "<path class=\"pele-destroy\" data-id=\"{}\" d=\"M{},{}L{},{}M{},{}L{},{}\"/>",
                    esc_text(cols[vb].id),
                    num(cx - 6.0),
                    num(cy - 6.0),
                    num(cx + 6.0),
                    num(cy + 6.0),
                    num(cx + 6.0),
                    num(cy - 6.0),
                    num(cx - 6.0),
                    num(cy + 6.0)
                ));
                y = y.max(cy + 6.0);
            }
        }
    }

    y += 20.0;
    for col in &mut cols {
        while !col.activations.is_empty() {
            end_activation(col, y, &mut activations_out, &mut min_x, &mut max_x);
        }
    }

    let actor_svg = |col: &Column,
                     top: f64,
                     footer: bool,
                     ink_of: &mut dyn FnMut(Option<&str>) -> Option<&'static str>|
     -> String {
        let cls = col
            .actor
            .properties
            .get("class")
            .and_then(|v| v.as_str())
            .map(class_names)
            .unwrap_or_default();
        let fig_ink = if is_figure(col.kind) {
            let box_fill = col.actor.box_idx.map(|bi| db.boxes[bi].fill.as_str());
            ink_on(ink_of, box_fill)
        } else {
            String::new()
        };
        format!(
            "<g class=\"pele-node pele-actor pele-actor-{}{}{}\" data-id=\"{}\">{}</g>",
            col.kind,
            if footer { " pele-actor-footer" } else { "" },
            cls,
            esc_text(col.id),
            draw_actor(col.kind, col.x, top, col.size, &col.label, &fig_ink)
        )
    };

    let mut bottom = y;
    if mirror {
        let mut foot_h = 0.0f64;
        for col in &cols {
            if col.life_end >= 0.0 {
                continue;
            }
            foot_h = foot_h.max(col.size.h);
            actors_out.push_str(&actor_svg(col, y, true, &mut ink_of));
        }
        bottom = y + foot_h;
    }

    let mut lifelines = String::new();
    for col in &cols {
        actors_out.push_str(&actor_svg(col, col.top, false, &mut ink_of));
        let end = if col.life_end >= 0.0 { col.life_end } else { y };
        if end > col.life_start {
            lifelines.push_str(&format!(
                "<path class=\"pele-lifeline\" data-id=\"{}\" d=\"M{},{}V{}\"/>",
                esc_text(col.id),
                num(col.x),
                num(col.life_start),
                num(end)
            ));
        }
    }

    let mut boxes_out = String::new();
    for run in &runs {
        let (x0, x1) = box_edges(&cols, run);
        let b = &db.boxes[run.box_idx];
        let fill = if b.fill == "transparent" {
            String::new()
        } else {
            resolve_style(&[format!("fill:{}", b.fill)]).shape
        };
        let rect_in = format!(
            "{}{}",
            if fill.is_empty() {
                " fill-opacity=\"0.5\""
            } else {
                ""
            },
            fill
        );
        let lbl_ink = ink_on(&mut ink_of, Some(&b.fill));
        let lbl_attrs = format!(
            " class=\"pele-cluster-label\"{}",
            if lbl_ink.is_empty() {
                " fill=\"var(--_m)\""
            } else {
                &lbl_ink
            }
        );
        let data_id = match b.name {
            Some(ref n) if !n.is_empty() => format!(" data-id=\"{}\"", esc_text(n)),
            _ => String::new(),
        };
        boxes_out.push_str(&format!(
            "<g class=\"pele-cluster pele-box\"{data_id}><rect x=\"{}\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\" stroke=\"var(--_b)\"{}/>{}</g>",
            num(x0),
            num(x1 - x0),
            num(bottom + box_pad),
            rounded(&rect_in),
            label_svg(
                &run.title,
                (x0 + x1) / 2.0,
                box_pad / 2.0 + 2.0 + run.title.height / 2.0,
                &lbl_attrs
            )
        ));
    }
    if !runs.is_empty() {
        bottom += box_pad;
    }

    let mut min_y = 0.0f64;
    let mut title_out = String::new();
    let room = title_room(options);
    let plain_title = title_label(
        db.title.as_deref(),
        measurer,
        size,
        if tight { room } else { f64::INFINITY },
    );
    let title = if tight {
        break_label(&plain_title, measurer, (120.0f64).max(room))
    } else {
        plain_title
    };
    if min_x > max_x {
        if 0.0 < min_x {
            min_x = 0.0;
        }
        let w = title.width.max(40.0);
        if w > max_x {
            max_x = w;
        }
    }
    if title.width > 0.0 {
        let cx = (min_x + max_x) / 2.0;
        if cx - title.width / 2.0 < min_x {
            min_x = cx - title.width / 2.0;
        }
        if cx + title.width / 2.0 > max_x {
            max_x = cx + title.width / 2.0;
        }
        min_y = -title.height - 12.0;
        title_out = label_svg(
            &title,
            cx,
            min_y + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\"",
        );
    }

    let width = (max_x - min_x + 2.0 * pad).ceil();
    // Retry with equal shares or room for activation notes and rounding.
    if grown {
        if let Some(max_width_opt) = options.max_width {
            if width > max_width_opt {
                let next_hold = if uneven {
                    hold
                } else if hold > 0.0 {
                    f64::INFINITY
                } else {
                    width - max_width_opt.floor()
                };
                return draw(db, config, options, tight, next_hold, false);
            }
        }
    }
    let height = (bottom - min_y + 2.0 * pad).ceil();

    let small_str = js_number(small);
    let rects_svg = if rects_out.is_empty() {
        String::new()
    } else {
        let joined: String = rects_out.iter().rev().map(|s| s.as_str()).collect();
        format!("<g class=\"pele-rects\" fill=\"var(--_a)\">{joined}</g>")
    };
    let activations_svg = if activations_out.is_empty() {
        String::new()
    } else {
        let joined: String = activations_out.iter().rev().map(|s| s.as_str()).collect();
        format!(
            "<g class=\"pele-activations\" fill=\"var(--_s)\" stroke=\"var(--_b)\">{joined}</g>"
        )
    };
    let content = format!(
        "<g transform=\"translate({},{})\">{}{}{}{}{}{}{}{}{}{}{}</g>",
        num(pad - min_x),
        num(pad - min_y),
        title_out,
        if boxes_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-boxes\" font-size=\"{small_str}\">{boxes_out}</g>")
        },
        rects_svg,
        if lifelines.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-lifelines\" fill=\"none\" stroke=\"var(--_b)\">{lifelines}</g>")
        },
        activations_svg,
        if frames_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-frames\" font-size=\"{small_str}\" stroke=\"var(--_b)\">{frames_out}</g>"
            )
        },
        if edges_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-edges\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\">{edges_out}</g>"
            )
        },
        if crosses_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-destroys\" fill=\"none\" stroke=\"var(--_l)\" stroke-width=\"1.5\" stroke-linecap=\"round\">{crosses_out}</g>"
            )
        },
        if labels_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-edge-labels\" font-size=\"{small_str}\">{labels_out}</g>")
        },
        if notes_out.is_empty() {
            String::new()
        } else {
            format!(
                "<g class=\"pele-notes\" font-size=\"{small_str}\" fill=\"var(--_a)\" stroke=\"var(--_b)\">{notes_out}</g>"
            )
        },
        if actors_out.is_empty() {
            String::new()
        } else {
            format!("<g class=\"pele-nodes\">{actors_out}</g>")
        },
    );

    let fixed = conf.get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "sequence",
        width,
        height,
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
        &content,
    );
    Rendered {
        svg,
        width,
        height,
        links: Vec::new(),
    }
}
