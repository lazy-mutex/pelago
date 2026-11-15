use super::db::SeqDb;
use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};
use super::types::{linetype, placement, SeqItem, SeqNoteActor, SeqStatement, SeqText};
use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

const fn build_signal() -> [i32; t::COUNT] {
    let mut s = [-1i32; t::COUNT];
    s[t::SOLID_OPEN_ARROW] = linetype::SOLID_OPEN;
    s[t::DOTTED_OPEN_ARROW] = linetype::DOTTED_OPEN;
    s[t::SOLID_ARROW] = linetype::SOLID;
    s[t::DOTTED_ARROW] = linetype::DOTTED;
    s[t::BIDIRECTIONAL_SOLID_ARROW] = linetype::BIDIRECTIONAL_SOLID;
    s[t::BIDIRECTIONAL_DOTTED_ARROW] = linetype::BIDIRECTIONAL_DOTTED;
    s[t::SOLID_CROSS] = linetype::SOLID_CROSS;
    s[t::DOTTED_CROSS] = linetype::DOTTED_CROSS;
    s[t::SOLID_POINT] = linetype::SOLID_POINT;
    s[t::DOTTED_POINT] = linetype::DOTTED_POINT;
    s[t::SOLID_ARROW_TOP] = linetype::SOLID_TOP;
    s[t::SOLID_ARROW_BOTTOM] = linetype::SOLID_BOTTOM;
    s[t::STICK_ARROW_TOP] = linetype::STICK_TOP;
    s[t::STICK_ARROW_BOTTOM] = linetype::STICK_BOTTOM;
    s[t::SOLID_ARROW_TOP_DOTTED] = linetype::SOLID_TOP_DOTTED;
    s[t::SOLID_ARROW_BOTTOM_DOTTED] = linetype::SOLID_BOTTOM_DOTTED;
    s[t::STICK_ARROW_TOP_DOTTED] = linetype::STICK_TOP_DOTTED;
    s[t::STICK_ARROW_BOTTOM_DOTTED] = linetype::STICK_BOTTOM_DOTTED;
    s[t::SOLID_ARROW_TOP_REVERSE] = linetype::SOLID_ARROW_TOP_REVERSE;
    s[t::SOLID_ARROW_BOTTOM_REVERSE] = linetype::SOLID_ARROW_BOTTOM_REVERSE;
    s[t::STICK_ARROW_TOP_REVERSE] = linetype::STICK_ARROW_TOP_REVERSE;
    s[t::STICK_ARROW_BOTTOM_REVERSE] = linetype::STICK_ARROW_BOTTOM_REVERSE;
    s[t::SOLID_ARROW_TOP_REVERSE_DOTTED] = linetype::SOLID_ARROW_TOP_REVERSE_DOTTED;
    s[t::SOLID_ARROW_BOTTOM_REVERSE_DOTTED] = linetype::SOLID_ARROW_BOTTOM_REVERSE_DOTTED;
    s[t::STICK_ARROW_TOP_REVERSE_DOTTED] = linetype::STICK_ARROW_TOP_REVERSE_DOTTED;
    s[t::STICK_ARROW_BOTTOM_REVERSE_DOTTED] = linetype::STICK_ARROW_BOTTOM_REVERSE_DOTTED;
    s
}

const SIGNAL: [i32; t::COUNT] = build_signal();

#[derive(Clone, Copy, PartialEq, Eq)]
enum K {
    Top,
    Box,
    Loop,
    Rect,
    Opt,
    Break,
    Alt,
    Par,
    Critical,
}

#[derive(Clone, Copy)]
enum DividerKey {
    AltText,
    ParText,
    OptionText,
}

struct Frame {
    kind: K,
    text: String,
    start_type: i32,
    // The current section's statements. Slot 0 of a block's first section is kept for its start entry.
    list: Vec<SeqItem>,
    // Finished sections, each paired with the divider text that ended it.
    sections: Vec<(Vec<SeqItem>, String)>,
}

fn block_kind(tok: usize) -> Option<K> {
    match tok {
        t::r#loop => Some(K::Loop),
        t::rect => Some(K::Rect),
        t::opt => Some(K::Opt),
        t::r#break => Some(K::Break),
        t::alt => Some(K::Alt),
        t::par | t::par_over => Some(K::Par),
        t::critical => Some(K::Critical),
        t::r#box => Some(K::Box),
        _ => None,
    }
}

