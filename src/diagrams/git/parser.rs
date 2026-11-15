use crate::chars::is_word;
use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, string_value, title_before, title_value, tokenize, Pattern, Reader, TokenType,
    ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE, SINGLE_LINE_COMMENT, STRING, WHITESPACE, YAML,
};
use crate::errors::Result;

const T_CHERRY_PICK: i32 = 0;
const T_GIT_GRAPH_COLON: i32 = 1;
const T_HIGHLIGHT: i32 = 2;
const T_GIT_GRAPH: i32 = 3;
const T_CHECKOUT: i32 = 4;
const T_REVERSE: i32 = 5;
const T_PARENT: i32 = 6;
const T_COMMIT: i32 = 7;
const T_NORMAL: i32 = 8;
const T_BRANCH: i32 = 9;
const T_ORDER: i32 = 10;
const T_SWITCH: i32 = 11;
const T_TYPE: i32 = 12;
const T_MERGE: i32 = 13;
const T_MSG: i32 = 14;
const T_TAG: i32 = 15;
const T_ID: i32 = 16;
const T_LR: i32 = 17;
const T_TB: i32 = 18;
const T_BT: i32 = 19;
const T_COLON: i32 = 20;
const T_ACC_DESCR: i32 = 21;
const T_ACC_TITLE: i32 = 22;
const T_TITLE: i32 = 23;
const T_INT: i32 = 24;
const T_STRING: i32 = 25;
const T_NEWLINE: i32 = 26;
const T_REFERENCE: i32 = 31;

const LONGER_NAME: &[usize] = &[31];

const fn word(name: &'static str) -> TokenType {
    TokenType::vis_longer(name, Pattern::Literal(name), LONGER_NAME)
}

// Matches /\w([-\./\w]*[-\w])?/y
fn scan_reference(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !is_word(*src.get(at)?) {
        return None;
    }
    let mut q = at + 1;
    while q < src.len() {
        let c = src[q];
        if is_word(c) || c == '-' || c == '.' || c == '/' {
            q += 1;
        } else {
            break;
        }
    }
    while q > at + 1 && (src[q - 1] == '.' || src[q - 1] == '/') {
        q -= 1;
    }
    Some(q)
}

// Matches /^[-./\w]*[-\w]/ starting at `after_title`.
fn title_rest_matches(src: &[char], after_title: usize) -> bool {
    let mut q = after_title;
    while q < src.len() {
        let c = src[q];
        if is_word(c) || c == '-' {
            return true;
        }
        if c == '.' || c == '/' {
            q += 1;
        } else {
            return false;
        }
    }
    false
}

// Matches /0|[1-9][0-9]*(?!\.)/y
fn scan_int(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let first = *src.get(at)?;
    if first == '0' {
        return Some(at + 1);
    }
    if !first.is_ascii_digit() {
        return None;
    }
    let mut end = at + 1;
    while end < src.len() && src[end].is_ascii_digit() {
        end += 1;
    }
    if src.get(end) == Some(&'.') {
        if end - 1 > at {
            Some(end - 1)
        } else {
            None
        }
    } else {
        Some(end)
    }
}

