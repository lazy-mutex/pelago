use std::collections::HashMap;

use crate::chars::is_space;
use crate::diagrams::common::fit_width::{tighten, title_room};
use crate::diagrams::common::literal_label::{left_label_svg, literal_label};
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_round, label_svg, num};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{class_names, ROUNDED};
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

use super::icons::{get_node_icon, glyph, icon_config, BUILTIN_PACK, FILE_GLYPH, FOLDER_GLYPH};
use super::model::{NodeType, TreeNode, TreeViewModel};

const ICON_GAP: f64 = 6.0;
const DESC_GAP: f64 = 16.0;
const GUIDE_INSET: f64 = 7.0;
const ELBOW: f64 = 4.0;
const HIGHLIGHT_OVERHANG: f64 = 4.0;

fn setting(config: &Value, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match config.get("treeView").get(key).as_f64() {
        Some(v) if v >= min && v <= max => v,
        _ => fallback,
    }
}

pub struct TreeSettings {
    pub padding_x: f64,
    pub padding_y: f64,
    pub row_indent: f64,
    pub line_thickness: f64,
}

pub fn tree_settings(config: &Value) -> TreeSettings {
    TreeSettings {
        padding_x: setting(config, "paddingX", 5.0, 0.0, 100.0),
        padding_y: setting(config, "paddingY", 2.0, 0.0, 100.0),
        row_indent: setting(config, "rowIndent", 15.0, 0.0, 400.0),
        line_thickness: setting(config, "lineThickness", 1.0, 0.25, 12.0),
    }
}

struct Row<'a> {
    node: &'a TreeNode,
    x: f64,
    cy: f64,
    icon: Option<String>,
    text: String,
    label_x: f64,
    highlighted: bool,
    label: Option<Label>,
    description: Option<Label>,
    top: f64,
    height: f64,
}

pub fn render_tree_view(model: &TreeViewModel, config: &Value, options: &RenderOptions) -> Rendered {
    tighten(options, &|tight| draw(model, config, options, tight))
}

