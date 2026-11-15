use super::builder::{esc, has_invalid_xml, js_round, num};
use super::theme::families;
use crate::options::{RenderOptions, Theme};
use std::collections::HashMap;

const DEFAULT_SANS: &str = "Arial, Liberation Sans, DejaVu Sans, Noto Sans, Helvetica, sans-serif";
const DEFAULT_MONO: &str = "DejaVu Sans Mono, Liberation Mono, Noto Sans Mono, Courier New, monospace";

fn safe_token_value(value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() || has_invalid_xml(v) {
        return None;
    }
    let lower = v.to_ascii_lowercase();
    if lower.contains("url(")
        || lower.contains("url ")
        || lower.contains("expression")
        || lower.contains("javascript:")
        || lower.contains("@import")
        || lower.contains("image-set")
        || lower.contains("/*")
        || v.contains(['<', '>', '{', '}', ';', '\\'])
    {
        return None;
    }
    Some(v.to_string())
}

fn seed_theme_vars(theme: &Theme, options: &RenderOptions) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut overrides = HashMap::new();
    let mut put = |name: &str, opt: &Option<String>| {
        if let Some(val) = opt.as_deref().and_then(safe_token_value) {
            overrides.insert(name.to_string(), val);
        }
    };
    put("--pele-bg", &theme.bg);
    put("--pele-fg", &theme.fg);
    put("--pele-muted", &theme.muted);
    put("--pele-line", &theme.line);
    put("--pele-surface", &theme.surface);
    put("--pele-surface-alt", &theme.surface_alt);
    put("--pele-border", &theme.border);
    put("--pele-accent", &theme.accent);
    put("--pele-fg-on-light", &theme.fg_on_light);
    put("--pele-fg-on-dark", &theme.fg_on_dark);
    put("--pele-critical", &theme.critical);
    put("--pele-radius", &theme.radius);
    put("--pele-title-weight", &theme.title_weight);
    put("--pele-heading-weight", &theme.heading_weight);
    put("--pele-bold-weight", &theme.bold_weight);
    for (i, entry) in theme.series.iter().enumerate() {
        put(&format!("--pele-series-{}", i + 1), entry);
    }

    // Named font stacks ensure SVG 1.1 renderers like Typst's `usvg` select crisp sans-serif and
    // monospace fonts instead of ignoring generic CSS family keywords.
    let mut defaults = HashMap::new();
    if options.font_family.as_deref().map_or(true, |f| families(Some(f), "") .is_empty()) {
        defaults.insert("--pele-font".to_string(), DEFAULT_SANS.to_string());
    }
    if options.font_family_mono.as_deref().map_or(true, |f| families(Some(f), "").is_empty()) {
        defaults.insert("--pele-font-mono".to_string(), DEFAULT_MONO.to_string());
    }
    (defaults, overrides)
}

fn unescape_attr(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        if let Some(semi) = rest.find(';') {
            let entity = &rest[..semi];
            let decoded = match entity {
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32)
                }
                _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(ch) = decoded {
                out.push(ch);
                rest = &rest[semi + 1..];
                continue;
            }
        }
        out.push('&');
    }
    out.push_str(rest);
    out
}

fn split_css_declarations(style: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut quote: Option<char> = None;
    let mut depth = 0i32;
    for (i, c) in style.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            ';' if depth == 0 => {
                if let Some(decl) = parse_decl(&style[start..i]) {
                    out.push(decl);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    if let Some(decl) = parse_decl(&style[start..]) {
        out.push(decl);
    }
    out
}

fn parse_decl(raw: &str) -> Option<(&str, &str)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let colon = trimmed.find(':')?;
    let prop = trimmed[..colon].trim();
    let val = trimmed[colon + 1..].trim();
    (!prop.is_empty() && !val.is_empty()).then_some((prop, val))
}

fn find_matching_paren(s: &str, open_idx: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (offset, c) in s[open_idx..].char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_idx + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_first_top_comma(s: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            ',' if depth == 0 => return (&s[..i], Some(&s[i + 1..])),
            _ => {}
        }
    }
    (s, None)
}

