use std::collections::{HashMap, HashSet};

use crate::chars::is_space;
use crate::diagrams::common::tokens::{acc_descr_value, acc_title_value, title_value};
use crate::errors::{locate, ErrorCode, PeleError, Result};

use super::parser::{EventModelAst, FrameKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Ui,
    Processor,
    Command,
    ReadModel,
    Event,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Ui => "ui",
            EntityKind::Processor => "processor",
            EntityKind::Command => "command",
            EntityKind::ReadModel => "readmodel",
            EntityKind::Event => "event",
        }
    }

    pub fn series_slot(self) -> Option<i64> {
        match self {
            EntityKind::Command => Some(0),
            EntityKind::Event => Some(1),
            EntityKind::ReadModel => Some(2),
            EntityKind::Processor => Some(6),
            EntityKind::Ui => None,
        }
    }

    fn from_token(s: &str) -> Self {
        match s {
            "ui" => EntityKind::Ui,
            "pcr" | "processor" => EntityKind::Processor,
            "rmo" | "readmodel" => EntityKind::ReadModel,
            "cmd" | "command" => EntityKind::Command,
            _ => EntityKind::Event,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaneGroup {
    Trigger,
    State,
    Stream,
}

impl LaneGroup {
    fn meta(self) -> (usize, &'static str, &'static str) {
        match self {
            LaneGroup::Trigger => (0, "UI/Automation", "UI/A: "),
            LaneGroup::State => (100, "Command/Read Model", "C/RM: "),
            LaneGroup::Stream => (200, "Events", "Stream: "),
        }
    }
}

fn group_of(kind: EntityKind) -> LaneGroup {
    match kind {
        EntityKind::Ui | EntityKind::Processor => LaneGroup::Trigger,
        EntityKind::Event => LaneGroup::Stream,
        EntityKind::Command | EntityKind::ReadModel => LaneGroup::State,
    }
}

#[derive(Debug, Clone)]
pub struct EmLane {
    pub index: usize,
    pub label: String,
    pub group: LaneGroup,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EmBox {
    pub id: String,
    pub kind: EntityKind,
    pub entity: String,
    pub name: String,
    pub namespace: Option<String>,
    pub reset: bool,
    pub lane_idx: usize,
    pub data: Option<String>,
    pub data_type: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct EmRelation {
    pub source_idx: usize,
    pub target_idx: usize,
}

#[derive(Debug, Clone)]
pub struct EmNote {
    pub frame: String,
    pub text: String,
    pub data_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EmSpecification {
    pub frame: String,
    pub given: Vec<String>,
    pub when: Vec<String>,
    pub then: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EventModelingModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub boxes: Vec<EmBox>,
    pub lanes: Vec<EmLane>,
    pub relations: Vec<EmRelation>,
    pub notes: Vec<EmNote>,
    pub specifications: Vec<EmSpecification>,
    pub entities: Vec<String>,
}

fn clean_acc_descr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut after_nl = false;
    for ch in s.chars() {
        if after_nl && is_space(ch) {
            continue;
        } else if ch == '\n' {
            out.push('\n');
            after_nl = true;
        } else {
            out.push(ch);
            after_nl = false;
        }
    }
    out
}

fn block_text(raw: &str) -> String {
    let open = raw.find('{');
    let close = raw.rfind('}');
    let inner: String = match (open, close) {
        (Some(o), Some(c)) if c > o => raw[o + 1..c].to_string(),
        (None, _) => {
            let chars: Vec<char> = raw.chars().collect();
            if chars.len() >= 2 {
                chars[1..chars.len() - 1].iter().collect()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    };

    let cleaned = inner.replace('\r', "");
    let mut lines: Vec<&str> = cleaned.split('\n').collect();
    while lines.first().is_some_and(|l| l.trim_matches(is_space).is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim_matches(is_space).is_empty()) {
        lines.pop();
    }
    let mut indent = usize::MAX;
    for &line in &lines {
        if line.trim_matches(is_space).is_empty() {
            continue;
        }
        let total_u16 = line.encode_utf16().count();
        let trimmed_u16 = line.trim_start_matches(is_space).encode_utf16().count();
        indent = indent.min(total_u16 - trimmed_u16);
    }
    if indent == usize::MAX {
        indent = 0;
    }
    lines
        .into_iter()
        .map(|line| {
            let mut skipped = 0usize;
            let mut byte_idx = 0usize;
            for (b_i, ch) in line.char_indices() {
                if skipped >= indent {
                    break;
                }
                skipped += ch.len_utf16();
                byte_idx = b_i + ch.len_utf8();
            }
            line[byte_idx..].trim_end_matches(is_space).to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build_event_model(
    ast: EventModelAst,
    title: Option<String>,
    line_offset: usize,
) -> Result<EventModelingModel> {
    let mut ids: HashSet<String> = HashSet::new();
    let src_chars: Vec<char> = ast.source.chars().collect();
    for frame in &ast.frames {
        if ids.contains(&frame.name) {
            let line = locate(&src_chars, frame.offset).line + line_offset;
            let mut err = PeleError::new(
                format!("Duplicate event modeling frame ID \"{}\" on line {}", frame.name, line),
                ErrorCode::Semantic,
            )
            .of("eventmodeling");
            err.line = line;
            return Err(err);
        }
        ids.insert(frame.name.clone());
    }

    let mut data: HashMap<String, String> = HashMap::new();
    for entity in ast.data_entities {
        data.entry(entity.name).or_insert(entity.data_block_value);
    }

    let mut raw_lanes: Vec<EmLane> = Vec::new();
    let mut lane_by_key: HashMap<(LaneGroup, Option<String>), usize> = HashMap::new();
    let mut next_in_group: HashMap<LaneGroup, usize> = HashMap::new();

    let mut get_or_create_lane = |kind: EntityKind, namespace: Option<String>| -> usize {
        let group = group_of(kind);
        let key = (group, namespace.clone());
        if let Some(&idx) = lane_by_key.get(&key) {
            return idx;
        }
        let (first, label, prefix) = group.meta();
        let index = match &namespace {
            None => first,
            Some(_) => {
                let idx = next_in_group.get(&group).copied().unwrap_or(first + 1);
                next_in_group.insert(group, idx + 1);
                idx
            }
        };
        let lane_label = match &namespace {
            None => label.to_string(),
            Some(ns) => format!("{}{}", prefix, ns),
        };
        let raw_idx = raw_lanes.len();
        raw_lanes.push(EmLane {
            index,
            label: lane_label,
            group,
            namespace,
        });
        lane_by_key.insert(key, raw_idx);
        raw_idx
    };

    let mut boxes: Vec<EmBox> = Vec::with_capacity(ast.frames.len());
    let mut relations: Vec<EmRelation> = Vec::new();

    for (index, frame) in ast.frames.into_iter().enumerate() {
        let parts: Vec<&str> = frame.entity_identifier.split('.').collect();
        let namespace = (parts.len() == 2).then(|| parts[0].to_string());
        let name = if parts.len() == 2 {
            parts[1].to_string()
        } else {
            frame.entity_identifier.clone()
        };
        let kind = EntityKind::from_token(&frame.model_entity_type);
        let mut text = frame.data_inline_value.as_deref().map(block_text);
        if let Some(ref data_ref) = frame.data_reference {
            if let Some(block) = data.get(data_ref) {
                text = Some(block_text(block));
            }
        }
        let raw_lane_idx = get_or_create_lane(kind, namespace.clone());
        let reset = frame.kind == FrameKind::Reset;
        boxes.push(EmBox {
            id: frame.name,
            kind,
            entity: frame.entity_identifier,
            name,
            namespace,
            reset,
            lane_idx: raw_lane_idx,
            data: text,
            data_type: frame.data_type,
        });

        if !frame.source_frames.is_empty() {
            let wanted: HashSet<&str> = frame.source_frames.iter().map(String::as_str).collect();
            for (s_idx, src_box) in boxes.iter().enumerate() {
                if wanted.contains(src_box.id.as_str()) {
                    relations.push(EmRelation { source_idx: s_idx, target_idx: index });
                }
            }
        } else if index > 0 && !reset {
            for s_idx in (0..index).rev() {
                if boxes[s_idx].lane_idx != raw_lane_idx {
                    relations.push(EmRelation { source_idx: s_idx, target_idx: index });
                    break;
                }
            }
        }
    }

    // Sort lanes by `lane.index` and remap `box.lane_idx` to the sorted index.
    let mut order: Vec<usize> = (0..raw_lanes.len()).collect();
    order.sort_by_key(|&i| raw_lanes[i].index);
    let mut remap = vec![0usize; raw_lanes.len()];
    let mut sorted_lanes = Vec::with_capacity(raw_lanes.len());
    for (sorted_idx, raw_idx) in order.into_iter().enumerate() {
        remap[raw_idx] = sorted_idx;
        sorted_lanes.push(raw_lanes[raw_idx].clone());
    }
    for b in &mut boxes {
        b.lane_idx = remap[b.lane_idx];
    }

    let resolved_title = ast
        .title
        .map(|t| title_value(&t))
        .filter(|t| !t.is_empty())
        .or(title);
    let acc_title = ast
        .acc_title
        .map(|t| acc_title_value(&t).trim_start_matches(is_space).to_string())
        .filter(|t| !t.is_empty());
    let acc_descr = ast
        .acc_descr
        .map(|d| clean_acc_descr(&acc_descr_value(&d)))
        .filter(|d| !d.is_empty());

    let notes = ast
        .note_entities
        .into_iter()
        .map(|n| EmNote {
            frame: n.source_frame,
            text: block_text(&n.data_block_value),
            data_type: n.data_type,
        })
        .collect();

    let specifications = ast
        .gwt_entities
        .into_iter()
        .map(|g| EmSpecification {
            frame: g.source_frame,
            given: g.given_statements,
            when: g.when_statements,
            then: g.then_statements,
        })
        .collect();

    Ok(EventModelingModel {
        title: resolved_title,
        acc_title,
        acc_descr,
        boxes,
        lanes: sorted_lanes,
        relations,
        notes,
        specifications,
        entities: ast.model_entities,
    })
}
