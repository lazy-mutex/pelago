use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};
use super::types::{
    ApplyClassStmt, ClassDefStmt, ClickStmt, ClickTarget, Description, DirectionStmt, RelationStmt,
    StateNote, StateStmt, Stmt, StyleStmt,
};
use crate::chars::is_space;
use crate::errors::{locate, syntax_error, ErrorCode, PeleError, Result};

pub trait StateBuilder {
    fn set_root_doc(&mut self, doc: Vec<Stmt>) -> Result<()>;
    fn trim_colon(&self, text: &str) -> String;
    fn get_divider_id(&mut self) -> String;
    fn set_direction(&mut self, dir: &str);
    fn set_acc_title(&mut self, text: &str);
    fn set_acc_description(&mut self, text: &str);
}

const DIRECTIONS: [&str; 4] = ["TB", "BT", "RL", "LR"];
const STATEMENT: &str = "'NL', 'ID', 'HIDE_EMPTY', 'scale', 'COMPOSIT_STATE', 'STATE_DESCR', 'FORK', 'JOIN', 'CHOICE', 'CONCURRENT', 'note', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'CLICK', 'classDef', 'style', 'class', 'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'EDGE_STATE'";

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

pub fn parse_state(src_str: &str, db: &mut impl StateBuilder) -> Result<Vec<Stmt>> {
    let src: Vec<char> = src_str.chars().collect();
    let tokens = tokenize(&src);
    let types = &tokens.types;
    let texts = &tokens.texts;
    let starts = &tokens.starts;
    let error = &tokens.error;
    let mut i = 0usize;

    let fail = |idx: usize, expected: &str| -> PeleError {
        let tok = types[idx];
        if tok == t::ERROR {
            if let Some(msg) = error {
                let loc = locate(&src, starts[idx]);
                let mut err = PeleError::new(msg.clone(), ErrorCode::Syntax).of("state");
                err.line = loc.line;
                err.column = loc.column;
                err.snippet = loc.snippet;
                return err;
            }
            return syntax_error("state", &src, starts[idx], "", true);
        }
        syntax_error(
            "state",
            &src,
            starts[idx],
            &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok as usize]),
            false,
        )
    };

    let expect = |i: &mut usize, expected_type: u16| -> Result<String> {
        if types[*i] != expected_type {
            return Err(fail(*i, &format!("'{}'", TOKEN_NAMES[expected_type as usize])));
        }
        let val = texts[*i].clone();
        *i += 1;
        Ok(val)
    };

    let id_statement = |i: &mut usize| -> Result<StateStmt> {
        let tok = types[*i];
        if tok != t::ID && tok != t::EDGE_STATE {
            return Err(fail(*i, "'ID', 'EDGE_STATE'"));
        }
        let id = js_trim(&texts[*i]).to_string();
        *i += 1;
        let mut stmt = StateStmt::new(id, Some("default".to_string()));
        stmt.description = Some(Description::Single(String::new()));
        if types[*i] != t::STYLE_SEPARATOR {
            return Ok(stmt);
        }
        *i += 1;
        let cls = js_trim(&expect(i, t::ID)?).to_string();
        stmt.classes = vec![cls];
        Ok(stmt)
    };

    while types[i] == t::NL {
        i += 1;
    }
    if types[i] != t::SD {
        return Err(fail(i, "'NL', 'SD'"));
    }
    i += 1;

    let mut doc: Vec<Stmt> = Vec::new();
    let mut outer: Vec<(String, String, Vec<Stmt>)> = Vec::new();

    // The grammar drops a statement whose value is the string it uses for a blank line.
    let add = |doc: &mut Vec<Stmt>, value: Stmt| {
        if let Stmt::Text(ref s) = value {
            if s == "nl" {
                return;
            }
        }
        doc.push(value);
    };

    loop {
        let tok = types[i];
        match tok {
            t::NL => {
                i += 1;
            }
            t::ID | t::EDGE_STATE => {
                let mut first = id_statement(&mut i)?;
                if types[i] == t::DESCR {
                    first.description = Some(Description::Single(db.trim_colon(&texts[i])));
                    i += 1;
                    add(&mut doc, Stmt::State(first));
                } else if types[i] == t::ARROW {
                    i += 1;
                    let second = id_statement(&mut i)?;
                    if types[i] == t::DESCR {
                        let desc = db.trim_colon(&texts[i]);
                        i += 1;
                        add(
                            &mut doc,
                            Stmt::Relation(RelationStmt {
                                state1: first,
                                state2: second,
                                description: Some(desc),
                            }),
                        );
                    } else {
                        add(
                            &mut doc,
                            Stmt::Relation(RelationStmt {
                                state1: first,
                                state2: second,
                                description: None,
                            }),
                        );
                    }
                } else {
                    add(&mut doc, Stmt::State(first));
                }
            }
            t::HIDE_EMPTY => {
                let text = texts[i].clone();
                i += 1;
                add(&mut doc, Stmt::Text(text));
            }
            t::scale => {
                let kw = texts[i].clone();
                i += 1;
                expect(&mut i, t::WIDTH)?;
                add(&mut doc, Stmt::Text(kw));
            }
            t::COMPOSIT_STATE => {
                let text = texts[i].clone();
                i += 1;
                if types[i] == t::STRUCT_START {
                    i += 1;
                    let prev = std::mem::take(&mut doc);
                    outer.push((text, String::new(), prev));
                } else {
                    add(&mut doc, Stmt::Text(text));
                }
            }
            t::STATE_DESCR => {
                let description = texts[i].clone();
                i += 1;
                expect(&mut i, t::AS)?;
                let id = expect(&mut i, t::ID)?;
                if types[i] == t::STRUCT_START {
                    i += 1;
                    let prev = std::mem::take(&mut doc);
                    outer.push((id, description, prev));
                } else if id.contains(':') {
                    let mut parts = id.split(':');
                    let p0 = parts.next().unwrap_or("").to_string();
                    let p1 = parts.next().unwrap_or("").to_string();
                    let mut st = StateStmt::new(p0, Some("default".to_string()));
                    st.description = Some(Description::List(vec![js_trim(&description).to_string(), p1]));
                    add(&mut doc, Stmt::State(st));
                } else {
                    let mut st = StateStmt::new(id, Some("default".to_string()));
                    st.description = Some(Description::Single(js_trim(&description).to_string()));
                    add(&mut doc, Stmt::State(st));
                }
            }
            t::FORK => {
                let id = texts[i].clone();
                i += 1;
                add(&mut doc, Stmt::State(StateStmt::new(id, Some("fork".to_string()))));
            }
            t::JOIN => {
                let id = texts[i].clone();
                i += 1;
                add(&mut doc, Stmt::State(StateStmt::new(id, Some("join".to_string()))));
            }
            t::CHOICE => {
                let id = texts[i].clone();
                i += 1;
                add(&mut doc, Stmt::State(StateStmt::new(id, Some("choice".to_string()))));
            }
            t::CONCURRENT => {
                i += 1;
                let id = db.get_divider_id();
                add(&mut doc, Stmt::State(StateStmt::new(id, Some("divider".to_string()))));
            }
            t::note => {
                let keyword = texts[i].clone();
                i += 1;
                if types[i] == t::left_of || types[i] == t::right_of {
                    let position = js_trim(&texts[i]).to_string();
                    i += 1;
                    let id = js_trim(&expect(&mut i, t::ID)?).to_string();
                    let note_text = js_trim(&expect(&mut i, t::NOTE_TEXT)?).to_string();
                    let mut st = StateStmt::new(id, None);
                    st.note = Some(StateNote { position: Some(position), text: note_text });
                    add(&mut doc, Stmt::State(st));
                } else {
                    if types[i] != t::NOTE_TEXT {
                        return Err(fail(i, "'NOTE_TEXT', 'left_of', 'right_of'"));
                    }
                    i += 1;
                    expect(&mut i, t::AS)?;
                    expect(&mut i, t::ID)?;
                    add(&mut doc, Stmt::Text(keyword));
                }
            }
            t::direction_tb | t::direction_bt | t::direction_rl | t::direction_lr => {
                let value = DIRECTIONS[(tok - t::direction_tb) as usize].to_string();
                i += 1;
                db.set_direction(&value);
                add(&mut doc, Stmt::Dir(DirectionStmt { value }));
            }
            t::acc_title => {
                i += 1;
                let value = js_trim(&expect(&mut i, t::acc_title_value)?).to_string();
                db.set_acc_title(&value);
                add(&mut doc, Stmt::Text(value));
            }
            t::acc_descr => {
                i += 1;
                let value = js_trim(&expect(&mut i, t::acc_descr_value)?).to_string();
                db.set_acc_description(&value);
                add(&mut doc, Stmt::Text(value));
            }
            t::acc_descr_multiline_value => {
                let value = js_trim(&texts[i]).to_string();
                i += 1;
                db.set_acc_description(&value);
                add(&mut doc, Stmt::Text(value));
            }
            t::CLICK => {
                i += 1;
                let id = id_statement(&mut i)?;
                let url: String;
                let mut tooltip = String::new();
                if types[i] == t::HREF {
                    i += 1;
                    url = expect(&mut i, t::STRING)?;
                } else {
                    if types[i] != t::STRING {
                        return Err(fail(i, "'STRING', 'HREF'"));
                    }
                    url = texts[i].clone();
                    i += 1;
                    tooltip = expect(&mut i, t::STRING)?;
                }
                expect(&mut i, t::NL)?;
                add(
                    &mut doc,
                    Stmt::Click(ClickStmt {
                        id: ClickTarget::State(id),
                        url,
                        tooltip,
                    }),
                );
            }
            t::classDef => {
                i += 1;
                let id = js_trim(&expect(&mut i, t::CLASSDEF_ID)?).to_string();
                let classes = js_trim(&expect(&mut i, t::CLASSDEF_STYLEOPTS)?).to_string();
                add(&mut doc, Stmt::ClassDef(ClassDefStmt { id, classes }));
            }
            t::style => {
                i += 1;
                let id = js_trim(&expect(&mut i, t::STYLE_IDS)?).to_string();
                let style_class = js_trim(&expect(&mut i, t::STYLEDEF_STYLEOPTS)?).to_string();
                add(&mut doc, Stmt::Style(StyleStmt { id, style_class }));
            }
            t::class => {
                i += 1;
                let id = js_trim(&expect(&mut i, t::CLASSENTITY_IDS)?).to_string();
                let style_class = js_trim(&expect(&mut i, t::STYLECLASS)?).to_string();
                add(&mut doc, Stmt::ApplyClass(ApplyClassStmt { id, style_class }));
            }
            t::STRUCT_STOP => {
                let Some((open_id, open_descr, mut parent_doc)) = outer.pop() else {
                    return Err(fail(i, &format!("'$end', {}", STATEMENT)));
                };
                i += 1;
                let inner = doc;
                let mut st = StateStmt::new(open_id, Some("default".to_string()));
                st.description = Some(Description::Single(open_descr));
                st.doc = Some(inner);
                parent_doc.push(Stmt::State(st));
                doc = parent_doc;
            }
            t::END => {
                if !outer.is_empty() {
                    return Err(fail(i, &format!("'STRUCT_STOP', {}", STATEMENT)));
                }
                db.set_root_doc(doc.clone())?;
                return Ok(doc);
            }
            _ => {
                if !outer.is_empty() {
                    return Err(fail(i, &format!("'STRUCT_STOP', {}", STATEMENT)));
                } else {
                    return Err(fail(i, &format!("'$end', {}", STATEMENT)));
                }
            }
        }
    }
}
