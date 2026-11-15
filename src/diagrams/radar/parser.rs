use crate::chars::is_word;
use crate::diagrams::common::tokens::{
    acc_descr_value, acc_title_value, slice_starts_with, string_value, title_before, title_value,
    tokenize, Pattern, Reader, TokenType, Tokens, ACC_DESCR, ACC_TITLE, DIRECTIVE, NEWLINE,
    SINGLE_LINE_COMMENT, STRING, WHITESPACE, YAML,
};
use crate::errors::Result;

use super::model::Graticule;

mod t {
    pub const RADAR_COLON: i32 = 0;
    pub const RADAR: i32 = 1;
    pub const SHOW_LEGEND: i32 = 2;
    pub const GRATICULE: i32 = 3;
    pub const CURVE: i32 = 4;
    pub const TICKS: i32 = 5;
    pub const AXIS: i32 = 6;
    pub const MAX: i32 = 7;
    pub const MIN: i32 = 8;
    pub const COLON: i32 = 9;
    pub const COMMA: i32 = 10;
    pub const OPEN: i32 = 11;
    pub const CLOSE: i32 = 12;
    pub const OPEN_BRACE: i32 = 13;
    pub const CLOSE_BRACE: i32 = 14;
    pub const GRATICULE_VALUE: i32 = 15;
    pub const BOOLEAN: i32 = 16;
    pub const ACC_DESCR: i32 = 17;
    pub const ACC_TITLE: i32 = 18;
    pub const TITLE: i32 = 19;
    pub const NUMBER: i32 = 20;
    pub const STRING: i32 = 21;
    pub const ID: i32 = 22;
    pub const NEWLINE: i32 = 23;
}

const LONGER_ID: &[usize] = &[t::ID as usize];

fn digits(src: &[char], at: usize) -> usize {
    let mut end = at;
    while end < src.len() && src[end].is_ascii_digit() {
        end += 1;
    }
    end
}

// The NUMBER pattern without its backtracking: /(?:[0-9]+\.[0-9]+(?!\.))|(?:0|[1-9][0-9]*(?!\.))/y
fn number_scanner() -> Box<dyn FnMut(&[char], usize, i32) -> Option<usize>> {
    let mut run_from = 0usize;
    let mut run_end = 0usize;
    Box::new(move |src: &[char], at: usize, _prev: i32| {
        if at < run_from || at >= run_end {
            run_from = at;
            run_end = digits(src, at);
        }
        let whole = run_end;
        if whole == at {
            return None;
        }
        let dot = src.get(whole) == Some(&'.');
        if dot {
            let fraction = digits(src, whole + 1);
            let length = fraction - whole - 1;
            if length > 0 && src.get(fraction) != Some(&'.') {
                return Some(fraction);
            }
            if length > 1 {
                return Some(fraction - 1);
            }
        }
        if src[at] == '0' {
            return Some(at + 1);
        }
        if !dot {
            return Some(whole);
        }
        if whole - 1 > at {
            Some(whole - 1)
        } else {
            None
        }
    })
}

// Matches /[-\w]*\w/y at `at`.
fn has_name_rest(src: &[char], at: usize) -> bool {
    let mut i = at;
    while i < src.len() && (is_word(src[i]) || src[i] == '-') {
        if is_word(src[i]) {
            return true;
        }
        i += 1;
    }
    false
}

// Matches /[\w]([-\w]*\w)?/y at `at`.
fn match_id(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !src.get(at).is_some_and(|&c| is_word(c)) {
        return None;
    }
    let mut end = at + 1;
    let mut i = at + 1;
    while i < src.len() && (is_word(src[i]) || src[i] == '-') {
        if is_word(src[i]) {
            end = i + 1;
        }
        i += 1;
    }
    Some(end)
}

fn match_graticule_value(src: &[char], at: usize, prev: i32) -> Option<usize> {
    let end = if slice_starts_with(src, at, "circle") {
        at + 6
    } else if slice_starts_with(src, at, "polygon") {
        at + 7
    } else {
        return None;
    };
    if prev == t::GRATICULE || !has_name_rest(src, end) {
        Some(end)
    } else {
        None
    }
}

fn match_boolean(src: &[char], at: usize, prev: i32) -> Option<usize> {
    let end = if slice_starts_with(src, at, "true") {
        at + 4
    } else if slice_starts_with(src, at, "false") {
        at + 5
    } else {
        return None;
    };
    if prev == t::SHOW_LEGEND || !has_name_rest(src, end) {
        Some(end)
    } else {
        None
    }
}

const fn word(text: &'static str) -> TokenType {
    TokenType::vis_longer(text, Pattern::Literal(text), LONGER_ID)
}

const fn mark(text: &'static str) -> TokenType {
    TokenType::vis(text, Pattern::Literal(text))
}

pub const RADAR_TOKENS: &[TokenType] = &[
    mark("radar-beta:"),
    TokenType::vis_longer("radar-beta", Pattern::Keyword("radar-beta"), LONGER_ID),
    word("showLegend"),
    word("graticule"),
    word("curve"),
    word("ticks"),
    word("axis"),
    word("max"),
    word("min"),
    mark(":"),
    mark(","),
    mark("["),
    mark("]"),
    mark("{"),
    mark("}"),
    TokenType::vis("GRATICULE", Pattern::Custom(match_graticule_value)),
    TokenType::vis("BOOLEAN", Pattern::Custom(match_boolean)),
    ACC_DESCR,
    ACC_TITLE,
    title_before(has_name_rest),
    TokenType::vis("NUMBER", Pattern::Scanner(number_scanner)),
    STRING,
    TokenType::vis("ID", Pattern::Custom(match_id)),
    NEWLINE,
    WHITESPACE,
    YAML,
    DIRECTIVE,
    SINGLE_LINE_COMMENT,
];

