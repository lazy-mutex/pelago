use super::types::{OrderMap, VennStyle, VennSubset, VennText};
use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use std::collections::HashSet;

// Mermaid takes one pair of quotes off an id, label or style value.
pub fn normalize_text(text: &str) -> String {
    let trimmed = text.trim_matches(is_space);
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn js_sort_strings(list: &mut [String]) {
    list.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
}

pub fn json_string_array(ids: &[String]) -> String {
    let mut out = String::from("[");
    for (idx, id) in ids.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push('"');
        for ch in id.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0c}' => out.push_str("\\f"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

fn normalize_ids(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ids.iter().map(|s| normalize_text(s)).collect();
    js_sort_strings(&mut out);
    out
}

#[derive(Debug, Clone, Default)]
pub struct VennDb {
    pub title: Option<String>,
    pub subsets: Vec<VennSubset>,
    pub text_nodes: Vec<VennText>,
    pub styles: Vec<VennStyle>,
    known: HashSet<String>,
    current: Option<Vec<String>>,
    // Whether an indented `text` line belongs to the set or union before it. The lexer reads this.
    indent: bool,
}

impl VennDb {
    pub fn set_diagram_title(&mut self, title: String) {
        self.title = Some(title);
    }

    // A set without a size gets 10, and an overlap of n sets 10 / n².
    pub fn add_subset_data(&mut self, ids: &[String], label: Option<String>, size: Option<f64>) {
        let sets = normalize_ids(ids);
        if sets.len() == 1 {
            self.known.insert(sets[0].clone());
        }
        self.current = Some(sets.clone());
        let n = ids.len() as f64;
        self.subsets.push(VennSubset {
            sets,
            size: size.unwrap_or(10.0 / (n * n)),
            label: label.filter(|s| !s.is_empty()).map(|s| normalize_text(&s)),
        });
    }

    pub fn add_text_data(&mut self, ids: &[String], id: &str, label: Option<String>) {
        self.text_nodes.push(VennText {
            sets: normalize_ids(ids),
            id: normalize_text(id),
            label: label.filter(|s| !s.is_empty()).map(|s| normalize_text(&s)),
        });
    }

    pub fn add_style_data(&mut self, ids: &[String], data: &[(String, String)]) {
        let mut styles = OrderMap::new();
        for (key, value) in data {
            let norm = if value.is_empty() { value.clone() } else { normalize_text(value) };
            styles.insert(key.clone(), norm);
        }
        self.styles.push(VennStyle { targets: normalize_ids(ids), styles });
    }

    pub fn validate_union_identifiers(&self, ids: &[String]) -> Result<()> {
        let unknown: Vec<String> = ids
            .iter()
            .map(|id| normalize_text(id))
            .filter(|id| !self.known.contains(id))
            .collect();
        if !unknown.is_empty() {
            return Err(PeleError::new(format!("unknown set identifier: {}", unknown.join(", ")), ErrorCode::Semantic).of("venn"));
        }
        Ok(())
    }

    pub fn get_current_sets(&self) -> Option<&[String]> {
        self.current.as_deref()
    }

    pub fn get_indent_mode(&self) -> bool {
        self.indent
    }

    pub fn set_indent_mode(&mut self, enabled: bool) {
        self.indent = enabled;
    }
}
