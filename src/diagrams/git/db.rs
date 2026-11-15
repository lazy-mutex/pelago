use std::cmp::Ordering;
use std::collections::HashMap;

use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use crate::value::Value;

use super::parser::{CommitTypeName, GitDirection, GitGraphAst, StatementAst};

pub mod commit_type {
    pub const NORMAL: usize = 0;
    pub const REVERSE: usize = 1;
    pub const HIGHLIGHT: usize = 2;
    pub const MERGE: usize = 3;
    pub const CHERRY_PICK: usize = 4;
}

#[derive(Debug, Clone)]
pub struct GitCommit {
    pub id: String,
    pub message: String,
    pub seq: usize,
    pub commit_type: usize,
    pub tags: Vec<String>,
    pub parents: Vec<String>,
    pub branch: String,
    pub custom_type: Option<usize>,
    pub custom_id: bool,
}

#[derive(Debug, Clone)]
pub struct GitModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub direction: GitDirection,
    pub commits: HashMap<String, GitCommit>,
    pub branches: HashMap<String, Option<String>>,
    // Branch names in drawing order: by `order`, then by creation.
    pub lanes: Vec<String>,
    pub current_branch: String,
    pub head: Option<GitCommit>,
    pub warnings: Vec<String>,
}

fn fail(message: impl Into<String>) -> PeleError {
    PeleError::new(message, ErrorCode::Semantic).of("gitGraph")
}

fn type_code(name: CommitTypeName) -> usize {
    match name {
        CommitTypeName::Reverse => commit_type::REVERSE,
        CommitTypeName::Highlight => commit_type::HIGHLIGHT,
        CommitTypeName::Normal => commit_type::NORMAL,
    }
}

