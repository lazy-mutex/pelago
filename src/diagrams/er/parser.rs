use super::db::ErDb;
use super::lexer::{tokenize, Tokens};
use super::tokens::{t, TOKEN_NAMES};
use super::types::{Cardinality, DocItem, Identification, ParsedAttribute, RelSpec};
use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

const DIRECTIONS: &[&str; 4] = &["TB", "BT", "RL", "LR"];

const NAME: &str = "'UNICODE_TEXT', 'NUM', 'ENTITY_NAME', 'DECIMAL_NUM', 'ENTITY_ONE'";
const CARDS: &str = "'ZERO_OR_ONE', 'ZERO_OR_MORE', 'ONE_OR_MORE', 'ONLY_ONE', 'MD_PARENT'";
const LINE_HEAD: &str = "'EOF', 'NEWLINE', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'SUBGRAPH', 'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'CLASSDEF', 'CLASS', 'STYLE', ";

fn line_expected() -> String {
    format!("{}{}", LINE_HEAD, NAME)
}

fn cardinality_of(tok: i32) -> Option<Cardinality> {
    match tok {
        t::ZERO_OR_ONE => Some(Cardinality::ZeroOrOne),
        t::ZERO_OR_MORE => Some(Cardinality::ZeroOrMore),
        t::ONE_OR_MORE => Some(Cardinality::OneOrMore),
        t::ONLY_ONE => Some(Cardinality::OnlyOne),
        t::MD_PARENT => Some(Cardinality::MdParent),
        _ => None,
    }
}

fn is_name(tok: i32) -> bool {
    matches!(tok, t::UNICODE_TEXT | t::ENTITY_NAME | t::NUM | t::DECIMAL_NUM | t::ENTITY_ONE)
}

fn is_style_component(tok: i32) -> bool {
    matches!(tok, t::STYLE_TEXT | t::NUM | t::COLON | t::BRKT)
}

fn unquote(text: &str) -> String {
    let mut chars = text.chars();
    chars.next();
    chars.next_back();
    chars.as_str().to_string()
}

struct Frame {
    id: String,
    title: String,
    // What Mermaid's grammar hands to add_sub_graph: the value of every statement inside. Besides entity
    // names that includes accessibility text and the keywords `style`, `class` and `classDef`.
    list: Vec<DocItem>,
}

struct Parser<'a> {
    src: &'a [char],
    tokens: Tokens,
    i: usize,
}

impl<'a> Parser<'a> {
    fn fail(&self, expected: &str) -> PeleError {
        let tok = self.tokens.types[self.i];
        if tok == t::ERROR {
            return syntax_error("er", self.src, self.tokens.starts[self.i], "", true);
        }
        let got = if tok == t::CHAR {
            self.tokens.texts[self.i].as_str()
        } else {
            TOKEN_NAMES[tok as usize]
        };
        syntax_error(
            "er",
            self.src,
            self.tokens.starts[self.i],
            &format!("Expecting {}, got '{}'", expected, got),
            false,
        )
    }

    fn expect(&mut self, ty: i32) -> Result<String> {
        if self.tokens.types[self.i] != ty {
            return Err(self.fail(&format!("'{}'", TOKEN_NAMES[ty as usize])));
        }
        let s = self.tokens.texts[self.i].clone();
        self.i += 1;
        Ok(s)
    }

    fn separator(&mut self) -> Result<()> {
        let tok = self.tokens.types[self.i];
        if tok != t::NEWLINE && tok != t::SEMI && tok != t::EOF {
            return Err(self.fail("'EOF', 'NEWLINE', 'SEMI'"));
        }
        self.i += 1;
        Ok(())
    }

    fn entity_name(&mut self) -> Result<String> {
        let tok = self.tokens.types[self.i];
        if !is_name(tok) {
            return Err(self.fail(NAME));
        }
        let text = &self.tokens.texts[self.i];
        let out = if tok == t::ENTITY_NAME { unquote(text) } else { text.clone() };
        self.i += 1;
        Ok(out)
    }