fn find_ascii_case_insensitive(haystack: &str, needle_lower: &str) -> Option<usize> {
    let nlen = needle_lower.len();
    if haystack.len() < nlen {
        return None;
    }
    let hbytes = haystack.as_bytes();
    let nbytes = needle_lower.as_bytes();
    for i in 0..=hbytes.len() - nlen {
        if hbytes[i..i + nlen].iter().zip(nbytes).all(|(a, b)| a.to_ascii_lowercase() == *b) {
            return Some(i);
        }
    }
    None
}

fn expand_vars(input: &str, vars: &HashMap<String, String>, depth: usize) -> String {
    if depth > 8 || find_ascii_case_insensitive(input, "var(").is_none() {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let Some(rel) = find_ascii_case_insensitive(&input[cursor..], "var(") else {
            out.push_str(&input[cursor..]);
            break;
        };
        let start = cursor + rel;
        let before_ok = start == 0 || {
            let prev = input[..start].chars().next_back().unwrap();
            !prev.is_ascii_alphanumeric() && prev != '-' && prev != '_'
        };
        if !before_ok {
            out.push_str(&input[cursor..start + 4]);
            cursor = start + 4;
            continue;
        }
        let open_paren = start + 3;
        let Some(close_paren) = find_matching_paren(input, open_paren) else {
            out.push_str(&input[cursor..]);
            break;
        };
        out.push_str(&input[cursor..start]);
        let inner = &input[open_paren + 1..close_paren];
        let (name_raw, fallback) = split_first_top_comma(inner);
        let name = name_raw.trim();
        if let Some(found) = vars.get(name) {
            out.push_str(&expand_vars(found, vars, depth + 1));
        } else if let Some(fb) = fallback {
            out.push_str(&expand_vars(fb.trim(), vars, depth + 1));
        }
        cursor = close_paren + 1;
    }
    out
}

