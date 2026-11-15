use std::collections::{HashMap, HashSet};

use crate::errors::{ErrorCode, PeleError, Result};

const MAX_SPACES: usize = 100_000;

#[derive(Debug, Clone, Default)]
pub struct RawBlock {
    pub id: Option<String>,
    pub block_type: Option<String>,
    pub label: Option<String>,
    pub children: Vec<Statement>,
    pub columns: Option<i64>,
    pub width_in_columns: Option<i64>,
    pub width: Option<i64>,
    pub directions: Vec<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub arrow_type_start: Option<String>,
    pub arrow_type_end: Option<String>,
    pub thickness: Option<String>,
    pub pattern: Option<String>,
    pub css: Option<String>,
    pub style_class: Option<String>,
    pub styles_str: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Single(RawBlock),
    Chain(Vec<RawBlock>),
}

#[derive(Debug, Clone, Default)]
pub struct Block {
    pub id: String,
    pub block_type: Option<String>,
    pub label: Option<String>,
    // Indices into `BlockDb::arena` for composite children.
    pub children: Vec<usize>,
    pub columns: Option<i64>,
    pub width_in_columns: Option<i64>,
    pub width: Option<i64>,
    pub directions: Vec<String>,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub color_index: Option<usize>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub arrow_type_start: Option<String>,
    pub arrow_type_end: Option<String>,
    pub thickness: Option<String>,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BlockClassDef {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

pub fn type_str_2_type(type_str: Option<&str>) -> &'static str {
    match type_str {
        Some("[]") => "square",
        Some("()") => "round",
        Some("(())") => "circle",
        Some(">]") => "rect_left_inv_arrow",
        Some("{}") => "diamond",
        Some("{{}}") => "hexagon",
        Some("([])") => "stadium",
        Some("[[]]") => "subroutine",
        Some("[()]") => "cylinder",
        Some("((()))") => "doublecircle",
        Some("[//]") => "lean_right",
        Some("[\\\\]") => "lean_left",
        Some("[/\\]") => "trapezoid",
        Some("[\\/]") => "inv_trapezoid",
        Some("<[]>") => "block_arrow",
        _ => "na",
    }
}

pub fn edge_str_to_edge_data(type_str: &str) -> &'static str {
    match type_str.trim().chars().next_back() {
        Some('x') => "arrow_cross",
        Some('o') => "arrow_circle",
        Some('>') => "arrow_point",
        _ => "",
    }
}

pub fn edge_str_to_edge_start_data(type_str: &str) -> &'static str {
    match type_str.trim().chars().next() {
        Some('x') => "arrow_cross",
        Some('o') => "arrow_circle",
        Some('<') => "arrow_point",
        _ => "arrow_open",
    }
}

pub fn edge_str_to_thickness(type_str: &str) -> &'static str {
    if type_str.contains("==") { "thick" } else { "normal" }
}

pub fn edge_str_to_pattern(type_str: &str) -> &'static str {
    if type_str.contains(".-") { "dotted" } else { "solid" }
}

struct Frame {
    list: Vec<RawBlock>,
    at: usize,
    parent_idx: usize,
    children: Vec<usize>,
}

fn make_frame(statements: Vec<Statement>, parent_idx: usize) -> Frame {
    let mut list = Vec::new();
    for stmt in statements {
        match stmt {
            Statement::Single(b) => list.push(b),
            Statement::Chain(bs) => list.extend(bs),
        }
    }
    Frame {
        list,
        at: 0,
        parent_idx,
        children: Vec::new(),
    }
}

pub struct BlockDb {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    // `arena[0]` is `root`.
    pub arena: Vec<Block>,
    pub edges: Vec<Block>,
    pub classes: HashMap<String, BlockClassDef>,
    pub by_id: HashMap<String, usize>,

    pending: HashSet<String>,
    edge_count: HashMap<String, usize>,
    ids: usize,
    colors: usize,
    spaces: usize,
}

impl Default for BlockDb {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockDb {
    pub fn new() -> Self {
        let root = Block {
            id: "root".to_string(),
            block_type: Some("composite".to_string()),
            columns: Some(-1),
            ..Default::default()
        };
        Self {
            title: None,
            acc_title: None,
            acc_descr: None,
            arena: vec![root],
            edges: Vec::new(),
            classes: HashMap::new(),
            by_id: HashMap::new(),
            pending: HashSet::new(),
            edge_count: HashMap::new(),
            ids: 0,
            colors: 0,
            spaces: 0,
        }
    }

    pub fn generate_id(&mut self) -> String {
        self.ids += 1;
        format!("id-{}", self.ids)
    }

    pub fn add_style_class(&mut self, id: &str, style_attributes: &str) {
        let found = self.classes.entry(id.to_string()).or_insert_with(|| BlockClassDef {
            id: id.to_string(),
            styles: Vec::new(),
            text_styles: Vec::new(),
        });
        for attribute in style_attributes.split(',') {
            let fixed = match attribute.find(';') {
                None => attribute.trim().to_string(),
                Some(semi) => format!("{}{}", &attribute[..semi], &attribute[semi + 1..]).trim().to_string(),
            };
            if attribute.contains("color") {
                found
                    .text_styles
                    .push(fixed.replacen("fill", "bgFill", 1).replacen("color", "fill", 1));
            }
            found.styles.push(fixed);
        }
    }

    pub fn add_style_2_node(&mut self, id: &str, styles: &str) {
        let idx = self.named(id);
        self.arena[idx].styles = styles.split(',').map(String::from).collect();
    }

