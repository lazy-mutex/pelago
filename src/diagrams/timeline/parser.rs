use super::db::{Direction, TimelineDb};
use super::lexer::{t, tokenize, TOKEN_NAMES};
use crate::chars::is_space;
use crate::errors::{syntax_error, Result};

const STATEMENT: &str =
    "'EOF', 'SPACE', 'NEWLINE', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'section', 'period', 'event'";

pub fn parse_timeline(src_str: &str, db: &mut TimelineDb) -> Result<()> {
    let src: Vec<char> = src_str.chars().collect();
    let tokens = tokenize(&src);
    let types = &tokens.types;
    let texts = &tokens.texts;
    let starts = &tokens.starts;

    let fail = |idx: usize, expected: &str| {
        let got = if types[idx] == t::END {
            "1"
        } else {
            TOKEN_NAMES[types[idx] as usize]
        };
        syntax_error(
            "timeline",
            &src,
            starts[idx],
            &format!("Expecting {}, got '{}'", expected, got),
            false,
        )
    };

    let head = types[0];
    if head != t::TIMELINE && head != t::TIMELINE_LR && head != t::TIMELINE_TD {
        return Err(fail(0, "'timeline', 'timeline_lr', 'timeline_td'"));
    }
    if head != t::TIMELINE {
        db.set_direction(if head == t::TIMELINE_LR {
            Direction::Lr
        } else {
            Direction::Td
        });
    }

    let mut i = 1usize;
    loop {
        let text = &texts[i];
        match types[i] {
            t::NEWLINE => {}
            t::TITLE => {
                db.set_diagram_title(text.chars().skip(6).collect());
            }
            t::ACC_TITLE => {
                i += 1;
                db.set_acc_title(texts[i].trim_matches(is_space));
            }
            t::ACC_DESCR => {
                i += 1;
                db.set_acc_description(texts[i].trim_matches(is_space));
            }
            t::ACC_DESCR_MULTILINE_VALUE => {
                db.set_acc_description(text.trim_matches(is_space));
            }
            t::SECTION => {
                db.add_section(text.chars().skip(8).collect());
            }
            t::PERIOD => {
                db.add_task(text.clone(), 0, "");
            }
            t::EVENT => {
                db.add_event(text.chars().skip(2).collect())?;
            }
            t::EOF => return Ok(()),
            _ => return Err(fail(i, STATEMENT)),
        }
        i += 1;
    }
}
