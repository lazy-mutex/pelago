use super::db::VennDb;
use super::lexer::{t, VennLexer, TOKEN_NAMES};
use crate::errors::{syntax_error, ErrorCode, PeleError, Result};

const STATEMENT: &str = "'EOF', 'NEWLINE', 'TITLE', 'SET', 'UNION', 'TEXT', 'INDENT_TEXT', 'STYLE'";
const VALUE: &str = "'NUMERIC', 'IDENTIFIER', 'STRING', 'HEXCOLOR', 'RGBCOLOR', 'RGBACOLOR'";

struct Parser<'a> {
    src: &'a [char],
    lexer: VennLexer<'a>,
    cur: Option<usize>,
}

impl<'a> Parser<'a> {
    fn new(src: &'a [char]) -> Self {
        Self {
            src,
            lexer: VennLexer::new(src),
            cur: None,
        }
    }

    fn peek(&mut self, db: &mut VennDb) -> usize {
        if let Some(tok) = self.cur {
            tok
        } else {
            let tok = self.lexer.next(db);
            self.cur = Some(tok);
            tok
        }
    }

    fn fail(&mut self, db: &mut VennDb, expected: &str) -> PeleError {
        let tok = self.peek(db);
        if tok == t::ERROR {
            syntax_error("venn", self.src, self.lexer.start, "", true)
        } else {
            syntax_error(
                "venn",
                self.src,
                self.lexer.start,
                &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok]),
                false,
            )
        }
    }

    fn take(&mut self) -> String {
        self.cur = None;
        self.lexer.text.clone()
    }

    fn expect(&mut self, db: &mut VennDb, wanted: usize) -> Result<String> {
        if self.peek(db) != wanted {
            return Err(self.fail(db, &format!("'{}'", TOKEN_NAMES[wanted])));
        }
        Ok(self.take())
    }

    fn identifier(&mut self, db: &mut VennDb) -> Result<String> {
        let tok = self.peek(db);
        if tok != t::IDENTIFIER && tok != t::STRING {
            return Err(self.fail(db, "'IDENTIFIER', 'STRING'"));
        }
        Ok(self.take())
    }

    fn identifiers(&mut self, db: &mut VennDb) -> Result<Vec<String>> {
        let mut list = vec![self.identifier(db)?];
        while self.peek(db) == t::COMMA {
            self.take();
            list.push(self.identifier(db)?);
        }
        Ok(list)
    }

    fn subset(&mut self, db: &mut VennDb, ids: Vec<String>, union: bool) -> Result<()> {
        let label = if self.peek(db) == t::BRACKET_LABEL {
            Some(self.take())
        } else {
            None
        };
        let size = if self.peek(db) == t::COLON {
            self.take();
            let num_str = self.expect(db, t::NUMERIC)?;
            Some(num_str.parse::<f64>().unwrap_or(f64::NAN))
        } else {
            None
        };
        if union {
            if ids.len() < 2 {
                return Err(PeleError::new("union requires multiple identifiers", ErrorCode::Semantic).of("venn"));
            }
            db.validate_union_identifiers(&ids)?;
        }
        db.add_subset_data(&ids, label, size);
        db.set_indent_mode(true);
        Ok(())
    }

    fn text_stmt(&mut self, db: &mut VennDb, explicit_sets: Option<Vec<String>>) -> Result<()> {
        let kind = self.peek(db);
        if kind != t::IDENTIFIER && kind != t::STRING && kind != t::NUMERIC {
            return Err(self.fail(db, "'NUMERIC', 'IDENTIFIER', 'STRING'"));
        }
        let id = self.take();
        let label = if kind != t::NUMERIC && self.peek(db) == t::BRACKET_LABEL {
            Some(self.take())
        } else {
            None
        };
        let sets = match explicit_sets {
            Some(s) => s,
            None => match db.get_current_sets() {
                Some(cur) => cur.to_vec(),
                None => return Err(PeleError::new("text requires set", ErrorCode::Semantic).of("venn")),
            },
        };
        db.add_text_data(&sets, &id, label);
        Ok(())
    }

    fn style_stmt(&mut self, db: &mut VennDb) -> Result<()> {
        let ids = self.identifiers(db)?;
        let mut styles: Vec<(String, String)> = Vec::new();
        loop {
            let key = self.expect(db, t::IDENTIFIER)?;
            self.expect(db, t::COLON)?;
            let value = if self.peek(db) == t::STRING {
                self.take()
            } else {
                let mut parts: Vec<String> = Vec::new();
                loop {
                    let tok = self.peek(db);
                    if tok == t::IDENTIFIER || (tok >= t::NUMERIC && tok <= t::RGBACOLOR) {
                        parts.push(self.take());
                    } else {
                        break;
                    }
                }
                if parts.is_empty() {
                    return Err(self.fail(db, VALUE));
                }
                parts.join(" ")
            };
            styles.push((key, value));
            if self.peek(db) != t::COMMA {
                break;
            }
            self.take();
        }
        db.add_style_data(&ids, &styles);
        Ok(())
    }
}

pub fn parse_venn(src_str: &str, db: &mut VennDb) -> Result<()> {
    let src: Vec<char> = src_str.chars().collect();
    let mut p = Parser::new(&src);

    while p.peek(db) == t::NEWLINE {
        p.take();
    }
    p.expect(db, t::VENN)?;
    loop {
        let statement = p.peek(db);
        if statement == t::EOF {
            return Ok(());
        }
        if statement < t::NEWLINE || statement > t::STYLE || statement == t::VENN {
            return Err(p.fail(db, STATEMENT));
        }
        let tok_text = p.take();
        match statement {
            t::TITLE => db.set_diagram_title(tok_text.chars().skip(6).collect()),
            t::SET => {
                let id = p.identifier(db)?;
                p.subset(db, vec![id], false)?;
            }
            t::UNION => {
                let ids = p.identifiers(db)?;
                p.subset(db, ids, true)?;
            }
            t::TEXT => {
                let ids = p.identifiers(db)?;
                p.text_stmt(db, Some(ids))?;
            }
            t::INDENT_TEXT => {
                p.text_stmt(db, None)?;
            }
            t::STYLE => {
                p.style_stmt(db)?;
            }
            _ => {}
        }
    }
}
