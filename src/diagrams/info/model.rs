use crate::chars::is_space;
use super::parser::InfoAst;

#[derive(Debug, Clone)]
pub struct InfoModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
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

pub fn build_info(ast: InfoAst, title: Option<String>) -> InfoModel {
    InfoModel {
        title: ast.title.filter(|s| !s.is_empty()).or_else(|| title.filter(|s| !s.is_empty())),
        acc_title: ast
            .acc_title
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches(is_space).to_string()),
        acc_descr: ast
            .acc_descr
            .filter(|s| !s.is_empty())
            .map(|s| collapse_nl_spaces(&s)),
    }
}
