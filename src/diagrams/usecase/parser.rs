use super::db::{
    empty_drafts, resolve_usecase, DraftBoundary, DraftClassAssign, DraftClassDef,
    DraftClassTarget, DraftElement, DraftEndpoint, DraftJson, DraftLabel, DraftMetadata,
    DraftNote, DraftProperty, DraftPropertyValue, DraftRelationship, DraftStyleAssign, Drafts,
    ElementKind,
};
use super::json::parse_ordered_json;
use super::lexer::{line_column, location_text, tokenize};
use super::tokens::{t, TOKEN_NAMES};
use super::types::{
    arrow, Direction, EdgeOccurrence, GraphStatement, LabelType, MetadataOccurrence,
    NodeOccurrence, OrdMap, RelationshipType, Span, StatementKind, UseCaseShape, UsecaseModel,
};
use crate::errors::{ErrorCode, PeleError, Result};
use crate::value::Value;

// Mermaid parses use case diagrams with a Chevrotain parser, which decides each choice by looking
// at most three tokens ahead and reports the first token that fits nowhere. This parser makes the
// same decisions from the same lookahead, so it accepts the same text and names the same token in
// its errors. It builds the model directly, where Mermaid builds a syntax tree and then visits it.

const LINE: u8 = 1;
const RELATION: u8 = 2;
const STYLE_PART: u8 = 4;
const FOLLOWS_NAME: u8 = 8;
const BOUNDARY_LINE: u8 = 16;
const CLOSES_LABEL: u8 = 32;
const METADATA_FIRST: u8 = 64;

const ARROWS: &str = "DEPENDENCY_ARROW|GENERALIZATION|FORWARD_SOLID|BACKWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|BACKWARD_CIRCLE|FORWARD_CROSS|BACKWARD_CROSS";
const NAMES: &str = "IDENTIFIER|PLAIN_STRING|MARKDOWN_STRING";
const LABELS: &str = "PLAIN_STRING|MARKDOWN_STRING|LABEL_TEXT";
const STATEMENTS: &str = "ACC_TITLE_LINE|ACC_DESCR_LINE|ACC_DESCR_BLOCK|DIRECTION|ACTOR|SYSTEM_BOUNDARY|NOTE|JSON_DECLARATION_START|CLASS_DEF|CLASS|STYLE";
const STYLE_PARTS: &str = "WORD|PLAIN_STRING|NUMBER|HASH_COLOR|CSS_IDENTIFIER|CSS_ESCAPED_COMMA|DASH|DOT|PERCENT|CSS_PUNCTUATION|COLON|LPAREN|RPAREN|LBRACKET|RBRACKET|LBRACE|RBRACE|AT|MARKERLESS_SOLID";

fn build_flags() -> [u8; t::COUNT] {
    let mut flags = [0u8; t::COUNT];
    let mut flag = |names: &str, bits: u8| {
        for name in names.split('|') {
            if name == "WORD" {
                for tok in t::USECASE..=t::IDENTIFIER {
                    flags[tok] |= bits;
                }
            } else if let Some(idx) = TOKEN_NAMES.iter().position(|&n| n == name) {
                flags[idx] |= bits;
            }
        }
    };
    flag(
        &format!("NEWLINE|COMMENT|{}|{}", STATEMENTS, NAMES),
        LINE,
    );
    flag(&format!("IDENTIFIER|{}", ARROWS), RELATION);
    flag(STYLE_PARTS, STYLE_PART);
    flag(
        &format!("STEREOTYPE_START|CLASS_SEPARATOR|IDENTIFIER|{}|NEWLINE|EOF", ARROWS),
        FOLLOWS_NAME,
    );
    flag(&format!("NEWLINE|COMMENT|ACTOR|{}", NAMES), BOUNDARY_LINE);
    flag(
        "FORWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|FORWARD_CROSS",
        CLOSES_LABEL,
    );
    flag(
        "NEWLINE|IDENTIFIER|PLAIN_STRING|RBRACE",
        METADATA_FIRST,
    );
    flags
}

fn is_forbidden(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "allowmixing" | "newpage" | "package" | "rectangle" | "skinparam"
    )
}

fn statement_sequences() -> String {
    let names: Vec<&str> = NAMES.split('|').collect();
    let mut metadata = Vec::new();
    for &name in &names {
        for next in ["NEWLINE", "IDENTIFIER", "PLAIN_STRING", "RBRACE"] {
            metadata.push(format!("{}, METADATA_START, {}", name, next));
        }
    }
    let follows_str = format!(
        "STEREOTYPE_START|CLASS_SEPARATOR|IDENTIFIER|{}|NEWLINE|EOF",
        ARROWS
    );
    let follows: Vec<&str> = follows_str.split('|').collect();
    let mut entities = Vec::new();
    for (i, &name) in names.iter().enumerate() {
        if i == 0 {
            for next in ["LPAREN", "LBRACKET"] {
                entities.push(format!("{}, {}", name, next));
            }
        }
        for &next in &follows {
            entities.push(format!("{}, {}", name, next));
        }
    }
    let mut parts = vec![STATEMENTS.to_string()];
    parts.extend(metadata.iter().cloned());
    parts.extend(entities);
    parts.extend(metadata);
    parts.join("|")
}

