use crate::chars::{is_space, is_word};
use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, slice_starts_with, string_value, title_value, tokenize,
    Pattern, Reader, TokenType, ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE, SINGLE_LINE_COMMENT,
    STRING, TITLE, WHITESPACE, YAML,
};
use crate::errors::Result;

const T_LBRACKET: i32 = 0;
const T_COMMA: i32 = 1;
const T_RBRACKET: i32 = 2;
const T_AT: i32 = 3;
const T_SLASH: i32 = 4;
const T_LPAREN: i32 = 5;
const T_RPAREN: i32 = 6;
const T_MINUS: i32 = 7;
const T_LBRACE: i32 = 8;
const T_RBRACE: i32 = 9;
const T_NUMBER: i32 = 10;
const T_ARROW: i32 = 11;
const T_LINK_PORT: i32 = 12;
const T_LINK_ARROW: i32 = 13;
const T_LINK_LABEL: i32 = 14;
const T_STRATEGY: i32 = 15;
const T_WARDLEY: i32 = 16;
const T_SIZE: i32 = 17;
const T_EVOLUTION: i32 = 18;
const T_ANCHOR: i32 = 19;
const T_COMPONENT: i32 = 20;
const T_LABEL: i32 = 21;
const T_INERTIA: i32 = 22;
const T_EVOLVE: i32 = 23;
const T_PIPELINE: i32 = 24;
const T_NOTE: i32 = 25;
const T_ANNOTATIONS: i32 = 26;
const T_ANNOTATION: i32 = 27;
const T_ACCELERATOR: i32 = 28;
const T_DEACCELERATOR: i32 = 29;
const T_NAME: i32 = 30;
// 31: WS (hidden)
const T_ACC_DESCR: i32 = 32;
const T_ACC_TITLE: i32 = 33;
const T_TITLE: i32 = 34;
const T_INT: i32 = 35;
const T_STRING: i32 = 36;
const T_ID: i32 = 37;
const T_NEWLINE: i32 = 38;
// 39: WHITESPACE, 40: YAML, 41: DIRECTIVE, 42: SINGLE_LINE_COMMENT

fn scan_digits(src: &[char], mut i: usize) -> usize {
    while i < src.len() && src[i].is_ascii_digit() {
        i += 1;
    }
    i
}

// Answers as WARDLEY_NUMBER's pattern `/[0-9]+\.[0-9]+/y` does, remembering scanned digit runs.
fn number_scanner() -> Box<dyn FnMut(&[char], usize, i32) -> Option<usize>> {
    let mut from = 0usize;
    let mut end = 0usize;
    Box::new(move |src, p, _prev| {
        if p < from || p >= end {
            from = p;
            end = scan_digits(src, p);
        }
        if end == p || src.get(end) != Some(&'.') {
            return None;
        }
        let stop = scan_digits(src, end + 1);
        (stop > end + 1).then_some(stop)
    })
}

// `/\+<>|\+>|\+</y`
fn match_link_port(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if src.get(at) != Some(&'+') {
        return None;
    }
    if slice_starts_with(src, at + 1, "<>") {
        Some(at + 3)
    } else if matches!(src.get(at + 1), Some('>') | Some('<')) {
        Some(at + 2)
    } else {
        None
    }
}

// `/-->|-\.->|>|\+'[^']*'<>|\+'[^']*'<|\+'[^']*'>/y`
fn match_link_arrow(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if slice_starts_with(src, at, "-->") {
        return Some(at + 3);
    }
    if slice_starts_with(src, at, "-.->") {
        return Some(at + 4);
    }
    if src.get(at) == Some(&'>') {
        return Some(at + 1);
    }
    if src.get(at) == Some(&'+') && src.get(at + 1) == Some(&'\'') {
        let mut j = at + 2;
        while j < src.len() && src[j] != '\'' {
            j += 1;
        }
        if j < src.len() {
            if slice_starts_with(src, j + 1, "<>") {
                return Some(j + 3);
            }
            if matches!(src.get(j + 1), Some('<') | Some('>')) {
                return Some(j + 2);
            }
        }
    }
    None
}

