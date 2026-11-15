use std::collections::HashMap;

use crate::chars::is_space;

#[derive(Debug, Clone)]
pub struct C4Named {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub enum C4Attr {
    Positional(String),
    Named(C4Named),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementId {
    Shape(usize),
    Boundary(usize),
}

#[derive(Debug, Clone, Default)]
pub struct C4Element {
    pub alias: String,
    pub label: String,
    pub alias_attr: Option<C4Named>,
    pub label_attr: Option<C4Named>,
    pub parent: Option<usize>,
    pub elem_type: Option<String>,
    pub descr: Option<String>,
    pub techn: Option<String>,
    pub sprite: Option<String>,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub bg_color: Option<String>,
    pub font_color: Option<String>,
    pub border_color: Option<String>,
    pub shadowing: Option<String>,
    pub shape: Option<String>,
    pub legend_text: Option<String>,
    pub legend_sprite: Option<String>,
    pub extra: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct C4Shape {
    pub elem: C4Element,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct C4Boundary {
    pub elem: C4Element,
    pub node_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct C4Rel {
    pub rel_type: String,
    pub from: String,
    pub to: String,
    pub label: String,
    pub from_attr: Option<C4Named>,
    pub to_attr: Option<C4Named>,
    pub label_attr: Option<C4Named>,
    pub techn: Option<String>,
    pub descr: Option<String>,
    pub sprite: Option<String>,
    pub tags: Option<String>,
    pub link: Option<String>,
    pub text_color: Option<String>,
    pub line_color: Option<String>,
    pub offset_x: Option<f64>,
    pub offset_y: Option<f64>,
    pub extra: HashMap<String, String>,
}

// Matches JavaScript `parseInt(str)` without an explicit radix.
fn js_parse_int(raw: &str) -> f64 {
    let trimmed = raw.trim_start_matches(is_space);
    let (sign, rest) = if let Some(r) = trimmed.strip_prefix('-') {
        (-1.0, r)
    } else if let Some(r) = trimmed.strip_prefix('+') {
        (1.0, r)
    } else {
        (1.0, trimmed)
    };
    if let Some(hex_rest) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        let mut acc = 0.0f64;
        let mut any = false;
        for b in hex_rest.bytes() {
            let d = match b {
                b'0'..=b'9' => (b - b'0') as f64,
                b'a'..=b'f' => (b - b'a' + 10) as f64,
                b'A'..=b'F' => (b - b'A' + 10) as f64,
                _ => break,
            };
            acc = acc * 16.0 + d;
            any = true;
        }
        return if any { sign * acc } else { f64::NAN };
    }
    let mut acc = 0.0f64;
    let mut any = false;
    for b in rest.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        acc = acc * 10.0 + (b - b'0') as f64;
        any = true;
    }
    if any { sign * acc } else { f64::NAN }
}

fn set_elem_field(el: &mut C4Element, key: &str, value: String) -> bool {
    match key {
        "label" => el.label = value,
        "type" => el.elem_type = Some(value),
        "descr" => el.descr = Some(value),
        "techn" => el.techn = Some(value),
        "sprite" => el.sprite = Some(value),
        "tags" => el.tags = Some(value),
        "link" => el.link = Some(value),
        "bgColor" => el.bg_color = Some(value),
        "fontColor" => el.font_color = Some(value),
        "borderColor" => el.border_color = Some(value),
        "shadowing" => el.shadowing = Some(value),
        "shape" => el.shape = Some(value),
        "legendText" => el.legend_text = Some(value),
        "legendSprite" => el.legend_sprite = Some(value),
        _ => return false,
    }
    true
}

fn named_elem(el: &mut C4Element, attr: &C4Named) {
    if !set_elem_field(el, &attr.key, attr.value.clone()) {
        el.extra.insert(attr.key.clone(), attr.value.clone());
    }
}

fn put_elem(el: &mut C4Element, field: &str, attr: Option<&C4Attr>) {
    let Some(attr) = attr else { return };
    match attr {
        C4Attr::Positional(val) => {
            set_elem_field(el, field, val.clone());
        }
        C4Attr::Named(named) => named_elem(el, named),
    }
}

fn text_elem(el: &mut C4Element, field: &str, attr: Option<&C4Attr>) {
    match attr {
        None => match field {
            "descr" => {
                el.descr.get_or_insert_with(String::new);
            }
            "techn" => {
                el.techn.get_or_insert_with(String::new);
            }
            _ => {}
        },
        Some(_) => put_elem(el, field, attr),
    }
}

fn label_elem(el: &mut C4Element, attr: Option<&C4Attr>) {
    el.label_attr = None;
    match attr {
        Some(C4Attr::Named(named)) => {
            el.label.clear();
            el.label_attr = Some(named.clone());
            named_elem(el, named);
        }
        Some(C4Attr::Positional(val)) => {
            el.label = val.clone();
        }
        None => {
            el.label.clear();
        }
    }
}

fn set_rel_field(rel: &mut C4Rel, key: &str, value: String) -> bool {
    match key {
        "label" => rel.label = value,
        "descr" => rel.descr = Some(value),
        "techn" => rel.techn = Some(value),
        "sprite" => rel.sprite = Some(value),
        "tags" => rel.tags = Some(value),
        "link" => rel.link = Some(value),
        "textColor" => rel.text_color = Some(value),
        "lineColor" => rel.line_color = Some(value),
        _ => return false,
    }
    true
}

fn named_rel(rel: &mut C4Rel, attr: &C4Named) {
    if attr.key == "offsetX" {
        rel.offset_x = Some(js_parse_int(&attr.value));
    } else if attr.key == "offsetY" {
        rel.offset_y = Some(js_parse_int(&attr.value));
    } else if !set_rel_field(rel, &attr.key, attr.value.clone()) {
        rel.extra.insert(attr.key.clone(), attr.value.clone());
    }
}

fn put_rel(rel: &mut C4Rel, field: &str, attr: Option<&C4Attr>) {
    let Some(attr) = attr else { return };
    match attr {
        C4Attr::Positional(val) => {
            set_rel_field(rel, field, val.clone());
        }
        C4Attr::Named(named) => named_rel(rel, named),
    }
}

fn text_rel(rel: &mut C4Rel, field: &str, attr: Option<&C4Attr>) {
    match attr {
        None => match field {
            "descr" => {
                rel.descr.get_or_insert_with(String::new);
            }
            "techn" => {
                rel.techn.get_or_insert_with(String::new);
            }
            _ => {}
        },
        Some(_) => put_rel(rel, field, attr),
    }
}

fn label_rel(rel: &mut C4Rel, attr: Option<&C4Attr>) {
    rel.label_attr = None;
    match attr {
        Some(C4Attr::Named(named)) => {
            rel.label.clear();
            rel.label_attr = Some(named.clone());
            named_rel(rel, named);
        }
        Some(C4Attr::Positional(val)) => {
            rel.label = val.clone();
        }
        None => {
            rel.label.clear();
        }
    }
}

pub struct C4Db {
    pub c4_type: Option<String>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub shapes: Vec<C4Shape>,
    // The first boundary is the unnamed one that holds everything at the top level.
    pub boundaries: Vec<C4Boundary>,
    pub rels: Vec<C4Rel>,
    pub shape_in_row: usize,
    pub boundary_in_row: usize,

