use super::db::{DocItem, FlowDb, FlowText, LabelType, LinkInfo, Vertex};
use super::lexer::{tok, tokenize, Tokens, TOKEN_NAMES};
use crate::errors::{syntax_error, PeleError, Result};

fn is_id(t: u8) -> bool {
    matches!(
        t,
        tok::NUM | tok::NODE_STRING | tok::DOWN | tok::MINUS | tok::DEFAULT | tok::COMMA | tok::COLON | tok::AMP | tok::BRKT | tok::MULT | tok::UNICODE_TEXT
    )
}

fn is_alphanum(t: u8) -> bool {
    matches!(
        t,
        tok::NUM | tok::UNICODE_TEXT | tok::NODE_STRING | tok::DIR | tok::DOWN | tok::MINUS | tok::COMMA | tok::COLON | tok::AMP | tok::BRKT | tok::MULT
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
            | tok::SUBGRAPH
            | tok::END_KW
            | tok::DOWN
            | tok::UP
            | tok::START_LINK
    )
}

fn is_style_part(t: u8) -> bool {
    matches!(t, tok::NUM | tok::NODE_STRING | tok::COLON | tok::SPACE | tok::BRKT | tok::STYLE)
}

fn is_text(t: u8) -> bool {
    matches!(t, tok::TEXT | tok::TAGSTART | tok::TAGEND | tok::UNICODE_TEXT)
}

fn ends_statement(t: u8) -> bool {
    matches!(t, tok::NEWLINE | tok::SEMI | tok::EOF)
}

struct Frame {
    id: Option<FlowText>,
    title: Option<FlowText>,
    // Whether the title is the id itself, as in `subgraph One two`.
    same: bool,
    list: Vec<DocItem>,
    collect: bool,
}

const TEXT_TOKENS: &str = "'UNICODE_TEXT', 'TEXT', 'TAGSTART'";
const DIRECTIONS: [&str; 5] = ["TB", "BT", "RL", "LR", "TD"];
const STATEMENT_START: &str = "'SEMI', 'NEWLINE', 'SPACE', 'EOF', 'subgraph', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'STYLE', 'LINKSTYLE', 'CLASSDEF', 'CLASS', 'CLICK', 'NODE_STRING'";