fn is_word_token(tok: usize) -> bool {
    (t::USECASE..=t::IDENTIFIER).contains(&tok)
}

fn is_label_text(tok: usize) -> bool {
    (t::USECASE..=t::LABEL_SYMBOL).contains(&tok)
}

fn is_key(tok: usize) -> bool {
    tok == t::IDENTIFIER || tok == t::PLAIN_STRING
}

fn generated_id(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    for c in label.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            for _ in 0..c.len_utf16() {
                out.push('_');
            }
        }
    }
    out
}

fn occurrences(properties: &[DraftProperty]) -> Vec<MetadataOccurrence> {
    properties
        .iter()
        .map(|p| MetadataOccurrence {
            key: p.key.clone(),
            span: p.span,
            key_span: p.key_span,
            value_span: p.value_span,
        })
        .collect()
}

struct Item {
    id: String,
    label: DraftLabel,
    span: Span,
    generated: bool,
    shape: Option<UseCaseShape>,
    metadata: Option<Vec<DraftProperty>>,
    stereotype: Option<String>,
    stereotype_span: Option<Span>,
    classes: Vec<String>,
    class_spans: Vec<Span>,
    // Whether this mention says something about the element, and so declares it.
    explicit: bool,
    node_span: Span,
}

struct Tail {
    explicit_id: Option<String>,
    explicit_id_span: Option<Span>,
    rel_type: RelationshipType,
    arrow_type: usize,
    label: Option<DraftLabel>,
    minlen: usize,
    target: Item,
    span: Span,
}

struct Parser<'a> {
    src: &'a [char],
    types: Vec<usize>,
    starts: Vec<usize>,
    ends: Vec<usize>,
    flags: [u8; t::COUNT],
    p: usize,
    // Where the last token before the most recent line end stops, which is where its statement stops.
    last_end: usize,
    drafts: Drafts,
    parent: Option<DraftBoundary>,
    pending_json: Vec<(usize, usize)>,
    edge_count: usize,
    note_count: usize,
    line_offset: usize,
}

impl<'a> Parser<'a> {
    fn slice(&self, from: usize, to: usize) -> String {
        self.src[from..to].iter().collect()
    }

    fn image(&self, k: usize) -> String {
        self.slice(self.starts[k], self.ends[k])
    }

    fn span(&self, k: usize) -> Span {
        (self.starts[k], self.ends[k])
    }

    fn fail<T>(&self, detail: &str) -> Result<T> {
        let (line, column) = line_column(self.src, self.starts[self.p], self.line_offset);
        let loc = location_text(
            self.src,
            self.starts[self.p],
            self.ends[self.p],
            self.line_offset,
        );
        let mut err = PeleError::new(
            format!("Error parsing usecase diagram: {} at {}", detail, loc),
            ErrorCode::Syntax,
        )
        .of("usecase");
        err.line = line;
        err.column = column;
        Err(err)
    }

    fn mismatch<T>(&self, name: &str) -> Result<T> {
        self.fail(&format!(
            "Expecting token of type --> {} <-- but found --> '{}' <--",
            name,
            self.image(self.p)
        ))
    }

    fn expect(&mut self, expected: usize) -> Result<usize> {
        if self.types[self.p] != expected {
            return self.mismatch(TOKEN_NAMES[expected]);
        }
        let k = self.p;
        self.p += 1;
        Ok(k)
    }

    // Chevrotain lists every token sequence that would have been accepted.
    fn no_alternative<T>(&self, expected: &str) -> Result<T> {
        let list: Vec<String> = expected
            .split('|')
            .enumerate()
            .map(|(i, seq)| format!("  {}. [{}]", i + 1, seq))
            .collect();
        self.fail(&format!(
            "Expecting: one of these possible Token sequences:\n{}\nbut found: '{}'",
            list.join("\n"),
            self.image(self.p)
        ))
    }

    fn line_end(&mut self) -> Result<()> {
        self.last_end = self.ends[self.p - 1];
        if self.types[self.p] == t::NEWLINE {
            self.p += 1;
            Ok(())
        } else if self.types[self.p] == t::EOF {
            Ok(())
        } else {
            self.no_alternative("NEWLINE|EOF")
        }
    }

    // The text of a name or string token, without its quotes.
    fn content(&self, k: usize) -> Span {
        let trim = if self.types[k] == t::MARKDOWN_STRING {
            2
        } else if self.types[k] == t::PLAIN_STRING {
            1
        } else {
            0
        };
        (self.starts[k] + trim, self.ends[k] - trim)
    }