    pub fn set_css_class(&mut self, item_ids: &str, css_class_name: &str) {
        for raw_id in item_ids.split(',') {
            let id = raw_id.trim();
            let idx = self.named(id);
            self.arena[idx].classes.push(css_class_name.to_string());
        }
    }

    fn named(&mut self, id: &str) -> usize {
        if let Some(&idx) = self.by_id.get(id) {
            return idx;
        }
        let idx = self.arena.len();
        self.arena.push(Block {
            id: id.to_string(),
            block_type: Some("na".to_string()),
            ..Default::default()
        });
        self.by_id.insert(id.to_string(), idx);
        self.pending.insert(id.to_string());
        idx
    }

    pub fn set_hierarchy(&mut self, statements: Vec<Statement>) -> Result<()> {
        let mut stack = vec![make_frame(statements, 0)];
        while let Some(f) = stack.last_mut() {
            if f.at == f.list.len() {
                let finished = stack.pop().unwrap();
                self.arena[finished.parent_idx].children = finished.children;
                continue;
            }
            let mut raw = std::mem::take(&mut f.list[f.at]);
            f.at += 1;
            let parent_idx = f.parent_idx;

            match raw.block_type.as_deref() {
                Some("classDef") => {
                    let id = raw.id.unwrap_or_default();
                    let css = raw.css.unwrap_or_default();
                    self.add_style_class(&id, &css);
                    continue;
                }
                Some("applyClass") => {
                    let id = raw.id.unwrap_or_default();
                    let sc = raw.style_class.unwrap_or_default();
                    self.set_css_class(&id, &sc);
                    continue;
                }
                Some("applyStyles") => {
                    if let Some(styles_str) = raw.styles_str.filter(|s| !s.is_empty()) {
                        let id = raw.id.unwrap_or_default();
                        self.add_style_2_node(&id, &styles_str);
                    }
                    continue;
                }
                Some("column-setting") => {
                    self.arena[parent_idx].columns = Some(raw.columns.unwrap_or(-1));
                    continue;
                }
                Some("edge") => {
                    let base_id = raw.id.unwrap_or_default();
                    let count = self.edge_count.get(&base_id).copied().unwrap_or(0) + 1;
                    self.edge_count.insert(base_id.clone(), count);
                    self.edges.push(Block {
                        id: format!("{}-{}", count, base_id),
                        block_type: Some("edge".to_string()),
                        label: raw.label,
                        directions: raw.directions,
                        start: raw.start,
                        end: raw.end,
                        arrow_type_start: raw.arrow_type_start,
                        arrow_type_end: raw.arrow_type_end,
                        thickness: raw.thickness,
                        pattern: raw.pattern,
                        ..Default::default()
                    });
                    continue;
                }
                _ => {}
            }

            let inner = std::mem::take(&mut raw.children);
            let id = match raw.id.take() {
                Some(id) => id,
                None => self.generate_id(),
            };
            let is_composite = raw.block_type.as_deref() == Some("composite");
            let label = match raw.label.take() {
                Some(lbl) if !lbl.is_empty() => lbl,
                _ => {
                    if is_composite {
                        String::new()
                    } else {
                        id.clone()
                    }
                }
            };

            let known_idx = self.by_id.get(&id).copied();
            let fresh = known_idx.is_none() || self.pending.remove(&id);

            let mut block = Block {
                id: id.clone(),
                block_type: raw.block_type,
                label: Some(label.clone()),
                children: Vec::new(),
                columns: raw.columns,
                width_in_columns: raw.width_in_columns,
                width: raw.width,
                directions: raw.directions,
                classes: Vec::new(),
                styles: Vec::new(),
                color_index: None,
                start: raw.start,
                end: raw.end,
                arrow_type_start: raw.arrow_type_start,
                arrow_type_end: raw.arrow_type_end,
                thickness: raw.thickness,
                pattern: raw.pattern,
            };

            let blk_idx = self.arena.len();
            if !fresh {
                if let Some(k_idx) = known_idx {
                    if block.block_type.as_deref() != Some("na") {
                        self.arena[k_idx].block_type = block.block_type.clone();
                    }
                    if label != id {
                        self.arena[k_idx].label = Some(label);
                    }
                }
                self.arena.push(block);
            } else {
                if let Some(k_idx) = known_idx {
                    block.classes = self.arena[k_idx].classes.clone();
                    block.styles = self.arena[k_idx].styles.clone();
                }
                if is_composite {
                    block.color_index = Some(self.colors);
                    self.colors += 1;
                }
                self.arena.push(block);
                self.by_id.insert(id.clone(), blk_idx);
            }

            if self.arena[blk_idx].block_type.as_deref() == Some("space") {
                let width = self.arena[blk_idx].width.unwrap_or(1);
                if width > (MAX_SPACES - self.spaces) as i64 {
                    return Err(PeleError::new(
                        format!("A block diagram can hold at most {} space blocks.", MAX_SPACES),
                        ErrorCode::Limit,
                    )
                    .of("block"));
                }
                let count = width.max(0) as usize;
                self.spaces += count;
                let template = self.arena[blk_idx].clone();
                for j in 0..count {
                    let copy_id = format!("{}-{}", id, j);
                    let copy_idx = self.arena.len();
                    let mut copy = template.clone();
                    copy.id = copy_id.clone();
                    copy.children.clear();
                    self.arena.push(copy);
                    self.by_id.insert(copy_id, copy_idx);
                    stack.last_mut().unwrap().children.push(copy_idx);
                }
            } else if fresh {
                stack.last_mut().unwrap().children.push(blk_idx);
            }

            if !inner.is_empty() {
                stack.push(make_frame(inner, blk_idx));
            }
        }
        Ok(())
    }
}
