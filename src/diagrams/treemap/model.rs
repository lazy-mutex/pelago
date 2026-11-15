use std::collections::HashMap;

use crate::chars::is_space;

use super::parser::{TreemapAst, TreemapItemAst, TreemapRowKind};

#[derive(Debug, Clone)]
pub struct TreemapNode {
    pub name: String,
    pub children: Option<Vec<TreemapNode>>,
    pub value: Option<f64>,
    pub class_selector: Option<String>,
    pub css_compiled_styles: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct TreemapItem {
    pub level: usize,
    pub name: String,
    pub is_leaf: bool,
    pub value: Option<f64>,
    pub class_selector: Option<String>,
    pub css_compiled_styles: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct TreemapModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub roots: Vec<TreemapNode>,
    pub classes: HashMap<String, Vec<String>>,
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

pub fn build_hierarchy(items: Vec<TreemapItem>) -> Vec<TreemapNode> {
    let mut roots: Vec<TreemapNode> = Vec::new();
    // Each stack entry holds `(level, node)` while building children, flushed into parent on pop!
    let mut stack: Vec<(usize, TreemapNode)> = Vec::new();

    let flush_one = |stack: &mut Vec<(usize, TreemapNode)>, roots: &mut Vec<TreemapNode>| {
        let (_, completed) = stack.pop().unwrap();
        if let Some((_, parent)) = stack.last_mut() {
            parent.children.as_mut().unwrap().push(completed);
        } else {
            roots.push(completed);
        }
    };

    for item in items {
        while stack.last().is_some_and(|(lvl, _)| *lvl >= item.level) {
            flush_one(&mut stack, &mut roots);
        }
        let node = TreemapNode {
            name: item.name,
            children: if item.is_leaf { None } else { Some(Vec::new()) },
            value: if item.is_leaf { item.value } else { None },
            class_selector: item.class_selector,
            css_compiled_styles: item.css_compiled_styles,
        };
        if item.is_leaf {
            if let Some((_, parent)) = stack.last_mut() {
                parent.children.as_mut().unwrap().push(node);
            } else {
                roots.push(node);
            }
        } else {
            stack.push((item.level, node));
        }
    }
    while !stack.is_empty() {
        flush_one(&mut stack, &mut roots);
    }
    roots
}

fn split_class_styles(raw: &str) -> Vec<String> {
    // Equivalent to raw.replace(/\\,/g, '§§§').replace(/,/g, ';').replace(/§§§/g, ',').split(';')
    let chars: Vec<char> = raw.chars().collect();
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\\' && chars.get(i + 1) == Some(&',') {
            cur.push(',');
            i += 2;
        } else if chars[i] == ',' || chars[i] == ';' {
            parts.push(std::mem::take(&mut cur));
            i += 1;
        } else {
            cur.push(chars[i]);
            i += 1;
        }
    }
    parts.push(cur);
    parts
}

pub fn build_treemap(ast: TreemapAst, title: Option<String>) -> TreemapModel {
    let mut classes: HashMap<String, Vec<String>> = HashMap::new();
    for row in &ast.rows {
        if let TreemapRowKind::ClassDef { class_name, style_text, .. } = row {
            let styles = classes.entry(class_name.clone()).or_default();
            for s in split_class_styles(style_text.as_deref().unwrap_or("")) {
                styles.push(s);
            }
        }
    }

    let mut items: Vec<TreemapItem> = Vec::new();
    for row in ast.rows {
        if let TreemapRowKind::Row { indent, item } = row {
            let (name, is_leaf, value, class_selector) = match item {
                TreemapItemAst::Section { name, class_selector } => (name, false, None, class_selector),
                TreemapItemAst::Leaf { name, value, class_selector } => (name, true, Some(value), class_selector),
            };
            let css_compiled_styles = class_selector
                .as_ref()
                .and_then(|sel| classes.get(sel))
                .filter(|s| !s.is_empty())
                .cloned();
            items.push(TreemapItem {
                level: indent.unwrap_or(0),
                name,
                is_leaf,
                value,
                class_selector,
                css_compiled_styles,
            });
        }
    }

    let resolved_title = ast.title.filter(|t| !t.is_empty()).or(title);
    let acc_title = ast
        .acc_title
        .filter(|t| !t.is_empty())
        .map(|t| t.trim_start_matches(is_space).to_string());
    let acc_descr = ast
        .acc_descr
        .filter(|d| !d.is_empty())
        .map(|d| clean_acc_descr(&d));

    TreemapModel {
        title: resolved_title,
        acc_title,
        acc_descr,
        roots: build_hierarchy(items),
        classes,
    }
}
