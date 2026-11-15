use super::builder::{esc, has_invalid_xml};

// Private aliases set once on the root element, so each use is short and the public
// --pele-* tokens keep their fallbacks.
pub const ROOT_STYLE: &str = "--_bg:var(--pele-bg,#fff);--_fg:var(--pele-fg,#1f1f1f);--_m:var(--pele-muted,#6e6e6e);\
--_l:var(--pele-line,#1f1f1f);--_s:var(--pele-surface,#f3f3f3);--_a:var(--pele-surface-alt,#e6e6e6);\
--_b:var(--pele-border,#8a8a8a);--_c:var(--pele-accent,#5b7bd5);color:var(--_fg)";

pub const FONT_MONO: &str = "var(--_fm)";
pub const RADIUS: &str = "var(--pele-radius,4px)";
pub const ROUNDED: &str = " style=\"rx:var(--_r)\"";

// Expects a shape style from resolve_style().
pub fn sets_fill(shape: &str) -> bool {
    shape.contains("\"fill:") || shape.contains(";fill:")
}

pub fn rounded(attrs: &str) -> String {
    match attrs.find(" style=\"") {
        None => format!("{}{}", attrs, ROUNDED),
        Some(at) => format!("{}rx:var(--_r);{}", &attrs[..at + 8], &attrs[at + 8..]),
    }
}

// The color of what a diagram marks as critical.
pub const CRITICAL: &str = "var(--pele-critical,#e45756)";

fn family_char(c: char, first: bool) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || (!first && (c == ' ' || c == '.'))
}

// A list of font names, quoted or not, and nothing that could end the declaration it is put in.
fn valid_families(list: &str) -> bool {
    if has_invalid_xml(list) {
        return false;
    }
    list.split(',').all(|part| {
        let name = part.trim();
        let quoted = |q: char| {
            name.len() >= 2
                && name.starts_with(q)
                && name.ends_with(q)
                && !name[1..name.len() - 1].contains(['"', '\'', '\\', '\n', '<', '>', '{', '}', '(', ')', ';', ':', '@'])
        };
        if quoted('"') || quoted('\'') {
            return true;
        }
        !name.is_empty() && name.chars().enumerate().all(|(i, c)| family_char(c, i == 0))
    })
}

pub fn families<'a>(list: Option<&'a str>, generic: &'a str) -> &'a str {
    match list {
        Some(l) if !l.is_empty() && valid_families(l) => l,
        _ => generic,
    }
}

// The fonts the labels were measured in are the ones they are drawn in, unless the page names others.
pub fn font_style(size: f64, family: Option<&str>, mono: Option<&str>) -> String {
    format!(
        ";--_fm:var(--pele-font-mono,{});font:{}px sans-serif;letter-spacing:normal;word-spacing:normal;text-transform:none;direction:ltr;font-family:var(--pele-font,{})",
        families(mono, "monospace"),
        super::builder::js_number(size),
        families(family, "sans-serif")
    )
}

const SHAPE_PROPS: &[&str] = &[
    "fill",
    "fill-opacity",
    "stroke",
    "stroke-width",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-opacity",
    "opacity",
    "rx",
    "ry",
];

const TEXT_PROPS: &[&str] =
    &["color", "font-size", "font-family", "font-weight", "font-style", "text-decoration", "letter-spacing", "word-spacing", "opacity"];

// /url\s*\(|expression\s*\(|javascript:|[<>{};\\]|@import|image-set|\/\*/i
fn unsafe_value(value: &str) -> bool {
    let v = value.to_lowercase();
    let call = |name: &str| {
        let mut from = 0;
        while let Some(at) = v[from..].find(name) {
            let rest = v[from + at + name.len()..].trim_start();
            if rest.starts_with('(') {
                return true;
            }
            from += at + name.len();
        }
        false
    };
    call("url") || call("expression") || v.contains("javascript:") || v.contains(['<', '>', '{', '}', ';', '\\']) || v.contains("@import") || v.contains("image-set") || v.contains("/*")
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedStyle {
    pub shape: String,
    pub line: String,
    pub text: String,
    pub stroke: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub font_size: Option<f64>,
}

fn set(map: &mut Vec<(String, String)>, key: &str, value: &str) {
    match map.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value.to_string(),
        None => map.push((key.to_string(), value.to_string())),
    }
}

fn strip_important(value: &str) -> String {
    let lower = value.to_lowercase();
    if !lower.contains("!important") {
        return value.to_string();
    }
    let mut out = String::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find("!important") {
        out.push_str(&value[from..from + at]);
        from += at + "!important".len();
    }
    out.push_str(&value[from..]);
    out
}

