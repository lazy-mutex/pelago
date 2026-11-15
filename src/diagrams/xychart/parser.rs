use super::db::{DataPoint, XyChartDb, XyText};
use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};
use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

fn is_word_tok(tok: usize) -> bool {
    matches!(
        tok,
        t::AMP | t::NUM | t::ALPHA | t::PLUS | t::EQUALS | t::MULT | t::DOT | t::BRKT | t::MINUS | t::UNDERSCORE
    )
}

fn is_eol_tok(tok: usize) -> bool {
    matches!(tok, t::NEWLINE | t::SEMI | t::EOF)
}

fn is_text_tok(tok: usize) -> bool {
    tok == t::STR || is_word_tok(tok)
}

pub fn parse_xychart(src_str: &str, db: &mut XyChartDb) -> Result<()> {
    let src: Vec<char> = src_str.chars().collect();
    let tokens = tokenize(&src);
    let types = &tokens.types;
    let texts = &tokens.texts;
    let starts = &tokens.starts;
    let mut i = 0usize;

    let fail = |idx: usize, expected: &str| -> PeleError {
        let tok = types[idx];
        if tok == t::ERROR {
            syntax_error("xychart", &src, starts[idx], "", true)
        } else {
            syntax_error(
                "xychart",
                &src,
                starts[idx],
                &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[tok]),
                false,
            )
        }
    };

    let expect = |i: &mut usize, tok: usize| -> Result<String> {
        if types[*i] != tok {
            return Err(fail(*i, &format!("'{}'", TOKEN_NAMES[tok])));
        }
        let out = texts[*i].clone();
        *i += 1;
        Ok(out)
    };

    let parse_text = |i: &mut usize| -> Result<XyText> {
        if !is_text_tok(types[*i]) {
            return Err(fail(*i, "'STR', 'ALPHA', 'NUM'"));
        }
        let first_tok = types[*i];
        let mut s = texts[*i].clone();
        *i += 1;
        if first_tok != t::STR {
            while is_word_tok(types[*i]) {
                s.push_str(&texts[*i]);
                *i += 1;
            }
        }
        Ok(XyText { text: s })
    };

    let parse_range = |i: &mut usize| -> Result<(f64, f64)> {
        let min_s = expect(i, t::NUMBER_WITH_DECIMAL)?;
        let min = min_s.parse::<f64>().unwrap_or(f64::NAN);
        expect(i, t::ARROW_DELIMITER)?;
        let max_s = expect(i, t::NUMBER_WITH_DECIMAL)?;
        let max = max_s.parse::<f64>().unwrap_or(f64::NAN);
        Ok((min, max))
    };

    let parse_points = |i: &mut usize| -> Result<Vec<DataPoint>> {
        let mut out = Vec::new();
        expect(i, t::SQUARE_BRACES_START)?;
        loop {
            let val_s = expect(i, t::NUMBER_WITH_DECIMAL)?;
            let value = val_s.parse::<f64>().unwrap_or(f64::NAN);
            let label = if types[*i] == t::STR {
                let s = texts[*i].clone();
                *i += 1;
                s
            } else {
                String::new()
            };
            out.push(DataPoint { value, label });
            if types[*i] != t::COMMA {
                break;
            }
            *i += 1;
        }
        expect(i, t::SQUARE_BRACES_END)?;
        Ok(out)
    };

    loop {
        if is_eol_tok(types[i]) {
            i += 1;
        } else if types[i] != t::XYCHART {
            break;
        } else {
            i += 1;
            if types[i] == t::CHART_ORIENTATION {
                db.set_orientation(&texts[i]);
                i += 1;
            }
        }
    }

    while types[i] != t::END {
        let tok = types[i];
        i += 1;
        match tok {
            t::TITLE => {
                let txt = parse_text(&mut i)?;
                db.set_diagram_title(txt.text.trim_matches(is_space).to_string());
            }
            t::X_AXIS | t::Y_AXIS => {
                let axis_title = if is_text_tok(types[i]) {
                    Some(parse_text(&mut i)?)
                } else {
                    None
                };
                if tok == t::X_AXIS && types[i] == t::SQUARE_BRACES_START {
                    let mut categories = Vec::new();
                    loop {
                        i += 1;
                        categories.push(parse_text(&mut i)?);
                        if types[i] != t::COMMA {
                            break;
                        }
                    }
                    expect(&mut i, t::SQUARE_BRACES_END)?;
                    db.set_x_axis_band(categories);
                } else if types[i] == t::NUMBER_WITH_DECIMAL || axis_title.is_none() {
                    let (min, max) = parse_range(&mut i)?;
                    if tok == t::X_AXIS {
                        db.set_x_axis_range_data(min, max);
                    } else {
                        db.set_y_axis_range_data(min, max);
                    }
                }
                let t_obj = axis_title.unwrap_or_else(|| XyText { text: String::new() });
                if tok == t::X_AXIS {
                    db.set_x_axis_title(t_obj);
                } else {
                    db.set_y_axis_title(t_obj);
                }
            }
            t::LINE | t::BAR => {
                let s_title = if types[i] == t::SQUARE_BRACES_START {
                    XyText { text: String::new() }
                } else {
                    parse_text(&mut i)?
                };
                let data = parse_points(&mut i)?;
                if tok == t::LINE {
                    db.set_line_data(s_title, data);
                } else {
                    db.set_bar_data(s_title, data);
                }
            }
            t::ACC_TITLE => {
                let val = expect(&mut i, t::ACC_TITLE_VALUE)?;
                db.set_acc_title(val.trim_matches(is_space));
            }
            t::ACC_DESCR => {
                let val = expect(&mut i, t::ACC_DESCR_VALUE)?;
                db.set_acc_description(val.trim_matches(is_space));
            }
            t::ACC_DESCR_MULTILINE_VALUE => {
                db.set_acc_description(texts[i - 1].trim_matches(is_space));
            }
            _ => {
                i -= 1;
                return Err(fail(
                    i,
                    "'title', 'X_AXIS', 'Y_AXIS', 'LINE', 'BAR', 'acc_title', 'acc_descr', 'acc_descr_multiline_value'",
                ));
            }
        }
        while is_eol_tok(types[i]) {
            i += 1;
        }
    }
    Ok(())
}
