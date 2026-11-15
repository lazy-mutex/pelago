use super::db::IshikawaDb;
use super::lexer::{t, tokenize, TOKEN_NAMES};
use crate::errors::{syntax_error, Result};

const STOP: &str = "'SPACELINE', 'NL', 'EOF'";

// Follows the parser Jison builds from Mermaid's grammar: blank lines and comments may come first,
// then the keyword and at most one line break, then statements. A statement is a line of text,
// with or without indentation, or a blank line; each is ended by a blank line, a line break or
// the end of the text, and any number of line breaks after that.
pub fn parse_ishikawa(src_str: &str, db: &mut IshikawaDb) -> Result<()> {
    let src: Vec<char> = src_str.chars().collect();
    let tokens = tokenize(&src);
    let types = &tokens.types;
    let starts = &tokens.starts;
    let ends = &tokens.ends;
    let mut i = 0usize;

    let fail = |idx: usize, expected: &str| {
        syntax_error(
            "ishikawa",
            &src,
            starts[idx],
            &format!("Expecting {}, got '{}'", expected, TOKEN_NAMES[types[idx] as usize]),
            false,
        )
    };

    if types[i] == t::SPACELINE {
        i += 1;
        while types[i] == t::SPACELINE || types[i] == t::NL {
            i += 1;
        }
        if types[i] != t::ISHIKAWA {
            return Err(fail(i, "'SPACELINE', 'NL', 'ISHIKAWA'"));
        }
    } else if types[i] != t::ISHIKAWA {
        return Err(fail(i, "'SPACELINE', 'ISHIKAWA'"));
    }
    i += 1;
    if types[i] == t::NL {
        i += 1;
    }

    loop {
        let kind = types[i];
        if kind == t::TEXT || (kind == t::SPACELIST && types[i + 1] == t::TEXT) {
            let text_idx = if kind == t::TEXT { i } else { i + 1 };
            i = text_idx + 1;
            if types[i] != t::SPACELINE && types[i] != t::NL && types[i] != t::EOF {
                return Err(fail(i, STOP));
            }
            let raw_level: isize = if kind == t::TEXT {
                0
            } else {
                src[starts[text_idx - 1]..ends[text_idx - 1]]
                    .iter()
                    .map(|c| c.len_utf16() as isize)
                    .sum()
            };
            let text: String = src[starts[text_idx]..ends[text_idx]]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            db.add_node(raw_level, text);
        } else if kind == t::SPACELINE || kind == t::SPACELIST {
            i += 1;
            if types[i] != t::SPACELINE && types[i] != t::NL && types[i] != t::EOF {
                return Err(fail(
                    i,
                    if kind == t::SPACELIST {
                        "'SPACELINE', 'NL', 'EOF', 'TEXT'"
                    } else {
                        STOP
                    },
                ));
            }
        } else {
            return Err(fail(i, "'SPACELINE', 'SPACELIST', 'TEXT'"));
        }
        i += 1;
        while types[i] == t::NL || types[i] == t::EOF {
            i += 1;
        }
        if types[i] == t::END {
            return Ok(());
        }
    }
}
