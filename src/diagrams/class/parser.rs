use super::db::{line_type, relation_type, ClassDb};
use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};
use super::types::{ClassRelation, RelationEnd, RelationSpec};
use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

const NAME_TOKENS: &str = "'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS', 'BQUOTE_STR'";

struct Frame {
    id: String,
    classes: Vec<String>,
    notes: Vec<String>,
}

fn is_alphanum(tok: u16) -> bool {
    matches!(tok, t::UNICODE_TEXT | t::ALPHA | t::MINUS)
}

fn is_style_part(tok: u16) -> bool {
    matches!(tok, t::ALPHA | t::COLON | t::BRKT | t::STYLE | t::PCT | t::LABEL)
}

fn end_marker_code(tok: u16) -> Option<u8> {
    match tok {
        t::AGGREGATION => Some(relation_type::AGGREGATION),
        t::EXTENSION => Some(relation_type::EXTENSION),
        t::COMPOSITION => Some(relation_type::COMPOSITION),
        t::DEPENDENCY => Some(relation_type::DEPENDENCY),
        t::LOLLIPOP => Some(relation_type::LOLLIPOP),
        _ => None,
    }
}

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

struct Parser<'a> {
    src: Vec<char>,
    types: Vec<u16>,
    texts: Vec<String>,
    starts: Vec<usize>,
    i: usize,
    db: &'a mut ClassDb,
}