pub const GIT_TOKENS: &[TokenType] = &[
    word("cherry-pick"),
    TokenType::vis("gitGraph:", Pattern::Literal("gitGraph:")),
    word("HIGHLIGHT"),
    TokenType::vis_longer("gitGraph", Pattern::Keyword("gitGraph"), LONGER_NAME),
    word("checkout"),
    word("REVERSE"),
    TokenType::vis("parent:", Pattern::Literal("parent:")),
    word("commit"),
    word("NORMAL"),
    word("branch"),
    TokenType::vis("order:", Pattern::Literal("order:")),
    word("switch"),
    TokenType::vis("type:", Pattern::Literal("type:")),
    word("merge"),
    TokenType::vis("msg:", Pattern::Literal("msg:")),
    TokenType::vis("tag:", Pattern::Literal("tag:")),
    TokenType::vis("id:", Pattern::Literal("id:")),
    word("LR"),
    word("TB"),
    word("BT"),
    TokenType::vis(":", Pattern::Literal(":")),
    ACC_DESCR,
    ACC_TITLE,
    title_before(title_rest_matches),
    TokenType::vis("INT", Pattern::Custom(scan_int)),
    STRING,
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
    TokenType::vis("REFERENCE", Pattern::Custom(scan_reference)),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitTypeName {
    Normal,
    Reverse,
    Highlight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitDirection {
    Lr,
    Tb,
    Bt,
}

#[derive(Debug, Clone)]
pub enum StatementAst {
    Commit {
        id: Option<String>,
        message: Option<String>,
        tags: Vec<String>,
        commit_type: Option<CommitTypeName>,
    },
    Branch {
        name: String,
        order: Option<f64>,
    },
    Merge {
        branch: String,
        id: Option<String>,
        tags: Vec<String>,
        commit_type: Option<CommitTypeName>,
    },
    Checkout {
        branch: String,
    },
    CherryPicking {
        id: Option<String>,
        tags: Vec<String>,
        parent: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct GitGraphAst {
    pub dir: Option<GitDirection>,
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub statements: Vec<StatementAst>,
}

fn end_of_line(r: &mut Reader<'_>) -> Result<()> {
    if r.kind() == -1 {
        return Ok(());
    }
    if r.kind() != T_NEWLINE {
        return Err(r.fail("a line break or the end of input"));
    }
    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    Ok(())
}

fn parse_name(r: &mut Reader<'_>) -> Result<String> {
    if r.kind() == T_REFERENCE {
        return Ok(r.take());
    }
    if r.kind() == T_STRING {
        return Ok(string_value(&r.take()));
    }
    Err(r.fail("a branch name"))
}

fn parse_string(r: &mut Reader<'_>) -> Result<String> {
    let raw = r.expect(T_STRING)?;
    Ok(string_value(&raw))
}

fn parse_commit_type(r: &mut Reader<'_>) -> Result<CommitTypeName> {
    match r.kind() {
        T_NORMAL => {
            r.i += 1;
            Ok(CommitTypeName::Normal)
        }
        T_REVERSE => {
            r.i += 1;
            Ok(CommitTypeName::Reverse)
        }
        T_HIGHLIGHT => {
            r.i += 1;
            Ok(CommitTypeName::Highlight)
        }
        _ => Err(r.fail("NORMAL, REVERSE, or HIGHLIGHT")),
    }
}

pub fn parse_git(src: &str) -> Result<GitGraphAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, GIT_TOKENS, "gitGraph")?;
    let mut r = Reader::new(&chars, tokens, GIT_TOKENS, "gitGraph");
    let mut ast = GitGraphAst {
        dir: None,
        title: None,
        acc_title: None,
        acc_descr: None,
        statements: Vec::new(),
    };

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    if !r.accept(T_GIT_GRAPH_COLON) {
        r.expect(T_GIT_GRAPH)?;
        let next = r.kind_at(r.i + 1);
        if (r.kind() == T_LR || r.kind() == T_TB || r.kind() == T_BT) && next == T_COLON {
            ast.dir = Some(match r.kind() {
                T_TB => GitDirection::Tb,
                T_BT => GitDirection::Bt,
                _ => GitDirection::Lr,
            });
            r.i += 2;
        } else {
            r.accept(T_COLON);
        }
    }

    while r.kind() != -1 {
        match r.kind() {
            T_NEWLINE => {
                r.i += 1;
            }
            T_ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(&r.take()));
                end_of_line(&mut r)?;
            }
            T_ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            T_TITLE => {
                ast.title = Some(title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            T_COMMIT => {
                r.i += 1;
                let mut id = None;
                let mut message = None;
                let mut tags = Vec::new();
                let mut commit_type = None;
                loop {
                    if r.accept(T_ID) {
                        id = Some(parse_string(&mut r)?);
                    } else if r.accept(T_TAG) {
                        tags.push(parse_string(&mut r)?);
                    } else if r.accept(T_TYPE) {
                        commit_type = Some(parse_commit_type(&mut r)?);
                    } else if r.accept(T_MSG) || r.kind() == T_STRING {
                        message = Some(parse_string(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Commit { id, message, tags, commit_type });
            }
            T_BRANCH => {
                r.i += 1;
                let name = parse_name(&mut r)?;
                let order = if r.accept(T_ORDER) {
                    let raw = r.expect(T_INT)?;
                    raw.parse::<f64>().ok()
                } else {
                    None
                };
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Branch { name, order });
            }
            T_MERGE => {
                r.i += 1;
                let branch = parse_name(&mut r)?;
                let mut id = None;
                let mut tags = Vec::new();
                let mut commit_type = None;
                loop {
                    if r.accept(T_ID) {
                        id = Some(parse_string(&mut r)?);
                    } else if r.accept(T_TAG) {
                        tags.push(parse_string(&mut r)?);
                    } else if r.accept(T_TYPE) {
                        commit_type = Some(parse_commit_type(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Merge { branch, id, tags, commit_type });
            }
            T_CHECKOUT | T_SWITCH => {
                r.i += 1;
                let branch = parse_name(&mut r)?;
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::Checkout { branch });
            }
            T_CHERRY_PICK => {
                r.i += 1;
                let mut id = None;
                let mut tags = Vec::new();
                let mut parent = None;
                loop {
                    if r.accept(T_ID) {
                        id = Some(parse_string(&mut r)?);
                    } else if r.accept(T_TAG) {
                        tags.push(parse_string(&mut r)?);
                    } else if r.accept(T_PARENT) {
                        parent = Some(parse_string(&mut r)?);
                    } else {
                        break;
                    }
                }
                end_of_line(&mut r)?;
                ast.statements.push(StatementAst::CherryPicking { id, tags, parent });
            }
            _ => return Err(r.fail("a statement, a title, or a line break")),
        }
    }

    Ok(ast)
}
