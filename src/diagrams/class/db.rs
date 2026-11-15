use super::members::{ClassMember, MemberType};
use super::types::{
    ClassInterface, ClassNode, ClassNote, ClassRelation, NamespaceNode, OrderMap, RelationEnd,
    StyleClass,
};
use crate::chars::is_space;
use crate::text::entities::decode_entities;
use crate::url::sanitize_url;
use std::collections::HashMap;

pub mod line_type {
    pub const LINE: u8 = 0;
    pub const DOTTED_LINE: u8 = 1;
}

pub mod relation_type {
    pub const AGGREGATION: u8 = 0;
    pub const EXTENSION: u8 = 1;
    pub const COMPOSITION: u8 = 2;
    pub const DEPENDENCY: u8 = 3;
    pub const LOLLIPOP: u8 = 4;
}

// Mermaid looks up ids that start with a digit under this prefix, so they never match a class.
const DOM_ID_PREFIX: &str = "classId-";

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

pub fn split_name(id: &str) -> (String, String) {
    let Some(at) = id.find('~') else {
        return (id.to_string(), String::new());
    };
    if at == 0 {
        return (id.to_string(), String::new());
    }
    let rest = &id[at + 1..];
    let generic_part = match rest.find('~') {
        Some(end) => &rest[..end],
        None => rest,
    };
    (id[..at].to_string(), generic_part.to_string())
}

fn lookup_id(id: &str) -> String {
    if id.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        let prefixed = format!("{}{}", DOM_ID_PREFIX, id);
        split_name(&prefixed).0
    } else {
        split_name(id).0
    }
}

#[derive(Debug)]
pub struct ClassDb {
    pub direction: String,
    pub classes: OrderMap<ClassNode>,
    pub relations: Vec<ClassRelation>,
    pub notes: OrderMap<ClassNote>,
    pub namespaces: OrderMap<NamespaceNode>,
    pub interfaces: Vec<ClassInterface>,
    pub style_classes: OrderMap<StyleClass>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    namespace_stack: Vec<String>,
    with_css_class: HashMap<String, Vec<usize>>,
}

impl Default for ClassDb {
    fn default() -> Self {
        Self {
            direction: "TB".to_string(),
            classes: OrderMap::new(),
            relations: Vec::new(),
            notes: OrderMap::new(),
            namespaces: OrderMap::new(),
            interfaces: Vec::new(),
            style_classes: OrderMap::new(),
            title: None,
            acc_title: None,
            acc_descr: None,
            namespace_stack: Vec::new(),
            with_css_class: HashMap::new(),
        }
    }
}

