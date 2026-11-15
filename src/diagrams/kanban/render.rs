use super::db::{KanbanDb, KanbanNode};
use crate::diagrams::common::fit_width::title_room;
use crate::options::{LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num, text_style};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{class_names, CRITICAL, ROUNDED};
use crate::svg::title::title_label;
use crate::text::label::{label_line, layout_label, line_middle, Label, Span};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::url::{link_url, rel_attr, safe_url};
use crate::value::Value;

const COLUMN_GAP: f64 = 12.0;
const COLUMN_PAD: f64 = 8.0;
const CARD_PAD: f64 = 10.0;
const CARD_GAP: f64 = 8.0;
const META_GAP: f64 = 4.0;

// Mermaid names five priorities and gives Medium no color. Matching here ignores case. The high
// ones are marked in the critical color and the low ones in the muted one, the nearer to Medium
// the fainter. They are not series colors, as a theme may put those in any order.
fn priority_attrs(priority: &str) -> Option<String> {
    match priority {
        "very high" => Some(format!("stroke=\"{}\"", CRITICAL)),
        "high" => Some(format!("stroke=\"{}\" opacity=\"0.5\"", CRITICAL)),
        "low" => Some("stroke=\"var(--_m)\" opacity=\"0.5\"".to_string()),
        "very low" => Some("stroke=\"var(--_m)\"".to_string()),
        _ => None,
    }
}

// Lays out text for a fixed width. A word too long for a line is broken between characters,
// because a card cannot grow sideways to hold it.
fn fit_label(
    raw: Option<&str>,
    markdown: bool,
    measurer: &dyn TextMeasurer,
    size: f64,
    max: f64,
    base: u8,
) -> Label {
    let label = layout_label(raw, markdown, measurer, size, max, base);
    if label.width <= max {
        return label;
    }
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();
    let mut widest = 0.0_f64;
    let mut line: Vec<Span> = Vec::new();
    let mut width = 0.0_f64;

    let commit = |lines: &mut Vec<Vec<Span>>,
                  widths: &mut Vec<f64>,
                  widest: &mut f64,
                  line: &mut Vec<Span>,
                  width: &mut f64| {
        lines.push(std::mem::take(line));
        widths.push(*width);
        *widest = widest.max(*width);
        *width = 0.0;
    };

    for i in 0..label.lines.len() {
        if label.widths[i] <= max {
            line = label.lines[i].clone();
            width = label.widths[i];
            commit(&mut lines, &mut widths, &mut widest, &mut line, &mut width);
            continue;
        }
        for span in &label.lines[i] {
            let mut text = String::new();
            let mut from = width;
            for ch in span.text.chars() {
                let cs = ch.to_string();
                let w = measurer.width(&cs, size, span.style);
                if width > 0.0 && width + w > max {
                    if !text.is_empty() {
                        line.push(Span {
                            text: std::mem::take(&mut text),
                            style: span.style,
                            width: width - from,
                        });
                    }
                    commit(&mut lines, &mut widths, &mut widest, &mut line, &mut width);
                    from = 0.0;
                }
                text.push(ch);
                width += w;
            }
            if !text.is_empty() {
                line.push(Span {
                    text,
                    style: span.style,
                    width: width - from,
                });
            }
        }
        commit(&mut lines, &mut widths, &mut widest, &mut line, &mut width);
    }

    let height = lines.len() as f64 * label.line_height;
    Label {
        lines,
        widths,
        width: widest,
        height,
        size: label.size,
        line_height: label.line_height,
    }
}

// Writes a label with every line starting at x, or ending there, and the first line's box starting at y.
fn text_block(label: &Label, x: f64, y: f64, attrs: &str, end: bool) -> String {
    let mut out = String::new();
    for i in 0..label.lines.len() {
        let line = &label.lines[i];
        let cy = y + line_middle(label, i);
        if line.len() == 1 && (line[0].style & (style::SUB | style::SUP | style::UNDERLINE)) == 0 {
            out.push_str(&format!(
                "<text{} x=\"{}\" y=\"{}\"{}>{}</text>",
                text_style(attrs, line[0].style),
                num(x),
                num(cy + label.size * 0.35),
                if end { " text-anchor=\"end\"" } else { "" },
                esc(&line[0].text)
            ));
        } else {
            let width = label.widths[i];
            out.push_str(&label_svg(
                &label_line(label, i),
                x + if end { -width } else { width } / 2.0,
                cy,
                attrs,
            ));
        }
    }
    out
}

