use super::db::{relationships, requirement_type, risk_level, verify_type, RequirementDb};
use super::lexer::{tokenize, Tokens};
use super::tokens::{t, TOKEN_NAMES};
use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

// What the keyword tokens stand for, from REQUIREMENT to TRACES in token order.
const VALUES: &[&str; 21] = &[
    requirement_type::REQUIREMENT,
    requirement_type::FUNCTIONAL_REQUIREMENT,
    requirement_type::INTERFACE_REQUIREMENT,
    requirement_type::PERFORMANCE_REQUIREMENT,
    requirement_type::PHYSICAL_REQUIREMENT,
    requirement_type::DESIGN_CONSTRAINT,
    risk_level::LOW_RISK,
    risk_level::MED_RISK,
    risk_level::HIGH_RISK,
    verify_type::VERIFY_ANALYSIS,
    verify_type::VERIFY_DEMONSTRATION,
    verify_type::VERIFY_INSPECTION,
    verify_type::VERIFY_TEST,
    "",
    relationships::CONTAINS,
    relationships::COPIES,
    relationships::DERIVES,
    relationships::SATISFIES,
    relationships::VERIFIES,
    relationships::REFINES,
    relationships::TRACES,
];

fn is_style_part(tok: i32) -> bool {
    matches!(tok, t::ALPHA | t::COLON | t::BRKT | t::MINUS | t::SEMICOLON)
}

// Fails with Jison's wording, naming the token types `from` to `to` and any others as expected.
fn fail(src: &[char], tokens: &Tokens, i: usize, from: i32, to: i32, others: &[i32]) -> PeleError {
    let tok = tokens.types[i];
    if tok == t::ERROR {
        return syntax_error("requirement", src, tokens.starts[i], "", true);
    }
    let expected: Vec<String> = (from..=to)
        .chain(others.iter().copied())
        .map(|ty| format!("'{}'", TOKEN_NAMES[ty as usize]))
        .collect();
    syntax_error(
        "requirement",
        src,
        tokens.starts[i],
        &format!("Expecting {}, got '{}'", expected.join(", "), TOKEN_NAMES[tok as usize]),
        false,
    )
}

struct Parser<'a> {
    src: &'a [char],
    tokens: Tokens,
    i: usize,
    fields: Vec<i32>,
    values: Vec<String>,
}

impl<'a> Parser<'a> {
    fn fail(&self, from: i32, to: i32, others: &[i32]) -> PeleError {
        fail(self.src, &self.tokens, self.i, from, to, others)
    }

    fn expect(&mut self, ty: i32) -> Result<String> {
        if self.tokens.types[self.i] != ty {
            return Err(self.fail(ty, ty, &[]));
        }
        let s = self.tokens.texts[self.i].clone();
        self.i += 1;
        Ok(s)
    }

