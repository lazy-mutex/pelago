use crate::chars::is_space;
use crate::svg::builder::{esc, js_round, num, shared_style, span_style, text_style};
use crate::text::label::{Label, Span};
use crate::text::measurer::TextMeasurer;

// Literal labels, including filenames and protocol fields, must retain every character but the
// spaces a line breaks at. Long words can break between code points; measurement stays bounded
// by one line of text.
pub fn literal_label(text: &str, measurer: &dyn TextMeasurer, size: f64, room: f64, style: u8) -> Label {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let chars: Vec<char> = raw.chars().collect();
        if chars.is_empty() {
            lines.push(String::new());
        }
        let mut from = 0usize;
        while from < chars.len() {
            let fits = |length: usize| -> bool {
                let sub: String = chars[from..from + length].iter().collect();
                measurer.width(&sub, size, style) <= room
            };
            let mut hi = 1usize;
            while hi < chars.len() - from && fits(hi) {
                hi = (chars.len() - from).min(hi * 2);
            }
            let mut lo = 1usize;
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                if fits(mid) {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            let mut take = lo;
            let wraps = from + take < chars.len();
            if wraps && !is_space(chars[from + take]) {
                let mut i = take - 1;
                while i > 0 {
                    if is_space(chars[from + i]) {
                        take = i + 1;
                        break;
                    }
                    i -= 1;
                }
            }
            let line: String = chars[from..from + take].iter().collect();
            if wraps {
                let trimmed = line.trim_end();
                lines.push(if trimmed.is_empty() { line } else { trimmed.to_string() });
            } else {
                lines.push(line);
            }
            from += take;
            while wraps && from < chars.len() && is_space(chars[from]) {
                from += 1;
            }
        }
    }
    if text.is_empty() {
        lines.clear();
    }
    let widths: Vec<f64> = lines.iter().map(|line| measurer.width(line, size, style)).collect();
    let width = widths.iter().fold(0.0f64, |a, &b| a.max(b));
    let line_height = js_round(size * 1.5);
    let height = lines.len() as f64 * line_height;
    Label {
        lines: lines
            .into_iter()
            .enumerate()
            .map(|(i, line)| vec![Span { text: line, style, width: widths[i] }])
            .collect(),
        widths,
        width,
        height,
        size,
        line_height,
    }
}

pub fn break_label(label: &Label, measurer: &dyn TextMeasurer, room: f64) -> Label {
    if label.width <= room {
        return label.clone();
    }
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();
    for i in 0..label.lines.len() {
        if label.widths[i] <= room {
            lines.push(label.lines[i].clone());
            widths.push(label.widths[i]);
            continue;
        }
        let mut current: Vec<Span> = Vec::new();
        let mut width = 0.0f64;
        for span in &label.lines[i] {
            for ch in span.text.chars() {
                let cs = ch.to_string();
                let joined = current.last().is_some_and(|last| last.style == span.style);
                let next_width = if joined {
                    let last = current.last().unwrap();
                    let mut combined = String::with_capacity(last.text.len() + cs.len());
                    combined.push_str(&last.text);
                    combined.push(ch);
                    measurer.width(&combined, label.size, span.style)
                } else {
                    measurer.width(&cs, label.size, span.style)
                };
                let mut added = next_width - if joined { current.last().unwrap().width } else { 0.0 };
                if !current.is_empty() && width + added > room {
                    lines.push(std::mem::take(&mut current));
                    widths.push(width);
                    width = 0.0;
                    added = measurer.width(&cs, label.size, span.style);
                }
                match current.last_mut() {
                    Some(last) if last.style == span.style => {
                        last.text.push(ch);
                        last.width += added;
                    }
                    _ => current.push(Span { text: cs, style: span.style, width: added }),
                }
                width += added;
            }
        }
        if !current.is_empty() || label.lines[i].is_empty() {
            lines.push(current);
            widths.push(width);
        }
    }
    let width = widths.iter().fold(0.0f64, |a, &b| a.max(b));
    let height = lines.len() as f64 * label.line_height;
    Label {
        lines,
        widths,
        width,
        height,
        size: label.size,
        line_height: label.line_height,
    }
}

// Writes a label with every line starting at x. A line is anchored at its start and keeps its
// spaces, so that where it begins does not depend on how its text was measured.
pub fn left_label_svg(label: &Label, x: f64, top: f64, attrs: &str) -> String {
    let shared = shared_style(label);
    let open = format!("<text{}", text_style(attrs, shared));
    let mut body = String::new();
    for (i, spans) in label.lines.iter().enumerate() {
        let cy = top + (i as f64 + 0.5) * label.line_height;
        let mut text = String::new();
        for span in spans {
            let own = span_style(span.style & !shared);
            if own.is_empty() {
                text.push_str(&esc(&span.text));
            } else {
                text.push_str(&format!("<tspan{}>{}</tspan>", own, esc(&span.text)));
            }
        }
        if !text.is_empty() {
            body.push_str(&format!(
                "{} x=\"{}\" y=\"{}\" text-anchor=\"start\" xml:space=\"preserve\">{}</text>",
                open,
                num(x),
                num(cy + label.size * 0.35),
                text
            ));
        }
    }
    body
}
