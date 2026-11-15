use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuadrantTextType {
    Text,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct QuadrantText {
    pub text: String,
    pub kind: QuadrantTextType,
}

#[derive(Debug, Clone, Default)]
pub struct PointStyle {
    pub radius: Option<f64>,
    pub color: Option<String>,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<String>,
}

#[derive(Debug, Clone)]
pub struct QuadrantPoint {
    pub x: f64,
    pub y: f64,
    pub text: QuadrantText,
    pub class_name: String,
    pub style: PointStyle,
}

#[derive(Debug, Clone, Default)]
pub struct QuadrantDb {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    // Quadrants 1 to 4: top right, top left, bottom left, bottom right.
    pub quadrants: [Option<QuadrantText>; 4],
    pub x_axis_left: Option<QuadrantText>,
    pub x_axis_right: Option<QuadrantText>,
    pub y_axis_bottom: Option<QuadrantText>,
    pub y_axis_top: Option<QuadrantText>,
    pub points: Vec<QuadrantPoint>,
    pub classes: HashMap<String, PointStyle>,
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_hex_color(s: &str) -> bool {
    let hex = s.strip_prefix('#').unwrap_or(s);
    (hex.len() == 3 || hex.len() == 6) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_pixels(s: &str) -> bool {
    s.strip_suffix("px").is_some_and(is_number)
}

fn invalid(style: &str, value: &str, kind: &str) -> PeleError {
    PeleError::new(format!("value for {} {} is invalid, please use a valid {}", style, value, kind), ErrorCode::Semantic).of("quadrantChart")
}

// The first two parts of Mermaid's `style.trim().split(/\s*:\s*/)`, found in one pass.
fn key_value(style: &str) -> (&str, &str) {
    let s = style.trim_matches(is_space);
    let Some(colon) = s.find(':') else {
        return (s, "undefined");
    };
    let rest = s[colon + 1..].trim_start_matches(is_space);
    let val = match rest.find(':') {
        None => rest,
        Some(next) => rest[..next].trim_end_matches(is_space),
    };
    (s[..colon].trim_end_matches(is_space), val)
}

pub fn parse_styles(styles: &[String]) -> Result<PointStyle> {
    let mut out = PointStyle::default();
    for style in styles {
        let (key, value) = key_value(style);
        match key {
            "radius" => {
                if !is_number(value) {
                    return Err(invalid(key, value, "number"));
                }
                out.radius = Some(value.parse::<f64>().unwrap_or(0.0));
            }
            "color" => {
                if !is_hex_color(value) {
                    return Err(invalid(key, value, "hex code"));
                }
                out.color = Some(value.to_string());
            }
            "stroke-color" => {
                if !is_hex_color(value) {
                    return Err(invalid(key, value, "hex code"));
                }
                out.stroke_color = Some(value.to_string());
            }
            "stroke-width" => {
                if !is_pixels(value) {
                    return Err(invalid(key, value, "number of pixels (eg. 10px)"));
                }
                out.stroke_width = Some(value.to_string());
            }
            _ => {
                return Err(PeleError::new(format!("style named {} is not supported.", key), ErrorCode::Semantic).of("quadrantChart"));
            }
        }
    }
    Ok(out)
}

fn trimmed(label: QuadrantText) -> QuadrantText {
    QuadrantText { text: label.text.trim_matches(is_space).to_string(), kind: label.kind }
}

// JavaScript's `Number(s)` for coordinate strings matched by `(1)|(0(.\d+)?)`.
fn js_number_parse(s: &str) -> f64 {
    let t = s.trim_matches(is_space);
    if t.is_empty() {
        return 0.0;
    }
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u128::from_str_radix(hex, 16).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    if let Some(oct) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return u128::from_str_radix(oct, 8).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    if let Some(bin) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return u128::from_str_radix(bin, 2).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    if t.eq_ignore_ascii_case("inf") || t.eq_ignore_ascii_case("infinity") || t.eq_ignore_ascii_case("nan") {
        return match t {
            "Infinity" | "+Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => f64::NAN,
        };
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

fn collapse_newline_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' && i + 1 < chars.len() && is_space(chars[i + 1]) {
            out.push('\n');
            i += 2;
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

impl QuadrantDb {
    pub fn set_quadrant_text(&mut self, index: usize, label: QuadrantText) {
        self.quadrants[index] = Some(trimmed(label));
    }

    pub fn set_x_axis_left_text(&mut self, label: QuadrantText) {
        self.x_axis_left = Some(trimmed(label));
    }

    pub fn set_x_axis_right_text(&mut self, label: QuadrantText) {
        self.x_axis_right = Some(trimmed(label));
    }

    pub fn set_y_axis_bottom_text(&mut self, label: QuadrantText) {
        self.y_axis_bottom = Some(trimmed(label));
    }

    pub fn set_y_axis_top_text(&mut self, label: QuadrantText) {
        self.y_axis_top = Some(trimmed(label));
    }

    pub fn add_point(&mut self, label: QuadrantText, class_name: String, x: &str, y: &str, styles: &[String]) -> Result<()> {
        let style = parse_styles(styles)?;
        self.points.push(QuadrantPoint {
            x: js_number_parse(x),
            y: js_number_parse(y),
            text: trimmed(label),
            class_name,
            style,
        });
        Ok(())
    }

    pub fn add_class(&mut self, class_name: String, styles: &[String]) -> Result<()> {
        let style = parse_styles(styles)?;
        self.classes.insert(class_name, style);
        Ok(())
    }

    pub fn set_diagram_title(&mut self, text: String) {
        self.title = Some(text);
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_spaces(text));
    }
}