fn named_rgb(name: &str) -> Option<(u8, u8, u8)> {
    // Standard CSS Color 4 / SVG 1.1 named colors.
    match name {
        "aliceblue" => Some((240, 248, 255)),
        "antiquewhite" => Some((250, 235, 215)),
        "aqua" | "cyan" => Some((0, 255, 255)),
        "aquamarine" => Some((127, 255, 212)),
        "azure" => Some((240, 255, 255)),
        "beige" => Some((245, 245, 220)),
        "bisque" => Some((255, 228, 196)),
        "black" => Some((0, 0, 0)),
        "blanchedalmond" => Some((255, 235, 205)),
        "blue" => Some((0, 0, 255)),
        "blueviolet" => Some((138, 43, 226)),
        "brown" => Some((165, 42, 42)),
        "burlywood" => Some((222, 184, 135)),
        "cadetblue" => Some((95, 158, 160)),
        "chartreuse" => Some((127, 255, 0)),
        "chocolate" => Some((210, 105, 30)),
        "coral" => Some((255, 127, 80)),
        "cornflowerblue" => Some((100, 149, 237)),
        "cornsilk" => Some((255, 248, 220)),
        "crimson" => Some((220, 20, 60)),
        "darkblue" => Some((0, 0, 139)),
        "darkcyan" => Some((0, 139, 139)),
        "darkgoldenrod" => Some((184, 134, 11)),
        "darkgray" | "darkgrey" => Some((169, 169, 169)),
        "darkgreen" => Some((0, 100, 0)),
        "darkkhaki" => Some((189, 183, 107)),
        "darkmagenta" => Some((139, 0, 139)),
        "darkolivegreen" => Some((85, 107, 47)),
        "darkorange" => Some((255, 140, 0)),
        "darkorchid" => Some((153, 50, 204)),
        "darkred" => Some((139, 0, 0)),
        "darksalmon" => Some((233, 150, 122)),
        "darkseagreen" => Some((143, 188, 143)),
        "darkslateblue" => Some((72, 61, 139)),
        "darkslategray" | "darkslategrey" => Some((47, 79, 79)),
        "darkturquoise" => Some((0, 206, 209)),
        "darkviolet" => Some((148, 0, 211)),
        "deeppink" => Some((255, 20, 147)),
        "deepskyblue" => Some((0, 191, 255)),
        "dimgray" | "dimgrey" => Some((105, 105, 105)),
        "dodgerblue" => Some((30, 144, 255)),
        "firebrick" => Some((178, 34, 34)),
        "floralwhite" => Some((255, 250, 240)),
        "forestgreen" => Some((34, 139, 34)),
        "fuchsia" | "magenta" => Some((255, 0, 255)),
        "gainsboro" => Some((220, 220, 220)),
        "ghostwhite" => Some((248, 248, 255)),
        "gold" => Some((255, 215, 0)),
        "goldenrod" => Some((218, 165, 32)),
        "gray" | "grey" => Some((128, 128, 128)),
        "green" => Some((0, 128, 0)),
        "greenyellow" => Some((173, 255, 47)),
        "honeydew" => Some((240, 255, 240)),
        "hotpink" => Some((255, 105, 180)),
        "indianred" => Some((205, 92, 92)),
        "indigo" => Some((75, 0, 130)),
        "ivory" => Some((255, 255, 240)),
        "khaki" => Some((240, 230, 140)),
        "lavender" => Some((230, 230, 250)),
        "lavenderblush" => Some((255, 240, 245)),
        "lawngreen" => Some((124, 252, 0)),
        "lemonchiffon" => Some((255, 250, 205)),
        "lightblue" => Some((173, 216, 230)),
        "lightcoral" => Some((240, 128, 128)),
        "lightcyan" => Some((224, 255, 255)),
        "lightgoldenrodyellow" => Some((250, 250, 210)),
        "lightgray" | "lightgrey" => Some((211, 211, 211)),
        "lightgreen" => Some((144, 238, 144)),
        "lightpink" => Some((255, 182, 193)),
        "lightsalmon" => Some((255, 160, 122)),
        "lightseagreen" => Some((32, 178, 170)),
        "lightskyblue" => Some((135, 206, 250)),
        "lightslategray" | "lightslategrey" => Some((119, 136, 153)),
        "lightsteelblue" => Some((176, 196, 222)),
        "lightyellow" => Some((255, 255, 224)),
        "lime" => Some((0, 255, 0)),
        "limegreen" => Some((50, 205, 50)),
        "linen" => Some((250, 240, 230)),
        "maroon" => Some((128, 0, 0)),
        "mediumaquamarine" => Some((102, 205, 170)),
        "mediumblue" => Some((0, 0, 205)),
        "mediumorchid" => Some((186, 85, 211)),
        "mediumpurple" => Some((147, 112, 219)),
        "mediumseagreen" => Some((60, 179, 113)),
        "mediumslateblue" => Some((123, 104, 238)),
        "mediumspringgreen" => Some((0, 250, 154)),
        "mediumturquoise" => Some((72, 209, 204)),
        "mediumvioletred" => Some((199, 21, 133)),
        "midnightblue" => Some((25, 25, 112)),
        "mintcream" => Some((245, 255, 250)),
        "mistyrose" => Some((255, 228, 225)),
        "moccasin" => Some((255, 228, 181)),
        "navajowhite" => Some((255, 222, 173)),
        "navy" => Some((0, 0, 128)),
        "oldlace" => Some((253, 245, 230)),
        "olive" => Some((128, 128, 0)),
        "olivedrab" => Some((107, 142, 35)),
        "orange" => Some((255, 165, 0)),
        "orangered" => Some((255, 69, 0)),
        "orchid" => Some((218, 112, 214)),
        "palegoldenrod" => Some((238, 232, 170)),
        "palegreen" => Some((152, 251, 152)),
        "paleturquoise" => Some((175, 238, 238)),
        "palevioletred" => Some((219, 112, 147)),
        "papayawhip" => Some((255, 239, 213)),
        "peachpuff" => Some((255, 218, 185)),
        "peru" => Some((205, 133, 63)),
        "pink" => Some((255, 192, 203)),
        "plum" => Some((221, 160, 221)),
        "powderblue" => Some((176, 224, 230)),
        "purple" => Some((128, 0, 128)),
        "rebeccapurple" => Some((102, 51, 153)),
        "red" => Some((255, 0, 0)),
        "rosybrown" => Some((188, 143, 143)),
        "royalblue" => Some((65, 105, 225)),
        "saddlebrown" => Some((139, 69, 19)),
        "salmon" => Some((250, 128, 114)),
        "sandybrown" => Some((244, 164, 96)),
        "seagreen" => Some((46, 139, 87)),
        "seashell" => Some((255, 245, 238)),
        "sienna" => Some((160, 82, 45)),
        "silver" => Some((192, 192, 192)),
        "skyblue" => Some((135, 206, 235)),
        "slateblue" => Some((106, 90, 205)),
        "slategray" | "slategrey" => Some((112, 128, 144)),
        "snow" => Some((255, 250, 250)),
        "springgreen" => Some((0, 255, 127)),
        "steelblue" => Some((70, 130, 180)),
        "tan" => Some((210, 180, 140)),
        "teal" => Some((0, 128, 128)),
        "thistle" => Some((216, 191, 216)),
        "tomato" => Some((255, 99, 71)),
        "turquoise" => Some((64, 224, 208)),
        "violet" => Some((238, 130, 238)),
        "wheat" => Some((245, 222, 179)),
        "white" => Some((255, 255, 255)),
        "whitesmoke" => Some((245, 245, 245)),
        "yellow" => Some((255, 255, 0)),
        "yellowgreen" => Some((154, 205, 50)),
        _ => None,
    }
}