struct Card<'a> {
    item: &'a KanbanNode,
    label: Label,
    ticket: Label,
    assigned: Label,
    // Height of the rows under the text that hold the ticket and the assignee.
    meta: f64,
    height: f64,
}

pub fn render_kanban(model: &KanbanDb, config: &Value, options: &RenderOptions) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let line = js_round(size * 1.5);
    let small_line = js_round(small * 1.5);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let conf = config.get("kanban");
    let section_width = conf.get("sectionWidth").as_f64();

    let own_width = section_width
        .filter(|&sw| sw > 0.0)
        .map_or(200.0, |sw| sw.max(80.0).min(2000.0));

    // A board wider than the width the host has is folded into rows of columns, which then
    // share the row between them.
    let room = match options.max_width.filter(|&mw| mw > 0.0) {
        Some(mw) => mw - 2.0 * pad,
        None => f64::INFINITY,
    };
    let count = model.sections.len();
    let fits = count as f64 * (own_width + COLUMN_GAP) - COLUMN_GAP <= room;
    let per_row = if fits {
        count.max(1)
    } else {
        1usize.max(((room + COLUMN_GAP) / (own_width + COLUMN_GAP)).floor() as usize)
    };
    let column_width = if fits {
        own_width
    } else {
        own_width
            .min(room)
            .max((((room + COLUMN_GAP) / per_row as f64).floor() - COLUMN_GAP).min(own_width * 2.0))
    };
    let base_url = conf.get("ticketBaseUrl").as_str().unwrap_or("");
    let card_width = column_width - 2.0 * COLUMN_PAD;
    let text_width = card_width - 2.0 * CARD_PAD;
    let mut links: Vec<LinkInfo> = Vec::new();

    let sections = &model.sections;
    let mut titles: Vec<Label> = Vec::with_capacity(sections.len());
    let mut cards: Vec<Vec<Card>> = Vec::with_capacity(sections.len());
    let mut head_height = line;
    let mut body_heights: Vec<f64> = Vec::with_capacity(sections.len());

    for section in sections {
        let count_width = measurer.width(&section.items.len().to_string(), small, 0);
        let reserved = count_width + 12.0;
        let title = fit_label(
            Some(&section.node.label),
            true,
            measurer,
            size,
            card_width - 8.0 - reserved,
            style::BOLD,
        );
        head_height = head_height.max(title.height);
        titles.push(title);

        let mut list: Vec<Card> = Vec::with_capacity(section.items.len());
        let mut height = 0.0_f64;
        for item in &section.items {
            let label = fit_label(Some(&item.label), true, measurer, small, text_width, 0);
            let ticket = fit_label(item.ticket.as_deref(), false, measurer, small, text_width, 0);
            let assigned =
                fit_label(item.assigned.as_deref(), false, measurer, small, text_width, 0);
            let meta = if ticket.width + assigned.width + 8.0 > text_width {
                ticket.height + assigned.height
            } else {
                ticket.height.max(assigned.height)
            };
            let card_height = 2.0 * CARD_PAD
                + label.height.max(small_line)
                + if meta > 0.0 { META_GAP + meta } else { 0.0 };
            list.push(Card {
                item,
                label,
                ticket,
                assigned,
                meta,
                height: card_height,
            });
            height += card_height + CARD_GAP;
        }
        cards.push(list);
        body_heights.push(height);
    }
    head_height += 2.0 * COLUMN_PAD;

    let mut row_heights: Vec<f64> = Vec::new();
    let mut row_tops: Vec<f64> = Vec::new();
    let mut first = 0usize;
    let mut y_acc = 0.0_f64;
    while first < sections.len() {
        let mut tallest = CARD_GAP;
        let stop = (first + per_row).min(sections.len());
        for &bh in &body_heights[first..stop] {
            tallest = tallest.max(bh);
        }
        row_tops.push(y_acc);
        row_heights.push(head_height + tallest);
        y_acc += head_height + tallest + COLUMN_GAP;
        first += per_row;
    }
    let board_height = if !row_heights.is_empty() {
        row_tops[row_tops.len() - 1] + row_heights[row_heights.len() - 1]
    } else {
        0.0
    };

    let title = title_label(model.title.as_deref(), measurer, size, title_room(options));
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };
    let board_width =
        (sections.len().min(per_row) as f64 * (column_width + COLUMN_GAP) - COLUMN_GAP).max(0.0);
    let width = board_width.max(title.width);
    let left = pad + (width - board_width) / 2.0;
    let edge = column_width - COLUMN_PAD - 4.0;
    let small_str = js_number(small);

    let mut out = String::new();
    for (index, section) in sections.iter().enumerate() {
        let row = index / per_row;
        let mut body = format!(
            "<rect width=\"{}\" height=\"{}\" fill=\"var(--_s)\"{}/>{}<text class=\"pele-count\" x=\"{}\" y=\"{}\" text-anchor=\"end\" font-size=\"{}\" fill=\"var(--_m)\">{}</text>",
            js_number(column_width),
            num(row_heights[row]),
            ROUNDED,
            text_block(
                &titles[index],
                COLUMN_PAD + 4.0,
                COLUMN_PAD,
                " class=\"pele-cluster-label\" font-weight=\"var(--_hw)\"",
                false
            ),
            js_number(edge),
            num(COLUMN_PAD + line / 2.0 + small * 0.35),
            small_str,
            section.items.len()
        );

        let mut y = head_height;
        for card in &cards[index] {
            let item = card.item;
            let priority = item
                .priority
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_lowercase();
            let color = priority_attrs(&priority);
            let mut inner = format!(
                "<rect width=\"{}\" height=\"{}\" fill=\"var(--_bg)\" stroke=\"var(--_b)\"{}/>",
                js_number(card_width),
                num(card.height),
                ROUNDED
            );
            if let Some(ref c_attr) = color {
                inner.push_str(&format!(
                    "<path class=\"pele-priority\" d=\"M2.5,5V{}\" fill=\"none\" {} stroke-width=\"3\" stroke-linecap=\"round\"/>",
                    num(card.height - 5.0),
                    c_attr
                ));
            }
            inner.push_str(&text_block(
                &card.label,
                CARD_PAD,
                CARD_PAD,
                " class=\"pele-label\"",
                false,
            ));

            let bottom = card.height - CARD_PAD;
            if let Some(ref ticket_str) = item.ticket.as_ref().filter(|s| !s.is_empty()) {
                let top = bottom - card.meta;
                let safe = if !base_url.is_empty() {
                    Some(safe_url(&base_url.replacen("#TICKET#", ticket_str, 1)))
                } else {
                    None
                };
                let href_opt = safe.as_deref().and_then(|u| link_url(u, options));
                match href_opt {
                    None => {
                        inner.push_str(&text_block(
                            &card.ticket,
                            CARD_PAD,
                            top,
                            " class=\"pele-ticket\" fill=\"var(--_m)\"",
                            false,
                        ));
                    }
                    Some(href) => {
                        inner.push_str(&format!(
                            "<a href=\"{}\" target=\"_blank\"{}{}>{}</a>",
                            esc(href),
                            "",
                            rel_attr(options),
                            text_block(
                                &card.ticket,
                                CARD_PAD,
                                top,
                                " class=\"pele-ticket\" fill=\"var(--_c)\"",
                                false
                            )
                        ));
                        links.push(LinkInfo {
                            id: item.id.clone(),
                            href: href.to_string(),
                            internal: false,
                        });
                    }
                }
            }
            if item.assigned.as_ref().is_some_and(|s| !s.is_empty()) {
                inner.push_str(&text_block(
                    &card.assigned,
                    card_width - CARD_PAD,
                    bottom - card.assigned.height,
                    " class=\"pele-assigned\" fill=\"var(--_m)\"",
                    true,
                ));
            }

            let prio_class = if color.is_some() {
                format!(" pele-priority-{}", priority.replace(' ', "-"))
            } else {
                String::new()
            };
            body.push_str(&format!(
                "<g class=\"pele-node pele-card{}{}\" data-id=\"{}\" transform=\"translate({},{})\" font-size=\"{}\">{}</g>",
                prio_class,
                class_names(item.css_classes.as_deref().unwrap_or("")),
                esc_text(&item.id),
                js_number(COLUMN_PAD),
                num(y),
                small_str,
                inner
            ));
            y += card.height + CARD_GAP;
        }

        out.push_str(&format!(
            "<g class=\"pele-cluster pele-column{}\" data-id=\"{}\" transform=\"translate({},{})\">{}</g>",
            class_names(section.node.css_classes.as_deref().unwrap_or("")),
            esc_text(&section.node.id),
            num(left + (index % per_row) as f64 * (column_width + COLUMN_GAP)),
            num(pad + title_height + row_tops[row]),
            body
        ));
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (board_height + title_height + 2.0 * pad).ceil();
    let content = format!(
        "{}{}",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        out
    );

    let fixed = conf.get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "kanban",
        total_width,
        total_height,
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
        height: total_height,
        links,
    }
}
