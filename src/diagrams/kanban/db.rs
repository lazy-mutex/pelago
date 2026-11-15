use crate::diagrams::common::outline::OutlineDb;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::value::parse_yaml;

#[derive(Debug, Clone)]
pub struct KanbanNode {
    pub id: String,
    pub level: isize,
    pub label: String,
    pub icon: Option<String>,
    pub assigned: Option<String>,
    pub ticket: Option<String>,
    pub priority: Option<String>,
    pub css_classes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KanbanSection {
    pub node: KanbanNode,
    pub items: Vec<KanbanNode>,
}

#[derive(Debug, Clone, Default)]
pub struct KanbanDb {
    pub title: Option<String>,
    pub sections: Vec<KanbanSection>,
    section_level: isize,
    last_is_section: bool,
    misplaced: Option<String>,
}

impl OutlineDb for KanbanDb {
    fn add_node(
        &mut self,
        level: isize,
        id: &str,
        descr: &str,
        _node_type: u8,
        shape_data: Option<&str>,
    ) -> Result<()> {
        let mut node = KanbanNode {
            id: id.to_string(),
            level,
            label: descr.to_string(),
            icon: None,
            assigned: None,
            ticket: None,
            priority: None,
            css_classes: None,
        };

        if let Some(sd) = shape_data {
            let yaml = if sd.contains('\n') {
                format!("{}\n", sd)
            } else {
                format!("{{\n{}\n}}", sd)
            };
            let doc = parse_yaml(&yaml).map_err(|err| {
                PeleError::new(
                    format!("Metadata of \"{}\" is not valid YAML. {}", node.id, err),
                    ErrorCode::Syntax,
                )
                .of("kanban")
            })?;
            // Mermaid throws a TypeError on metadata that is empty or not a mapping. Here it is ignored.
            if doc.is_object() {
                let shape = if doc.get("shape").truthy() {
                    doc.get("shape").to_text()
                } else {
                    String::new()
                };
                if shape != shape.to_lowercase() || shape.contains('_') {
                    return Err(PeleError::new(
                        format!("No such shape: {}. Shape names should be lowercase.", shape),
                        ErrorCode::Semantic,
                    )
                    .of("kanban"));
                }
                if doc.get("label").truthy() {
                    node.label = doc.get("label").to_text();
                }
                if doc.get("icon").truthy() {
                    node.icon = Some(doc.get("icon").to_text());
                }
                if doc.get("assigned").truthy() {
                    node.assigned = Some(doc.get("assigned").to_text());
                }
                if doc.get("ticket").truthy() {
                    node.ticket = Some(doc.get("ticket").to_text());
                }
                if doc.get("priority").truthy() {
                    node.priority = Some(doc.get("priority").to_text());
                }
            }
        }

        // A node indented less than the first column becomes a card, and Mermaid rejects whatever follows it.
        if let Some(ref bad_label) = self.misplaced {
            return Err(PeleError::new(
                format!("Items without section detected, found section (\"{}\")", bad_label),
                ErrorCode::Semantic,
            )
            .of("kanban"));
        }

        if self.sections.is_empty() {
            self.section_level = level;
        }
        if level == self.section_level {
            self.sections.push(KanbanSection {
                node,
                items: Vec::new(),
            });
            self.last_is_section = true;
        } else {
            if level < self.section_level {
                self.misplaced = Some(node.label.clone());
            }
            if let Some(sec) = self.sections.last_mut() {
                sec.items.push(node);
            }
            self.last_is_section = false;
        }
        Ok(())
    }

    // Mermaid throws a TypeError when a decoration comes before any node. Here it is ignored.
    fn decorate_node(&mut self, icon: Option<&str>, class: Option<&str>) {
        let Some(sec) = self.sections.last_mut() else {
            return;
        };
        let target = if self.last_is_section {
            &mut sec.node
        } else {
            match sec.items.last_mut() {
                Some(item) => item,
                None => return,
            }
        };
        if let Some(ic) = icon.filter(|s| !s.is_empty()) {
            target.icon = Some(ic.to_string());
        }
        if let Some(cl) = class.filter(|s| !s.is_empty()) {
            target.css_classes = Some(cl.to_string());
        }
    }
}