const SIGNAL_TOKENS: &str = "'SOLID_OPEN_ARROW', 'DOTTED_OPEN_ARROW', 'SOLID_ARROW', 'DOTTED_ARROW', 'SOLID_CROSS', 'DOTTED_CROSS', 'SOLID_POINT', 'DOTTED_POINT'";

fn stmt_item(s: SeqStatement) -> SeqItem {
    SeqItem::Stmt(Box::new(s))
}

struct Parser<'a, 'b> {
    src: &'a [char],
    types: Vec<usize>,
    texts: Vec<String>,
    starts: Vec<usize>,
    i: usize,
    db: &'b mut SeqDb,
}

impl<'a, 'b> Parser<'a, 'b> {
    fn fail<R>(&self, expected: &str) -> Result<R> {
        let tok = self.types[self.i];
        if tok == t::ERROR {
            Err(syntax_error(
                "sequence",
                self.src,
                self.starts[self.i],
                "",
                true,
            ))
        } else {
            Err(syntax_error(
                "sequence",
                self.src,
                self.starts[self.i],
                &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok]),
                false,
            ))
        }
    }

    fn err(&self, expected: &str) -> PeleError {
        let tok = self.types[self.i];
        if tok == t::ERROR {
            syntax_error("sequence", self.src, self.starts[self.i], "", true)
        } else {
            syntax_error(
                "sequence",
                self.src,
                self.starts[self.i],
                &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok]),
                false,
            )
        }
    }

    fn expect(&mut self, kind: usize) -> Result<String> {
        if self.types[self.i] != kind {
            return self.fail(&format!("'{}'", TOKEN_NAMES[kind]));
        }
        let out = std::mem::take(&mut self.texts[self.i]);
        self.i += 1;
        Ok(out)
    }

    fn actor(&mut self) -> Result<(String, SeqStatement)> {
        let tok = self.types[self.i];
        if tok != t::ACTOR && tok != t::link {
            return self.fail("'ACTOR'");
        }
        let name = std::mem::take(&mut self.texts[self.i]);
        self.i += 1;
        let stmt = SeqStatement {
            kind: "addParticipant",
            actor: Some(SeqNoteActor::Single(name.clone())),
            ..Default::default()
        };
        Ok((name, stmt))
    }

    fn text2(&mut self) -> Result<SeqText> {
        let raw = self.expect(t::TXT)?;
        let stripped: String = raw.trim_matches(is_space).chars().skip(1).collect();
        Ok(self.db.parse_message(&stripped))
    }

    fn participant(&mut self) -> Result<SeqStatement> {
        let tok = self.types[self.i];
        self.i += 1;
        if tok == t::destroy {
            let (_, mut node) = self.actor()?;
            self.expect(t::NEWLINE)?;
            node.kind = "destroyParticipant";
            return Ok(node);
        }
        let (_, mut node) = self.actor()?;
        if self.types[self.i] == t::CONFIG_START && self.types[self.i - 1] == t::ACTOR {
            self.i += 1;
            let content = self.expect(t::CONFIG_CONTENT)?;
            self.expect(t::CONFIG_END)?;
            node.config = Some(content.trim_matches(is_space).to_string());
        }
        node.draw = Some(
            if tok == t::participant {
                "participant"
            } else {
                "actor"
            }
            .to_string(),
        );
        if self.types[self.i] == t::AS {
            self.i += 1;
            let alias = self.expect(t::restOfLine)?;
            self.expect(t::NEWLINE)?;
            node.description = Some(self.db.parse_message(&alias));
        } else if self.types[self.i] == t::NEWLINE {
            self.i += 1;
        } else {
            return self.fail("'AS', 'NEWLINE'");
        }
        Ok(node)
    }

    fn signal(&mut self, from_name: String, from_stmt: SeqStatement) -> Result<SeqItem> {
        let mut central_from = false;
        if self.types[self.i] == t::CENTRAL {
            central_from = true;
            self.i += 1;
        }
        let signal_type = SIGNAL[self.types[self.i]];
        if signal_type < 0 {
            return self.fail(SIGNAL_TOKENS);
        }
        self.i += 1;
        let mark = self.types[self.i];
        if mark == t::CENTRAL || (!central_from && (mark == t::PLUS || mark == t::MINUS)) {
            self.i += 1;
        }
        let (to_name, to_stmt) = self.actor()?;
        let msg = self.text2()?;
        let mut message = SeqStatement {
            kind: "addMessage",
            from: Some(from_name.clone()),
            to: Some(to_name.clone()),
            signal_type: Some(signal_type),
            msg: Some(msg),
            ..Default::default()
        };
        let mut tail: Vec<SeqItem> = Vec::new();
        if central_from {
            let dual = mark == t::CENTRAL;
            message.activate = Some(dual);
            message.central_connection = Some(if dual {
                linetype::CENTRAL_CONNECTION_DUAL
            } else {
                linetype::CENTRAL_CONNECTION_REVERSE
            });
            if dual {
                tail.push(stmt_item(SeqStatement {
                    kind: "centralConnection",
                    signal_type: Some(linetype::CENTRAL_CONNECTION),
                    actor: Some(SeqNoteActor::Single(to_name)),
                    ..Default::default()
                }));
            }
            tail.push(stmt_item(SeqStatement {
                kind: "centralConnectionReverse",
                signal_type: Some(linetype::CENTRAL_CONNECTION_REVERSE),
                actor: Some(SeqNoteActor::Single(from_name)),
                ..Default::default()
            }));
        } else if mark == t::PLUS {
            message.activate = Some(true);
            tail.push(stmt_item(SeqStatement {
                kind: "activeStart",
                signal_type: Some(linetype::ACTIVE_START),
                actor: Some(SeqNoteActor::Single(to_name)),
                ..Default::default()
            }));
        } else if mark == t::MINUS {
            tail.push(stmt_item(SeqStatement {
                kind: "activeEnd",
                signal_type: Some(linetype::ACTIVE_END),
                actor: Some(SeqNoteActor::Single(from_name)),
                ..Default::default()
            }));
        } else if mark == t::CENTRAL {
            message.activate = Some(true);
            message.central_connection = Some(linetype::CENTRAL_CONNECTION);
            tail.push(stmt_item(SeqStatement {
                kind: "centralConnection",
                signal_type: Some(linetype::CENTRAL_CONNECTION),
                actor: Some(SeqNoteActor::Single(to_name)),
                ..Default::default()
            }));
        }
        let mut out = vec![stmt_item(from_stmt), stmt_item(to_stmt), stmt_item(message)];
        out.extend(tail);
        Ok(SeqItem::List(out))
    }

    fn menu(&mut self, kind: &'static str) -> Result<SeqItem> {
        self.i += 1;
        let (who_name, who_stmt) = self.actor()?;
        let txt = self.text2()?;
        Ok(SeqItem::List(vec![
            stmt_item(who_stmt),
            stmt_item(SeqStatement {
                kind,
                actor: Some(SeqNoteActor::Single(who_name)),
                text: Some(txt),
                ..Default::default()
            }),
        ]))
    }

    fn note(&mut self) -> Result<SeqItem> {
        self.i += 1;
        let tok = self.types[self.i];
        if tok == t::over {
            self.i += 1;
            let (first_name, first_stmt) = self.actor()?;
            let mut who = stmt_item(first_stmt);
            let mut ids = [first_name.clone(), first_name];
            if self.types[self.i] == t::COMMA {
                self.i += 1;
                let (second_name, second_stmt) = self.actor()?;
                who = SeqItem::List(vec![who, stmt_item(second_stmt)]);
                ids[1] = second_name;
            }
            let txt = self.text2()?;
            return Ok(SeqItem::List(vec![
                who,
                stmt_item(SeqStatement {
                    kind: "addNote",
                    placement: Some(placement::OVER),
                    actor: Some(SeqNoteActor::Pair(ids)),
                    text: Some(txt),
                    ..Default::default()
                }),
            ]));
        }
        if tok != t::left_of && tok != t::right_of {
            return self.fail("'over', 'left_of', 'right_of'");
        }
        self.i += 1;
        let (who_name, who_stmt) = self.actor()?;
        let pl = if tok == t::left_of {
            placement::LEFTOF
        } else {
            placement::RIGHTOF
        };
        let txt = self.text2()?;
        Ok(SeqItem::List(vec![
            stmt_item(who_stmt),
            stmt_item(SeqStatement {
                kind: "addNote",
                placement: Some(pl),
                actor: Some(SeqNoteActor::Single(who_name)),
                text: Some(txt),
                ..Default::default()
            }),
        ]))
    }

    fn autonumber(&mut self) -> Result<SeqStatement> {
        self.i += 1;
        let mut node = SeqStatement {
            kind: "sequenceIndex",
            sequence_visible: Some(true),
            signal_type: Some(linetype::AUTONUMBER),
            ..Default::default()
        };
        if self.types[self.i] == t::NUM {
            node.sequence_index = Some(self.texts[self.i].parse::<f64>().unwrap_or(f64::NAN));
            self.i += 1;
            node.sequence_index_step = if self.types[self.i] == t::NUM {
                let s = self.texts[self.i].parse::<f64>().unwrap_or(f64::NAN);
                self.i += 1;
                Some(s)
            } else {
                Some(1.0)
            };
        } else if self.types[self.i] == t::off {
            self.i += 1;
            node.sequence_visible = Some(false);
        }
        if self.types[self.i] != t::NEWLINE {
            return self.fail("'NEWLINE', 'NUM', 'off'");
        }
        self.i += 1;
        Ok(node)
    }

    // Folds the sections of an alt, par, or critical block from the last one back, as the grammar's
    // right recursion does, so the texts reach the model builder in the same order.
    fn fold(
        &self,
        mut frame: Frame,
        divider: &'static str,
        key: DividerKey,
        signal_type: i32,
    ) -> Vec<SeqItem> {
        let mut list = frame.list;
        while let Some((mut prev_section, div_text)) = frame.sections.pop() {
            let parsed = self.db.parse_message(&div_text);
            let mut head = SeqStatement {
                kind: divider,
                signal_type: Some(signal_type),
                ..Default::default()
            };
            match key {
                DividerKey::AltText => head.alt_text = Some(parsed),
                DividerKey::ParText => head.par_text = Some(parsed),
                DividerKey::OptionText => head.option_text = Some(parsed),
            }
            prev_section.push(stmt_item(head));
            prev_section.push(SeqItem::List(list));
            list = prev_section;
        }
        list
    }

    fn close(&self, mut frame: Frame) -> Vec<SeqItem> {
        let text = std::mem::take(&mut frame.text);
        match frame.kind {
            K::Box => {
                frame.list[0] = stmt_item(SeqStatement {
                    kind: "boxStart",
                    box_data: Some(self.db.parse_box_data(&text)),
                    ..Default::default()
                });
                frame.list.push(stmt_item(SeqStatement {
                    kind: "boxEnd",
                    box_text: Some(text),
                    ..Default::default()
                }));
                frame.list
            }
            K::Loop => {
                frame.list[0] = stmt_item(SeqStatement {
                    kind: "loopStart",
                    loop_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::LOOP_START),
                    ..Default::default()
                });
                frame.list.push(stmt_item(SeqStatement {
                    kind: "loopEnd",
                    signal_type: Some(linetype::LOOP_END),
                    ..Default::default()
                }));
                frame.list
            }
            K::Rect => {
                frame.list[0] = stmt_item(SeqStatement {
                    kind: "rectStart",
                    color: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::RECT_START),
                    ..Default::default()
                });
                frame.list.push(stmt_item(SeqStatement {
                    kind: "rectEnd",
                    color: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::RECT_END),
                    ..Default::default()
                }));
                frame.list
            }
            K::Opt => {
                frame.list[0] = stmt_item(SeqStatement {
                    kind: "optStart",
                    opt_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::OPT_START),
                    ..Default::default()
                });
                frame.list.push(stmt_item(SeqStatement {
                    kind: "optEnd",
                    opt_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::OPT_END),
                    ..Default::default()
                }));
                frame.list
            }
            K::Break => {
                frame.list[0] = stmt_item(SeqStatement {
                    kind: "breakStart",
                    break_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::BREAK_START),
                    ..Default::default()
                });
                frame.list.push(stmt_item(SeqStatement {
                    kind: "breakEnd",
                    opt_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::BREAK_END),
                    ..Default::default()
                }));
                frame.list
            }
            K::Alt => {
                let mut list = self.fold(frame, "else", DividerKey::AltText, linetype::ALT_ELSE);
                list[0] = stmt_item(SeqStatement {
                    kind: "altStart",
                    alt_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::ALT_START),
                    ..Default::default()
                });
                list.push(stmt_item(SeqStatement {
                    kind: "altEnd",
                    signal_type: Some(linetype::ALT_END),
                    ..Default::default()
                }));
                list
            }
            K::Par => {
                let start_type = frame.start_type;
                let mut list = self.fold(frame, "and", DividerKey::ParText, linetype::PAR_AND);
                list[0] = stmt_item(SeqStatement {
                    kind: "parStart",
                    par_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(start_type),
                    ..Default::default()
                });
                list.push(stmt_item(SeqStatement {
                    kind: "parEnd",
                    signal_type: Some(linetype::PAR_END),
                    ..Default::default()
                }));
                list
            }
            K::Critical | K::Top => {
                let mut list = self.fold(
                    frame,
                    "option",
                    DividerKey::OptionText,
                    linetype::CRITICAL_OPTION,
                );
                list[0] = stmt_item(SeqStatement {
                    kind: "criticalStart",
                    critical_text: Some(self.db.parse_message(&text)),
                    signal_type: Some(linetype::CRITICAL_START),
                    ..Default::default()
                });
                list.push(stmt_item(SeqStatement {
                    kind: "criticalEnd",
                    signal_type: Some(linetype::CRITICAL_END),
                    ..Default::default()
                }));
                list
            }
        }
    }
}

