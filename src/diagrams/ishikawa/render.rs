use super::db::IshikawaDb;
use super::types::IshikawaNode;
use crate::diagrams::common::fit_width::{title_room, turn_to_fit};
use crate::diagrams::common::literal_label::break_label;
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::edges::{marker, ARROW};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::ROUNDED;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;

// How far a bone moves toward the tail for each unit it rises from the spine.
const SLANT: f64 = 0.5;
const FIRST_ROW: f64 = 14.0;
const ROW_GAP: f64 = 6.0;
const BONE_END: f64 = 8.0;
const TWIG_STEP: f64 = 22.0;
const STUB: f64 = 18.0;
const TEXT_GAP: f64 = 5.0;
const PAIR_GAP: f64 = 26.0;
const HEAD_GAP: f64 = 34.0;
const TAIL: f64 = 28.0;
const BOX_PAD_X: f64 = 10.0;
const BOX_PAD_Y: f64 = 5.0;

// A cause below a category. Bones at even depths run level, from the slanted bone above them
// toward the tail; bones at odd depths slant away from the spine, from the level bone above them.
struct Bone {
    label: Label,
    depth: usize,
    parent: isize,
    // Its place among its siblings, counted from the spine outward.
    slot: usize,
    kids: usize,
    // How far its row is from the spine.
    rise: f64,
    // Where it meets its parent, measured level from the category's bone: 0 on it, negative toward the tail.
    at: f64,
    // Where its text ends.
    text_end: f64,
}

struct Category<'a> {
    node: &'a IshikawaNode,
    label: Label,
    box_w: f64,
    box_h: f64,
    bones: Vec<Bone>,
    // The length of its bone, as distance from the spine.
    rise: f64,
    // How far its causes reach toward the tail, measured level from its bone.
    reach: f64,
}

struct Frame {
    node_idx: usize,
    depth: usize,
    parent: isize,
    slot: usize,
    // Set when the bone was made and only its row is still to come.
    bone: isize,
}

// Gives every cause of a category a row of its own, so that no two texts can meet, and measures
// how far the whole of it reaches. A level bone takes the row nearest the spine and its twigs the
// rows beyond; a slanted twig takes the row beyond everything that hangs off it. The outline may
// be nested as deep as its indentation goes, so the walk keeps a stack of its own.
fn arrange<'a>(
    arena: &'a [IshikawaNode],
    cat_idx: usize,
    up: bool,
    measurer: &dyn TextMeasurer,
    size: f64,
    small: f64,
) -> Category<'a> {
    let node = &arena[cat_idx];
    let label = layout_label(Some(&node.text), false, measurer, size, 180.0, 0);
    let line_height = js_round(small * 1.5);
    let mut bones: Vec<Bone> = Vec::new();
    let mut rows: Vec<usize> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();

    // Above the spine the first cause goes farthest out, so that the causes read from the top down.
    let push_children =
        |stack: &mut Vec<Frame>, children: &[usize], depth: usize, parent: isize| {
            let n = children.len();
            for slot in (0..n).rev() {
                let child_idx = children[if up { n - 1 - slot } else { slot }];
                stack.push(Frame {
                    node_idx: child_idx,
                    depth,
                    parent,
                    slot,
                    bone: -1,
                });
            }
        };

    push_children(&mut stack, &node.children, 2, -1);
    while let Some(mut frame) = stack.pop() {
        if frame.bone != -1 {
            rows.push(frame.bone as usize);
            continue;
        }
        let index = bones.len();
        let fnode = &arena[frame.node_idx];
        bones.push(Bone {
            label: layout_label(Some(&fnode.text), false, measurer, small, 200.0, 0),
            depth: frame.depth,
            parent: frame.parent,
            slot: frame.slot,
            kids: fnode.children.len(),
            rise: 0.0,
            at: 0.0,
            text_end: 0.0,
        });
        let next_depth = frame.depth + 1;
        if frame.depth % 2 == 0 {
            rows.push(index);
        } else {
            frame.bone = index as isize;
            stack.push(frame);
        }
        push_children(&mut stack, &fnode.children, next_depth, index as isize);
    }

    let mut rise = FIRST_ROW;
    for index in rows {
        let height = bones[index].label.height.max(line_height) + ROW_GAP;
        bones[index].rise = rise + height / 2.0;
        rise += height;
    }
    let box_w = label.width + 2.0 * BOX_PAD_X;
    let mut reach = box_w / 2.0;
    for b_idx in 0..bones.len() {
        let parent_info = if bones[b_idx].parent >= 0 {
            let p = &bones[bones[b_idx].parent as usize];
            Some((p.at, p.kids))
        } else {
            None
        };
        let bone = &mut bones[b_idx];
        if bone.depth % 2 == 0 {
            bone.at = parent_info.map_or(0.0, |(pat, _)| pat);
            bone.text_end = bone.at
                - if bone.kids > 0 {
                    (bone.kids + 1) as f64 * TWIG_STEP
                } else {
                    STUB
                }
                - TEXT_GAP;
        } else {
            // Twigs stand along their level bone, the one nearest the spine nearest the tail.
            let (pat, pkids) = parent_info.unwrap();
            bone.at = pat - (pkids - bone.slot) as f64 * TWIG_STEP;
            bone.text_end = bone.at - TEXT_GAP;
        }
        reach = reach.max(bone.label.width - bone.text_end);
    }
    let box_h = label.height + 2.0 * BOX_PAD_Y;
    Category {
        node,
        label,
        box_w,
        box_h,
        bones,
        rise: rise + BONE_END,
        reach,
    }
}

