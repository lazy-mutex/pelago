use super::entities::decode_entities;
use super::measurer::{style, TextMeasurer};
use crate::svg::builder::js_round;

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: u8,
    pub width: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Label {
    pub lines: Vec<Vec<Span>>,
    pub widths: Vec<f64>,
    pub width: f64,
    pub height: f64,
    pub size: f64,
    pub line_height: f64,
}

pub const LINE_HEIGHT: f64 = 1.5;
// Lines of one wrapped label sit closer, so the label reads as one block.
const WRAPPED_LINE_HEIGHT: f64 = 1.3;

const SHIFT: u8 = style::SUB | style::SUP;

fn tag_style(tag: &str) -> u8 {
    match tag {
        "b" | "strong" => style::BOLD,
        "i" | "em" | "cite" | "dfn" | "var" => style::ITALIC,
        "code" | "kbd" | "samp" | "tt" => style::MONO,
        "sub" => style::SUB,
        "sup" => style::SUP,
        "u" | "ins" => style::UNDERLINE,
        _ => 0,
    }
}

const HTML_TAGS: &str = " a abbr address article aside audio b bdi bdo big blockquote body br button caption center cite code col \
colgroup data dd del details dfn dialog div dl dt em embed fieldset figcaption figure font footer form h1 h2 \
h3 h4 h5 h6 head header hr html i iframe img input ins kbd label legend li link main map mark meta meter nav \
noscript object ol optgroup option output p picture pre progress q rp rt ruby s samp script section select \
small source span strike strong style sub summary sup table tbody td template textarea tfoot th thead time \
title tr track tt u ul var video wbr ";

fn is_html_tag(tag: &str) -> bool {
    HTML_TAGS.contains(&format!(" {} ", tag))
}

// Break between any two Chinese or Japanese characters. Hangul is left out: Korean puts spaces between words.
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2e80..=0x9fff | 0xf900..=0xfaff | 0xff00..=0xff60)
}

const CLOSE: &str = "、。，．：；！？）］｝」』】〕〉》〙〗ー…‥・ゝゞぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮ,.:;!?)]}";
const OPEN: &str = "（［｛「『【〔〈《〘〖([{";

// Break hints: a zero-width space, and a soft hyphen, which shows as '-' where a line breaks.
fn is_hint(c: char) -> bool {
    c == '\u{200b}' || c == '\u{ad}'
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

// Splits text into the pieces a line may break between:
// /\s*(?:[OPEN]*[CJK][CLOSE]*|[^\s CJK HINT]+[HINT]*[CLOSE]*|[HINT]+)\s*|\s+/g
fn pieces(text: &str) -> Vec<String> {
    let s: Vec<char> = text.chars().collect();
    let n = s.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && s[j].is_whitespace() {
            j += 1;
        }
        if j == n {
            out.push(s[i..n].iter().collect());
            break;
        }
        let mut k = j;
        while k < n && OPEN.contains(s[k]) {
            k += 1;
        }
        let mut end = None;
        if k < n && is_cjk(s[k]) {
            end = Some(k + 1);
        } else {
            // The opening brackets were taken greedily; give them back until one is itself a CJK character.
            let mut m = k;
            while m > j {
                m -= 1;
                if is_cjk(s[m]) {
                    end = Some(m + 1);
                    break;
                }
            }
        }
        let mut k = match end {
            Some(mut k) => {
                while k < n && CLOSE.contains(s[k]) {
                    k += 1;
                }
                k
            }
            None if is_hint(s[j]) => {
                let mut k = j;
                while k < n && is_hint(s[k]) {
                    k += 1;
                }
                k
            }
            None => {
                let mut k = j;
                while k < n && !s[k].is_whitespace() && !is_cjk(s[k]) && !is_hint(s[k]) {
                    k += 1;
                }
                while k < n && is_hint(s[k]) {
                    k += 1;
                }
                while k < n && CLOSE.contains(s[k]) {
                    k += 1;
                }
                k
            }
        };
        while k < n && s[k].is_whitespace() {
            k += 1;
        }
        out.push(s[i..k].iter().collect());
        i = k;
    }
    out
}