pub fn parse_sequence(src: &str, db: &mut SeqDb) -> Result<()> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars);
    let mut p = Parser {
        src: &chars,
        types: tokens.types,
        texts: tokens.texts,
        starts: tokens.starts,
        i: 0,
        db,
    };

    while p.types[p.i] == t::NEWLINE {
        p.i += 1;
    }
    p.expect(t::SD)?;

    let mut stack: Vec<Frame> = Vec::new();
    let mut frame = Frame {
        kind: K::Top,
        text: String::new(),
        start_type: 0,
        list: Vec::new(),
        sections: Vec::new(),
    };

    loop {
        let tok = p.types[p.i];
        if frame.kind == K::Box {
            if tok == t::participant || tok == t::participant_actor || tok == t::destroy {
                let stmt = p.participant()?;
                frame.list.push(stmt_item(stmt));
            } else if tok == t::NEWLINE {
                frame.list.push(SeqItem::List(Vec::new()));
                p.i += 1;
            } else if tok == t::end {
                p.i += 1;
                let done = p.close(frame);
                frame = stack.pop().unwrap();
                frame.list.push(SeqItem::List(done));
            } else {
                return Err(p.err("'end', 'NEWLINE', 'participant', 'participant_actor', 'destroy'"));
            }
            continue;
        }

        match tok {
            t::NEWLINE | t::INVALID => {
                frame.list.push(SeqItem::List(Vec::new()));
                p.i += 1;
            }
            t::participant | t::participant_actor | t::destroy => {
                let stmt = p.participant()?;
                frame.list.push(stmt_item(stmt));
            }
            t::create => {
                p.i += 1;
                let next = p.types[p.i];
                if next != t::participant && next != t::participant_actor && next != t::destroy {
                    return Err(p.err("'participant', 'participant_actor', 'destroy'"));
                }
                let mut node = p.participant()?;
                node.kind = "createParticipant";
                frame.list.push(stmt_item(node));
            }
            t::ACTOR => {
                let (from_name, from_stmt) = p.actor()?;
                let sig = p.signal(from_name, from_stmt)?;
                frame.list.push(sig);
                p.expect(t::NEWLINE)?;
            }
            t::link => {
                let next = p.types[p.i + 1];
                let item = if next == t::ACTOR || next == t::link {
                    p.menu("addALink")?
                } else {
                    let (from_name, from_stmt) = p.actor()?;
                    p.signal(from_name, from_stmt)?
                };
                frame.list.push(item);
                p.expect(t::NEWLINE)?;
            }
            t::links => {
                let item = p.menu("addLinks")?;
                frame.list.push(item);
                p.expect(t::NEWLINE)?;
            }
            t::properties => {
                let item = p.menu("addProperties")?;
                frame.list.push(item);
                p.expect(t::NEWLINE)?;
            }
            t::details => {
                let item = p.menu("addDetails")?;
                frame.list.push(item);
                p.expect(t::NEWLINE)?;
            }
            t::note => {
                let item = p.note()?;
                frame.list.push(item);
                p.expect(t::NEWLINE)?;
            }
            t::autonumber => {
                let stmt = p.autonumber()?;
                frame.list.push(stmt_item(stmt));
            }
            t::activate | t::deactivate => {
                p.i += 1;
                let (who_name, _) = p.actor()?;
                p.expect(t::NEWLINE)?;
                frame.list.push(stmt_item(if tok == t::activate {
                    SeqStatement {
                        kind: "activeStart",
                        signal_type: Some(linetype::ACTIVE_START),
                        actor: Some(SeqNoteActor::Single(who_name)),
                        ..Default::default()
                    }
                } else {
                    SeqStatement {
                        kind: "activeEnd",
                        signal_type: Some(linetype::ACTIVE_END),
                        actor: Some(SeqNoteActor::Single(who_name)),
                        ..Default::default()
                    }
                }));
            }
            t::title | t::legacy_title => {
                let raw = std::mem::take(&mut p.texts[p.i]);
                p.i += 1;
                let skip = if tok == t::title { 6 } else { 7 };
                let title: String = raw.chars().skip(skip).collect();
                p.db.set_diagram_title(title.clone());
                frame.list.push(SeqItem::Str(title));
            }
            t::acc_title => {
                p.i += 1;
                let value = p.expect(t::acc_title_value)?.trim_matches(is_space).to_string();
                p.db.set_acc_title(&value);
                frame.list.push(SeqItem::Str(value));
            }
            t::acc_descr => {
                p.i += 1;
                let value = p.expect(t::acc_descr_value)?.trim_matches(is_space).to_string();
                p.db.set_acc_description(&value);
                frame.list.push(SeqItem::Str(value));
            }
            t::acc_descr_multiline_value => {
                let value = std::mem::take(&mut p.texts[p.i])
                    .trim_matches(is_space)
                    .to_string();
                p.i += 1;
                p.db.set_acc_description(&value);
                frame.list.push(SeqItem::Str(value));
            }
            t::r#loop
            | t::rect
            | t::opt
            | t::r#break
            | t::alt
            | t::par
            | t::par_over
            | t::critical
            | t::r#box => {
                p.i += 1;
                let text = p.expect(t::restOfLine)?;
                stack.push(frame);
                frame = Frame {
                    kind: block_kind(tok).unwrap(),
                    text,
                    start_type: if tok == t::par_over {
                        linetype::PAR_OVER_START
                    } else {
                        linetype::PAR_START
                    },
                    list: vec![SeqItem::List(Vec::new())],
                    sections: Vec::new(),
                };
            }
            t::r#else | t::and | t::option => {
                let expected_kind = if tok == t::r#else {
                    K::Alt
                } else if tok == t::and {
                    K::Par
                } else {
                    K::Critical
                };
                if frame.kind != expected_kind {
                    return Err(p.err("'end'"));
                }
                p.i += 1;
                let text = p.expect(t::restOfLine)?;
                let completed = std::mem::take(&mut frame.list);
                frame.sections.push((completed, text));
            }
            t::end => {
                if frame.kind == K::Top {
                    return Err(p.err("'NEWLINE', 'EOF'"));
                }
                p.i += 1;
                let done = p.close(frame);
                frame = stack.pop().unwrap();
                frame.list.push(SeqItem::List(done));
            }
            t::END => {
                if frame.kind != K::Top {
                    return Err(p.err("'end'"));
                }
                p.db.apply(SeqItem::List(frame.list))?;
                return Ok(());
            }
            _ => {
                return Err(p.err("'NEWLINE', 'ACTOR', 'participant', 'note', 'end'"));
            }
        }
    }
}