struct Parser<'a> {
    src: &'a [char],
    types: Vec<u8>,
    texts: Vec<String>,
    starts: Vec<usize>,
    i: usize,
    pending_data: Option<String>,
    db: &'a mut FlowDb,
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
        let at = self.i.min(self.types.len() - 1);
        let t = self.types[at];
        if t == tok::ERROR {
            return syntax_error("flowchart", self.src, self.starts[at], "", true);
        }
        syntax_error("flowchart", self.src, self.starts[at], &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[t as usize]), false)
    }

    fn expect(&mut self, kind: u8, also: &str) -> Result<String> {
        if self.t() != kind {
            let name = TOKEN_NAMES[kind as usize];
            return Err(self.fail(&if also.is_empty() { format!("'{}'", name) } else { format!("'{}', {}", name, also) }));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Result<()> {
        if !ends_statement(self.t()) {
            return Err(self.fail("'SEMI', 'NEWLINE', 'EOF'"));
        }
        self.i += 1;
        Ok(())
    }

    // The texts of a run of tokens that `accepts` them, the first of which must.
    fn run(&mut self, accepts: fn(u8) -> bool, expected: &str) -> Result<String> {
        if !accepts(self.t()) {
            return Err(self.fail(expected));
        }
        let mut s = self.take();
        while accepts(self.t()) {
            s.push_str(&self.take());
        }
        Ok(s)
    }

    fn id_string(&mut self) -> Result<String> {
        self.run(is_id, "'NODE_STRING'")
    }

    fn alpha_num(&mut self) -> Result<String> {
        self.run(is_alphanum, "'NODE_STRING'")
    }

    fn text(&mut self) -> Result<FlowText> {
        let t = self.t();
        let kind = match t {
            tok::STR => LabelType::String,
            tok::MD_STR => LabelType::Markdown,
            _ if is_text(t) => LabelType::Text,
            _ => return Err(self.fail(&format!("{}, 'TAGEND', 'STR', 'MD_STR'", TEXT_TOKENS))),
        };
        let mut s = self.take();
        while is_text(self.t()) {
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
        let mut s = self.take();
        while self.t() == tok::SHAPE_DATA {
            s.push_str(&self.take());
        }
        s
    }

    // A label between brackets that end with `close`.
    fn bracketed(&mut self, id: &str, close: u8, shape: &str) -> Result<()> {
        self.i += 1;
        let label = self.text()?;
        self.closed(close)?;
        self.db.add_vertex(id, Vertex { text: Some(&label), shape: Some(shape), ..Default::default() })
    }

    fn vertex(&mut self) -> Result<String> {
        let id = self.id_string()?;
        match self.t() {
            tok::SQS => self.bracketed(&id, tok::SQE, "square")?,
            tok::DOUBLECIRCLESTART => self.bracketed(&id, tok::DOUBLECIRCLEEND, "doublecircle")?,
            tok::PS => {
                if self.types[self.i + 1] == tok::PS {
                    self.i += 1;
                    self.bracketed(&id, tok::PE, "circle")?;
                    self.expect(tok::PE, "")?;
                } else {
                    self.bracketed(&id, tok::PE, "round")?;
                }
            }
            tok::ELLIPSE_START => self.bracketed(&id, tok::ELLIPSE_END, "ellipse")?,
            tok::STADIUMSTART => self.bracketed(&id, tok::STADIUMEND, "stadium")?,
            tok::SUBROUTINESTART => self.bracketed(&id, tok::SUBROUTINEEND, "subroutine")?,
            tok::VERTEX_WITH_PROPS_START => {
                self.i += 1;
                let field = self.expect(tok::NODE_STRING, "")?;
                self.expect(tok::COLON, "")?;
                let value = self.expect(tok::NODE_STRING, "")?;
                self.expect(tok::PIPE, "")?;
                let label = self.text()?;
                self.closed(tok::SQE)?;
                self.db.add_vertex(&id, Vertex { text: Some(&label), shape: Some("rect"), props: Some((field, value)), ..Default::default() })?;
            }
            tok::CYLINDERSTART => self.bracketed(&id, tok::CYLINDEREND, "cylinder")?,
            tok::DIAMOND_START => {
                if self.types[self.i + 1] == tok::DIAMOND_START {
                    self.i += 1;
                    self.bracketed(&id, tok::DIAMOND_STOP, "hexagon")?;
                    self.expect(tok::DIAMOND_STOP, "")?;
                } else {
                    self.bracketed(&id, tok::DIAMOND_STOP, "diamond")?;
                }
            }
            tok::TAGEND => self.bracketed(&id, tok::SQE, "odd")?,
            tok::TRAPSTART | tok::INVTRAPSTART => {
                let inv = self.t() == tok::INVTRAPSTART;
                self.i += 1;
                let label = self.text()?;
                // `[/x/]` leans right and `[\x\]` left; mixed ends make the two trapezoids.
                let (crossed, straight) = if inv { (tok::TRAPEND, tok::INVTRAPEND) } else { (tok::INVTRAPEND, tok::TRAPEND) };
                let shape = if self.t() == crossed {
                    self.i += 1;
                    if inv {
                        "lean_left"
                    } else {
                        "lean_right"
                    }
                } else {
                    self.closed(straight)?;
                    if inv {
                        "inv_trapezoid"
                    } else {
                        "trapezoid"
                    }
                };
                self.db.add_vertex(&id, Vertex { text: Some(&label), shape: Some(shape), ..Default::default() })?;
            }
            _ => self.db.add_vertex(&id, Vertex::default())?,
        }
        Ok(id)
    }

    fn styled_vertex(&mut self) -> Result<String> {
        let id = self.vertex()?;
        if self.t() == tok::STYLE_SEPARATOR {
            self.i += 1;
            let class = self.id_string()?;
            self.db.set_class(&id, &class);
        }
        Ok(id)
    }

    fn skip_spaces(&mut self) {
        while self.t() == tok::SPACE {
            self.i += 1;
        }
    }

    fn node(&mut self) -> Result<Vec<String>> {
        let mut ids = vec![self.styled_vertex()?];
        self.pending_data = None;
        loop {
            let data = if self.t() == tok::SHAPE_DATA { Some(self.shape_data()) } else { None };
            if self.t() != tok::SPACE {
                self.pending_data = data;
                return Ok(ids);
            }
            self.skip_spaces();
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
            self.skip_spaces();
            let next = self.styled_vertex()?;
            if let Some(data) = &data {
                self.db.add_vertex(ids.last().unwrap(), Vertex { metadata: Some(data), ..Default::default() })?;
            }
            ids.push(next);
        }
    }

    fn link(&mut self) -> Result<LinkInfo> {
        let id = if self.t() == tok::LINK_ID { Some(self.take()) } else { None };
        if self.t() == tok::LINK {
            let line = self.take();
            let mut info = self.db.destruct_link(&line, None);
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

    fn apply_pending(&mut self, ids: &[String]) -> Result<()> {
        if let Some(data) = self.pending_data.take() {
            self.db.add_vertex(ids.last().unwrap(), Vertex { metadata: Some(&data), ..Default::default() })?;
            self.pending_data = Some(data);
        }
        Ok(())
    }

    fn vertex_statement(&mut self, collect: bool) -> Result<Vec<String>> {
        let mut stmt = self.node()?;
        self.apply_pending(&stmt)?;
        let mut nodes = stmt.clone();
        loop {
            if !matches!(self.t(), tok::LINK | tok::START_LINK | tok::LINK_ID) {
                return Ok(nodes);
            }
            let info = self.link()?;
            let next = self.node()?;
            self.apply_pending(&next)?;
            self.db.add_link(&stmt, &next, Some(&info))?;
            if collect {
                let mut joined = next.clone();
                joined.extend(nodes);
                nodes = joined;
            }
            stmt = next;
        }
    }

    fn styles_opt(&mut self) -> Result<Vec<String>> {
        let expected = "'NODE_STRING', 'NUM', 'COLON', 'BRKT'";
        let mut out = vec![self.run(is_style_part, expected)?];
        while self.t() == tok::COMMA {
            self.i += 1;
            out.push(self.run(is_style_part, expected)?);
        }
        Ok(out)
    }

    fn link_style_statement(&mut self) -> Result<()> {
        self.i += 1;
        self.expect(tok::SPACE, "")?;
        let mut positions: Vec<String>;
        if self.t() == tok::DEFAULT {
            positions = vec![self.take()];
        } else {
            positions = vec![self.expect(tok::NUM, "'DEFAULT'")?];
            while self.t() == tok::COMMA {
                self.i += 1;
                positions.push(self.expect(tok::NUM, "")?);
            }
        }
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
            self.db.update_link(&positions, &styles)
        } else {
            self.db.update_link_interpolate(&positions, &interpolate)
        }
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
        self.db.set_tooltip(id, &tooltip);
        Ok(())
    }

    fn click_statement(&mut self) -> Result<()> {
        let id = self.take();
        match self.t() {
            tok::CALLBACKNAME => {
                self.i += 1;
                if self.t() == tok::CALLBACKARGS {
                    self.i += 1;
                }
                let mut tooltip = None;
                if self.t() == tok::SPACE {
                    self.i += 1;
                    tooltip = Some(self.expect(tok::STR, "")?);
                }
                self.db.set_click_event(&id);
                if let Some(tooltip) = tooltip {
                    self.db.set_tooltip(&id, &tooltip);
                }
            }
            tok::HREF => {
                self.i += 1;
                let url = self.expect(tok::STR, "")?;
                self.link_tail(&id, &url)?;
            }
            tok::STR => {
                let url = self.take();
                self.link_tail(&id, &url)?;
            }
            _ => {
                self.alpha_num()?;
                if self.t() == tok::SPACE {
                    self.i += 1;
                    let tooltip = self.expect(tok::STR, "")?;
                    self.db.set_click_event(&id);
                    self.db.set_tooltip(&id, &tooltip);
                } else {
                    self.db.set_click_event(&id);
                }
            }
        }
        Ok(())
    }

    fn subgraph_header(&mut self) -> Result<Frame> {
        self.i += 1;
        if ends_statement(self.t()) {
            self.i += 1;
            return Ok(Frame { id: None, title: None, same: true, list: Vec::new(), collect: false });
        }
        self.expect(tok::SPACE, "'SEMI', 'NEWLINE', 'EOF'")?;
        let id = self.text_no_tags()?;
        let mut title = id.clone();
        let mut same = true;
        if self.t() == tok::SQS {
            self.i += 1;
            title = self.text()?;
            same = false;
            self.closed(tok::SQE)?;
        }
        if !ends_statement(self.t()) {
            return Err(self.fail("'SEMI', 'NEWLINE', 'EOF', 'SQS'"));
        }
        self.i += 1;
        Ok(Frame { id: Some(id), title: Some(title), same, list: Vec::new(), collect: false })
    }

    // A statement, and what it adds to the subgraph it is in.
    fn statement(&mut self, collect: bool) -> Result<Option<DocItem>> {
        let t = self.t();
        match t {
            tok::STYLE => {
                self.i += 1;
                self.expect(tok::SPACE, "")?;
                let id = self.id_string()?;
                self.expect(tok::SPACE, "")?;
                let style = self.styles_opt()?;
                self.db.add_vertex(&id, Vertex { style: Some(style), ..Default::default() })?;
                self.separator()?;
                Ok(None)
            }
            tok::LINKSTYLE => {
                self.link_style_statement()?;
                self.separator()?;
                Ok(None)
            }
            tok::CLASSDEF => {
                self.i += 1;
                self.expect(tok::SPACE, "")?;
                let id = self.id_string()?;
                self.expect(tok::SPACE, "")?;
                let styles = self.styles_opt()?;
                self.db.add_class(&id, &styles);
                self.separator()?;
                Ok(None)
            }
            tok::CLASS => {
                self.i += 1;
                self.expect(tok::SPACE, "")?;
                let ids = self.id_string()?;
                self.expect(tok::SPACE, "")?;
                let class = self.id_string()?;
                self.db.set_class(&ids, &class);
                self.separator()?;
                Ok(None)
            }
            tok::CLICK => {
                self.click_statement()?;
                self.separator()?;
                Ok(None)
            }
            tok::DIRECTION_TB..=tok::DIRECTION_TD => {
                self.i += 1;
                Ok(Some(DocItem::Dir(DIRECTIONS[(t - tok::DIRECTION_TB) as usize].to_string())))
            }
            tok::ACC_TITLE => {
                self.i += 1;
                let value = self.expect(tok::ACC_TITLE_VALUE, "")?.trim().to_string();
                self.db.set_acc_title(&value);
                Ok(Some(DocItem::Text(value)))
            }
            tok::ACC_DESCR => {
                self.i += 1;
                let value = self.expect(tok::ACC_DESCR_VALUE, "")?.trim().to_string();
                self.db.set_acc_description(&value);
                Ok(Some(DocItem::Text(value)))
            }
            tok::ACC_DESCR_MULTILINE_VALUE => {
                let value = self.take().trim().to_string();
                self.db.set_acc_description(&value);
                Ok(Some(DocItem::Text(value)))
            }
            _ => {
                if !is_id(t) {
                    return Err(self.fail(STATEMENT_START));
                }
                let nodes = self.vertex_statement(collect)?;
                self.separator()?;
                Ok(Some(DocItem::List(nodes)))
            }
        }
    }

    // Subgraphs nest without recursion: each open one keeps the list and collect flag of its parent.
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
                let Some(frame) = open.pop() else { break };
                self.i += 1;
                let id = self.db.add_sub_graph(frame.id.as_ref(), frame.same, &list, frame.title.as_ref());
                list = frame.list;
                collect = frame.collect;
                if collect {
                    list.push(DocItem::Text(id));
                }
            } else if matches!(t, tok::SEMI | tok::NEWLINE | tok::SPACE | tok::EOF) {
                if collect {
                    list.push(DocItem::Text(self.texts[self.i].clone()));
                }
                self.i += 1;
            } else if t == tok::SUBGRAPH {
                let mut frame = self.subgraph_header()?;
                frame.list = std::mem::take(&mut list);
                frame.collect = collect;
                open.push(frame);
                collect = true;
            } else if let Some(item) = self.statement(collect)? {
                if collect {
                    list.push(item);
                }
            }
        }
        if !open.is_empty() {
            return Err(self.fail("'end'"));
        }
        Ok(())
    }
}