fn dedent(text: &str) -> String {
    let indent = |line: &str| line.chars().count() - line.trim_start().chars().count();
    let mut min = usize::MAX;
    for line in text.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        min = min.min(indent(line));
    }
    if min == usize::MAX || min == 0 {
        return text.to_string();
    }
    text.split('\n').map(|line| line.chars().skip(min.min(indent(line))).collect::<String>()).collect::<Vec<_>>().join("\n")
}

fn push_text(line: &mut Vec<Span>, text: &str, style: u8) {
    if text.is_empty() {
        return;
    }
    let decoded = decode_entities(text);
    match line.last_mut() {
        Some(last) if last.style == style => last.text.push_str(&decoded),
        _ => line.push(Span { text: decoded, style, width: 0.0 }),
    }
}

// Finds `fa:fa-name` from `from`: the start and end of the match.
fn find_icon(s: &[char], from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while i + 1 < s.len() {
        if s[i] == 'f' && s[i + 1] == 'a' {
            let mut j = i + 2;
            if j < s.len() && matches!(s[j], 'b' | 'k' | 'l' | 'r' | 's') {
                j += 1;
            }
            if j + 4 <= s.len() && s[j] == ':' && s[j + 1] == 'f' && s[j + 2] == 'a' && s[j + 3] == '-' {
                let start = j + 4;
                let mut k = start;
                while k < s.len() && (is_word(s[k]) || s[k] == '-') {
                    k += 1;
                }
                if k > start {
                    return Some((i, k));
                }
            }
        }
        i += 1;
    }
    None
}

// No icon set is bundled, so an icon leaves no gap, nor the space that set it off from the text.
fn push_icons(line: &mut Vec<Span>, text: &str, style: u8) {
    if !text.contains(":fa-") {
        push_text(line, text, style);
        return;
    }
    let s: Vec<char> = text.chars().collect();
    let mut last = 0;
    while let Some((start, end)) = find_icon(&s, last) {
        push_text(line, &s[last..start].iter().collect::<String>(), style);
        last = end;
        if s.get(last) == Some(&' ') {
            last += 1;
        } else if last == s.len() {
            if let Some(before) = line.last_mut() {
                before.text.truncate(before.text.trim_end().len());
            }
        }
    }
    push_text(line, &s[last..].iter().collect::<String>(), style);
}

struct Open {
    style: u8,
    // Sub/sup stack; only the innermost tag shifts text.
    shifts: Vec<u8>,
}

