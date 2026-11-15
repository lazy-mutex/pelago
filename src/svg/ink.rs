// Picks a label color that contrasts with a style's fill.

// Named colors that #fff contrasts with more than #1f1f1f does, and the rest.
const DARK: &str = " black blue blueviolet brown crimson darkblue darkcyan darkgreen darkmagenta darkolivegreen darkorchid darkred darkslateblue darkslategray darkviolet dimgray firebrick forestgreen green indigo maroon mediumblue mediumslateblue mediumvioletred midnightblue navy olive purple rebeccapurple royalblue saddlebrown seagreen sienna slateblue steelblue teal ";
const LIGHT: &str = " aliceblue antiquewhite aqua aquamarine azure beige bisque blanchedalmond burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk cyan darkgoldenrod darkgray darkkhaki darkorange darksalmon darkseagreen darkturquoise deeppink deepskyblue dodgerblue floralwhite fuchsia gainsboro ghostwhite gold goldenrod gray greenyellow honeydew hotpink indianred ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan lightgoldenrodyellow lightgray lightgreen lightpink lightsalmon lightseagreen lightskyblue lightslategray lightsteelblue lightyellow lime limegreen linen magenta mediumaquamarine mediumorchid mediumpurple mediumseagreen mediumspringgreen mediumturquoise mintcream mistyrose moccasin navajowhite oldlace olivedrab orange orangered orchid palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue red rosybrown salmon sandybrown seashell silver skyblue slategray snow springgreen tan thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen ";

pub const ON_LIGHT: &str = "var(--pele-fg-on-light,#1f1f1f)";
pub const ON_DARK: &str = "var(--pele-fg-on-dark,#fff)";

fn named(set: &str, name: &str) -> bool {
    !name.is_empty() && !name.contains(' ') && set.contains(&format!(" {} ", name))
}

fn linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

// Luminance where #1f1f1f and #fff have equal contrast.
fn even() -> f64 {
    let c = 31.0 / 255.0;
    (1.05 * (luminance(c, c, c) + 0.05)).sqrt() - 0.05
}

fn is_hex(value: &str) -> bool {
    value.len() > 1 && value.starts_with('#') && matches!(value.len() - 1, 3 | 4 | 6 | 8) && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

// A CSS number with an optional unit: the value and the unit.
fn number(raw: &str) -> Option<(f64, &str)> {
    let lower_units = ["%", "deg", "grad", "rad", "turn"];
    let mut body = raw;
    let mut unit = "";
    for u in lower_units {
        if raw.len() > u.len() && raw[raw.len() - u.len()..].eq_ignore_ascii_case(u) {
            // `grad` ends in `rad`; take the longest unit that leaves a number.
            let head = &raw[..raw.len() - u.len()];
            if head.bytes().last().is_some_and(|b| b.is_ascii_digit() || b == b'.') {
                body = head;
                unit = u;
                break;
            }
        }
    }
    let ok = !body.is_empty()
        && body.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-' | b'e' | b'E'))
        && body.bytes().any(|b| b.is_ascii_digit());
    if !ok {
        return None;
    }
    body.parse::<f64>().ok().map(|n| (n, unit))
}

fn amount(raw: &str, full: f64) -> f64 {
    match number(raw) {
        Some((n, "%")) => (n / 100.0).clamp(0.0, 1.0),
        Some((n, "")) => (n / full).clamp(0.0, 1.0),
        _ => f64::NAN,
    }
}

fn hue(raw: &str) -> f64 {
    match number(raw) {
        Some((_, "%")) | None => f64::NAN,
        Some((n, "turn")) => n * 360.0,
        Some((n, "rad")) => n * 180.0 / std::f64::consts::PI,
        Some((n, "grad")) => n * 0.9,
        Some((n, _)) => n,
    }
}

// The comma syntax takes no `none`, rgb() channels all or none in %, and hsl() saturation and lightness in %.
fn args(body: &str, hsl: bool) -> Option<Vec<String>> {
    let legacy = body.contains(',');
    let (main, alpha) = if legacy {
        (body, None)
    } else {
        let mut parts = body.split('/');
        let main = parts.next()?;
        let alpha = parts.next();
        if parts.next().is_some() {
            return None;
        }
        (main, alpha)
    };
    let mut list: Vec<String> = if legacy {
        main.trim().split(',').map(|c| c.trim().to_string()).collect()
    } else {
        main.split_whitespace().map(String::from).collect()
    };
    if legacy {
        if list.len() < 3 || list.len() > 4 {
            return None;
        }
    } else if list.len() != 3 {
        return None;
    }
    if let Some(alpha) = alpha {
        list.push(alpha.trim().to_string());
    }
    if !legacy {
        return Some(list.into_iter().map(|c| if c == "none" { "0".to_string() } else { c }).collect());
    }
    let percent = list[if hsl { 1 } else { 0 }..3].iter().filter(|c| c.ends_with('%')).count();
    (percent == if hsl { 2 } else { 3 } || (!hsl && percent == 0)).then_some(list)
}

// A hex color or one of CSS's named colors.
pub fn plain_color(raw: &str) -> bool {
    let value = raw.trim().to_lowercase();
    let name = value.replacen("grey", "gray", 1);
    is_hex(&value) || named(DARK, &name) || named(LIGHT, &name) || value == "transparent" || value == "currentcolor"
}

// Luminance and alpha; named colors use 0 or 1 to preserve the contrast choice.
fn color(raw: &str) -> Option<(f64, f64)> {
    let value = raw.trim().to_lowercase();
    let name = value.replacen("grey", "gray", 1);
    if named(DARK, &name) {
        return Some((0.0, 1.0));
    }
    if named(LIGHT, &name) {
        return Some((1.0, 1.0));
    }
    if is_hex(&value) {
        let digits: String = if value.len() < 6 { value[1..].chars().flat_map(|c| [c, c]).collect() } else { value[1..].to_string() };
        let n = |i: usize| u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).unwrap_or(0) as f64 / 255.0;
        return Some((luminance(n(0), n(1), n(2)), if digits.len() == 8 { n(3) } else { 1.0 }));
    }
    let open = value.find('(')?;
    let name = &value[..open];
    if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") || !value.ends_with(')') {
        return None;
    }
    let body = &value[open + 1..value.len() - 1];
    if body.contains(['(', ')']) {
        return None;
    }
    let hsl = name.starts_with("hsl");
    let list = args(body, hsl)?;
    let alpha = if list.len() == 4 { amount(&list[3], 1.0) } else { 1.0 };
    let rgb: [f64; 3] = if !hsl {
        [amount(&list[0], 255.0), amount(&list[1], 255.0), amount(&list[2], 255.0)]
    } else {
        let h = (((hue(&list[0]) % 360.0) + 360.0) % 360.0) / 30.0;
        let s = amount(&list[1], 100.0);
        let l = amount(&list[2], 100.0);
        let a = s * l.min(1.0 - l);
        [0.0, 8.0, 4.0].map(|k| {
            let t = (k + h) % 12.0;
            l - a * (-1.0f64).max((t - 3.0).min(9.0 - t).min(1.0))
        })
    };
    let lum = luminance(rgb[0], rgb[1], rgb[2]);
    if (lum + alpha).is_nan() {
        None
    } else {
        Some((lum, alpha))
    }
}