fn parse_channel(raw: &str, full: f64) -> Option<f64> {
    let s = raw.trim();
    if let Some(pct) = s.strip_suffix('%') {
        pct.trim().parse::<f64>().ok().map(|n| (n / 100.0).clamp(0.0, 1.0))
    } else {
        s.parse::<f64>().ok().map(|n| (n / full).clamp(0.0, 1.0))
    }
}

fn parse_rgba(raw: &str) -> Option<(f64, f64, f64, f64)> {
    let v = raw.trim().to_ascii_lowercase();
    if v == "transparent" {
        return Some((0.0, 0.0, 0.0, 0.0));
    }
    if let Some((r, g, b)) = named_rgb(&v) {
        return Some((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, 1.0));
    }
    if let Some(hex) = v.strip_prefix('#') {
        if hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            let expanded: String = match hex.len() {
                3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
                6 | 8 => hex.to_string(),
                _ => return None,
            };
            let byte = |i: usize| u8::from_str_radix(&expanded[2 * i..2 * i + 2], 16).ok().map(|n| n as f64 / 255.0);
            let (r, g, b) = (byte(0)?, byte(1)?, byte(2)?);
            let a = if expanded.len() == 8 { byte(3)? } else { 1.0 };
            return Some((r, g, b, a));
        }
    }
    let open = v.find('(')?;
    if !v.ends_with(')') {
        return None;
    }
    let fname = &v[..open];
    let body = &v[open + 1..v.len() - 1];
    let (parts, alpha_str): (Vec<&str>, Option<&str>) = if body.contains(',') {
        let items: Vec<&str> = body.split(',').map(str::trim).collect();
        if items.len() < 3 || items.len() > 4 {
            return None;
        }
        (items[..3].to_vec(), items.get(3).copied())
    } else {
        let mut slash = body.split('/');
        let main = slash.next()?;
        let alpha = slash.next().map(str::trim);
        if slash.next().is_some() {
            return None;
        }
        let items: Vec<&str> = main.split_whitespace().collect();
        if items.len() != 3 {
            return None;
        }
        (items, alpha)
    };
    let a = match alpha_str {
        Some(s) => parse_channel(s, 1.0)?,
        None => 1.0,
    };
    match fname {
        "rgb" | "rgba" => Some((parse_channel(parts[0], 255.0)?, parse_channel(parts[1], 255.0)?, parse_channel(parts[2], 255.0)?, a)),
        "hsl" | "hsla" => {
            let h_deg = parts[0].strip_suffix("deg").unwrap_or(parts[0]).trim().parse::<f64>().ok()?;
            let h = (((h_deg % 360.0) + 360.0) % 360.0) / 30.0;
            let s = parse_channel(parts[1], 100.0)?;
            let l = parse_channel(parts[2], 100.0)?;
            let k_amp = s * l.min(1.0 - l);
            let ch = |k: f64| {
                let t = (k + h) % 12.0;
                l - k_amp * (-1.0f64).max((t - 3.0).min(9.0 - t).min(1.0))
            };
            Some((ch(0.0), ch(8.0), ch(4.0), a))
        }
        _ => None,
    }
}

