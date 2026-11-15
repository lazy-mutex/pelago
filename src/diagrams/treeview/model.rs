use std::collections::HashMap;

use crate::chars::is_space;
use crate::errors::{locate, syntax_error, PeleError, Result};

use super::box_drawing::{line_start, preprocess_box_drawing, remap_error_lines};
use super::parser::{parse_tree_view, TreeViewAst};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    File,
    Directory,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub id: usize,
    pub level: isize,
    pub name: String,
    pub node_type: NodeType,
    pub icon: Option<String>,
    pub css_class: Option<String>,
    pub description: Option<String>,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone)]
pub struct TreeViewModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub root: TreeNode,
    pub count: usize,
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

pub fn build_tree_view(ast: TreeViewAst, title: Option<String>) -> TreeViewModel {
    let mut count = 1usize;
    let root = TreeNode {
        id: 0,
        level: -1,
        name: "/".to_string(),
        node_type: NodeType::Directory,
        icon: None,
        css_class: None,
        description: None,
        children: Vec::new(),
    };
    let mut stack: Vec<TreeNode> = vec![root];

    let flush_one = |stack: &mut Vec<TreeNode>| {
        let completed = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(completed);
    };

    for ast_node in ast.nodes {
        let level = ast_node.indent.unwrap_or(0) as isize;
        let mut name = ast_node.name;
        let is_directory = name.ends_with('/');
        if is_directory {
            name.pop();
        }
        let icon = ast_node
            .icon_annotation
            .map(|ic| if ic.is_empty() { "none".to_string() } else { ic });
        let css_class = ast_node.class_annotation.filter(|s| !s.is_empty());
        let description = ast_node.desc_annotation.filter(|s| !s.is_empty());

        while stack.len() > 1 && level <= stack.last().unwrap().level {
            flush_one(&mut stack);
        }
        let node = TreeNode {
            id: count,
            level,
            name,
            node_type: if is_directory { NodeType::Directory } else { NodeType::File },
            icon,
            css_class,
            description,
            children: Vec::new(),
        };
        count += 1;
        stack.push(node);
    }

    while stack.len() > 1 {
        flush_one(&mut stack);
    }
    let root = stack.pop().unwrap();

    let resolved_title = ast.title.filter(|t| !t.is_empty()).or(title);
    let acc_title = ast
        .acc_title
        .filter(|t| !t.is_empty())
        .map(|t| t.trim_start_matches(is_space).to_string());
    let acc_descr = ast
        .acc_descr
        .filter(|d| !d.is_empty())
        .map(|d| clean_acc_descr(&d));

    TreeViewModel {
        title: resolved_title,
        acc_title,
        acc_descr,
        root,
        count,
    }
}

pub fn read_tree_view(source: &str) -> Result<TreeViewAst> {
    let pre = preprocess_box_drawing(source)?;
    match parse_tree_view(&pre.text) {
        Ok(ast) => Ok(ast),
        Err(error) => {
            if pre.text == source {
                return Err(error);
            }
            let Some(&line) = pre.line_map.get(&error.line) else {
                return Err(error);
            };
            let at = if error.column > 0 {
                pre.columns
                    .get(&error.line)
                    .map_or(error.column - 1, |col_fn| col_fn(error.column - 1))
            } else {
                0
            };
            let source_chars: Vec<char> = source.chars().collect();
            let source_offset = line_start(&source_chars, line) + at;
            if error.raw_offset.is_some() {
                return Err(syntax_error(
                    "treeView",
                    &source_chars,
                    source_offset,
                    &error.detail,
                    error.lexical,
                ));
            }
            let written = locate(&source_chars, source_offset);
            let head = error.message.find('\n').unwrap_or(error.message.len());
            let rest = &error.message[head..];
            let mut single_map = HashMap::new();
            single_map.insert(error.line, written.line);
            let remapped_rest = if !error.snippet.is_empty() {
                rest.replacen(&error.snippet, &written.snippet, 1)
            } else {
                rest.to_string()
            };
            let message = format!("{}{}", remap_error_lines(&error.message[..head], &single_map), remapped_rest);
            let mut relocated = PeleError::new(message, error.code);
            relocated.diagram = error.diagram;
            relocated.line = written.line;
            relocated.column = if error.column > 0 { written.column } else { 0 };
            relocated.snippet = if !error.snippet.is_empty() { written.snippet } else { String::new() };
            Err(relocated)
        }
    }
}