    fn token_label(&self, k: usize) -> DraftLabel {
        let at = self.content(k);
        DraftLabel {
            text: self.slice(at.0, at.1),
            label_type: if self.types[k] == t::MARKDOWN_STRING {
                LabelType::Markdown
            } else {
                LabelType::Text
            },
            span: at,
        }
    }

    // An unquoted label runs to the next delimiter and is taken from the source as written.
    fn label(&mut self) -> Result<DraftLabel> {
        let tok = self.types[self.p];
        if tok == t::PLAIN_STRING || tok == t::MARKDOWN_STRING {
            let k = self.p;
            self.p += 1;
            return Ok(self.token_label(k));
        }
        if !is_label_text(tok) {
            return self.no_alternative(LABELS);
        }
        let first = self.p;
        while is_label_text(self.types[self.p]) {
            self.p += 1;
        }
        let at: Span = (self.starts[first], self.ends[self.p - 1]);
        Ok(DraftLabel {
            text: self.slice(at.0, at.1),
            label_type: LabelType::Text,
            span: at,
        })
    }

    fn property(&mut self) -> Result<DraftProperty> {
        let key = self.p;
        if !is_key(self.types[self.p]) {
            return self.no_alternative("IDENTIFIER|PLAIN_STRING");
        }
        self.p += 1;
        self.expect(t::COLON)?;
        let value = self.p;
        let tok = self.types[self.p];
        if !is_key(tok) && tok != t::TRUE && tok != t::FALSE {
            return self.no_alternative("IDENTIFIER|PLAIN_STRING|TRUE|FALSE");
        }
        self.p += 1;
        let key_span = self.content(key);
        let value_span = self.content(value);
        let prop_val = if tok == t::TRUE {
            DraftPropertyValue::Bool(true)
        } else if tok == t::FALSE {
            DraftPropertyValue::Bool(false)
        } else {
            DraftPropertyValue::Str(self.slice(value_span.0, value_span.1))
        };
        Ok(DraftProperty {
            key: self.slice(key_span.0, key_span.1),
            value: prop_val,
            span: (self.starts[key], self.ends[value]),
            key_span,
            value_span,
        })
    }

    // Whether another property follows. A comma or line breaks before the closing brace do not count,
    // but Chevrotain looks only three tokens ahead, so a comma and two line breaks, or three line
    // breaks, are taken for a separator and the brace after them is then an error.
    fn separator_ahead(&self) -> bool {
        let a = self.types[self.p];
        let b = self.types[self.p + 1];
        let c = self.types[self.p + 2];
        if a == t::COMMA {
            return is_key(b) || (b == t::NEWLINE && (c == t::NEWLINE || is_key(c)));
        }
        if a != t::NEWLINE {
            return false;
        }
        b == t::COMMA
            || is_key(b)
            || (b == t::NEWLINE && (c == t::NEWLINE || c == t::COMMA || is_key(c)))
    }

    fn metadata(&mut self) -> Result<Vec<DraftProperty>> {
        self.expect(t::METADATA_START)?;
        let mut properties = Vec::new();
        while self.types[self.p] == t::NEWLINE {
            self.p += 1;
        }
        if is_key(self.types[self.p]) {
            properties.push(self.property()?);
            while self.separator_ahead() {
                if self.types[self.p] == t::COMMA {
                    self.p += 1;
                } else {
                    while self.types[self.p] == t::NEWLINE {
                        self.p += 1;
                    }
                    if self.types[self.p] == t::COMMA {
                        self.p += 1;
                    }
                }
                while self.types[self.p] == t::NEWLINE {
                    self.p += 1;
                }
                properties.push(self.property()?);
            }
            if self.types[self.p] == t::COMMA {
                self.p += 1;
            }
            while self.types[self.p] == t::NEWLINE {
                self.p += 1;
            }
        }
        self.expect(t::RBRACE)?;
        Ok(properties)
    }

    fn class_suffix(&mut self, classes: &mut Vec<String>, spans: &mut Vec<Span>) -> Result<()> {
        loop {
            self.p += 1;
            let id = self.expect(t::IDENTIFIER)?;
            classes.push(self.image(id));
            spans.push(self.span(id));
            if self.types[self.p] != t::COMMA {
                break;
            }
        }
        Ok(())
    }