impl<'a> Parser<'a> {
    fn fail(&self, expected: &str) -> PeleError {
        let tok = self.types[self.i];
        if tok == t::ERROR {
            return syntax_error("class", &self.src, self.starts[self.i], "", true);
        }
        syntax_error(
            "class",
            &self.src,
            self.starts[self.i],
            &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok as usize]),
            false,
        )
    }

    fn expect(&mut self, expected_type: u16) -> Result<String> {
        if self.types[self.i] != expected_type {
            return Err(self.fail(&format!("'{}'", TOKEN_NAMES[expected_type as usize])));
        }
        let val = self.texts[self.i].clone();
        self.i += 1;
        Ok(val)
    }

    fn alpha_num(&mut self) -> Result<String> {
        if !is_alphanum(self.types[self.i]) {
            return Err(self.fail("'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS'"));
        }
        let val = self.texts[self.i].clone();
        self.i += 1;
        Ok(val)
    }

    fn name(&mut self, generic: bool) -> Result<String> {
        let mut out = String::new();
        loop {
            if self.types[self.i] == t::BQUOTE_STR {
                out.push_str(&self.texts[self.i]);
                self.i += 1;
                break;
            }
            if !is_alphanum(self.types[self.i]) {
                return Err(self.fail(NAME_TOKENS));
            }
            out.push_str(&self.texts[self.i]);
            self.i += 1;
            let tok = self.types[self.i];
            if tok == t::DOT {
                out.push('.');
                self.i += 1;
            } else if !is_alphanum(tok) && tok != t::BQUOTE_STR {
                break;
            }
        }
        if generic && self.types[self.i] == t::GENERICTYPE {
            out.push('~');
            out.push_str(&self.texts[self.i]);
            out.push('~');
            self.i += 1;
        }
        Ok(out)
    }

    fn class_label(&mut self) -> Result<String> {
        self.i += 1;
        let label = self.expect(t::STR)?;
        self.expect(t::SQE)?;
        Ok(label)
    }

    fn members(&mut self) -> Result<Vec<String>> {
        if self.types[self.i] != t::MEMBER {
            return Err(self.fail("'STRUCT_STOP', 'MEMBER'"));
        }
        let mut list = Vec::new();
        while self.types[self.i] == t::MEMBER {
            list.push(self.texts[self.i].clone());
            self.i += 1;
        }
        self.expect(t::STRUCT_STOP)?;
        // The grammar collects members last to first, and add_members turns them back.
        list.reverse();
        Ok(list)
    }

    fn class_statement(&mut self) -> Result<String> {
        self.i += 1;
        let id = self.name(true)?;
        self.db.add_class(&id);
        if self.types[self.i] == t::SQS {
            let lbl = self.class_label()?;
            self.db.set_class_label(&id, lbl);
        }
        match self.types[self.i] {
            t::STYLE_SEPARATOR => {
                self.i += 1;
                let css = self.alpha_num()?;
                self.db.set_css_class(&id, &css);
                if self.types[self.i] == t::STRUCT_START {
                    self.i += 1;
                    let m = self.members()?;
                    self.db.add_members(&id, m);
                }
            }
            t::STRUCT_START => {
                self.i += 1;
                if self.types[self.i] == t::STRUCT_STOP {
                    self.i += 1;
                } else {
                    let m = self.members()?;
                    self.db.add_members(&id, m);
                }
            }
            t::ANNOTATION_START => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(t::ANNOTATION_END)?;
                self.db.add_annotation(&id, annotation);
                if self.types[self.i] == t::STRUCT_START {
                    self.i += 1;
                    if self.types[self.i] == t::STRUCT_STOP {
                        self.i += 1;
                    } else {
                        let m = self.members()?;
                        self.db.add_members(&id, m);
                    }
                }
            }
            _ => {}
        }
        Ok(id)
    }

    fn note_statement(&mut self) -> Result<String> {
        let tok = self.types[self.i];
        self.i += 1;
        if tok == t::NOTE {
            let text = self.expect(t::STR)?;
            return Ok(self.db.add_note(text, None));
        }
        let id = self.name(true)?;
        let text = self.expect(t::STR)?;
        Ok(self.db.add_note(text, Some(id)))
    }

    // Namespaces nest, so they are parsed with a stack of open ones.
    fn namespace_statement(&mut self) -> Result<()> {
        let mut open: Vec<Frame> = Vec::new();
        loop {
            self.i += 1;
            let ns = self.name(false)?;
            let id = if self.types[self.i] == t::SQS {
                let lbl = self.class_label()?;
                self.db.add_namespace(&ns, Some(&lbl))
            } else {
                self.db.add_namespace(&ns, None)
            };
            self.expect(t::STRUCT_START)?;
            if self.types[self.i] == t::NEWLINE {
                self.i += 1;
            }
            open.push(Frame { id, classes: Vec::new(), notes: Vec::new() });
            let mut item = true;
            while item {
                let tok = self.types[self.i];
                if tok == t::NAMESPACE {
                    break;
                }
                if tok == t::CLASS {
                    let cls_id = self.class_statement()?;
                    open.last_mut().unwrap().classes.push(cls_id);
                } else if tok == t::NOTE || tok == t::NOTE_FOR {
                    let note_id = self.note_statement()?;
                    open.last_mut().unwrap().notes.push(note_id);
                } else {
                    return Err(self.fail("'NAMESPACE', 'CLASS', 'NOTE_FOR', 'NOTE'"));
                }
                loop {
                    if self.types[self.i] == t::NEWLINE {
                        let next = self.types[self.i + 1];
                        if matches!(next, t::NAMESPACE | t::CLASS | t::NOTE | t::NOTE_FOR) {
                            self.i += 1;
                            break;
                        }
                        self.i += 1;
                    }
                    self.expect(t::STRUCT_STOP)?;
                    let done = open.pop().unwrap();
                    self.db.add_classes_to_namespace(&done.id, &done.classes, &done.notes);
                    self.db.pop_namespace();
                    if open.is_empty() {
                        item = false;
                        break;
                    }
                }
            }
            if open.is_empty() {
                return Ok(());
            }
        }
    }

    fn relation_end(&mut self) -> RelationEnd {
        if let Some(code) = end_marker_code(self.types[self.i]) {
            self.i += 1;
            RelationEnd::Marker(code)
        } else {
            RelationEnd::None
        }
    }

    fn class_or_relation(&mut self) -> Result<()> {
        let id1 = self.name(true)?;
        let mut tok = self.types[self.i];
        if tok == t::LABEL {
            let cleaned = self.db.cleanup_label(&self.texts[self.i]);
            self.i += 1;
            self.db.add_member(&id1, &cleaned);
            return Ok(());
        }
        let mut title1 = "none".to_string();
        if tok == t::STR {
            title1 = self.texts[self.i].clone();
            self.i += 1;
        } else if end_marker_code(tok).is_none() && tok != t::LINE && tok != t::DOTTED_LINE {
            return Ok(());
        }
        let type1 = self.relation_end();
        tok = self.types[self.i];
        if tok != t::LINE && tok != t::DOTTED_LINE {
            return Err(self.fail(
                "'AGGREGATION', 'EXTENSION', 'COMPOSITION', 'DEPENDENCY', 'LOLLIPOP', 'LINE', 'DOTTED_LINE'",
            ));
        }
        self.i += 1;
        let type2 = self.relation_end();
        let title2 = if self.types[self.i] == t::STR {
            let s = self.texts[self.i].clone();
            self.i += 1;
            s
        } else {
            "none".to_string()
        };
        let id2 = self.name(true)?;
        let title = if self.types[self.i] == t::LABEL {
            let cleaned = self.db.cleanup_label(&self.texts[self.i]);
            self.i += 1;
            Some(cleaned)
        } else {
            None
        };
        self.db.add_relation(ClassRelation {
            id1,
            id2,
            relation: RelationSpec {
                type1,
                type2,
                line_type: if tok == t::LINE { line_type::LINE } else { line_type::DOTTED_LINE },
            },
            relation_title1: title1,
            relation_title2: title2,
            title,
        });
        Ok(())
    }

    fn styles(&mut self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        loop {
            if !is_style_part(self.types[self.i]) {
                return Err(self.fail(
                    "'ALPHA', 'NUM', 'COLON', 'UNIT', 'SPACE', 'BRKT', 'STYLE', 'PCT', 'LABEL'",
                ));
            }
            let mut s = self.texts[self.i].clone();
            self.i += 1;
            while is_style_part(self.types[self.i]) {
                s.push_str(&self.texts[self.i]);
                self.i += 1;
            }
            out.push(s);
            if self.types[self.i] != t::COMMA {
                return Ok(out);
            }
            self.i += 1;
        }
    }

    fn tooltip(&mut self, id: &str) {
        if self.types[self.i] == t::STR {
            let tip = self.texts[self.i].clone();
            self.i += 1;
            self.db.set_tooltip(id, Some(&tip));
        }
    }

    fn link_statement(&mut self, id: &str) -> Result<()> {
        let href = self.expect(t::STR)?;
        let tip = if self.types[self.i] == t::STR {
            let s = self.texts[self.i].clone();
            self.i += 1;
            Some(s)
        } else {
            None
        };
        if self.types[self.i] == t::LINK_TARGET {
            let tgt = self.texts[self.i].clone();
            self.i += 1;
            self.db.set_link(id, &href, Some(&tgt));
        } else {
            self.db.set_link(id, &href, None);
        }
        if let Some(ref tip_str) = tip {
            self.db.set_tooltip(id, Some(tip_str));
        }
        Ok(())
    }

    fn statement(&mut self) -> Result<()> {
        let tok = self.types[self.i];
        match tok {
            t::UNICODE_TEXT | t::ALPHA | t::MINUS | t::BQUOTE_STR => self.class_or_relation(),
            t::NAMESPACE => self.namespace_statement(),
            t::CLASS => {
                self.class_statement()?;
                Ok(())
            }
            t::MEMBER => {
                self.i += 1;
                Ok(())
            }
            t::ANNOTATION_START => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(t::ANNOTATION_END)?;
                let id = self.name(true)?;
                self.db.add_annotation(&id, annotation);
                Ok(())
            }
            t::CALLBACK => {
                self.i += 1;
                let id = self.name(true)?;
                let func = self.expect(t::STR)?;
                self.db.set_click_event(&id, Some(&func), None);
                self.tooltip(&id);
                Ok(())
            }
            t::LINK => {
                self.i += 1;
                let id = self.name(true)?;
                self.link_statement(&id)
            }
            t::CLICK => {
                self.i += 1;
                let id = self.name(true)?;
                if self.types[self.i] == t::HREF {
                    self.i += 1;
                    return self.link_statement(&id);
                }
                let func = self.expect(t::CALLBACK_NAME)?;
                if self.types[self.i] == t::CALLBACK_ARGS {
                    let args = self.texts[self.i].clone();
                    self.i += 1;
                    self.db.set_click_event(&id, Some(&func), Some(&args));
                } else {
                    self.db.set_click_event(&id, Some(&func), None);
                }
                self.tooltip(&id);
                Ok(())
            }
            t::STYLE => {
                self.i += 1;
                let id = self.expect(t::ALPHA)?;
                let st = self.styles()?;
                self.db.set_css_style(&id, &st);
                Ok(())
            }
            t::CSSCLASS => {
                self.i += 1;
                let ids = self.expect(t::STR)?;
                let cls = self.expect(t::ALPHA)?;
                self.db.set_css_class(&ids, &cls);
                Ok(())
            }
            t::NOTE | t::NOTE_FOR => {
                self.note_statement()?;
                Ok(())
            }
            t::CLASSDEF => {
                self.i += 1;
                let mut ids = vec![self.expect(t::ALPHA)?];
                while self.types[self.i] == t::COMMA {
                    self.i += 1;
                    ids.push(self.expect(t::ALPHA)?);
                }
                let st = self.styles()?;
                self.db.define_class(&ids, &st);
                Ok(())
            }
            t::direction_tb | t::direction_bt | t::direction_rl | t::direction_lr => {
                self.i += 1;
                let dir = match tok {
                    t::direction_tb => "TB",
                    t::direction_bt => "BT",
                    t::direction_rl => "RL",
                    _ => "LR",
                };
                self.db.set_direction(dir);
                Ok(())
            }
            t::acc_title => {
                self.i += 1;
                let val = self.expect(t::acc_title_value)?;
                self.db.set_acc_title(js_trim(&val));
                Ok(())
            }
            t::acc_descr => {
                self.i += 1;
                let val = self.expect(t::acc_descr_value)?;
                self.db.set_acc_description(js_trim(&val));
                Ok(())
            }
            t::acc_descr_multiline_value => {
                let val = self.texts[self.i].clone();
                self.i += 1;
                self.db.set_acc_description(js_trim(&val));
                Ok(())
            }
            _ => Err(self.fail(
                "'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'NAMESPACE', 'ANNOTATION_START', 'CLASS', 'MEMBER', 'NOTE_FOR', 'NOTE', 'CLASSDEF', 'direction_tb', 'CALLBACK', 'LINK', 'CLICK', 'STYLE', 'CSSCLASS', 'ALPHA', 'MINUS', 'BQUOTE_STR'",
            )),
        }
    }

    fn run(&mut self) -> Result<()> {
        // Mermaid's grammar also accepts statements with no `classDiagram` line before them.
        let header = self.types[self.i] == t::CLASS_DIAGRAM;
        if header {
            self.i += 1;
            self.expect(t::NEWLINE)?;
        }
        loop {
            self.statement()?;
            if self.types[self.i] != t::NEWLINE {
                break;
            }
            self.i += 1;
            if self.types[self.i] == t::EOF || self.types[self.i] == t::END {
                break;
            }
        }
        if header {
            self.expect(t::EOF)?;
        }
        self.expect(t::END)?;
        Ok(())
    }
}

pub fn parse_class_diagram(src_str: &str, db: &mut ClassDb) -> Result<()> {
    let src: Vec<char> = src_str.chars().collect();
    let tokens = tokenize(&src);
    let mut parser = Parser {
        src,
        types: tokens.types,
        texts: tokens.texts,
        starts: tokens.starts,
        i: 0,
        db,
    };
    parser.run()
}
