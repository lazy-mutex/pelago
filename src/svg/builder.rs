use crate::text::entities::decode_entities;
use crate::text::label::Label;
use crate::text::measurer::style;

// JavaScript's Math.round: halves go up.
pub fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

// JavaScript's Math.sign, with 0 for 0.
pub fn sign(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        0.0
    }
}

// A number as JavaScript prints it, for the plain decimals a drawing has.
pub fn js_number(v: f64) -> String {
    if v == 0.0 {
        "0".into()
    } else if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn invalid_xml(c: char) -> bool {
    matches!(c as u32, 0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f | 0xfffe | 0xffff)
}

// Removing these characters in esc() can make validated values unsafe.
pub fn has_invalid_xml(text: &str) -> bool {
    text.chars().any(invalid_xml)
}

// Escapes text for XML and drops invalid XML characters and DEL.
pub fn esc(text: &str) -> String {
    if !text.chars().any(|c| matches!(c, '&' | '<' | '>' | '"') || invalid_xml(c)) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if invalid_xml(c) => {}
            c => out.push(c),
        }
    }
    out
}

// Escapes text that is only ever read as text: a tooltip, an accessible name, a `data-id`. Entity
// codes are turned into their characters first.
pub fn esc_text(text: &str) -> String {
    esc(&decode_entities(text))
}

// A coordinate with at most two decimals.
pub fn num(value: f64) -> String {
    let n = js_round(value * 100.0);
    if n == 0.0 || n.is_nan() {
        return if n.is_nan() { "NaN".into() } else { "0".into() };
    }
    if n.abs() >= 1e11 {
        return js_number(n / 100.0);
    }
    let m = n.abs() as u64;
    let q = m / 100;
    let r = m % 100;
    let sign = if n < 0.0 { "-" } else { "" };
    if r == 0 {
        format!("{}{}", sign, q)
    } else if r % 10 == 0 {
        format!("{}{}.{}", sign, q, r / 10)
    } else {
        format!("{}{}.{:02}", sign, q, r)
    }
}

const BOLD: &str = " font-weight=\"var(--_w)\"";
const ITALIC: &str = " font-style=\"italic\"";
const UNDERLINE: &str = " text-decoration=\"underline\"";
const MONO: &str = " font-family=\"var(--_fm)\"";
const SUB: &str = " baseline-shift=\"sub\" font-size=\"smaller\"";
const SUP: &str = " baseline-shift=\"super\" font-size=\"smaller\"";

pub fn span_style(s: u8) -> String {
    let mut out = String::new();
    if s & style::BOLD != 0 {
        out.push_str(BOLD);
    }
    if s & style::ITALIC != 0 {
        out.push_str(ITALIC);
    }
    if s & style::MONO != 0 {
        out.push_str(MONO);
    }
    if s & style::UNDERLINE != 0 {
        out.push_str(UNDERLINE);
    }
    if s & style::SUB != 0 {
        out.push_str(SUB);
    } else if s & style::SUP != 0 {
        out.push_str(SUP);
    }
    out
}

// The emphasis every span of a label has. It is written once, on the text element.
pub fn shared_style(label: &Label) -> u8 {
    let mut shared = style::BOLD | style::ITALIC | style::MONO;
    let mut any = false;
    for line in &label.lines {
        for span in line {
            if span.text.is_empty() {
                continue;
            }
            shared &= span.style;
            any = true;
        }
    }
    if any {
        shared
    } else {
        0
    }
}

// Adds that emphasis to a text element's attributes, unless they give a weight of their own.
pub fn text_style(attrs: &str, shared: u8) -> String {
    let mut out = attrs.to_string();
    if shared & style::BOLD != 0 && !attrs.contains(" font-weight=\"") {
        out.push_str(BOLD);
    }
    if shared & style::ITALIC != 0 && !attrs.contains(" font-style=\"") {
        out.push_str(ITALIC);
    }
    if shared & style::MONO != 0 && !attrs.contains(" font-family=\"") {
        out.push_str(MONO);
    }
    out
}

// Writes a measured label as SVG text centered on (cx, cy).
pub fn label_svg(label: &Label, cx: f64, cy: f64, attrs: &str) -> String {
    let lines = &label.lines;
    if lines.is_empty() {
        return String::new();
    }
    let lh = label.line_height;
    let shift = label.size * 0.35;
    let mut top = cy - label.height / 2.0;

    if lines.len() == 1 && lines[0].len() == 1 && lines[0][0].style == 0 {
        return format!(
            "<text{} x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
            attrs,
            num(cx),
            num(top + lh / 2.0 + shift),
            esc(&lines[0][0].text)
        );
    }

    let shared = shared_style(label);
    let attrs = text_style(attrs, shared);
    let mut body = String::new();
    for line in lines {
        let y = top + lh / 2.0 + shift;
        for (j, span) in line.iter().enumerate() {
            let pos = if j == 0 { format!(" x=\"{}\" y=\"{}\"", num(cx), num(y)) } else { String::new() };
            body.push_str(&format!("<tspan{}{}>{}</tspan>", pos, span_style(span.style & !shared), esc(&span.text)));
        }
        if line.is_empty() {
            body.push_str(&format!("<tspan x=\"{}\" y=\"{}\"> </tspan>", num(cx), num(y)));
        }
        top += lh;
    }
    format!("<text{} text-anchor=\"middle\" xml:space=\"preserve\">{}</text>", attrs, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(num(0.0), "0");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(12.0), "12");
        assert_eq!(num(12.5), "12.5");
        assert_eq!(num(12.345), "12.35");
        assert_eq!(num(-3.05), "-3.05");
        assert_eq!(num(0.005), "0.01");
        assert_eq!(num(-0.005), "0");
    }

    #[test]
    fn escapes() {
        assert_eq!(esc("a<b & \"c\"\u{1}"), "a&lt;b &amp; &quot;c&quot;");
    }
}
