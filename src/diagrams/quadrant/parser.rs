use super::db::{QuadrantDb, QuadrantText, QuadrantTextType};
use super::lexer::tokenize;
use super::tokens::{t, TOKEN_NAMES};
use crate::chars::is_space;
use crate::errors::{syntax_error, PeleError, Result};

fn is_word_tok(tok: usize) -> bool {
    matches!(
        tok,
        t::PUNCTUATION
            | t::AMP
            | t::NUM
            | t::ALPHA
            | t::COMMA
            | t::PLUS
            | t::EQUALS
            | t::MULT
            | t::DOT
            | t::BRKT
            | t::UNDERSCORE
            | t::UNICODE_TEXT
    )
}

fn is_id_tok(tok: usize) -> bool {
    matches!(
        tok,
        t::ALPHA | t::NUM | t::MINUS | t::COMMA | t::COLON | t::AMP | t::BRKT | t::MULT | t::UNICODE_TEXT
    )
}

fn is_style_tok(tok: usize) -> bool {
    matches!(tok, t::ALPHA | t::NUM | t::COLON | t::SPACE | t::BRKT | t::MINUS)
}

fn is_eol_tok(tok: usize) -> bool {
    matches!(tok, t::NEWLINE | t::SEMI | t::EOF)
}

fn is_text_tok(tok: usize) -> bool {
    tok == t::STR || tok == t::MD_STR || is_word_tok(tok)
}

const TEXT_TOKENS: &str = "'STR', 'MD_STR', 'ALPHA', 'NUM', 'UNICODE_TEXT', 'PUNCTUATION'";

pub fn parse_quadrant(src_str: &str, db: &mut QuadrantDb) -> Result<()> {
    let src: Vec<char> = src_str.chars().collect();
    let tokens = tokenize(&src);
    let types = &tokens.types;
    let texts = &tokens.texts;
    let starts = &tokens.starts;
    let mut i = 0usize;

    let fail = |idx: usize, expected: &str| -> PeleError {
        let tok = types[idx];
        if tok == t::ERROR {
            syntax_error("quadrantChart", &src, starts[idx], "", true)
        } else {
            syntax_error(
                "quadrantChart",
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

    let parse_text = |i: &mut usize| -> Result<QuadrantText> {
        let first = types[*i];
        if !is_text_tok(first) {
            return Err(fail(*i, TEXT_TOKENS));
        }
        let mut s = texts[*i].clone();
        *i += 1;
        while is_word_tok(types[*i]) || types[*i] == t::SPACE || types[*i] == t::MINUS {
            s.push_str(&texts[*i]);
            *i += 1;
        }
        Ok(QuadrantText {
            text: s,
            kind: if first == t::MD_STR { QuadrantTextType::Markdown } else { QuadrantTextType::Text },
        })
    };

    let parse_styles_list = |i: &mut usize| -> Result<Vec<String>> {
        let mut out = Vec::new();
        loop {
            if !is_style_tok(types[*i]) {
                return Err(fail(*i, "'ALPHA', 'NUM', 'COLON', 'SPACE', 'BRKT', 'MINUS'"));
            }
            let mut s = texts[*i].clone();
            *i += 1;
            while is_style_tok(types[*i]) {
                s.push_str(&texts[*i]);
                *i += 1;
            }
            out.push(s.trim_matches(is_space).to_string());
            if types[*i] != t::COMMA {
                return Ok(out);
            }
            *i += 1;
        }
    };

    while is_eol_tok(types[i]) || types[i] == t::SPACE {
        i += 1;
    }
    expect(&mut i, t::QUADRANT)?;

    while types[i] != t::END {
        while types[i] == t::SPACE {
            i += 1;
        }
        let tok = types[i];
        match tok {
            t::CLASSDEF => {
                i += 1;
                expect(&mut i, t::SPACE)?;
                if !is_id_tok(types[i]) {
                    return Err(fail(i, "'ALPHA', 'NUM', 'UNICODE_TEXT'"));
                }
                let mut id = texts[i].clone();
                i += 1;
                while is_id_tok(types[i]) {
                    id.push_str(&texts[i]);
                    i += 1;
                }
                expect(&mut i, t::SPACE)?;
                let st = parse_styles_list(&mut i)?;
                db.add_class(id, &st)?;
            }
            t::X_AXIS | t::Y_AXIS => {
                i += 1;
                let mut from = parse_text(&mut i)?;
                let mut to: Option<QuadrantText> = None;
                if types[i] == t::DELIMITER {
                    i += 1;
                    if is_text_tok(types[i]) {
                        to = Some(parse_text(&mut i)?);
                    } else {
                        from.text.push_str(" ⟶ ");
                    }
                }
                if tok == t::X_AXIS {
                    db.set_x_axis_left_text(from);
                    if let Some(to) = to {
                        db.set_x_axis_right_text(to);
                    }
                } else {
                    db.set_y_axis_bottom_text(from);
                    if let Some(to) = to {
                        db.set_y_axis_top_text(to);
                    }
                }
            }
            t::QUADRANT_1 | t::QUADRANT_2 | t::QUADRANT_3 | t::QUADRANT_4 => {
                i += 1;
                let qt = parse_text(&mut i)?;
                db.set_quadrant_text(tok - t::QUADRANT_1, qt);
            }
            t::TITLE => {
                i += 1;
                let val = expect(&mut i, t::TITLE_VALUE)?;
                db.set_diagram_title(val.trim_matches(is_space).to_string());
            }
            t::ACC_TITLE => {
                i += 1;
                let val = expect(&mut i, t::ACC_TITLE_VALUE)?;
                db.set_acc_title(val.trim_matches(is_space));
            }
            t::ACC_DESCR => {
                i += 1;
                let val = expect(&mut i, t::ACC_DESCR_VALUE)?;
                db.set_acc_description(val.trim_matches(is_space));
            }
            t::ACC_DESCR_MULTILINE_VALUE => {
                let val = &texts[i];
                i += 1;
                db.set_acc_description(val.trim_matches(is_space));
            }
            _ => {
                if is_text_tok(tok) {
                    let label = parse_text(&mut i)?;
                    let class_name = if types[i] == t::CLASS_NAME {
                        let cn = texts[i].clone();
                        i += 1;
                        cn
                    } else {
                        String::new()
                    };
                    expect(&mut i, t::POINT_START)?;
                    let x = expect(&mut i, t::POINT_X)?;
                    let y = expect(&mut i, t::POINT_Y)?;
                    let st = if is_style_tok(types[i]) {
                        parse_styles_list(&mut i)?
                    } else {
                        Vec::new()
                    };
                    db.add_point(label, class_name, &x, &y, &st)?;
                }
            }
        }
        if !is_eol_tok(types[i]) {
            return Err(fail(i, "'NEWLINE', 'SEMI', 'EOF'"));
        }
        i += 1;
    }
    Ok(())
}