impl ClassDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_class(&mut self, id: &str) {
        let (class_name, generic_type) = split_name(id);
        if self.classes.contains_key(&class_name) {
            return;
        }
        let node = ClassNode {
            id: class_name.clone(),
            generic_type,
            label: class_name.clone(),
            css_classes: "default".to_string(),
            methods: Vec::new(),
            members: Vec::new(),
            annotations: Vec::new(),
            styles: Vec::new(),
            parent: None,
            link: None,
            link_target: None,
            have_callback: false,
            tooltip: None,
        };
        let idx = self.classes.insert(class_name, node);
        self.tag(idx, "default");
    }

    fn tag(&mut self, class_idx: usize, css_class: &str) {
        let list = self.with_css_class.entry(css_class.to_string()).or_default();
        if !list.contains(&class_idx) {
            list.push(class_idx);
        }
    }

    // Returns the class for a name that may carry a generic type, creating it if needed.
    // Mermaid throws a TypeError where this creates, as for an annotation on a class not yet declared.
    fn ensure_mut(&mut self, id: &str) -> &mut ClassNode {
        self.add_class(id);
        let (class_name, _) = split_name(id);
        self.classes.get_mut(&class_name).unwrap()
    }

    pub fn set_class_label(&mut self, id: &str, label: String) {
        self.ensure_mut(id).label = label;
    }

    pub fn add_relation(&mut self, mut relation: ClassRelation) {
        let type1 = relation.relation.type1;
        let type2 = relation.relation.type2;
        if type1 == RelationEnd::Marker(relation_type::LOLLIPOP) && type2 == RelationEnd::None {
            self.add_class(&relation.id2);
            relation.id1 = self.add_interface(&relation.id1, &relation.id2);
        } else if type2 == RelationEnd::Marker(relation_type::LOLLIPOP) && type1 == RelationEnd::None {
            self.add_class(&relation.id1);
            relation.id2 = self.add_interface(&relation.id2, &relation.id1);
        } else {
            self.add_class(&relation.id1);
            self.add_class(&relation.id2);
        }
        relation.id1 = split_name(&relation.id1).0;
        relation.id2 = split_name(&relation.id2).0;
        relation.relation_title1 = js_trim(&relation.relation_title1).to_string();
        relation.relation_title2 = js_trim(&relation.relation_title2).to_string();
        self.relations.push(relation);
    }

    fn add_interface(&mut self, label: &str, class_id: &str) -> String {
        let id = format!("interface{}", self.interfaces.len());
        self.interfaces.push(ClassInterface {
            id: id.clone(),
            label: label.to_string(),
            class_id: class_id.to_string(),
        });
        id
    }

    pub fn add_annotation(&mut self, class_name: &str, annotation: String) {
        self.ensure_mut(class_name).annotations.push(annotation);
    }

    pub fn add_member(&mut self, class_name: &str, member: &str) {
        let node = self.ensure_mut(class_name);
        let text = js_trim(member);
        if text.len() >= 4 && text.starts_with("<<") && text.ends_with(">>") {
            node.annotations.push(text[2..text.len() - 2].to_string());
        } else if text.find(')').is_some_and(|pos| pos > 0) {
            let parsed = ClassMember::new(text, MemberType::Method);
            if parsed.member_type == MemberType::Method {
                node.methods.push(parsed);
            } else {
                node.members.push(parsed);
            }
        } else if !text.is_empty() {
            node.members.push(ClassMember::new(text, MemberType::Attribute));
        }
    }

    pub fn add_members(&mut self, class_name: &str, mut members: Vec<String>) {
        members.reverse();
        for member in &members {
            self.add_member(class_name, member);
        }
    }

    pub fn add_note(&mut self, text: String, class_name: Option<String>) -> String {
        let index = self.notes.len();
        let id = format!("note{}", index);
        self.notes.insert(
            id.clone(),
            ClassNote {
                id: id.clone(),
                class_name,
                text,
                index,
                parent: None,
            },
        );
        id
    }

    pub fn cleanup_label(&self, label: &str) -> String {
        let rest = label.strip_prefix(':').unwrap_or(label);
        js_trim(rest).to_string()
    }

    pub fn set_css_class(&mut self, ids: &str, class_name: &str) {
        let words: Vec<&str> = class_name.split(' ').collect();
        for id in ids.split(',') {
            let key = lookup_id(id);
            let Some(idx) = self.classes.index_of(&key) else {
                continue;
            };
            if let Some(node) = self.classes.get_by_index_mut(idx) {
                node.css_classes.push(' ');
                node.css_classes.push_str(class_name);
            }
            for &word in &words {
                self.tag(idx, word);
            }
        }
    }

    pub fn define_class(&mut self, ids: &[String], style: &[String]) {
        for id in ids {
            if !self.style_classes.contains_key(id) {
                self.style_classes.insert(
                    id.clone(),
                    StyleClass {
                        id: id.clone(),
                        styles: Vec::new(),
                        text_styles: Vec::new(),
                    },
                );
            }
            let sc = self.style_classes.get_mut(id).unwrap();
            for s in style {
                if s.contains("color") {
                    sc.text_styles.push(s.replacen("fill", "bgFill", 1));
                }
                sc.styles.push(s.clone());
            }
            // Mermaid tests `cssClasses.includes(id)`, which also matches part of a longer name
            // (a classDef named `def` styles every class, through `default`). Whole names are compared here.
            let Some(tagged) = self.with_css_class.get(id).cloned() else {
                continue;
            };
            let parts: Vec<&str> = style.iter().flat_map(|s| s.split(',')).collect();
            for idx in tagged {
                if let Some(node) = self.classes.get_by_index_mut(idx) {
                    for &part in &parts {
                        node.styles.push(part.to_string());
                    }
                }
            }
        }
    }

    pub fn set_tooltip(&mut self, ids: &str, tooltip: Option<&str>) {
        let Some(tip) = tooltip else {
            return;
        };
        for id in ids.split(',') {
            let (class_name, _) = split_name(id);
            if let Some(node) = self.classes.get_mut(&class_name) {
                node.tooltip = Some(tip.to_string());
            }
        }
    }

    pub fn set_link(&mut self, ids: &str, link: &str, target: Option<&str>) {
        // Entity codes are resolved first, so that what is checked is what a browser would follow.
        let decoded = decode_entities(link);
        let url = js_trim(&decoded);
        let sanitized = if url.is_empty() { None } else { Some(sanitize_url(url)) };
        let resolved_target = target.unwrap_or("_blank").to_string();
        for id in ids.split(',') {
            let key = lookup_id(id);
            if let Some(node) = self.classes.get_mut(&key) {
                node.link = sanitized.clone();
                node.link_target = Some(resolved_target.clone());
            }
        }
        self.set_css_class(ids, "clickable");
    }

    // Callbacks are never executed; the class is only marked clickable, as in Mermaid's strict mode.
    pub fn set_click_event(&mut self, ids: &str, _function_name: Option<&str>, _function_args: Option<&str>) {
        for id in ids.split(',') {
            let (class_name, _) = split_name(id);
            if let Some(node) = self.classes.get_mut(&class_name) {
                node.have_callback = true;
            }
        }
        self.set_css_class(ids, "clickable");
    }

    pub fn set_direction(&mut self, dir: &str) {
        self.direction = dir.to_string();
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(js_trim_start(text).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_ws(text));
    }

    pub fn add_namespace(&mut self, id: &str, label: Option<&str>) -> String {
        let prefix = self.namespace_stack.last().cloned();
        let qualified = match &prefix {
            Some(p) if !p.is_empty() => format!("{}.{}", p, id),
            _ => id.to_string(),
        };
        self.namespace_stack.push(qualified.clone());

        if let Some(existing) = self.namespaces.get_mut(&qualified) {
            existing.explicit = true;
            if let Some(lbl) = label.filter(|s| !s.is_empty()) {
                existing.label = lbl.to_string();
            }
            return qualified;
        }

        // The enclosing namespace and its ancestors exist already, so only the parts of `id` are walked.
        let mut parent_id: Option<String> = prefix.as_deref().filter(|s| !s.is_empty()).map(str::to_string);
        let mut from = match &prefix {
            Some(p) if !p.is_empty() => p.len() + 1,
            _ => 0,
        };
        loop {
            let dot = qualified[from..].find('.').map(|rel| from + rel);
            let leaf = dot.is_none();
            let end = dot.unwrap_or(qualified.len());
            let current = qualified[..end].to_string();
            if let Some(node) = self.namespaces.get_mut(&current) {
                if leaf {
                    node.explicit = true;
                }
            } else {
                let part = &qualified[from..end];
                let node_label = match label {
                    Some(lbl) if leaf && !lbl.is_empty() => lbl.to_string(),
                    _ => part.to_string(),
                };
                self.namespaces.insert(
                    current.clone(),
                    NamespaceNode {
                        id: current.clone(),
                        label: node_label,
                        classes: Vec::new(),
                        notes: Vec::new(),
                        children: Vec::new(),
                        parent: parent_id.clone(),
                        explicit: leaf,
                    },
                );
            }
            if let Some(ref pid) = parent_id {
                if self.namespaces.contains_key(pid) {
                    if let Some(parent_ns) = self.namespaces.get_mut(pid) {
                        if !parent_ns.children.contains(&current) {
                            parent_ns.children.push(current.clone());
                        }
                    }
                    if let Some(cur_ns) = self.namespaces.get_mut(&current) {
                        if cur_ns.parent.is_none() {
                            cur_ns.parent = Some(pid.clone());
                        }
                    }
                }
            }
            if leaf {
                return qualified;
            }
            parent_id = Some(current);
            from = end + 1;
        }
    }

    pub fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
    }

    pub fn add_classes_to_namespace(&mut self, id: &str, class_names: &[String], note_names: &[String]) {
        if !self.namespaces.contains_key(id) {
            return;
        }
        let mut added_classes = Vec::new();
        for name in class_names {
            let (class_name, _) = split_name(name);
            if let Some(node) = self.classes.get_mut(&class_name) {
                node.parent = Some(id.to_string());
                added_classes.push(class_name);
            }
        }
        let mut added_notes = Vec::new();
        for name in note_names {
            if let Some(note) = self.notes.get_mut(name) {
                note.parent = Some(id.to_string());
                added_notes.push(name.clone());
            }
        }
        if let Some(namespace) = self.namespaces.get_mut(id) {
            for cn in added_classes {
                if !namespace.classes.contains(&cn) {
                    namespace.classes.push(cn);
                }
            }
            for nn in added_notes {
                if !namespace.notes.contains(&nn) {
                    namespace.notes.push(nn);
                }
            }
        }
    }

    pub fn set_css_style(&mut self, id: &str, styles: &[String]) {
        let Some(node) = self.classes.get_mut(id) else {
            return;
        };
        for s in styles {
            for part in s.split(',') {
                node.styles.push(part.to_string());
            }
        }
    }
}