// `/;[^\n\r]+/y`
fn match_link_label(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if src.get(at) != Some(&';') {
        return None;
    }
    let mut j = at + 1;
    while j < src.len() && src[j] != '\n' && src[j] != '\r' {
        j += 1;
    }
    (j > at + 1).then_some(j)
}

// `/build|buy|outsource|market/y`
fn match_strategy(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    for kw in ["build", "buy", "outsource", "market"] {
        if slice_starts_with(src, at, kw) {
            return Some(at + kw.len());
        }
    }
    None
}

fn scan_name_tail(src: &[char], mut i: usize) -> usize {
    while i < src.len() {
        let c = src[i];
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '(' | ')' | '&') {
            i += 1;
        } else if c == '-' && src.get(i + 1) != Some(&'>') {
            i += 1;
        } else {
            break;
        }
    }
    i
}

// `/(?!title\s|accTitle|accDescr)[A-Za-z](?:[A-Za-z0-9_()&]|-(?!>))*(?:[ \t]+[A-Za-z(](?:[A-Za-z0-9_()&]|-(?!>))*)*/y`
fn match_name_with_spaces(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let &first = src.get(at)?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if slice_starts_with(src, at, "title") && src.get(at + 5).map_or(false, |&c| is_space(c)) {
        return None;
    }
    if slice_starts_with(src, at, "accTitle") || slice_starts_with(src, at, "accDescr") {
        return None;
    }
    let mut end = scan_name_tail(src, at + 1);
    loop {
        let mut ws = end;
        while ws < src.len() && (src[ws] == ' ' || src[ws] == '\t') {
            ws += 1;
        }
        if ws == end {
            break;
        }
        match src.get(ws) {
            Some(&c) if c.is_ascii_alphabetic() || c == '(' => {
                end = scan_name_tail(src, ws + 1);
            }
            _ => break,
        }
    }
    Some(end)
}

// `/0|[1-9][0-9]*(?!\.)/y`
fn match_int(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    match src.get(at) {
        Some('0') => Some(at + 1),
        Some('1'..='9') => {
            let mut end = scan_digits(src, at + 1);
            if src.get(end) == Some(&'.') {
                end -= 1;
            }
            (end > at).then_some(end)
        }
        _ => None,
    }
}

// `/[\w]([-\w]*\w)?/y`
fn match_id(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let &first = src.get(at)?;
    if !is_word(first) {
        return None;
    }
    let mut end = at + 1;
    while end < src.len() && (is_word(src[end]) || src[end] == '-') {
        end += 1;
    }
    while end > at + 1 && src[end - 1] == '-' {
        end -= 1;
    }
    Some(end)
}

const fn word(name: &'static str, text: &'static str) -> TokenType {
    TokenType::vis(name, Pattern::Literal(text))
}