pub fn render_ishikawa(
    model: &IshikawaDb,
    config: &Value,
    options: &RenderOptions,
) -> Rendered {
    turn_to_fit(options, &|down| draw(model, config, options, down))
}

fn draw(model: &IshikawaDb, config: &Value, options: &RenderOptions, down: bool) -> Rendered {
    if down {
        return vertical_causes(model, config, options);
    }
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let length = SLANT.hypot(1.0);

    let title = layout_label(model.title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };

    let mut body = String::new();
    let mut min_x = 0.0_f64;
    let mut max_x = 0.0_f64;
    let mut min_y = 0.0_f64;
    let mut max_y = 0.0_f64;

    if let Some(root) = model.nodes.first() {
        let head = layout_label(Some(&root.text), false, measurer, size, 200.0, style::BOLD);
        let head_h = (head.height + 24.0).max(48.0);
        let head_w = head.width + 24.0 + head_h / 2.0;
        let mut categories: Vec<Category> = root
            .children
            .iter()
            .enumerate()
            .map(|(i, &cat_idx)| arrange(&model.nodes, cat_idx, i % 2 == 0, measurer, size, small))
            .collect();
        // Even a category without causes has a bone long enough to carry its name clear of the head.
        for category in &mut categories {
            category.rise = category.rise.max(head_h / 2.0 + 14.0);
        }

        // Categories meet the spine in pairs, one above and one below, each pair as far toward the
        // tail from the one before as the wider of the two needs.
        let mut xs: Vec<f64> = Vec::new();
        let mut x = -HEAD_GAP;
        let mut i = 0usize;
        while i < categories.len() {
            if i > 0 {
                let mut room = 0.0_f64;
                for side in 0..2 {
                    if let Some(before) = categories.get(i - 2 + side) {
                        let next_half = categories.get(i + side).map_or(0.0, |n| n.box_w / 2.0);
                        room = room.max(before.reach + next_half);
                    }
                }
                x -= room + PAIR_GAP;
            }
            xs.push(x);
            xs.push(x);
            i += 2;
        }
        let tail = if !categories.is_empty() {
            x - TAIL
        } else {
            -HEAD_GAP
        };

        max_x = head_w;
        min_x = tail;
        min_y = -head_h / 2.0;
        max_y = head_h / 2.0;
        let mut groups = String::new();
        let small_str = js_number(small);

        for (i, category) in categories.iter().enumerate() {
            let sign = if i % 2 == 0 { -1.0 } else { 1.0 };
            let ax = xs[i];
            let px = |at: f64, rise: f64| -> f64 { ax + at - rise * SLANT };
            let end_x = px(0.0, category.rise);
            let end_y = sign * category.rise;
            // The bone stops short of the spine to leave its arrowhead a clean point.
            let trim = ARROW - 1.0;
            let mut d = format!(
                "M{},{}L{},{}",
                num(ax - (SLANT / length) * trim),
                num((sign * trim) / length),
                num(end_x),
                num(end_y)
            );
            let mut texts = String::new();
            for bone in &category.bones {
                let y = sign * bone.rise;
                if bone.depth % 2 == 0 {
                    d.push_str(&format!(
                        "M{},{}H{}",
                        num(px(bone.at, bone.rise)),
                        num(y),
                        num(px(bone.text_end + TEXT_GAP, bone.rise))
                    ));
                } else {
                    let from = category.bones[bone.parent as usize].rise;
                    d.push_str(&format!(
                        "M{},{}L{},{}",
                        num(px(bone.at, from)),
                        num(sign * from),
                        num(px(bone.at, bone.rise)),
                        num(y)
                    ));
                }
                let right = px(bone.text_end, bone.rise);
                texts.push_str(&label_svg(
                    &bone.label,
                    right - bone.label.width / 2.0,
                    y,
                    if bone.depth > 2 { " fill=\"var(--_m)\"" } else { "" },
                ));
                min_x = min_x.min(right - bone.label.width);
            }
            let box_y = end_y + sign * (category.box_h / 2.0);
            min_x = min_x.min(end_x - category.box_w / 2.0);
            max_x = max_x.max(end_x + category.box_w / 2.0);
            min_y = min_y.min(box_y - category.box_h / 2.0);
            max_y = max_y.max(box_y + category.box_h / 2.0);

            groups.push_str(&format!(
                "<g class=\"pele-ishikawa-category\" data-id=\"{}\"><path class=\"pele-ishikawa-bones\" d=\"{}\" fill=\"none\" stroke=\"var(--_l)\"/>{}<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>{}{}</g>",
                esc_text(&category.node.text),
                d,
                marker("arrow_point", ax, 0.0, SLANT / length, -sign / length, "var(--_l)", "none"),
                num(end_x - category.box_w / 2.0),
                num(box_y - category.box_h / 2.0),
                num(category.box_w),
                num(category.box_h),
                ROUNDED,
                label_svg(&category.label, end_x, box_y, " class=\"pele-label\""),
                if !texts.is_empty() {
                    format!("<g class=\"pele-ishikawa-causes\" font-size=\"{}\">{}</g>", small_str, texts)
                } else {
                    String::new()
                }
            ));
        }

        let r = head_h / 2.0;
        body = format!(
            "<path class=\"pele-ishikawa-spine\" d=\"M{},0H{}\" fill=\"none\" stroke=\"var(--_l)\"/>{}{}<g class=\"pele-ishikawa-head\" data-id=\"{}\"><path d=\"M0,{}H{}A{},{} 0 0 1 {},{}H0Z\" fill=\"var(--_s)\" stroke=\"var(--_b)\" stroke-linejoin=\"round\"/>{}</g>",
            num(tail),
            num(-(ARROW - 1.0)),
            marker("arrow_point", 0.0, 0.0, 1.0, 0.0, "var(--_l)", "none"),
            groups,
            esc_text(&root.text),
            num(-r),
            num(head_w - r),
            num(r),
            num(r),
            num(head_w - r),
            num(r),
            label_svg(&head, 12.0 + head.width / 2.0, 0.0, " class=\"pele-label\" font-weight=\"var(--_hw)\"")
        );
    }

    let inner = (max_x - min_x).max(title.width);
    let width = (inner + 2.0 * pad).ceil();
    let height = (max_y - min_y + title_height + 2.0 * pad).ceil();
    let ox = pad + (inner - (max_x - min_x)) / 2.0 - min_x;
    let oy = pad + title_height - min_y;
    let mut content = label_svg(
        &title,
        width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    );
    if !body.is_empty() {
        content.push_str(&format!(
            "<g transform=\"translate({},{})\">{}</g>",
            num(ox),
            num(oy),
            body
        ));
    }

    let fixed = config.get("ishikawa").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "ishikawa",
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
        &Accessible::default(),
        &content,
    );
    Rendered {
        svg,
        width,
        height,
        links: Vec::new(),
    }
}

