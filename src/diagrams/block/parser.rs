use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

use super::db::{
    edge_str_to_edge_data, edge_str_to_edge_start_data, edge_str_to_pattern,
    edge_str_to_thickness, type_str_2_type, BlockDb, RawBlock, Statement,
};
use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};

struct NodeInfo {
    id: String,
    label: Option<String>,
    type_str: Option<String>,
    directions: Vec<String>,
}

struct Frame {
    list: Vec<Statement>,
    header: Option<Statement>,
}

const STATEMENT_START: &str =
    "'NODE_ID', 'COLUMNS', 'SPACE_BLOCK', 'BLOCK_DIAGRAM_KEY', 'id-block', 'classDef', 'class', 'style'";

fn close_document(mut list: Vec<Statement>) -> Vec<Statement> {
    if matches!(list.last(), Some(Statement::Chain(_))) {
        if let Some(Statement::Chain(chain)) = list.pop() {
            list.extend(chain.into_iter().map(Statement::Single));
        }
    }
    list
}

fn parse_int_saturated(s: &str) -> i64 {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix('-') {
        let mut acc = 0i64;
        for b in rest.bytes() {
            if !b.is_ascii_digit() {
                break;
            }
            acc = acc.saturating_mul(10).saturating_sub((b - b'0') as i64);
        }
        acc
    } else {
        let mut acc = 0i64;
        for b in trimmed.bytes() {
            if !b.is_ascii_digit() {
                break;
            }
            acc = acc.saturating_mul(10).saturating_add((b - b'0') as i64);
        }
        acc
    }
}

fn trim_js(s: &str) -> String {
    s.trim_matches(is_space).to_string()
}

