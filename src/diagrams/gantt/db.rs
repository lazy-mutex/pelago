use super::dates::{
    add, add_days, format, is_valid, iso_date, parse_js_date, parse_strict, JsDate, Unit, WEEKDAYS,
};
use crate::chars::{is_space, is_word};
use crate::errors::{ErrorCode, PeleError, Result};
use crate::url::safe_url;
use std::collections::{HashMap, HashSet};

const WORK_LIMIT: usize = 400_000_000;
const DAY_STEP: usize = 400;
const DATED_STEP: usize = 200;
const FORMAT_STEP: usize = 400;

#[derive(Debug, Clone)]
pub struct RawTask {
    pub data: String,
    pub start: Option<String>,
    pub end: String,
}

#[derive(Debug, Clone)]
pub struct GanttTask {
    pub id: String,
    pub task: String,
    pub section: String,
    pub start_time: Option<JsDate>,
    pub end_time: Option<JsDate>,
    pub render_end_time: Option<JsDate>,
    pub manual_end_time: bool,
    pub processed: bool,
    pub active: bool,
    pub done: bool,
    pub crit: bool,
    pub milestone: bool,
    pub vert: bool,
    pub order: i32,
    pub classes: Vec<String>,
    pub prev_task_id: Option<String>,
    pub raw: RawTask,
}

// The one place the current time is read.
pub fn clock(now: Option<f64>) -> f64 {
    if let Some(given) = now.filter(|v| v.is_finite()) {
        return given.trunc();
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        // Deterministic fallback in sandboxed WASM environments without a host clock.
        1_704_067_200_000.0
    }
}

pub fn parse_duration(raw: &str) -> (f64, Unit) {
    // `/^(\d+(?:\.\d+)?)([Mdhmswy]|ms)$/`
    let s = raw.trim_matches(is_space);
    let (num_part, unit) = if let Some(n) = s.strip_suffix("ms") {
        (n, Unit::Ms)
    } else if let Some((idx, ch)) = s.char_indices().next_back() {
        let u = match ch {
            'M' => Unit::Month,
            'd' => Unit::D,
            'h' => Unit::H,
            'm' => Unit::Min,
            's' => Unit::S,
            'w' => Unit::W,
            'y' => Unit::Y,
            _ => return (f64::NAN, Unit::Ms),
        };
        (&s[..idx], u)
    } else {
        return (f64::NAN, Unit::Ms);
    };

    if num_part.is_empty() {
        return (f64::NAN, Unit::Ms);
    }
    let valid_num = match num_part.split_once('.') {
        None => num_part.bytes().all(|b| b.is_ascii_digit()),
        Some((int_p, frac_p)) => {
            !int_p.is_empty()
                && !frac_p.is_empty()
                && int_p.bytes().all(|b| b.is_ascii_digit())
                && frac_p.bytes().all(|b| b.is_ascii_digit())
        }
    };
    if !valid_num {
        return (f64::NAN, Unit::Ms);
    }
    match num_part.parse::<f64>() {
        Ok(v) => (v, unit),
        Err(_) => (f64::NAN, Unit::Ms),
    }
}

fn merge_tokens(existing: &[String], txt: &str) -> Vec<String> {
    let mut merged = existing.to_vec();
    for tok in txt.to_lowercase().split(|c: char| is_space(c) || c == ',') {
        if !tok.is_empty() && !merged.iter().any(|e| e == tok) {
            merged.push(tok.to_string());
        }
    }
    merged
}

fn is_numeric_format(s: &str) -> bool {
    // `/^[YMDHhmsSZXx\-/.:, T]*$/`
    s.chars().all(|c| {
        matches!(
            c,
            'Y' | 'M' | 'D' | 'H' | 'h' | 'm' | 's' | 'S' | 'Z' | 'X' | 'x' | '-' | '/' | '.' | ':'
                | ',' | ' ' | 'T'
        )
    })
}

fn match_tag(s: &str) -> Option<&str> {
    let trimmed = s.trim_matches(is_space);
    match trimmed {
        "active" | "done" | "crit" | "milestone" | "vert" => Some(trimmed),
        _ => None,
    }
}

