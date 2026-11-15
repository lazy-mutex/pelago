use super::db::AgentflowDb;
use super::lexer::{tokenize, Tokens};
use super::tokens::{tok, TOKEN_NAMES};
use super::types::{DocItem, FlowText, LabelType, LinkInfo, Loc};
use crate::chars::is_space;
use crate::errors::{input_offset, syntax_error, Edit, PeleError, Result};

fn is_id(t: u8) -> bool {
    matches!(
        t,
        tok::NUM
            | tok::NODE_STRING
            | tok::DOWN
            | tok::MINUS
            | tok::DEFAULT
            | tok::COMMA
            | tok::COLON
            | tok::AMP
            | tok::BRKT
            | tok::MULT
            | tok::UNICODE_TEXT
    )
}

fn is_alphanum(t: u8) -> bool {
    matches!(
        t,
        tok::NUM
            | tok::UNICODE_TEXT
            | tok::NODE_STRING
            | tok::DIR
            | tok::DOWN
            | tok::MINUS
            | tok::COMMA
            | tok::COLON
            | tok::AMP
            | tok::BRKT
            | tok::MULT
    )
}

fn is_no_tags(t: u8) -> bool {
    matches!(
        t,
        tok::NUM
            | tok::NODE_STRING
            | tok::SPACE
            | tok::MINUS
            | tok::AMP
            | tok::UNICODE_TEXT
            | tok::COLON
            | tok::MULT
            | tok::BRKT
            | tok::STYLE
            | tok::LINKSTYLE
            | tok::CLASSDEF
            | tok::CLASS
            | tok::CLICK
            | tok::GRAPH
            | tok::DIR
            | tok::FLOW
            | tok::CONNECTOR
            | tok::GLOBAL
            | tok::END_KW
            | tok::DOWN
            | tok::UP
            | tok::START_LINK
    )
}

fn is_style_part(t: u8) -> bool {
    matches!(
        t,
        tok::NUM | tok::NODE_STRING | tok::COLON | tok::SPACE | tok::BRKT | tok::STYLE
    )
}

fn is_separator(t: u8) -> bool {
    matches!(t, tok::NEWLINE | tok::SEMI | tok::EOF | tok::COMMENT)
}

fn is_trailing(t: u8) -> bool {
    matches!(t, tok::COMMENT | tok::NEWLINE | tok::SPACE | tok::EOF)
}

fn is_text_tok(t: u8) -> bool {
    matches!(
        t,
        tok::TEXT | tok::TAGSTART | tok::TAGEND | tok::UNICODE_TEXT
    )
}

struct Frame {
    global: bool,
    // Token index of the `flow` or `connector` keyword.
    at: usize,
    id: Option<FlowText>,
    title: Option<FlowText>,
    data: Option<String>,
    data_from: usize,
    data_to: usize,
    list: Vec<DocItem>,
    collect: bool,
}

const TEXT_TOKENS: &str = "'UNICODE_TEXT', 'TEXT', 'TAGSTART'";
const SEPARATORS: &str = "'COMMENT', 'SEMI', 'NEWLINE', 'EOF'";
const DIRECTIONS: [&str; 5] = ["TB", "BT", "RL", "LR", "TD"];
const STATEMENT_START: &str = "'COMMENT', 'SEMI', 'NEWLINE', 'SPACE', 'EOF', 'flow', 'global', 'connector', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'STYLE', 'LINKSTYLE', 'CLASSDEF', 'CLASS', 'CLICK', 'NODE_STRING'";

// Mermaid trims spaces and tabs between a closing brace and the end of its line before parsing: `/}[^\S\n]*\n/g`.
fn strip_brace_trails(orig: &[char]) -> (String, Vec<char>, Vec<Edit>) {
    let mut out_str = String::with_capacity(orig.len());
    let mut out_chars = Vec::with_capacity(orig.len());
    let mut edits = Vec::new();
    let mut i = 0;
    while i < orig.len() {
        if orig[i] == '}' {
            let mut m = i + 1;
            while m < orig.len() && is_space(orig[m]) && orig[m] != '\n' {
                m += 1;
            }
            if m < orig.len() && orig[m] == '\n' {
                if m > i + 1 {
                    edits.push((i, m + 1, 2));
                }
                out_str.push('}');
                out_str.push('\n');
                out_chars.push('}');
                out_chars.push('\n');
                i = m + 1;
                continue;
            }
        }
        out_str.push(orig[i]);
        out_chars.push(orig[i]);
        i += 1;
    }
    (out_str, out_chars, edits)
}

