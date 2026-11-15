use super::parser::PacketAst;
use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::svg::builder::js_number;
use crate::value::Value;

#[derive(Debug, Clone)]
pub struct PacketBlock {
    pub start: f64,
    pub end: f64,
    // As Mermaid counts it: one less than the width for the pieces of a field split across rows.
    pub bits: f64,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct PacketModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub bits_per_row: f64,
    pub rows: Vec<Vec<PacketBlock>>,
}

const MAX_ROWS: usize = 10_000;

pub fn packet_option(config: &Value, key: &str, fallback: f64, min: f64, max: f64) -> f64 {
    match config.get("packet").get(key).as_f64() {
        Some(v) if v >= min => v.min(max),
        _ => fallback,
    }
}

fn invalid(message: String) -> PeleError {
    PeleError::new(message, ErrorCode::Semantic).of("packet")
}

fn collapse_nl_spaces(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' && i + 1 < chars.len() && is_space(chars[i + 1]) {
            out.push('\n');
            i += 2;
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn build_packet(ast: PacketAst, config: &Value, title: Option<String>) -> Result<PacketModel> {
    let bits_per_row = packet_option(config, "bitsPerRow", 32.0, 1.0, 4096.0).floor();
    let max_word_len = bits_per_row as usize + 1;
    let mut rows: Vec<Vec<PacketBlock>> = Vec::new();
    let mut last_bit = -1.0_f64;
    let mut word: Vec<PacketBlock> = Vec::new();
    let mut row = 1.0_f64;

    for block in ast.blocks {
        if let (Some(s), Some(e)) = (block.start, block.end) {
            if e < s {
                return Err(invalid(format!(
                    "Packet block {} - {} is invalid. End must be greater than start.",
                    js_number(s),
                    js_number(e)
                )));
            }
        }
        let mut start = block.start.unwrap_or(last_bit + 1.0);
        if start != last_bit + 1.0 {
            let shown_end = block.end.unwrap_or(start);
            return Err(invalid(format!(
                "Packet block {} - {} is not contiguous. It should start from {}.",
                js_number(start),
                js_number(shown_end),
                js_number(last_bit + 1.0)
            )));
        }
        if block.bits == Some(0.0) {
            return Err(invalid(format!(
                "Packet block {} is invalid. Cannot have a zero bit field.",
                js_number(start)
            )));
        }
        let end = block.end.unwrap_or(start + block.bits.unwrap_or(1.0) - 1.0);
        let mut bits = block.bits.unwrap_or(end - start + 1.0);
        last_bit = end;

        while word.len() <= max_word_len && rows.len() < MAX_ROWS {
            let row_end = row * bits_per_row - 1.0;
            if end <= row_end {
                word.push(PacketBlock {
                    start,
                    end,
                    bits,
                    label: block.label.clone(),
                });
                if end == row_end {
                    rows.push(std::mem::take(&mut word));
                    row += 1.0;
                }
                break;
            }
            word.push(PacketBlock {
                start,
                end: row_end,
                bits: row_end - start,
                label: block.label.clone(),
            });
            rows.push(std::mem::take(&mut word));
            row += 1.0;
            start = row_end + 1.0;
            bits = end - start;
        }
    }
    if !word.is_empty() {
        rows.push(word);
    }

    Ok(PacketModel {
        title: ast.title.filter(|s| !s.is_empty()).or_else(|| title.filter(|s| !s.is_empty())),
        acc_title: ast
            .acc_title
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches(is_space).to_string()),
        acc_descr: ast
            .acc_descr
            .filter(|s| !s.is_empty())
            .map(|s| collapse_nl_spaces(&s)),
        bits_per_row,
        rows,
    })
}