    shape_index: HashMap<String, usize>,
    // The top-level boundary is not listed, so a boundary a diagram names `global` is its own box.
    boundary_index: HashMap<String, usize>,
    rel_index: HashMap<String, HashMap<String, usize>>,
    current: usize,
    stack: Vec<usize>,
}

impl Default for C4Db {
    fn default() -> Self {
        Self::new()
    }
}

impl C4Db {
    pub fn new() -> Self {
        let global = C4Boundary {
            elem: C4Element {
                alias: "global".to_string(),
                label: "global".to_string(),
                elem_type: Some("global".to_string()),
                parent: None,
                ..Default::default()
            },
            node_type: None,
        };
        Self {
            c4_type: None,
            title: None,
            acc_title: None,
            acc_descr: None,
            shapes: Vec::new(),
            boundaries: vec![global],
            rels: Vec::new(),
            shape_in_row: 4,
            boundary_in_row: 2,
            shape_index: HashMap::new(),
            boundary_index: HashMap::new(),
            rel_index: HashMap::new(),
            current: 0,
            stack: Vec::new(),
        }
    }

    pub fn set_c4_type(&mut self, c4_type: String) {
        self.c4_type = Some(c4_type);
    }

    pub fn set_title(&mut self, title: String) {
        self.title = Some(title);
    }

