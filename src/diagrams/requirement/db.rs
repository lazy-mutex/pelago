use super::types::{Relation, Requirement, RequirementClass, RequirementElement};
use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use std::collections::HashMap;

pub mod requirement_type {
    pub const REQUIREMENT: &str = "Requirement";
    pub const FUNCTIONAL_REQUIREMENT: &str = "Functional Requirement";
    pub const INTERFACE_REQUIREMENT: &str = "Interface Requirement";
    pub const PERFORMANCE_REQUIREMENT: &str = "Performance Requirement";
    pub const PHYSICAL_REQUIREMENT: &str = "Physical Requirement";
    pub const DESIGN_CONSTRAINT: &str = "Design Constraint";
}

pub mod risk_level {
    pub const LOW_RISK: &str = "Low";
    pub const MED_RISK: &str = "Medium";
    pub const HIGH_RISK: &str = "High";
}

pub mod verify_type {
    pub const VERIFY_ANALYSIS: &str = "Analysis";
    pub const VERIFY_DEMONSTRATION: &str = "Demonstration";
    pub const VERIFY_INSPECTION: &str = "Inspection";
    pub const VERIFY_TEST: &str = "Test";
}

pub mod relationships {
    pub const CONTAINS: &str = "contains";
    pub const COPIES: &str = "copies";
    pub const DERIVES: &str = "derives";
    pub const SATISFIES: &str = "satisfies";
    pub const VERIFIES: &str = "verifies";
    pub const REFINES: &str = "refines";
    pub const TRACES: &str = "traces";
}

// Applying a class copies its declarations onto each node, so a short text can ask for
// hundreds of millions of copies. Past this many the diagram is refused.
const MAX_STYLE_COPIES: usize = 1_000_000;

fn collapse_newline_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' && i + 1 < chars.len() && is_space(chars[i + 1]) {
            out.push('\n');
            i += 2;
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

pub struct RequirementDb {
    pub direction: String,
    pub requirements: Vec<Requirement>,
    req_index: HashMap<String, usize>,
    pub elements: Vec<RequirementElement>,
    elem_index: HashMap<String, usize>,
    pub relations: Vec<Relation>,
    pub classes: HashMap<String, RequirementClass>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,

    // The grammar reports a body's fields before the requirement or element they belong to.
    new_id: String,
    new_text: String,
    new_risk: String,
    new_verify_method: String,
    new_element_type: String,
    new_doc_ref: String,
    style_copies: usize,
}

impl Default for RequirementDb {
    fn default() -> Self {
        Self {
            direction: "TB".to_string(),
            requirements: Vec::new(),
            req_index: HashMap::new(),
            elements: Vec::new(),
            elem_index: HashMap::new(),
            relations: Vec::new(),
            classes: HashMap::new(),
            title: None,
            acc_title: None,
            acc_descr: None,
            new_id: String::new(),
            new_text: String::new(),
            new_risk: String::new(),
            new_verify_method: String::new(),
            new_element_type: String::new(),
            new_doc_ref: String::new(),
            style_copies: 0,
        }
    }
}

impl RequirementDb {
    pub fn has_node(&self, id: &str) -> bool {
        self.req_index.contains_key(id) || self.elem_index.contains_key(id)
    }

    fn check_copy<S: AsRef<str>>(style_copies: &mut usize, target: &mut Vec<String>, styles: &[S]) -> Result<()> {
        *style_copies += styles.len();
        if *style_copies > MAX_STYLE_COPIES {
            return Err(PeleError::new("Requirement diagram applies too many style declarations.", ErrorCode::Limit).of("requirement"));
        }
        for s in styles {
            target.push(s.as_ref().to_string());
        }
        Ok(())
    }

    pub fn set_direction(&mut self, dir: &str) {
        self.direction = dir.to_string();
    }

    pub fn add_requirement(&mut self, name: String, req_type: String) {
        if !self.req_index.contains_key(&name) {
            let idx = self.requirements.len();
            self.req_index.insert(name.clone(), idx);
            self.requirements.push(Requirement {
                name,
                req_type,
                requirement_id: std::mem::take(&mut self.new_id),
                text: std::mem::take(&mut self.new_text),
                risk: std::mem::take(&mut self.new_risk),
                verify_method: std::mem::take(&mut self.new_verify_method),
                css_styles: Vec::new(),
                classes: vec!["default".to_string()],
            });
        } else {
            self.new_id.clear();
            self.new_text.clear();
            self.new_risk.clear();
            self.new_verify_method.clear();
        }
    }