struct OrderEntry {
    node_idx: usize,
    depth: usize,
    parent: isize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn vertical_causes(model: &IshikawaDb, config: &Value, options: &RenderOptions) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let room = 140.0_f64.max(options.max_width.unwrap_or(320.0) - 2.0 * pad);
    let title = layout_label(
        model.title.as_deref(),
        false,
        measurer,
        size,
        title_room(options),
        style::BOLD,
    );

    let mut order: Vec<OrderEntry> = Vec::new();
    let mut pending: Vec<(usize, usize, isize)> = if !model.nodes.is_empty() {
        vec![(0, 0, -1)]
    } else {
        Vec::new()
    };
    let mut max_depth = 0usize;
    while let Some((node_idx, depth, parent)) = pending.pop() {
        let index = order.len();
        order.push(OrderEntry {
            node_idx,
            depth,
            parent,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
        max_depth = max_depth.max(depth);
        let children = &model.nodes[node_idx].children;
        for i in (0..children.len()).rev() {
            pending.push((children[i], depth + 1, index as isize));
        }
    }

    let indent = 20.0_f64.min(2.0_f64.max((room - 120.0) / 1usize.max(max_depth) as f64));
    let mut y = pad + if title.height > 0.0 { title.height + 12.0 } else { 0.0 };
    let mut width = title.width;
    let mut nodes_svg = String::new();
    let mut edges_svg = String::new();
    let small_str = js_number(small);

    for idx in 0..order.len() {
        let depth = order[idx].depth;
        let node_text = &model.nodes[order[idx].node_idx].text;
        let ex = pad + depth as f64 * indent;
        let label_room = 60.0_f64.max(room - depth as f64 * indent - 20.0);
        let label = break_label(
            &layout_label(
                Some(node_text),
                false,
                measurer,
                if depth < 2 { size } else { small },
                label_room,
                if depth < 2 { style::BOLD } else { 0 },
            ),
            measurer,
            label_room,
        );
        let ew = label.width + 20.0;
        let eh = label.height + 12.0;
        let ey = y;
        order[idx].x = ex;
        order[idx].y = ey;
        order[idx].w = ew;
        order[idx].h = eh;
        y += eh + 12.0;
        width = width.max(ex - pad + ew);

        let framed = depth < 2;
        let kind_class = match depth {
            0 => "pele-ishikawa-head",
            1 => "pele-ishikawa-category",
            _ => "pele-ishikawa-cause",
        };
        let frame_rect = if framed {
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"/>",
                num(ex),
                num(ey),
                num(ew),
                num(eh),
                if depth == 0 { 12 } else { 4 }
            )
        } else {
            String::new()
        };
        let label_attrs = format!(
            " class=\"pele-label\"{}{}",
            if depth < 2 {
                " font-weight=\"var(--_hw)\""
            } else {
                ""
            },
            if depth > 1 {
                format!(" font-size=\"{}\"", small_str)
            } else {
                String::new()
            }
        );
        nodes_svg.push_str(&format!(
            "<g class=\"pele-node {}\" data-id=\"{}\">{}{}</g>",
            kind_class,
            esc_text(node_text),
            frame_rect,
            label_svg(&label, ex + ew / 2.0, ey + eh / 2.0, &label_attrs)
        ));

        let parent_idx = order[idx].parent;
        if parent_idx >= 0 {
            let p = &order[parent_idx as usize];
            let gx = p.x + indent / 2.0;
            edges_svg.push_str(&format!(
                "<path class=\"pele-edge\" d=\"M{},{}V{}H{}\" fill=\"none\" stroke=\"var(--_b)\"/>",
                num(gx),
                num(p.y + p.h),
                num(ey + eh / 2.0),
                num(ex)
            ));
        }
    }

    let total_width = (width + 2.0 * pad).ceil();
    let height = ((pad + title.height).max(y - 12.0) + pad).ceil();
    let content = format!(
        "{}{}{}",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        edges_svg,
        nodes_svg
    );

    let fixed = config.get("ishikawa").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "ishikawa",
        total_width,
        height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive: options.responsive.unwrap_or(!fixed),
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: "",
        },
        &Accessible::default(),
        &content,
    );
    Rendered {
        svg,
        width: total_width,
        height,
        links: Vec::new(),
    }
}