    // `descr.replace(/\n\s+/g, '\n')`
    pub fn set_acc_description(&mut self, descr: &str) {
        let mut out = String::with_capacity(descr.len());
        let mut chars = descr.chars().peekable();
        while let Some(ch) = chars.next() {
            out.push(ch);
            if ch == '\n' {
                while chars.peek().copied().is_some_and(is_space) {
                    chars.next();
                }
            }
        }
        self.acc_descr = Some(out);
    }

    fn ensure_shape(&mut self, alias: Option<&C4Attr>, title: Option<&C4Attr>) -> usize {
        let existing = match alias {
            Some(C4Attr::Positional(a)) => self.shape_index.get(a).copied(),
            _ => None,
        };
        let idx = match existing {
            Some(idx) => idx,
            None => {
                let mut elem = C4Element::default();
                match alias {
                    Some(C4Attr::Positional(a)) => {
                        elem.alias = a.clone();
                    }
                    Some(C4Attr::Named(named)) => {
                        elem.alias_attr = Some(named.clone());
                        named_elem(&mut elem, named);
                    }
                    None => {}
                }
                let idx = self.shapes.len();
                if let Some(C4Attr::Positional(a)) = alias {
                    self.shape_index.insert(a.clone(), idx);
                }
                self.shapes.push(C4Shape { elem, kind: String::new() });
                idx
            }
        };
        label_elem(&mut self.shapes[idx].elem, title);
        idx
    }

    fn ensure_boundary(&mut self, alias: Option<&C4Attr>, title: Option<&C4Attr>) -> usize {
        let existing = match alias {
            Some(C4Attr::Positional(a)) => self.boundary_index.get(a).copied(),
            _ => None,
        };
        let idx = match existing {
            Some(idx) => idx,
            None => {
                let mut elem = C4Element::default();
                match alias {
                    Some(C4Attr::Positional(a)) => {
                        elem.alias = a.clone();
                    }
                    Some(C4Attr::Named(named)) => {
                        elem.alias_attr = Some(named.clone());
                        named_elem(&mut elem, named);
                    }
                    None => {}
                }
                let idx = self.boundaries.len();
                if let Some(C4Attr::Positional(a)) = alias {
                    self.boundary_index.insert(a.clone(), idx);
                }
                self.boundaries.push(C4Boundary { elem, node_type: None });
                idx
            }
        };
        label_elem(&mut self.boundaries[idx].elem, title);
        idx
    }

    // alias, label, ?descr, ?sprite, ?tags, $link
    pub fn add_person_or_system(&mut self, kind: &str, attrs: &[C4Attr]) {
        let idx = self.ensure_shape(attrs.first(), attrs.get(1));
        text_elem(&mut self.shapes[idx].elem, "descr", attrs.get(2));
        self.finish_shape(idx, kind, attrs, 3);
    }

    // alias, label, ?techn, ?descr, ?sprite, ?tags, $link
    pub fn add_container(&mut self, kind: &str, attrs: &[C4Attr]) {
        let idx = self.ensure_shape(attrs.first(), attrs.get(1));
        text_elem(&mut self.shapes[idx].elem, "techn", attrs.get(2));
        text_elem(&mut self.shapes[idx].elem, "descr", attrs.get(3));
        self.finish_shape(idx, kind, attrs, 4);
    }