pub fn parse_block(source: &str, db: &mut BlockDb) -> Result<()> {
    let src: Vec<char> = source.chars().collect();
    let tokens = tokenize(&src);
    let types = &tokens.types;
    let texts = &tokens.texts;
    let starts = &tokens.starts;
    let mut i = 0usize;

    let fail = |idx: usize, expected: &str| -> PeleError {
        let tok = types[idx];
        if tok == t::ERROR {
            syntax_error("block", &src, starts[idx], "", true)
        } else {
            syntax_error(
                "block",
                &src,
                starts[idx],
                &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok]),
                false,
            )
        }
    };

    let expect = |i: &mut usize, tok: usize| -> Result<String> {
        if types[*i] != tok {
            return Err(fail(*i, &format!("'{}'", TOKEN_NAMES[tok])));
        }
        let s = texts[*i].clone();
        *i += 1;
        Ok(s)
    };

    let parse_node = |i: &mut usize| -> Result<NodeInfo> {
        let id = expect(i, t::NODE_ID)?;
        if types[*i] == t::NODE_DSTART {
            let open = texts[*i].clone();
            *i += 1;
            let label = expect(i, t::STR)?;
            let close = expect(i, t::NODE_DEND)?;
            return Ok(NodeInfo {
                id,
                label: Some(label),
                type_str: Some(format!("{}{}", open, close)),
                directions: Vec::new(),
            });
        }
        if types[*i] == t::BLOCK_ARROW_START {
            let open = texts[*i].clone();
            *i += 1;
            let label = expect(i, t::STR)?;
            let mut directions = vec![expect(i, t::DIR)?];
            while types[*i] == t::DIR {
                directions.push(texts[*i].clone());
                *i += 1;
            }
            let close = expect(i, t::BLOCK_ARROW_END)?;
            return Ok(NodeInfo {
                id,
                label: Some(label),
                type_str: Some(format!("{}{}", open, close)),
                directions,
            });
        }
        Ok(NodeInfo {
            id,
            label: None,
            type_str: None,
            directions: Vec::new(),
        })
    };

    let parse_node_statement = |i: &mut usize| -> Result<Statement> {
        let first = parse_node(i)?;
        let size = if types[*i] == t::SIZE {
            let v = parse_int_saturated(&texts[*i]);
            *i += 1;
            v
        } else {
            1
        };
        let from = RawBlock {
            id: Some(first.id),
            label: first.label,
            block_type: Some(type_str_2_type(first.type_str.as_deref()).to_string()),
            directions: first.directions,
            width_in_columns: Some(size),
            ..Default::default()
        };
        let mut chain: Option<Vec<RawBlock>> = None;
        while types[*i] == t::LINK || types[*i] == t::START_LINK {
            let link_kind = types[*i];
            *i += 1;
            let mut label = String::new();
            if link_kind == t::START_LINK {
                expect(i, t::LINK_LABEL)?;
                label = expect(i, t::STR)?;
                expect(i, t::LINK)?;
            }
            let link = texts[*i - 1].clone();
            let to = parse_node(i)?;
            let arrow_type_end = edge_str_to_edge_data(&link).to_string();
            let arrow_type_start = edge_str_to_edge_start_data(&link).to_string();
            let thickness = edge_str_to_thickness(&link).to_string();
            let pattern = edge_str_to_pattern(&link).to_string();

            let list = chain.get_or_insert_with(|| {
                vec![RawBlock {
                    id: from.id.clone(),
                    label: from.label.clone(),
                    block_type: from.block_type.clone(),
                    directions: from.directions.clone(),
                    ..Default::default()
                }]
            });
            let prev_id = list.last().and_then(|b| b.id.clone()).unwrap_or_default();
            let next_block = RawBlock {
                id: Some(to.id.clone()),
                label: to.label,
                block_type: Some(type_str_2_type(to.type_str.as_deref()).to_string()),
                directions: to.directions.clone(),
                ..Default::default()
            };
            list.push(RawBlock {
                id: Some(format!("{}-{}", prev_id, to.id)),
                start: Some(prev_id),
                end: Some(to.id),
                label: Some(label),
                block_type: Some("edge".to_string()),
                thickness: Some(thickness),
                pattern: Some(pattern),
                directions: to.directions,
                arrow_type_end: Some(arrow_type_end),
                arrow_type_start: Some(arrow_type_start),
                ..Default::default()
            });
            list.push(next_block);
        }
        Ok(match chain {
            Some(c) => Statement::Chain(c),
            None => Statement::Single(from),
        })
    };

    expect(&mut i, t::KEY)?;
    let mut open: Vec<Frame> = Vec::new();
    let mut list: Vec<Statement> = Vec::new();
    loop {
        match types[i] {
            t::NODE_ID => {
                list.push(parse_node_statement(&mut i)?);
            }
            t::COLUMNS => {
                let cols = parse_int_saturated(&texts[i]);
                i += 1;
                list.push(Statement::Single(RawBlock {
                    block_type: Some("column-setting".to_string()),
                    columns: Some(cols),
                    ..Default::default()
                }));
            }
            t::SPACE_BLOCK => {
                let width = parse_int_saturated(&texts[i]);
                i += 1;
                let id = db.generate_id();
                list.push(Statement::Single(RawBlock {
                    id: Some(id),
                    block_type: Some("space".to_string()),
                    label: Some(String::new()),
                    width: Some(width),
                    ..Default::default()
                }));
            }
            t::KEY => {
                i += 1;
                open.push(Frame { list, header: None });
                list = Vec::new();
            }
            t::ID_BLOCK => {
                i += 1;
                if types[i] != t::NODE_ID {
                    return Err(fail(i, "'NODE_ID'"));
                }
                let header = parse_node_statement(&mut i)?;
                open.push(Frame {
                    list,
                    header: Some(header),
                });
                list = Vec::new();
            }
            t::end => {
                let Some(frame) = open.pop() else {
                    return Err(fail(i, STATEMENT_START));
                };
                if list.is_empty() {
                    return Err(fail(i, STATEMENT_START));
                }
                i += 1;
                let id = db.generate_id();
                let children = close_document(list);
                list = frame.list;
                let composite = match frame.header {
                    None => RawBlock {
                        id: Some(id),
                        block_type: Some("composite".to_string()),
                        label: Some(String::new()),
                        children,
                        ..Default::default()
                    },
                    Some(Statement::Single(mut hdr)) => {
                        hdr.block_type = Some("composite".to_string());
                        hdr.children = children;
                        hdr
                    }
                    // Spreading an array header in JS yields `{ type: 'composite', children }` with `id: undefined`.
                    Some(Statement::Chain(_)) => RawBlock {
                        id: None,
                        block_type: Some("composite".to_string()),
                        children,
                        ..Default::default()
                    },
                };
                list.push(Statement::Single(composite));
            }
            t::CLASSDEF => {
                i += 1;
                let id = trim_js(&expect(&mut i, t::CLASSDEF_ID)?);
                let css = trim_js(&expect(&mut i, t::CLASSDEF_STYLEOPTS)?);
                list.push(Statement::Single(RawBlock {
                    block_type: Some("classDef".to_string()),
                    id: Some(id),
                    css: Some(css),
                    ..Default::default()
                }));
            }
            t::CLASS => {
                i += 1;
                let id = trim_js(&expect(&mut i, t::CLASSENTITY_IDS)?);
                let style_class = trim_js(&expect(&mut i, t::STYLECLASS)?);
                list.push(Statement::Single(RawBlock {
                    block_type: Some("applyClass".to_string()),
                    id: Some(id),
                    style_class: Some(style_class),
                    ..Default::default()
                }));
            }
            t::STYLE => {
                i += 1;
                let id = trim_js(&expect(&mut i, t::STYLE_ENTITY_IDS)?);
                let styles_str = trim_js(&expect(&mut i, t::STYLE_DEFINITION_DATA)?);
                list.push(Statement::Single(RawBlock {
                    block_type: Some("applyStyles".to_string()),
                    id: Some(id),
                    styles_str: Some(styles_str),
                    ..Default::default()
                }));
            }
            t::EOF => {
                if !open.is_empty() {
                    return Err(fail(i, &format!("{}, 'end'", STATEMENT_START)));
                }
                if list.is_empty() {
                    return Err(fail(i, STATEMENT_START));
                }
                return db.set_hierarchy(close_document(list));
            }
            _ => {
                let expected = if list.is_empty() {
                    STATEMENT_START.to_string()
                } else if !open.is_empty() {
                    format!("{}, 'end'", STATEMENT_START)
                } else {
                    format!("{}, 'EOF'", STATEMENT_START)
                };
                return Err(fail(i, &expected));
            }
        }
    }
}