    // An id with an optional label in parentheses or brackets, or a string that stands for both.
    fn name(&mut self, brackets: bool) -> Result<Item> {
        let k = self.p;
        let tok = self.types[self.p];
        let span = self.content(k);
        let generated = tok != t::IDENTIFIER;
        let (id, lbl, shape) = if tok == t::IDENTIFIER {
            self.p += 1;
            let id_str = self.image(k);
            let open = self.types[self.p];
            if open == t::LPAREN || (brackets && open == t::LBRACKET) {
                self.p += 1;
                let parsed_lbl = self.label()?;
                self.expect(if open == t::LPAREN {
                    t::RPAREN
                } else {
                    t::RBRACKET
                })?;
                let sh = if open == t::LPAREN {
                    UseCaseShape::Ellipse
                } else {
                    UseCaseShape::Rect
                };
                (id_str, parsed_lbl, Some(sh))
            } else {
                (id_str, self.token_label(k), None)
            }
        } else if tok == t::PLAIN_STRING || tok == t::MARKDOWN_STRING {
            self.p += 1;
            let parsed_lbl = self.token_label(k);
            let id_str = generated_id(&parsed_lbl.text);
            (id_str, parsed_lbl, None)
        } else {
            return self.no_alternative(NAMES);
        };

        Ok(Item {
            id,
            label: lbl,
            span,
            generated,
            shape,
            metadata: None,
            stereotype: None,
            stereotype_span: None,
            classes: Vec::new(),
            class_spans: Vec::new(),
            explicit: false,
            node_span: (0, 0),
        })
    }

    fn element(&mut self, actor: bool) -> Result<Item> {
        let first = self.p;
        let mut item = self.name(!actor)?;
        if actor {
            item.shape = None;
        }
        if self.types[self.p] == t::METADATA_START {
            item.metadata = Some(self.metadata()?);
        }
        if self.types[self.p] == t::STEREOTYPE_START {
            self.p += 1;
            let text = self.expect(t::STEREOTYPE_TEXT)?;
            self.expect(t::STEREOTYPE_END)?;
            item.stereotype = Some(self.image(text).trim().to_string());
            item.stereotype_span = Some(self.span(text));
        }
        if self.types[self.p] == t::CLASS_SEPARATOR {
            self.class_suffix(&mut item.classes, &mut item.class_spans)?;
        }
        item.explicit = item.shape.is_some()
            || item.metadata.is_some()
            || item.stereotype.as_ref().is_some_and(|s| !s.is_empty());
        item.node_span = (self.starts[first], self.ends[self.p - 1]);
        Ok(item)
    }

    fn make_draft(&self, item: &Item, kind: ElementKind) -> DraftElement {
        DraftElement {
            id: item.id.clone(),
            kind,
            label: item.label.clone(),
            span: item.span,
            generated: item.generated,
            parent: self.parent.clone(),
            shape: item.shape,
            metadata: item.metadata.clone(),
            stereotype: item.stereotype.clone(),
            classes: item.classes.clone(),
        }
    }

    fn make_endpoint(item: &Item, declares: bool) -> DraftEndpoint {
        DraftEndpoint {
            id: item.id.clone(),
            label: item.label.clone(),
            span: item.span,
            generated: item.generated,
            declares,
            classed: !item.classes.is_empty(),
        }
    }

    fn make_occurrence(item: &Item, defines: bool) -> NodeOccurrence {
        NodeOccurrence {
            id: item.id.clone(),
            span: item.node_span,
            id_span: item.span,
            label_span: Some(item.label.span),
            defines: if defines { Some(true) } else { None },
            stereotype_span: item.stereotype_span,
            metadata: item.metadata.as_deref().map(occurrences),
            class_spans: if item.class_spans.is_empty() {
                None
            } else {
                Some(item.class_spans.clone())
            },
        }
    }

    // Whether a label and then one of the operators that can close it come next.
    fn labeled_right(&self, closers: u8) -> bool {
        if !(t::USECASE..=t::MARKDOWN_STRING).contains(&self.types[self.p]) {
            return false;
        }
        let mut k = self.p + 1;
        loop {
            let tok = self.types[k];
            if tok == t::MARKERLESS_SOLID || (self.flags[tok] & closers) != 0 {
                return true;
            }
            if !(t::USECASE..=t::MARKDOWN_STRING).contains(&tok) {
                return false;
            }
            k += 1;
        }
    }

    // Each dash beyond the second asks for one more rank between the two ends.
    fn minimum_length(&self, k: usize) -> usize {
        let len = self.ends[k] - self.starts[k];
        let sub = if self.types[k] == t::MARKERLESS_SOLID {
            0
        } else {
            1
        };
        len.saturating_sub(sub + 1).max(1)
    }