// Matches /<\/?([a-zA-Z][\w-]*)(?:\s[^<>]*)?\/?>/ at `i`: the tag name and the length of the match.
fn match_tag(s: &[char], i: usize) -> Option<(String, usize)> {
    let mut j = i + 1;
    if s.get(j) == Some(&'/') {
        j += 1;
    }
    if !s.get(j).is_some_and(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let start = j;
    while j < s.len() && (is_word(s[j]) || s[j] == '-') {
        j += 1;
    }
    let name: String = s[start..j].iter().collect();
    if s.get(j).is_some_and(|c| c.is_whitespace()) {
        let mut k = j + 1;
        while k < s.len() && s[k] != '<' && s[k] != '>' {
            k += 1;
        }
        if s.get(k) == Some(&'>') {
            return Some((name, k + 1 - i));
        }
    }
    if s.get(j) == Some(&'/') {
        j += 1;
    }
    (s.get(j) == Some(&'>')).then(|| (name, j + 1 - i))
}

fn find(s: &[char], needle: &[char], from: usize) -> bool {
    (from..s.len().saturating_sub(needle.len() - 1)).any(|i| s[i..i + needle.len()] == *needle)
}

// Splits inline HTML and, for markdown labels, emphasis markers into styled spans.
fn inline(line: &mut Vec<Span>, source: &str, markdown: bool, base: u8, open: &mut Open) {
    let s: Vec<char> = source.chars().collect();
    let n = s.len();
    let mut style = open.style;
    let mut marked = 0u8;
    let mut text = String::new();
    let mut i = 0;
    macro_rules! flush {
        () => {
            push_icons(line, &text, style | open.shifts.last().copied().unwrap_or(0));
            text.clear();
        };
    }
    while i < n {
        let ch = s[i];
        if ch == '<' {
            if let Some((name, len)) = match_tag(&s, i) {
                let tag = name.to_lowercase();
                let closing = s[i + 1] == '/';
                let bit = tag_style(&tag);
                if bit & SHIFT != 0 {
                    flush!();
                    if !closing {
                        open.shifts.push(bit);
                    } else if let Some(at) = open.shifts.iter().rposition(|&b| b == bit) {
                        open.shifts.remove(at);
                    }
                } else if bit != 0 {
                    flush!();
                    open.style = if closing { (open.style & !bit) | (base & bit) } else { open.style | bit };
                    style = if closing { (style & !bit) | (base & bit) } else { style | bit };
                }
                // Other HTML tags are dropped and their text kept. Anything else shaped like a tag, such
                // as the type in List<T>, is text.
                if bit != 0 || is_html_tag(&tag) {
                    i += len;
                    continue;
                }
            }
        } else if markdown && (ch == '*' || ch == '_') {
            let double = s.get(i + 1) == Some(&ch);
            let bit = if double { style::BOLD } else { style::ITALIC };
            let len = if double { 2 } else { 1 };
            let opening = marked & bit == 0;
            let prev = if i > 0 { s[i - 1] } else { ' ' };
            let next = if i + len < n { s[i + len] } else { ' ' };
            let needle: &[char] = if double { &[ch, ch] } else { &[ch] };
            let can_open = opening && !next.is_whitespace() && find(&s, needle, i + len);
            let can_close = !opening && !prev.is_whitespace();
            let wordy = ch == '_' && is_word(prev) && is_word(next);
            if (can_open || can_close) && !wordy {
                flush!();
                marked = if can_open { marked | bit } else { marked & !bit };
                style = if can_open { style | bit } else { (style & !bit) | (base & bit) };
                i += len;
                continue;
            }
        }
        text.push(ch);
        i += 1;
    }
    flush!();
}

fn measure_spans(line: &mut [Span], measurer: &dyn TextMeasurer, size: f64) -> f64 {
    let mut total = 0.0;
    for span in line {
        span.width = measurer.width(&span.text, size, span.style);
        total += span.width;
    }
    total
}

fn drop_last_char(s: &str) -> &str {
    match s.char_indices().next_back() {
        Some((at, _)) => &s[..at],
        None => s,
    }
}

struct Wrapper<'a> {
    measurer: &'a dyn TextMeasurer,
    size: f64,
    current: Vec<Span>,
    width: f64,
    out: &'a mut Vec<Vec<Span>>,
    widths: &'a mut Vec<f64>,
}

impl Wrapper<'_> {
    fn commit(&mut self, hyphen: bool) {
        if let Some(last) = self.current.last_mut() {
            let mut trimmed = last.text.trim_end().to_string();
            if hyphen && last.text.ends_with('\u{ad}') {
                trimmed = format!("{}-", drop_last_char(&trimmed));
            }
            if trimmed != last.text {
                let w = self.measurer.width(&trimmed, self.size, last.style);
                self.width += w - last.width;
                last.width = w;
                last.text = trimmed;
            }
        }
        self.out.push(std::mem::take(&mut self.current));
        self.widths.push(self.width);
        self.width = 0.0;
    }
}