    fn id_list(&mut self) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        loop {
            let tok = self.tokens.types[self.i];
            if tok != t::UNICODE_TEXT && tok != t::STYLE_TEXT {
                return Err(self.fail("'UNICODE_TEXT', 'STYLE_TEXT'"));
            }
            ids.push(self.tokens.texts[self.i].clone());
            self.i += 1;
            if self.tokens.types[self.i] != t::COMMA {
                return Ok(ids);
            }
            self.i += 1;
        }
    }

    fn styles_opt(&mut self) -> Result<Vec<String>> {
        let mut styles = Vec::new();
        loop {
            if !is_style_component(self.tokens.types[self.i]) {
                return Err(self.fail("'STYLE_TEXT', 'COLON', 'NUM', 'BRKT'"));
            }
            let mut style = self.tokens.texts[self.i].clone();
            self.i += 1;
            while is_style_component(self.tokens.types[self.i]) {
                style.push_str(&self.tokens.texts[self.i]);
                self.i += 1;
            }
            styles.push(style);
            if self.tokens.types[self.i] != t::COMMA {
                return Ok(styles);
            }
            self.i += 1;
        }
    }

    fn rel_spec(&mut self) -> Result<RelSpec> {
        let card_b = cardinality_of(self.tokens.types[self.i]).unwrap();
        self.i += 1;
        let r = self.tokens.types[self.i];
        if r != t::IDENTIFYING && r != t::NON_IDENTIFYING {
            return Err(self.fail("'NON_IDENTIFYING', 'IDENTIFYING'"));
        }
        let rel_type = if r == t::IDENTIFYING {
            Identification::Identifying
        } else {
            Identification::NonIdentifying
        };
        self.i += 1;
        let Some(card_a) = cardinality_of(self.tokens.types[self.i]) else {
            return Err(self.fail(CARDS));
        };
        self.i += 1;
        Ok(RelSpec { card_a, rel_type, card_b })
    }

    fn role(&mut self) -> Result<String> {
        let tok = self.tokens.types[self.i];
        if tok != t::WORD && tok != t::ENTITY_NAME && tok != t::UNICODE_TEXT {
            return Err(self.fail("'UNICODE_TEXT', 'ENTITY_NAME', 'WORD'"));
        }
        let text = &self.tokens.texts[self.i];
        let out = if tok == t::UNICODE_TEXT { text.clone() } else { unquote(text) };
        self.i += 1;
        Ok(out)
    }

    fn is_char(&self, ch: &str) -> bool {
        self.tokens.types[self.i] == t::CHAR && self.tokens.texts[self.i] == ch
    }

    // Returns the attributes last to first, as Mermaid's right-recursive rule builds them.
    fn attributes(&mut self) -> Result<Vec<ParsedAttribute>> {
        let mut list = Vec::new();
        loop {
            let mut attr_type = self.expect(t::ATTRIBUTE_WORD)?;
            if self.is_char("?") {
                attr_type.push_str(&self.tokens.texts[self.i]);
                self.i += 1;
            }
            if self.tokens.types[self.i] != t::ATTRIBUTE_WORD {
                return Err(self.fail(if attr_type.ends_with('?') {
                    "'ATTRIBUTE_WORD'"
                } else {
                    "'ATTRIBUTE_WORD', '?'"
                }));
            }
            let name = self.tokens.texts[self.i].clone();
            self.i += 1;
            let keys = if self.tokens.types[self.i] == t::ATTRIBUTE_KEY {
                let mut ks = vec![self.tokens.texts[self.i].clone()];
                self.i += 1;
                while self.is_char(",") {
                    self.i += 1;
                    ks.push(self.expect(t::ATTRIBUTE_KEY)?);
                }
                Some(ks)
            } else {
                None
            };
            let comment = if self.tokens.types[self.i] == t::COMMENT {
                let c = unquote(&self.tokens.texts[self.i]);
                self.i += 1;
                Some(c)
            } else {
                None
            };
            list.push(ParsedAttribute { attr_type, name, keys, comment });
            if self.tokens.types[self.i] == t::BLOCK_STOP {
                break;
            }
        }
        list.reverse();
        Ok(list)
    }

    // Parses an optional `{ ... }` block; None when there is none or it is empty.
    fn block(&mut self) -> Result<Option<Vec<ParsedAttribute>>> {
        if self.tokens.types[self.i] != t::BLOCK_START {
            return Ok(None);
        }
        self.i += 1;
        if self.tokens.types[self.i] == t::BLOCK_STOP {
            self.i += 1;
            return Ok(None);
        }
        let list = self.attributes()?;
        self.i += 1;
        Ok(Some(list))
    }

    fn entity_statement(&mut self, db: &mut ErDb) -> Result<Vec<String>> {
        let a = self.entity_name()?;
        let mut tok = self.tokens.types[self.i];
        if tok == t::SQS {
            self.i += 1;
            let alias = self.entity_name()?;
            self.expect(t::SQE)?;
            let classes = if self.tokens.types[self.i] == t::STYLE_SEPARATOR {
                self.i += 1;
                Some(self.id_list()?)
            } else {
                None
            };
            let attrs = self.block()?;
            db.add_entity(&a, &alias);
            if let Some(attrs) = attrs {
                db.add_attributes(&a, attrs);
            }
            if let Some(classes) = classes {
                db.set_class(std::slice::from_ref(&a), &classes);
            }
            return Ok(vec![a]);
        }
        let classes = if tok == t::STYLE_SEPARATOR {
            self.i += 1;
            let cls = self.id_list()?;
            tok = self.tokens.types[self.i];
            Some(cls)
        } else {
            None
        };
        if cardinality_of(tok).is_some() {
            let spec = self.rel_spec()?;
            let b = self.entity_name()?;
            let classes_b = if self.tokens.types[self.i] == t::STYLE_SEPARATOR {
                self.i += 1;
                Some(self.id_list()?)
            } else {
                None
            };
            self.expect(t::COLON)?;
            let label = self.role()?;
            db.add_entity(&a, "");
            db.add_entity(&b, "");
            db.add_relationship(&a, label, &b, spec);
            if let Some(classes) = classes {
                db.set_class(std::slice::from_ref(&a), &classes);
            }
            if let Some(classes_b) = classes_b {
                db.set_class(std::slice::from_ref(&b), &classes_b);
            }
            return Ok(vec![a, b]);
        }
        let attrs = self.block()?;
        db.add_entity(&a, "");
        if let Some(attrs) = attrs {
            db.add_attributes(&a, attrs);
        }
        if let Some(classes) = classes {
            db.set_class(std::slice::from_ref(&a), &classes);
        }
        Ok(vec![a])
    }
}

