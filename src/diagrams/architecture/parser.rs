use crate::chars::{is_line_end, is_space, is_word};
use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, string_value, title_before, title_value, tokenize, Pattern,
    Reader, TokenType, ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE, SINGLE_LINE_COMMENT, STRING,
    WHITESPACE, YAML,
};
use crate::errors::Result;

const T_HEADER: i32 = 0;
const T_JUNCTION: i32 = 1;
const T_SERVICE: i32 = 2;
const T_COLUMN: i32 = 3;
const T_GROUP: i32 = 4;
const T_ALIGN: i32 = 5;
const T_ROW: i32 = 6;
const T_DASHES: i32 = 7;
const T_IN: i32 = 8;
const T_COLON: i32 = 9;
const T_DASH: i32 = 10;
const T_DIRECTION: i32 = 11;
const T_ARROW_GROUP: i32 = 12;
const T_INTO: i32 = 13;
const T_ACC_DESCR: i32 = 14;
const T_ACC_TITLE: i32 = 15;
const T_TITLE: i32 = 16;
const T_STRING: i32 = 17;
const T_ID: i32 = 18;
const T_NEWLINE: i32 = 19;
const T_YAML: usize = 21;
const T_ICON: i32 = 24;
const T_LABEL: i32 = 25;

const T_ID_USIZE: usize = 18;

fn match_id(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !src.get(at).copied().is_some_and(is_word) {
        return None;
    }
    let mut last_word = at + 1;
    let mut j = at + 1;
    while j < src.len() && (is_word(src[j]) || src[j] == '-') {
        if is_word(src[j]) {
            last_word = j + 1;
        }
        j += 1;
    }
    Some(last_word)
}

fn title_rest(src: &[char], mut i: usize) -> bool {
    while i < src.len() && (is_word(src[i]) || src[i] == '-') {
        if is_word(src[i]) {
            return true;
        }
        i += 1;
    }
    false
}

fn match_direction(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    matches!(src.get(at), Some('L' | 'R' | 'T' | 'B')).then_some(at + 1)
}

fn match_into(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    matches!(src.get(at), Some('<' | '>')).then_some(at + 1)
}

fn match_arch_icon(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if src.get(at) != Some(&'(') {
        return None;
    }
    let mut j = at + 1;
    while j < src.len() && (is_word(src[j]) || src[j] == '-' || src[j] == ':') {
        j += 1;
    }
    (j > at + 1 && src.get(j) == Some(&')')).then_some(j + 1)
}