fn parse_mix_stop(stop: &str) -> Option<((f64, f64, f64, f64), Option<f64>)> {
    let s = stop.trim();
    if let Some(pct_trimmed) = s.strip_suffix('%') {
        if let Some(space) = pct_trimmed.rfind(char::is_whitespace) {
            if let Ok(p) = pct_trimmed[space + 1..].trim().parse::<f64>() {
                let c = parse_rgba(pct_trimmed[..space].trim())?;
                return Some((c, Some(p.clamp(0.0, 100.0))));
            }
        }
    }
    Some((parse_rgba(s)?, None))
}

fn format_rgba((r, g, b, a): (f64, f64, f64, f64)) -> String {
    let byte = |v: f64| js_round(v.clamp(0.0, 1.0) * 255.0) as u8;
    let (rb, gb, bb) = (byte(r), byte(g), byte(b));
    if (a - 1.0).abs() < 1e-4 {
        format!("#{:02x}{:02x}{:02x}", rb, gb, bb)
    } else {
        format!("rgba({},{},{},{})", rb, gb, bb, num(a.clamp(0.0, 1.0)))
    }
}

fn eval_color_mix(inner: &str) -> Option<String> {
    let (space, rest) = split_first_top_comma(inner);
    if !space.trim().eq_ignore_ascii_case("in srgb") {
        return None;
    }
    let (stop1, stop2) = split_first_top_comma(rest?);
    let ((r1, g1, b1, a1), p1) = parse_mix_stop(stop1)?;
    let ((r2, g2, b2, a2), p2) = parse_mix_stop(stop2?)?;
    let (mut w1, mut w2, mut alpha_mul) = match (p1, p2) {
        (Some(x), None) => (x / 100.0, 1.0 - x / 100.0, 1.0),
        (None, Some(y)) => (1.0 - y / 100.0, y / 100.0, 1.0),
        (None, None) => (0.5, 0.5, 1.0),
        (Some(x), Some(y)) => {
            let sum = x + y;
            if sum <= 0.0 {
                return Some("transparent".into());
            }
            (x / sum, y / sum, (sum / 100.0).min(1.0))
        }
    };
    let _ = (&mut w1, &mut w2, &mut alpha_mul);
    let a_mix = a1 * w1 + a2 * w2;
    if a_mix <= 0.0 {
        return Some("transparent".into());
    }
    let r = (r1 * a1 * w1 + r2 * a2 * w2) / a_mix;
    let g = (g1 * a1 * w1 + g2 * a2 * w2) / a_mix;
    let b = (b1 * a1 * w1 + b2 * a2 * w2) / a_mix;
    Some(format_rgba((r, g, b, a_mix * alpha_mul)))
}