#[derive(Debug, Clone)]
pub struct AxisAst {
    pub name: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EntryAst {
    pub axis: Option<String>,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct CurveAst {
    pub name: String,
    pub label: Option<String>,
    pub entries: Vec<EntryAst>,
}

#[derive(Debug, Clone)]
pub enum OptionValue {
    ShowLegend(bool),
    Graticule(Graticule),
    Ticks(f64),
    Max(f64),
    Min(f64),
}

#[derive(Debug, Clone, Default)]
pub struct RadarAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub axes: Vec<AxisAst>,
    pub curves: Vec<CurveAst>,
    pub options: Vec<OptionValue>,
}

pub fn lex_radar(src: &[char]) -> Result<Tokens> {
    tokenize(src, RADAR_TOKENS, "radar")
}

fn parse_num(text: &str) -> f64 {
    text.parse::<f64>().unwrap_or(f64::NAN)
}

fn newlines(r: &mut Reader) {
    while r.kind() == t::NEWLINE {
        r.i += 1;
    }
}

fn end_of_line(r: &mut Reader) -> Result<()> {
    if r.kind() == -1 {
        return Ok(());
    }
    if r.kind() != t::NEWLINE {
        return Err(r.fail("a line break or the end of input"));
    }
    newlines(r);
    Ok(())
}

fn parse_label(r: &mut Reader) -> Result<Option<String>> {
    if !r.accept(t::OPEN) {
        return Ok(None);
    }
    let text = string_value(&r.expect(t::STRING)?);
    r.expect(t::CLOSE)?;
    Ok(Some(text))
}

fn parse_entry(r: &mut Reader, detailed: bool) -> Result<EntryAst> {
    if !detailed {
        let value = parse_num(&r.expect(t::NUMBER)?);
        return Ok(EntryAst { axis: None, value });
    }
    let axis = r.expect(t::ID)?;
    r.accept(t::COLON);
    let value = parse_num(&r.expect(t::NUMBER)?);
    Ok(EntryAst { axis: Some(axis), value })
}

fn parse_curve(r: &mut Reader) -> Result<CurveAst> {
    let name = r.expect(t::ID)?;
    let label = parse_label(r)?;
    r.expect(t::OPEN_BRACE)?;
    newlines(r);
    if r.kind() != t::NUMBER && r.kind() != t::ID {
        return Err(r.fail("a number or an axis name"));
    }
    let detailed = r.kind() == t::ID;
    let mut entries = vec![parse_entry(r, detailed)?];
    while r.accept(t::COMMA) {
        newlines(r);
        entries.push(parse_entry(r, detailed)?);
    }
    newlines(r);
    r.expect(t::CLOSE_BRACE)?;
    Ok(CurveAst { name, label, entries })
}

fn parse_axis(r: &mut Reader) -> Result<AxisAst> {
    let name = r.expect(t::ID)?;
    let label = parse_label(r)?;
    Ok(AxisAst { name, label })
}

fn is_option(kind: i32) -> bool {
    matches!(kind, t::SHOW_LEGEND | t::TICKS | t::MAX | t::MIN | t::GRATICULE)
}

fn parse_option(r: &mut Reader) -> Result<OptionValue> {
    let kind = r.kind();
    r.take();
    match kind {
        t::SHOW_LEGEND => Ok(OptionValue::ShowLegend(r.expect(t::BOOLEAN)? == "true")),
        t::GRATICULE => {
            let val = r.expect(t::GRATICULE_VALUE)?;
            Ok(OptionValue::Graticule(if val == "polygon" { Graticule::Polygon } else { Graticule::Circle }))
        }
        t::TICKS => Ok(OptionValue::Ticks(parse_num(&r.expect(t::NUMBER)?))),
        t::MAX => Ok(OptionValue::Max(parse_num(&r.expect(t::NUMBER)?))),
        _ => Ok(OptionValue::Min(parse_num(&r.expect(t::NUMBER)?))),
    }
}

pub fn parse_radar(src: &str) -> Result<RadarAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = lex_radar(&chars)?;
    let mut r = Reader::new(&chars, tokens, RADAR_TOKENS, "radar");
    let mut ast = RadarAst::default();

    newlines(&mut r);
    if !r.accept(t::RADAR_COLON) {
        r.expect(t::RADAR)?;
        r.accept(t::COLON);
    }
    newlines(&mut r);

    while r.kind() != -1 {
        match r.kind() {
            t::NEWLINE => r.i += 1,
            t::ACC_DESCR => {
                ast.acc_descr = Some(acc_descr_value(&r.take()));
                end_of_line(&mut r)?;
            }
            t::ACC_TITLE => {
                ast.acc_title = Some(acc_title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            t::TITLE => {
                ast.title = Some(title_value(&r.take()));
                end_of_line(&mut r)?;
            }
            t::AXIS => {
                r.i += 1;
                loop {
                    ast.axes.push(parse_axis(&mut r)?);
                    if !r.accept(t::COMMA) {
                        break;
                    }
                }
            }
            t::CURVE => {
                r.i += 1;
                loop {
                    ast.curves.push(parse_curve(&mut r)?);
                    if !r.accept(t::COMMA) {
                        break;
                    }
                }
            }
            _ => {
                if !is_option(r.kind()) {
                    return Err(r.fail("an axis, a curve, an option, a title, or a line break"));
                }
                ast.options.push(parse_option(&mut r)?);
                while r.accept(t::COMMA) {
                    if !is_option(r.kind()) {
                        return Err(r.fail("an option"));
                    }
                    ast.options.push(parse_option(&mut r)?);
                }
            }
        }
    }
    Ok(ast)
}
