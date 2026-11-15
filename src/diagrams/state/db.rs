use super::graph::{build_state_graph, StateGraph};
use super::parser::StateBuilder;
use super::types::{
    ClickTarget, Description, DirectionStmt, StateClassDef, StateLink, StateNote, StateRelation,
    StateStmt, Stmt,
};
use crate::chars::is_space;
use crate::errors::Result;
use std::collections::HashMap;

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

fn js_trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

fn collapse_newline_ws(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        out.push(c);
        i += 1;
        if c == '\n' && i < chars.len() && is_space(chars[i]) {
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
        }
    }
    out
}

// Mermaid's StateDB in its version 2 form, which is what Mermaid uses for both `stateDiagram`
// and `stateDiagram-v2`. The legacy version, which numbers `[*]` states, is not ported.
#[derive(Debug, Default)]
pub struct StateDb {
    pub root_doc: Vec<Stmt>,
    pub states: HashMap<String, StateStmt>,
    pub relations: Vec<StateRelation>,
    pub classes: HashMap<String, StateClassDef>,
    pub links: HashMap<String, StateLink>,
    pub graph: StateGraph,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    divider_count: usize,
}

impl StateDb {
    pub fn new() -> Self {
        Self::default()
    }

    // The direction the root document is laid out in: its last `direction` statement.
    pub fn direction(&self) -> &str {
        let mut dir = "TB";
        for item in &self.root_doc {
            if let Stmt::Dir(d) = item {
                dir = &d.value;
            }
        }
        dir
    }

    // Only the root document's statements become states, relations and classes here; the
    // statements inside composites are read when the graph is built.
    pub fn extract(&mut self, doc: &[Stmt]) -> Result<()> {
        self.states.clear();
        self.relations.clear();
        self.classes.clear();
        for item in doc {
            match item {
                Stmt::Text(_) | Stmt::Dir(_) => {}
                Stmt::State(st) => {
                    self.add_state(
                        js_trim(&st.id),
                        st.kind.as_deref(),
                        st.doc.clone(),
                        st.description.clone(),
                        st.note.clone(),
                        &[],
                    );
                }
                Stmt::Relation(rel) => {
                    self.add_relation(&rel.state1, &rel.state2, rel.description.as_deref());
                }
                Stmt::ClassDef(cd) => {
                    self.add_style_class(js_trim(&cd.id), Some(&cd.classes));
                }
                Stmt::Style(s) => {
                    let styles: Vec<String> = s
                        .style_class
                        .split(',')
                        .map(|part| js_trim(&part.replace(';', "")).to_string())
                        .collect();
                    for key in self.named(js_trim(&s.id)) {
                        if let Some(state) = self.states.get_mut(&key) {
                            state.styles = styles.clone();
                        }
                    }
                }
                Stmt::ApplyClass(ac) => {
                    self.set_css_class(js_trim(&ac.id), &ac.style_class);
                }
                Stmt::Click(c) => {
                    let target_id = match &c.id {
                        ClickTarget::Id(id) => id.clone(),
                        ClickTarget::State(st) => st.id.clone(),
                    };
                    self.add_link(target_id, c.url.clone(), c.tooltip.clone());
                }
            }
        }
        self.graph = build_state_graph(doc, &self.states, &self.classes)?;
        Ok(())
    }

    // Mermaid's docTranslator: gives each `[*]` an id scoped to its parent, and turns a document
    // split by `--` into one region per part.
    pub fn translate(&mut self) {
        let mut root = std::mem::take(&mut self.root_doc);
        for item in &mut root {
            self.translate_stmt("root", item, true);
        }
        self.root_doc = root;
    }

    fn translate_stmt(&mut self, parent_id: &str, stmt: &mut Stmt, first: bool) {
        match stmt {
            Stmt::Relation(rel) => {
                self.translate_state(parent_id, &mut rel.state1, true);
                self.translate_state(parent_id, &mut rel.state2, false);
            }
            Stmt::State(st) => {
                self.translate_state(parent_id, st, first);
            }
            _ => {}
        }
    }

    fn translate_state(&mut self, parent_id: &str, node: &mut StateStmt, first: bool) {
        if node.id == "[*]" {
            node.id = format!("{}{}", parent_id, if first { "_start" } else { "_end" });
            node.start = Some(first);
        } else {
            node.id = js_trim(&node.id).to_string();
        }
        let Some(doc) = node.doc.take() else {
            return;
        };
        let mut regions: Vec<StateStmt> = Vec::new();
        let mut current: Vec<Stmt> = Vec::new();
        for item in doc {
            let is_divider = matches!(&item, Stmt::State(s) if s.kind.as_deref() == Some("divider") && s.doc.is_none());
            if is_divider {
                if let Stmt::State(mut div) = item {
                    div.doc = Some(current);
                    regions.push(div);
                    current = Vec::new();
                }
            } else {
                current.push(item);
            }
        }
        let mut next_doc = if !regions.is_empty() {
            if !current.is_empty() {
                let mut tail = StateStmt::new(self.get_divider_id(), Some("divider".to_string()));
                tail.doc = Some(current);
                regions.push(tail);
            }
            regions.into_iter().map(Stmt::State).collect()
        } else {
            current
        };
        for child in &mut next_doc {
            self.translate_stmt(&node.id, child, true);
        }
        node.doc = Some(next_doc);
    }

