use super::graph::AgentGraph;
use super::shapes::{is_valid_shape, resolve_shape_alias, ShapeSink};
use super::types::{
    AgentClassDef, AgentEdge, AgentNode, AgentSubgraph, Diagnostic, DiagnosticId, DocItem,
    EdgeSemantic, ElementMapping, ElementPosition, FlowText, LabelType, LinkInfo, Loc, Metadata,
    SemanticConnector, SemanticEdge, SemanticModel, SemanticSubgraph, SemanticVertex, Severity,
    StatementType, VertexKind,
};
use crate::chars::is_space;
use crate::errors::{locate, ErrorCode, PeleError, Result};
use crate::svg::builder::js_number;
use crate::url::safe_url;
use crate::value::{parse_yaml, Value};
use std::collections::{HashMap, HashSet};

#[derive(Default, Clone)]
pub struct AgentflowDbOptions {
    pub max_edges: Option<usize>,
    pub inherit_dir: bool,
    pub direction: Option<String>,
    // Lines of front matter before the text the parser sees, added to every reported line.
    pub line_offset: usize,
    // The text the parser sees, where errors are located.
    pub source: Option<String>,
}

// Presentation-only metadata keys, left out of the semantic model.
fn is_presentation_key(key: &str) -> bool {
    matches!(
        key,
        "shape" | "view" | "icon" | "img" | "form" | "pos" | "w" | "h" | "class" | "style" | "labelType"
    )
}

pub fn is_tool_definition(vertex: &AgentNode) -> bool {
    matches!(
        vertex.shape.as_deref(),
        Some("subroutine" | "subprocess" | "subproc" | "framed-rectangle" | "tool")
    )
}

fn label_type(kind: Option<&str>) -> LabelType {
    match kind {
        Some("markdown") => LabelType::Markdown,
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

// Drops `__proto__`, `constructor` and `prototype` keys at every level, so that a consumer
// merging metadata into its own objects cannot be made to pollute a prototype.
fn strip_prototype_keys(root: Value) -> Value {
    match root {
        Value::Arr(items) => Value::Arr(items.into_iter().map(strip_prototype_keys).collect()),
        Value::Obj(entries) => Value::Obj(
            entries
                .into_iter()
                .filter(|(key, _)| !matches!(key.as_str(), "__proto__" | "constructor" | "prototype"))
                .map(|(key, val)| (key, strip_prototype_keys(val)))
                .collect(),
        ),
        other => other,
    }
}

fn set_meta(meta: &mut Metadata, key: String, val: Value) {
    match meta.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = val,
        None => meta.push((key, val)),
    }
}

// Adds what a metadata block holds to an element's metadata. Mermaid spreads the parsed YAML
// whatever it is, so a list adds its indexes as keys and a string its characters.
fn merge_metadata(into: Option<Metadata>, doc: &Value) -> Metadata {
    let mut out = into.unwrap_or_default();
    match doc {
        Value::Obj(entries) => {
            for (k, v) in entries {
                set_meta(&mut out, k.clone(), v.clone());
            }
        }
        Value::Arr(items) => {
            for (i, v) in items.iter().enumerate() {
                set_meta(&mut out, i.to_string(), v.clone());
            }
        }
        Value::Str(s) => {
            for (i, ch) in s.chars().enumerate() {
                set_meta(&mut out, i.to_string(), Value::Str(ch.to_string()));
            }
        }
        _ => {}
    }
    out
}

// Matches `/:\s*[>|][\d+-]*\s*$/`.
fn is_block_scalar(code: &str) -> bool {
    let trimmed = code.trim_end_matches(is_space);
    let before_ind = trimmed.trim_end_matches(|c: char| c.is_ascii_digit() || c == '+' || c == '-');
    let Some(before_bar) = before_ind.strip_suffix(['>', '|']) else {
        return false;
    };
    before_bar.trim_end_matches(is_space).ends_with(':')
}

// Replaces `/,([\t ]*)$/` with `$1`.
fn strip_trailing_comma(code: &str) -> String {
    let head = code.trim_end_matches([' ', '\t']);
    let tail = &code[head.len()..];
    match head.strip_suffix(',') {
        Some(prefix) => format!("{}{}", prefix, tail),
        None => code.to_string(),
    }
}

// Removes the comma that ends a line of a multi-line metadata body, which block YAML does not allow.
// Commas inside quoted scalars, flow collections and block scalars are content and stay.
fn strip_line_trailing_commas(body: &str) -> String {
    let mut in_single = false;
    let mut in_double = false;
    let mut flow_depth: usize = 0;
    let mut block_scalar_indent: Option<usize> = None;
    let mut out: Vec<String> = Vec::new();
    for line in body.split('\n') {
        let chars: Vec<char> = line.chars().collect();
        let leading_ws = chars.iter().take_while(|&&c| is_space(c)).count();
        let is_empty = leading_ws == chars.len();
        if let Some(bs_indent) = block_scalar_indent {
            if is_empty || leading_ws > bs_indent {
                out.push(line.to_string());
                continue;
            }
            block_scalar_indent = None;
        }
        let mut comment_start_char: Option<usize> = None;
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if in_double {
                if ch == '\\' {
                    i += 1;
                } else if ch == '"' {
                    in_double = false;
                }
            } else if in_single {
                if ch == '\'' {
                    in_single = false;
                }
            } else if ch == '"' {
                in_double = true;
            } else if ch == '\'' {
                in_single = true;
            } else if ch == '#' && (i == 0 || chars[i - 1] == ' ' || chars[i - 1] == '\t') {
                comment_start_char = Some(i);
                break;
            } else if ch == '[' || ch == '{' {
                flow_depth += 1;
            } else if ch == ']' || ch == '}' {
                flow_depth = flow_depth.saturating_sub(1);
            }
            i += 1;
        }
        if in_single || in_double || flow_depth > 0 {
            out.push(line.to_string());
            continue;
        }
        let (code, comment) = match comment_start_char {
            Some(at) => {
                let c_str: String = chars[..at].iter().collect();
                let m_str: String = chars[at..].iter().collect();
                (c_str, m_str)
            }
            None => (line.to_string(), String::new()),
        };
        if is_block_scalar(&code) {
            block_scalar_indent = Some(leading_ws);
            out.push(line.to_string());
            continue;
        }
        out.push(format!("{}{}", strip_trailing_comma(&code), comment));
    }
    out.join("\n")
}

