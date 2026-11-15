use crate::chars::is_space;
use crate::errors::{input_offset, syntax_error, Edit, PeleError, Result};
use crate::text::entities::decode_entities;

use super::db::SankeyDb;
use super::lexer::{tokenize, T, TOKEN_NAMES};

fn is_sankey_blank(c: char) -> bool {
    if c == '\n' || c == '\r' {
        return false;
    }
    let cp = c as u32;
    if cp < 128 {
        return cp == 32 || cp == 9 || cp == 11 || cp == 12;
    }
    cp == 0xa0
        || cp == 0x1680
        || (0x2000..=0x200a).contains(&cp)
        || cp == 0x2028
        || cp == 0x2029
        || cp == 0x202f
        || cp == 0x205f
        || cp == 0x3000
        || cp == 0xfeff
}

fn has_sankey_invalid_xml(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c as u32, 0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0xfffe | 0xffff))
}

// JavaScript's parseFloat: trims leading whitespace and parses the longest valid decimal prefix.
fn js_parse_float(input: &str) -> f64 {
    let trimmed = input.trim_start_matches(is_space);
    if trimmed.is_empty() {
        return f64::NAN;
    }
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
    }
    if trimmed[i..].starts_with("Infinity") {
        return if bytes[0] == b'-' { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    let int_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let has_int = i > int_start;
    let mut has_frac = false;
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        has_frac = i > frac_start;
    }
    if !has_int && !has_frac {
        return f64::NAN;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    trimmed[..i].parse::<f64>().unwrap_or(f64::NAN)
}

fn trim_rows(text: &[char], edits: &mut Vec<Edit>) -> Vec<char> {
    let mut end = text.len();
    while end > 0 && is_sankey_blank(text[end - 1]) {
        end -= 1;
    }
    let mut out = Vec::with_capacity(end);
    let mut kept = 0usize;
    let mut quoted = false;
    let mut i = 0usize;
    while i < end {
        let c = text[i];
        if quoted {
            if c == '"' {
                if text.get(i + 1) == Some(&'"') {
                    i += 1;
                } else {
                    quoted = false;
                }
            }
            i += 1;
        } else if c == '"' {
            quoted = true;
            i += 1;
        } else if is_sankey_blank(c) {
            let mut j = i + 1;
            while j < text.len() && is_sankey_blank(text[j]) {
                j += 1;
            }
            let p = if i > 0 { Some(text[i - 1]) } else { None };
            let q = text.get(j).copied();
            // Preserve blank first fields, which Mermaid accepts as empty names.
            if i == 0 || matches!(q, Some('\n' | '\r')) || (matches!(p, Some('\n' | '\r')) && q != Some(',')) {
                out.extend_from_slice(&text[kept..i]);
                edits.push((i, j, 0));
                kept = j;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    if end < text.len() {
        edits.push((end, text.len(), 0));
    }
    out.extend_from_slice(&text[kept..end]);
    out
}

// Unlike Mermaid's sankeyUtils, also trim row edges outside quoted fields.
pub fn prepare_text_for_parsing(text: &str, steps: Option<&mut Vec<Vec<Edit>>>) -> String {
    let text_chars: Vec<char> = text.chars().collect();
    let mut row_edits: Vec<Edit> = Vec::new();
    let rows = trim_rows(&text_chars, &mut row_edits);

    let mut collapsed = Vec::with_capacity(rows.len());
    let mut lb_edits: Vec<Edit> = Vec::new();
    let mut i = 0usize;
    while i < rows.len() {
        if rows[i] == '\n' || rows[i] == '\r' {
            let mut j = i + 1;
            while j < rows.len() && (rows[j] == '\n' || rows[j] == '\r') {
                j += 1;
            }
            collapsed.push('\n');
            lb_edits.push((i, j, 1));
            i = j;
        } else {
            collapsed.push(rows[i]);
            i += 1;
        }
    }

    let mut lead = 0usize;
    while lead < collapsed.len() && is_space(collapsed[lead]) {
        lead += 1;
    }
    let mut trail_start = collapsed.len();
    while trail_start > lead && is_space(collapsed[trail_start - 1]) {
        trail_start -= 1;
    }

    if let Some(steps) = steps {
        steps.push(row_edits);
        steps.push(lb_edits);
        steps.push(vec![(0, lead, 0), (trail_start, collapsed.len(), 0)]);
    }

    collapsed[lead..trail_start].iter().collect()
}

// Report errors in `source`, the text before prepare_text_for_parsing.
pub fn parse_sankey(src: &str, db: &mut SankeyDb, source: Option<&str>) -> Result<()> {
    let src_chars: Vec<char> = src.chars().collect();
    let raw_source = source.unwrap_or(src);
    let source_chars: Vec<char> = raw_source.chars().collect();
    let tokens = tokenize(&src_chars);
    let mut i = 0usize;

    let make_error = |at: usize, detail: &str| -> PeleError {
        let mut steps: Vec<Vec<Edit>> = Vec::new();
        if raw_source != src {
            prepare_text_for_parsing(raw_source, Some(&mut steps));
        }
        // Map EOF before the trimmed trailing blanks.
        let offset = if at > 0 && at >= src_chars.len() {
            input_offset(&steps, src_chars.len() - 1) + 1
        } else {
            input_offset(&steps, at)
        };
        syntax_error("sankey", &source_chars, offset, detail, false)
    };

    let fail = |idx: usize, expected: &str| -> PeleError {
        let t = tokens.types[idx];
        let got = if t == T::Stuck {
            if idx > 0 {
                TOKEN_NAMES[tokens.types[idx - 1] as usize]
            } else {
                "undefined"
            }
        } else {
            TOKEN_NAMES[t as usize]
        };
        make_error(tokens.starts[idx], &format!("Expecting {}, got '{}'", expected, got))
    };

    let expect = |idx: &mut usize, kind: T, also: &str| -> Result<()> {
        if tokens.types[*idx] != kind {
            return Err(fail(*idx, &format!("'{}'{}", TOKEN_NAMES[kind as usize], also)));
        }
        *idx += 1;
        Ok(())
    };

    let field = |idx: &mut usize| -> Result<String> {
        if tokens.types[*idx] == T::NonEscapedText {
            let s = tokens.texts[*idx].clone();
            *idx += 1;
            return Ok(s);
        }
        if tokens.types[*idx] != T::Dquote {
            return Err(fail(*idx, "'DQUOTE', 'NON_ESCAPED_TEXT'"));
        }
        *idx += 1;
        if tokens.types[*idx] != T::EscapedText {
            return Err(fail(*idx, "'ESCAPED_TEXT'"));
        }
        let text = tokens.texts[*idx].clone();
        *idx += 1;
        expect(idx, T::Dquote, "")?;
        Ok(text)
    };

    let name = |idx: &mut usize| -> Result<String> {
        let start = tokens.starts[*idx];
        let raw = field(idx)?;
        let text = raw.trim_matches(is_space).replace("\"\"", "\"");
        // Entity codes can encode characters excluded by the lexer.
        if has_sankey_invalid_xml(&decode_entities(&text)) {
            return Err(make_error(start, "Invalid XML character in node name"));
        }
        Ok(text)
    };

    expect(&mut i, T::Sankey, "")?;
    expect(&mut i, T::Newline, "")?;
    loop {
        let from = name(&mut i)?;
        expect(&mut i, T::Comma, ", 'NEWLINE', 'EOF'")?;
        let to = name(&mut i)?;
        expect(&mut i, T::Comma, ", 'NEWLINE', 'EOF'")?;
        let start = tokens.starts[i];
        // Keep Mermaid's numeric prefixes, but reject non-finite values.
        let val_str = field(&mut i)?;
        let value = js_parse_float(&val_str);
        if !value.is_finite() {
            return Err(make_error(start, "Expecting a number"));
        }
        let src_node = db.find_or_create_node(&from);
        let dst_node = db.find_or_create_node(&to);
        db.add_link(src_node, dst_node, value);
        if tokens.types[i] != T::Newline {
            break;
        }
        i += 1;
    }
    if tokens.types[i] == T::Eof {
        i += 1;
    }
    if tokens.types[i] != T::End {
        return Err(fail(i, "'NEWLINE', 'EOF'"));
    }
    Ok(())
}