    // The value of the next token if it is a keyword in the given range of token types.
    fn keyword(&mut self, from: i32, to: i32) -> Result<&'static str> {
        let tok = self.tokens.types[self.i];
        if tok < from || tok > to {
            return Err(self.fail(from, to, &[]));
        }
        self.i += 1;
        Ok(VALUES[(tok - t::REQUIREMENT) as usize])
    }

    fn string(&mut self) -> Result<String> {
        let tok = self.tokens.types[self.i];
        if tok != t::UNQ_STRING && tok != t::Q_STRING {
            return Err(self.fail(t::Q_STRING, t::UNQ_STRING, &[]));
        }
        let s = self.tokens.texts[self.i].clone();
        self.i += 1;
        Ok(s)
    }

    fn directive(&mut self, db: &mut RequirementDb) -> Result<bool> {
        let tok = self.tokens.types[self.i];
        if tok == t::ACC_TITLE {
            self.i += 1;
            let v = self.expect(t::ACC_TITLE_VALUE)?;
            db.set_acc_title(v.trim_matches(is_space));
        } else if tok == t::ACC_DESCR {
            self.i += 1;
            let v = self.expect(t::ACC_DESCR_VALUE)?;
            db.set_acc_description(v.trim_matches(is_space));
        } else if tok == t::ACC_DESCR_MULTILINE_VALUE {
            let v = self.tokens.texts[self.i].clone();
            self.i += 1;
            db.set_acc_description(v.trim_matches(is_space));
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    fn id_list(&mut self) -> Result<Vec<String>> {
        let mut list = Vec::new();
        loop {
            let tok = self.tokens.types[self.i];
            if tok != t::ALPHA && tok != t::UNQ_STRING && tok != t::Q_STRING {
                return Err(self.fail(t::Q_STRING, t::UNQ_STRING, &[t::ALPHA]));
            }
            list.push(self.tokens.texts[self.i].clone());
            self.i += 1;
            if self.tokens.types[self.i] != t::COMMA {
                return Ok(list);
            }
            self.i += 1;
        }
    }

    fn styles_opt(&mut self) -> Result<Vec<String>> {
        let mut styles = Vec::new();
        loop {
            if !is_style_part(self.tokens.types[self.i]) {
                return Err(self.fail(t::ALPHA, t::SEMICOLON, &[t::MINUS, t::BRKT]));
            }
            let mut style = self.tokens.texts[self.i].clone();
            self.i += 1;
            while is_style_part(self.tokens.types[self.i]) {
                style.push_str(&self.tokens.texts[self.i]);
                self.i += 1;
            }
            styles.push(style);
            if self.tokens.types[self.i] != t::COMMA {
                return Ok(styles);
            }
            self.i += 1;
        }
    }

    fn class_list(&mut self) -> Result<Option<Vec<String>>> {
        let classes = if self.tokens.types[self.i] == t::STYLE_SEPARATOR {
            self.i += 1;
            let list = self.id_list()?;
            self.expect(t::STRUCT_START)?;
            Some(list)
        } else if self.tokens.types[self.i] == t::STRUCT_START {
            self.i += 1;
            None
        } else {
            return Err(self.fail(t::STRUCT_START, t::STRUCT_START, &[t::STYLE_SEPARATOR]));
        };
        self.expect(t::NEWLINE)?;
        Ok(classes)
    }

    // Reads a body up to its closing brace. The grammar is right recursive, so Mermaid applies
    // the fields last to first and the first of two repeated fields wins.
    fn body(&mut self, requirement: bool, db: &mut RequirementDb) -> Result<()> {
        self.fields.clear();
        self.values.clear();
        loop {
            let tok = self.tokens.types[self.i];
            if tok == t::STRUCT_STOP {
                self.i += 1;
                break;
            }
            if tok == t::NEWLINE {
                self.i += 1;
                continue;
            }
            let bad = if requirement {
                tok < t::ID || tok > t::VERIFYMTHD
            } else {
                tok != t::TYPE && tok != t::DOCREF
            };
            if bad {
                if requirement {
                    return Err(self.fail(t::ID, t::VERIFYMTHD, &[t::NEWLINE, t::STRUCT_STOP]));
                }
                return Err(self.fail(t::TYPE, t::DOCREF, &[t::NEWLINE, t::STRUCT_STOP]));
            }
            self.i += 1;
            self.expect(t::COLONSEP)?;
            let val = if tok == t::RISK {
                self.keyword(t::LOW_RISK, t::HIGH_RISK)?.to_string()
            } else if tok == t::VERIFYMTHD {
                self.keyword(t::VERIFY_ANALYSIS, t::VERIFY_TEST)?.to_string()
            } else {
                self.string()?
            };
            self.values.push(val);
            self.fields.push(tok);
            self.expect(t::NEWLINE)?;
        }
        for k in (0..self.fields.len()).rev() {
            let value = self.values[k].clone();
            match self.fields[k] {
                t::ID => db.set_new_req_id(value),
                t::TEXT => db.set_new_req_text(value),
                t::RISK => db.set_new_req_risk(value),
                t::VERIFYMTHD => db.set_new_req_verify_method(value),
                t::TYPE => db.set_new_element_type(value),
                _ => db.set_new_element_doc_ref(value),
            }
        }
        Ok(())
    }
}

pub fn parse_requirement(source: &str, db: &mut RequirementDb) -> Result<()> {
    let src: Vec<char> = source.chars().collect();
    let tokens = tokenize(&src);
    let mut p = Parser { src: &src, tokens, i: 0, fields: Vec::new(), values: Vec::new() };

    while p.directive(db)? {
        if p.tokens.types[p.i] == t::NEWLINE {
            p.i += 1;
        }
    }
    if p.tokens.types[p.i] != t::RD {
        return Err(p.fail(t::RD, t::RD, &[t::ACC_TITLE, t::ACC_DESCR, t::ACC_DESCR_MULTILINE_VALUE]));
    }
    p.i += 1;
    p.expect(t::NEWLINE)?;

    loop {
        let tok = p.tokens.types[p.i];
        if tok == t::EOF {
            return Ok(());
        }
        if tok == t::NEWLINE {
            p.i += 1;
        } else if p.directive(db)? {
            continue;
        } else if (t::DIRECTION_TB..=t::DIRECTION_LR).contains(&tok) {
            p.i += 1;
            let dir = match tok {
                t::DIRECTION_TB => "TB",
                t::DIRECTION_BT => "BT",
                t::DIRECTION_RL => "RL",
                _ => "LR",
            };
            db.set_direction(dir);
        } else if (t::REQUIREMENT..=t::DESIGN_CONSTRAINT).contains(&tok) {
            let req_type = p.keyword(t::REQUIREMENT, t::DESIGN_CONSTRAINT)?.to_string();
            let name = p.string()?;
            let classes = p.class_list()?;
            p.body(true, db)?;
            db.add_requirement(name.clone(), req_type);
            if let Some(classes) = classes {
                db.set_class(&[name], &classes)?;
            }
        } else if tok == t::ELEMENT {
            p.i += 1;
            let name = p.string()?;
            let classes = p.class_list()?;
            p.body(false, db)?;
            db.add_element(name.clone());
            if let Some(classes) = classes {
                db.set_class(&[name], &classes)?;
            }
        } else if tok == t::STYLE {
            p.i += 1;
            let ids = p.id_list()?;
            let styles = p.styles_opt()?;
            db.set_css_style(&ids, &styles)?;
        } else if tok == t::CLASSDEF {
            p.i += 1;
            let ids = p.id_list()?;
            let styles = p.styles_opt()?;
            db.define_class(&ids, &styles)?;
        } else if tok == t::CLASS {
            p.i += 1;
            let ids = p.id_list()?;
            let classes = p.id_list()?;
            db.set_class(&ids, &classes)?;
        } else if tok == t::UNQ_STRING || tok == t::Q_STRING {
            let id = p.tokens.texts[p.i].clone();
            p.i += 1;
            let next = p.tokens.types[p.i];
            if next == t::STYLE_SEPARATOR {
                p.i += 1;
                let classes = p.id_list()?;
                db.set_class(&[id], &classes)?;
            } else if next == t::END_ARROW_L || next == t::LINE {
                p.i += 1;
                let relationship = p.keyword(t::CONTAINS, t::TRACES)?.to_string();
                p.expect(if next == t::LINE { t::END_ARROW_R } else { t::LINE })?;
                let other = p.string()?;
                if next == t::LINE {
                    db.add_relationship(relationship, id, other);
                } else {
                    db.add_relationship(relationship, other, id);
                }
            } else {
                return Err(p.fail(t::STYLE_SEPARATOR, t::STYLE_SEPARATOR, &[t::END_ARROW_L, t::LINE]));
            }
        } else {
            return Err(p.fail(
                t::NEWLINE,
                t::EOF,
                &[
                    t::ACC_TITLE,
                    t::ACC_DESCR,
                    t::ACC_DESCR_MULTILINE_VALUE,
                    t::DIRECTION_TB,
                    t::DIRECTION_BT,
                    t::DIRECTION_RL,
                    t::DIRECTION_LR,
                    t::REQUIREMENT,
                    t::FUNCTIONAL_REQUIREMENT,
                    t::INTERFACE_REQUIREMENT,
                    t::PERFORMANCE_REQUIREMENT,
                    t::PHYSICAL_REQUIREMENT,
                    t::DESIGN_CONSTRAINT,
                    t::ELEMENT,
                    t::CLASSDEF,
                    t::CLASS,
                    t::STYLE,
                    t::UNQ_STRING,
                    t::Q_STRING,
                ],
            ));
        }
    }
}
