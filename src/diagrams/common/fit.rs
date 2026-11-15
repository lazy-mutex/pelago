use crate::chars::{is_space, is_word};
use crate::text::label::{layout_label, Label};
use crate::text::measurer::TextMeasurer;

fn is_br_after_lt(chars: &[char], after_lt: usize) -> bool {
    if after_lt + 2 >= chars.len() {
        return false;
    }
    if !chars[after_lt].eq_ignore_ascii_case(&'b') || !chars[after_lt + 1].eq_ignore_ascii_case(&'r') {
        return false;
    }
    let mut j = after_lt + 2;
    while j < chars.len() && is_space(chars[j]) {
        j += 1;
    }
    if chars.get(j) == Some(&'/') {
        j += 1;
    }
    chars.get(j) == Some(&'>')
}

fn br_len(chars: &[char], lt: usize) -> Option<usize> {
    if chars.get(lt) != Some(&'<') || lt + 3 >= chars.len() {
        return None;
    }
    if !chars[lt + 1].eq_ignore_ascii_case(&'b') || !chars[lt + 2].eq_ignore_ascii_case(&'r') {
        return None;
    }
    let mut j = lt + 3;
    while j < chars.len() && is_space(chars[j]) {
        j += 1;
    }
    if chars.get(j) == Some(&'/') {
        j += 1;
    }
    (chars.get(j) == Some(&'>')).then_some(j + 1 - lt)
}

fn escape_non_br_tags(raw: &str) -> String {
    if !raw.contains('<') {
        return raw.to_string();
    }
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        if c == '<' && !is_br_after_lt(&chars, i + 1) {
            out.push_str("&lt;");
        } else {
            out.push(c);
        }
    }
    out
}

fn match_units(raw: &str) -> Vec<String> {
    let chars: Vec<char> = raw.chars().collect();
    let n = chars.len();
    let mut units = Vec::new();
    let mut i = 0;
    while i < n {
        if let Some(len) = br_len(&chars, i) {
            units.push(chars[i..i + len].iter().collect());
            i += len;
            continue;
        }
        // ﬂ°°?\w+¶ß
        if chars[i] == 'ﬂ' && chars.get(i + 1) == Some(&'°') {
            let mut j = i + 2;
            if chars.get(j) == Some(&'°') {
                j += 1;
            }
            let w_start = j;
            while j < n && is_word(chars[j]) {
                j += 1;
            }
            if j > w_start && chars.get(j) == Some(&'¶') && chars.get(j + 1) == Some(&'ß') {
                units.push(chars[i..j + 2].iter().collect());
                i = j + 2;
                continue;
            }
        }
        // &#?\w+;
        if chars[i] == '&' {
            let mut j = i + 1;
            if chars.get(j) == Some(&'#') {
                j += 1;
            }
            let w_start = j;
            while j < n && is_word(chars[j]) {
                j += 1;
            }
            if j > w_start && chars.get(j) == Some(&';') {
                units.push(chars[i..j + 1].iter().collect());
                i = j + 1;
                continue;
            }
        }
        units.push(chars[i].to_string());
        i += 1;
    }
    units
}

// Lays out text the way Mermaid's journey and timeline draw it: as plain text in which only
// <br> is markup, so other tags and icon names show as written instead of being read as formatting.
pub fn plain_label(
    raw: Option<&str>,
    measurer: &dyn TextMeasurer,
    size: f64,
    max: f64,
    base: u8,
) -> Label {
    let transformed = raw.map(|t| {
        let mut s = escape_non_br_tags(t);
        if s.contains(":fa-") {
            s = s.replace(":fa-", "&colon;fa-");
        }
        s
    });
    layout_label(transformed.as_deref(), false, measurer, size, max, base)
}

// layout_label wraps at spaces only. For a box of fixed width, this also cuts any word too long to fit.
pub fn fit_label(
    raw: Option<&str>,
    measurer: &dyn TextMeasurer,
    size: f64,
    max: f64,
    base: u8,
) -> Label {
    let label = plain_label(raw, measurer, size, max, base);
    let Some(raw) = raw else {
        return label;
    };
    if label.width <= max {
        return label;
    }
    let mut out = String::new();
    let mut width = 0.0;
    for unit in match_units(raw) {
        let utf16_len = unit.encode_utf16().count();
        if unit.trim().is_empty() || (utf16_len > 2 && unit.starts_with('<')) {
            width = 0.0;
        } else {
            let sample = if utf16_len > 2 { "x" } else { unit.as_str() };
            let w = measurer.width(sample, size, base);
            if width > 0.0 && width + w > max {
                out.push('\n');
                width = 0.0;
            }
            width += w;
        }
        out.push_str(&unit);
    }
    plain_label(Some(&out), measurer, size, max, base)
}
