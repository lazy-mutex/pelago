use super::builder::{esc, esc_text, js_number};
use super::theme::{font_style, RADIUS, ROOT_STYLE};

#[derive(Default)]
pub struct Accessible<'a> {
    pub acc_title: Option<&'a str>,
    pub acc_descr: Option<&'a str>,
}

pub struct Document<'a> {
    pub id_prefix: &'a str,
    pub responsive: bool,
    pub font_family: Option<&'a str>,
    pub font_family_mono: Option<&'a str>,
    // The --pele-series-* declarations a diagram's theme variables set.
    pub palette: &'a str,
}

// The weights of bold text, each set only in a diagram that has such text.
const WEIGHTS: [(&str, &str); 3] = [("tw", "title"), ("hw", "heading"), ("w", "bold")];

pub fn svg_document(kind: &str, width: f64, height: f64, font_size: f64, doc: &Document, acc: &Accessible, content: &str) -> String {
    let prefix = esc(doc.id_prefix);
    let mut head = String::new();
    let mut aria = String::new();
    if let Some(title) = acc.acc_title.filter(|t| !t.is_empty()) {
        head.push_str(&format!("<title id=\"{}-title\">{}</title>", prefix, esc_text(title)));
        aria.push_str(&format!(" aria-labelledby=\"{}-title\"", prefix));
    }
    if let Some(descr) = acc.acc_descr.filter(|d| !d.is_empty()) {
        head.push_str(&format!("<desc id=\"{}-desc\">{}</desc>", prefix, esc_text(descr)));
        aria.push_str(&format!(" aria-describedby=\"{}-desc\"", prefix));
    }
    let fit = if doc.responsive { "max-width:100%;height:auto;" } else { "" };
    let mut weights = if content.contains("var(--_r)") { format!(";--_r:{}", RADIUS) } else { String::new() };
    for (alias, name) in WEIGHTS {
        if content.contains(&format!("var(--_{})", alias)) {
            weights.push_str(&format!(";--_{}:var(--pele-{}-weight,bold)", alias, name));
        }
    }
    let (w, h) = (js_number(width), js_number(height));
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"pele pele-{kind}\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" style=\"{fit}{ROOT_STYLE}{weights}{font}{palette}\" fill=\"var(--_fg)\" role=\"graphics-document document\" aria-roledescription=\"{kind}\"{aria}>{head}{content}</svg>",
        font = esc(&font_style(font_size, doc.font_family, doc.font_family_mono)),
        palette = doc.palette,
    )
}
