use crate::diagrams::common::outline::OutlineDb;
use crate::errors::{ErrorCode, PeleError, Result};

#[derive(Debug, Clone)]
pub struct MindmapNode {
    pub id: usize,
    pub node_id: String,
    pub level: isize,
    pub descr: String,
    pub node_type: u8,
    pub children: Vec<usize>,
    pub is_root: bool,
    // Index of the first-level branch the node belongs to, wrapping after eleven as in Mermaid.
    pub section: Option<usize>,
    pub class: Option<String>,
    pub icon: Option<String>,
}

const SECTIONS: usize = 11;

#[derive(Debug, Clone, Default)]
pub struct MindmapDb {
    pub title: Option<String>,
    // In source order, which is depth-first. The first node is the root.
    pub nodes: Vec<MindmapNode>,
    base_level: isize,
    // The chain of nodes that can still take children, with strictly increasing levels.
    open: Vec<usize>,
}

impl OutlineDb for MindmapDb {
    fn add_node(
        &mut self,
        mut level: isize,
        id: &str,
        descr: &str,
        node_type: u8,
        _shape_data: Option<&str>,
    ) -> Result<()> {
        let is_root = self.nodes.is_empty();
        if is_root {
            self.base_level = level;
        }
        level -= self.base_level;

        // Mermaid's parent is the last node with a smaller indentation, wherever it is in the tree.
        let mut depth = self.open.len();
        while depth > 0 && self.nodes[self.open[depth - 1]].level >= level {
            depth -= 1;
        }
        let parent_idx = depth.checked_sub(1).map(|d| self.open[d]);
        if parent_idx.is_none() && !is_root {
            return Err(PeleError::new(
                format!(
                    "There can be only one root. No parent could be found for (\"{}\")",
                    descr
                ),
                ErrorCode::Semantic,
            )
            .of("mindmap"));
        }

        let node_idx = self.nodes.len();
        let section = if let Some(p) = parent_idx {
            let sec = if self.nodes[p].is_root {
                Some(self.nodes[p].children.len() % SECTIONS)
            } else {
                self.nodes[p].section
            };
            self.nodes[p].children.push(node_idx);
            sec
        } else {
            None
        };

        self.open.truncate(depth);
        self.open.push(node_idx);
        self.nodes.push(MindmapNode {
            id: node_idx,
            node_id: id.to_string(),
            level,
            descr: descr.to_string(),
            node_type,
            children: Vec::new(),
            is_root,
            section,
            class: None,
            icon: None,
        });
        Ok(())
    }

    // Mermaid throws a TypeError when a decoration comes before any node. Here it is ignored.
    fn decorate_node(&mut self, icon: Option<&str>, class: Option<&str>) {
        let Some(node) = self.nodes.last_mut() else {
            return;
        };
        if let Some(ic) = icon.filter(|s| !s.is_empty()) {
            node.icon = Some(ic.to_string());
        }
        if let Some(cl) = class.filter(|s| !s.is_empty()) {
            node.class = Some(cl.to_string());
        }
    }
}