pub fn parse_er(source: &str, db: &mut ErDb) -> Result<()> {
    let src: Vec<char> = source.chars().collect();
    let tokens = tokenize(&src);
    let mut p = Parser { src: &src, tokens, i: 0 };

    p.expect(t::ER_DIAGRAM)?;
    // Subgraphs nest on this stack rather than on the call stack; the first frame is the diagram itself.
    let mut frames: Vec<Frame> = vec![Frame { id: String::new(), title: String::new(), list: Vec::new() }];

    loop {
        let tok = p.tokens.types[p.i];
        let nested = frames.len() > 1;
        match tok {
            t::NEWLINE => {
                p.i += 1;
            }
            t::EOF => {
                p.i += 1;
                if nested {
                    return Err(p.fail(&format!("'END', {}", line_expected())));
                }
                return Ok(());
            }
            t::END_KW => {
                if !nested {
                    return Err(p.fail(&line_expected()));
                }
                p.i += 1;
                let top = frames.pop().unwrap();
                let id = db.add_sub_graph(&top.id, top.list, &top.title, None);
                if frames.len() > 1 {
                    frames.last_mut().unwrap().list.push(DocItem::Str(id));
                }
            }
            t::SUBGRAPH => {
                p.i += 1;
                let id = p.entity_name()?;
                let mut title = id.clone();
                if p.tokens.types[p.i] == t::SQS {
                    p.i += 1;
                    title = p.entity_name()?;
                    while is_name(p.tokens.types[p.i]) {
                        title.push(' ');
                        title.push_str(&p.entity_name()?);
                    }
                    p.expect(t::SQE)?;
                }
                p.separator()?;
                frames.push(Frame { id, title, list: Vec::new() });
            }
            t::ACC_TITLE => {
                p.i += 1;
                let value = p.expect(t::ACC_TITLE_VALUE)?.trim_matches(is_space).to_string();
                db.set_acc_title(&value);
                if nested {
                    frames.last_mut().unwrap().list.push(DocItem::Str(value));
                }
            }
            t::ACC_DESCR => {
                p.i += 1;
                let value = p.expect(t::ACC_DESCR_VALUE)?.trim_matches(is_space).to_string();
                db.set_acc_description(&value);
                if nested {
                    frames.last_mut().unwrap().list.push(DocItem::Str(value));
                }
            }
            t::ACC_DESCR_MULTILINE_VALUE => {
                let value = p.tokens.texts[p.i].trim_matches(is_space).to_string();
                p.i += 1;
                db.set_acc_description(&value);
                if nested {
                    frames.last_mut().unwrap().list.push(DocItem::Str(value));
                }
            }
            t::DIRECTION_TB | t::DIRECTION_BT | t::DIRECTION_RL | t::DIRECTION_LR => {
                p.i += 1;
                let value = DIRECTIONS[(tok - t::DIRECTION_TB) as usize];
                if nested {
                    frames.last_mut().unwrap().list.push(DocItem::Dir(value.to_string()));
                } else {
                    db.set_direction(value);
                }
            }
            t::CLASSDEF => {
                let keyword = p.tokens.texts[p.i].clone();
                p.i += 1;
                let ids = p.id_list()?;
                let styles = p.styles_opt()?;
                p.separator()?;
                db.add_class(&ids, &styles);
                if nested {
                    frames.last_mut().unwrap().list.push(DocItem::Str(keyword));
                }
            }
            t::CLASS => {
                let keyword = p.tokens.texts[p.i].clone();
                p.i += 1;
                let ids = p.id_list()?;
                let classes = p.id_list()?;
                db.set_class(&ids, &classes);
                if nested {
                    frames.last_mut().unwrap().list.push(DocItem::Str(keyword));
                }
            }
            t::STYLE => {
                let keyword = p.tokens.texts[p.i].clone();
                p.i += 1;
                let ids = p.id_list()?;
                let styles = p.styles_opt()?;
                p.separator()?;
                db.add_css_styles(&ids, &styles);
                if nested {
                    frames.last_mut().unwrap().list.push(DocItem::Str(keyword));
                }
            }
            _ => {
                if !is_name(tok) {
                    let line = line_expected();
                    return Err(p.fail(&if nested { format!("'END', {}", line) } else { line }));
                }
                let names = p.entity_statement(db)?;
                if nested {
                    let list = &mut frames.last_mut().unwrap().list;
                    for name in names {
                        list.push(DocItem::Str(name));
                    }
                }
            }
        }
    }
}