    pub fn set_new_req_id(&mut self, id: String) {
        self.new_id = id;
    }

    pub fn set_new_req_text(&mut self, text: String) {
        self.new_text = text;
    }

    pub fn set_new_req_risk(&mut self, risk: String) {
        self.new_risk = risk;
    }

    pub fn set_new_req_verify_method(&mut self, verify_method: String) {
        self.new_verify_method = verify_method;
    }

    pub fn add_element(&mut self, name: String) {
        if !self.elem_index.contains_key(&name) {
            let idx = self.elements.len();
            self.elem_index.insert(name.clone(), idx);
            self.elements.push(RequirementElement {
                name,
                elem_type: std::mem::take(&mut self.new_element_type),
                doc_ref: std::mem::take(&mut self.new_doc_ref),
                css_styles: Vec::new(),
                classes: vec!["default".to_string()],
            });
        } else {
            self.new_element_type.clear();
            self.new_doc_ref.clear();
        }
    }

    pub fn set_new_element_type(&mut self, elem_type: String) {
        self.new_element_type = elem_type;
    }

    pub fn set_new_element_doc_ref(&mut self, doc_ref: String) {
        self.new_doc_ref = doc_ref;
    }

    pub fn add_relationship(&mut self, rel_type: String, src: String, dst: String) {
        self.relations.push(Relation { rel_type, src, dst });
    }

    fn node_styles_and_classes(&mut self, id: &str) -> Option<(&mut Vec<String>, &mut Vec<String>, &mut usize)> {
        if let Some(&idx) = self.req_index.get(id) {
            let req = &mut self.requirements[idx];
            Some((&mut req.css_styles, &mut req.classes, &mut self.style_copies))
        } else if let Some(&idx) = self.elem_index.get(id) {
            let elem = &mut self.elements[idx];
            Some((&mut elem.css_styles, &mut elem.classes, &mut self.style_copies))
        } else {
            None
        }
    }

    pub fn set_css_style(&mut self, ids: &[String], styles: &[String]) -> Result<()> {
        for id in ids {
            // Mermaid stops at the first unknown id, leaving the rest of the list unstyled.
            let Some((css_styles, _, style_copies)) = self.node_styles_and_classes(id) else {
                return Ok(());
            };
            for s in styles {
                if s.contains(',') {
                    let parts: Vec<&str> = s.split(',').collect();
                    Self::check_copy(style_copies, css_styles, &parts)?;
                } else {
                    Self::check_copy(style_copies, css_styles, &[s.as_str()])?;
                }
            }
        }
        Ok(())
    }

    pub fn set_class(&mut self, ids: &[String], class_names: &[String]) -> Result<()> {
        for id in ids {
            if !self.has_node(id) {
                continue;
            }
            for name in class_names {
                let class_styles = self.classes.get(name).map(|c| c.styles.clone());
                let (css_styles, classes, style_copies) = self.node_styles_and_classes(id).unwrap();
                classes.push(name.clone());
                if let Some(styles) = class_styles {
                    Self::check_copy(style_copies, css_styles, &styles)?;
                }
            }
        }
        Ok(())
    }

    pub fn define_class(&mut self, ids: &[String], style: &[String]) -> Result<()> {
        let split: Vec<&str> = style.iter().flat_map(|s| s.split(',')).collect();
        for id in ids {
            let style_class = self.classes.entry(id.clone()).or_insert_with(|| RequirementClass {
                id: id.clone(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });
            for s in style {
                if s.contains("color") {
                    style_class.text_styles.push(s.replacen("fill", "bgFill", 1));
                }
                style_class.styles.push(s.clone());
            }
            for req in &mut self.requirements {
                if req.classes.contains(id) {
                    Self::check_copy(&mut self.style_copies, &mut req.css_styles, &split)?;
                }
            }
            for elem in &mut self.elements {
                if elem.classes.contains(id) {
                    Self::check_copy(&mut self.style_copies, &mut elem.css_styles, &split)?;
                }
            }
        }
        Ok(())
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_spaces(text));
    }
}
