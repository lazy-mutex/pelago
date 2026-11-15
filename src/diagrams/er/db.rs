use super::types::{
    DocItem, ErAttribute, ErClassDef, ErEntity, ErRelationship, ErSubgraph, ParsedAttribute, RelSpec,
};
use crate::chars::is_space;
use std::collections::{HashMap, HashSet};

fn label_type(ty: Option<&str>) -> &'static str {
    match ty {
        Some("markdown") => "markdown",
        Some("string") => "string",
        Some("text") => "text",
        _ => "markdown",
    }
}

fn collapse_newline_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
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

pub struct ErDb {
    pub direction: String,
    // Insertion-ordered `(name, entity)` pairs matching JS `Map<string, ErEntity>`.
    pub entities: Vec<(String, ErEntity)>,
    entity_index: HashMap<String, usize>,
    pub relationships: Vec<ErRelationship>,
    pub classes: HashMap<String, ErClassDef>,
    pub subgraphs: Vec<ErSubgraph>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    subgraph_lookup: HashMap<String, usize>,
    claimed: HashSet<String>,
}

impl Default for ErDb {
    fn default() -> Self {
        Self {
            direction: "TB".to_string(),
            entities: Vec::new(),
            entity_index: HashMap::new(),
            relationships: Vec::new(),
            classes: HashMap::new(),
            subgraphs: Vec::new(),
            title: None,
            acc_title: None,
            acc_descr: None,
            subgraph_lookup: HashMap::new(),
            claimed: HashSet::new(),
        }
    }
}

impl ErDb {
    pub fn add_entity(&mut self, name: &str, alias: &str) -> usize {
        if let Some(&idx) = self.entity_index.get(name) {
            if self.entities[idx].1.alias.is_empty() && !alias.is_empty() {
                self.entities[idx].1.alias = alias.to_string();
            }
            idx
        } else {
            let idx = self.entities.len();
            let id = format!("entity-{}-{}", name, idx);
            let entity = ErEntity {
                id,
                label: name.to_string(),
                alias: alias.to_string(),
                attributes: Vec::new(),
                css_classes: "default".to_string(),
                css_styles: Vec::new(),
            };
            self.entity_index.insert(name.to_string(), idx);
            self.entities.push((name.to_string(), entity));
            idx
        }
    }

    // The grammar builds the list back to front, so the last attribute written comes first.
    pub fn add_attributes(&mut self, name: &str, attributes: Vec<ParsedAttribute>) {
        let idx = self.add_entity(name, "");
        let entity = &mut self.entities[idx].1;
        for attribute in attributes.into_iter().rev() {
            entity.attributes.push(ErAttribute {
                attr_type: attribute.attr_type,
                name: attribute.name,
                keys: attribute.keys.unwrap_or_default(),
                comment: attribute.comment.unwrap_or_default(),
            });
        }
    }

    pub fn add_relationship(&mut self, ent_a: &str, role_a: String, ent_b: &str, rel_spec: RelSpec) {
        let entity_a = if self.subgraph_lookup.contains_key(ent_a) {
            ent_a.to_string()
        } else {
            let idx = self.add_entity(ent_a, "");
            self.entities[idx].1.id.clone()
        };
        let entity_b = if self.subgraph_lookup.contains_key(ent_b) {
            ent_b.to_string()
        } else {
            let idx = self.add_entity(ent_b, "");
            self.entities[idx].1.id.clone()
        };
        self.relationships.push(ErRelationship { entity_a, role_a, entity_b, rel_spec });
    }

    pub fn set_direction(&mut self, dir: &str) {
        self.direction = dir.to_string();
    }

    pub fn add_css_styles(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            if let Some(&idx) = self.entity_index.get(id) {
                for style in styles {
                    self.entities[idx].1.css_styles.push(style.clone());
                }
            }
            if let Some(&sg_idx) = self.subgraph_lookup.get(id) {
                for style in styles {
                    self.subgraphs[sg_idx].css_styles.push(style.clone());
                }
            }
        }
    }

    pub fn add_class(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            let def = self.classes.entry(id.clone()).or_insert_with(|| ErClassDef {
                id: id.clone(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
            for style in styles {
                if style.contains("color") {
                    def.text_styles.push(style.replacen("fill", "bgFill", 1));
                }
                def.styles.push(style.clone());
            }
        }
    }

    pub fn set_class(&mut self, ids: &[String], class_names: &[String]) {
        for id in ids {
            if let Some(&idx) = self.entity_index.get(id) {
                for name in class_names {
                    self.entities[idx].1.css_classes.push(' ');
                    self.entities[idx].1.css_classes.push_str(name);
                }
            }
            if let Some(&sg_idx) = self.subgraph_lookup.get(id) {
                for name in class_names {
                    self.subgraphs[sg_idx].classes.push(name.clone());
                }
            }
        }
    }

    pub fn add_sub_graph(&mut self, id_text: &str, list: Vec<DocItem>, title_text: &str, title_type: Option<&str>) -> String {
        let id = id_text.trim_matches(is_space).to_string();
        let mut seen: HashSet<String> = HashSet::new();
        let mut nodes: Vec<String> = Vec::new();
        let mut dir: Option<String> = None;
        for item in list {
            match item {
                DocItem::Dir(value) => {
                    dir = Some(value);
                }
                DocItem::Str(s) => {
                    let trimmed = s.trim_matches(is_space);
                    if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
                        continue;
                    }
                    // A node belongs to the first subgraph that lists it.
                    if !self.claimed.contains(&s) {
                        nodes.push(s);
                    }
                }
            }
        }
        for node in &nodes {
            self.claimed.insert(node.clone());
        }
        let sg_idx = self.subgraphs.len();
        self.subgraphs.push(ErSubgraph {
            id: id.clone(),
            nodes,
            title: title_text.trim_matches(is_space).to_string(),
            classes: Vec::new(),
            css_styles: Vec::new(),
            dir,
            label_type: label_type(title_type),
        });
        self.subgraph_lookup.insert(id.clone(), sg_idx);
        id
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_spaces(text));
    }
}
