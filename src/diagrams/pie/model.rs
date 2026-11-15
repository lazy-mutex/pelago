use super::parser::PieAst;
use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::svg::builder::js_number;

#[derive(Debug, Clone)]
pub struct PieModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub show_data: bool,
    pub sections: Vec<(String, f64)>,
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

pub fn build_pie(ast: PieAst, title: Option<String>) -> Result<PieModel> {
    let mut sections: Vec<(String, f64)> = Vec::new();
    for (label, value) in ast.sections {
        if value < 0.0 {
            return Err(PeleError::new(
                format!(
                    "\"{}\" has invalid value: {}. Negative values are not allowed in pie charts. All slice values must be >= 0.",
                    label,
                    js_number(value)
                ),
                ErrorCode::Semantic,
            )
            .of("pie"));
        }
        let clean_val = if value == 0.0 { 0.0 } else { value };
        if !sections.iter().any(|(l, _)| *l == label) {
            sections.push((label, clean_val));
        }
    }
    Ok(PieModel {
        title: ast.title.filter(|s| !s.is_empty()).or_else(|| title.filter(|s| !s.is_empty())),
        acc_title: ast
            .acc_title
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches(is_space).to_string()),
        acc_descr: ast
            .acc_descr
            .filter(|s| !s.is_empty())
            .map(|s| collapse_nl_spaces(&s)),
        show_data: ast.show_data,
        sections,
    })
}