    pub fn add_component(&mut self, kind: &str, attrs: &[C4Attr]) {
        self.add_container(kind, attrs);
    }

    fn finish_shape(&mut self, idx: usize, kind: &str, attrs: &[C4Attr], at: usize) {
        let current = self.current;
        let shape = &mut self.shapes[idx];
        put_elem(&mut shape.elem, "sprite", attrs.get(at));
        put_elem(&mut shape.elem, "tags", attrs.get(at + 1));
        put_elem(&mut shape.elem, "link", attrs.get(at + 2));
        shape.kind = kind.to_string();
        shape.elem.parent = Some(current);
    }

    fn boundary(&mut self, attrs: &[C4Attr], fallback: &str) -> usize {
        let idx = self.ensure_boundary(attrs.first(), attrs.get(1));
        match attrs.get(2) {
            None => self.boundaries[idx].elem.elem_type = Some(fallback.to_string()),
            Some(_) => put_elem(&mut self.boundaries[idx].elem, "type", attrs.get(2)),
        }
        idx
    }

    fn open(&mut self, idx: usize) {
        let current = self.current;
        self.boundaries[idx].elem.parent = Some(current);
        self.stack.push(current);
        self.current = idx;
    }

    // alias, label, ?type, ?tags, $link
    pub fn add_person_or_system_boundary(&mut self, attrs: &[C4Attr], fallback: &str) {
        let idx = self.boundary(attrs, fallback);
        put_elem(&mut self.boundaries[idx].elem, "tags", attrs.get(3));
        put_elem(&mut self.boundaries[idx].elem, "link", attrs.get(4));
        self.open(idx);
    }

    pub fn add_container_boundary(&mut self, attrs: &[C4Attr]) {
        self.add_person_or_system_boundary(attrs, "container");
    }

    // alias, label, ?type, ?descr, ?sprite, ?tags, $link
    pub fn add_deployment_node(&mut self, node_type: &str, attrs: &[C4Attr]) {
        let idx = self.boundary(attrs, "node");
        let b = &mut self.boundaries[idx];
        text_elem(&mut b.elem, "descr", attrs.get(3));
        put_elem(&mut b.elem, "sprite", attrs.get(4));
        put_elem(&mut b.elem, "tags", attrs.get(5));
        put_elem(&mut b.elem, "link", attrs.get(6));
        b.node_type = Some(node_type.to_string());
        self.open(idx);
    }

    pub fn pop_boundary_parse_stack(&mut self) {
        self.current = self.stack.pop().unwrap_or(0);
    }

    // from, to, label, ?techn, ?descr, ?sprite, ?tags, $link
    pub fn add_rel(&mut self, rel_type: &str, attrs: &[C4Attr]) {
        let (Some(from), Some(to), Some(_)) = (attrs.first(), attrs.get(1), attrs.get(2)) else {
            return;
        };
        let known = match (from, to) {
            (C4Attr::Positional(f), C4Attr::Positional(t)) => Some((f.as_str(), t.as_str())),
            _ => None,
        };
        let existing = known.and_then(|(f, t)| self.rel_index.get(f).and_then(|m| m.get(t)).copied());
        let idx = match existing {
            Some(idx) => idx,
            None => {
                let mut rel = C4Rel {
                    rel_type: rel_type.to_string(),
                    from: String::new(),
                    to: String::new(),
                    label: String::new(),
                    from_attr: None,
                    to_attr: None,
                    label_attr: None,
                    techn: None,
                    descr: None,
                    sprite: None,
                    tags: None,
                    link: None,
                    text_color: None,
                    line_color: None,
                    offset_x: None,
                    offset_y: None,
                    extra: HashMap::new(),
                };
                match from {
                    C4Attr::Positional(f) => rel.from = f.clone(),
                    C4Attr::Named(n) => rel.from_attr = Some(n.clone()),
                }
                match to {
                    C4Attr::Positional(t) => rel.to = t.clone(),
                    C4Attr::Named(n) => rel.to_attr = Some(n.clone()),
                }
                let idx = self.rels.len();
                if let Some((f, t)) = known {
                    self.rel_index.entry(f.to_string()).or_default().insert(t.to_string(), idx);
                }
                self.rels.push(rel);
                idx
            }
        };
        let rel = &mut self.rels[idx];
        rel.rel_type = rel_type.to_string();
        label_rel(rel, attrs.get(2));
        text_rel(rel, "techn", attrs.get(3));
        text_rel(rel, "descr", attrs.get(4));
        put_rel(rel, "sprite", attrs.get(5));
        put_rel(rel, "tags", attrs.get(6));
        put_rel(rel, "link", attrs.get(7));
    }

