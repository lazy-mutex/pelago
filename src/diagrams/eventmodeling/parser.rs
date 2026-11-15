use crate::chars::{is_line_end, is_space, is_word};
use crate::diagrams::common::tokens::{
    acc_descr, slice_starts_with, tokenize, Pattern, Reader, TokenType, ACC_TITLE, DIRECTIVE,
    SINGLE_LINE_COMMENT, YAML,
};
use crate::errors::Result;

mod t {
    pub const KEYWORD: i32 = 0;
    pub const RESETFRAME: i32 = 1;
    pub const READMODEL: i32 = 2;
    pub const PROCESSOR: i32 = 3;
    pub const TIMEFRAME: i32 = 4;
    pub const COMMAND: i32 = 5;
    pub const ENTITY: i32 = 6;
    pub const EVENT: i32 = 7;
    pub const JSOBJ: i32 = 8;
    pub const FIGMA: i32 = 9;
    pub const GIVEN: i32 = 10;
    pub const JSON: i32 = 11;
    pub const SALT: i32 = 12;
    pub const HTML: i32 = 13;
    pub const TEXT: i32 = 14;
    pub const DATA: i32 = 15;
    pub const NOTE: i32 = 16;
    pub const WHEN: i32 = 17;
    pub const THEN: i32 = 18;
    pub const RMO: i32 = 19;
    pub const CMD: i32 = 20;
    pub const EVT: i32 = 21;
    pub const PCR: i32 = 22;
    pub const URI: i32 = 23;
    pub const ARROW: i32 = 24;
    pub const GWT: i32 = 25;
    pub const UI: i32 = 26;
    pub const MD: i32 = 27;
    pub const TF: i32 = 28;
    pub const OPEN: i32 = 29;
    pub const CLOSE: i32 = 30;
    pub const RF: i32 = 31;
    pub const TICK: i32 = 32;
    pub const DOT: i32 = 33;
    pub const ID: i32 = 34;
    pub const FRAME_ID: i32 = 35;
    pub const INLINE: i32 = 36;
    pub const BLOCK: i32 = 37;
    pub const ACC_DESCR: i32 = 38;
    pub const ACC_TITLE: i32 = 39;
    pub const TITLE: i32 = 40;
}

const LONGER_ID: &[usize] = &[t::ID as usize];

const fn word(text: &'static str) -> TokenType {
    TokenType::vis_longer(text, Pattern::Literal(text), LONGER_ID)
}

const fn sign_tok(text: &'static str) -> TokenType {
    TokenType::vis(text, Pattern::Literal(text))
}

fn has_word_after(src: &[char], at: usize) -> bool {
    src.get(at).is_some_and(|&c| is_word(c))
}

