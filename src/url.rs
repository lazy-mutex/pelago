use crate::options::{LinkInfo, RenderOptions};
use crate::svg::builder::{esc, has_invalid_xml};
use crate::text::entities::decode_entities;

const BLANK: &str = "about:blank";
const LINK_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];
const IMAGE_SCHEMES: &[&str] = &["http", "https"];

fn is_ctrl(c: char) -> bool {
    matches!(c as u32, 0x00..=0x1f | 0x7f..=0x9f | 0x2000..=0x200d | 0xfeff | 0xfffe | 0xffff)
}

fn decode_percent(uri: &str) -> String {
    if !uri.contains('%') {
        return uri.to_string();
    }
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return uri.to_string();
            }
            let (Some(hi), Some(lo)) = ((bytes[i + 1] as char).to_digit(16), (bytes[i + 2] as char).to_digit(16)) else {
                return uri.to_string();
            };
            out.push(((hi << 4) | lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| uri.to_string())
}

fn strip_html_entities_and_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_ctrl(c) {
            i += 1;
            continue;
        }
        // &(newline|tab);
        if c == '&' {
            let tail: String = chars[i..chars.len().min(i + 10)].iter().collect();
            let lower = tail.to_ascii_lowercase();
            if lower.starts_with("&newline;") {
                i += 9;
                continue;
            }
            if lower.starts_with("&tab;") {
                i += 5;
                continue;
            }
            // &#(\w+);?
            if i + 2 < chars.len() && chars[i + 1] == '#' && (chars[i + 2].is_ascii_alphanumeric() || chars[i + 2] == '_') {
                let mut j = i + 2;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                let word: String = chars[i + 2..j].iter().collect();
                if j < chars.len() && chars[j] == ';' {
                    j += 1;
                }
                if let Ok(code) = word.parse::<u32>() {
                    if let Some(ch) = char::from_u32(code & 0xffff) {
                        if !is_ctrl(ch) {
                            out.push(ch);
                        }
                    }
                }
                i = j;
                continue;
            }
        }
        // (\\|%5[cC])((%(6[eE]|72|74))|[nrt])
        let backslash_len = if c == '\\' {
            Some(1)
        } else if c == '%' && i + 2 < chars.len() && chars[i + 1] == '5' && matches!(chars[i + 2], 'c' | 'C') {
            Some(3)
        } else {
            None
        };
        if let Some(blen) = backslash_len {
            let k = i + blen;
            if k < chars.len() && matches!(chars[k], 'n' | 'r' | 't') {
                i = k + 1;
                continue;
            }
            if k + 2 < chars.len() && chars[k] == '%' {
                let esc2 = (chars[k + 1], chars[k + 2]);
                if matches!(esc2, ('6', 'e' | 'E') | ('7', '2' | '4')) {
                    i = k + 3;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

// Removes control characters, entity codes, and escapes that could hide a scheme.
fn unwrap(url: &str, percent: bool) -> Option<String> {
    let mut out = if percent { decode_percent(url.trim()) } else { url.trim().to_string() };
    for _ in 0..32 {
        let next = strip_html_entities_and_escapes(&out);
        let next = next.trim().to_string();
        let next = if percent { decode_percent(&next) } else { next };
        if next == out {
            return Some(out);
        }
        out = next;
    }
    None
}

fn find_scheme_prefix(decoded: &str) -> Option<&str> {
    for line in decoded.lines() {
        let lower = line.to_ascii_lowercase();
        let end = if let Some(pos) = lower.rfind(":&colon;") {
            Some(pos + 8)
        } else {
            let colon = lower.rfind(':').map(|p| p + 1);
            let ecolon = lower.rfind("&colon;").map(|p| p + 7);
            match (colon, ecolon) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            }
        };
        if let Some(end) = end {
            if end > 1 && !line[..end].starts_with(':') && !lower[..end].starts_with("&colon;") {
                return Some(&line[..end]);
            }
        }
    }
    None
}

fn normalize_http_url(url: &str) -> Option<String> {
    let colon = url.find(':')?;
    let scheme = url[..colon].to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Some(url.to_string());
    }
    let rest = url[colon + 1..].trim_start_matches('/');
    if rest.is_empty() {
        return None;
    }
    let delim = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..delim];
    if authority.is_empty() || authority.chars().any(|c| c.is_whitespace() || is_ctrl(c)) {
        return None;
    }
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(at) => (&authority[..=at], &authority[at + 1..]),
        None => ("", authority),
    };
    if hostport.is_empty() {
        return None;
    }
    let host_lower = hostport.to_ascii_lowercase();
    let default_port = if scheme == "http" { ":80" } else { ":443" };
    let host_clean = host_lower.strip_suffix(default_port).unwrap_or(&host_lower);
    if host_clean.is_empty() {
        return None;
    }
    let tail = &rest[delim..];
    let path_and_query = if tail.is_empty() || tail.starts_with('?') || tail.starts_with('#') {
        format!("/{}", tail)
    } else {
        tail.to_string()
    };
    Some(format!("{}://{}{}{}", scheme, userinfo, host_clean, path_and_query))
}