    pub fn get_c4_shape(&self, alias: &str) -> Option<ElementId> {
        if let Some(&idx) = self.shape_index.get(alias) {
            Some(ElementId::Shape(idx))
        } else {
            self.boundary_index.get(alias).copied().map(ElementId::Boundary)
        }
    }

    // elementName, ?bgColor, ?fontColor, ?borderColor, ?shadowing, ?shape, ?sprite, ?techn, ?legendText, ?legendSprite
    pub fn update_el_style(&mut self, attrs: &[C4Attr]) {
        let Some(C4Attr::Positional(name)) = attrs.first() else { return };
        let Some(id) = self.get_c4_shape(name) else { return };
        let elem = match id {
            ElementId::Shape(idx) => &mut self.shapes[idx].elem,
            ElementId::Boundary(idx) => &mut self.boundaries[idx].elem,
        };
        put_elem(elem, "bgColor", attrs.get(1));
        put_elem(elem, "fontColor", attrs.get(2));
        put_elem(elem, "borderColor", attrs.get(3));
        put_elem(elem, "shadowing", attrs.get(4));
        put_elem(elem, "shape", attrs.get(5));
        put_elem(elem, "sprite", attrs.get(6));
        put_elem(elem, "techn", attrs.get(7));
        put_elem(elem, "legendText", attrs.get(8));
        put_elem(elem, "legendSprite", attrs.get(9));
    }

    // from, to, ?textColor, ?lineColor, ?offsetX, ?offsetY
    pub fn update_rel_style(&mut self, attrs: &[C4Attr]) {
        let (Some(C4Attr::Positional(from)), Some(C4Attr::Positional(to))) = (attrs.first(), attrs.get(1)) else {
            return;
        };
        let Some(&idx) = self.rel_index.get(from.as_str()).and_then(|m| m.get(to.as_str())) else {
            return;
        };
        let rel = &mut self.rels[idx];
        put_rel(rel, "textColor", attrs.get(2));
        put_rel(rel, "lineColor", attrs.get(3));
        match attrs.get(4) {
            Some(C4Attr::Positional(x)) => rel.offset_x = Some(js_parse_int(x)),
            Some(C4Attr::Named(n)) => named_rel(rel, n),
            None => {}
        }
        match attrs.get(5) {
            Some(C4Attr::Positional(y)) => rel.offset_y = Some(js_parse_int(y)),
            Some(C4Attr::Named(n)) => named_rel(rel, n),
            None => {}
        }
    }

    // ?c4ShapeInRow, ?c4BoundaryInRow
    pub fn update_layout_config(&mut self, attrs: &[C4Attr]) {
        let count = |attr: Option<&C4Attr>| -> f64 {
            match attr {
                None => f64::NAN,
                Some(C4Attr::Positional(s)) => js_parse_int(s),
                Some(C4Attr::Named(n)) => js_parse_int(&n.value),
            }
        };
        let shapes = count(attrs.first());
        let boundaries = count(attrs.get(1));
        if shapes >= 1.0 {
            self.shape_in_row = shapes.min(usize::MAX as f64) as usize;
        }
        if boundaries >= 1.0 {
            self.boundary_in_row = boundaries.min(usize::MAX as f64) as usize;
        }
    }
}