// Mermaid trims whitespace between a closing brace and the end of its line before parsing:
// /}\s*\n/g becomes "}\n".
fn trim_brace_trails(src: &str) -> Vec<char> {
    let s: Vec<char> = src.chars().collect();
    if !src.contains('}') {
        return s;
    }
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        out.push(s[i]);
        if s[i] == '}' {
            let mut j = i + 1;
            let mut last = None;
            while j < s.len() && crate::chars::is_space(s[j]) {
                if s[j] == '\n' {
                    last = Some(j);
                }
                j += 1;
            }
            if let Some(last) = last {
                out.push('\n');
                i = last + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

pub fn parse_flowchart(source: &str, db: &mut FlowDb) -> Result<()> {
    let src = trim_brace_trails(source);
    let Tokens { types, texts, starts } = tokenize(&src);
    let mut p = Parser { src: &src, types, texts, starts, i: 0, pending_data: None, db };
    while matches!(p.t(), tok::SPACE | tok::NEWLINE) {
        p.i += 1;
    }
    p.expect(tok::GRAPH, "'SPACE', 'NEWLINE'")?;
    if p.t() == tok::NODIR {
        p.i += 1;
        p.db.set_direction("TB");
    } else {
        let dir = p.expect(tok::DIR, "'NODIR'")?;
        let t = p.t();
        if t == tok::SPACE {
            p.skip_spaces();
            if p.t() != tok::NEWLINE {
                return Err(p.fail("'NEWLINE', 'SPACE'"));
            }
        } else if t != tok::SEMI && t != tok::NEWLINE {
            return Err(p.fail("'SEMI', 'NEWLINE', 'SPACE'"));
        }
        p.i += 1;
        p.db.set_direction(&dir);
    }
    p.document()?;
    if p.t() != tok::END {
        return Err(p.fail(STATEMENT_START));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> FlowDb {
        let mut db = FlowDb::default();
        parse_flowchart(src, &mut db).unwrap();
        db
    }

    #[test]
    fn nodes_and_edges() {
        let db = parse("flowchart TD\n  A[Mermaid text] --> B{Flowchart?}\n  B -- Yes --> C[Pele]\n  B -- No --> D[Mermaid]\n  C --> E([SVG])\n  D --> E\n");
        assert_eq!(db.direction.as_deref(), Some("TB"));
        assert_eq!(db.nodes.len(), 5);
        assert_eq!(db.node("B").unwrap().shape.as_deref(), Some("diamond"));
        assert_eq!(db.node("E").unwrap().text.as_deref(), Some("SVG"));
        assert_eq!(db.edges.len(), 5);
        assert_eq!(db.edges[1].text, "Yes");
        assert_eq!(db.edges[1].kind.as_deref(), Some("arrow_point"));
    }

    #[test]
    fn subgraphs_and_styles() {
        let db = parse("graph LR\nsubgraph one [Title here]\n  a --> b & c\nend\nclassDef x fill:#f00,color:#fff\nclass a x\nstyle b stroke:#333\nlinkStyle 0 stroke:red\na@{ shape: cyl, label: \"Store\" }\n");
        assert_eq!(db.subgraphs.len(), 1);
        assert_eq!(db.subgraphs[0].title, "Title here");
        assert_eq!(db.subgraphs[0].nodes, ["b", "c", "a"]);
        assert_eq!(db.edges.len(), 2);
        assert_eq!(db.node("a").unwrap().classes, ["x"]);
        assert_eq!(db.node("a").unwrap().shape.as_deref(), Some("cyl"));
        assert_eq!(db.node("a").unwrap().text.as_deref(), Some("Store"));
        assert_eq!(db.edges[0].style.as_ref().unwrap(), &["stroke:red", "fill:none"]);
    }

    #[test]
    fn errors() {
        let mut db = FlowDb::default();
        let err = parse_flowchart("flowchart TD\n  A --> \n", &mut db).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.message.starts_with("Parse error on line 2:"));
    }
}
