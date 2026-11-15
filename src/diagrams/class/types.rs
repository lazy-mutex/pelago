use super::members::ClassMember;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationEnd {
    Marker(u8),
    None,
}

#[derive(Debug, Clone)]
pub struct OrderMap<V> {
    items: Vec<V>,
    index: HashMap<String, usize>,
}

impl<V> Default for OrderMap<V> {
    fn default() -> Self {
        Self { items: Vec::new(), index: HashMap::new() }
    }
}

impl<V> OrderMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&i| &self.items[i])
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.index.get(key).copied().map(|i| &mut self.items[i])
    }

    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.index.get(key).copied()
    }

    pub fn get_by_index_mut(&mut self, idx: usize) -> Option<&mut V> {
        self.items.get_mut(idx)
    }

    pub fn insert(&mut self, key: String, value: V) -> usize {
        if let Some(&idx) = self.index.get(&key) {
            self.items[idx] = value;
            idx
        } else {
            let idx = self.items.len();
            self.index.insert(key, idx);
            self.items.push(value);
            idx
        }
    }

    pub fn values(&self) -> std::slice::Iter<'_, V> {
        self.items.iter()
    }
}

#[derive(Debug, Clone)]
pub struct ClassNode {
    pub id: String,
    pub generic_type: String,
    pub label: String,
    pub css_classes: String,
    pub methods: Vec<ClassMember>,
    pub members: Vec<ClassMember>,
    pub annotations: Vec<String>,
    pub styles: Vec<String>,
    pub parent: Option<String>,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub have_callback: bool,
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct RelationSpec {
    pub type1: RelationEnd,
    pub type2: RelationEnd,
    pub line_type: u8,
}

#[derive(Debug, Clone)]
pub struct ClassRelation {
    pub id1: String,
    pub id2: String,
    pub relation_title1: String,
    pub relation_title2: String,
    pub title: Option<String>,
    pub relation: RelationSpec,
}

#[derive(Debug, Clone)]
pub struct ClassNote {
    pub id: String,
    pub class_name: Option<String>,
    pub text: String,
    pub index: usize,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassInterface {
    pub id: String,
    pub label: String,
    pub class_id: String,
}

#[derive(Debug, Clone)]
pub struct NamespaceNode {
    pub id: String,
    pub label: String,
    pub classes: Vec<String>,
    pub notes: Vec<String>,
    pub children: Vec<String>,
    pub parent: Option<String>,
    // False for the ancestors created on the way to a dotted name such as `A.B.C`.
    pub explicit: bool,
}

#[derive(Debug, Clone)]
pub struct StyleClass {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}
