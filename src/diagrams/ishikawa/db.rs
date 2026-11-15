use super::types::IshikawaNode;

#[derive(Debug, Clone, Default)]
pub struct IshikawaDb {
    pub title: Option<String>,
    // Arena of nodes; index 0 is the root (the effect) when non-empty.
    pub nodes: Vec<IshikawaNode>,
    // The line of ancestors of the last node, each with its (level, node_index).
    stack: Vec<(isize, usize)>,
    base_level: Option<isize>,
}

impl IshikawaDb {
    // The first line is the effect. Every later line is a cause, nested by its indentation relative
    // to the first cause, so the effect itself may be indented more than the causes.
    pub fn add_node(&mut self, raw_level: isize, text: String) {
        let idx = self.nodes.len();
        self.nodes.push(IshikawaNode {
            text,
            children: Vec::new(),
        });
        if idx == 0 {
            self.stack = vec![(0, 0)];
            return;
        }
        let base = *self.base_level.get_or_insert(raw_level);
        let level = (raw_level - base + 1).max(1);
        while self.stack.len() > 1 && self.stack[self.stack.len() - 1].0 >= level {
            self.stack.pop();
        }
        let parent_idx = self.stack[self.stack.len() - 1].1;
        self.nodes[parent_idx].children.push(idx);
        self.stack.push((level, idx));
    }
}