// Refuses what Mermaid's check of click targets refuses, while keeping percent escapes intact.
pub fn sanitize_url(url: &str) -> String {
    if url.is_empty() {
        return BLANK.into();
    }
    let (Some(decoded), Some(kept)) = (unwrap(url, true), unwrap(url, false)) else {
        return BLANK.into();
    };
    if decoded.is_empty() || kept.is_empty() {
        return BLANK.into();
    }
    if decoded.starts_with('.') || decoded.starts_with('/') {
        return kept;
    }
    let trimmed_decoded = decoded.trim_start();
    let Some(scheme_match) = find_scheme_prefix(trimmed_decoded) else {
        return kept;
    };
    let protocol = scheme_match.to_ascii_lowercase();
    let protocol = protocol.trim();
    let bare_proto = protocol.trim_start_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    if ["javascript", "data", "vbscript"].iter().any(|bad| bare_proto.starts_with(bad)) {
        return BLANK.into();
    }
    let normalized = kept.trim_start().replace('\\', "/");
    if protocol == "mailto:" || protocol.contains("://") {
        return normalized;
    }
    if protocol == "http:" || protocol == "https:" {
        return normalize_http_url(&normalized).unwrap_or_else(|| BLANK.into());
    }
    normalized
}

// A URL as written in a diagram: entity codes are read first, so they cannot hide a scheme.
pub fn safe_url(url: &str) -> String {
    sanitize_url(&decode_entities(url))
}

fn allowed<S: AsRef<str>>(url: &str, schemes: &[S]) -> bool {
    let bytes = url.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'/' | b'\\') && matches!(bytes[1], b'/' | b'\\') {
        return false;
    }
    if let Some(colon) = url.find(':') {
        let head = &url[..colon];
        let valid_scheme = head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && head.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
        if valid_scheme {
            let lower = head.to_ascii_lowercase();
            return schemes.iter().any(|s| s.as_ref().eq_ignore_ascii_case(&lower));
        }
    }
    true
}

fn allow<'a, S: AsRef<str>>(url: &'a str, schemes: &[S]) -> Option<&'a str> {
    let decoded = unwrap(url, true)?;
    (allowed(url, schemes) && allowed(&decoded, schemes)).then_some(url)
}

pub fn link_url<'a>(url: &'a str, options: &RenderOptions) -> Option<&'a str> {
    if options.links == Some(false) {
        return None;
    }
    match &options.link_schemes {
        Some(list) => allow(url, list),
        None => allow(url, LINK_SCHEMES),
    }
}

pub fn image_url<'a>(url: &'a str, options: &RenderOptions) -> Option<&'a str> {
    if options.images == Some(false) {
        return None;
    }
    match &options.image_schemes {
        Some(list) => allow(url, list),
        None => allow(url, IMAGE_SCHEMES),
    }
}

pub fn internal_link(name: &str, options: &RenderOptions) -> bool {
    let url = sanitize_url(name);
    if url == BLANK || link_url(&url, options).is_none() {
        return false;
    }
    let cleaned: String = name
        .chars()
        .filter(|&c| !matches!(c as u32, 0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f | 0xfffe | 0xffff) && !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let seen = cleaned.trim_start_matches(|c: char| (c as u32) <= 0x20);
    let _ = has_invalid_xml;
    link_url(seen, options).is_some()
}

pub fn rel_attr(options: &RenderOptions) -> String {
    let rel = options.link_rel.as_deref().unwrap_or("noopener");
    if rel.is_empty() {
        String::new()
    } else {
        format!(" rel=\"{}\"", esc(rel))
    }
}

pub fn linked(body: String, url: &str, id: &str, links: &mut Vec<LinkInfo>, options: &RenderOptions, attrs: &str) -> String {
    let Some(href) = link_url(url, options) else { return body };
    links.push(LinkInfo { id: id.to_string(), href: href.to_string(), internal: false });
    format!("<a href=\"{}\"{}{}>{}</a>", esc(href), attrs, rel_attr(options), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes() {
        let opts = RenderOptions::default();
        assert_eq!(safe_url("https://example.com"), "https://example.com/");
        assert_eq!(safe_url("https://EXAMPLE.COM/a?b=1"), "https://example.com/a?b=1");
        assert_eq!(safe_url(" java\tscript:alert(1)"), BLANK);
        assert_eq!(safe_url("jav&#x09;ascript:alert(1)"), BLANK);
        assert_eq!(safe_url("&#106;avascript:alert(1)"), BLANK);
        assert_eq!(safe_url("%0ajavascript:alert(1)"), BLANK);
        assert_eq!(link_url("file:///etc/passwd", &opts), None);
        assert_eq!(link_url("notes/a.md", &opts), Some("notes/a.md"));
        assert!(!internal_link("javascript:alert(1)", &opts));
        assert!(internal_link("My note", &opts));
    }
}