fn expand_color_mix(input: &str) -> String {
    if find_ascii_case_insensitive(input, "color-mix(").is_none() {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let Some(rel) = find_ascii_case_insensitive(&input[cursor..], "color-mix(") else {
            out.push_str(&input[cursor..]);
            break;
        };
        let start = cursor + rel;
        let open_paren = start + "color-mix".len();
        let Some(close_paren) = find_matching_paren(input, open_paren) else {
            out.push_str(&input[cursor..]);
            break;
        };
        out.push_str(&input[cursor..start]);
        let inner = expand_color_mix(&input[open_paren + 1..close_paren]);
        match eval_color_mix(&inner) {
            Some(resolved) => out.push_str(&resolved),
            None => {
                out.push_str(&input[start..=open_paren]);
                out.push_str(&inner);
                out.push(')');
            }
        }
        cursor = close_paren + 1;
    }
    out
}

fn resolve_css_value(raw: &str, vars: &HashMap<String, String>) -> String {
    let expanded = expand_vars(raw, vars, 0);
    expand_color_mix(&expanded)
}

fn plain_svg_length(val: &str) -> Option<String> {
    let s = val.trim();
    if s == "100%" {
        return Some(s.to_string());
    }
    let digits = s.strip_suffix("px").unwrap_or(s);
    if digits.is_empty() {
        return None;
    }
    let valid = digits.parse::<f64>().is_ok()
        && digits.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+' | b'e' | b'E'))
        && digits.bytes().any(|b| b.is_ascii_digit());
    valid.then(|| digits.to_string())
}

fn parse_tag_attrs(attr_str: &str) -> Option<Vec<(String, String)>> {
    let mut attrs = Vec::new();
    let bytes = attr_str.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let name = &attr_str[name_start..i];
        if name.is_empty() || i >= bytes.len() || bytes[i] != b'=' {
            return None;
        }
        i += 1;
        if i >= bytes.len() || bytes[i] != b'"' {
            return None;
        }
        i += 1;
        let val_start = i;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let val = unescape_attr(&attr_str[val_start..i]);
        attrs.push((name.to_string(), val));
        i += 1;
    }
    Some(attrs)
}

