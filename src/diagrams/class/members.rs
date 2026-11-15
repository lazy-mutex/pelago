use crate::chars::{is_line_end, is_space};
use crate::diagrams::common::generics::parse_generic_types;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberType {
    Method,
    Attribute,
}

fn is_visibility(ch: char) -> bool {
    matches!(ch, '#' | '+' | '~' | '-')
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

#[derive(Debug, Clone)]
pub struct DisplayDetails {
    pub display_text: String,
    pub css_style: &'static str,
}

#[derive(Debug, Clone)]
pub struct ClassMember {
    pub id: String,
    pub member_type: MemberType,
    pub visibility: String,
    pub classifier: String,
    pub parameters: String,
    pub return_type: String,
}

impl ClassMember {
    pub fn new(input: &str, member_type: MemberType) -> Self {
        let mut m = ClassMember {
            id: String::new(),
            member_type,
            visibility: String::new(),
            classifier: String::new(),
            parameters: String::new(),
            return_type: String::new(),
        };
        let chars: Vec<char> = input.chars().collect();
        // Mermaid throws a TypeError for a method with no opening parenthesis, such as `run)`.
        // It is kept here as an attribute, so it is shown as written.
        if member_type != MemberType::Method || !m.parse_method(&chars) {
            m.parse_attribute(&chars);
        }
        let trimmed = js_trim(&m.id);
        m.id = if m.id.starts_with(' ') {
            format!(" {}", trimmed)
        } else {
            trimmed.to_string()
        };
        m
    }

    // The same split as Mermaid's /([#+~-])?(.+)\((.*)\)([\s$*])?(.*)([$*])?/, found in one pass:
    // the first line with an opening parenthesis after its first character and a closing one later.
    fn parse_method(&mut self, chars: &[char]) -> bool {
        let n = chars.len();
        let mut start = 0usize;
        while start < n {
            let mut end = start;
            while end < n && !is_line_end(chars[end]) {
                end += 1;
            }
            let mut close = end as isize - 1;
            while close > start as isize && chars[close as usize] != ')' {
                close -= 1;
            }
            let mut open = close - 1;
            while open > start as isize && chars[open as usize] != '(' {
                open -= 1;
            }
            if open > start as isize {
                let open = open as usize;
                let close = close as usize;
                let first = chars[start];
                let visible = is_visibility(first) && open > start + 1;
                if visible {
                    self.visibility.push(first);
                }
                let id_start = if visible { start + 1 } else { start };
                self.id = chars[id_start..open].iter().collect();
                let params_raw: String = chars[open + 1..close].iter().collect();
                self.parameters = js_trim(&params_raw).to_string();
                let mut rest = close + 1;
                let mut classifier = String::new();
                if rest < n {
                    let next = chars[rest];
                    if next == '$' || next == '*' {
                        classifier.push(next);
                        rest += 1;
                    } else if is_space(next) {
                        rest += 1;
                    }
                }
                let mut stop = rest;
                while stop < n && !is_line_end(chars[stop]) {
                    stop += 1;
                }
                let ret_raw: String = chars[rest..stop].iter().collect();
                self.return_type = js_trim(&ret_raw).to_string();
                if classifier.is_empty() {
                    if let Some(last) = self.return_type.chars().last() {
                        if last == '$' || last == '*' {
                            classifier.push(last);
                            self.return_type.pop();
                        }
                    }
                }
                self.classifier = classifier;
                return true;
            }
            start = end + 1;
        }
        self.member_type = MemberType::Attribute;
        false
    }

    fn parse_attribute(&mut self, chars: &[char]) {
        if let Some(&first) = chars.first() {
            if is_visibility(first) {
                self.visibility.push(first);
            }
        }
        if let Some(&last) = chars.last() {
            if last == '$' || last == '*' {
                self.classifier.push(last);
            }
        }
        let from = usize::from(!self.visibility.is_empty());
        let to = chars.len().saturating_sub(usize::from(!self.classifier.is_empty()));
        self.id = if from < to {
            chars[from..to].iter().collect()
        } else {
            String::new()
        };
    }

    pub fn get_display_details(&self) -> DisplayDetails {
        let mut display_text = format!("{}{}", self.visibility, parse_generic_types(&self.id));
        if self.member_type == MemberType::Method {
            display_text.push_str(&format!(
                "({})",
                parse_generic_types(js_trim(&self.parameters))
            ));
            if !self.return_type.is_empty() {
                display_text.push_str(" : ");
                display_text.push_str(&parse_generic_types(&self.return_type));
            }
        }
        DisplayDetails {
            display_text: js_trim(&display_text).to_string(),
            css_style: self.parse_classifier(),
        }
    }

    pub fn parse_classifier(&self) -> &'static str {
        match self.classifier.as_str() {
            "*" => "font-style:italic;",
            "$" => "text-decoration:underline;",
            _ => "",
        }
    }
}