// Splits Mermaid style declarations into what applies to a shape and what applies to its text.
pub fn resolve_style<S: AsRef<str>>(declarations: &[S]) -> ResolvedStyle {
    let mut shape: Vec<(String, String)> = Vec::new();
    let mut text: Vec<(String, String)> = Vec::new();
    for decl in declarations {
        let decl = decl.as_ref();
        let Some(at) = decl.find(':').filter(|&at| at > 0) else { continue };
        let prop = decl[..at].trim().to_lowercase();
        let value = strip_important(&decl[at + 1..]);
        let value = value.trim();
        if value.is_empty() || unsafe_value(value) || has_invalid_xml(value) {
            continue;
        }
        if SHAPE_PROPS.contains(&prop.as_str()) {
            set(&mut shape, &prop, value);
        }
        if TEXT_PROPS.contains(&prop.as_str()) {
            set(&mut text, if prop == "color" { "fill" } else { &prop }, value);
        }
    }
    if shape.is_empty() && text.is_empty() {
        return ResolvedStyle::default();
    }
    let mut shape_css = String::new();
    let mut line_css = String::new();
    for (prop, value) in &shape {
        shape_css.push_str(&format!("{}:{};", prop, value));
        if prop != "fill" && prop != "fill-opacity" {
            line_css.push_str(&format!("{}:{};", prop, value));
        }
    }
    let mut text_css = String::new();
    for (prop, value) in &text {
        text_css.push_str(&format!("{}:{};", prop, value));
    }
    let get = |map: &Vec<(String, String)>, key: &str| map.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let weight = get(&text, "font-weight");
    let px = get(&text, "font-size").and_then(|size| {
        let digits = size.strip_suffix("px").unwrap_or(&size);
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')).then(|| leading_float(digits)).flatten()
    });
    let attr = |css: &str| if css.is_empty() { String::new() } else { format!(" style=\"{}\"", esc(css)) };
    ResolvedStyle {
        shape: attr(&shape_css),
        line: attr(&line_css),
        text: attr(&text_css),
        stroke: get(&shape, "stroke"),
        bold: weight.as_deref().is_some_and(|w| w == "bold" || w == "bolder" || w.trim().parse::<f64>().is_ok_and(|n| n >= 600.0)),
        italic: get(&text, "font-style").as_deref() == Some("italic"),
        font_size: px.filter(|&px| px > 0.0 && px <= 200.0),
    }
}

// JavaScript's parseFloat for a string of digits and dots.
pub fn leading_float(s: &str) -> Option<f64> {
    let mut end = 0;
    let mut dot = false;
    for (i, b) in s.bytes().enumerate() {
        if b == b'.' {
            if dot {
                break;
            }
            dot = true;
        } else if !b.is_ascii_digit() {
            break;
        }
        end = i + 1;
    }
    s[..end].parse().ok()
}

// Keeps what can be a class name: /[^\w-]+/g becomes a space.
pub fn class_names(raw: &str) -> String {
    let mut cleaned = String::new();
    let mut gap = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            if gap && !cleaned.is_empty() {
                cleaned.push(' ');
            }
            gap = false;
            cleaned.push(c);
        } else {
            gap = true;
        }
    }
    if cleaned.is_empty() {
        cleaned
    } else {
        format!(" {}", cleaned)
    }
}

const SERIES: [&str; 8] = ["#4c78a8", "#f58518", "#54a24b", "#e45756", "#72b7b2", "#eeca3b", "#b279a2", "#9d755d"];

// Color for the nth data series in a chart, cycling through eight tokens.
pub fn series_color(index: i64) -> String {
    let k = index.rem_euclid(8) as usize;
    format!("var(--pele-series-{},{})", k + 1, SERIES[k])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles() {
        let s = resolve_style(&["fill:#f9f", "stroke:#333", "stroke-width:4px", "color:red", "font-weight:bold", "background:url(x)"]);
        assert_eq!(s.shape, " style=\"fill:#f9f;stroke:#333;stroke-width:4px;\"");
        assert_eq!(s.line, " style=\"stroke:#333;stroke-width:4px;\"");
        assert_eq!(s.text, " style=\"fill:red;font-weight:bold;\"");
        assert!(s.bold);
        assert_eq!(s.stroke.as_deref(), Some("#333"));
        assert_eq!(resolve_style(&["fill:url(#a)"]), ResolvedStyle::default());
        assert_eq!(class_names("a b$c  d-e"), " a b c d-e");
    }
}