    pub fn add_state(
        &mut self,
        id: &str,
        kind: Option<&str>,
        doc: Option<Vec<Stmt>>,
        descr: Option<Description>,
        note: Option<StateNote>,
        classes: &[String],
    ) {
        let trimmed_id = js_trim(id).to_string();
        let resolved_kind = kind.unwrap_or("default").to_string();
        if let Some(state) = self.states.get_mut(&trimmed_id) {
            if state.doc.is_none() {
                state.doc = doc;
            }
            if state.kind.as_deref().unwrap_or("").is_empty() {
                state.kind = Some(resolved_kind);
            }
            if note.is_some() {
                state.note = note;
            }
        } else {
            let mut state = StateStmt::new(trimmed_id.clone(), Some(resolved_kind));
            state.doc = doc;
            state.note = note;
            self.states.insert(trimmed_id.clone(), state);
        }
        if let Some(d) = descr {
            if d.is_truthy() {
                for text in d.into_vec() {
                    self.add_description(&trimmed_id, js_trim(&text));
                }
            }
        }
        for name in classes {
            self.set_css_class(&trimmed_id, js_trim(name));
        }
    }

    pub fn get_state(&self, id: &str) -> Option<&StateStmt> {
        self.states.get(id)
    }

    pub fn add_link(&mut self, state_id: String, url: String, tooltip: String) {
        self.links.insert(state_id, StateLink { url, tooltip });
    }

    pub fn add_relation(&mut self, item1: &StateStmt, item2: &StateStmt, title: Option<&str>) {
        for item in [item1, item2] {
            self.add_state(
                &item.id,
                item.kind.as_deref(),
                item.doc.clone(),
                item.description.clone(),
                item.note.clone(),
                &item.classes,
            );
        }
        self.relations.push(StateRelation {
            id1: js_trim(&item1.id).to_string(),
            id2: js_trim(&item2.id).to_string(),
            relation_title: Some(title.unwrap_or("").to_string()),
        });
    }

    pub fn add_description(&mut self, id: &str, descr: &str) {
        if let Some(state) = self.states.get_mut(id) {
            let formatted = if let Some(rest) = descr.strip_prefix(':') {
                js_trim(rest).to_string()
            } else {
                descr.to_string()
            };
            state.descriptions.push(formatted);
        }
    }

    pub fn add_style_class(&mut self, id: &str, style_attributes: Option<&str>) {
        let entry = self.classes.entry(id.to_string()).or_insert_with(|| StateClassDef {
            id: id.to_string(),
            styles: Vec::new(),
            text_styles: Vec::new(),
        });
        let Some(attrs) = style_attributes else {
            return;
        };
        if attrs.is_empty() {
            return;
        }
        for attrib in attrs.split(',') {
            let fixed = js_trim(&attrib.replacen(';', "", 1)).to_string();
            if attrib.contains("color") {
                entry.text_styles.push(
                    fixed
                        .replacen("fill", "bgFill", 1)
                        .replacen("color", "fill", 1),
                );
            }
            entry.styles.push(fixed);
        }
    }

    pub fn set_css_class(&mut self, item_ids: &str, css_class_name: &str) {
        for key in self.named(item_ids) {
            if let Some(state) = self.states.get_mut(&key) {
                state.classes.push(css_class_name.to_string());
            }
        }
    }

    pub fn get_direction(&self) -> &str {
        for item in &self.root_doc {
            if let Stmt::Dir(d) = item {
                return &d.value;
            }
        }
        "TB"
    }

    // The states a comma-separated list of ids names, created where they do not exist yet. As in
    // Mermaid, an id is looked up as written and trimmed only when it has to be created.
    fn named(&mut self, ids: &str) -> Vec<String> {
        let mut out = Vec::new();
        for id in ids.split(',') {
            if self.states.contains_key(id) {
                out.push(id.to_string());
            } else {
                self.add_state(id, None, None, None, None, &[]);
                let trimmed = js_trim(id);
                if self.states.contains_key(trimmed) {
                    out.push(trimmed.to_string());
                }
            }
        }
        out
    }
}

impl StateBuilder for StateDb {
    fn set_root_doc(&mut self, doc: Vec<Stmt>) -> Result<()> {
        self.root_doc = doc;
        self.translate();
        let doc = self.root_doc.clone();
        self.extract(&doc)
    }

    fn trim_colon(&self, text: &str) -> String {
        if let Some(rest) = text.strip_prefix(':') {
            js_trim(rest).to_string()
        } else {
            js_trim(text).to_string()
        }
    }

    fn get_divider_id(&mut self) -> String {
        self.divider_count += 1;
        format!("divider-id-{}", self.divider_count)
    }

    // The grammar calls this while it is still building the document, so the statement lands in the
    // previous root document and the direction is read from the parsed `dir` statements instead.
    fn set_direction(&mut self, dir: &str) {
        for item in &mut self.root_doc {
            if let Stmt::Dir(d) = item {
                d.value = dir.to_string();
                return;
            }
        }
        self.root_doc.insert(0, Stmt::Dir(DirectionStmt { value: dir.to_string() }));
    }

    fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(js_trim_start(text).to_string());
    }

    fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_ws(text));
    }
}