    fn relation_tail(&mut self) -> Result<Tail> {
        let first = self.p;
        let mut explicit_id = None;
        let mut explicit_id_span = None;
        if self.types[self.p] == t::IDENTIFIER && self.types[self.p + 1] == t::AT {
            explicit_id = Some(self.image(self.p));
            explicit_id_span = Some(self.span(self.p));
            self.p += 2;
        }
        let k = self.p;
        let tok = self.types[self.p];
        self.p += 1;

        let mut rel_type = RelationshipType::Association;
        let mut arrow_type = arrow::SOLID_ARROW;
        let mut lbl = None;
        let mut minlen = 1usize;

        if tok == t::DEPENDENCY_ARROW {
            self.expect(t::COLON)?;
            if self.types[self.p] != t::INCLUDE && self.types[self.p] != t::EXTEND {
                return self.no_alternative("INCLUDE|EXTEND");
            }
            rel_type = if self.types[self.p] == t::INCLUDE {
                RelationshipType::Include
            } else {
                RelationshipType::Extend
            };
            lbl = Some(DraftLabel {
                text: rel_type.as_str().to_string(),
                label_type: LabelType::Text,
                span: self.span(self.p),
            });
            self.p += 1;
        } else if tok == t::GENERALIZATION {
            rel_type = RelationshipType::Generalization;
        } else if tok == t::FORWARD_SOLID {
            minlen = self.minimum_length(k);
        } else if tok == t::BACKWARD_SOLID {
            arrow_type = arrow::BACK_ARROW;
            // Only the shortest operator can open a label: `A <-- label -- B`.
            if self.ends[k] - self.starts[k] == 3 && self.labeled_right(0) {
                lbl = Some(self.label()?);
                let close = self.expect(t::MARKERLESS_SOLID)?;
                minlen = self.minimum_length(close);
            } else {
                minlen = self.minimum_length(k);
            }
        } else if tok == t::MARKERLESS_SOLID {
            arrow_type = arrow::LINE_SOLID;
            if self.ends[k] - self.starts[k] == 2 && self.labeled_right(CLOSES_LABEL) {
                lbl = Some(self.label()?);
                let close = self.types[self.p];
                if close == t::FORWARD_SOLID {
                    arrow_type = arrow::SOLID_ARROW;
                } else if close == t::FORWARD_CIRCLE {
                    arrow_type = arrow::CIRCLE_ARROW;
                } else if close == t::FORWARD_CROSS {
                    arrow_type = arrow::CROSS_ARROW;
                } else if close != t::MARKERLESS_SOLID {
                    return self.no_alternative(
                        "FORWARD_SOLID|MARKERLESS_SOLID|FORWARD_CIRCLE|FORWARD_CROSS",
                    );
                }
                if arrow_type == arrow::SOLID_ARROW || arrow_type == arrow::LINE_SOLID {
                    minlen = self.minimum_length(self.p);
                }
                self.p += 1;
            } else {
                minlen = self.minimum_length(k);
            }
        } else if tok == t::FORWARD_CIRCLE {
            arrow_type = arrow::CIRCLE_ARROW;
        } else if tok == t::FORWARD_CROSS {
            arrow_type = arrow::CROSS_ARROW;
        } else if tok == t::BACKWARD_CIRCLE || tok == t::BACKWARD_CROSS {
            arrow_type = if tok == t::BACKWARD_CIRCLE {
                arrow::CIRCLE_ARROW_REVERSED
            } else {
                arrow::CROSS_ARROW_REVERSED
            };
            if self.labeled_right(0) {
                lbl = Some(self.label()?);
                self.expect(t::MARKERLESS_SOLID)?;
            }
        } else {
            self.p -= 1;
            return self.no_alternative(ARROWS);
        }

        let target = self.element(false)?;
        let span = (self.starts[first], self.ends[self.p - 1]);
        Ok(Tail {
            explicit_id,
            explicit_id_span,
            rel_type,
            arrow_type,
            label: lbl,
            minlen,
            target,
            span,
        })
    }

    fn relationship(
        &mut self,
        source: &Item,
        declaration: bool,
        tail: Tail,
        mut nodes: Vec<NodeOccurrence>,
    ) -> GraphStatement {
        if tail.target.explicit {
            self.drafts
                .elements
                .push(self.make_draft(&tail.target, ElementKind::Usecase));
        }
        self.drafts.relationships.push(DraftRelationship {
            source: Self::make_endpoint(source, declaration),
            target: Self::make_endpoint(&tail.target, tail.target.explicit),
            span: tail.span,
            id: tail.explicit_id.clone(),
            id_span: tail.explicit_id_span,
            rel_type: tail.rel_type,
            arrow_type: tail.arrow_type,
            label: tail.label.clone(),
            minlen: tail.minlen,
        });
        nodes.push(Self::make_occurrence(&tail.target, tail.target.explicit));
        let edge_id = match tail.explicit_id {
            Some(id) => id,
            None => {
                let id = format!("edge-{}", self.edge_count);
                self.edge_count += 1;
                id
            }
        };
        let mut out = GraphStatement::new(StatementKind::Edge);
        out.nodes = Some(nodes);
        out.edges = Some(vec![EdgeOccurrence {
            id: edge_id,
            span: out.span,
            label_span: tail.label.map(|l| l.span),
            id_span: tail.explicit_id_span,
            metadata: None,
        }]);
        out
    }

    fn declaration(nodes: Vec<NodeOccurrence>) -> GraphStatement {
        let mut out = GraphStatement::new(StatementKind::Node);
        out.nodes = Some(nodes);
        out
    }