fn edge_semantic(kind: &str, stroke: &str) -> Option<EdgeSemantic> {
    match (kind, stroke) {
        ("arrow_point", "normal") => Some(EdgeSemantic::Sequence),
        ("arrow_cross", "normal") => Some(EdgeSemantic::Failure),
        ("arrow_open", "dotted") => Some(EdgeSemantic::Reference),
        _ => None,
    }
}

fn destruct_end_link(raw: &str) -> (String, String, usize) {
    let s = raw.trim();
    let last = s.chars().next_back();
    let mut line = match last {
        Some(c) => &s[..s.len() - c.len_utf8()],
        None => s,
    };
    let mut kind = "arrow_open";
    match last {
        Some('x') => kind = "arrow_cross",
        Some('>') => kind = "arrow_point",
        Some('-' | '.') => line = s,
        _ => {}
    }
    let mut stroke = "normal";
    let mut length = line.chars().count().saturating_sub(1);
    let dots = line.bytes().filter(|&b| b == b'.').count();
    if dots > 0 {
        stroke = "dotted";
        length = dots;
    }
    (kind.to_string(), stroke.to_string(), length)
}

fn semantic_metadata(metadata: Option<&Metadata>) -> Option<Metadata> {
    let metadata = metadata?;
    let out: Metadata = metadata
        .iter()
        .filter(|(key, _)| !is_presentation_key(key))
        .cloned()
        .collect();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn json_stringify(val: &Value) -> String {
    match val {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            if n.is_finite() {
                js_number(*n)
            } else {
                "null".to_string()
            }
        }
        Value::Str(s) => json_escape(s),
        Value::Arr(items) => {
            let inner: Vec<String> = items.iter().map(json_stringify).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Obj(entries) => {
            let inner: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}:{}", json_escape(k), json_stringify(v)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

#[derive(Default, Clone)]
pub struct AgentflowDb {
    pub direction: Option<String>,
    pub nodes: Vec<AgentNode>,
    pub edges: Vec<AgentEdge>,
    pub subgraphs: Vec<AgentSubgraph>,
    pub classes: Vec<AgentClassDef>,
    pub tooltips: HashMap<String, String>,
    pub default_edge_style: Option<Vec<String>>,
    pub default_interpolate: Option<String>,
    // Where each statement sits in the source, in the order the statements were read.
    pub mappings: Vec<ElementMapping>,
    pub diagnostics: Vec<Diagnostic>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub line_offset: usize,

    node_at: HashMap<String, usize>,
    connectors: HashSet<String>,
    class_at: HashMap<String, usize>,
    subgraph_at: HashMap<String, usize>,
    global_nodes: HashSet<String>,
    members: HashSet<String>,
    edge_by_id: HashMap<String, usize>,
    pair_count: HashMap<(String, String), usize>,
    sub_count: usize,
    bare_mappings: HashSet<usize>,
    first_mapping: HashMap<String, usize>,
    reported: HashMap<String, Vec<Diagnostic>>,
    options: AgentflowDbOptions,
    source_chars: Option<Vec<char>>,
    pub(crate) graph: Option<AgentGraph>,
}

impl AgentflowDb {
    pub fn new(options: AgentflowDbOptions) -> Self {
        let line_offset = options.line_offset;
        let source_chars = options.source.as_ref().map(|s| s.chars().collect());
        Self {
            line_offset,
            options,
            source_chars,
            ..Default::default()
        }
    }

    pub fn node(&self, id: &str) -> Option<&AgentNode> {
        self.node_at.get(id).map(|&i| &self.nodes[i])
    }

    pub fn is_connector(&self, id: &str) -> bool {
        self.connectors.contains(id)
    }

    pub fn class(&self, id: &str) -> Option<&AgentClassDef> {
        self.class_at.get(id).map(|&i| &self.classes[i])
    }

    pub fn subgraph(&self, id: &str) -> Option<&AgentSubgraph> {
        self.subgraph_at.get(id).map(|&i| &self.subgraphs[i])
    }

    fn error_at(&self, loc: Option<Loc>) -> (usize, usize) {
        let Some(loc) = loc else {
            return (0, 0);
        };
        let Some(source) = &self.source_chars else {
            return (loc.first_line + self.line_offset, loc.first_column + 1);
        };
        let mut start = 0;
        for _ in 1..loc.first_line {
            match source[start..].iter().position(|&c| c == '\n') {
                Some(nl) => start += nl + 1,
                None => {
                    start = source.len();
                    break;
                }
            }
        }
        let found = locate(source, start + loc.first_column);
        (found.line + self.line_offset, found.column)
    }

    fn shape_error(&self, message: String, loc: Option<Loc>) -> PeleError {
        let (line, column) = self.error_at(loc);
        let mut err = PeleError::new(message, ErrorCode::Semantic).of("agentflow");
        err.line = line;
        err.column = column;
        err
    }

    fn parse_metadata(&self, id: &str, source: &str, loc: Option<Loc>) -> Result<Value> {
        let multiline = source.contains('\n');
        let yaml_in = if multiline {
            format!("{}\n", source)
        } else {
            format!("{{\n{}\n}}", source)
        };
        let parsed = match parse_yaml(&yaml_in) {
            Ok(val) => val,
            Err(error) => {
                // Block YAML rejects the comma-separated style of the single-line form, so try once more without the commas.
                let stripped = if multiline {
                    strip_line_trailing_commas(source)
                } else {
                    source.to_string()
                };
                let retried = if stripped != source {
                    parse_yaml(&format!("{}\n", stripped)).ok()
                } else {
                    None
                };
                match retried {
                    Some(val) => val,
                    None => {
                        let (line, column) = self.error_at(loc);
                        let at = if loc.is_some() {
                            format!(" ({}:{})", line, column)
                        } else {
                            String::new()
                        };
                        let mut err = PeleError::new(
                            format!("Metadata of \"{}\" is not valid YAML. {}{}", id, error, at),
                            ErrorCode::Syntax,
                        )
                        .of("agentflow");
                        err.line = line;
                        err.column = column;
                        return Err(err);
                    }
                }
            }
        };
        let non_null = if parsed == Value::Null {
            Value::object()
        } else {
            parsed
        };
        Ok(strip_prototype_keys(non_null))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_vertex(
        &mut self,
        id: &str,
        text_obj: Option<&FlowText>,
        shape: Option<&str>,
        style: Option<Vec<String>>,
        classes: Option<Vec<String>>,
        dir: Option<&str>,
        props: Option<Vec<(String, String)>>,
        metadata: Option<&str>,
        metadata_loc: Option<Loc>,
    ) -> Result<()> {
        if id.trim().is_empty() {
            return Ok(());
        }

        let mut doc: Option<Value> = None;
        let mut authored_shape: Option<String> = None;
        if let Some(meta_src) = metadata {
            let mut d = self.parse_metadata(id, meta_src, metadata_loc)?;
            if d.is_object() {
                if let Some(s) = d.get("shape").as_str().map(String::from) {
                    let resolved = resolve_shape_alias(Some(&s)).unwrap_or(&s).to_string();
                    authored_shape = Some(s);
                    d.set("shape", Value::Str(resolved));
                }
            }
            doc = Some(d);
        }

        if let Some(&sg_at) = self.subgraph_at.get(id) {
            if let Some(ref d) = doc {
                if d.truthy() {
                    let prev = self.subgraphs[sg_at].metadata.take();
                    self.subgraphs[sg_at].metadata = Some(merge_metadata(prev, d));
                    return Ok(());
                }
            }
        }

        if self.connectors.contains(id) {
            if let Some(ref d) = doc {
                if d.truthy() {
                    if let Some(&at) = self.node_at.get(id) {
                        let prev = self.nodes[at].metadata.take();
                        self.nodes[at].metadata = Some(merge_metadata(prev, d));
                    }
                    return Ok(());
                }
            }
        }

        // Mermaid reserves this id and creates no node for it.
        if id == "connectors" {
            return Ok(());
        }

        if let Some(&edge_at) = self.edge_by_id.get(id) {
            if let Some(ref d) = doc {
                let edge = &mut self.edges[edge_at];
                if d.is_object() {
                    if d.has("animate") {
                        edge.animate = Some(d.get("animate").clone());
                    }
                    if d.has("animation") {
                        edge.animation = Some(d.get("animation").clone());
                    }
                    if d.has("curve") {
                        let curve_val = d.get("curve");
                        if let Some(s) = curve_val.as_str() {
                            edge.interpolate = Some(s.to_string());
                        } else if curve_val.truthy() {
                            edge.interpolate = Some(curve_val.to_text());
                        } else {
                            edge.interpolate = None;
                        }
                    }
                }
                if d.truthy() {
                    let prev = edge.metadata.take();
                    edge.metadata = Some(merge_metadata(prev, d));
                }
            }
            return Ok(());
        }

        let at = match self.node_at.get(id) {
            Some(&at) => at,
            None => {
                self.nodes.push(AgentNode {
                    id: id.to_string(),
                    label_type: LabelType::Text,
                    ..Default::default()
                });
                let idx = self.nodes.len() - 1;
                self.node_at.insert(id.to_string(), idx);
                idx
            }
        };

        {
            let vertex = &mut self.nodes[at];
            if let Some(t) = text_obj {
                vertex.label_type = t.kind;
                vertex.text = Some(unquote(t.text.trim()).to_string());
            } else if vertex.text.is_none() {
                vertex.text = Some(id.to_string());
            }
            if let Some(sh) = shape {
                vertex.shape = Some(sh.to_string());
            }
            if let Some(st) = style {
                vertex.styles.extend(st);
            }
            if let Some(cl) = classes {
                vertex.classes.extend(cl);
            }
            if let Some(d) = dir {
                vertex.dir = Some(d.to_string());
            }
            let prop_entries = props.unwrap_or_default();
            match &mut vertex.props {
                None => vertex.props = Some(prop_entries),
                Some(existing) => {
                    for (k, v) in prop_entries {
                        match existing.iter_mut().find(|(ek, _)| *ek == k) {
                            Some(entry) => entry.1 = v,
                            None => existing.push((k, v)),
                        }
                    }
                }
            }
        }

        let Some(d) = doc.filter(|d| d.truthy()) else {
            return Ok(());
        };
        let prev = self.nodes[at].metadata.take();
        self.nodes[at].metadata = Some(merge_metadata(prev, &d));
        if !d.is_object() {
            return Ok(());
        }

        if d.get("shape").truthy() {
            let shape_field = d.get("shape");
            let Some(authored) = authored_shape.as_deref().or_else(|| shape_field.as_str()) else {
                return Err(self.shape_error(
                    format!("No such shape: {}.", json_stringify(shape_field)),
                    metadata_loc,
                ));
            };
            if authored != authored.to_lowercase() || authored.contains('_') {
                return Err(self.shape_error(
                    format!("No such shape: {}. Shape names should be lowercase.", authored),
                    metadata_loc,
                ));
            }
            let resolved_shape = shape_field.as_str().unwrap_or(authored);
            if !is_valid_shape(resolved_shape) {
                return Err(self.shape_error(format!("No such shape: {}.", resolved_shape), metadata_loc));
            }
            self.nodes[at].shape = Some(resolved_shape.to_string());
        }

        if d.get("label").truthy() {
            self.nodes[at].text = Some(d.get("label").to_text());
            self.nodes[at].label_type = label_type(d.get("labelType").as_str());
        }
        Ok(())
    }

    pub fn add_single_link(
        &mut self,
        start: &str,
        end: &str,
        info: Option<&LinkInfo>,
        id: Option<&str>,
    ) -> Result<()> {
        let mut edge = AgentEdge {
            start: start.to_string(),
            end: end.to_string(),
            label_type: LabelType::Text,
            interpolate: self.default_interpolate.clone(),
            ..Default::default()
        };
        if let Some(info) = info {
            if let Some(link_text) = &info.text {
                edge.text = unquote(link_text.text.trim()).to_string();
                edge.label_type = label_type(Some(link_text.kind.as_str()));
            }
            edge.kind = Some(info.kind.clone());
            edge.stroke = Some(info.stroke.clone());
            edge.length = info.length.map(|l| l.min(10));
            if let Some(sem) = info.edge_semantic {
                edge.edge_semantic = Some(sem);
            }
        }
        let pair = (start.to_string(), end.to_string());
        let existing = self.pair_count.get(&pair).copied().unwrap_or(0);
        match id.filter(|id| !id.is_empty() && !self.edge_by_id.contains_key(*id)) {
            Some(uid) => {
                edge.id = uid.to_string();
                edge.is_user_defined_id = true;
            }
            None => {
                edge.id = format!(
                    "L_{}_{}_{}",
                    start,
                    end,
                    if existing == 0 { 0 } else { existing + 1 }
                );
            }
        }

        if let Some(max) = self.options.max_edges {
            if self.edges.len() >= max {
                return Err(PeleError::new(
                    format!("Edge limit exceeded. The limit is {}.", max),
                    ErrorCode::Limit,
                )
                .of("agentflow"));
            }
        }
        let edge_id = edge.id.clone();
        self.edges.push(edge);
        self.pair_count.insert(pair, existing + 1);
        self.edge_by_id.entry(edge_id).or_insert(self.edges.len() - 1);
        Ok(())
    }

    pub fn add_link(
        &mut self,
        starts: &[String],
        ends: &[String],
        link_data: Option<&LinkInfo>,
    ) -> Result<()> {
        let id = link_data
            .and_then(|d| d.id.as_ref())
            .map(|s| s.replacen('@', "", 1));
        let last_start = starts.last();
        let first_end = ends.first();
        for start in starts {
            for end in ends {
                let named = Some(start) == last_start && Some(end) == first_end;
                self.add_single_link(start, end, link_data, if named { id.as_deref() } else { None })?;
            }
        }
        Ok(())
    }

    fn edge_at(&mut self, pos: &str) -> Result<&mut AgentEdge> {
        let count = self.edges.len();
        pos.parse::<usize>()
            .ok()
            .and_then(|at| self.edges.get_mut(at))
            .ok_or_else(|| {
                PeleError::new(
                    format!(
                        "The index {} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                        pos,
                        count as i64 - 1
                    ),
                    ErrorCode::Semantic,
                )
                .of("agentflow")
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
        let mut style = style.to_vec();
        for pos in positions {
            if pos == "default" {
                self.default_edge_style = Some(style.clone());
                continue;
            }
            let edge = self.edge_at(pos)?;
            if !style.is_empty() && !style.iter().any(|s| s.starts_with("fill")) {
                style.push("fill:none".to_string());
            }
            edge.style = Some(style.clone());
        }
        Ok(())
    }

    pub fn add_class(&mut self, ids: &str, styles: &[String]) {
        let joined = styles
            .join(",")
            .replace("\\,", "§§§")
            .replace(',', ";")
            .replace("§§§", ",");
        let style: Vec<&str> = joined.split(';').collect();
        for id in ids.split(',') {
            let at = match self.class_at.get(id) {
                Some(&at) => at,
                None => {
                    self.classes.push(AgentClassDef {
                        id: id.to_string(),
                        ..Default::default()
                    });
                    let idx = self.classes.len() - 1;
                    self.class_at.insert(id.to_string(), idx);
                    idx
                }
            };
            for s in &style {
                if s.contains("color") {
                    self.classes[at]
                        .text_styles
                        .push(s.replacen("fill", "bgFill", 1));
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

    pub fn set_tooltip(&mut self, ids: &str, tooltip: Option<&str>) {
        let Some(tooltip) = tooltip else { return };
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
    pub fn set_click_event(&mut self, ids: &str, _fn_name: Option<&str>, _fn_args: Option<&str>) {
        self.set_class(ids, "clickable");
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            out.push(c);
            if c == '\n' {
                while chars.peek().is_some_and(|&n| is_space(n)) {
                    chars.next();
                }
            }
        }
        self.acc_descr = Some(out);
    }

    pub fn add_sub_graph(
        &mut self,
        id_text: Option<&FlowText>,
        same: bool,
        list: &[DocItem],
        title_text: Option<&FlowText>,
    ) -> String {
        let mut id: Option<String> = id_text.map(|t| t.text.trim().to_string());
        let title = title_text.map_or("", |t| t.text.as_str());
        if id_text.is_some() && same && title.chars().any(is_space) {
            id = None;
        }

        let mut seen: HashSet<String> = HashSet::new();
        let mut node_list: Vec<String> = Vec::new();
        let mut dir: Option<String> = None;
        let mut push_node = |entry: &str| {
            if !entry.trim().is_empty() && seen.insert(entry.to_string()) {
                node_list.push(entry.to_string());
            }
        };
        for entry in list {
            match entry {
                DocItem::Text(t) => push_node(t),
                DocItem::List(items) => {
                    for item in items {
                        push_node(item);
                    }
                }
                DocItem::Dir(value) => dir = Some(value.clone()),
            }
        }
        if dir.is_none() && self.options.inherit_dir {
            dir = self.direction.clone().or_else(|| self.options.direction.clone());
        }

        let id = id.unwrap_or_else(|| format!("subGraph{}", self.sub_count));
        self.sub_count += 1;

        // A flow never contains itself, a globally scoped node, or a node another flow already holds.
        let nodes: Vec<String> = node_list
            .into_iter()
            .filter(|node_id| {
                *node_id != id && !self.global_nodes.contains(node_id) && !self.members.contains(node_id)
            })
            .collect();
        for node_id in &nodes {
            self.members.insert(node_id.clone());
        }

        if let Some(&at) = self.subgraph_at.get(&id) {
            // A repeated id adds its members to the first flow; its title and direction win.
            self.subgraphs[at].nodes.extend(nodes);
            if !title.trim().is_empty() {
                self.subgraphs[at].title = title.trim().to_string();
            }
            if let Some(d) = dir.filter(|d| !d.is_empty()) {
                self.subgraphs[at].dir = Some(d);
            }
            return id;
        }

        let subgraph = AgentSubgraph {
            id: id.clone(),
            nodes,
            title: title.trim().to_string(),
            classes: Vec::new(),
            dir,
            label_type: label_type(title_text.map(|t| t.kind.as_str())),
            kind: "flow",
            metadata: None,
        };
        self.subgraphs.push(subgraph);
        self.subgraph_at.insert(id.clone(), self.subgraphs.len() - 1);
        id
    }

    // Every node named inside a `global` block stays out of the flows that mention it.
    pub fn add_global(&mut self, list: &[DocItem]) {
        let mut release = false;
        let mut add = |item: &str| {
            let id = item.trim();
            if id.is_empty() {
                return;
            }
            self.global_nodes.insert(id.to_string());
            if self.members.remove(id) {
                release = true;
            }
        };
        for entry in list {
            match entry {
                DocItem::Text(s) => add(s),
                DocItem::List(items) => {
                    for item in items {
                        add(item);
                    }
                }
                DocItem::Dir(_) => {}
            }
        }
        if !release {
            return;
        }
        for subgraph in &mut self.subgraphs {
            subgraph.nodes.retain(|node_id| !self.global_nodes.contains(node_id));
        }
    }

    pub fn add_connector(
        &mut self,
        text_obj: Option<&FlowText>,
        title_obj: Option<&FlowText>,
    ) -> String {
        let Some(id) = text_obj.map(|t| t.text.trim()).filter(|s| !s.is_empty()) else {
            return String::new();
        };
        let title = title_obj.map_or("", |t| t.text.trim());
        if !self.connectors.contains(id) {
            let created = AgentNode {
                id: id.to_string(),
                label_type: LabelType::Text,
                styles: Vec::new(),
                classes: Vec::new(),
                is_connector: true,
                text: Some(if !title.is_empty() {
                    title.to_string()
                } else {
                    id.to_string()
                }),
                ..Default::default()
            };
            self.connectors.insert(id.to_string());
            match self.node_at.get(id) {
                Some(&at) => self.nodes[at] = created,
                None => {
                    self.nodes.push(created);
                    self.node_at.insert(id.to_string(), self.nodes.len() - 1);
                }
            }
        } else if !title.is_empty() {
            if let Some(&at) = self.node_at.get(id) {
                self.nodes[at].text = Some(title.to_string());
            }
        }
        id.to_string()
    }

    pub fn destruct_link(&self, end: &str, start: Option<&str>) -> LinkInfo {
        let (kind, stroke, length) = destruct_end_link(end);
        if let Some(start) = start.filter(|s| !s.is_empty()) {
            // The first half of a split arrow only says whether the line is dotted.
            let start_stroke = if start.contains('.') { "dotted" } else { "normal" };
            if start_stroke != stroke {
                return LinkInfo {
                    kind: "INVALID".to_string(),
                    stroke: "INVALID".to_string(),
                    ..Default::default()
                };
            }
        }
        let edge_semantic = edge_semantic(&kind, &stroke);
        LinkInfo {
            kind,
            stroke,
            length: Some(length),
            edge_semantic,
            text: None,
            id: None,
        }
    }

    pub fn vertex_kind(&self, vertex: &AgentNode) -> VertexKind {
        if is_tool_definition(vertex) {
            return VertexKind::Tool;
        }
        match resolve_shape_alias(vertex.shape.as_deref()) {
            Some("hexagon" | "hex") => VertexKind::Action,
            Some("lean-right" | "lean_right") => VertexKind::Input,
            Some("lin-doc" | "lined-document") => VertexKind::Refdoc,
            Some("diamond") => VertexKind::Decision,
            _ => VertexKind::Task,
        }
    }

    pub fn semantic_model(&self) -> SemanticModel {
        let mut vertices = Vec::new();
        let mut connectors = Vec::new();
        for v in &self.nodes {
            if self.subgraph_at.contains_key(&v.id) {
                continue;
            }
            let metadata = semantic_metadata(v.metadata.as_ref());
            if v.is_connector {
                connectors.push(SemanticConnector {
                    id: v.id.clone(),
                    title: v.text.as_ref().filter(|t| t.as_str() != v.id).cloned(),
                    metadata,
                });
                continue;
            }
            vertices.push(SemanticVertex {
                id: v.id.clone(),
                label: v.text.clone(),
                shape: resolve_shape_alias(v.shape.as_deref()).map(String::from),
                vertex_kind: Some(self.vertex_kind(v)),
                metadata,
            });
        }

        let edges = self
            .edges
            .iter()
            .map(|e| SemanticEdge {
                start: e.start.clone(),
                end: e.end.clone(),
                id: (!e.id.is_empty()).then(|| e.id.clone()),
                label: (!e.text.is_empty()).then(|| e.text.clone()),
                kind: e.kind.clone(),
                stroke: e.stroke.clone(),
                edge_semantic: e.edge_semantic,
                length: e.length,
                metadata: e.metadata.as_ref().filter(|m| !m.is_empty()).cloned(),
            })
            .collect();

        let sub_graphs = self
            .subgraphs
            .iter()
            .map(|sg| SemanticSubgraph {
                id: sg.id.clone(),
                kind: Some(sg.kind.to_string()),
                title: Some(sg.title.clone()),
                nodes: sg.nodes.clone(),
                metadata: semantic_metadata(sg.metadata.as_ref()),
                direction: sg.dir.clone(),
            })
            .collect();

        SemanticModel {
            direction: self.direction.clone(),
            vertices,
            edges,
            sub_graphs,
            connectors,
            diagnostics: self.diagnostics.clone(),
        }
    }

    fn position(&self, loc: Option<Loc>) -> ElementPosition {
        let start_line = loc.map_or(0, |l| l.first_line);
        let start_column = loc.map_or(0, |l| l.first_column);
        ElementPosition {
            start_line: start_line + self.line_offset,
            start_column,
            end_line: loc.map_or(start_line, |l| l.last_line) + self.line_offset,
            end_column: loc.map_or(start_column, |l| l.last_column),
            // Mermaid builds its parser without ranges, so these are always zero.
            start_index: 0,
            end_index: 0,
        }
    }

    fn push_mapping(
        &mut self,
        id: &str,
        kind: StatementType,
        position: ElementPosition,
    ) -> usize {
        let mapping = ElementMapping {
            id: id.to_string(),
            kind,
            position,
        };
        self.mappings.push(mapping);
        let idx = self.mappings.len() - 1;
        self.first_mapping.entry(id.to_string()).or_insert(idx);
        idx
    }

    pub fn add_vertex_mapping(&mut self, id: &str, has_shape: bool, loc: Option<Loc>) {
        if id.is_empty() {
            return;
        }
        let pos = self.position(loc);
        let idx = self.push_mapping(id, StatementType::Vertex, pos);
        // A bare reference becomes an attachment if a metadata block follows it.
        if !has_shape {
            self.bare_mappings.insert(idx);
        }
    }

    // Widens a node's span over the metadata block that follows it.
    pub fn extend_vertex_mapping(&mut self, id: &str, loc: Option<Loc>) {
        let Some(loc) = loc else { return };
        if id.is_empty() {
            return;
        }
        let end = self.position(Some(loc));
        for i in (0..self.mappings.len()).rev() {
            if self.mappings[i].kind != StatementType::Vertex || self.mappings[i].id != id {
                continue;
            }
            if self.bare_mappings.remove(&i) {
                self.mappings[i].kind = StatementType::Attachment;
            }
            let p = &mut self.mappings[i].position;
            if end.end_line > p.end_line
                || (end.end_line == p.end_line && end.end_column > p.end_column)
            {
                p.end_line = end.end_line;
                p.end_column = end.end_column;
            }
            return;
        }
        self.push_mapping(id, StatementType::Attachment, end);
    }

    pub fn add_edge_mapping(&mut self, to_nodes: &[String], loc: Option<Loc>) {
        let ids: Vec<&str> = to_nodes
            .iter()
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .collect();
        let edge_id = if !ids.is_empty() {
            ids.join(">")
        } else {
            "edge".to_string()
        };
        let pos = self.position(loc);
        self.push_mapping(&edge_id, StatementType::Edge, pos);
    }

    fn block_mapping(
        &mut self,
        id: &str,
        kind: StatementType,
        start_loc: Option<Loc>,
        end_loc: Option<Loc>,
    ) {
        if id.is_empty() {
            return;
        }
        let start = self.position(start_loc);
        let end = end_loc.map_or(start, |l| self.position(Some(l)));
        self.push_mapping(
            id,
            kind,
            ElementPosition {
                start_line: start.start_line,
                start_column: start.start_column,
                end_line: end.end_line,
                end_column: end.end_column,
                start_index: 0,
                end_index: 0,
            },
        );
    }

    pub fn add_subgraph_mapping(
        &mut self,
        id_text: Option<&FlowText>,
        start_loc: Option<Loc>,
        end_loc: Option<Loc>,
    ) {
        let text = id_text.map_or("", |t| t.text.as_str());
        self.block_mapping(text, StatementType::Subgraph, start_loc, end_loc);
    }

    pub fn add_connector_mapping(
        &mut self,
        text_obj: Option<&FlowText>,
        start_loc: Option<Loc>,
        end_loc: Option<Loc>,
    ) {
        let text = text_obj.map_or("", |t| t.text.trim());
        self.block_mapping(text, StatementType::Connector, start_loc, end_loc);
    }

    fn emit(
        &mut self,
        id: DiagnosticId,
        severity: Severity,
        message: String,
        node_id: Option<&str>,
        edge_id: Option<&str>,
    ) {
        let anchor = node_id.or(edge_id);
        let position = anchor
            .and_then(|a| self.first_mapping.get(a))
            .map(|&idx| self.mappings[idx].position);
        let diagnostic = Diagnostic {
            id,
            severity,
            message,
            node_id: node_id.map(String::from),
            edge_id: if node_id.is_some() {
                None
            } else {
                edge_id.map(String::from)
            },
            position,
        };

        // The graph may be built more than once; each finding is recorded once.
        let key = anchor.unwrap_or("").to_string();
        if let Some(same) = self.reported.get_mut(&key) {
            if same.iter().any(|d| {
                d.id == diagnostic.id
                    && d.severity == diagnostic.severity
                    && d.message == diagnostic.message
                    && d.node_id == diagnostic.node_id
                    && d.edge_id == diagnostic.edge_id
            }) {
                return;
            }
            same.push(diagnostic.clone());
        } else {
            self.reported.insert(key, vec![diagnostic.clone()]);
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn emit_warning(
        &mut self,
        id: DiagnosticId,
        message: String,
        node_id: Option<&str>,
        edge_id: Option<&str>,
    ) {
        self.emit(id, Severity::Warning, message, node_id, edge_id);
    }

    pub fn emit_error(
        &mut self,
        id: DiagnosticId,
        message: String,
        node_id: Option<&str>,
        edge_id: Option<&str>,
    ) {
        self.emit(id, Severity::Error, message, node_id, edge_id);
    }
}

impl ShapeSink for AgentflowDb {
    fn emit_shape_error(&mut self, id: DiagnosticId, message: String, node_id: &str) {
        self.emit_error(id, message, Some(node_id), None);
    }

    fn emit_shape_warning(&mut self, id: DiagnosticId, message: String, node_id: &str) {
        self.emit_warning(id, message, Some(node_id), None);
    }
}