pub const WARDLEY_TOKENS: &[TokenType] = &[
    TokenType::vis("[", Pattern::Literal("[")),                              // 0
    TokenType::vis(",", Pattern::Literal(",")),                              // 1
    TokenType::vis("]", Pattern::Literal("]")),                              // 2
    TokenType::vis("@", Pattern::Literal("@")),                              // 3
    TokenType::vis("/", Pattern::Literal("/")),                              // 4
    TokenType::vis("(", Pattern::Literal("(")),                              // 5
    TokenType::vis(")", Pattern::Literal(")")),                              // 6
    TokenType::vis_longer("-", Pattern::Literal("-"), &[11, 13, 40]),        // 7
    TokenType::vis("{", Pattern::Literal("{")),                              // 8
    TokenType::vis("}", Pattern::Literal("}")),                              // 9
    TokenType::vis("WARDLEY_NUMBER", Pattern::Scanner(number_scanner)),      // 10
    TokenType::vis("ARROW", Pattern::Literal("->")),                         // 11
    TokenType::vis("LINK_PORT", Pattern::Custom(match_link_port)),           // 12
    TokenType::vis("LINK_ARROW", Pattern::Custom(match_link_arrow)),         // 13
    TokenType::vis("LINK_LABEL", Pattern::Custom(match_link_label)),         // 14
    TokenType::vis("STRATEGY", Pattern::Custom(match_strategy)),             // 15
    word("KW_WARDLEY", "wardley-beta"),                                      // 16
    word("KW_SIZE", "size"),                                                 // 17
    word("KW_EVOLUTION", "evolution"),                                       // 18
    word("KW_ANCHOR", "anchor"),                                             // 19
    word("KW_COMPONENT", "component"),                                       // 20
    word("KW_LABEL", "label"),                                               // 21
    word("KW_INERTIA", "inertia"),                                           // 22
    word("KW_EVOLVE", "evolve"),                                             // 23
    word("KW_PIPELINE", "pipeline"),                                         // 24
    word("KW_NOTE", "note"),                                                 // 25
    word("KW_ANNOTATIONS", "annotations"),                                   // 26
    word("KW_ANNOTATION", "annotation"),                                     // 27
    word("KW_ACCELERATOR", "accelerator"),                                   // 28
    word("KW_DEACCELERATOR", "deaccelerator"),                               // 29
    TokenType::vis("NAME_WITH_SPACES", Pattern::Custom(match_name_with_spaces)), // 30
    TokenType::hid("WS", Pattern::Whitespace),                               // 31
    ACC_DESCR,                                                               // 32
    ACC_TITLE,                                                               // 33
    TITLE,                                                                   // 34
    TokenType::vis("INT", Pattern::Custom(match_int)),                       // 35
    STRING,                                                                  // 36
    TokenType::vis("ID", Pattern::Custom(match_id)),                         // 37
    NEWLINE,                                                                 // 38
    WHITESPACE,                                                              // 39
    YAML,                                                                    // 40
    DIRECTIVE,                                                               // 41
    SINGLE_LINE_COMMENT,                                                     // 42
];

#[derive(Debug, Clone)]
pub struct LabelAst {
    pub neg_x: bool,
    pub offset_x: f64,
    pub neg_y: bool,
    pub offset_y: f64,
}

#[derive(Debug, Clone)]
pub struct StageAst {
    pub name: String,
    pub boundary: Option<f64>,
    pub second_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ComponentAst {
    pub name: String,
    pub visibility: f64,
    pub evolution: f64,
    pub label: Option<LabelAst>,
    pub decorator_strategy: Option<String>,
    pub inertia: bool,
}

#[derive(Debug, Clone)]
pub struct PipelineComponentAst {
    pub name: String,
    pub evolution: f64,
    pub label: Option<LabelAst>,
}

#[derive(Debug, Clone)]
pub struct LinkAst {
    pub from: String,
    pub from_port: Option<String>,
    pub arrow: Option<String>,
    pub to: String,
    pub to_port: Option<String>,
    pub link_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnchorAst {
    pub name: String,
    pub visibility: f64,
    pub evolution: f64,
}

#[derive(Debug, Clone)]
pub struct EvolveAst {
    pub component: String,
    pub target: f64,
}

#[derive(Debug, Clone)]
pub struct PipelineAst {
    pub parent: String,
    pub components: Vec<PipelineComponentAst>,
}

#[derive(Debug, Clone)]
pub struct NoteAst {
    pub text: String,
    pub visibility: f64,
    pub evolution: f64,
}

#[derive(Debug, Clone)]
pub struct AnnotationsBoxAst {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct AnnotationAst {
    pub number: f64,
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ForceAst {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct SizeAst {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default)]
pub struct WardleyAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub size: Option<SizeAst>,
    pub evolution_stages: Option<Vec<StageAst>>,
    pub anchors: Vec<AnchorAst>,
    pub components: Vec<ComponentAst>,
    pub links: Vec<LinkAst>,
    pub evolves: Vec<EvolveAst>,
    pub pipelines: Vec<PipelineAst>,
    pub notes: Vec<NoteAst>,
    pub annotations: Vec<AnnotationsBoxAst>,
    pub annotation: Vec<AnnotationAst>,
    pub accelerators: Vec<ForceAst>,
    pub deaccelerators: Vec<ForceAst>,
}

fn read_name(r: &mut Reader) -> Result<String> {
    let k = r.kind();
    if k == T_STRING {
        Ok(string_value(&r.take()))
    } else if k == T_ID || k == T_NAME {
        Ok(r.take())
    } else {
        Err(r.fail("a name"))
    }
}

fn read_number(r: &mut Reader) -> Result<f64> {
    let text = r.expect(T_NUMBER)?;
    Ok(text.parse::<f64>().unwrap_or(f64::NAN))
}

fn read_int(r: &mut Reader) -> Result<f64> {
    let text = r.expect(T_INT)?;
    Ok(text.parse::<f64>().unwrap_or(f64::NAN))
}

fn read_coordinate(r: &mut Reader) -> Result<f64> {
    if r.kind() == T_NUMBER {
        let text = r.take();
        Ok(text.parse::<f64>().unwrap_or(f64::NAN))
    } else {
        read_int(r)
    }
}

fn end_of_line(r: &mut Reader) -> Result<()> {
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

fn read_pair<F>(r: &mut Reader, mut read: F) -> Result<(f64, f64)>
where
    F: FnMut(&mut Reader) -> Result<f64>,
{
    r.expect(T_LBRACKET)?;
    let first = read(r)?;
    r.expect(T_COMMA)?;
    let second = read(r)?;
    r.expect(T_RBRACKET)?;
    Ok((first, second))
}

fn read_label(r: &mut Reader) -> Result<LabelAst> {
    r.expect(T_LABEL)?;
    r.expect(T_LBRACKET)?;
    let neg_x = r.accept(T_MINUS);
    let offset_x = read_int(r)?;
    r.expect(T_COMMA)?;
    let neg_y = r.accept(T_MINUS);
    let offset_y = read_int(r)?;
    r.expect(T_RBRACKET)?;
    Ok(LabelAst { neg_x, offset_x, neg_y, offset_y })
}

fn read_stage(r: &mut Reader) -> Result<StageAst> {
    let name = read_name(r)?;
    let boundary = if r.accept(T_AT) { Some(read_number(r)?) } else { None };
    let second_name = if r.accept(T_SLASH) { Some(read_name(r)?) } else { None };
    Ok(StageAst { name, boundary, second_name })
}

pub fn parse_wardley(src: &str) -> Result<WardleyAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, WARDLEY_TOKENS, "wardley")?;
    let mut r = Reader::new(&chars, tokens, WARDLEY_TOKENS, "wardley");
    let mut ast = WardleyAst::default();