    // `actor A, B` declares several actors; at the top level `actor A --> B` declares one and relates it.
    fn actors(&mut self, top: bool) -> Result<GraphStatement> {
        self.p += 1;
        let mut items = vec![self.element(true)?];
        let mut tail = None;
        if self.types[self.p] == t::COMMA {
            while self.types[self.p] == t::COMMA {
                self.p += 1;
                items.push(self.element(true)?);
            }
        } else if top && (self.flags[self.types[self.p]] & RELATION) != 0 {
            tail = Some(self.relation_tail()?);
        }
        self.line_end()?;
        for item in &items {
            self.drafts
                .elements
                .push(self.make_draft(item, ElementKind::Actor));
        }
        let nodes: Vec<NodeOccurrence> = items
            .iter()
            .map(|item| Self::make_occurrence(item, true))
            .collect();
        Ok(match tail {
            Some(t_rel) => self.relationship(&items[0], true, t_rel, nodes),
            None => Self::declaration(nodes),
        })
    }

    fn entity(&mut self, top: bool) -> Result<GraphStatement> {
        let mut source = self.element(false)?;
        let tail = if top && (self.flags[self.types[self.p]] & RELATION) != 0 {
            Some(self.relation_tail()?)
        } else {
            None
        };
        self.line_end()?;
        if tail.is_none() {
            source.explicit = true;
            if source.shape.is_none() {
                source.shape = Some(UseCaseShape::Ellipse);
            }
        }
        if source.explicit {
            self.drafts
                .elements
                .push(self.make_draft(&source, ElementKind::Usecase));
        }
        let nodes = vec![Self::make_occurrence(&source, source.explicit)];
        Ok(match tail {
            Some(t_rel) => self.relationship(&source, source.explicit, t_rel, nodes),
            None => Self::declaration(nodes),
        })
    }

    fn boundary(&mut self) -> Result<GraphStatement> {
        self.p += 1;
        let mut named = self.name(true)?;
        let inline = if self.types[self.p] == t::METADATA_START {
            Some(self.metadata()?)
        } else {
            None
        };
        if self.types[self.p] == t::CLASS_SEPARATOR {
            self.class_suffix(&mut named.classes, &mut named.class_spans)?;
        }
        self.line_end()?;
        let group = DraftBoundary {
            id: named.id.clone(),
            label: named.label.clone(),
            span: named.span,
            generated: named.generated,
            classes: named.classes,
        };
        self.drafts.boundaries.push(group.clone());
        self.parent = Some(group.clone());
        let mut children = Vec::new();
        while (self.flags[self.types[self.p]] & BOUNDARY_LINE) != 0 {
            children.push(self.line(false, None)?);
        }
        self.parent = None;
        let end = self.expect(t::END)?;
        self.line_end()?;
        let mut out = GraphStatement::new(StatementKind::Group);
        out.group = Some(group.id.clone());
        out.id_span = Some(group.span);
        out.title_span = Some(group.label.span);
        out.end_span = Some(self.span(end));
        out.class_spans = Some(named.class_spans);
        if !children.is_empty() {
            out.children = Some(children);
        }
        if let Some(inline_props) = inline {
            out.metadata = Some(occurrences(&inline_props));
            self.drafts.metadata.push(DraftMetadata {
                target: group.id,
                span: group.span,
                properties: inline_props,
                stmt_index: None,
            });
        }
        Ok(out)
    }

    fn style_decl(&mut self) -> Result<String> {
        let first = self.p;
        let tok = self.types[self.p];
        if is_word_token(tok) || tok == t::CSS_IDENTIFIER {
            self.p += 1;
        } else if tok == t::MARKERLESS_SOLID {
            self.p += 1;
            if !is_word_token(self.types[self.p]) {
                return self.mismatch("WORD");
            }
            self.p += 1;
        } else {
            return self.no_alternative("WORD|CSS_IDENTIFIER|MARKERLESS_SOLID");
        }
        self.expect(t::COLON)?;
        if (self.flags[self.types[self.p]] & STYLE_PART) == 0 {
            return self.fail(&format!(
                "Expecting: expecting at least one iteration which starts with one of these possible Token sequences::\n  <[{}]>\nbut found: '{}'",
                STYLE_PARTS.replace('|', "] ,["),
                self.image(self.p)
            ));
        }
        while (self.flags[self.types[self.p]] & STYLE_PART) != 0 {
            self.p += 1;
        }
        // The value is taken from the source, so the spaces inside `1px solid red` survive.
        Ok(self
            .slice(self.starts[first], self.ends[self.p - 1])
            .replace("\\,", ","))
    }

    fn styles_list(&mut self) -> Result<Vec<String>> {
        let mut out = vec![self.style_decl()?];
        while self.types[self.p] == t::COMMA {
            self.p += 1;
            out.push(self.style_decl()?);
        }
        Ok(out)
    }

    fn identifiers(&mut self) -> Result<Vec<usize>> {
        let mut out = vec![self.expect(t::IDENTIFIER)?];
        while self.types[self.p] == t::COMMA {
            self.p += 1;
            out.push(self.expect(t::IDENTIFIER)?);
        }
        Ok(out)
    }

