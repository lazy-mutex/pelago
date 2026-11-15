use super::parser::{AlignDir, ArchitectureAst, EdgeAst, GroupAst, JunctionAst, ServiceAst, Side};
use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum ArchNode {
    Service {
        id: String,
        icon: Option<String>,
        icon_text: Option<String>,
        title: Option<String>,
        parent: Option<String>,
    },
    Junction {
        id: String,
        parent: Option<String>,
    },
}

impl ArchNode {
    pub fn id(&self) -> &str {
        match self {
            ArchNode::Service { id, .. } | ArchNode::Junction { id, .. } => id,
        }
    }

    pub fn parent(&self) -> Option<&str> {
        match self {
            ArchNode::Service { parent, .. } | ArchNode::Junction { parent, .. } => parent.as_deref(),
        }
    }

    pub fn is_junction(&self) -> bool {
        matches!(self, ArchNode::Junction { .. })
    }
}

#[derive(Debug, Clone)]
pub struct ArchGroup {
    pub id: String,
    pub icon: Option<String>,
    pub title: Option<String>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArchEdge {
    pub lhs_id: String,
    pub lhs_dir: Side,
    pub lhs_into: bool,
    pub lhs_group: bool,
    pub rhs_id: String,
    pub rhs_dir: Side,
    pub rhs_into: bool,
    pub rhs_group: bool,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArchLayoutHint {
    pub direction: AlignDir,
    pub members: Vec<String>,
}

fn fail<T>(message: impl Into<String>) -> Result<T> {
    Err(PeleError::new(message, ErrorCode::Semantic).of("architecture"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisteredKind {
    Node,
    Group,
}

impl RegisteredKind {
    fn as_str(self) -> &'static str {
        match self {
            RegisteredKind::Node => "node",
            RegisteredKind::Group => "group",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ArchitectureDb {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub nodes: Vec<ArchNode>,
    node_map: HashMap<String, usize>,
    pub groups: Vec<ArchGroup>,
    group_map: HashMap<String, usize>,
    pub edges: Vec<ArchEdge>,
    pub layout_hints: Vec<ArchLayoutHint>,
    registered: HashMap<String, RegisteredKind>,
}

impl ArchitectureDb {
    fn register(&mut self, kind: &str, id: &str, parent: Option<&str>) -> Result<()> {
        if let Some(&existing) = self.registered.get(id) {
            return fail(format!(
                "The {} id [{}] is already in use by another {}",
                kind,
                id,
                existing.as_str()
            ));
        }
        if let Some(parent) = parent {
            if id == parent {
                return fail(format!("The {} [{}] cannot be placed within itself", kind, id));
            }
            let Some(&parent_kind) = self.registered.get(parent) else {
                return fail(format!(
                    "The {} [{}]'s parent does not exist. Please make sure the parent is created before this {}",
                    kind, id, kind
                ));
            };
            if parent_kind == RegisteredKind::Node {
                return fail(format!("The {} [{}]'s parent is not a group", kind, id));
            }
        }
        self.registered.insert(
            id.to_string(),
            if kind == "group" { RegisteredKind::Group } else { RegisteredKind::Node },
        );
        Ok(())
    }

    pub fn add_service(&mut self, svc: ServiceAst) -> Result<()> {
        self.register("service", &svc.id, svc.parent.as_deref())?;
        let idx = self.nodes.len();
        self.node_map.insert(svc.id.clone(), idx);
        self.nodes.push(ArchNode::Service {
            id: svc.id,
            icon: svc.icon,
            icon_text: svc.icon_text,
            title: svc.title,
            parent: svc.parent,
        });
        Ok(())
    }

    pub fn add_junction(&mut self, junc: JunctionAst) -> Result<()> {
        self.register("junction", &junc.id, junc.parent.as_deref())?;
        let idx = self.nodes.len();
        self.node_map.insert(junc.id.clone(), idx);
        self.nodes.push(ArchNode::Junction { id: junc.id, parent: junc.parent });
        Ok(())
    }

    pub fn add_group(&mut self, grp: GroupAst) -> Result<()> {
        self.register("group", &grp.id, grp.parent.as_deref())?;
        let idx = self.groups.len();
        self.group_map.insert(grp.id.clone(), idx);
        self.groups.push(ArchGroup {
            id: grp.id,
            icon: grp.icon,
            title: grp.title,
            parent: grp.parent,
        });
        Ok(())
    }

    pub fn add_edge(&mut self, edge: EdgeAst) -> Result<()> {
        let ends = [
            ("left", edge.lhs_id.as_str(), edge.lhs_group),
            ("right", edge.rhs_id.as_str(), edge.rhs_group),
        ];
        for (side, id, _) in ends {
            if !self.registered.contains_key(id) {
                return fail(format!(
                    "The {}-hand id [{}] does not yet exist. Please create the service/group before declaring an edge to it.",
                    side, id
                ));
            }
        }
        // Mermaid lets a group id through the check above and then fails with a TypeError.
        for (side, id, _) in ends {
            if !self.node_map.contains_key(id) {
                return fail(format!(
                    "The {}-hand id [{}] is a group. Add {{group}} to a service inside it instead.",
                    side, id
                ));
            }
        }
        let lhs_parent = self.nodes[self.node_map[&edge.lhs_id]].parent();
        let rhs_parent = self.nodes[self.node_map[&edge.rhs_id]].parent();
        if lhs_parent.is_some() && lhs_parent == rhs_parent {
            for (side, id, crosses) in ends {
                if crosses {
                    return fail(format!(
                        "The {}-hand id [{}] is modified to traverse the group boundary, but the edge does not pass through two groups.",
                        side, id
                    ));
                }
            }
        }
        self.edges.push(ArchEdge {
            lhs_id: edge.lhs_id,
            lhs_dir: edge.lhs_dir,
            lhs_into: edge.lhs_into,
            lhs_group: edge.lhs_group,
            rhs_id: edge.rhs_id,
            rhs_dir: edge.rhs_dir,
            rhs_into: edge.rhs_into,
            rhs_group: edge.rhs_group,
            title: edge.title,
        });
        Ok(())
    }

    pub fn add_layout_hint(&mut self, hint: ArchLayoutHint) -> Result<()> {
        if hint.members.len() < 2 {
            return fail(format!(
                "An align directive requires at least two members; got {}",
                hint.members.len()
            ));
        }
        let mut seen = HashSet::new();
        for id in &hint.members {
            if self.registered.get(id.as_str()) != Some(&RegisteredKind::Node) {
                return fail(format!(
                    "align {} references [{}], which is not a service or junction",
                    hint.direction.as_str(),
                    id
                ));
            }
            if !seen.insert(id.as_str()) {
                return fail(format!(
                    "align {} lists [{}] more than once",
                    hint.direction.as_str(),
                    id
                ));
            }
        }
        self.layout_hints.push(hint);
        Ok(())
    }
}

fn collapse_newline_indent(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' {
            out.push('\n');
            i += 1;
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

// Mermaid's populateDb: every group, then services, junctions, edges, and alignments.
pub fn populate(ast: ArchitectureAst, db: &mut ArchitectureDb) -> Result<()> {
    if let Some(d) = ast.acc_descr.as_deref().filter(|s| !s.is_empty()) {
        db.acc_descr = Some(collapse_newline_indent(d));
    }
    if let Some(t) = ast.acc_title.as_deref().filter(|s| !s.is_empty()) {
        db.acc_title = Some(t.trim_start_matches(is_space).to_string());
    }
    if let Some(t) = ast.title.filter(|s| !s.is_empty()) {
        db.title = Some(t);
    }
    for group in ast.groups {
        db.add_group(group)?;
    }
    for service in ast.services {
        db.add_service(service)?;
    }
    for junction in ast.junctions {
        db.add_junction(junction)?;
    }
    for edge in ast.edges {
        db.add_edge(edge)?;
    }
    for alignment in ast.alignments {
        db.add_layout_hint(ArchLayoutHint {
            direction: alignment.direction,
            members: alignment.members,
        })?;
    }
    Ok(())
}
