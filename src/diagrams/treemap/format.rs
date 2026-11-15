use crate::chars::is_line_end;
use crate::svg::builder::{js_number, js_round};

#[derive(Clone, Copy)]
struct Spec {
    plus: bool,
    dollar: bool,
    comma: bool,
    precision: Option<usize>,
    trim: bool,
    kind: Option<char>,
}

// Matches /^(?:.?[<>=^])?([-+( ])?(\$)?0?\d*(,)?(?:\.(\d+))?(~)?([a-zA-Z%])?$/
fn parse_spec(raw: &str) -> Option<Spec> {
    let s: Vec<char> = raw.chars().collect();
    let n = s.len();
    let mut i = if n >= 2 && !is_line_end(s[0]) && matches!(s[1], '<' | '>' | '=' | '^') {
        2
    } else if n >= 1 && matches!(s[0], '<' | '>' | '=' | '^') {
        1
    } else {
        0
    };

    let mut plus = false;
    if i < n && matches!(s[i], '-' | '+' | '(' | ' ') {
        plus = s[i] == '+';
        i += 1;
    }

    let mut dollar = false;
    if i < n && s[i] == '$' {
        dollar = true;
        i += 1;
    }

    while i < n && s[i].is_ascii_digit() {
        i += 1;
    }

    let mut comma = false;
    if i < n && s[i] == ',' {
        comma = true;
        i += 1;
    }

    let mut precision = None;
    if i < n && s[i] == '.' {
        i += 1;
        let start = i;
        while i < n && s[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
        let digits: String = s[start..i].iter().collect();
        precision = Some(digits.parse::<usize>().unwrap_or(20).min(20));
    }

    let mut trim = false;
    if i < n && s[i] == '~' {
        trim = true;
        i += 1;
    }

    let mut kind = None;
    if i < n && (s[i].is_ascii_alphabetic() || s[i] == '%') {
        kind = Some(s[i]);
        i += 1;
    }

    if i != n {
        return None;
    }

    Some(Spec { plus, dollar, comma, precision, trim, kind })
}

fn group(digits: &str) -> String {
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    let mut first_len = bytes.len() % 3;
    if first_len == 0 && !bytes.is_empty() {
        first_len = 3;
    }
    out.push_str(&digits[..first_len]);
    let mut i = first_len;
    while i < bytes.len() {
        out.push(',');
        out.push_str(&digits[i..i + 3]);
        i += 3;
    }
    out
}

// ECMAScript Number.prototype.toFixed(p) for abs >= 0, p <= 20.
fn js_to_fixed(abs: f64, p: usize) -> String {
    if !abs.is_finite() || abs >= 1e21 {
        return js_number(abs);
    }
    let scaled = abs * 2.0_f64.powi((p + 1) as i32);
    if scaled.is_finite() && scaled < 9007199254740992.0 && scaled.fract() == 0.0 && (scaled as u64) % 2 == 1 {
        let m = scaled as u128;
        let rounded = (m * 5u128.pow(p as u32) + 1) / 2;
        let digits = rounded.to_string();
        if p == 0 {
            return digits;
        }
        if digits.len() <= p {
            return format!("0.{}{}", "0".repeat(p - digits.len()), digits);
        }
        let split = digits.len() - p;
        return format!("{}.{}", &digits[..split], &digits[split..]);
    }
    format!("{:.*}", p, abs)
}

fn js_to_exponential(abs: f64, p: usize) -> String {
    if !abs.is_finite() {
        return js_number(abs);
    }
    let raw = format!("{:.*e}", p, abs);
    if let Some((mant, exp)) = raw.split_once('e') {
        if exp.starts_with('-') || exp.starts_with('+') {
            format!("{}e{}", mant, exp)
        } else {
            format!("{}e+{}", mant, exp)
        }
    } else {
        raw
    }
}

fn js_to_precision_number(abs: f64, sig: usize) -> String {
    if !abs.is_finite() || abs == 0.0 {
        return js_number(abs);
    }
    let sci = format!("{:.*e}", sig - 1, abs);
    if let Some((_, exp_str)) = sci.split_once('e') {
        if let Ok(exp) = exp_str.parse::<i32>() {
            let places = sig as i32 - 1 - exp;
            if (0..=20).contains(&places) {
                let fixed = js_to_fixed(abs, places as usize);
                if let Ok(parsed) = fixed.parse::<f64>() {
                    return js_number(parsed);
                }
            } else if (-20..0).contains(&places) {
                let factor = 10.0_f64.powi(-places);
                let shifted = abs / factor;
                if shifted.is_finite() && shifted < 1e20 {
                    let fixed = js_to_fixed(shifted, 0);
                    if let Ok(parsed) = fixed.parse::<f64>() {
                        return js_number(parsed * factor);
                    }
                }
            }
        }
    }
    js_number(sci.parse::<f64>().unwrap_or(abs))
}

pub fn formatter(spec: &str) -> impl Fn(f64) -> String {
    let normalized = if spec == "$0,0" { "$," } else { spec };
    let sp = parse_spec(normalized).unwrap_or(Spec {
        plus: false,
        dollar: false,
        comma: true,
        precision: None,
        trim: false,
        kind: None,
    });

    move |value: f64| {
        let abs = value.abs();
        let mut suffix = "";
        let mut body = match sp.kind {
            Some('f') => js_to_fixed(abs, sp.precision.unwrap_or(6)),
            Some('%') => {
                suffix = "%";
                js_to_fixed(abs * 100.0, sp.precision.unwrap_or(6))
            }
            Some('d') => js_to_fixed(js_round(abs), 0),
            Some('e') => js_to_exponential(abs, sp.precision.unwrap_or(6)),
            _ => js_to_precision_number(abs, sp.precision.unwrap_or(12).max(1)),
        };

        if sp.trim && body.contains('.') && !body.contains('e') {
            let trimmed = body.trim_end_matches('0');
            let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
            body = trimmed.to_string();
        }

        if sp.comma && !body.contains('e') {
            body = match body.find('.') {
                None => group(&body),
                Some(dot) => format!("{}{}", group(&body[..dot]), &body[dot..]),
            };
        }

        let sign = if value < 0.0 {
            "-"
        } else if sp.plus {
            "+"
        } else {
            ""
        };
        let cur = if sp.dollar { "$" } else { "" };
        format!("{}{}{}{}", sign, cur, body, suffix)
    }
}
