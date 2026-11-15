use std::collections::HashMap;
use std::hash::Hash;

// Insertion-ordered map matching JavaScript's `Map` semantics: updating an existing key updates
// its value in place without changing iteration order.
#[derive(Debug, Clone)]
pub struct OrderMap<K, V> {
    entries: Vec<(K, V)>,
    index: HashMap<K, usize>,
}

impl<K, V> Default for OrderMap<K, V> {
    fn default() -> Self {
        Self { entries: Vec::new(), index: HashMap::new() }
    }
}

impl<K: Eq + Hash + Clone, V> OrderMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.index.get(key).map(|&i| &mut self.entries[i].1)
    }

    pub fn insert(&mut self, key: K, value: V) {
        if let Some(&i) = self.index.get(&key) {
            self.entries[i].1 = value;
        } else {
            let i = self.entries.len();
            self.index.insert(key.clone(), i);
            self.entries.push((key, value));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.entries.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().map(|(_, v)| v)
    }
}

// A set (one id) or an overlap of sets (two or more ids, sorted), with its relative area.
#[derive(Debug, Clone)]
pub struct VennSubset {
    pub sets: Vec<String>,
    pub size: f64,
    pub label: Option<String>,
}

// A line of text shown inside the region of the given sets.
#[derive(Debug, Clone)]
pub struct VennText {
    pub sets: Vec<String>,
    pub id: String,
    pub label: Option<String>,
}

// Targets are the sorted ids of a set or overlap, or the id of a text.
#[derive(Debug, Clone)]
pub struct VennStyle {
    pub targets: Vec<String>,
    pub styles: OrderMap<String, String>,
}