    fn reference(&self, k: usize) -> NodeOccurrence {
        let at = self.content(k);
        NodeOccurrence {
            id: self.image(k),
            span: at,
            id_span: at,
            label_span: None,
            defines: None,
            stereotype_span: None,
            metadata: None,
            class_spans: None,
        }
    }

    fn top_statement(&mut self, stmt_idx: Option<usize>) -> Result<GraphStatement> {
        let k = self.p;
        let tok = self.types[self.p];
        let mut out: GraphStatement;

        if tok == t::ACC_TITLE_LINE || tok == t::ACC_DESCR_LINE || tok == t::ACC_DESCR_BLOCK {
            self.p += 1;
            let text = self.image(k);
            if tok == t::ACC_DESCR_BLOCK {
                let open = text.find('{').map_or(0, |idx| idx + 1);
                let close = text.rfind('}').unwrap_or(text.len());
                self.drafts.acc_description = text[open..close].trim().to_string();
            } else if tok == t::ACC_DESCR_LINE {
                let colon = text.find(':').map_or(0, |idx| idx + 1);
                self.drafts.acc_description = text[colon..].trim().to_string();
            } else {
                let colon = text.find(':').map_or(0, |idx| idx + 1);
                self.drafts.acc_title = text[colon..].trim().to_string();
            }
            out = GraphStatement::new(if tok == t::ACC_TITLE_LINE {
                StatementKind::AccTitle
            } else {
                StatementKind::AccDescr
            });
        } else if tok == t::DIRECTION {
            self.p += 1;
            let value = self.types[self.p];
            if !(t::TD..=t::RL).contains(&value) {
                return self.no_alternative("TD|TB|BT|LR|RL");
            }
            self.drafts.direction = Some(match value {
                t::TD | t::TB => Direction::TB,
                t::BT => Direction::BT,
                t::LR => Direction::LR,
                _ => Direction::RL,
            });
            self.p += 1;
            out = GraphStatement::new(StatementKind::Direction);
        } else if tok == t::ACTOR {
            return self.actors(true);
        } else if tok == t::SYSTEM_BOUNDARY {
            return self.boundary();
        } else if tok == t::NOTE {
            self.p += 1;
            self.expect(t::FOR)?;
            let target = self.expect(t::IDENTIFIER)?;
            let text = self.label()?;
            self.drafts.notes.push(DraftNote {
                target: self.image(target),
                span: self.span(target),
                label: text.clone(),
            });
            out = GraphStatement::new(StatementKind::Note);
            out.r#ref = Some(format!("note-{}", self.note_count));
            self.note_count += 1;
            out.ref_span = Some(text.span);
            out.nodes = Some(vec![self.reference(target)]);
        } else if tok == t::JSON_DECLARATION_START {
            self.p += 1;
            let literal = self.expect(t::JSON_OBJECT_LITERAL)?;
            let mut classes = Vec::new();
            let mut class_spans = Vec::new();
            if self.types[self.p] == t::CLASS_SEPARATOR {
                self.class_suffix(&mut classes, &mut class_spans)?;
            }
            let mut from = self.starts[k] + 4;
            while matches!(self.src.get(from), Some(' ' | '\t')) {
                from += 1;
            }
            let mut to = from;
            while self
                .src
                .get(to)
                .is_some_and(|&c| c.is_ascii_alphanumeric() || c == '_')
            {
                to += 1;
            }
            let id = self.slice(from, to);
            let json_span: Span = (from, to);
            let json_idx = self.drafts.jsons.len();
            self.drafts.jsons.push(DraftJson {
                id: id.clone(),
                value: Value::object(),
                property_order: OrdMap::new(),
                span: json_span,
                classes,
            });
            self.pending_json.push((json_idx, literal));
            out = GraphStatement::new(StatementKind::Json);
            out.nodes = Some(vec![NodeOccurrence {
                id,
                span: json_span,
                id_span: json_span,
                label_span: None,
                defines: Some(true),
                stereotype_span: None,
                metadata: None,
                class_spans: Some(class_spans.clone()),
            }]);
            out.class_spans = Some(class_spans);
        } else if tok == t::CLASS_DEF {
            self.p += 1;
            let ids = self.identifiers()?;
            let styles = self.styles_list()?;
            self.drafts.class_defs.push(DraftClassDef {
                ids: ids.iter().map(|&id| self.image(id)).collect(),
                styles,
            });
            out = GraphStatement::new(StatementKind::ClassDef);
            out.r#ref = Some(self.image(ids[0]));
            out.ref_span = Some(self.span(ids[0]));
        } else if tok == t::CLASS {
            self.p += 1;
            let targets = self.identifiers()?;
            let classes = self.identifiers()?;
            self.drafts.classes.push(DraftClassAssign {
                targets: targets
                    .iter()
                    .map(|&id| DraftClassTarget {
                        id: self.image(id),
                        span: self.span(id),
                    })
                    .collect(),
                classes: classes.iter().map(|&id| self.image(id)).collect(),
            });
            out = GraphStatement::new(StatementKind::ClassAssign);
            out.r#ref = Some(self.image(classes[0]));
            out.ref_span = Some(self.span(classes[0]));
            out.nodes = Some(targets.iter().map(|&id| self.reference(id)).collect());
        } else if tok == t::STYLE {
            self.p += 1;
            let target = self.expect(t::IDENTIFIER)?;
            let styles = self.styles_list()?;
            self.drafts.styles.push(DraftStyleAssign {
                target: self.image(target),
                span: self.span(target),
                styles,
            });
            out = GraphStatement::new(StatementKind::Style);
            out.nodes = Some(vec![self.reference(target)]);
        } else {
            let next = self.types[self.p + 1];
            if next == t::METADATA_START {
                if (self.flags[self.types[self.p + 2]] & METADATA_FIRST) == 0 {
                    return self.no_alternative(&statement_sequences());
                }
                self.p += 1;
                let mut target = self.reference(k);
                if tok != t::IDENTIFIER {
                    target.id = generated_id(&self.slice(target.span.0, target.span.1));
                }
                let assigned = self.metadata()?;
                out = GraphStatement::new(StatementKind::Metadata);
                let target_id = target.id.clone();
                let target_span = target.span;
                out.nodes = Some(vec![target]);
                out.metadata = Some(occurrences(&assigned));
                self.drafts.metadata.push(DraftMetadata {
                    target: target_id,
                    span: target_span,
                    properties: assigned,
                    stmt_index: stmt_idx,
                });
            } else {
                let follows = (self.flags[next] & FOLLOWS_NAME) != 0
                    || (tok == t::IDENTIFIER && (next == t::LPAREN || next == t::LBRACKET));
                // Mermaid turns away the PlantUML statements people are most likely to try.
                if !follows || (tok == t::IDENTIFIER && is_forbidden(&self.image(k))) {
                    return self.no_alternative(&statement_sequences());
                }
                return self.entity(true);
            }
        }
        self.line_end()?;
        Ok(out)
    }

