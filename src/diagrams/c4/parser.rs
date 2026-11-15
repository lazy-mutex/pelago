use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

use super::db::{C4Attr, C4Db, C4Named};
use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};

const KINDS: &[&str] = &[
    "person",
    "external_person",
    "system",
    "system_db",
    "system_queue",
    "external_system",
    "external_system_db",
    "external_system_queue",
    "container",
    "container_db",
    "container_queue",
    "external_container",
    "external_container_db",
    "external_container_queue",
    "component",
    "component_db",
    "component_queue",
    "external_component",
    "external_component_db",
    "external_component_queue",
];

const REL_TYPES: &[&str] = &["rel", "birel", "rel_u", "rel_d", "rel_l", "rel_r", "rel_b", "rel"];
const NODE_TYPES: &[&str] = &["node", "nodeL", "nodeR"];

const HEADERS: &str = "'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'C4_CONTEXT', 'C4_CONTAINER', 'C4_COMPONENT', 'C4_DYNAMIC', 'C4_DEPLOYMENT'";
const ATTRIBUTES: &str = "'STR', 'STR_KEY', 'ATTRIBUTE', 'ATTRIBUTE_EMPTY'";

#[derive(Clone, Copy, PartialEq, Eq)]
enum P {
    Item,
    Diagram,
    ItemOrEnd,
    AfterOther,
    AfterDiagram,
}

fn is_other(tok: usize) -> bool {
    matches!(
        tok,
        t::title | t::accDescription | t::acc_title | t::acc_descr | t::acc_descr_multiline_value
    )
}

fn is_diagram(tok: usize) -> bool {
    (t::PERSON..=t::UPDATE_LAYOUT_CONFIG).contains(&tok)
}

fn trim_js(s: &str) -> String {
    s.trim_matches(is_space).to_string()
}