struct Parser<'a> {
    orig_src: &'a [char],
    src: &'a [char],
    edits: Vec<Edit>,
    types: Vec<u8>,
    texts: Vec<String>,
    starts: Vec<usize>,
    locs: Vec<usize>,
    i: usize,
    pending_data: Option<String>,
    data_from: usize,
    data_to: usize,
    db: &'a mut AgentflowDb,
}

impl Parser<'_> {
    fn t(&self) -> u8 {
        self.types[self.i.min(self.types.len() - 1)]
    }

    fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.texts[self.i]);
        self.i += 1;
        text
    }

    fn fail(&self, expected: &str) -> PeleError {
        let idx = self.i.min(self.types.len() - 1);
        let t = self.types[idx];
        let mut from = self.starts[idx];
        if t == tok::EOF || t == tok::END {
            // Comments are tokens here, so skip trailing ones.
            let mut k = idx;
            while k > 0 && is_trailing(self.types[k - 1]) {
                k -= 1;
            }
            let mut end = self.starts[k];
            while end > 0 && is_space(self.src[end - 1]) {
                end -= 1;
            }
            from = end;
        }
        let at = if self.edits.is_empty() {
            from
        } else {
            input_offset(std::slice::from_ref(&self.edits), from)
        };
        if t == tok::ERROR {
            return syntax_error("agentflow", self.orig_src, at, "", true);
        }
        syntax_error(
            "agentflow",
            self.orig_src,
            at,
            &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[t as usize]),
            false,
        )
    }

    // The span from the start of token `a` to the end of token `b`.
    fn span(&self, a: usize, b: usize) -> Loc {
        Loc {
            first_line: self.locs[4 * a],
            first_column: self.locs[4 * a + 1],
            last_line: self.locs[4 * b + 2],
            last_column: self.locs[4 * b + 3],
        }
    }

    fn expect(&mut self, kind: u8, also: &str) -> Result<String> {
        if self.t() != kind {
            let name = TOKEN_NAMES[kind as usize];
            let msg = if also.is_empty() {
                format!("'{}'", name)
            } else {
                format!("'{}', {}", name, also)
            };
            return Err(self.fail(&msg));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Result<()> {
        if !is_separator(self.t()) {
            return Err(self.fail(SEPARATORS));
        }
        self.i += 1;
        Ok(())
    }

    fn id_string(&mut self) -> Result<String> {
        if !is_id(self.t()) {
            return Err(self.fail("'NODE_STRING'"));
        }
        let mut s = self.take();
        while is_id(self.t()) {
            s.push_str(&self.take());
        }
        Ok(s)
    }

    fn alpha_num(&mut self) -> Result<String> {
        if !is_alphanum(self.t()) {
            return Err(self.fail("'NODE_STRING'"));
        }
        let mut s = self.take();
        while is_alphanum(self.t()) {
            s.push_str(&self.take());
        }
        Ok(s)
    }

    fn text(&mut self) -> Result<FlowText> {
        let t = self.t();
        let kind = match t {
            tok::STR => LabelType::String,
            tok::MD_STR => LabelType::Markdown,
            _ if is_text_tok(t) => LabelType::Text,
            _ => return Err(self.fail(&format!("{}, 'TAGEND', 'STR', 'MD_STR'", TEXT_TOKENS))),
        };
        let mut s = self.take();
        while is_text_tok(self.t()) {
            s.push_str(&self.take());
        }
        Ok(FlowText { text: s, kind })
    }

    fn closed(&mut self, kind: u8) -> Result<()> {
        self.expect(kind, TEXT_TOKENS).map(|_| ())
    }

    fn edge_text(&mut self) -> Result<FlowText> {
        let t = self.t();
        let kind = match t {
            tok::STR => LabelType::String,
            tok::MD_STR => LabelType::Markdown,
            tok::EDGE_TEXT | tok::UNICODE_TEXT => LabelType::Text,
            _ => return Err(self.fail("'EDGE_TEXT', 'STR', 'MD_STR'")),
        };
        let mut s = self.take();
        while matches!(self.t(), tok::EDGE_TEXT | tok::UNICODE_TEXT) {
            s.push_str(&self.take());
        }
        Ok(FlowText { text: s, kind })
    }

    fn text_no_tags(&mut self) -> Result<FlowText> {
        let t = self.t();
        let kind = match t {
            tok::STR => LabelType::Text,
            tok::MD_STR => LabelType::Markdown,
            _ if is_no_tags(t) => LabelType::Text,
            _ => return Err(self.fail("'NODE_STRING', 'STR', 'MD_STR'")),
        };
        let mut s = self.take();
        while is_no_tags(self.t()) {
            s.push_str(&self.take());
        }
        Ok(FlowText { text: s, kind })
    }

    fn shape_data(&mut self) -> String {
        self.data_from = self.i;
        let mut s = self.take();
        while self.t() == tok::SHAPE_DATA {
            s.push_str(&self.take());
        }
        self.data_to = self.i - 1;
        s
    }

    fn attach(&mut self, id: &str, data: &str, from: usize, to: usize) -> Result<()> {
        let loc = self.span(from, to);
        self.db.add_vertex(
            id,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(data),
            Some(loc),
        )?;
        self.db.extend_vertex_mapping(id, Some(loc));
        Ok(())
    }

    fn attach_data(&mut self, id: &str, frame: &Frame) -> Result<()> {
        let Some(data) = &frame.data else {
            return Ok(());
        };
        let loc = self.span(frame.data_from, frame.data_to);
        self.db.add_vertex(
            id,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(data),
            Some(loc),
        )
    }

    fn vertex(&mut self) -> Result<String> {
        let from = self.i;
        let id = self.id_string()?;
        let mut label: Option<FlowText> = None;
        let mut shape: Option<&'static str> = None;
        let mut props: Option<Vec<(String, String)>> = None;
        match self.t() {
            tok::SQS => {
                self.i += 1;
                label = Some(self.text()?);
                self.closed(tok::SQE)?;
                shape = Some("square");
            }
            tok::DOUBLECIRCLESTART => {
                self.i += 1;
                label = Some(self.text()?);
                self.closed(tok::DOUBLECIRCLEEND)?;
                shape = Some("doublecircle");
            }
            tok::PS => {
                self.i += 1;
                if self.t() == tok::PS {
                    self.i += 1;
                    label = Some(self.text()?);
                    self.closed(tok::PE)?;
                    self.expect(tok::PE, "")?;
                    shape = Some("circle");
                } else {
                    label = Some(self.text()?);
                    self.closed(tok::PE)?;
                    shape = Some("round");
                }
            }
            tok::ELLIPSE_START => {
                self.i += 1;
                label = Some(self.text()?);
                self.closed(tok::ELLIPSE_END)?;
                shape = Some("ellipse");
            }
            tok::STADIUMSTART => {
                self.i += 1;
                label = Some(self.text()?);
                self.closed(tok::STADIUMEND)?;
                shape = Some("stadium");
            }
            tok::SUBROUTINESTART => {
                self.i += 1;
                label = Some(self.text()?);
                self.closed(tok::SUBROUTINEEND)?;
                shape = Some("subroutine");
            }
            tok::VERTEX_WITH_PROPS_START => {
                self.i += 1;
                let field = self.expect(tok::NODE_STRING, "")?;
                self.expect(tok::COLON, "")?;
                let value = self.expect(tok::NODE_STRING, "")?;
                self.expect(tok::PIPE, "")?;
                label = Some(self.text()?);
                self.closed(tok::SQE)?;
                shape = Some("rect");
                props = Some(vec![(field, value)]);
            }
            tok::CYLINDERSTART => {
                self.i += 1;
                label = Some(self.text()?);
                self.closed(tok::CYLINDEREND)?;
                shape = Some("cylinder");
            }
            tok::DIAMOND_START => {
                self.i += 1;
                if self.t() == tok::DIAMOND_START {
                    self.i += 1;
                    label = Some(self.text()?);
                    self.closed(tok::DIAMOND_STOP)?;
                    self.expect(tok::DIAMOND_STOP, "")?;
                    shape = Some("hexagon");
                } else {
                    label = Some(self.text()?);
                    self.closed(tok::DIAMOND_STOP)?;
                    shape = Some("diamond");
                }
            }
            tok::TAGEND => {
                self.i += 1;
                label = Some(self.text()?);
                self.closed(tok::SQE)?;
                shape = Some("odd");
            }
            tok::TRAPSTART => {
                self.i += 1;
                label = Some(self.text()?);
                if self.t() == tok::INVTRAPEND {
                    self.i += 1;
                    shape = Some("lean_right");
                } else {
                    self.closed(tok::TRAPEND)?;
                    shape = Some("trapezoid");
                }
            }
            tok::INVTRAPSTART => {
                self.i += 1;
                label = Some(self.text()?);
                if self.t() == tok::TRAPEND {
                    self.i += 1;
                    shape = Some("lean_left");
                } else {
                    self.closed(tok::INVTRAPEND)?;
                    shape = Some("inv_trapezoid");
                }
            }
            _ => {}
        }

        if props.is_some() {
            self.db.add_vertex(
                &id,
                label.as_ref(),
                shape,
                None,
                None,
                None,
                props,
                None,
                None,
            )?;
        } else if shape.is_some() {
            self.db.add_vertex(
                &id,
                label.as_ref(),
                shape,
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
        } else {
            self.db
                .add_vertex(&id, None, None, None, None, None, None, None, None)?;
        }
        let loc = self.span(from, self.i - 1);
        self.db.add_vertex_mapping(&id, shape.is_some(), Some(loc));
        Ok(id)
    }

    fn styled_vertex(&mut self) -> Result<String> {
        let id = self.vertex()?;
        if self.t() == tok::STYLE_SEPARATOR {
            self.i += 1;
            let cls = self.id_string()?;
            self.db.set_class(&id, &cls);
        }
        Ok(id)
    }

    fn node(&mut self) -> Result<Vec<String>> {
        let mut ids = vec![self.styled_vertex()?];
        self.pending_data = None;
        loop {
            let data = (self.t() == tok::SHAPE_DATA).then(|| self.shape_data());
            let from = self.data_from;
            let to = self.data_to;
            if self.t() != tok::SPACE {
                self.pending_data = data;
                return Ok(ids);
            }
            while self.t() == tok::SPACE {
                self.i += 1;
            }
            if self.t() != tok::AMP {
                if data.is_some() {
                    return Err(self.fail("'AMP'"));
                }
                return Ok(ids);
            }
            self.i += 1;
            if self.t() != tok::SPACE {
                return Err(self.fail("'SPACE'"));
            }
            while self.t() == tok::SPACE {
                self.i += 1;
            }
            let next = self.styled_vertex()?;
            if let Some(d) = data {
                let last = ids.last().unwrap().clone();
                self.attach(&last, &d, from, to)?;
            }
            ids.push(next);
        }
    }

    fn link(&mut self) -> Result<LinkInfo> {
        let id = (self.t() == tok::LINK_ID).then(|| self.take());
        if self.t() == tok::LINK {
            let raw = self.take();
            let mut info = self.db.destruct_link(&raw, None);
            info.id = id;
            if self.t() == tok::PIPE {
                self.i += 1;
                info.text = Some(self.text()?);
                self.closed(tok::PIPE)?;
                if self.t() == tok::SPACE {
                    self.i += 1;
                }
            }
            return Ok(info);
        }
        if self.t() == tok::START_LINK {
            let start = self.take();
            let label = self.edge_text()?;
            let end = self.expect(tok::LINK, "'EDGE_TEXT'")?;
            let mut info = self.db.destruct_link(&end, Some(&start));
            info.text = Some(label);
            info.id = id;
            return Ok(info);
        }
        Err(self.fail("'LINK', 'START_LINK'"))
    }

    fn vertex_statement(&mut self) -> Result<Vec<String>> {
        let from = self.i;
        let mut stmt = self.node()?;
        if let Some(data) = self.pending_data.take() {
            let last = stmt.last().unwrap().clone();
            self.attach(&last, &data, self.data_from, self.data_to)?;
        }
        let mut nodes = stmt.clone();
        loop {
            let t = self.t();
            if !matches!(t, tok::LINK | tok::START_LINK | tok::LINK_ID) {
                return Ok(nodes);
            }
            let info = self.link()?;
            let next = self.node()?;
            if let Some(data) = self.pending_data.take() {
                let last = next.last().unwrap().clone();
                self.attach(&last, &data, self.data_from, self.data_to)?;
            }
            self.db.add_link(&stmt, &next, Some(&info))?;
            let loc = self.span(from, self.i - 1);
            self.db.add_edge_mapping(&next, Some(loc));
            let mut combined = next.clone();
            combined.extend(nodes);
            nodes = combined;
            stmt = next;
        }
    }

    fn style_item(&mut self) -> Result<String> {
        if !is_style_part(self.t()) {
            return Err(self.fail("'NODE_STRING', 'NUM', 'COLON', 'BRKT'"));
        }
        let mut s = self.take();
        while is_style_part(self.t()) {
            s.push_str(&self.take());
        }
        Ok(s)
    }

    fn styles_opt(&mut self) -> Result<Vec<String>> {
        let mut out = vec![self.style_item()?];
        while self.t() == tok::COMMA {
            self.i += 1;
            out.push(self.style_item()?);
        }
        Ok(out)
    }

    fn link_style_statement(&mut self) -> Result<()> {
        self.i += 1;
        self.expect(tok::SPACE, "")?;
        let positions = if self.t() == tok::DEFAULT {
            vec![self.take()]
        } else {
            let mut list = vec![self.expect(tok::NUM, "'DEFAULT'")?];
            while self.t() == tok::COMMA {
                self.i += 1;
                list.push(self.expect(tok::NUM, "")?);
            }
            list
        };
        self.expect(tok::SPACE, "")?;
        if self.t() != tok::INTERPOLATE {
            let styles = self.styles_opt()?;
            return self.db.update_link(&positions, &styles);
        }
        self.i += 1;
        self.expect(tok::SPACE, "")?;
        let interpolate = self.alpha_num()?;
        if self.t() == tok::SPACE {
            self.i += 1;
            let styles = self.styles_opt()?;
            self.db.update_link_interpolate(&positions, &interpolate)?;
            self.db.update_link(&positions, &styles)?;
        } else {
            self.db.update_link_interpolate(&positions, &interpolate)?;
        }
        Ok(())
    }

    fn link_tail(&mut self, id: &str, url: &str) -> Result<()> {
        if self.t() != tok::SPACE {
            self.db.set_link(id, url, None);
            return Ok(());
        }
        self.i += 1;
        if self.t() == tok::LINK_TARGET {
            let target = self.take();
            self.db.set_link(id, url, Some(&target));
            return Ok(());
        }
        let tooltip = self.expect(tok::STR, "'LINK_TARGET'")?;
        if self.t() == tok::SPACE {
            self.i += 1;
            let target = self.expect(tok::LINK_TARGET, "")?;
            self.db.set_link(id, url, Some(&target));
        } else {
            self.db.set_link(id, url, None);
        }
        self.db.set_tooltip(id, Some(&tooltip));
        Ok(())
    }

    fn click_statement(&mut self) -> Result<()> {
        let id = self.take();
        match self.t() {
            tok::CALLBACKNAME => {
                let name = self.take();
                let args = (self.t() == tok::CALLBACKARGS).then(|| self.take());
                let tooltip = if self.t() == tok::SPACE {
                    self.i += 1;
                    Some(self.expect(tok::STR, "")?)
                } else {
                    None
                };
                self.db
                    .set_click_event(&id, Some(&name), args.as_deref());
                if let Some(tip) = tooltip {
                    self.db.set_tooltip(&id, Some(&tip));
                }
                Ok(())
            }
            tok::HREF => {
                self.i += 1;
                let url = self.expect(tok::STR, "")?;
                self.link_tail(&id, &url)
            }
            tok::STR => {
                let url = self.take();
                self.link_tail(&id, &url)
            }
            _ => {
                let name = self.alpha_num()?;
                if self.t() == tok::SPACE {
                    self.i += 1;
                    let tooltip = self.expect(tok::STR, "")?;
                    self.db.set_click_event(&id, Some(&name), None);
                    self.db.set_tooltip(&id, Some(&tooltip));
                } else {
                    self.db.set_click_event(&id, Some(&name), None);
                }
                Ok(())
            }
        }
    }

    // Reads `flow` or `connector` up to the separator: an id, an optional bracketed title, optional metadata.
    fn header(&mut self, at: usize) -> Result<Frame> {
        self.expect(tok::SPACE, SEPARATORS)?;
        let id = self.text_no_tags()?;
        let mut title = FlowText {
            text: String::new(),
            kind: LabelType::Text,
        };
        if self.t() == tok::SQS {
            self.i += 1;
            title = self.text()?;
            self.closed(tok::SQE)?;
        }
        let data = (self.t() == tok::SHAPE_DATA).then(|| self.shape_data());
        if !is_separator(self.t()) {
            return Err(self.fail(&format!("{}, 'SHAPE_DATA'", SEPARATORS)));
        }
        Ok(Frame {
            global: false,
            at,
            id: Some(id),
            title: Some(title),
            data,
            data_from: self.data_from,
            data_to: self.data_to,
            list: Vec::new(),
            collect: false,
        })
    }

    fn statement(&mut self) -> Result<DocItem> {
        let t = self.t();
        match t {
            tok::STYLE => {
                self.i += 1;
                self.expect(tok::SPACE, "")?;
                let id = self.id_string()?;
                self.expect(tok::SPACE, "")?;
                let styles = self.styles_opt()?;
                self.db.add_vertex(
                    &id,
                    None,
                    None,
                    Some(styles),
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;
                self.separator()?;
                Ok(DocItem::List(Vec::new()))
            }
            tok::LINKSTYLE => {
                self.link_style_statement()?;
                self.separator()?;
                Ok(DocItem::List(Vec::new()))
            }
            tok::CLASSDEF => {
                self.i += 1;
                self.expect(tok::SPACE, "")?;
                let id = self.id_string()?;
                self.expect(tok::SPACE, "")?;
                let styles = self.styles_opt()?;
                self.db.add_class(&id, &styles);
                self.separator()?;
                Ok(DocItem::List(Vec::new()))
            }
            tok::CLASS => {
                self.i += 1;
                self.expect(tok::SPACE, "")?;
                let ids = self.id_string()?;
                self.expect(tok::SPACE, "")?;
                let cls = self.id_string()?;
                self.db.set_class(&ids, &cls);
                self.separator()?;
                Ok(DocItem::List(Vec::new()))
            }
            tok::CLICK => {
                self.click_statement()?;
                self.separator()?;
                Ok(DocItem::List(Vec::new()))
            }
            tok::CONNECTOR => {
                let at = self.i;
                self.i += 1;
                let frame = self.header(at)?;
                let sep = self.i;
                self.i += 1;
                let id = self
                    .db
                    .add_connector(frame.id.as_ref(), frame.title.as_ref());
                self.attach_data(&id, &frame)?;
                let start_loc = self.span(frame.at, frame.at);
                let end_loc = self.span(sep, sep);
                self.db
                    .add_connector_mapping(frame.id.as_ref(), Some(start_loc), Some(end_loc));
                Ok(DocItem::Text(id))
            }
            tok::DIRECTION_TB..=tok::DIRECTION_TD => {
                self.i += 1;
                Ok(DocItem::Dir(
                    DIRECTIONS[(t - tok::DIRECTION_TB) as usize].to_string(),
                ))
            }
            tok::ACC_TITLE => {
                self.i += 1;
                let value = self.expect(tok::ACC_TITLE_VALUE, "")?.trim().to_string();
                self.db.set_acc_title(&value);
                Ok(DocItem::Text(value))
            }
            tok::ACC_DESCR => {
                self.i += 1;
                let value = self.expect(tok::ACC_DESCR_VALUE, "")?.trim().to_string();
                self.db.set_acc_description(&value);
                Ok(DocItem::Text(value))
            }
            tok::ACC_DESCR_MULTILINE_VALUE => {
                let value = self.take().trim().to_string();
                self.db.set_acc_description(&value);
                Ok(DocItem::Text(value))
            }
            _ => {
                if !is_id(t) {
                    return Err(self.fail(STATEMENT_START));
                }
                let nodes = self.vertex_statement()?;
                self.separator()?;
                Ok(DocItem::List(nodes))
            }
        }
    }

    // Blocks nest without recursion: each open one keeps the list and collect flag of its parent.
    fn document(&mut self) -> Result<()> {
        let mut open: Vec<Frame> = Vec::new();
        let mut list: Vec<DocItem> = Vec::new();
        let mut collect = false;
        loop {
            let t = self.t();
            if t == tok::END {
                break;
            }
            if t == tok::END_KW {
                let Some(frame) = open.pop() else {
                    break;
                };
                let at = self.i;
                self.i += 1;
                if frame.global {
                    self.db.add_global(&list);
                    list = frame.list;
                    collect = frame.collect;
                    continue;
                }
                let id =
                    self.db
                        .add_sub_graph(frame.id.as_ref(), false, &list, frame.title.as_ref());
                self.attach_data(&id, &frame)?;
                let start_loc = self.span(frame.at, frame.at);
                let end_loc = self.span(at, at);
                self.db
                    .add_subgraph_mapping(frame.id.as_ref(), Some(start_loc), Some(end_loc));
                list = frame.list;
                collect = frame.collect;
                if collect {
                    list.push(DocItem::Text(id));
                }
            } else if matches!(t, tok::SEMI | tok::NEWLINE | tok::SPACE | tok::EOF) {
                let text = self.take();
                if collect {
                    list.push(DocItem::Text(text));
                }
            } else if t == tok::COMMENT {
                self.i += 1;
            } else if matches!(t, tok::FLOW | tok::GLOBAL) {
                let is_global = t == tok::GLOBAL;
                let at = self.i;
                self.i += 1;
                let frame = if is_global || is_separator(self.t()) {
                    self.separator()?;
                    Frame {
                        global: is_global,
                        at,
                        id: None,
                        title: None,
                        data: None,
                        data_from: 0,
                        data_to: 0,
                        list,
                        collect,
                    }
                } else {
                    let mut f = self.header(at)?;
                    f.list = list;
                    f.collect = collect;
                    self.i += 1;
                    f
                };
                open.push(frame);
                list = Vec::new();
                collect = true;
            } else {
                let item = self.statement()?;
                if collect && !matches!(&item, DocItem::List(v) if v.is_empty()) {
                    list.push(item);
                }
            }
        }
        if !open.is_empty() {
            return Err(self.fail("'end'"));
        }
        Ok(())
    }

    fn parse(&mut self) -> Result<()> {
        while matches!(self.t(), tok::SPACE | tok::NEWLINE | tok::COMMENT) {
            self.i += 1;
        }
        self.expect(tok::GRAPH, "'SPACE', 'NEWLINE', 'COMMENT'")?;
        if self.t() == tok::NODIR {
            self.i += 1;
            self.db.set_direction("TB");
        } else {
            let dir = self.expect(tok::DIR, "'NODIR'")?;
            let mut t = self.t();
            if t == tok::SPACE {
                while self.t() == tok::SPACE {
                    self.i += 1;
                }
                t = self.t();
                if t != tok::NEWLINE {
                    return Err(self.fail("'NEWLINE', 'SPACE'"));
                }
            } else if !matches!(t, tok::SEMI | tok::NEWLINE | tok::COMMENT) {
                return Err(self.fail("'COMMENT', 'SEMI', 'NEWLINE', 'SPACE'"));
            }
            self.i += 1;
            self.db.set_direction(&dir);
        }
        self.document()?;
        if self.t() != tok::END {
            return Err(self.fail(STATEMENT_START));
        }
        Ok(())
    }
}

pub fn parse_agentflow(source: &str, db: &mut AgentflowDb) -> Result<()> {
    parse_agentflow_with_mode(source, db, false)
}

// `raw` parses the text as it is, the way Mermaid's generated parser does without its wrapper.
pub fn parse_agentflow_with_mode(source: &str, db: &mut AgentflowDb, raw: bool) -> Result<()> {
    let orig_chars: Vec<char> = source.chars().collect();
    let (cleaned_str, cleaned_chars, edits) = if !raw && source.contains('}') {
        strip_brace_trails(&orig_chars)
    } else {
        (String::new(), Vec::new(), Vec::new())
    };
    let (lex_str, lex_chars): (&str, &[char]) = if edits.is_empty() && (raw || !source.contains('}')) {
        (source, &orig_chars)
    } else {
        (&cleaned_str, &cleaned_chars)
    };
    let Tokens {
        types,
        texts,
        starts,
        locs,
    } = tokenize(lex_str, lex_chars);
    let mut parser = Parser {
        orig_src: &orig_chars,
        src: lex_chars,
        edits,
        types,
        texts,
        starts,
        locs,
        i: 0,
        pending_data: None,
        data_from: 0,
        data_to: 0,
        db,
    };
    parser.parse()
}
