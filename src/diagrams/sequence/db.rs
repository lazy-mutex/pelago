use super::colors::is_css_color;
use super::types::{
    linetype, OrdMap, SeqActor, SeqBox, SeqBoxData, SeqItem, SeqMessage, SeqMessageBody, SeqNote,
    SeqNoteActor, SeqNumbering, SeqStatement, SeqText,
};
use crate::chars::{is_space, is_word};
use crate::errors::{ErrorCode, PeleError, Result};
use crate::value::{parse_json, parse_yaml, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct SeqDbOptions {
    // The top-level `wrap` setting, which the `%%{wrap}%%` directive also sets.
    pub wrap: Option<bool>,
    // `sequence.wrap`.
    pub sequence_wrap: bool,
}

fn fail(message: impl Into<String>) -> PeleError {
    PeleError::new(message, ErrorCode::Semantic).of("sequence")
}

fn extract_wrap(raw: &str) -> SeqText {
    let text = raw.trim_matches(is_space);
    let (wrap, prefix_len) = if text.starts_with(":wrap:") {
        (Some(true), 6)
    } else if text.starts_with("wrap:") {
        (Some(true), 5)
    } else if text.starts_with(":nowrap:") {
        (Some(false), 8)
    } else if text.starts_with("nowrap:") {
        (Some(false), 7)
    } else {
        (None, 0)
    };
    let rest = if wrap.is_some() {
        &text[prefix_len..]
    } else {
        text
    };
    SeqText {
        text: rest.trim_matches(is_space).to_string(),
        wrap,
    }
}

fn has_line_terminator(s: &str) -> bool {
    s.contains(['\r', '\u{2028}', '\u{2029}'])
}

// Matches /^(?:rgba?|hsla?)\s*\(/ and returns byte index right after `(`.
fn match_box_function(s: &str) -> Option<usize> {
    let prefix_len = if s.starts_with("rgba") || s.starts_with("hsla") {
        4
    } else if s.starts_with("rgb") || s.starts_with("hsl") {
        3
    } else {
        return None;
    };
    let rest = s[prefix_len..].trim_start_matches(is_space);
    if rest.starts_with('(') {
        Some(s.len() - rest.len() + 1)
    } else {
        None
    }
}

// Splits a box line into a leading color and the rest, as Mermaid's
// /^((?:rgba?|hsla?)\s*\(.*\)|\w*)(.*)$/ does. `.` stops at a line terminator, so a line that
// holds one outside the gap before the parenthesis does not match at all.
fn split_box(s: &str) -> Option<(&str, &str)> {
    if let Some(fn_end) = match_box_function(s) {
        if let Some(close) = s.rfind(')') {
            if close >= fn_end {
                if has_line_terminator(&s[fn_end..]) {
                    return None;
                }
                return Some((&s[..close + 1], &s[close + 1..]));
            }
        }
    }
    if has_line_terminator(s) {
        return None;
    }
    let word_end = s
        .char_indices()
        .take_while(|&(_, c)| is_word(c))
        .last()
        .map_or(0, |(idx, c)| idx + c.len_utf8());
    Some((&s[..word_end], &s[word_end..]))
}

fn entries(value: Value) -> Vec<(String, Value)> {
    match value {
        Value::Str(s) => s
            .chars()
            .enumerate()
            .map(|(i, ch)| (i.to_string(), Value::Str(ch.to_string())))
            .collect(),
        Value::Arr(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect(),
        Value::Obj(list) => list,
        _ => Vec::new(),
    }
}

fn js_slice(chars: &[char], start: isize, end: isize) -> String {
    let len = chars.len() as isize;
    let clamp = |k: isize| -> usize {
        if k < 0 {
            (len + k).max(0) as usize
        } else {
            k.min(len) as usize
        }
    };
    let s = clamp(start);
    let e = clamp(end);
    if s >= e {
        String::new()
    } else {
        chars[s..e].iter().collect()
    }
}

pub struct SeqDb {
    pub actors: OrdMap<String, SeqActor>,
    pub created_actors: HashMap<String, usize>,
    pub destroyed_actors: HashMap<String, usize>,
    pub boxes: Vec<SeqBox>,
    pub messages: Vec<SeqMessage>,
    pub notes: Vec<SeqNote>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    prev_actor: Option<String>,
    current_box: Option<usize>,
    last_created: Option<String>,
    last_destroyed: Option<String>,
    wrap_enabled: Option<bool>,
    sequence_wrap: bool,
    active: HashMap<String, i32>,
}

impl SeqDb {
    pub fn new(options: SeqDbOptions) -> Self {
        Self {
            actors: OrdMap::new(),
            created_actors: HashMap::new(),
            destroyed_actors: HashMap::new(),
            boxes: Vec::new(),
            messages: Vec::new(),
            notes: Vec::new(),
            title: None,
            acc_title: None,
            acc_descr: None,
            prev_actor: None,
            current_box: None,
            last_created: None,
            last_destroyed: None,
            wrap_enabled: options.wrap,
            sequence_wrap: options.sequence_wrap,
            active: HashMap::new(),
        }
    }

    pub fn auto_wrap(&self) -> bool {
        self.wrap_enabled.unwrap_or(self.sequence_wrap)
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        // Matches text.replace(/\n\s+/g, '\n')
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
        self.acc_descr = Some(out);
    }

    pub fn set_diagram_title(&mut self, text: String) {
        self.title = Some(text);
    }

    pub fn parse_message(&self, s: &str) -> SeqText {
        extract_wrap(s)
    }

    pub fn parse_box_data(&self, s: &str) -> SeqBoxData {
        let m = split_box(s);
        let mut color = match m {
            Some((c, _)) if !c.is_empty() => c.trim_matches(is_space).to_string(),
            _ => "transparent".to_string(),
        };
        let mut title = match m {
            Some((_, rest)) if !rest.is_empty() => Some(rest.trim_matches(is_space).to_string()),
            _ => None,
        };
        if !is_css_color(&color) {
            color = "transparent".to_string();
            title = Some(s.trim_matches(is_space).to_string());
        }
        let Some(title) = title else {
            return SeqBoxData {
                text: None,
                color,
                wrap: None,
            };
        };
        let SeqText { text, wrap } = extract_wrap(&title);
        SeqBoxData {
            text: if text.is_empty() { None } else { Some(text) },
            color,
            wrap,
        }
    }

    pub fn add_box(&mut self, data: SeqBoxData) {
        let idx = self.boxes.len();
        self.boxes.push(SeqBox {
            name: data.text,
            wrap: data.wrap.unwrap_or_else(|| self.auto_wrap()),
            fill: data.color,
            actor_keys: Vec::new(),
        });
        self.current_box = Some(idx);
    }

    pub fn add_actor(
        &mut self,
        id: &str,
        name: &str,
        mut description: Option<SeqText>,
        mut actor_type: Option<String>,
        metadata: Option<&str>,
    ) -> Result<()> {
        let mut assigned_box = self.current_box;
        let mut doc: Option<Value> = None;
        if let Some(meta) = metadata {
            let yaml_src = if meta.contains('\n') {
                format!("{}\n", meta)
            } else {
                format!("{{\n{}\n}}", meta)
            };
            let parsed = parse_yaml(&yaml_src).map_err(|err| {
                PeleError::new(
                    format!("Config of participant \"{}\" is not valid YAML. {}", id, err),
                    ErrorCode::Syntax,
                )
                .of("sequence")
            })?;
            if parsed.is_object() {
                doc = Some(parsed);
            }
        }
        if let Some(ref d) = doc {
            if d.has("type") {
                let v = d.get("type");
                if !matches!(v, Value::Null) {
                    actor_type = Some(v.to_text());
                }
            }
            if d.has("alias") {
                let alias_val = d.get("alias");
                if alias_val.truthy()
                    && (description.is_none()
                        || description.as_ref().is_some_and(|desc| desc.text == name))
                {
                    description = Some(SeqText {
                        text: alias_val.to_text(),
                        wrap: description.and_then(|desc| desc.wrap),
                    });
                }
            }
        }

        if let Some(old) = self.actors.get_mut(id) {
            if let (Some(cur_b), Some(old_b)) = (self.current_box, old.box_idx) {
                if cur_b != old_b {
                    let old_name = old.name.clone();
                    let old_box_name = self.boxes[old_b]
                        .name
                        .as_deref()
                        .unwrap_or("undefined")
                        .to_string();
                    let cur_box_name = self.boxes[cur_b]
                        .name
                        .as_deref()
                        .unwrap_or("undefined")
                        .to_string();
                    return Err(fail(format!(
                        "A same participant should only be defined in one Box: {} can't be in '{}' and in '{}' at the same time.",
                        old_name, old_box_name, cur_box_name
                    )));
                }
            }
            assigned_box = old.box_idx.or(self.current_box);
            old.box_idx = assigned_box;
            if name == old.name && description.is_none() {
                return Ok(());
            }
        }

        // A participant that is only mentioned in a message has no kind and takes its name as its label.
        let (desc_text, desc_wrap) = match description {
            Some(d) if actor_type.is_some() => (d.text, d.wrap),
            _ => (name.to_string(), None),
        };
        let auto_wrap = self.auto_wrap();
        let prev_actor = self.prev_actor.clone();
        self.actors.insert(
            id.to_string(),
            SeqActor {
                box_idx: assigned_box,
                name: name.to_string(),
                description: desc_text,
                wrap: desc_wrap.unwrap_or(auto_wrap),
                prev_actor: prev_actor.clone(),
                next_actor: None,
                links: OrdMap::new(),
                properties: OrdMap::new(),
                details: None,
                kind: actor_type.unwrap_or_else(|| "participant".to_string()),
            },
        );
        if let Some(ref prev_id) = prev_actor {
            if let Some(prev) = self.actors.get_mut(prev_id) {
                prev.next_actor = Some(id.to_string());
            }
        }
        if let Some(cur_b) = self.current_box {
            self.boxes[cur_b].actor_keys.push(id.to_string());
        }
        self.prev_actor = Some(id.to_string());
        Ok(())
    }

    #[allow(dead_code)]
    pub fn add_message(&mut self, from: String, to: String, message: SeqText) {
        let wrap = message.wrap.unwrap_or_else(|| self.auto_wrap());
        self.messages.push(SeqMessage {
            id: self.messages.len().to_string(),
            from: Some(from),
            to: Some(to),
            message: SeqMessageBody::Text(message.text),
            wrap,
            kind: None,
            activate: None,
            placement: None,
            central_connection: None,
        });
    }

    pub fn add_signal(
        &mut self,
        from: Option<String>,
        to: Option<String>,
        message: Option<SeqText>,
        kind: Option<i32>,
        activate: bool,
        central_connection: Option<i32>,
    ) -> Result<bool> {
        // Mermaid recounts every earlier message here. A running count per participant says the same.
        if kind == Some(linetype::ACTIVE_END) {
            let count = from
                .as_deref()
                .and_then(|f| self.active.get(f).copied())
                .unwrap_or(0);
            if count < 1 {
                let who = from.as_deref().unwrap_or("undefined");
                return Err(fail(format!(
                    "Trying to inactivate an inactive participant ({})",
                    who
                )));
            }
            if let Some(ref f) = from {
                self.active.insert(f.clone(), count - 1);
            }
        } else if kind == Some(linetype::ACTIVE_START) {
            if let Some(ref f) = from {
                let count = self.active.get(f).copied().unwrap_or(0);
                self.active.insert(f.clone(), count + 1);
            }
        }
        let auto_wrap = self.auto_wrap();
        let (msg_text, msg_wrap) = match message {
            Some(m) => (m.text, m.wrap.unwrap_or(auto_wrap)),
            None => (String::new(), auto_wrap),
        };
        self.messages.push(SeqMessage {
            id: self.messages.len().to_string(),
            from,
            to,
            message: SeqMessageBody::Text(msg_text),
            wrap: msg_wrap,
            kind,
            activate: Some(activate),
            placement: None,
            central_connection: Some(central_connection.unwrap_or(0)),
        });
        Ok(true)
    }

    pub fn add_note(&mut self, actor: SeqNoteActor, placement: i32, message: SeqText) {
        let wrap = message.wrap.unwrap_or_else(|| self.auto_wrap());
        let (from, to) = match &actor {
            SeqNoteActor::Single(a) => (a.clone(), a.clone()),
            SeqNoteActor::Pair([a, b]) => (a.clone(), b.clone()),
        };
        self.notes.push(SeqNote {
            actor,
            placement,
            message: message.text.clone(),
            wrap,
        });
        self.messages.push(SeqMessage {
            id: self.messages.len().to_string(),
            from: Some(from),
            to: Some(to),
            message: SeqMessageBody::Text(message.text),
            wrap,
            kind: Some(linetype::NOTE),
            activate: None,
            placement: Some(placement),
            central_connection: None,
        });
    }

    // Mermaid logs and ignores menu text that is not valid JSON.
    pub fn add_links(&mut self, actor_id: &str, text: &SeqText) {
        let Some(actor) = self.actors.get_mut(actor_id) else {
            return;
        };
        if let Ok(parsed) = parse_json(&text.text) {
            for (key, value) in entries(parsed) {
                actor.links.insert(key, value);
            }
        }
    }

    pub fn add_a_link(&mut self, actor_id: &str, text: &SeqText) {
        let Some(actor) = self.actors.get_mut(actor_id) else {
            return;
        };
        let chars: Vec<char> = text.text.chars().collect();
        let sep: isize = chars.iter().position(|&c| c == '@').map_or(-1, |i| i as isize);
        let key = js_slice(&chars, 0, sep - 1)
            .trim_matches(is_space)
            .to_string();
        let val = js_slice(&chars, sep + 1, chars.len() as isize)
            .trim_matches(is_space)
            .to_string();
        actor.links.insert(key, Value::Str(val));
    }

    pub fn add_properties(&mut self, actor_id: &str, text: &SeqText) {
        let Some(actor) = self.actors.get_mut(actor_id) else {
            return;
        };
        if let Ok(parsed) = parse_json(&text.text) {
            for (key, value) in entries(parsed) {
                actor.properties.insert(key, value);
            }
        }
    }

    // Mermaid reads the properties and links from a page element with this id. Pele keeps the id.
    pub fn add_details(&mut self, actor_id: &str, text: &SeqText) {
        if let Some(actor) = self.actors.get_mut(actor_id) {
            actor.details = Some(text.text.clone());
        }
    }

    // Mermaid walks the nested statement lists by recursion; an explicit stack takes any depth.
    pub fn apply(&mut self, root: SeqItem) -> Result<()> {
        let mut lists: Vec<std::vec::IntoIter<SeqItem>> = Vec::new();
        let mut item: Option<SeqItem> = Some(root);
        loop {
            if let Some(cur) = item.take() {
                match cur {
                    SeqItem::List(vec) => lists.push(vec.into_iter()),
                    SeqItem::Stmt(stmt) => self.apply_one(*stmt)?,
                    SeqItem::Str(_) => {}
                }
            }
            let Some(top) = lists.last_mut() else {
                return Ok(());
            };
            if let Some(next) = top.next() {
                item = Some(next);
            } else {
                lists.pop();
            }
        }
    }

    fn apply_one(&mut self, param: SeqStatement) -> Result<()> {
        let actor_str = param
            .actor
            .as_ref()
            .and_then(|a| a.as_single())
            .map(|s| s.to_string());
        match param.kind {
            "sequenceIndex" => {
                self.messages.push(SeqMessage {
                    id: self.messages.len().to_string(),
                    from: None,
                    to: None,
                    message: SeqMessageBody::Numbering(SeqNumbering {
                        start: param.sequence_index,
                        step: param.sequence_index_step,
                        visible: param.sequence_visible == Some(true),
                    }),
                    wrap: false,
                    kind: param.signal_type,
                    activate: None,
                    placement: None,
                    central_connection: None,
                });
            }
            "addParticipant" => {
                let a = actor_str.as_deref().unwrap_or("");
                self.add_actor(a, a, param.description, param.draw, param.config.as_deref())?;
            }
            "createParticipant" => {
                let a = actor_str.unwrap_or_default();
                if self.actors.contains_key(&a) {
                    return Err(fail(
                        "It is not possible to have actors with the same id, even if one is destroyed before the next is created. Use 'AS' aliases to simulate the behavior",
                    ));
                }
                self.last_created = Some(a.clone());
                self.add_actor(&a, &a, param.description, param.draw, param.config.as_deref())?;
                self.created_actors.insert(a, self.messages.len());
            }
            "destroyParticipant" => {
                let a = actor_str.unwrap_or_default();
                self.last_destroyed = Some(a.clone());
                self.destroyed_actors.insert(a, self.messages.len());
            }
            "activeStart"
            | "activeEnd"
            | "centralConnection"
            | "centralConnectionReverse" => {
                self.add_signal(actor_str, None, None, param.signal_type, false, None)?;
            }
            "addNote" => {
                if let (Some(act), Some(pl), Some(txt)) = (param.actor, param.placement, param.text)
                {
                    self.add_note(act, pl, txt);
                }
            }
            "addLinks" => {
                if let (Some(a), Some(ref txt)) = (actor_str.as_deref(), param.text) {
                    self.add_links(a, txt);
                }
            }
            "addALink" => {
                if let (Some(a), Some(ref txt)) = (actor_str.as_deref(), param.text) {
                    self.add_a_link(a, txt);
                }
            }
            "addProperties" => {
                if let (Some(a), Some(ref txt)) = (actor_str.as_deref(), param.text) {
                    self.add_properties(a, txt);
                }
            }
            "addDetails" => {
                if let (Some(a), Some(ref txt)) = (actor_str.as_deref(), param.text) {
                    self.add_details(a, txt);
                }
            }
            "addMessage" => {
                if let Some(ref created) = self.last_created {
                    if param.to.as_deref() != Some(created.as_str()) {
                        return Err(fail(format!(
                            "The created participant {} does not have an associated creating message after its declaration. Please check the sequence diagram.",
                            created
                        )));
                    }
                    self.last_created = None;
                } else if let Some(ref destroyed) = self.last_destroyed {
                    if param.to.as_deref() != Some(destroyed.as_str())
                        && param.from.as_deref() != Some(destroyed.as_str())
                    {
                        return Err(fail(format!(
                            "The destroyed participant {} does not have an associated destroying message after its declaration. Please check the sequence diagram.",
                            destroyed
                        )));
                    }
                    self.last_destroyed = None;
                }
                self.add_signal(
                    param.from,
                    param.to,
                    param.msg,
                    param.signal_type,
                    param.activate.unwrap_or(false),
                    param.central_connection,
                )?;
            }
            "boxStart" => {
                if let Some(data) = param.box_data {
                    self.add_box(data);
                }
            }
            "boxEnd" => {
                self.current_box = None;
            }
            "loopStart" => {
                self.add_signal(None, None, param.loop_text, param.signal_type, false, None)?;
            }
            "rectStart" => {
                self.add_signal(None, None, param.color, param.signal_type, false, None)?;
            }
            "optStart" => {
                self.add_signal(None, None, param.opt_text, param.signal_type, false, None)?;
            }
            "altStart" | "else" => {
                self.add_signal(None, None, param.alt_text, param.signal_type, false, None)?;
            }
            "parStart" | "and" => {
                self.add_signal(None, None, param.par_text, param.signal_type, false, None)?;
            }
            "criticalStart" => {
                self.add_signal(
                    None,
                    None,
                    param.critical_text,
                    param.signal_type,
                    false,
                    None,
                )?;
            }
            "option" => {
                self.add_signal(
                    None,
                    None,
                    param.option_text,
                    param.signal_type,
                    false,
                    None,
                )?;
            }
            "breakStart" => {
                self.add_signal(None, None, param.break_text, param.signal_type, false, None)?;
            }
            "loopEnd" | "rectEnd" | "optEnd" | "altEnd" | "parEnd" | "criticalEnd" | "breakEnd" => {
                self.add_signal(None, None, None, param.signal_type, false, None)?;
            }
            _ => {}
        }
        Ok(())
    }
}
