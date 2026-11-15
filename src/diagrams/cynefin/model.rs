use std::collections::HashMap;

use crate::chars::is_space;

use super::parser::{CynefinAst, DomainName};

#[derive(Debug, Clone)]
pub struct CynefinItem {
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct CynefinDomain {
    pub name: DomainName,
    pub items: Vec<CynefinItem>,
}

#[derive(Debug, Clone)]
pub struct CynefinTransition {
    pub from: DomainName,
    pub to: DomainName,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CynefinModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub domains: HashMap<DomainName, CynefinDomain>,
    pub transitions: Vec<CynefinTransition>,
}

fn clean_acc_descr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut after_nl = false;
    for ch in s.chars() {
        if after_nl && is_space(ch) {
            continue;
        } else if ch == '\n' {
            out.push('\n');
            after_nl = true;
        } else {
            out.push(ch);
            after_nl = false;
        }
    }
    out
}

pub fn build_cynefin(ast: CynefinAst, title: Option<String>) -> CynefinModel {
    let mut domains: HashMap<DomainName, CynefinDomain> = HashMap::new();
    for block in ast.domains {
        let items = block
            .items
            .into_iter()
            .map(|label| CynefinItem { label })
            .collect();
        domains.insert(block.domain, CynefinDomain { name: block.domain, items });
    }

    let transitions: Vec<CynefinTransition> = ast
        .transitions
        .into_iter()
        .filter(|tr| tr.from != tr.to)
        .map(|tr| CynefinTransition {
            from: tr.from,
            to: tr.to,
            label: tr.label.filter(|l| !l.is_empty()),
        })
        .collect();

    let resolved_title = ast.title.filter(|t| !t.is_empty()).or(title);
    let acc_title = ast
        .acc_title
        .filter(|t| !t.is_empty())
        .map(|t| t.trim_start_matches(is_space).to_string());
    let acc_descr = ast
        .acc_descr
        .filter(|d| !d.is_empty())
        .map(|d| clean_acc_descr(&d));

    CynefinModel {
        title: resolved_title,
        acc_title,
        acc_descr,
        domains,
        transitions,
    }
}