// The value of a declaration in a style attribute from resolve_style(): /[";]prop:([^;"]*)/
pub fn declaration<'a>(style: &'a str, prop: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(at) = style[from..].find(prop) {
        let start = from + at;
        let after = start + prop.len();
        let before = style[..start].chars().next_back();
        if matches!(before, Some('"') | Some(';')) && style[after..].starts_with(':') {
            let value = &style[after + 1..];
            let end = value.find([';', '"']).unwrap_or(value.len());
            return Some(&value[..end]);
        }
        from = after;
    }
    None
}

// How opaque a style's fill-opacity and opacity leave its fill, or NaN when either isn't a plain number.
fn opacity(shape: &str) -> f64 {
    let mut product = 1.0;
    for prop in ["fill-opacity", "opacity"] {
        if let Some(value) = declaration(shape, prop) {
            product *= amount(value.trim(), 1.0);
        }
    }
    product
}

// Styles come from resolve_style(). Fills below half opacity depend on an unknown background.
pub fn ink(shape: &str, text: &str) -> Option<&'static str> {
    let fill = declaration(shape, "fill")?;
    if declaration(text, "fill").is_some() {
        return None;
    }
    let (lum, alpha) = color(fill)?;
    // An opacity that isn't a plain number, such as calc() or var(), may still be low.
    if !(alpha * opacity(shape) >= 0.5) {
        return None;
    }
    Some(if lum > even() { ON_LIGHT } else { ON_DARK })
}

// Unknown fill colors and opacities are treated as opaque.
pub fn paints(shape: &str) -> bool {
    let Some(fill) = declaration(shape, "fill").map(|f| f.trim().to_lowercase()) else { return false };
    if fill.is_empty() || fill == "none" || fill == "transparent" {
        return false;
    }
    let scale = opacity(shape);
    color(&fill).map_or(1.0, |c| c.1) * (if scale.is_nan() { 1.0 } else { scale }) > 0.0
}

pub fn ink_fill(shape: &str, text: &str, fallback: Option<&str>) -> String {
    match ink(shape, text).or(fallback) {
        Some(color) => format!(" fill=\"{}\"", color),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_contrast() {
        assert_eq!(ink(" style=\"fill:#000;\"", ""), Some(ON_DARK));
        assert_eq!(ink(" style=\"fill:#ffeeaa;stroke:#333;\"", ""), Some(ON_LIGHT));
        assert_eq!(ink(" style=\"fill:rgb(10, 20, 30);\"", ""), Some(ON_DARK));
        assert_eq!(ink(" style=\"fill:hsl(60 100% 80%);\"", ""), Some(ON_LIGHT));
        assert_eq!(ink(" style=\"fill:navy;opacity:0.2;\"", ""), None);
        assert_eq!(ink(" style=\"stroke:#000;\"", ""), None);
        assert!(paints(" style=\"fill:red;\""));
        assert!(!paints(" style=\"fill:none;\""));
    }
}
