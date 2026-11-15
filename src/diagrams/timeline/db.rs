use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Lr,
    Td,
}

#[derive(Debug, Clone)]
pub struct TimelinePeriod {
    pub text: String,
    pub events: Vec<String>,
    // Name of the section in force when the period was declared, '' before the first one.
    pub section: String,
    // Index into `sections` of that section, -1 before the first one.
    pub section_index: isize,
}

#[derive(Debug, Clone, Default)]
pub struct TimelineDb {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub direction: Direction,
    pub sections: Vec<String>,
    pub periods: Vec<TimelinePeriod>,
    current_section: String,
}

fn collapse_newline_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        out.push(ch);
        i += 1;
        if ch == '\n' {
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
        }
    }
    out
}

impl TimelineDb {
    pub fn set_diagram_title(&mut self, text: String) {
        self.title = Some(text);
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_spaces(text));
    }

    pub fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }

    pub fn add_section(&mut self, text: String) {
        self.current_section = text.clone();
        self.sections.push(text);
    }

    pub fn add_task(&mut self, period: String, _length: usize, event: &str) {
        let events = if event.is_empty() {
            Vec::new()
        } else {
            vec![event.to_string()]
        };
        let section_index = self.sections.len() as isize - 1;
        self.periods.push(TimelinePeriod {
            text: period,
            events,
            section: self.current_section.clone(),
            section_index,
        });
    }

    pub fn add_event(&mut self, event: String) -> Result<()> {
        let Some(period) = self.periods.last_mut() else {
            // Mermaid fails here too, with a TypeError.
            return Err(PeleError::new(
                "An event needs a time period before it.",
                ErrorCode::Semantic,
            )
            .of("timeline"));
        };
        period.events.push(event);
        Ok(())
    }
}
