use std::collections::HashMap;

use crate::diagrams::common::fit::plain_label;
use crate::options::{turn_to_fit, RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::palette::palette;
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{series_color, ROUNDED};
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

use super::db::{commit_type, GitCommit, GitModel};
use super::parser::GitDirection;

const DOT: f64 = 7.0;
const STEP: f64 = 44.0;
const LANE: f64 = 44.0;
// Shorter than a step, so a curve between lanes never runs through a commit on a lane it crosses.
const CURVE: f64 = 36.0;
const GAP: f64 = 8.0;
const LABEL_GAP: f64 = 20.0;
const TAIL: f64 = 20.0;
const NOTCH: f64 = 7.0;
const TAG_GAP: f64 = 6.0;
const HALF: f64 = std::f64::consts::FRAC_1_SQRT_2;
const CLASSES: [&str; 5] = ["normal", "reverse", "highlight", "merge", "cherry-pick"];

struct View {
    commit: GitCommit,
    index: usize,
    lane: usize,
    m: f64,
    label: Option<Label>,
    tags: Vec<Label>,
}

fn flag(config: &Value, key: &str, fallback: bool) -> bool {
    config.get("gitGraph").get(key).as_bool().unwrap_or(fallback)
}

fn tag_width(tag: &Label) -> f64 {
    NOTCH + 4.0 + tag.width + 6.0
}

fn symbol(kind: usize, x: f64, y: f64, color: &str) -> String {
    let at = format!("cx=\"{}\" cy=\"{}\"", num(x), num(y));
    if kind == commit_type::HIGHLIGHT {
        return format!(
            "<rect x=\"{}\" y=\"{}\" width=\"16\" height=\"16\" rx=\"2\" fill=\"{}\"/>",
            num(x - DOT - 1.0),
            num(y - DOT - 1.0),
            color
        );
    }
    if kind == commit_type::MERGE {
        return format!(
            "<circle {} r=\"6\" fill=\"var(--_bg)\" stroke=\"{}\" stroke-width=\"2\"/><circle {} r=\"2.5\" fill=\"{}\"/>",
            at, color, at, color
        );
    }
    let dot = format!("<circle {} r=\"7\" fill=\"{}\"/>", at, color);
    if kind == commit_type::REVERSE {
        return format!(
            "{}<path d=\"M{},{}l6,6M{},{}l6,-6\" fill=\"none\" stroke=\"var(--_bg)\" stroke-width=\"1.5\" stroke-linecap=\"round\"/>",
            dot,
            num(x - 3.0),
            num(y - 3.0),
            num(x - 3.0),
            num(y + 3.0)
        );
    }
    if kind == commit_type::CHERRY_PICK {
        return format!(
            "{}<path d=\"M{},{}L{},{}L{},{}\" fill=\"none\" stroke=\"var(--_bg)\" stroke-linejoin=\"round\"/><circle cx=\"{}\" cy=\"{}\" r=\"1.8\" fill=\"var(--_bg)\"/><circle cx=\"{}\" cy=\"{}\" r=\"1.8\" fill=\"var(--_bg)\"/>",
            dot,
            num(x - 2.6),
            num(y + 1.0),
            num(x + 0.8),
            num(y - 4.2),
            num(x + 2.6),
            num(y + 1.0),
            num(x - 2.6),
            num(y + 2.2),
            num(x + 2.6),
            num(y + 2.2),
        );
    }
    dot
}

pub fn render_git(model: &GitModel, config: &Value, options: &RenderOptions) -> Rendered {
    if model.direction != GitDirection::Lr {
        return draw(model, config, options, model.direction);
    }
    turn_to_fit(options, &|down| {
        draw(
            model,
            config,
            options,
            if down { GitDirection::Tb } else { GitDirection::Lr },
        )
    })
}

fn draw(model: &GitModel, config: &Value, options: &RenderOptions, direction: GitDirection) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.75);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let pal = palette("gitGraph", config);

    let show_branches = flag(config, "showBranches", true);
    let show_commit_label = flag(config, "showCommitLabel", true);
    let rotate = flag(config, "rotateCommitLabel", true);
    let parallel = flag(config, "parallelCommits", false);
    let vertical = direction != GitDirection::Lr;
    let flip = direction == GitDirection::Bt;

    let mut lane_of: HashMap<String, usize> = HashMap::with_capacity(model.lanes.len());
    for (index, name) in model.lanes.iter().enumerate() {
        lane_of.insert(name.clone(), index);
    }
    let lane_count = model.lanes.len();
    let medium = js_round(size * 0.875);
    // Mermaid writes branch names, tags, and commit ids as plain text, so tags in them show as written.
    let names: Vec<Label> = model
        .lanes
        .iter()
        .map(|name| plain_label(Some(if show_branches { name.as_str() } else { "" }), measurer, medium, 4000.0, 0))
        .collect();

    let mut sorted_commits: Vec<GitCommit> = model.commits.values().cloned().collect();
    sorted_commits.sort_by_key(|c| c.seq);

    let mut views: Vec<View> = Vec::with_capacity(sorted_commits.len());
    let mut by_id: HashMap<String, usize> = HashMap::with_capacity(sorted_commits.len());
    for commit in sorted_commits {
        let labelled = show_commit_label
            && commit.commit_type != commit_type::CHERRY_PICK
            && (commit.commit_type != commit_type::MERGE || commit.custom_id);
        let mut tags: Vec<Label> = Vec::new();
        for tag in &commit.tags {
            let label = plain_label(Some(tag.as_str()), measurer, small, 4000.0, 0);
            if !label.lines.is_empty() {
                tags.push(label);
            }
        }
        let index = views.len();
        let lane = lane_of.get(&commit.branch).copied().unwrap_or(0);
        let label = if labelled {
            Some(plain_label(Some(&commit.id), measurer, small, 4000.0, 0))
        } else {
            None
        };
        by_id.insert(commit.id.clone(), index);
        views.push(View {
            commit,
            index,
            lane,
            m: 0.0,
            label,
            tags,
        });
    }

    // Room each commit needs: along the main axis on either side, and across it towards the
    // previous lane (`lo`) and the next one (`hi`). Tags sit on one side of a lane and ids on the other.
    let mut half = vec![0.0f64; views.len()];
    let mut tag_half = vec![0.0f64; views.len()];
    let mut lo = vec![DOT; lane_count];
    let mut hi = vec![DOT; lane_count];
    for (k, view) in views.iter().enumerate() {
        let mut tags_main = 0.0f64;
        let mut tags_cross = 0.0f64;
        for tag in &view.tags {
            let w = tag_width(tag);
            if vertical {
                tags_main = tags_main.max(tag.height);
                tags_cross += w + 4.0;
            } else {
                tags_main = tags_main.max(w);
                tags_cross += tag.height + 3.0;
            }
        }
        let mut label_main = 0.0f64;
        let mut label_cross = 0.0f64;
        if let Some(ref label) = view.label {
            if !label.lines.is_empty() {
                if rotate {
                    label_cross = 3.0 + HALF * (label.width + label.height);
                } else {
                    label_main = if vertical { label.height } else { label.width };
                    label_cross = 6.0 + if vertical { label.width } else { label.height };
                }
            }
        }
        half[k] = DOT.max(tags_main / 2.0).max(label_main / 2.0);
        tag_half[k] = tags_main / 2.0;
        let tag_side = DOT + if !view.tags.is_empty() { TAG_GAP + tags_cross } else { 0.0 };
        let label_side = DOT + label_cross;
        let lane = view.lane;
        if lane < lane_count {
            lo[lane] = lo[lane].max(if vertical { label_side } else { tag_side });
            hi[lane] = hi[lane].max(if vertical { tag_side } else { label_side });
        }
    }

    // In a left-to-right graph the tags stand over their commit. A connector that comes down from
    // a lane above joins the commit's lane this far before the commit, clear of the tags, and the
    // commit is placed far enough along to leave room for that.
    let mut lead = vec![0.0f64; views.len()];
    if !vertical {
        for (k, view) in views.iter().enumerate() {
            if tag_half[k] == 0.0 {
                continue;
            }
            for id in &view.commit.parents {
                if let Some(&p_idx) = by_id.get(id) {
                    if views[p_idx].lane < view.lane {
                        lead[k] = tag_half[k] + 4.0;
                    }
                }
            }
        }
    }
    let space = |p: usize, k: usize| -> f64 {
        STEP.max(half[p] + half[k] + GAP)
            .max(if lead[k] > 0.0 { half[p] + CURVE + lead[k] } else { 0.0 })
    };

    for k in 0..views.len() {
        if !parallel {
            views[k].m = if k == 0 { 0.0 } else { views[k - 1].m + space(k - 1, k) };
            continue;
        }
        let mut m = 0.0f64;
        for id in &views[k].commit.parents {
            if let Some(&p_idx) = by_id.get(id) {
                if p_idx < k {
                    m = m.max(views[p_idx].m + space(p_idx, k));
                }
            }
        }
        views[k].m = m;
    }

    let mut end = 0.0f64;
    for view in &views {
        end = end.max(view.m);
    }

    let mut cross = vec![0.0f64; lane_count];
    for i in 0..lane_count {
        let name = &names[i];
        let reach = if name.lines.is_empty() {
            0.0
        } else if vertical {
            name.width / 2.0 + 8.0
        } else {
            name.height / 2.0 + 2.0
        };
        lo[i] = lo[i].max(reach);
        hi[i] = hi[i].max(reach);
        if i > 0 {
            cross[i] = cross[i - 1] + LANE.max(hi[i - 1] + lo[i] + GAP);
        }
    }

    let px = |m: f64, c: f64| -> f64 { if vertical { c } else { m } };
    let py = |m: f64, c: f64| -> f64 {
        if vertical {
            if flip { -m } else { m }
        } else {
            c
        }
    };
    let point = |m: f64, c: f64| -> String { format!("{},{}", num(px(m, c)), num(py(m, c))) };

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut grow = |x0: f64, y0: f64, x1: f64, y1: f64| {
        if x0 < min_x {
            min_x = x0;
        }
        if y0 < min_y {
            min_y = y0;
        }
        if x1 > max_x {
            max_x = x1;
        }
        if y1 > max_y {
            max_y = y1;
        }
    };

    let mut branches_svg = String::new();
    if show_branches {
        for i in 0..lane_count {
            let name = &names[i];
            let color = series_color(i as i64);
            let c = cross[i];
            let w = name.width + 16.0;
            let h = name.height + 4.0;
            let from = if name.lines.is_empty() {
                -LABEL_GAP
            } else {
                -LABEL_GAP - if vertical { h } else { w }
            };
            let span = if vertical { h } else { w };
            let cx = px(from + span / 2.0, c);
            let cy = py(from + span / 2.0, c);
            grow(px(-LABEL_GAP, c), py(-LABEL_GAP, c), px(-LABEL_GAP, c), py(-LABEL_GAP, c));
            grow(px(end + TAIL, c), py(end + TAIL, c), px(end + TAIL, c), py(end + TAIL, c));
            branches_svg.push_str(&format!(
                "<g class=\"pele-branch\" data-id=\"{}\"><path class=\"pele-lane\" d=\"M{}L{}\" stroke=\"{}\" stroke-dasharray=\"2 4\"/>",
                esc_text(&model.lanes[i]),
                point(-LABEL_GAP, c),
                point(end + TAIL, c),
                color,
            ));
            if !name.lines.is_empty() {
                grow(cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0);
                let ink = pal.ink_fill(i as isize);
                let fill_attr = if ink.is_empty() { " fill=\"var(--_bg)\"" } else { ink.as_str() };
                branches_svg.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{}/>{}",
                    num(cx - w / 2.0),
                    num(cy - h / 2.0),
                    num(w),
                    num(h),
                    color,
                    ROUNDED,
                    label_svg(name, cx, cy, &format!(" class=\"pele-label\"{}", fill_attr)),
                ));
            }
            branches_svg.push_str("</g>");
        }
    }

    // Positions of the commits on each lane, to tell whether a connector may run along it.
    let mut taken: Vec<Vec<f64>> = vec![Vec::new(); lane_count];
    for view in &views {
        if view.lane < lane_count {
            taken[view.lane].push(view.m);
        }
    }
    if parallel {
        for list in &mut taken {
            list.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        }
    }
    let busy = |lane: usize, from: f64, to: f64| -> bool {
        let list = &taken[lane];
        let mut low = 0usize;
        let mut high = list.len();
        while low < high {
            let mid = (low + high) >> 1;
            if list[mid] <= from {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        low < list.len() && list[low] < to
    };

    let mut curves_svg = String::new();
    let mut lines_svg = String::new();
    for view in &views {
        let c2 = cross[view.lane];
        let m2 = view.m;
        for (j, parent_id) in view.commit.parents.iter().enumerate() {
            let Some(&p_idx) = by_id.get(parent_id) else {
                continue;
            };
            if p_idx == view.index {
                continue;
            }
            let parent = &views[p_idx];
            let c1 = cross[parent.lane];
            let m1 = parent.m;
            if parent.lane == view.lane {
                lines_svg.push_str(&format!(
                    "<path class=\"pele-edge\" d=\"M{}L{}\" stroke=\"{}\"/>",
                    point(m1, c1),
                    point(m2, c2),
                    series_color(view.lane as i64),
                ));
                continue;
            }
            let merged = j > 0 && view.commit.commit_type == commit_type::MERGE;
            let picked = j > 0 && !merged;
            let mut d = format!("M{}", point(m1, c1));
            if m2 <= m1 {
                let mid = (m1 + m2) / 2.0;
                d.push_str(&format!(
                    "C{} {} {}",
                    point(mid, c1),
                    point(mid, c2),
                    point(m2, c2)
                ));
            } else if !merged && !busy(view.lane, m1, m2) {
                // Leaves the parent at once and runs along the child's lane.
                let s = CURVE.min(m2 - m1);
                d.push_str(&format!(
                    "C{} {} {}L{}",
                    point(m1 + s / 2.0, c1),
                    point(m1 + s / 2.0, c2),
                    point(m1 + s, c2),
                    point(m2, c2),
                ));
            } else {
                // Turns in just before the child, or before its tags when it comes from their side.
                let to = if parent.lane < view.lane && m2 - lead[view.index] > m1 {
                    m2 - lead[view.index]
                } else {
                    m2
                };
                let mut s = CURVE.min(to - m1);
                let mut along = c1;
                if !merged || busy(parent.lane, m1, m2) {
                    // Neither lane is free to run along. Between two lanes, past the ids of one and before
                    // the tags of the next, a strip stays empty, and the connector takes that.
                    along = if c1 > c2 {
                        c2 + hi[view.lane] + GAP / 2.0
                    } else {
                        c2 - lo[view.lane] - GAP / 2.0
                    };
                    s = CURVE.min((to - m1) / 2.0);
                    d.push_str(&format!(
                        "C{} {} {}",
                        point(m1 + s / 2.0, c1),
                        point(m1 + s / 2.0, along),
                        point(m1 + s, along),
                    ));
                }
                d.push_str(&format!(
                    "L{}C{} {} {}",
                    point(to - s, along),
                    point(to - s / 2.0, along),
                    point(to - s / 2.0, c2),
                    point(to, c2),
                ));
                if to < m2 {
                    d.push_str(&format!("L{}", point(m2, c2)));
                }
            }
            let extra_class = if merged {
                " pele-edge-merge"
            } else if picked {
                " pele-edge-cherry-pick"
            } else {
                ""
            };
            let stroke_lane = if merged { parent.lane } else { view.lane };
            let dash = if picked { " stroke-dasharray=\"4 3\"" } else { "" };
            curves_svg.push_str(&format!(
                "<path class=\"pele-edge{}\" d=\"{}\" stroke=\"{}\"{}/>",
                extra_class,
                d,
                series_color(stroke_lane as i64),
                dash,
            ));
        }
    }

    let mut commits_svg = String::new();
    for view in &views {
        let commit = &view.commit;
        let x = px(view.m, cross[view.lane]);
        let y = py(view.m, cross[view.lane]);
        let kind = commit.custom_type.unwrap_or(commit.commit_type);
        grow(x - DOT - 1.0, y - DOT - 1.0, x + DOT + 1.0, y + DOT + 1.0);
        // Merges and cherry-picks carry a message Mermaid makes up; only a written one is worth showing.
        let written = commit.commit_type < commit_type::MERGE && !commit.message.is_empty();
        let mut body = if written {
            format!("<title>{}</title>", esc_text(&commit.message))
        } else {
            String::new()
        };
        body.push_str(&symbol(kind, x, y, &series_color(view.lane as i64)));

        if let Some(ref label) = view.label {
            if !label.lines.is_empty() {
                let w = label.width;
                let h = label.height;
                let back = |cx: f64, cy: f64| -> String {
                    format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" fill=\"var(--_bg)\"/>",
                        num(cx - w / 2.0 - 2.0),
                        num(cy - h / 2.0 + 2.0),
                        num(w + 4.0),
                        num(h - 4.0),
                    )
                };
                if rotate {
                    // The text ends beside the commit and trails back the way the history came.
                    let k = (HALF * h) / 2.0;
                    let ax = if vertical { x - DOT - 3.0 - k } else { x + 3.0 };
                    let ay = if vertical { y } else { y + DOT + 3.0 + k };
                    let reach = HALF * w + k;
                    grow(
                        ax - reach,
                        if flip { ay - reach } else { ay - k },
                        ax + k,
                        if flip { ay + k } else { ay + reach },
                    );
                    let angle = if flip { 45 } else { -45 };
                    body.push_str(&format!(
                        "<g class=\"pele-commit-label\" transform=\"rotate({} {} {})\">{}{}</g>",
                        angle,
                        num(ax),
                        num(ay),
                        back(ax - w / 2.0, ay),
                        label_svg(label, ax - w / 2.0, ay, ""),
                    ));
                } else {
                    let cx = if vertical { x - DOT - 6.0 - w / 2.0 } else { x };
                    let cy = if vertical { y } else { y + DOT + 6.0 + h / 2.0 };
                    grow(cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0);
                    body.push_str(&format!(
                        "<g class=\"pele-commit-label\">{}{}</g>",
                        back(cx, cy),
                        label_svg(label, cx, cy, ""),
                    ));
                }
            }
        }

        // Beside the lane the tags form a row; above it they stack, the first one furthest out.
        let mut offset = DOT + TAG_GAP;
        let indices: Vec<usize> = if vertical {
            (0..view.tags.len()).collect()
        } else {
            (0..view.tags.len()).rev().collect()
        };
        for t in indices {
            let tag = &view.tags[t];
            let w = tag_width(tag);
            let h = tag.height;
            let left = if vertical { x + offset } else { x - w / 2.0 };
            let cy = if vertical { y } else { y - offset - h / 2.0 };
            offset += if vertical { w + 4.0 } else { h + 3.0 };
            grow(left, cy - h / 2.0, left + w, cy + h / 2.0);
            body.push_str(&format!(
                "<g class=\"pele-tag\"><path d=\"M{},{}L{},{}H{}V{}H{}Z\" fill=\"var(--_s)\" stroke=\"var(--_b)\" stroke-linejoin=\"round\"/><circle cx=\"{}\" cy=\"{}\" r=\"1.5\" fill=\"var(--_b)\"/>{}</g>",
                num(left),
                num(cy),
                num(left + NOTCH),
                num(cy - h / 2.0),
                num(left + w),
                num(cy + h / 2.0),
                num(left + NOTCH),
                num(left + NOTCH),
                num(cy),
                label_svg(tag, left + NOTCH + 4.0 + tag.width / 2.0, cy, " fill=\"var(--_fg)\""),
            ));
        }

        let merge_cls = if commit.commit_type == commit_type::MERGE && kind != commit_type::MERGE {
            " pele-commit-merge"
        } else {
            ""
        };
        let cls_name = CLASSES.get(kind).copied().unwrap_or(CLASSES[0]);
        commits_svg.push_str(&format!(
            "<g class=\"pele-commit pele-commit-{}{}\" data-id=\"{}\">{}</g>",
            cls_name,
            merge_cls,
            esc_text(&commit.id),
            body,
        ));
    }

    if min_x > max_x {
        min_x = 0.0;
        min_y = 0.0;
        max_x = 0.0;
        max_y = 0.0;
    }
    let title = layout_label(model.title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let inner = (max_x - min_x).max(title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let dx = pad - min_x + (inner - (max_x - min_x)) / 2.0;
    let dy = pad + title_height - min_y;

    let branches_group = if !branches_svg.is_empty() {
        format!("<g class=\"pele-branches\" fill=\"none\" font-size=\"{}\">{}</g>", js_number(medium), branches_svg)
    } else {
        String::new()
    };
    let edges_group = if !curves_svg.is_empty() || !lines_svg.is_empty() {
        format!(
            "<g class=\"pele-edges\" fill=\"none\" stroke-width=\"2\" stroke-linecap=\"round\">{}{}</g>",
            curves_svg, lines_svg
        )
    } else {
        String::new()
    };
    let commits_group = if !commits_svg.is_empty() {
        format!(
            "<g class=\"pele-commits\" font-size=\"{}\" fill=\"var(--_m)\">{}</g>",
            js_number(small),
            commits_svg
        )
    } else {
        String::new()
    };

    let content = format!(
        "{}<g transform=\"translate({},{})\">{}{}{}</g>",
        label_svg(&title, width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\""),
        num(dx),
        num(dy),
        branches_group,
        edges_group,
        commits_group,
    );

    let fixed = config.get("gitGraph").get("useMaxWidth").as_bool() == Some(false);
    let responsive = options.responsive.unwrap_or(!fixed);
    let doc = Document {
        id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
        responsive,
        font_family: options.font_family.as_deref(),
        font_family_mono: options.font_family_mono.as_deref(),
        palette: &pal.style,
    };
    let acc = Accessible {
        acc_title: model.acc_title.as_deref(),
        acc_descr: model.acc_descr.as_deref(),
    };
    let svg = svg_document("gitGraph", width, height, size, &doc, &acc, &content);

    Rendered { svg, width, height, links: Vec::new() }
}
