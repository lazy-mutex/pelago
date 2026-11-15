use super::model::{packet_option, PacketBlock, PacketModel};
use crate::diagrams::common::fit_text::{fit_text, Fitted};
use crate::diagrams::common::fit_width::{title_room, turn_to_fit};
use crate::diagrams::common::literal_label::literal_label;
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::{esc, js_number, js_round, label_svg, num};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::ROUNDED;
use crate::text::entities::decode_entities;
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;
use std::collections::HashMap;

pub fn render_packet(model: &PacketModel, config: &Value, options: &RenderOptions) -> Rendered {
    turn_to_fit(options, &|down| draw(model, config, options, down))
}

fn draw(model: &PacketModel, config: &Value, options: &RenderOptions, down: bool) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let label_size = js_round(size * 0.875);
    let bit_size = js_round(size * 0.625);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    let packet = config.get("packet");
    let row_height = packet_option(config, "rowHeight", 32.0, 1.0, 400.0);
    let mut bit_width = packet_option(config, "bitWidth", 32.0, 1.0, 400.0);
    let padding_x = packet_option(config, "paddingX", 5.0, 0.0, 400.0);
    let padding_y = packet_option(config, "paddingY", 5.0, 0.0, 400.0);
    let show_bits = packet.get("showBits").as_bool() != Some(false);
    let descending = packet.get("bitOrder").as_str() == Some("descending");
    let mut bits_per_row = model.bits_per_row;
    let mut owned_rows: Vec<Vec<PacketBlock>>;
    let rows: &[Vec<PacketBlock>] = if down {
        let count = 1.0_f64.max(
            ((options.max_width.unwrap_or(320.0) - 2.0 * pad) / bit_width.min(18.0)).floor(),
        );
        // A byte is the shortest row. Below that every label is one character to a line, and a long
        // packet is a row to a bit.
        bits_per_row = bits_per_row.min(8.0_f64.max(2.0_f64.powf(count.log2().floor())));
        let last = model
            .rows
            .last()
            .and_then(|r| r.last())
            .map_or(0.0, |b| b.end);
        if ((last + 1.0) / bits_per_row).ceil() > 10000.0 {
            bits_per_row = model.bits_per_row;
        }
        bit_width = bit_width.min(18.0_f64.max((options.max_width.unwrap_or(320.0) - 2.0 * pad) / bits_per_row));
        // The model has split each field at its own rows. Its pieces are joined again first, so that
        // a field is only split where one of the new rows ends. A piece counts one bit short.
        let piece = |block: &PacketBlock| -> bool { block.bits == block.end - block.start };
        let mut whole: Vec<PacketBlock> = Vec::new();
        for row in &model.rows {
            for block in row {
                if let Some(prev) = whole.last_mut() {
                    if piece(prev)
                        && piece(block)
                        && prev.label == block.label
                        && block.start == prev.end + 1.0
                        && block.start % model.bits_per_row == 0.0
                    {
                        prev.end = block.end;
                        prev.bits = prev.end - prev.start;
                        continue;
                    }
                }
                whole.push(block.clone());
            }
        }
        owned_rows = Vec::new();
        for block in whole {
            let mut start = block.start;
            while start <= block.end {
                let end = block
                    .end
                    .min(((start / bits_per_row).floor() + 1.0) * bits_per_row - 1.0);
                let index = (start / bits_per_row).floor() as usize;
                while owned_rows.len() <= index {
                    owned_rows.push(Vec::new());
                }
                owned_rows[index].push(PacketBlock {
                    start,
                    end,
                    bits: block.bits,
                    label: block.label.clone(),
                });
                start = end + 1.0;
            }
        }
        &owned_rows
    } else {
        &model.rows
    };

    let labels: Vec<Vec<Label>> = if down {
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|block| {
                        let room = 1.0_f64.max(
                            (block.end - block.start + 1.0) * bit_width
                                - padding_x.min(bit_width - 1.0)
                                - 6.0,
                        );
                        literal_label(
                            decode_entities(&block.label).trim(),
                            measurer,
                            label_size,
                            room,
                            0,
                        )
                    })
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    };

    // Bit numbers sit above each row and need room of their own.
    let numbers = if show_bits { bit_size + 4.0 } else { 0.0 };
    let row_heights: Vec<f64> = rows
        .iter()
        .enumerate()
        .map(|(r_idx, _row)| {
            let mut height = row_height;
            if down {
                for lbl in &labels[r_idx] {
                    height = height.max(lbl.height + 10.0);
                }
            }
            height
        })
        .collect();
    let mut tops: Vec<f64> = Vec::with_capacity(row_heights.len());
    let mut rows_height = 0.0_f64;
    for &height in &row_heights {
        tops.push(rows_height);
        rows_height += numbers + height + padding_y;
    }
    if !rows.is_empty() {
        rows_height -= padding_y;
    }

    let gap = padding_x.min(bit_width - 1.0);
    let width = if !rows.is_empty() {
        bits_per_row * bit_width - gap
    } else {
        0.0
    };
    let title = layout_label(
        model.title.as_deref(),
        false,
        measurer,
        size,
        if down { title_room(options) } else { 4000.0 },
        style::BOLD,
    );
    let title_height = if title.height > 0.0 {
        title.height + if rows_height > 0.0 { 12.0 } else { 0.0 }
    } else {
        0.0
    };

    // The pieces of a field split across rows share its label, so each width is fitted once.
    let mut fits: HashMap<String, HashMap<u64, Option<Fitted>>> = HashMap::new();
    let min_fit_size = js_round(size * 0.5);

    let left = pad + 0.0_f64.max((title.width - width) / 2.0);
    let mut fields = String::new();
    let mut bits_out = String::new();
    for (index, row) in rows.iter().enumerate() {
        let y = pad + tops[index] + numbers;
        let cur_row_height = row_heights[index];
        for (b_idx, block) in row.iter().enumerate() {
            let count = block.end - block.start + 1.0;
            let first = block.start % bits_per_row;
            let x = left
                + (if descending {
                    bits_per_row - first - count
                } else {
                    first
                }) * bit_width;
            let w = count * bit_width - gap;
            let label_markup = if down {
                label_svg(&labels[index][b_idx], x + w / 2.0, y + cur_row_height / 2.0, "")
            } else {
                let max_w = w - 6.0;
                let max_bits = if max_w == 0.0 { 0 } else { max_w.to_bits() };
                let by_width = fits.entry(block.label.clone()).or_default();
                let fitted = by_width
                    .entry(max_bits)
                    .or_insert_with(|| {
                        fit_text(
                            decode_entities(&block.label).trim(),
                            max_w,
                            label_size,
                            min_fit_size,
                            measurer,
                            0,
                        )
                    })
                    .as_ref();
                match fitted {
                    Some(text) if text.size <= cur_row_height => {
                        let fs = if text.size == label_size {
                            String::new()
                        } else {
                            format!(" font-size=\"{}\"", num(text.size))
                        };
                        format!(
                            "<text x=\"{}\" y=\"{}\"{}>{}</text>",
                            num(x + w / 2.0),
                            num(y + cur_row_height / 2.0 + text.size * 0.35),
                            fs,
                            esc(&text.text)
                        )
                    }
                    _ => String::new(),
                }
            };
            fields.push_str(&format!(
                "<g class=\"pele-node\" data-id=\"{}-{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>{}</g>",
                js_number(block.start),
                js_number(block.end),
                num(x),
                num(y),
                num(w),
                num(cur_row_height),
                ROUNDED,
                label_markup
            ));
            if !show_bits {
                continue;
            }
            let leading = if descending { block.end } else { block.start };
            let trailing = if descending { block.start } else { block.end };
            if count == 1.0 {
                bits_out.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
                    num(x + w / 2.0),
                    num(y - 4.0),
                    js_number(leading)
                ));
            } else {
                bits_out.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\">{}</text><text x=\"{}\" y=\"{}\" text-anchor=\"end\">{}</text>",
                    num(x),
                    num(y - 4.0),
                    js_number(leading),
                    num(x + w),
                    num(y - 4.0),
                    js_number(trailing)
                ));
            }
        }
    }

    let total_width = (width.max(title.width) + 2.0 * pad).ceil();
    let total_height = (rows_height + title_height + 2.0 * pad).ceil();
    let mut content = String::new();
    if !fields.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-packet-fields\" font-size=\"{}\" text-anchor=\"middle\">{}</g>",
            js_number(label_size),
            fields
        ));
    }
    if !bits_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-packet-bits\" font-size=\"{}\" fill=\"var(--_m)\">{}</g>",
            js_number(bit_size),
            bits_out
        ));
    }
    // Mermaid puts a packet's title under it.
    content.push_str(&label_svg(
        &title,
        total_width / 2.0,
        total_height - pad - title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    ));

    let fixed = packet.get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "packet",
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
        &Accessible {
            acc_title: model.acc_title.as_deref(),
            acc_descr: model.acc_descr.as_deref(),
        },
        &content,
    );

    Rendered {
        svg,
        width: total_width,
        height: total_height,
        links: Vec::new(),
    }
}
