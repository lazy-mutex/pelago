use super::builder::esc;
use super::ink::{ink, plain_color};
use crate::value::Value;

const SLOTS: usize = 8;
const FUNCTIONS: &[&str] = &[
    "rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color", "color-mix", "light-dark", "var",
];

fn palette_spec(diagram: &str) -> Option<(&'static str, usize, bool)> {
    match diagram {
        "pie" => Some(("pie", 1, false)),
        "gitGraph" => Some(("git", 0, false)),
        "timeline" | "radar" | "treemap" => Some(("cScale", 0, false)),
        "mindmap" => Some(("cScale", 1, false)),
        "venn" => Some(("venn", 1, true)),
        _ => None,
    }
}

fn safe_color(raw: &str) -> Option<String> {
    let color = raw.trim();
    if color.is_empty() || color.len() > 200 {
        return None;
    }
    let valid_chars = color.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '\t' | '#' | '%' | '.' | ',' | '+' | '-' | '/' | '(' | ')')
    });
    if !valid_chars {
        return None;
    }
    if !color.contains('(') {
        return plain_color(color).then(|| color.to_string());
    }
    let mut depth = 0i32;
    for c in color.chars() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth < 0 {
                return None;
            }
        }
    }
    if depth != 0 {
        return None;
    }
    let bytes = color.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'(' {
            let mut end = i;
            while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            let mut start = end;
            while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || matches!(bytes[start - 1], b'_' | b'-')) {
                start -= 1;
            }
            let fname = color[start..end].to_ascii_lowercase();
            if !FUNCTIONS.contains(&fname.as_str()) {
                return None;
            }
            if fname == "var" {
                let inner = color[i + 1..].trim_start();
                let vlen = inner.bytes().take_while(|b| b.is_ascii_alphanumeric() || matches!(*b, b'_' | b'-')).count();
                if !inner[..vlen].starts_with("--pele-") {
                    return None;
                }
            }
        }
        i += 1;
    }
    Some(color.to_string())
}

fn split_list(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in value.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(value[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(value[start..].trim().to_string());
    parts
}

#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub style: String,
    pub inks: [Option<&'static str>; SLOTS],
}

impl Palette {
    // A fill attribute for a label on the series color, when the palette sets one that needs it.
    pub fn ink_fill(&self, index: isize) -> String {
        let slot = index.rem_euclid(SLOTS as isize) as usize;
        match self.inks[slot] {
            Some(color) => format!(" fill=\"{}\"", color),
            None => String::new(),
        }
    }
}

pub fn palette(diagram: &str, config: &Value) -> Palette {
    let vars = config.get("themeVariables");
    let (list, repeat): (Vec<Option<String>>, bool) = if diagram == "xychart" {
        let Some(raw) = vars.get("xyChart").get("plotColorPalette").as_str() else {
            return Palette::default();
        };
        (split_list(raw).into_iter().map(Some).collect(), true)
    } else {
        let Some((prefix, first, repeat)) = palette_spec(diagram) else {
            return Palette::default();
        };
        let mut items = Vec::with_capacity(SLOTS);
        for i in 0..SLOTS {
            let v = vars.get(&format!("{}{}", prefix, first + i));
            let s = v.as_str().map(String::from).filter(|s| !s.is_empty());
            if !repeat || s.is_some() {
                items.push(s);
            }
        }
        (items, repeat)
    };
    if list.is_empty() {
        return Palette::default();
    }
    let mut style = String::new();
    let mut inks = [None; SLOTS];
    for i in 0..SLOTS {
        if !repeat && i >= list.len() {
            break;
        }
        let Some(candidate) = list[i % list.len()].as_deref().and_then(safe_color) else {
            continue;
        };
        style.push_str(&format!(";--pele-series-{}:{}", i + 1, candidate));
        inks[i] = ink(&format!(";fill:{}", candidate), "");
    }
    if style.is_empty() {
        Palette::default()
    } else {
        Palette { style: esc(&style), inks }
    }
}
