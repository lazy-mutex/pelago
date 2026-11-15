use super::db::GanttDb;
use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};
use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

const WEEKDAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

const LINE: &str = concat!(
    "'EOF', 'SPACE', 'NL', 'weekday_monday', 'weekday_tuesday', 'weekday_wednesday', 'weekday_thursday', ",
    "'weekday_friday', 'weekday_saturday', 'weekday_sunday', 'weekend_friday', 'weekend_saturday', 'dateFormat', ",
    "'inclusiveEndDates', 'topAxis', 'axisFormat', 'tickInterval', 'excludes', 'includes', 'todayMarker', 'title', ",
    "'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'section', 'taskTxt', 'click'"
);

fn starts_line(tok: usize) -> bool {
    matches!(
        tok,
        t::EOF
            | t::NL
            | t::DATE_FORMAT
            | t::INCLUSIVE_END_DATES
            | t::TOP_AXIS
            | t::AXIS_FORMAT
            | t::TICK_INTERVAL
            | t::EXCLUDES
            | t::INCLUDES
            | t::TODAY_MARKER
            | t::WEEKDAY_MONDAY..=t::WEEKEND_SATURDAY
            | t::TITLE
            | t::ACC_TITLE
            | t::ACC_DESCR
            | t::ACC_DESCR_MULTILINE_VALUE
            | t::SECTION
            | t::TASK_TXT
            | t::CLICK
    )
}

fn slice_from_chars(s: &str, count: usize) -> &str {
    s.char_indices().nth(count).map_or("", |(idx, _)| &s[idx..])
}

fn trim_js(s: &str) -> &str {
    s.trim_matches(is_space)
}

pub fn parse_gantt(src: &str, db: &mut GanttDb) -> Result<()> {
    let chars: Vec<char> = src.chars().collect();
    let tokens = tokenize(&chars);
    let mut i = 0usize;

    let fail = |idx: usize, expected: &str| -> PeleError {
        let tok = tokens.types[idx];
        let got = if tok == t::END { "1" } else { TOKEN_NAMES[tok] };
        syntax_error(
            "gantt",
            &chars,
            tokens.starts[idx],
            &format!("Expecting {}, got '{}'", expected, got),
            false,
        )
    };

    // Mermaid's parser runs a statement's action only after seeing that the next token can follow it.
    let end = |idx: usize, also: &str| -> Result<()> {
        if !starts_line(tokens.types[idx]) {
            return Err(fail(idx, &format!("{}{}", LINE, also)));
        }
        Ok(())
    };

    if tokens.types[i] != t::GANTT {
        return Err(fail(i, "'gantt'"));
    }
    i += 1;

    loop {
        let tok = tokens.types[i];
        if !starts_line(tok) {
            return Err(fail(i, LINE));
        }
        let text = &tokens.texts[i];
        i += 1;

        match tok {
            t::EOF => {
                if tokens.types[i] == t::END {
                    return Ok(());
                }
                end(i, "")?;
            }
            t::NL => {}
            t::DATE_FORMAT => {
                end(i, "")?;
                db.set_date_format(slice_from_chars(text, 11));
            }
            t::INCLUSIVE_END_DATES => {
                end(i, "")?;
                db.enable_inclusive_end_dates();
            }
            t::TOP_AXIS => {
                end(i, "")?;
                db.top_axis();
            }
            t::AXIS_FORMAT => {
                end(i, "")?;
                db.set_axis_format(slice_from_chars(text, 11));
            }
            t::TICK_INTERVAL => {
                end(i, "")?;
                db.set_tick_interval(slice_from_chars(text, 13));
            }
            t::EXCLUDES => {
                end(i, "")?;
                db.set_excludes(slice_from_chars(text, 9));
            }
            t::INCLUDES => {
                end(i, "")?;
                db.set_includes(slice_from_chars(text, 9));
            }
            t::TODAY_MARKER => {
                end(i, "")?;
                db.set_today_marker(slice_from_chars(text, 12));
            }
            t::WEEKEND_FRIDAY => {
                end(i, "")?;
                db.set_weekend("friday");
            }
            t::WEEKEND_SATURDAY => {
                end(i, "")?;
                db.set_weekend("saturday");
            }
            t::TITLE => {
                end(i, "")?;
                db.set_diagram_title(slice_from_chars(text, 6));
            }
            t::ACC_TITLE => {
                if tokens.types[i] != t::ACC_TITLE_VALUE {
                    return Err(fail(i, "'acc_title_value'"));
                }
                let value = &tokens.texts[i];
                i += 1;
                end(i, "")?;
                db.set_acc_title(trim_js(value));
            }
            t::ACC_DESCR => {
                if tokens.types[i] != t::ACC_DESCR_VALUE {
                    return Err(fail(i, "'acc_descr_value'"));
                }
                let value = &tokens.texts[i];
                i += 1;
                end(i, "")?;
                db.set_acc_description(trim_js(value));
            }
            t::ACC_DESCR_MULTILINE_VALUE => {
                end(i, "")?;
                db.set_acc_description(trim_js(text));
            }
            t::SECTION => {
                end(i, "")?;
                db.add_section(slice_from_chars(text, 8));
            }
            t::TASK_TXT => {
                if tokens.types[i] != t::TASK_DATA {
                    return Err(fail(i, "'taskData'"));
                }
                let data = &tokens.texts[i];
                i += 1;
                end(i, "")?;
                db.add_task(text, data)?;
            }
            t::CLICK => {
                let mut name: Option<&str> = None;
                let mut args: Option<&str> = None;
                let mut href: Option<&str> = None;
                if tokens.types[i] == t::CALLBACKNAME {
                    name = Some(&tokens.texts[i]);
                    i += 1;
                    if tokens.types[i] == t::CALLBACKARGS {
                        args = Some(&tokens.texts[i]);
                        i += 1;
                    }
                    if tokens.types[i] == t::HREF {
                        href = Some(&tokens.texts[i]);
                        i += 1;
                        end(i, "")?;
                    } else {
                        end(i, if args.is_none() { ", 'callbackargs', 'href'" } else { ", 'href'" })?;
                    }
                } else if tokens.types[i] == t::HREF {
                    href = Some(&tokens.texts[i]);
                    i += 1;
                    if tokens.types[i] == t::CALLBACKNAME {
                        name = Some(&tokens.texts[i]);
                        i += 1;
                        if tokens.types[i] == t::CALLBACKARGS {
                            args = Some(&tokens.texts[i]);
                            i += 1;
                            end(i, "")?;
                        } else {
                            end(i, ", 'callbackargs'")?;
                        }
                    } else {
                        end(i, ", 'callbackname'")?;
                    }
                } else {
                    return Err(fail(i, "'callbackname', 'href'"));
                }
                if let Some(n) = name {
                    db.set_click_event(text, n, args);
                }
                if let Some(h) = href {
                    db.set_link(text, h);
                }
            }
            _ => {
                end(i, "")?;
                db.set_weekday(WEEKDAYS[tok - t::WEEKDAY_MONDAY]);
            }
        }
    }
}