// Resolves Pele's CSS custom properties, `color-mix()`, and CSS `rx`/`ry` into plain SVG 1.1
// presentation attributes and colors so non-browser SVG engines (such as Typst's `usvg`) render them.
pub fn resolve_svg(svg: &str, options: &RenderOptions) -> String {
    let (mut vars, overrides) = seed_theme_vars(&options.theme, options);
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0;
    let mut is_first_svg = true;

    while cursor < svg.len() {
        let Some(rel_lt) = svg[cursor..].find('<') else {
            out.push_str(&svg[cursor..]);
            break;
        };
        let lt = cursor + rel_lt;
        out.push_str(&svg[cursor..lt]);

        // Find end of tag `>` (all `>` inside attributes are escaped as `&gt;` by builder::esc).
        let Some(rel_gt) = svg[lt..].find('>') else {
            out.push_str(&svg[lt..]);
            break;
        };
        let gt = lt + rel_gt;
        let inside = &svg[lt + 1..gt];
        if inside.starts_with('/') || inside.starts_with('!') || inside.starts_with('?') {
            out.push_str(&svg[lt..=gt]);
            cursor = gt + 1;
            continue;
        }
        let self_closing = inside.ends_with('/');
        let core = if self_closing { &inside[..inside.len() - 1] } else { inside };
        let name_len = core.bytes().take_while(|b| !b.is_ascii_whitespace()).count();
        let tag_name = &core[..name_len];
        let attr_part = &core[name_len..];

        let Some(mut attrs) = parse_tag_attrs(attr_part) else {
            out.push_str(&svg[lt..=gt]);
            cursor = gt + 1;
            continue;
        };

        // Collect custom properties from the root <svg> element's style attribute.
        if tag_name == "svg" && is_first_svg {
            is_first_svg = false;
            if let Some((_, style_val)) = attrs.iter().find(|(k, _)| k == "style") {
                for (prop, val) in split_css_declarations(style_val) {
                    if prop.starts_with("--") {
                        vars.insert(prop.to_string(), val.to_string());
                    }
                }
            }
            for (k, v) in &overrides {
                vars.insert(k.clone(), v.clone());
            }
        }

        let mut promoted_rx: Option<String> = None;
        let mut promoted_ry: Option<String> = None;
        let mut resolved_attrs: Vec<(String, String)> = Vec::with_capacity(attrs.len() + 2);

        for (name, value) in attrs.drain(..) {
            if name == "style" {
                let mut kept = String::new();
                for (prop, val) in split_css_declarations(&value) {
                    if prop.starts_with("--") {
                        continue;
                    }
                    let resolved = resolve_css_value(val, &vars);
                    if prop == "rx" {
                        if let Some(len) = plain_svg_length(&resolved) {
                            promoted_rx = Some(len);
                            continue;
                        }
                    } else if prop == "ry" {
                        if let Some(len) = plain_svg_length(&resolved) {
                            promoted_ry = Some(len);
                            continue;
                        }
                    }
                    kept.push_str(prop);
                    kept.push(':');
                    kept.push_str(&resolved);
                    kept.push(';');
                }
                if !kept.is_empty() {
                    resolved_attrs.push((name, kept));
                }
            } else {
                let resolved = resolve_css_value(&value, &vars);
                resolved_attrs.push((name, resolved));
            }
        }

        for (attr_name, promoted) in [("rx", promoted_rx), ("ry", promoted_ry)] {
            if let Some(val) = promoted {
                match resolved_attrs.iter_mut().find(|(k, _)| k == attr_name) {
                    Some(slot) => slot.1 = val,
                    None => resolved_attrs.push((attr_name.to_string(), val)),
                }
            }
        }

        out.push('<');
        out.push_str(tag_name);
        for (k, v) in &resolved_attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            out.push_str(&esc(v));
            out.push('"');
        }
        if self_closing {
            out.push('/');
        }
        out.push('>');
        cursor = gt + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_vars_color_mix_and_rx() {
        let raw = "<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"--_bg:var(--pele-bg,#fff);--_fg:var(--pele-fg,#1f1f1f);--_a:var(--pele-surface-alt,#e6e6e6);--_s:var(--pele-surface,#f3f3f3);--_b:var(--pele-border,#8a8a8a);--_r:var(--pele-radius,4px);color:var(--_fg)\" fill=\"var(--_fg)\"><rect x=\"0\" y=\"0\" width=\"20\" height=\"10\" fill=\"var(--_s)\" stroke=\"var(--_b)\" style=\"rx:var(--_r)\"/><text stroke=\"color-mix(in srgb,var(--_a) 50%,var(--_bg))\">var(--_bg) stays literal</text></svg>";
        let resolved = resolve_svg(raw, &RenderOptions::default());
        assert!(!resolved.contains("=\"var(--") && !resolved.contains(":var(--"));
        assert!(!resolved.contains("color-mix("));
        assert!(resolved.contains("fill=\"#1f1f1f\""));
        assert!(resolved.contains("<rect x=\"0\" y=\"0\" width=\"20\" height=\"10\" fill=\"#f3f3f3\" stroke=\"#8a8a8a\" rx=\"4\"/>"));
        assert!(resolved.contains("stroke=\"#f3f3f3\""));
        assert!(resolved.contains(">var(--_bg) stays literal</text>"));
    }

    #[test]
    fn theme_overrides_apply() {
        let raw = "<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"--_s:var(--pele-surface,#f3f3f3);--_r:var(--pele-radius,4px)\"><rect width=\"10\" height=\"10\" fill=\"var(--_s)\" style=\"rx:var(--_r);rx:12px;fill:#eee;\"/></svg>";
        let mut opts = RenderOptions::default();
        opts.theme.surface = Some("#112233".into());
        let resolved = resolve_svg(raw, &opts);
        assert!(resolved.contains("fill=\"#112233\""));
        assert!(resolved.contains("style=\"fill:#eee;\""));
        assert!(resolved.contains("rx=\"12\""));
    }
}