// /\[(?:"([^"\\]|\\.)*"|'([^'\\]|\\.)*'|[^\[\]\r\n]+)\]/y
fn match_arch_title(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if src.get(at) != Some(&'[') {
        return None;
    }
    for quote in ['"', '\''] {
        if src.get(at + 1) == Some(&quote) {
            let mut j = at + 2;
            while j < src.len() {
                let c = src[j];
                if c == quote {
                    if src.get(j + 1) == Some(&']') {
                        return Some(j + 2);
                    }
                    break;
                }
                if c == '\\' {
                    if j + 1 < src.len() && !is_line_end(src[j + 1]) {
                        j += 2;
                    } else {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
        }
    }
    let mut j = at + 1;
    while j < src.len() && !matches!(src[j], '[' | ']' | '\r' | '\n') {
        j += 1;
    }
    (j > at + 1 && src.get(j) == Some(&']')).then_some(j + 1)
}

const fn word(text: &'static str) -> TokenType {
    TokenType::vis_longer(text, Pattern::Literal(text), &[T_ID_USIZE])
}

// The direction letters come before identifiers and have no longer alternative, so Mermaid
// reads an id that starts with L, R, T or B as a direction followed by the rest.
pub const ARCHITECTURE_TOKENS: &[TokenType] = &[
    word("architecture-beta"),
    word("junction"),
    word("service"),
    word("column"),
    word("group"),
    word("align"),
    word("row"),
    TokenType::vis_longer("--", Pattern::Literal("--"), &[T_YAML]),
    word("in"),
    TokenType::vis(":", Pattern::Literal(":")),
    TokenType::vis_longer("-", Pattern::Literal("-"), &[T_YAML]),
    TokenType::vis("ARROW_DIRECTION", Pattern::Custom(match_direction)),
    TokenType::vis("ARROW_GROUP", Pattern::Literal("{group}")),
    TokenType::vis("ARROW_INTO", Pattern::Custom(match_into)),
    ACC_DESCR,
    ACC_TITLE,
    title_before(title_rest),
    STRING,
    TokenType::vis("ID", Pattern::Custom(match_id)),
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
    TokenType::vis("ARCH_ICON", Pattern::Custom(match_arch_icon)),
    TokenType::vis("ARCH_TITLE", Pattern::Custom(match_arch_title)),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    L,
    R,
    T,
    B,
}

impl Side {
    fn from_str(s: &str) -> Self {
        match s {
            "L" => Side::L,
            "R" => Side::R,
            "T" => Side::T,
            _ => Side::B,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignDir {
    Row,
    Column,
}

impl AlignDir {
    pub fn as_str(self) -> &'static str {
        match self {
            AlignDir::Row => "row",
            AlignDir::Column => "column",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GroupAst {
    pub id: String,
    pub icon: Option<String>,
    pub title: Option<String>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceAst {
    pub id: String,
    pub icon: Option<String>,
    pub icon_text: Option<String>,
    pub title: Option<String>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JunctionAst {
    pub id: String,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EdgeAst {
    pub lhs_id: String,
    pub lhs_group: bool,
    pub lhs_dir: Side,
    pub lhs_into: bool,
    pub title: Option<String>,
    pub rhs_into: bool,
    pub rhs_dir: Side,
    pub rhs_id: String,
    pub rhs_group: bool,
}

#[derive(Debug, Clone)]
pub struct AlignmentAst {
    pub direction: AlignDir,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ArchitectureAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub groups: Vec<GroupAst>,
    pub services: Vec<ServiceAst>,
    pub junctions: Vec<JunctionAst>,
    pub edges: Vec<EdgeAst>,
    pub alignments: Vec<AlignmentAst>,
}

// Mermaid's ArchitectureValueConverter for ARCH_TITLE.
fn label_value(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let inner: String = if chars.len() >= 2 {
        chars[1..chars.len() - 1].iter().collect()
    } else {
        String::new()
    };
    let mut result = inner.trim_matches(is_space).to_string();
    if (result.starts_with('"') && result.ends_with('"'))
        || (result.starts_with('\'') && result.ends_with('\''))
    {
        let rchars: Vec<char> = result.chars().collect();
        result = if rchars.len() >= 2 {
            rchars[1..rchars.len() - 1]
                .iter()
                .collect::<String>()
                .replace("\\\"", "\"")
                .replace("\\'", "'")
        } else {
            String::new()
        };
    }
    result.trim_matches(is_space).to_string()
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

pub fn parse_architecture(src: &str) -> Result<ArchitectureAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, ARCHITECTURE_TOKENS, "architecture")?;
    let mut r = Reader::new(&chars, tokens, ARCHITECTURE_TOKENS, "architecture");
    let mut ast = ArchitectureAst::default();

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    r.expect(T_HEADER)?;
    while r.kind() != -1 {
        match r.kind() {
            T_NEWLINE => {
                r.i += 1;
                continue;
            }
            T_ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(&r.take()));
            }
            T_ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(&r.take()));
            }
            T_TITLE => {
                ast.title = Some(title_value(&r.take()));
            }
            T_GROUP => {
                r.i += 1;
                let id = r.expect(T_ID)?;
                let icon = if r.kind() == T_ICON {
                    let raw = r.take();
                    Some(raw[1..raw.len() - 1].to_string())
                } else {
                    None
                };
                let title = if r.kind() == T_LABEL {
                    Some(label_value(&r.take()))
                } else {
                    None
                };
                let parent = if r.accept(T_IN) { Some(r.expect(T_ID)?) } else { None };
                ast.groups.push(GroupAst { id, icon, title, parent });
            }
            T_SERVICE => {
                r.i += 1;
                let id = r.expect(T_ID)?;
                let mut icon = None;
                let mut icon_text = None;
                if r.kind() == T_STRING {
                    icon_text = Some(string_value(&r.take()));
                } else if r.kind() == T_ICON {
                    let raw = r.take();
                    icon = Some(raw[1..raw.len() - 1].to_string());
                }
                let title = if r.kind() == T_LABEL {
                    Some(label_value(&r.take()))
                } else {
                    None
                };
                let parent = if r.accept(T_IN) { Some(r.expect(T_ID)?) } else { None };
                ast.services.push(ServiceAst { id, icon, icon_text, title, parent });
            }
            T_JUNCTION => {
                r.i += 1;
                let id = r.expect(T_ID)?;
                let parent = if r.accept(T_IN) { Some(r.expect(T_ID)?) } else { None };
                ast.junctions.push(JunctionAst { id, parent });
            }
            T_ID => {
                let lhs_id = r.take();
                let lhs_group = r.accept(T_ARROW_GROUP);
                r.expect(T_COLON)?;
                let lhs_dir = Side::from_str(&r.expect(T_DIRECTION)?);
                let lhs_into = r.accept(T_INTO);
                let mut title = None;
                if !r.accept(T_DASHES) {
                    if r.kind() != T_DASH {
                        return Err(r.fail("'--' or '-'"));
                    }
                    r.i += 1;
                    title = Some(label_value(&r.expect(T_LABEL)?));
                    r.expect(T_DASH)?;
                }
                let rhs_into = r.accept(T_INTO);
                let rhs_dir = Side::from_str(&r.expect(T_DIRECTION)?);
                r.expect(T_COLON)?;
                let rhs_id = r.expect(T_ID)?;
                let rhs_group = r.accept(T_ARROW_GROUP);
                ast.edges.push(EdgeAst {
                    lhs_id,
                    lhs_group,
                    lhs_dir,
                    lhs_into,
                    title,
                    rhs_into,
                    rhs_dir,
                    rhs_id,
                    rhs_group,
                });
            }
            T_ALIGN => {
                r.i += 1;
                if r.kind() != T_ROW && r.kind() != T_COLUMN {
                    return Err(r.fail("'row' or 'column'"));
                }
                let direction = if r.take() == "row" { AlignDir::Row } else { AlignDir::Column };
                let mut members = vec![r.expect(T_ID)?, r.expect(T_ID)?];
                while r.kind() == T_ID {
                    members.push(r.take());
                }
                ast.alignments.push(AlignmentAst { direction, members });
            }
            _ => {
                return Err(r.fail("a group, a service, a junction, an edge, an alignment, a title, or a line break"));
            }
        }
        end_of_line(&mut r)?;
    }
    Ok(ast)
}
