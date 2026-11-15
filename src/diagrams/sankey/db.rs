use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SankeyNode {
    pub id: String,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub struct SankeyLink {
    pub source: SankeyNode,
    pub target: SankeyNode,
    pub value: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SankeyDb {
    pub nodes: Vec<SankeyNode>,
    pub links: Vec<SankeyLink>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    lookup: HashMap<String, usize>,
}

impl SankeyDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn find_or_create_node(&mut self, id: &str) -> SankeyNode {
        if let Some(&idx) = self.lookup.get(id) {
            return self.nodes[idx].clone();
        }
        let index = self.nodes.len();
        let node = SankeyNode { id: id.to_string(), index };
        self.lookup.insert(id.to_string(), index);
        self.nodes.push(node.clone());
        node
    }

    pub fn add_link(&mut self, source: SankeyNode, target: SankeyNode, value: f64) {
        self.links.push(SankeyLink { source, target, value });
    }
}