pub fn parse_c4(source: &str, db: &mut C4Db) -> Result<()> {
    let src: Vec<char> = source.chars().collect();
    let tokens = tokenize(&src);
    let types = &tokens.types;
    let starts = &tokens.starts;
    let ends = &tokens.ends;
    let mut i = 0usize;

    let slice_from = |at: usize, skip: usize| -> String {
        let s = (starts[at] + skip).min(ends[at]);
        src[s..ends[at]].iter().collect()
    };
    let text = |at: usize| -> String { src[starts[at]..ends[at]].iter().collect() };

    let fail = |idx: usize, expected: &str| -> PeleError {
        let tok = types[idx];
        if tok == t::ERROR {
            syntax_error("c4", &src, starts[idx], "", true)
        } else {
            syntax_error("c4", &src, starts[idx], &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok]), false)
        }
    };

    let expect = |i: &mut usize, tok: usize| -> Result<()> {
        if types[*i] != tok {
            return Err(fail(*i, &format!("'{}'", TOKEN_NAMES[tok])));
        }
        *i += 1;
        Ok(())
    };

    let header = types[i];
    if !(t::C4_CONTEXT..=t::C4_DEPLOYMENT).contains(&header) {
        return Err(fail(i, HEADERS));
    }
    i += 1;
    expect(&mut i, t::NEWLINE)?;

    let mut depth = 0usize;
    let mut state = P::Item;
    loop {
        let tok = types[i];
        if state == P::AfterOther || state == P::AfterDiagram {
            if tok == t::NEWLINE {
                i += 1;
                state = P::ItemOrEnd;
                continue;
            }
            if state == P::AfterOther && is_diagram(tok) {
                state = P::Diagram;
                continue;
            }
        } else if is_diagram(tok) {
            i += 1;
            let mut attrs: Vec<C4Attr> = Vec::new();
            loop {
                let at = types[i];
                if at == t::STR {
                    attrs.push(C4Attr::Positional(trim_js(&text(i))));
                    i += 1;
                } else if at == t::ATTRIBUTE_EMPTY {
                    attrs.push(C4Attr::Positional(String::new()));
                    i += 1;
                } else if at == t::STR_KEY {
                    i += 1;
                    if types[i] != t::STR_VALUE {
                        return Err(fail(i, "'STR_VALUE'"));
                    }
                    attrs.push(C4Attr::Named(C4Named {
                        key: trim_js(&text(i - 1)),
                        value: trim_js(&text(i)),
                    }));
                    i += 1;
                } else {
                    break;
                }
            }
            if attrs.is_empty() {
                return Err(fail(i, ATTRIBUTES));
            }

            let mut opens_boundary = false;
            if tok <= t::SYSTEM_EXT_QUEUE {
                db.add_person_or_system(KINDS[tok - t::PERSON], &attrs);
            } else if tok <= t::CONTAINER_EXT_QUEUE {
                db.add_container(KINDS[tok - t::PERSON], &attrs);
            } else if tok <= t::COMPONENT_EXT_QUEUE {
                db.add_component(KINDS[tok - t::PERSON], &attrs);
            } else if tok <= t::NODE_R {
                let insert_at = 2.min(attrs.len());
                if tok == t::ENTERPRISE_BOUNDARY {
                    attrs.insert(insert_at, C4Attr::Positional("ENTERPRISE".to_string()));
                } else if tok == t::SYSTEM_BOUNDARY {
                    attrs.insert(insert_at, C4Attr::Positional("SYSTEM".to_string()));
                } else if tok == t::CONTAINER_BOUNDARY {
                    attrs.insert(insert_at, C4Attr::Positional("CONTAINER".to_string()));
                }
                if tok >= t::NODE {
                    db.add_deployment_node(NODE_TYPES[tok - t::NODE], &attrs);
                } else if tok == t::CONTAINER_BOUNDARY {
                    db.add_container_boundary(&attrs);
                } else {
                    db.add_person_or_system_boundary(&attrs, "system");
                }
                if types[i] == t::LBRACE {
                    i += 1;
                    expect(&mut i, t::NEWLINE)?;
                } else if types[i] == t::NEWLINE {
                    i += 1;
                    expect(&mut i, t::LBRACE)?;
                    if types[i] == t::NEWLINE {
                        i += 1;
                    }
                } else {
                    return Err(fail(i, "'NEWLINE', 'LBRACE'"));
                }
                opens_boundary = true;
            } else if tok <= t::REL_INDEX {
                if tok == t::REL_INDEX && !attrs.is_empty() {
                    attrs.remove(0);
                }
                db.add_rel(REL_TYPES[tok - t::REL], &attrs);
            } else if tok == t::UPDATE_EL_STYLE {
                db.update_el_style(&attrs);
            } else if tok == t::UPDATE_REL_STYLE {
                db.update_rel_style(&attrs);
            } else {
                db.update_layout_config(&attrs);
            }

            if opens_boundary {
                depth += 1;
                state = P::Diagram;
            } else {
                state = P::AfterDiagram;
            }
            continue;
        } else if is_other(tok) && state != P::Diagram {
            if tok == t::title {
                db.set_title(slice_from(i, 6));
                i += 1;
            } else if tok == t::accDescription {
                db.set_acc_description(&slice_from(i, 15));
                i += 1;
            } else if tok == t::acc_descr_multiline_value {
                db.set_acc_description(&trim_js(&text(i)));
                i += 1;
            } else {
                i += 1;
                if tok == t::acc_title {
                    if types[i] != t::acc_title_value {
                        return Err(fail(i, "'acc_title_value'"));
                    }
                    db.set_title(trim_js(&text(i)));
                    i += 1;
                } else {
                    if types[i] != t::acc_descr_value {
                        return Err(fail(i, "'acc_descr_value'"));
                    }
                    db.set_acc_description(&trim_js(&text(i)));
                    i += 1;
                }
            }
            state = P::AfterOther;
            continue;
        } else if state != P::ItemOrEnd {
            return Err(fail(
                i,
                "'title', 'accDescription', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'PERSON', 'REL'",
            ));
        }

        if depth > 0 {
            if tok != t::RBRACE {
                return Err(fail(i, "'NEWLINE', 'RBRACE'"));
            }
            i += 1;
            db.pop_boundary_parse_stack();
            depth -= 1;
            state = P::AfterDiagram;
        } else {
            if tok != t::EOF {
                return Err(fail(i, "'NEWLINE', 'EOF'"));
            }
            i += 1;
            expect(&mut i, t::END)?;
            db.set_c4_type(text(0));
            return Ok(());
        }
    }
}