fn draw(model: &TreeViewModel, config: &Value, options: &RenderOptions, tight: bool) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let icons = icon_config(config);

    let fixed_width = config.get("treeView").get("useMaxWidth").as_bool() == Some(false);
    let responsive = options.responsive.unwrap_or(!fixed_width);

    let TreeSettings {
        padding_x,
        padding_y,
        row_indent,
        line_thickness,
    } = tree_settings(config);

    let mut max_depth = 0usize;
    if tight {
        let mut depths: Vec<(&TreeNode, usize)> = model.root.children.iter().map(|n| (n, 0)).collect();
        let mut idx = 0usize;
        while idx < depths.len() {
            let (node, depth) = depths[idx];
            idx += 1;
            max_depth = max_depth.max(depth);
            for child in &node.children {
                depths.push((child, depth + 1));
            }
        }
    }

    let step = if tight {
        (row_indent + padding_x)
            .min(2.0_f64.max((options.max_width.unwrap_or(320.0) - 120.0) / (max_depth.max(1) as f64)))
    } else {
        row_indent + padding_x
    };
    let row_height = js_round(size * 1.5) + 2.0 * padding_y;
    let icon_size = size;

    let title = layout_label(
        model.title.as_deref(),
        false,
        measurer,
        size,
        if tight { title_room(options) } else { 4000.0 },
        style::BOLD,
    );
    let title_height = if title.height > 0.0 { title.height + 12.0 } else { 0.0 };

    let has_icon = |list: &[TreeNode]| -> bool {
        list.iter().any(|child| get_node_icon(child, &icons).is_some())
    };

    let mut rows: Vec<Row<'_>> = Vec::new();
    let mut row_of: HashMap<usize, usize> = HashMap::new();
    let mut pending: Vec<(&TreeNode, usize, bool)> = Vec::new();
    {
        let slot = has_icon(&model.root.children);
        for child in model.root.children.iter().rev() {
            pending.push((child, 0, slot));
        }
    }

    let mut label_right = 0.0f64;
    let mut any_highlight = false;
    let mut rows_height = 0.0f64;
    while let Some((node, depth, slot)) = pending.pop() {
        let x = depth as f64 * step;
        let text = decode_entities(&node.name);
        let label_x = x + if slot { icon_size + ICON_GAP } else { 0.0 };
        let room = 40.0_f64.max(options.max_width.unwrap_or(320.0) - 2.0 * pad - 2.0 * HIGHLIGHT_OVERHANG - label_x);
        let text_style = if node.node_type == NodeType::Directory { style::BOLD } else { 0 };
        let label = if tight {
            Some(literal_label(&text, measurer, size, room, text_style))
        } else {
            None
        };
        let description = match (&node.description, tight) {
            (Some(desc), true) => Some(literal_label(&decode_entities(desc), measurer, size, room, style::ITALIC)),
            _ => None,
        };
        let width = match &label {
            Some(l) => l.width,
            None => measurer.width(&text, size, text_style),
        };
        if label_x + width > label_right {
            label_right = label_x + width;
        }
        let highlighted = node
            .css_class
            .as_deref()
            .is_some_and(|cls| cls.split(is_space).any(|part| part == "highlight"));
        if highlighted {
            any_highlight = true;
        }
        let l_h = label.as_ref().map_or(0.0, |l| l.height);
        let d_h = description.as_ref().map_or(0.0, |d| d.height);
        let height = if tight {
            row_height.max(l_h + d_h + 2.0 * padding_y)
        } else {
            row_height
        };
        let cy = rows_height
            + if tight {
                padding_y + icon_size.max(l_h) / 2.0
            } else {
                height / 2.0
            };
        let row_idx = rows.len();
        row_of.insert(node.id, row_idx);
        rows.push(Row {
            node,
            x,
            cy,
            icon: get_node_icon(node, &icons),
            text,
            label_x,
            highlighted,
            label,
            description,
            top: rows_height,
            height,
        });
        rows_height += height;
        let child_slot = has_icon(&node.children);
        for child in node.children.iter().rev() {
            pending.push((child, depth + 1, child_slot));
        }
    }

    let desc_x = label_right + DESC_GAP;
    let mut width = label_right;
    for row in &rows {
        let Some(ref raw_desc) = row.node.description else { continue };
        let w = match &row.description {
            Some(d) => d.width,
            None => measurer.width(&decode_entities(raw_desc), size, style::ITALIC),
        };
        width = width.max((if tight { row.label_x } else { desc_x }) + w);
    }
    width = width.max(title.width);
    let inset = pad + if any_highlight { HIGHLIGHT_OVERHANG } else { 0.0 };
    let top = pad + title_height;
    let total_width = (width + 2.0 * inset).ceil();
    let total_height = (top + rows_height + pad).ceil();

    let mut guides = String::new();
    for row in &rows {
        let children = &row.node.children;
        if children.is_empty() {
            continue;
        }
        let gx = inset + row.x + GUIDE_INSET;
        let tick = gx.max(inset + row.x + step - padding_x);
        let last_child_row = &rows[row_of[&children.last().unwrap().id]];
        let last = top + last_child_row.cy;
        let r = ELBOW.min(tick - gx);
        let mut d = format!("M{},{}V{}", num(gx), num(top + row.top + row.height - 2.0), num(last - r));
        if r > 0.0 {
            d.push_str(&format!("Q{},{} {},{}H{}", num(gx), num(last), num(gx + r), num(last), num(tick)));
        }
        if tick > gx {
            for child in &children[..children.len() - 1] {
                let child_row = &rows[row_of[&child.id]];
                d.push_str(&format!("M{},{}H{}", num(gx), num(top + child_row.cy), num(tick)));
            }
        }
        guides.push_str(&format!("<path d=\"{}\"/>", d));
    }

    let baseline = size * 0.35;
    let mut body = String::new();
    let builtin_prefix = format!("{}:", BUILTIN_PACK);
    for row in &rows {
        let node = row.node;
        let cy = top + row.cy;
        let directory = node.node_type == NodeType::Directory;
        let mut inner = String::new();
        if row.highlighted {
            let x = inset + row.x - HIGHLIGHT_OVERHANG;
            inner.push_str(&format!(
                "<rect class=\"pele-tree-highlight\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_c)\" fill-opacity=\"0.14\"{}/>",
                num(x),
                num(top + row.top + 1.0),
                num(total_width - inset + HIGHLIGHT_OVERHANG - x),
                num(row.height - 2.0),
                ROUNDED
            ));
        }
        if let Some(ref ic) = row.icon {
            let name = ic.strip_prefix(&builtin_prefix).unwrap_or("");
            let g_path = glyph(name).unwrap_or(if directory { FOLDER_GLYPH } else { FILE_GLYPH });
            let drawn = format!("<path d=\"{}\" stroke=\"var(--_m)\" stroke-width=\"1.5\"/>", g_path);
            inner.push_str(&format!(
                "<svg class=\"pele-icon\" data-icon=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{}</svg>",
                esc_text(ic),
                num(inset + row.x),
                num(cy - icon_size / 2.0),
                num(icon_size),
                num(icon_size),
                drawn
            ));
        }
        if let Some(ref lbl) = row.label {
            inner.push_str(&left_label_svg(
                lbl,
                inset + row.label_x,
                top + row.top + padding_y,
                &format!(
                    " class=\"pele-label\"{}",
                    if directory { " font-weight=\"var(--_hw)\"" } else { "" }
                ),
            ));
        } else if !row.text.is_empty() {
            inner.push_str(&format!(
                "<text class=\"pele-label\" x=\"{}\" y=\"{}\"{} xml:space=\"preserve\">{}</text>",
                num(inset + row.label_x),
                num(cy + baseline),
                if directory { " font-weight=\"var(--_hw)\"" } else { "" },
                esc(&row.text)
            ));
        }
        if let Some(ref desc_lbl) = row.description {
            let l_h = row.label.as_ref().map_or(0.0, |l| l.height);
            inner.push_str(&left_label_svg(
                desc_lbl,
                inset + row.label_x,
                top + row.top + padding_y + l_h,
                " class=\"pele-tree-description\" font-style=\"italic\" fill=\"var(--_m)\"",
            ));
        } else if let Some(ref raw_desc) = node.description {
            inner.push_str(&format!(
                "<text class=\"pele-tree-description\" x=\"{}\" y=\"{}\" font-style=\"italic\" fill=\"var(--_m)\" xml:space=\"preserve\">{}</text>",
                num(inset + desc_x),
                num(cy + baseline),
                esc(&decode_entities(raw_desc))
            ));
        }
        body.push_str(&format!(
            "<g class=\"pele-node pele-tree-{}{}\" data-id=\"{}\">{}</g>",
            if directory { "dir" } else { "file" },
            class_names(node.css_class.as_deref().unwrap_or("")),
            esc_text(&node.name),
            inner
        ));
    }

    let mut content = label_svg(
        &title,
        total_width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    );
    if !guides.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-tree-guides\" fill=\"none\" stroke=\"var(--_b)\" stroke-width=\"{}\">{}</g>",
            num(line_thickness),
            guides
        ));
    }
    content.push_str(&body);

    let svg = svg_document(
        "treeView",
        total_width,
        total_height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive,
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: "",
        },
        &Accessible { acc_title: model.acc_title.as_deref(), acc_descr: model.acc_descr.as_deref() },
        &content,
    );

    Rendered {
        svg,
        width: total_width,
        height: total_height,
        links: Vec::new(),
    }
}