    fn line(&mut self, top: bool, stmt_idx: Option<usize>) -> Result<GraphStatement> {
        let k = self.p;
        let tok = self.types[self.p];
        if tok == t::NEWLINE {
            self.p += 1;
            let mut out = GraphStatement::new(StatementKind::Blank);
            out.span = self.span(k);
            return Ok(out);
        }
        if tok == t::COMMENT {
            self.p += 1;
            self.line_end()?;
            let mut out = GraphStatement::new(StatementKind::Comment);
            out.span = self.span(k);
            return Ok(out);
        }
        let mut out = if top {
            self.top_statement(stmt_idx)?
        } else if tok == t::ACTOR {
            self.actors(false)?
        } else {
            self.entity(false)?
        };
        out.span = (self.starts[k], self.last_end);
        if let Some(ref mut edges) = out.edges {
            for edge in edges {
                edge.span = out.span;
            }
        }
        Ok(out)
    }
}

pub fn parse_usecase(
    source: &str,
    title: Option<String>,
    line_offset: usize,
) -> Result<UsecaseModel> {
    let src: Vec<char> = source.chars().collect();
    let tokens = tokenize(&src, line_offset)?;
    let mut parser = Parser {
        src: &src,
        types: tokens.types,
        starts: tokens.starts,
        ends: tokens.ends,
        flags: build_flags(),
        p: 0,
        last_end: 0,
        drafts: empty_drafts(),
        parent: None,
        pending_json: Vec::new(),
        edge_count: 0,
        note_count: 0,
        line_offset,
    };

    let header = parser.expect(t::USECASE)?;
    parser.line_end()?;
    let mut statements = Vec::new();
    while (parser.flags[parser.types[parser.p]] & LINE) != 0 {
        let idx = statements.len();
        let stmt = parser.line(true, Some(idx))?;
        statements.push(stmt);
    }
    if parser.types[parser.p] != t::EOF {
        return parser.fail(&format!(
            "Redundant input, expecting EOF but found: {}",
            parser.image(parser.p)
        ));
    }

    // The whole text has to be well formed before the JSON in it is looked at, as in Mermaid.
    for &(json_idx, literal) in &parser.pending_json {
        let lit_text = parser.image(literal);
        let lit_start = parser.starts[literal];
        let parsed = parse_ordered_json(&lit_text, || line_column(&src, lit_start, line_offset))?;
        parser.drafts.jsons[json_idx].value = parsed.value;
        parser.drafts.jsons[json_idx].property_order = parsed.property_order;
    }

    let header_span = parser.span(header);
    resolve_usecase(
        source,
        &src,
        parser.drafts,
        header_span,
        statements,
        title,
        line_offset,
    )
}