    while r.kind() == T_NEWLINE {
        r.i += 1;
    }
    r.expect(T_WARDLEY)?;

    while r.kind() != -1 {
        match r.kind() {
            T_NEWLINE => {
                r.i += 1;
            }
            T_ACC_DESCR => {
                let raw = r.take();
                ast.acc_descr = Some(acc_descr_value(&raw));
                end_of_line(&mut r)?;
            }
            T_ACC_TITLE => {
                let raw = r.take();
                ast.acc_title = Some(acc_title_value(&raw));
                end_of_line(&mut r)?;
            }
            T_TITLE => {
                let raw = r.take();
                ast.title = Some(title_value(&raw));
                end_of_line(&mut r)?;
            }
            T_SIZE => {
                r.i += 1;
                let (width, height) = read_pair(&mut r, read_int)?;
                end_of_line(&mut r)?;
                ast.size = Some(SizeAst { width, height });
            }
            T_EVOLUTION => {
                r.i += 1;
                let mut stages = vec![read_stage(&mut r)?];
                r.expect(T_ARROW)?;
                stages.push(read_stage(&mut r)?);
                while r.accept(T_ARROW) {
                    stages.push(read_stage(&mut r)?);
                }
                end_of_line(&mut r)?;
                ast.evolution_stages = Some(stages);
            }
            T_ANCHOR => {
                r.i += 1;
                let anchor = read_name(&mut r)?;
                let (visibility, evolution) = read_pair(&mut r, read_number)?;
                end_of_line(&mut r)?;
                ast.anchors.push(AnchorAst { name: anchor, visibility, evolution });
            }
            T_COMPONENT => {
                r.i += 1;
                let name = read_name(&mut r)?;
                let (visibility, evolution) = read_pair(&mut r, read_number)?;
                let label = if r.kind() == T_LABEL { Some(read_label(&mut r)?) } else { None };
                let decorator_strategy = if r.kind() == T_LPAREN && r.kind_at(r.i + 1) == T_STRATEGY {
                    r.i += 1;
                    let strat = r.take();
                    r.expect(T_RPAREN)?;
                    Some(strat)
                } else {
                    None
                };
                let inertia = if r.accept(T_INERTIA) {
                    true
                } else if r.accept(T_LPAREN) {
                    r.expect(T_INERTIA)?;
                    r.expect(T_RPAREN)?;
                    true
                } else {
                    false
                };
                end_of_line(&mut r)?;
                ast.components.push(ComponentAst {
                    name,
                    visibility,
                    evolution,
                    label,
                    decorator_strategy,
                    inertia,
                });
            }
            T_EVOLVE => {
                r.i += 1;
                let component = read_name(&mut r)?;
                let target = read_number(&mut r)?;
                end_of_line(&mut r)?;
                ast.evolves.push(EvolveAst { component, target });
            }
            T_PIPELINE => {
                r.i += 1;
                let parent = read_name(&mut r)?;
                r.expect(T_LBRACE)?;
                r.expect(T_NEWLINE)?;
                while r.kind() == T_NEWLINE {
                    r.i += 1;
                }
                let mut components = Vec::new();
                loop {
                    r.expect(T_COMPONENT)?;
                    let name = read_name(&mut r)?;
                    r.expect(T_LBRACKET)?;
                    let evolution = read_number(&mut r)?;
                    r.expect(T_RBRACKET)?;
                    let label = if r.kind() == T_LABEL { Some(read_label(&mut r)?) } else { None };
                    end_of_line(&mut r)?;
                    components.push(PipelineComponentAst { name, evolution, label });
                    if r.kind() != T_COMPONENT {
                        break;
                    }
                }
                r.expect(T_RBRACE)?;
                end_of_line(&mut r)?;
                ast.pipelines.push(PipelineAst { parent, components });
            }
            T_NOTE => {
                r.i += 1;
                let raw = r.expect(T_STRING)?;
                let text = string_value(&raw);
                let (visibility, evolution) = read_pair(&mut r, read_number)?;
                end_of_line(&mut r)?;
                ast.notes.push(NoteAst { text, visibility, evolution });
            }
            T_ANNOTATIONS => {
                r.i += 1;
                let (x, y) = read_pair(&mut r, read_coordinate)?;
                end_of_line(&mut r)?;
                ast.annotations.push(AnnotationsBoxAst { x, y });
            }
            T_ANNOTATION => {
                r.i += 1;
                let number = read_int(&mut r)?;
                r.expect(T_COMMA)?;
                let (x, y) = read_pair(&mut r, read_coordinate)?;
                let raw = r.expect(T_STRING)?;
                let text = string_value(&raw);
                end_of_line(&mut r)?;
                ast.annotation.push(AnnotationAst { number, x, y, text });
            }
            T_ACCELERATOR | T_DEACCELERATOR => {
                let slow = r.kind() == T_DEACCELERATOR;
                r.i += 1;
                let force = read_name(&mut r)?;
                let (x, y) = read_pair(&mut r, read_number)?;
                end_of_line(&mut r)?;
                if slow {
                    ast.deaccelerators.push(ForceAst { name: force, x, y });
                } else {
                    ast.accelerators.push(ForceAst { name: force, x, y });
                }
            }
            _ => {
                let is_name = matches!(r.kind(), T_STRING | T_ID | T_NAME);
                if !is_name {
                    return Err(r.fail("a statement"));
                }
                let from = read_name(&mut r)?;
                let from_port = if r.kind() == T_LINK_PORT { Some(r.take()) } else { None };
                let arrow = if r.kind() == T_LINK_ARROW || r.kind() == T_ARROW {
                    Some(r.take())
                } else {
                    None
                };
                let to = read_name(&mut r)?;
                let to_port = if r.kind() == T_LINK_PORT { Some(r.take()) } else { None };
                let link_label = if r.kind() == T_LINK_LABEL {
                    let raw = r.take();
                    Some(raw[1..].trim_matches(is_space).to_string())
                } else {
                    None
                };
                end_of_line(&mut r)?;
                ast.links.push(LinkAst { from, from_port, arrow, to, to_port, link_label });
            }
        }
    }
    Ok(ast)
}