// Replaces /\n\s+/g with "\n".
fn collapse_newline_indent(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\n' {
            out.push('\n');
            i += 1;
            if i < chars.len() && is_space(chars[i]) {
                while i < chars.len() && is_space(chars[i]) {
                    i += 1;
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn build_git(ast: GitGraphAst, config: &Value, title: Option<String>) -> Result<GitModel> {
    let git_cfg = config.get("gitGraph");
    let main_name = git_cfg
        .get("mainBranchName")
        .as_str()
        .unwrap_or("main")
        .to_string();
    let main_order = git_cfg.get("mainBranchOrder").as_f64().unwrap_or(0.0);

    let mut commits: HashMap<String, GitCommit> = HashMap::new();
    let mut branches: HashMap<String, Option<String>> = HashMap::new();
    branches.insert(main_name.clone(), None);
    let mut orders: Vec<(String, Option<f64>)> = vec![(main_name.clone(), Some(main_order))];
    let mut warnings: Vec<String> = Vec::new();
    let mut head: Option<GitCommit> = None;
    let mut current = main_name;
    let mut direction = ast.dir.unwrap_or(GitDirection::Lr);
    let mut seq = 0usize;

    let next_id = |seq_val: usize, commits_map: &HashMap<String, GitCommit>| -> String {
        let mut salt = (seq_val as u32).wrapping_add(1);
        loop {
            let hash = salt.wrapping_mul(0x9e37_79b1);
            let hex = format!("{:08x}", hash);
            let id = format!("{}-{}", seq_val, &hex[..7]);
            if !commits_map.contains_key(&id) {
                return id;
            }
            salt = salt.wrapping_add(0x9e37_79b1);
        }
    };

    let checkout = |branch: &str,
                    branches_map: &HashMap<String, Option<String>>,
                    commits_map: &HashMap<String, GitCommit>,
                    current_out: &mut String,
                    head_out: &mut Option<GitCommit>|
     -> Result<()> {
        let Some(head_id_opt) = branches_map.get(branch) else {
            return Err(fail(format!(
                "Trying to checkout branch which is not yet created. (Help try using \"branch {}\")",
                branch
            )));
        };
        *current_out = branch.to_string();
        *head_out = head_id_opt
            .as_deref()
            .and_then(|id| commits_map.get(id).cloned());
        Ok(())
    };

    let _ = &mut direction;
    for statement in ast.statements {
        match statement {
            StatementAst::Commit { id, message, tags, commit_type: ct } => {
                let cid = match id {
                    Some(s) if !s.is_empty() => s,
                    _ => next_id(seq, &commits),
                };
                let commit = GitCommit {
                    id: cid,
                    message: message.unwrap_or_default(),
                    seq,
                    commit_type: ct.map_or(commit_type::NORMAL, type_code),
                    tags,
                    parents: match &head {
                        None => Vec::new(),
                        Some(h) => vec![h.id.clone()],
                    },
                    branch: current.clone(),
                    custom_type: None,
                    custom_id: false,
                };
                seq += 1;
                if commits.contains_key(&commit.id) {
                    warnings.push(format!("Commit ID {} already exists", commit.id));
                }
                head = Some(commit.clone());
                branches.insert(current.clone(), Some(commit.id.clone()));
                commits.insert(commit.id.clone(), commit);
            }
            StatementAst::Branch { name, order } => {
                if branches.contains_key(&name) {
                    return Err(fail(format!(
                        "Trying to create an existing branch. (Help: Either use a new name if you want create a new branch or try using \"checkout {}\")",
                        name
                    )));
                }
                branches.insert(name.clone(), head.as_ref().map(|h| h.id.clone()));
                orders.push((name.clone(), Some(order.unwrap_or(0.0))));
                checkout(&name, &branches, &commits, &mut current, &mut head)?;
            }
            StatementAst::Merge { branch: other, id, tags, commit_type: ct } => {
                let custom_id = id.unwrap_or_default();
                let current_head = branches.get(&current).and_then(|o| o.as_deref());
                let other_head = branches.get(&other).and_then(|o| o.as_deref());
                let current_commit = current_head.and_then(|cid| commits.get(cid));
                let other_commit = other_head.and_then(|cid| commits.get(cid));
                if let (Some(cc), Some(_oc)) = (current_commit, other_commit) {
                    if cc.branch == other {
                        return Err(fail(format!("Cannot merge branch '{}' into itself.", other)));
                    }
                }
                if current == other {
                    return Err(fail("Incorrect usage of \"merge\". Cannot merge a branch to itself"));
                }
                if current_commit.is_none() {
                    return Err(fail(format!(
                        "Incorrect usage of \"merge\". Current branch ({})has no commits",
                        current
                    )));
                }
                if !branches.contains_key(&other) {
                    return Err(fail(format!(
                        "Incorrect usage of \"merge\". Branch to be merged ({}) does not exist",
                        other
                    )));
                }
                if other_commit.is_none() {
                    return Err(fail(format!(
                        "Incorrect usage of \"merge\". Branch to be merged ({}) has no commits",
                        other
                    )));
                }
                if current_head == other_head {
                    return Err(fail("Incorrect usage of \"merge\". Both branches have same head"));
                }
                if !custom_id.is_empty() && commits.contains_key(&custom_id) {
                    return Err(fail(format!(
                        "Incorrect usage of \"merge\". Commit with id:{} already exists, use different custom id",
                        custom_id
                    )));
                }
                let has_custom_id = !custom_id.is_empty();
                let cid = if has_custom_id {
                    custom_id
                } else {
                    next_id(seq, &commits)
                };
                let parents = match &head {
                    None => Vec::new(),
                    Some(h) => vec![h.id.clone(), other_head.unwrap_or("").to_string()],
                };
                let commit = GitCommit {
                    id: cid,
                    message: format!("merged branch {} into {}", other, current),
                    seq,
                    parents,
                    branch: current.clone(),
                    commit_type: commit_type::MERGE,
                    custom_type: ct.map(type_code),
                    custom_id: has_custom_id,
                    tags,
                };
                seq += 1;
                head = Some(commit.clone());
                branches.insert(current.clone(), Some(commit.id.clone()));
                commits.insert(commit.id.clone(), commit);
            }
            StatementAst::Checkout { branch } => {
                checkout(&branch, &branches, &commits, &mut current, &mut head)?;
            }
            StatementAst::CherryPicking { id: source_id, tags, parent } => {
                let source = source_id
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .and_then(|sid| commits.get(sid))
                    .cloned();
                let Some(source) = source else {
                    return Err(fail(
                        "Incorrect usage of \"cherryPick\". Source commit id should exist and provided",
                    ));
                };
                let parent_non_empty = parent.as_deref().filter(|p| !p.is_empty());
                if let Some(p) = parent_non_empty {
                    if !source.parents.iter().any(|sp| sp == p) {
                        return Err(fail(
                            "Invalid operation: The specified parent commit is not an immediate parent of the cherry-picked commit.",
                        ));
                    }
                }
                if source.commit_type == commit_type::MERGE && parent_non_empty.is_none() {
                    return Err(fail(
                        "Incorrect usage of cherry-pick: If the source commit is a merge commit, an immediate parent commit must be specified.",
                    ));
                }
                if source.branch == current {
                    return Err(fail(
                        "Incorrect usage of \"cherryPick\". Source commit is already on current branch",
                    ));
                }
                let current_head = branches.get(&current).and_then(|o| o.as_deref());
                if !current_head.is_some_and(|ch| commits.contains_key(ch)) {
                    return Err(fail(format!(
                        "Incorrect usage of \"cherry-pick\". Current branch ({})has no commits",
                        current
                    )));
                }
                let cid = next_id(seq, &commits);
                let parents = match &head {
                    None => Vec::new(),
                    Some(h) => vec![h.id.clone(), source.id.clone()],
                };
                let final_tags = if !tags.is_empty() {
                    tags.into_iter().filter(|t| !t.is_empty()).collect()
                } else {
                    let parent_suffix = if source.commit_type == commit_type::MERGE {
                        format!("|parent:{}", parent_non_empty.unwrap_or(""))
                    } else {
                        String::new()
                    };
                    vec![format!("cherry-pick:{}{}", source.id, parent_suffix)]
                };
                let commit = GitCommit {
                    id: cid,
                    message: format!("cherry-picked {} into {}", source.message, current),
                    seq,
                    parents,
                    branch: current.clone(),
                    commit_type: commit_type::CHERRY_PICK,
                    custom_type: None,
                    custom_id: false,
                    tags: final_tags,
                };
                seq += 1;
                head = Some(commit.clone());
                branches.insert(current.clone(), Some(commit.id.clone()));
                commits.insert(commit.id.clone(), commit);
            }
        }
    }

    // A branch made without an order sorts by creation among those of order zero, as in Mermaid.
    let mut lane_entries: Vec<(String, f64)> = orders
        .into_iter()
        .enumerate()
        .map(|(i, (name, order))| {
            let ord = order.unwrap_or_else(|| format!("0.{}", i).parse::<f64>().unwrap_or(0.0));
            (name, ord)
        })
        .collect();
    lane_entries.sort_by(|a, b| {
        let diff = a.1 - b.1;
        if diff < 0.0 {
            Ordering::Less
        } else if diff > 0.0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
    let lanes = lane_entries.into_iter().map(|(name, _)| name).collect();

    let resolved_title = ast
        .title
        .filter(|s| !s.is_empty())
        .or_else(|| title.filter(|s| !s.is_empty()));
    let resolved_acc_title = ast
        .acc_title
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_start_matches(is_space).to_string());
    let resolved_acc_descr = ast
        .acc_descr
        .filter(|s| !s.is_empty())
        .map(|s| collapse_newline_indent(&s));

    Ok(GitModel {
        title: resolved_title,
        acc_title: resolved_acc_title,
        acc_descr: resolved_acc_descr,
        direction,
        commits,
        branches,
        lanes,
        current_branch: current,
        head,
        warnings,
    })
}