fn wrap(line: &[Span], measurer: &dyn TextMeasurer, size: f64, max: f64, out: &mut Vec<Vec<Span>>, widths: &mut Vec<f64>) {
    let mut w = Wrapper { measurer, size, current: Vec::new(), width: 0.0, out, widths };
    for (index, span) in line.iter().enumerate() {
        let parts = pieces(&span.text);
        // A piece ending in a soft hyphen fits if its '-' does, or if the rest of the word does, which
        // hides the hyphen. The line only grows along a word, so no later hyphen in it fits either,
        // and the remaining width of each word makes this one check.
        let soft = span.text.contains('\u{ad}');
        let n = if soft { parts.len() } else { 0 };
        let mut own = vec![0.0; n];
        let mut rest = vec![0.0; n + 1];
        for q in (0..n).rev() {
            // A soft hyphen before a space is no break of its own: the space is.
            let shy = parts[q].ends_with('\u{ad}');
            let end = parts[q].trim_end();
            own[q] = measurer.width(if shy { drop_last_char(end) } else { end }, size, span.style);
            // Word width beyond this span is unknown.
            rest[q] = if !shy {
                own[q]
            } else if q == n - 1 {
                if index == line.len() - 1 {
                    own[q]
                } else {
                    f64::INFINITY
                }
            } else {
                own[q] + rest[q + 1]
            };
        }
        for (p, piece) in parts.iter().enumerate() {
            let mut part: &str = piece;
            let part_width = if soft { own[p] } else { measurer.width(part.trim_end(), size, span.style) };
            let fits = if !soft || !part.ends_with('\u{ad}') {
                w.width + part_width <= max
            } else {
                w.width + measurer.width(&format!("{}-", drop_last_char(part)), size, span.style) <= max || w.width + rest[p] <= max
            };
            if w.width > 0.0 && !fits {
                // Breaking at whitespace leaves a preceding soft hyphen hidden.
                w.commit(part.trim_start() == part);
                part = part.trim_start();
            }
            match w.current.last_mut() {
                Some(last) if last.style == span.style => {
                    last.text.push_str(part);
                    // Remeasuring for each zero-width piece would take quadratic time.
                    if part_width > 0.0 || part.trim_end() != part {
                        let before = last.width;
                        last.width = measurer.width(&last.text, size, last.style);
                        w.width += last.width - before;
                    }
                }
                _ => {
                    let width = measurer.width(part, size, span.style);
                    w.current.push(Span { text: part.to_string(), style: span.style, width });
                    w.width += width;
                }
            }
        }
    }
    if !w.current.is_empty() || w.out.is_empty() {
        w.commit(false);
    }
}

// Whether the text has anything but plain characters: /[<\\\n*_]|fa[bklrs]?:fa-/
fn has_markup(raw: &str) -> bool {
    raw.contains(['<', '\\', '\n', '*', '_']) || (raw.contains(":fa-") && find_icon_prefix(raw))
}

fn find_icon_prefix(raw: &str) -> bool {
    ["fa:fa-", "fab:fa-", "fak:fa-", "fal:fa-", "far:fa-", "fas:fa-"].iter().any(|p| raw.contains(p))
}

// Splits at /<br\s*\/?>|\\n|\n/gi
fn split_breaks(source: &str) -> Vec<String> {
    let s: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut current = String::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c == '\n' {
            out.push(std::mem::take(&mut current));
            i += 1;
            continue;
        }
        if c == '\\' && s.get(i + 1) == Some(&'n') {
            out.push(std::mem::take(&mut current));
            i += 2;
            continue;
        }
        if c == '<' && i + 2 < s.len() && s[i + 1].eq_ignore_ascii_case(&'b') && s[i + 2].eq_ignore_ascii_case(&'r') {
            let mut j = i + 3;
            while j < s.len() && s[j].is_whitespace() {
                j += 1;
            }
            if s.get(j) == Some(&'/') {
                j += 1;
            }
            if s.get(j) == Some(&'>') {
                out.push(std::mem::take(&mut current));
                i = j + 1;
                continue;
            }
        }
        current.push(c);
        i += 1;
    }
    out.push(current);
    out
}