fn match_ref_keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    // `/^<kw>\s+([\d\w- ]+)/`
    let rest = s.strip_prefix(kw)?;
    let mut chars = rest.char_indices();
    let (_, first_sp) = chars.next()?;
    if !is_space(first_sp) {
        return None;
    }
    let mut list_start = first_sp.len_utf8();
    for (idx, ch) in chars {
        if is_space(ch) {
            list_start = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    let tail = &rest[list_start..];
    let mut list_end = 0usize;
    for (idx, ch) in tail.char_indices() {
        if is_word(ch) || ch == '-' || ch == ' ' {
            list_end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    (list_end > 0).then_some(&tail[..list_end])
}

fn collapse_newline_spaces(txt: &str) -> String {
    // `txt.replace(/\n\s+/g, '\n')`
    let chars: Vec<char> = txt.chars().collect();
    let mut out = String::with_capacity(txt.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' && chars.get(i + 1).is_some_and(|&c| is_space(c)) {
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

pub struct GanttDb {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub date_format: String,
    pub axis_format: String,
    pub tick_interval: Option<String>,
    pub today_marker: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub inclusive_end_dates: bool,
    pub top_axis: bool,
    pub display_mode: String,
    pub weekday: Option<String>,
    pub weekend: String,
    pub sections: Vec<String>,
    pub links: HashMap<String, String>,
    pub tasks: Vec<GanttTask>,

    by_id: HashMap<String, usize>,
    current_section: String,
    task_count: usize,
    last_order: i32,
    last_task_id: Option<String>,
    now: f64,
    work: usize,
    include_set: HashSet<String>,
    exclude_set: HashSet<String>,
    excluded_days: [bool; 7],
    dated_tokens: bool,
}

impl Default for GanttDb {
    fn default() -> Self {
        Self::new()
    }
}

impl GanttDb {
    pub fn new() -> Self {
        Self {
            title: None,
            acc_title: None,
            acc_descr: None,
            date_format: String::new(),
            axis_format: String::new(),
            tick_interval: None,
            today_marker: String::new(),
            includes: Vec::new(),
            excludes: Vec::new(),
            inclusive_end_dates: false,
            top_axis: false,
            display_mode: String::new(),
            weekday: None,
            weekend: "saturday".to_string(),
            sections: Vec::new(),
            links: HashMap::new(),
            tasks: Vec::new(),
            by_id: HashMap::new(),
            current_section: String::new(),
            task_count: 0,
            last_order: 0,
            last_task_id: None,
            now: 0.0,
            work: 0,
            include_set: HashSet::new(),
            exclude_set: HashSet::new(),
            excluded_days: [false; 7],
            dated_tokens: false,
        }
    }

    pub fn set_date_format(&mut self, txt: &str) {
        self.date_format = txt.to_string();
    }

    pub fn enable_inclusive_end_dates(&mut self) {
        self.inclusive_end_dates = true;
    }

    pub fn top_axis(&mut self) {
        self.top_axis = true;
    }

    pub fn set_axis_format(&mut self, txt: &str) {
        self.axis_format = txt.to_string();
    }

    pub fn set_tick_interval(&mut self, txt: &str) {
        self.tick_interval = Some(txt.to_string());
    }

    pub fn set_today_marker(&mut self, txt: &str) {
        self.today_marker = txt.to_string();
    }

    pub fn set_includes(&mut self, txt: &str) {
        self.includes = merge_tokens(&self.includes, txt);
    }

    pub fn set_excludes(&mut self, txt: &str) {
        self.excludes = merge_tokens(&self.excludes, txt);
    }

    pub fn set_weekday(&mut self, txt: &str) {
        self.weekday = Some(txt.to_string());
    }

    pub fn set_weekend(&mut self, start_day: &str) {
        self.weekend = start_day.to_string();
    }

    pub fn set_diagram_title(&mut self, txt: &str) {
        self.title = Some(txt.to_string());
    }

    pub fn set_acc_title(&mut self, txt: &str) {
        self.acc_title = Some(txt.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, txt: &str) {
        self.acc_descr = Some(collapse_newline_spaces(txt));
    }

    pub fn add_section(&mut self, txt: &str) {
        self.current_section = txt.to_string();
        self.sections.push(txt.to_string());
    }

    pub fn add_task(&mut self, descr: &str, data_str: &str) -> Result<()> {
        let stripped = data_str.strip_prefix(':').unwrap_or(data_str);
        let mut data: Vec<&str> = stripped.split(',').collect();
        let mut task = GanttTask {
            id: String::new(),
            task: descr.to_string(),
            section: self.current_section.clone(),
            start_time: None,
            end_time: None,
            render_end_time: None,
            manual_end_time: false,
            processed: false,
            active: false,
            done: false,
            crit: false,
            milestone: false,
            vert: false,
            order: -1,
            classes: Vec::new(),
            prev_task_id: self.last_task_id.clone(),
            raw: RawTask {
                data: data_str.to_string(),
                start: None,
                end: String::new(),
            },
        };

        let mut start_idx = 0usize;
        while start_idx < data.len() {
            let Some(tag) = match_tag(data[start_idx]) else {
                break;
            };
            match tag {
                "active" => task.active = true,
                "done" => task.done = true,
                "crit" => task.crit = true,
                "milestone" => task.milestone = true,
                "vert" => task.vert = true,
                _ => {}
            }
            start_idx += 1;
        }
        data.drain(0..start_idx);

        if data.is_empty() {
            return Err(PeleError::new(
                format!("Task \"{}\" needs a duration or an end date.", descr.trim_matches(is_space)),
                ErrorCode::Semantic,
            )
            .of("gantt"));
        }
        if data.len() > 3 {
            return Err(PeleError::new(
                format!(
                    "Task \"{}\" has too many fields. A task takes an id, a start, and an end or duration.",
                    descr.trim_matches(is_space)
                ),
                ErrorCode::Semantic,
            )
            .of("gantt"));
        }

        let fields: Vec<&str> = data.into_iter().map(|s| s.trim_matches(is_space)).collect();
        if fields.len() == 3 {
            task.id = fields[0].to_string();
        } else {
            self.task_count += 1;
            task.id = format!("task{}", self.task_count);
        }
        if fields.len() > 1 {
            task.raw.start = Some(fields[fields.len() - 2].to_string());
        }
        task.raw.end = fields[fields.len() - 1].to_string();

        if !task.vert {
            task.order = self.last_order;
            self.last_order += 1;
        }
        let task_id = task.id.clone();
        let idx = self.tasks.len();
        self.tasks.push(task);
        self.last_task_id = Some(task_id.clone());
        self.by_id.insert(task_id, idx);
        Ok(())
    }

    pub fn set_link(&mut self, ids: &str, link: &str) {
        let href = safe_url(link);
        for id in ids.split(',') {
            if self.by_id.contains_key(id) {
                self.links.insert(id.to_string(), href.clone());
            }
        }
        self.set_class(ids, "clickable");
    }

    pub fn set_click_event(&mut self, ids: &str, _function_name: &str, _function_args: Option<&str>) {
        self.set_class(ids, "clickable");
    }

    pub fn set_class(&mut self, ids: &str, class_name: &str) {
        for id in ids.split(',') {
            if let Some(&idx) = self.by_id.get(id) {
                self.tasks[idx].classes.push(class_name.to_string());
            }
        }
    }

    // Resolves every task's start and end, from scratch on each call (up to 11 passes).
    pub fn get_tasks(&mut self, now: Option<f64>) -> Result<&[GanttTask]> {
        self.now = clock(now);
        self.work = 0;
        self.include_set = self.includes.iter().cloned().collect();
        self.exclude_set = self.excludes.iter().cloned().collect();

        let weekend_start: Option<i64> = match self.weekend.as_str() {
            "friday" => Some(5),
            "saturday" => Some(6),
            _ => None,
        };
        let weekends = self.exclude_set.contains("weekends");
        for (day, name) in WEEKDAYS.iter().enumerate() {
            let iso = if day == 0 { 7 } else { day as i64 };
            let is_wknd = weekends
                && weekend_start.is_some_and(|ws| iso == ws || iso == ws + 1);
            self.excluded_days[day] = is_wknd || self.exclude_set.contains(&name.to_lowercase());
        }

        let fmt_trimmed = self.date_format.trim_matches(is_space);
        self.dated_tokens = !self.includes.is_empty() || !is_numeric_format(fmt_trimmed);
        for token in &self.excludes {
            if token != "weekends"
                && !WEEKDAYS.iter().any(|w| w.eq_ignore_ascii_case(token))
            {
                self.dated_tokens = true;
            }
        }

        for task in &mut self.tasks {
            task.start_time = None;
            task.end_time = None;
            task.render_end_time = None;
            task.processed = false;
            task.manual_end_time = false;
        }

        let mut all_processed = self.compile_tasks()?;
        let mut pass = 0;
        while !all_processed && pass < 10 {
            all_processed = self.compile_tasks()?;
            pass += 1;
        }
        Ok(&self.tasks)
    }

    // Whether a day is excluded (by weekday name, YYYY-MM-DD, or the chart's own `dateFormat`).
    pub fn is_invalid_date(&mut self, date: JsDate) -> Result<bool> {
        let weekday = self.excluded_days[date.day() as usize];
        if !self.dated_tokens {
            return Ok(weekday);
        }
        let template = self.date_format.trim_matches(is_space).to_string();
        let date_only = iso_date(date);
        let mut formatted: Option<String> = Some(date_only.clone());
        self.spend(DATED_STEP)?;
        if template != "YYYY-MM-DD" {
            self.spend(FORMAT_STEP + template.chars().count() * 8)?;
            formatted = format(date, &template);
        }
        let listed = |set: &HashSet<String>| {
            set.contains(&date_only) || formatted.as_ref().is_some_and(|f| set.contains(f))
        };
        if listed(&self.include_set) {
            return Ok(false);
        }
        Ok(weekday || listed(&self.exclude_set))
    }

    fn spend(&mut self, units: usize) -> Result<()> {
        self.work = self.work.saturating_add(units);
        if self.work > WORK_LIMIT {
            return Err(
                PeleError::new("The gantt chart takes too much work to schedule.", ErrorCode::Limit)
                    .of("gantt"),
            );
        }
        Ok(())
    }

    fn parse_date(&mut self, s: &str, template: &str) -> Result<Option<JsDate>> {
        self.spend((template.chars().count() + 1) * (s.chars().count() + 1))?;
        Ok(parse_strict(s, template, JsDate::from_ms(self.now)))
    }

    fn today(&self) -> JsDate {
        let mut d = JsDate::from_ms(self.now);
        d.set_hours(0, 0, 0, 0);
        d
    }

    fn compile_tasks(&mut self) -> Result<bool> {
        let mut all_processed = true;
        for idx in 0..self.tasks.len() {
            self.compile_task(idx)?;
            all_processed = all_processed && self.tasks[idx].processed;
        }
        Ok(all_processed)
    }

    fn compile_task(&mut self, idx: usize) -> Result<()> {
        let raw_start = self.tasks[idx].raw.start.clone();
        match raw_start {
            None => {
                let prev_end = self.tasks[idx]
                    .prev_task_id
                    .as_deref()
                    .and_then(|id| self.by_id.get(id).copied())
                    .map(|p_idx| self.tasks[p_idx].end_time);
                self.tasks[idx].start_time = match prev_end {
                    Some(end) => end,
                    None => Some(self.today()),
                };
            }
            Some(s) => {
                if let Some(st) = self.get_start_date(&s)? {
                    self.tasks[idx].start_time = Some(st);
                }
            }
        }
        let Some(start_time) = self.tasks[idx].start_time else {
            return Ok(());
        };
        let raw_end = self.tasks[idx].raw.end.clone();
        let end_time = self.get_end_date(start_time, &raw_end)?;
        self.tasks[idx].end_time = end_time;
        if end_time.is_none() {
            return Ok(());
        }
        self.tasks[idx].processed = true;
        self.tasks[idx].manual_end_time = self.parse_date(&raw_end, "YYYY-MM-DD")?.is_some();
        self.check_task_dates(idx)?;
        Ok(())
    }

    fn references(&self, list: &str) -> Vec<usize> {
        let mut found = Vec::new();
        for id in list.split(' ') {
            if id.is_empty() {
                continue;
            }
            if let Some(&idx) = self.by_id.get(id) {
                found.push(idx);
            }
        }
        found
    }

    fn get_start_date(&mut self, raw: &str) -> Result<Option<JsDate>> {
        let s = raw.trim_matches(is_space);
        let template = self.date_format.trim_matches(is_space).to_string();

        if (template == "x" || template == "X") && !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
            let ms = s.parse::<f64>().unwrap_or(f64::NAN);
            return self.checked(JsDate::from_ms(ms), s).map(Some);
        }

        if let Some(after_list) = match_ref_keyword(s, "after") {
            let refs = self.references(after_list);
            if refs.is_empty() {
                return Ok(Some(self.today()));
            }
            let mut latest_idx: Option<usize> = None;
            for r_idx in refs {
                match (latest_idx, self.tasks[r_idx].end_time) {
                    (None, _) => latest_idx = Some(r_idx),
                    (Some(cur), Some(cand_t)) => {
                        if self.tasks[cur].end_time.is_some_and(|cur_t| cand_t.time() > cur_t.time()) {
                            latest_idx = Some(r_idx);
                        }
                    }
                    _ => {}
                }
            }
            return Ok(latest_idx.and_then(|i| self.tasks[i].end_time));
        }

        if let Some(parsed) = self.parse_date(s, &template)? {
            return Ok(Some(parsed));
        }

        // Fallback to ECMAScript Date parsing, as in Mermaid.
        let date = parse_js_date(s);
        if !is_valid(date) || date.full_year() < -10_000 || date.full_year() > 10_000 {
            return Err(PeleError::new(format!("Invalid date:{}", s), ErrorCode::Semantic).of("gantt"));
        }
        Ok(Some(date))
    }

    fn checked(&self, date: JsDate, s: &str) -> Result<JsDate> {
        if !is_valid(date) {
            return Err(PeleError::new(format!("Invalid date:{}", s), ErrorCode::Semantic).of("gantt"));
        }
        Ok(date)
    }

    fn get_end_date(&mut self, start_time: JsDate, raw: &str) -> Result<Option<JsDate>> {
        let s = raw.trim_matches(is_space);

        if let Some(until_list) = match_ref_keyword(s, "until") {
            let refs = self.references(until_list);
            if refs.is_empty() {
                return Ok(Some(self.today()));
            }
            let mut earliest_idx: Option<usize> = None;
            for r_idx in refs {
                match (earliest_idx, self.tasks[r_idx].start_time) {
                    (None, _) => earliest_idx = Some(r_idx),
                    (Some(cur), Some(cand_t)) => {
                        if self.tasks[cur]
                            .start_time
                            .is_some_and(|cur_t| cand_t.time() < cur_t.time())
                        {
                            earliest_idx = Some(r_idx);
                        }
                    }
                    _ => {}
                }
            }
            return Ok(earliest_idx.and_then(|i| self.tasks[i].start_time));
        }

        let template = self.date_format.trim_matches(is_space).to_string();
        if let Some(parsed) = self.parse_date(s, &template)? {
            return Ok(Some(if self.inclusive_end_dates {
                add_days(parsed, 1)
            } else {
                parsed
            }));
        }

        let (value, unit) = parse_duration(s);
        if value.is_nan() {
            return Ok(Some(start_time));
        }
        let end = add(start_time, value, unit);
        Ok(Some(if is_valid(end) { end } else { start_time }))
    }

    // Pushes the end of a task out by one day for every excluded day it covers.
    fn check_task_dates(&mut self, idx: usize) -> Result<()> {
        if self.excludes.is_empty() || self.tasks[idx].manual_end_time {
            return Ok(());
        }
        let start_time = self.tasks[idx].start_time.unwrap();
        let mut cursor = add_days(start_time, 1);
        let mut end_time = self.tasks[idx].end_time.unwrap();
        let max_end_time = add_days(end_time, 10_000).time();
        let mut invalid = false;
        let mut render_end_time: Option<f64> = None;

        while cursor.time() <= end_time.time() {
            self.spend(DAY_STEP)?;
            if !invalid {
                render_end_time = Some(end_time.time());
            }
            invalid = self.is_invalid_date(cursor)?;
            if invalid {
                end_time.set_date(end_time.date() + 1);
                if end_time.time() > max_end_time {
                    return Err(PeleError::new(
                        "Failed to find a valid date that was not excluded by `excludes` after 10,000 iterations.",
                        ErrorCode::Semantic,
                    )
                    .of("gantt"));
                }
            }
            cursor.set_date(cursor.date() + 1);
        }

        self.tasks[idx].end_time = Some(end_time);
        self.tasks[idx].render_end_time = render_end_time.map(JsDate::from_ms);
        Ok(())
    }
}
