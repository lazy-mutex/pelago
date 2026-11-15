use super::shapes::is_valid_shape;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::url::safe_url;
use crate::value::{parse_yaml, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelType {
    #[default]
    Text,
    String,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct FlowText {
    pub text: String,
    pub kind: LabelType,
}

#[derive(Debug, Clone, Default)]
pub struct FlowNode {
    pub id: String,
    pub text: Option<String>,
    pub label_type: LabelType,
    pub shape: Option<String>,
    pub styles: Vec<String>,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub props: Vec<(String, String)>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub icon: Option<String>,
    pub form: Option<String>,
    pub pos: Option<String>,
    pub img: Option<String>,
    pub constraint: Option<String>,
    pub asset_width: Option<f64>,
    pub asset_height: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct FlowEdge {
    pub start: String,
    pub end: String,
    pub kind: Option<String>,
    pub text: String,
    pub label_type: LabelType,
    pub stroke: Option<String>,
    pub length: Option<usize>,
    pub style: Option<Vec<String>>,
    pub classes: Vec<String>,
    pub id: String,
    pub is_user_defined_id: bool,
    pub interpolate: Option<String>,
    pub animate: Option<bool>,
    pub animation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FlowClassDef {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FlowSubgraph {
    pub id: String,
    pub nodes: Vec<String>,
    pub title: String,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub label_type: LabelType,
    pub metadata: Vec<(String, Value)>,
}

#[derive(Debug, Clone, Default)]
pub struct LinkInfo {
    pub kind: String,
    pub stroke: String,
    pub length: Option<usize>,
    pub text: Option<FlowText>,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DocItem {
    Text(String),
    List(Vec<String>),
    Dir(String),
}

// What a vertex statement says about a node. Anything left out leaves the node as it is.
#[derive(Default)]
pub struct Vertex<'a> {
    pub text: Option<&'a FlowText>,
    pub shape: Option<&'a str>,
    pub style: Option<Vec<String>>,
    pub props: Option<(String, String)>,
    pub metadata: Option<&'a str>,
}

fn label_type(kind: Option<&str>) -> LabelType {
    match kind {
        Some("string") => LabelType::String,
        Some("text") => LabelType::Text,
        _ => LabelType::Markdown,
    }
}

fn unquote(text: &str) -> &str {
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

fn destruct_start_link(raw: &str) -> LinkInfo {
    let mut s = raw.trim();
    let mut kind = "arrow_open";
    match s.chars().next() {
        Some('<') => {
            kind = "arrow_point";
            s = &s[1..];
        }
        Some('x') => {
            kind = "arrow_cross";
            s = &s[1..];
        }
        Some('o') => {
            kind = "arrow_circle";
            s = &s[1..];
        }
        _ => {}
    }
    let mut stroke = "normal";
    if s.contains('=') {
        stroke = "thick";
    }
    if s.contains('.') {
        stroke = "dotted";
    }
    LinkInfo { kind: kind.into(), stroke: stroke.into(), ..Default::default() }
}

fn destruct_end_link(raw: &str) -> LinkInfo {
    let s = raw.trim();
    let last = s.chars().next_back();
    let mut line = match last {
        Some(c) => &s[..s.len() - c.len_utf8()],
        None => s,
    };
    let mut kind = "arrow_open".to_string();
    let pairs = [('x', "arrow_cross"), ('>', "arrow_point"), ('o', "arrow_circle")];
    let openers = [('x', 'x'), ('>', '<'), ('o', 'o')];
    for ((end, name), (_, open)) in pairs.iter().zip(openers) {
        if last == Some(*end) {
            kind = name.to_string();
            if s.starts_with(open) {
                kind = format!("double_{}", kind);
                line = line.get(1..).unwrap_or("");
            }
        }
    }
    let mut stroke = "normal";
    let mut length = line.chars().count().saturating_sub(1);
    if line.starts_with('=') {
        stroke = "thick";
    }
    if line.starts_with('~') {
        stroke = "invisible";
    }
    let dots = line.matches('.').count();
    if dots > 0 {
        stroke = "dotted";
        length = dots;
    }
    LinkInfo { kind, stroke: stroke.into(), length: Some(length), ..Default::default() }
}

fn semantic(message: String) -> PeleError {
    PeleError::new(message, ErrorCode::Semantic).of("flowchart")
}

#[derive(Default)]
pub struct FlowDb {
    pub direction: Option<String>,
    // Nodes in the order they were first named.
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    pub subgraphs: Vec<FlowSubgraph>,
    pub classes: Vec<FlowClassDef>,
    pub tooltips: HashMap<String, String>,
    pub default_edge_style: Option<Vec<String>>,
    pub default_interpolate: Option<String>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,

    node_at: HashMap<String, usize>,
    class_at: HashMap<String, usize>,
    subgraph_at: HashMap<String, usize>,
    members: HashSet<String>,
    edge_by_id: HashMap<String, usize>,
    pair_count: HashMap<(String, String), usize>,
    sub_count: usize,
    pub inherit_dir: bool,
    pub max_edges: Option<usize>,
}

impl FlowDb {
    pub fn node(&self, id: &str) -> Option<&FlowNode> {
        self.node_at.get(id).map(|&i| &self.nodes[i])
    }

    pub fn class(&self, id: &str) -> Option<&FlowClassDef> {
        self.class_at.get(id).map(|&i| &self.classes[i])
    }

    pub fn subgraph(&self, id: &str) -> Option<&FlowSubgraph> {
        self.subgraph_at.get(id).map(|&i| &self.subgraphs[i])
    }

    pub fn add_vertex(&mut self, id: &str, v: Vertex) -> Result<()> {
        if id.trim().is_empty() {
            return Ok(());
        }
        let mut doc: Option<Value> = None;
        if let Some(metadata) = v.metadata {
            let yaml = if metadata.contains('\n') { format!("{}\n", metadata) } else { format!("{{\n{}\n}}", metadata) };
            let parsed = parse_yaml(&yaml)
                .map_err(|e| PeleError::new(format!("Metadata of \"{}\" is not valid YAML. {}", id, e), ErrorCode::Syntax).of("flowchart"))?;
            doc = parsed.is_object().then_some(parsed);
        }

        if let (Some(&at), Some(Value::Obj(entries))) = (self.subgraph_at.get(id), &doc) {
            let metadata = &mut self.subgraphs[at].metadata;
            for (key, value) in entries {
                match metadata.iter_mut().find(|(k, _)| k == key) {
                    Some(entry) => entry.1 = value.clone(),
                    None => metadata.push((key.clone(), value.clone())),
                }
            }
            return Ok(());
        }

        if let Some(&at) = self.edge_by_id.get(id) {
            if let Some(doc) = &doc {
                let edge = &mut self.edges[at];
                if doc.has("animate") {
                    edge.animate = Some(doc.get("animate").truthy());
                }
                if doc.has("animation") {
                    edge.animation = Some(doc.get("animation").to_text());
                }
                if doc.has("curve") {
                    edge.interpolate = Some(doc.get("curve").to_text());
                }
            }
            return Ok(());
        }

        let at = match self.node_at.get(id) {
            Some(&at) => at,
            None => {
                self.nodes.push(FlowNode { id: id.to_string(), ..Default::default() });
                self.node_at.insert(id.to_string(), self.nodes.len() - 1);
                self.nodes.len() - 1
            }
        };
        let vertex = &mut self.nodes[at];
        if let Some(text) = v.text {
            vertex.label_type = text.kind;
            vertex.text = Some(unquote(text.text.trim()).to_string());
        } else if vertex.text.is_none() {
            vertex.text = Some(id.to_string());
        }
        if let Some(shape) = v.shape {
            vertex.shape = Some(shape.to_string());
        }
        if let Some(style) = v.style {
            vertex.styles.extend(style);
        }
        if let Some((key, value)) = v.props {
            match vertex.props.iter_mut().find(|(k, _)| *k == key) {
                Some(entry) => entry.1 = value,
                None => vertex.props.push((key, value)),
            }
        }

        let Some(doc) = doc else { return Ok(()) };
        if doc.get("shape").truthy() {
            let shape = doc.get("shape").to_text();
            if shape != shape.to_lowercase() || shape.contains('_') {
                return Err(semantic(format!("No such shape: {}. Shape names should be lowercase.", shape)));
            }
            if !is_valid_shape(&shape) {
                return Err(semantic(format!("No such shape: {}.", shape)));
            }
            vertex.shape = Some(shape);
        }
        let blank_label = doc.get("label").as_str().map_or(true, |l| l.trim().is_empty());
        if doc.get("label").truthy() {
            vertex.text = Some(doc.get("label").to_text());
            vertex.label_type = label_type(doc.get("labelType").as_str());
        }
        if doc.get("icon").truthy() {
            vertex.icon = Some(doc.get("icon").to_text());
            if blank_label && vertex.text.as_deref() == Some(id) {
                vertex.text = Some(String::new());
            }
        }
        if doc.get("form").truthy() {
            vertex.form = Some(doc.get("form").to_text());
        }
        if doc.get("pos").truthy() {
            vertex.pos = Some(doc.get("pos").to_text());
        }
        if doc.get("img").truthy() {
            vertex.img = Some(doc.get("img").to_text());
            if blank_label && vertex.text.as_deref() == Some(id) {
                vertex.text = Some(String::new());
            }
        }
        if doc.get("constraint").truthy() {
            vertex.constraint = Some(doc.get("constraint").to_text());
        }
        if doc.get("w").truthy() {
            vertex.asset_width = Some(doc.get("w").to_number());
        }
        if doc.get("h").truthy() {
            vertex.asset_height = Some(doc.get("h").to_number());
        }
        Ok(())
    }

    fn add_single_link(&mut self, start: &str, end: &str, info: Option<&LinkInfo>, id: Option<&str>) -> Result<()> {
        let mut edge = FlowEdge { start: start.into(), end: end.into(), interpolate: self.default_interpolate.clone(), ..Default::default() };
        if let Some(info) = info {
            if let Some(text) = &info.text {
                edge.text = unquote(text.text.trim()).to_string();
                edge.label_type = match text.kind {
                    LabelType::Markdown => LabelType::Markdown,
                    LabelType::String => LabelType::String,
                    LabelType::Text => LabelType::Text,
                };
            }
            edge.kind = Some(info.kind.clone());
            edge.stroke = Some(info.stroke.clone());
            edge.length = info.length.map(|l| l.min(10));
        }
        let pair = (start.to_string(), end.to_string());
        let count = self.pair_count.get(&pair).copied().unwrap_or(0);
        match id.filter(|id| !id.is_empty() && !self.edge_by_id.contains_key(*id)) {
            Some(id) => {
                edge.id = id.to_string();
                edge.is_user_defined_id = true;
            }
            None => edge.id = format!("L_{}_{}_{}", start, end, if count == 0 { 0 } else { count + 1 }),
        }
        if let Some(max) = self.max_edges {
            if self.edges.len() >= max {
                return Err(PeleError::new(format!("Edge limit exceeded. The limit is {}.", max), ErrorCode::Limit).of("flowchart"));
            }
        }
        let id = edge.id.clone();
        self.edges.push(edge);
        self.pair_count.insert(pair, count + 1);
        self.edge_by_id.entry(id).or_insert(self.edges.len() - 1);
        Ok(())
    }

    pub fn add_link(&mut self, starts: &[String], ends: &[String], info: Option<&LinkInfo>) -> Result<()> {
        let id = info.and_then(|i| i.id.as_ref()).map(|id| id.replacen('@', "", 1));
        let last_start = starts.last();
        let first_end = ends.first();
        for start in starts {
            for end in ends {
                let named = Some(start) == last_start && Some(end) == first_end;
                self.add_single_link(start, end, info, if named { id.as_deref() } else { None })?;
            }
        }
        Ok(())
    }

    fn edge_at(&mut self, pos: &str) -> Result<&mut FlowEdge> {
        let count = self.edges.len();
        pos.parse::<usize>().ok().and_then(|at| self.edges.get_mut(at)).ok_or_else(|| {
            semantic(format!(
                "The index {} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                pos,
                count as i64 - 1
            ))
        })
    }

    pub fn update_link_interpolate(&mut self, positions: &[String], interpolate: &str) -> Result<()> {
        for pos in positions {
            if pos == "default" {
                self.default_interpolate = Some(interpolate.to_string());
            } else {
                self.edge_at(pos)?.interpolate = Some(interpolate.to_string());
            }
        }
        Ok(())
    }

    pub fn update_link(&mut self, positions: &[String], style: &[String]) -> Result<()> {
        // Mermaid adds `fill:none` to the list the first edge is given, and the rest share that list.
        let mut style = style.to_vec();
        for pos in positions {
            if pos == "default" {
                self.default_edge_style = Some(style.clone());
                continue;
            }
            let edge = self.edge_at(pos)?;
            if !style.is_empty() && !style.iter().any(|s| s.starts_with("fill")) {
                style.push("fill:none".into());
            }
            edge.style = Some(style.clone());
        }
        Ok(())
    }

    pub fn add_class(&mut self, ids: &str, styles: &[String]) {
        let joined = styles.join(",").replace("\\,", "§§§").replace(',', ";").replace("§§§", ",");
        let style: Vec<&str> = joined.split(';').collect();
        for id in ids.split(',') {
            let at = match self.class_at.get(id) {
                Some(&at) => at,
                None => {
                    self.classes.push(FlowClassDef { id: id.to_string(), ..Default::default() });
                    self.class_at.insert(id.to_string(), self.classes.len() - 1);
                    self.classes.len() - 1
                }
            };
            for s in &style {
                if s.contains("color") {
                    self.classes[at].text_styles.push(s.replacen("fill", "bgFill", 1));
                }
                self.classes[at].styles.push(s.to_string());
            }
        }
    }

    pub fn set_direction(&mut self, dir: &str) {
        let mut d = dir.trim();
        if d.contains('<') {
            d = "RL";
        }
        if d.contains('^') {
            d = "BT";
        }
        if d.contains('>') {
            d = "LR";
        }
        if d.contains('v') {
            d = "TB";
        }
        if d == "TD" {
            d = "TB";
        }
        self.direction = Some(d.to_string());
    }

    pub fn set_class(&mut self, ids: &str, class_name: &str) {
        for id in ids.split(',') {
            if let Some(&at) = self.node_at.get(id) {
                self.nodes[at].classes.push(class_name.to_string());
            }
            if let Some(&at) = self.edge_by_id.get(id) {
                self.edges[at].classes.push(class_name.to_string());
            }
            if let Some(&at) = self.subgraph_at.get(id) {
                self.subgraphs[at].classes.push(class_name.to_string());
            }
        }
    }

    pub fn set_tooltip(&mut self, ids: &str, tooltip: &str) {
        for id in ids.split(',') {
            self.tooltips.insert(id.to_string(), tooltip.to_string());
        }
    }

    pub fn set_link(&mut self, ids: &str, link: &str, target: Option<&str>) {
        let url = link.trim();
        for id in ids.split(',') {
            if let Some(&at) = self.node_at.get(id) {
                self.nodes[at].link = (!url.is_empty()).then(|| safe_url(url));
                self.nodes[at].link_target = target.map(String::from);
            }
        }
        self.set_class(ids, "clickable");
    }

    // Callbacks are never executed; the node is only marked clickable, as in Mermaid's strict mode.
    pub fn set_click_event(&mut self, ids: &str) {
        self.set_class(ids, "clickable");
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start().to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        // /\n\s+/g becomes a line break.
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            out.push(c);
            if c == '\n' {
                while chars.peek().is_some_and(|n| n.is_whitespace()) {
                    chars.next();
                }
            }
        }
        self.acc_descr = Some(out);
    }

    // `same` says the id and the title are one piece of text, as in `subgraph One two`.
    pub fn add_sub_graph(&mut self, id_text: Option<&FlowText>, same: bool, list: &[DocItem], title_text: Option<&FlowText>) -> String {
        let mut id: Option<String> = id_text.map(|t| t.text.trim().to_string());
        let title = title_text.map_or("", |t| t.text.as_str());
        if id_text.is_some() && same && title.chars().any(|c| c.is_whitespace()) {
            id = None;
        }

        let mut node_list: Vec<String> = Vec::new();
        let mut dir: Option<String> = None;
        let mut add = |entry: &'_ str, seen: &mut HashSet<String>| {
            if !entry.trim().is_empty() && seen.insert(entry.to_string()) {
                node_list.push(entry.to_string());
            }
        };
        let mut seen_owned: HashSet<String> = HashSet::new();
        for entry in list {
            match entry {
                DocItem::Text(text) => add(text, &mut seen_owned),
                DocItem::List(items) => {
                    for item in items {
                        add(item, &mut seen_owned);
                    }
                }
                DocItem::Dir(value) => dir = Some(value.clone()),
            }
        }
        if dir.is_none() && self.inherit_dir {
            dir = self.direction.clone();
        }

        let id = id.unwrap_or_else(|| format!("subGraph{}", self.sub_count));
        self.sub_count += 1;

        // A node belongs to the first subgraph that lists it, and a subgraph never contains itself.
        let nodes: Vec<String> = node_list.into_iter().filter(|node| !self.members.contains(node) && *node != id).collect();
        for node in &nodes {
            self.members.insert(node.clone());
        }
        match self.subgraph_at.get(&id) {
            Some(&at) => self.subgraphs[at].nodes.extend(nodes),
            None => {
                self.subgraphs.push(FlowSubgraph {
                    id: id.clone(),
                    nodes,
                    title: title.trim().to_string(),
                    classes: Vec::new(),
                    dir,
                    label_type: match title_text.map(|t| t.kind) {
                        Some(LabelType::Text) => LabelType::Text,
                        Some(LabelType::String) => LabelType::String,
                        _ => LabelType::Markdown,
                    },
                    metadata: Vec::new(),
                });
                self.subgraph_at.insert(id.clone(), self.subgraphs.len() - 1);
            }
        }
        id
    }

    pub fn destruct_link(&self, end: &str, start: Option<&str>) -> LinkInfo {
        let info = destruct_end_link(end);
        let Some(start) = start.filter(|s| !s.is_empty()) else { return info };
        let mut start_info = destruct_start_link(start);
        let invalid = || LinkInfo { kind: "INVALID".into(), stroke: "INVALID".into(), ..Default::default() };
        if start_info.stroke != info.stroke {
            return invalid();
        }
        if start_info.kind == "arrow_open" {
            start_info.kind = info.kind.clone();
        } else {
            if start_info.kind != info.kind {
                return invalid();
            }
            start_info.kind = format!("double_{}", start_info.kind);
        }
        if start_info.kind == "double_arrow" {
            start_info.kind = "double_arrow_point".into();
        }
        start_info.length = info.length;
        start_info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let db = FlowDb::default();
        let l = db.destruct_link("-->", None);
        assert_eq!((l.kind.as_str(), l.stroke.as_str(), l.length), ("arrow_point", "normal", Some(1)));
        let l = db.destruct_link("--->", None);
        assert_eq!(l.length, Some(2));
        let l = db.destruct_link("<-->", None);
        assert_eq!(l.kind, "double_arrow_point");
        let l = db.destruct_link("-.->", None);
        assert_eq!((l.stroke.as_str(), l.length), ("dotted", Some(1)));
        let l = db.destruct_link("==>", Some("=="));
        assert_eq!((l.kind.as_str(), l.stroke.as_str()), ("arrow_point", "thick"));
        let l = db.destruct_link("---", None);
        assert_eq!((l.kind.as_str(), l.length), ("arrow_open", Some(1)));
        let l = db.destruct_link("~~~", None);
        assert_eq!(l.stroke, "invisible");
    }
}