// /[_a-zA-Z][\w_]*/y
fn match_em_id(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !src.get(at).is_some_and(|&c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let mut i = at + 1;
    while i < src.len() && is_word(src[i]) {
        i += 1;
    }
    Some(i)
}

// /\d{1,3}/y
fn match_em_fid(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let mut i = at;
    let limit = (at + 3).min(src.len());
    while i < limit && src[i].is_ascii_digit() {
        i += 1;
    }
    (i > at).then_some(i)
}

// /\{(.*)\}|"(.*)"|'(.*)'/y
fn match_em_inline(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let close_ch = match src.get(at)? {
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        _ => return None,
    };
    let mut line_end = at + 1;
    while line_end < src.len() && !is_line_end(src[line_end]) {
        line_end += 1;
    }
    for j in (at + 1..line_end).rev() {
        if src[j] == close_ch {
            return Some(j + 1);
        }
    }
    None
}

fn match_eol(src: &[char], at: usize) -> Option<usize> {
    match src.get(at) {
        Some('\r') if src.get(at + 1) == Some(&'\n') => Some(at + 2),
        Some('\n') => Some(at + 1),
        _ => None,
    }
}

fn end_after_brace(src: &[char], close_end: usize) -> Option<usize> {
    if let Some(after_close) = match_eol(src, close_end) {
        Some(after_close)
    } else if src.get(close_end).map_or(true, |&c| is_space(c)) {
        Some(close_end)
    } else {
        None
    }
}

fn block_opener(src: &[char], at: usize) -> bool {
    if src.get(at) != Some(&'{') {
        return false;
    }
    let mut i = at + 1;
    while i < src.len() && (src[i] == ' ' || src[i] == '\t') {
        i += 1;
    }
    match_eol(src, i).is_some()
}

// /\{[\t ]*\r?\n(?:[\S\s]*?\r?\n)?\}(?:\r?\n|(?!\S))/y
fn match_em_block(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if src.get(at) != Some(&'{') {
        return None;
    }
    let n = src.len();
    let mut i = at + 1;
    while i < n && (src[i] == ' ' || src[i] == '\t') {
        i += 1;
    }
    let after_open = match_eol(src, i)?;
    let mut j = after_open;
    while j < n {
        if let Some(after_nl) = match_eol(src, j) {
            if src.get(after_nl) == Some(&'}') {
                if let Some(end) = end_after_brace(src, after_nl + 1) {
                    return Some(end);
                }
            }
            j = after_nl;
        } else {
            j += 1;
        }
    }
    if src.get(after_open) == Some(&'}') {
        return end_after_brace(src, after_open + 1);
    }
    None
}

fn match_em_ws(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    let mut i = at;
    while i < src.len() && is_space(src[i]) {
        i += 1;
    }
    (i > at).then_some(i)
}

fn ml_comment_opener(src: &[char], at: usize) -> bool {
    slice_starts_with(src, at, "/*")
}

fn match_em_ml_comment(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !slice_starts_with(src, at, "/*") {
        return None;
    }
    let mut i = at + 2;
    while i + 1 < src.len() {
        if src[i] == '*' && src[i + 1] == '/' {
            return Some(i + 2);
        }
        i += 1;
    }
    None
}

fn match_em_sl_comment(src: &[char], at: usize, _prev: i32) -> Option<usize> {
    if !slice_starts_with(src, at, "//") {
        return None;
    }
    let mut i = at + 2;
    while i < src.len() && src[i] != '\n' && src[i] != '\r' {
        i += 1;
    }
    Some(i)
}

pub const EVENTMODELING_TOKENS: &[TokenType] = &[
    TokenType::vis_longer("eventmodeling", Pattern::Keyword("eventmodeling"), LONGER_ID),
    word("resetframe"),
    word("readmodel"),
    word("processor"),
    word("timeframe"),
    word("command"),
    word("entity"),
    word("event"),
    word("jsobj"),
    word("figma"),
    word("given"),
    word("json"),
    word("salt"),
    word("html"),
    word("text"),
    word("data"),
    word("note"),
    word("when"),
    word("then"),
    word("rmo"),
    word("cmd"),
    word("evt"),
    word("pcr"),
    word("uri"),
    sign_tok("->>"),
    word("gwt"),
    word("ui"),
    word("md"),
    word("tf"),
    sign_tok("[["),
    sign_tok("]]"),
    word("rf"),
    sign_tok("`"),
    sign_tok("."),
    TokenType::vis("EM_ID", Pattern::Custom(match_em_id)),
    TokenType::vis("EM_FID", Pattern::Custom(match_em_fid)),
    TokenType::vis("EM_DATA_INLINE", Pattern::Custom(match_em_inline)),
    TokenType::vis("EM_DATA_BLOCK", Pattern::CustomWithOpener(match_em_block, block_opener)),
    acc_descr("EM_ACC_DESCR"),
    TokenType::vis("EM_ACC_TITLE", ACC_TITLE.pattern),
    TokenType::vis("EM_TITLE", Pattern::TitleBefore(has_word_after)),
    TokenType::hid("EM_WS", Pattern::Custom(match_em_ws)),
    TokenType::hid("EM_YAML", YAML.pattern),
    TokenType::hid("EM_DIRECTIVE", DIRECTIVE.pattern),
    TokenType::hid("EM_SINGLE_LINE_COMMENT", SINGLE_LINE_COMMENT.pattern),
    TokenType::hid("EM_ML_COMMENT", Pattern::CustomWithOpener(match_em_ml_comment, ml_comment_opener)),
    TokenType::hid("EM_SL_COMMENT", Pattern::Custom(match_em_sl_comment)),
];

fn is_time_frame(k: i32) -> bool {
    k == t::TF || k == t::TIMEFRAME
}

fn is_reset_frame(k: i32) -> bool {
    k == t::RF || k == t::RESETFRAME
}

fn is_entity_type(k: i32) -> bool {
    matches!(
        k,
        t::RMO | t::READMODEL | t::UI | t::CMD | t::COMMAND | t::EVT | t::EVENT | t::PCR | t::PROCESSOR
    )
}

fn is_data_type(k: i32) -> bool {
    matches!(
        k,
        t::JSON | t::JSOBJ | t::FIGMA | t::SALT | t::URI | t::MD | t::HTML | t::TEXT
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Time,
    Reset,
}

#[derive(Debug, Clone)]
pub struct EmFrame {
    pub kind: FrameKind,
    pub name: String,
    pub model_entity_type: String,
    pub entity_identifier: String,
    pub source_frames: Vec<String>,
    pub data_reference: Option<String>,
    pub data_type: Option<String>,
    pub data_inline_value: Option<String>,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct EmDataEntity {
    pub name: String,
    pub data_type: Option<String>,
    pub data_block_value: String,
}

#[derive(Debug, Clone)]
pub struct EmNoteEntity {
    pub source_frame: String,
    pub data_type: Option<String>,
    pub data_block_value: String,
}

#[derive(Debug, Clone)]
pub struct EmGwt {
    pub source_frame: String,
    pub given_statements: Vec<String>,
    pub when_statements: Vec<String>,
    pub then_statements: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct EventModelAst {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub model_entities: Vec<String>,
    pub frames: Vec<EmFrame>,
    pub data_entities: Vec<EmDataEntity>,
    pub note_entities: Vec<EmNoteEntity>,
    pub gwt_entities: Vec<EmGwt>,
    pub source: String,
}

fn expect_pred(r: &mut Reader, pred: fn(i32) -> bool, expected: &str) -> Result<String> {
    if pred(r.kind()) {
        Ok(r.take())
    } else {
        Err(r.fail(expected))
    }
}

fn qualified_name(r: &mut Reader) -> Result<String> {
    let mut name = r.expect(t::ID)?;
    while r.accept(t::DOT) {
        name.push('.');
        name.push_str(&r.expect(t::ID)?);
    }
    Ok(name)
}

fn parse_data_type(r: &mut Reader) -> Result<Option<String>> {
    if !r.accept(t::TICK) {
        return Ok(None);
    }
    let dt = expect_pred(r, is_data_type, "a data type")?;
    r.expect(t::TICK)?;
    Ok(Some(dt))
}

fn parse_statements(r: &mut Reader) -> Result<Vec<String>> {
    let mut out = Vec::new();
    loop {
        expect_pred(r, is_entity_type, "an entity type")?;
        out.push(r.expect(t::ID)?);
        if !is_entity_type(r.kind()) {
            break;
        }
    }
    Ok(out)
}

pub fn parse_event_model(src: &str) -> Result<EventModelAst> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars, EVENTMODELING_TOKENS, "eventmodeling")?;
    let mut r = Reader::new(&chars, tokens, EVENTMODELING_TOKENS, "eventmodeling");
    let mut ast = EventModelAst {
        source: src.to_string(),
        ..Default::default()
    };

    r.expect(t::KEYWORD)?;
    while r.kind() != -1 {
        let kind = r.kind();
        if kind == t::ACC_DESCR {
            ast.acc_descr = Some(r.take());
        } else if kind == t::ACC_TITLE {
            ast.acc_title = Some(r.take());
        } else if kind == t::TITLE {
            ast.title = Some(r.take());
        } else if kind == t::ENTITY {
            r.i += 1;
            ast.model_entities.push(qualified_name(&mut r)?);
        } else if is_time_frame(kind) || is_reset_frame(kind) {
            let offset = r.tokens.starts[r.i];
            let fkind = if is_time_frame(kind) { FrameKind::Time } else { FrameKind::Reset };
            r.i += 1;
            let name = r.expect(t::FRAME_ID)?;
            let model_entity_type = expect_pred(&mut r, is_entity_type, "an entity type")?;
            let entity_identifier = qualified_name(&mut r)?;
            let mut source_frames = Vec::new();
            while r.accept(t::ARROW) {
                source_frames.push(r.expect(t::FRAME_ID)?);
            }
            let data_reference = if r.accept(t::OPEN) {
                let dr = r.expect(t::ID)?;
                r.expect(t::CLOSE)?;
                Some(dr)
            } else {
                None
            };
            let (data_type, data_inline_value) = if r.kind() == t::TICK || r.kind() == t::INLINE {
                let dt = parse_data_type(&mut r)?;
                let inline = r.expect(t::INLINE)?;
                (dt, Some(inline))
            } else {
                (None, None)
            };
            ast.frames.push(EmFrame {
                kind: fkind,
                name,
                model_entity_type,
                entity_identifier,
                source_frames,
                data_reference,
                data_type,
                data_inline_value,
                offset,
            });
        } else if kind == t::DATA {
            r.i += 1;
            let name = r.expect(t::ID)?;
            let data_type = parse_data_type(&mut r)?;
            let data_block_value = r.expect(t::BLOCK)?;
            ast.data_entities.push(EmDataEntity { name, data_type, data_block_value });
        } else if kind == t::NOTE {
            r.i += 1;
            let source_frame = r.expect(t::FRAME_ID)?;
            let data_type = parse_data_type(&mut r)?;
            let data_block_value = r.expect(t::BLOCK)?;
            ast.note_entities.push(EmNoteEntity { source_frame, data_type, data_block_value });
        } else if kind == t::GWT {
            r.i += 1;
            let source_frame = r.expect(t::FRAME_ID)?;
            r.expect(t::GIVEN)?;
            let given_statements = parse_statements(&mut r)?;
            let when_statements = if r.accept(t::WHEN) {
                parse_statements(&mut r)?
            } else {
                Vec::new()
            };
            r.expect(t::THEN)?;
            let then_statements = parse_statements(&mut r)?;
            ast.gwt_entities.push(EmGwt {
                source_frame,
                given_statements,
                when_statements,
                then_statements,
            });
        } else {
            return Err(r.fail("a frame, an entity, a data block, a note, or a specification"));
        }
    }
    Ok(ast)
}