pub fn layout_label(raw: Option<&str>, markdown: bool, measurer: &dyn TextMeasurer, size: f64, max_width: f64, base: u8) -> Label {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else { return Label::default() };
    let line_height = js_round(size * LINE_HEIGHT);

    if !has_markup(raw) && !raw.contains('&') && !raw.contains('ﬂ') {
        let text = raw.trim();
        let width = measurer.width(text, size, base);
        if width <= max_width || !(text.contains(' ') || text.chars().any(|c| is_cjk(c) || is_hint(c))) {
            return Label {
                lines: vec![vec![Span { text: text.to_string(), style: base, width }]],
                widths: vec![width],
                width,
                height: line_height,
                size,
                line_height,
            };
        }
        let mut lines = Vec::new();
        let mut widths = Vec::new();
        wrap(&[Span { text: text.to_string(), style: base, width }], measurer, size, max_width, &mut lines, &mut widths);
        let widest = widths.iter().fold(0.0f64, |a, &b| a.max(b));
        let snug = if lines.len() > 1 { js_round(size * WRAPPED_LINE_HEIGHT) } else { line_height };
        return Label { height: lines.len() as f64 * snug, lines, widths, width: widest, size, line_height: snug };
    }

    let source = if markdown { dedent(&raw.replace("\r\n", "\n").replace('\r', "\n")).trim().to_string() } else { raw.trim().to_string() };
    let mut lines: Vec<Vec<Span>> = Vec::new();
    let mut widths: Vec<f64> = Vec::new();
    // HTML tags can span explicit breaks; Markdown emphasis stays local to each line.
    let mut open = Open { style: base, shifts: Vec::new() };
    for part in split_breaks(&source) {
        let mut line = Vec::new();
        inline(&mut line, part.trim(), markdown, base, &mut open);
        let w = measure_spans(&mut line, measurer, size);
        if w > max_width {
            wrap(&line, measurer, size, max_width, &mut lines, &mut widths);
        } else {
            lines.push(line);
            widths.push(w);
        }
    }
    let width = widths.iter().fold(0.0f64, |a, &b| a.max(b));
    let snug = if lines.len() > 1 { js_round(size * WRAPPED_LINE_HEIGHT) } else { line_height };
    Label { height: lines.len() as f64 * snug, lines, widths, width, size, line_height: snug }
}

pub fn line_middle(label: &Label, i: usize) -> f64 {
    (i as f64 + 0.5) * label.line_height
}

pub fn label_line(label: &Label, i: usize) -> Label {
    let width = label.widths[i];
    Label {
        lines: vec![label.lines[i].clone()],
        widths: vec![width],
        width,
        height: label.line_height,
        size: label.size,
        line_height: label.line_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::measurer::MetricsMeasurer;

    #[test]
    fn plain_label() {
        let l = layout_label(Some("Hello"), false, &MetricsMeasurer, 14.0, 200.0, 0);
        assert_eq!(l.lines.len(), 1);
        assert_eq!(l.line_height, 21.0);
    }

    #[test]
    fn wraps_and_styles() {
        let l = layout_label(Some("one two three four five six seven"), false, &MetricsMeasurer, 14.0, 80.0, 0);
        assert!(l.lines.len() > 1);
        assert!(l.widths.iter().all(|&w| w <= 80.0));
        let l = layout_label(Some("a <b>bold</b><br/>**x** _y_"), true, &MetricsMeasurer, 14.0, 200.0, 0);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.lines[0][1].style, style::BOLD);
        assert_eq!(l.lines[1][0].text, "x");
        assert_eq!(l.lines[1][2].style, style::ITALIC);
    }

    #[test]
    fn piece_boundaries() {
        assert_eq!(pieces("ab cd  "), vec!["ab ", "cd  "]);
        assert_eq!(pieces("日本語。x"), vec!["日", "本", "語。", "x"]);
    }
}
