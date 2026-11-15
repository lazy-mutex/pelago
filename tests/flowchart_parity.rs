use std::collections::HashSet;
use std::path::Path;
use pelago::errors::ErrorCode;
use pelago::svg::edges::{fill_under, Fill, Under};
use pelago::svg::theme::resolve_style;
use pelago::value::{parse_json, Value};
use pelago::{detect_type, render, supports, LinkInfo, RenderOptions, Theme};

fn raw_opts() -> RenderOptions<'static> {
    RenderOptions { raw: true, ..Default::default() }
}

fn plain_opts() -> RenderOptions<'static> {
    RenderOptions::default()
}

fn invalid_xml_char(c: char) -> bool {
    matches!(c as u32, 0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f | 0x7f | 0xfffe | 0xffff)
}

fn valid_entity_after_amp(s: &str) -> bool {
    let Some(semi) = s.find(';') else { return false };
    let ent = &s[..semi];
    if matches!(ent, "amp" | "lt" | "gt" | "quot" | "apos") {
        return true;
    }
    if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
        return !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(dec) = ent.strip_prefix('#') {
        return !dec.is_empty() && dec.bytes().all(|b| b.is_ascii_digit());
    }
    false
}

fn check_amps(s: &str, ctx: &str) -> Result<(), String> {
    let mut from = 0;
    while let Some(rel) = s[from..].find('&') {
        let at = from + rel;
        if !valid_entity_after_amp(&s[at + 1..]) {
            return Err(format!("Unescaped \"&\" in {}", ctx));
        }
        from = at + 1;
    }
    Ok(())
}

fn decode_attr(val: &str) -> String {
    if !val.contains('&') {
        return val.to_string();
    }
    let mut out = String::new();
    let mut rest = val;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        let semi = rest.find(';').unwrap();
        let ent = &rest[..semi];
        let ch = match ent {
            "quot" => '"',
            "apos" => '\'',
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                char::from_u32(u32::from_str_radix(&ent[2..], 16).unwrap_or(0).min(0x10ffff)).unwrap_or('\u{fffd}')
            }
            _ => char::from_u32(ent[1..].parse::<u32>().unwrap_or(0).min(0x10ffff)).unwrap_or('\u{fffd}'),
        };
        out.push(ch);
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    out
}

struct XmlElement {
    name: String,
    attrs: Vec<(String, String)>,
}

fn parse_well_formed(xml: &str) -> Result<Vec<XmlElement>, String> {
    if let Some(bad) = xml.chars().find(|&c| invalid_xml_char(c)) {
        return Err(format!("Invalid XML character U+{:04X}", bad as u32));
    }
    let mut stack: Vec<String> = Vec::new();
    let mut elements: Vec<XmlElement> = Vec::new();
    let mut i = 0;
    while i < xml.len() {
        let lt = xml[i..].find('<').map(|p| i + p);
        let text = &xml[i..lt.unwrap_or(xml.len())];
        if text.contains('>') {
            return Err(format!("Unescaped \">\" in text: {:?}", &text[..text.len().min(40)]));
        }
        check_amps(text, "text")?;
        let Some(lt) = lt else { break };
        let Some(gt) = xml[lt + 1..].find('>').map(|p| lt + 1 + p) else {
            return Err(format!("Unclosed tag at {}", lt));
        };
        let inside = &xml[lt + 1..gt];
        if inside.contains('<') {
            return Err(format!("Nested '<' inside tag at {}", lt));
        }
        let closing = inside.starts_with('/');
        let self_closing = !closing && inside.ends_with('/');
        let body = if closing {
            &inside[1..]
        } else if self_closing {
            &inside[..inside.len() - 1]
        } else {
            inside
        };
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let name = &body[..name_end];
        if name.is_empty()
            || !name.chars().next().unwrap().is_ascii_alphabetic()
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-'))
        {
            return Err(format!("Malformed tag name {:?}", name));
        }
        let attr_str = body[name_end..].trim();
        if closing && !attr_str.is_empty() {
            return Err(format!("Attributes on closing tag </{}>", name));
        }
        check_amps(attr_str, &format!("attributes of <{}>", name))?;
        let mut seen = HashSet::new();
        let mut attrs = Vec::new();
        let mut rest = attr_str;
        while !rest.is_empty() {
            let eq = rest.find('=').ok_or_else(|| format!("Malformed attribute in <{}>: {:?}", name, rest))?;
            let aname = &rest[..eq];
            if aname.is_empty()
                || aname.chars().any(char::is_whitespace)
                || !aname.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.' | '-'))
            {
                return Err(format!("Invalid attribute name {:?}", aname));
            }
            let after_eq = &rest[eq + 1..];
            if !after_eq.starts_with('"') {
                return Err(format!("Unquoted attribute {} on <{}>", aname, name));
            }
            let close_q = after_eq[1..].find('"').ok_or_else(|| format!("Unclosed quote on {}", aname))?;
            let raw_val = &after_eq[1..1 + close_q];
            if !seen.insert(aname.to_string()) {
                return Err(format!("<{}> has {} twice", name, aname));
            }
            attrs.push((aname.to_string(), decode_attr(raw_val)));
            let tail = &after_eq[2 + close_q..];
            if !tail.is_empty() && !tail.starts_with(char::is_whitespace) {
                return Err(format!("Missing whitespace after {} in <{}>", aname, name));
            }
            rest = tail.trim_start();
        }
        if closing {
            let open = stack.pop().ok_or_else(|| format!("Unexpected </{}>", name))?;
            if open != name {
                return Err(format!("</{}> closes <{}>", name, open));
            }
        } else {
            elements.push(XmlElement { name: name.to_string(), attrs });
            if !self_closing {
                stack.push(name.to_string());
            }
        }
        i = gt + 1;
    }
    if let Some(open) = stack.last() {
        return Err(format!("Unclosed <{}>", open));
    }
    Ok(elements)
}

fn assert_inert(svg: &str, where_ctx: &str) {
    let elements = parse_well_formed(svg).unwrap_or_else(|e| panic!("{}: {}", where_ctx, e));
    assert!(!svg.contains("NaN"), "{}: contains NaN", where_ctx);
    assert!(!svg.contains("undefined"), "{}: contains undefined", where_ctx);

    let allowed_els: HashSet<&str> = [
        "svg", "g", "rect", "circle", "ellipse", "polygon", "path", "text", "tspan", "a", "title", "desc", "image",
    ]
    .into_iter()
    .collect();
    let allowed_attrs: HashSet<&str> = [
        "xmlns", "class", "viewBox", "width", "height", "style", "font-family", "font-size", "font-weight", "font-style",
        "text-decoration", "baseline-shift", "fill", "fill-opacity", "opacity", "stroke", "stroke-width",
        "stroke-dasharray", "stroke-linecap", "stroke-linejoin", "role", "aria-roledescription", "aria-labelledby",
        "aria-describedby", "id", "data-id", "data-icon", "data-href", "transform", "x", "y", "rx", "ry", "r", "cx",
        "cy", "d", "points", "text-anchor", "xml:space", "href", "target", "rel", "preserveAspectRatio",
        "pointer-events",
    ]
    .into_iter()
    .collect();
    let style_props: HashSet<&str> = [
        "fill", "fill-opacity", "stroke", "stroke-width", "stroke-dasharray", "stroke-dashoffset", "stroke-linecap",
        "stroke-linejoin", "stroke-opacity", "opacity", "rx", "ry", "font-size", "font-family", "font-weight",
        "font-style", "text-decoration", "letter-spacing", "word-spacing", "max-width",
    ]
    .into_iter()
    .collect();
    let numeric_attrs: HashSet<&str> = ["x", "y", "width", "height", "r", "cx", "cy", "rx", "ry"].into_iter().collect();

    for el in &elements {
        assert!(allowed_els.contains(el.name.as_str()), "{}: element <{}>", where_ctx, el.name);
        for (name, val) in &el.attrs {
            assert!(allowed_attrs.contains(name.as_str()), "{}: attribute {} on <{}>", where_ctx, name, el.name);
            if numeric_attrs.contains(name.as_str()) {
                let ok = val == "100%" || val.parse::<f64>().is_ok();
                assert!(ok, "{}: {}=\"{}\" is not a number", where_ctx, name, val);
            }
            if name == "href" {
                assert!(el.name == "a" || el.name == "image", "{}: href on <{}>", where_ctx, el.name);
                let seen: String = val.chars().filter(|c| !matches!(c, '\t' | '\n' | '\r')).collect();
                let seen = seen.trim_start_matches(|c: char| (c as u32) <= 0x20);
                let lower = seen.to_ascii_lowercase();
                assert!(
                    !lower.starts_with("javascript:") && !lower.starts_with("vbscript:") && !lower.starts_with("data:"),
                    "{}: href={:?} can run script",
                    where_ctx,
                    val
                );
                assert!(
                    !seen.starts_with("//") && !seen.starts_with("\\\\") && !seen.starts_with("/\\") && !seen.starts_with("\\/"),
                    "{}: network path href={:?}",
                    where_ctx,
                    val
                );
                if let Some(colon) = seen.find(':') {
                    let head = &seen[..colon];
                    if head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                        && head.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
                    {
                        assert!(
                            matches!(head.to_ascii_lowercase().as_str(), "http" | "https" | "mailto" | "tel"),
                            "{}: disallowed scheme in href={:?}",
                            where_ctx,
                            val
                        );
                    }
                }
            } else if name == "target" {
                assert!(matches!(val.as_str(), "_self" | "_blank" | "_parent" | "_top"), "{}: target={:?}", where_ctx, val);
            } else if name == "style" {
                let lower = val.to_ascii_lowercase();
                assert!(
                    !lower.contains("url(")
                        && !lower.contains("url ")
                        && !lower.contains("expression")
                        && !lower.contains("@import")
                        && !lower.contains("javascript:")
                        && !val.contains(['<', '>', '{', '}', '\\']),
                    "{}: unsafe style={:?}",
                    where_ctx,
                    val
                );
                for decl in val.split(';') {
                    let trimmed = decl.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let colon = trimmed.find(':').unwrap_or(0);
                    let prop = trimmed[..colon].trim();
                    let root_ok = el.name == "svg"
                        && (matches!(trimmed, "height:auto" | "text-transform:none" | "direction:ltr")
                            || prop == "color"
                            || (prop == "font" && trimmed.ends_with("px sans-serif"))
                            || prop.starts_with("--pele-series-"));
                    assert!(
                        root_ok || style_props.contains(prop) || prop.starts_with("--_"),
                        "{}: unexpected style property {} in {:?}",
                        where_ctx,
                        prop,
                        trimmed
                    );
                }
            } else if matches!(name.as_str(), "fill" | "stroke" | "font-family" | "rx") {
                let lower = val.to_ascii_lowercase();
                assert!(!lower.contains("url(") && !lower.contains("javascript:"), "{}: unsafe {}={:?}", where_ctx, name, val);
            }
        }
    }
}

fn load_pele_flowchart_corpus() -> Vec<String> {
    let mut seeds = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |s: String| {
        if seen.insert(s.clone()) {
            seeds.push(s);
        }
    };
    push("flowchart TD\n  A[Mermaid text] --> B{Flowchart?}\n  B -- Yes --> C[Pele]\n  B -- No --> D[Mermaid]\n  C --> E([SVG])\n  D --> E".into());
    push("flowchart LR\n  subgraph one [Group]\n    A[Start] --> B((Circle))\n  end\n  B --> C{Decision}".into());

    let docs_path = Path::new("/home/muka/pele/tests/corpus/flowchart-docs.json");
    if let Ok(raw) = std::fs::read_to_string(docs_path) {
        if let Ok(Value::Arr(items)) = parse_json(&raw) {
            for item in items {
                if let Some(s) = item.as_str() {
                    push(s.to_string());
                }
            }
        }
    }

    let compat_dir = Path::new("/home/muka/pele/tests/compat/flowchart");
    if let Ok(entries) = std::fs::read_dir(compat_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(fname) = path.file_name().and_then(|n| n.to_str()) else { continue };
            if !(fname.ends_with(".spec.js") || fname.ends_with(".spec.ts")) {
                continue;
            }
            if let Ok(src) = std::fs::read_to_string(&path) {
                for lit in extract_js_string_literals(&src) {
                    if lit.contains("graph") || lit.contains("flowchart") {
                        push(lit);
                    }
                }
            }
        }
    }
    seeds
}

fn extract_js_string_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let q = chars[i];
        if !matches!(q, '`' | '\'' | '"') {
            i += 1;
            continue;
        }
        i += 1;
        let mut buf = String::new();
        let mut closed = false;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' && i + 1 < chars.len() {
                let next = chars[i + 1];
                match next {
                    'n' => buf.push('\n'),
                    'r' => buf.push('\r'),
                    't' => buf.push('\t'),
                    '\\' => buf.push('\\'),
                    '\'' => buf.push('\''),
                    '"' => buf.push('"'),
                    '`' => buf.push('`'),
                    _ => {
                        buf.push('\\');
                        buf.push(next);
                    }
                }
                i += 2;
                continue;
            }
            if q != '`' && c == '\n' {
                break;
            }
            if c == q {
                closed = true;
                i += 1;
                break;
            }
            buf.push(c);
            i += 1;
        }
        if closed && !buf.contains("${") {
            out.push(buf);
        }
    }
    out
}

#[test]
fn corpus_renders_inert_in_both_raw_and_resolved_modes() {
    let corpus = load_pele_flowchart_corpus();
    let mut rendered_count = 0usize;
    for src in &corpus {
        let Ok(raw_res) = render(src, &raw_opts()) else { continue };
        rendered_count += 1;
        let ctx = format!("raw {:?}", &src[..src.len().min(80)]);
        assert_inert(&raw_res.svg, &ctx);
        assert_eq!(render(src, &raw_opts()).unwrap().svg, raw_res.svg, "deterministic raw SVG");

        let plain_res = render(src, &plain_opts()).expect("resolved render succeeds whenever raw succeeds");
        let pctx = format!("plain {:?}", &src[..src.len().min(80)]);
        assert_inert(&plain_res.svg, &pctx);
        assert!(!plain_res.svg.contains("=\"var(--"), "{}: unresolved attribute var()", pctx);
        assert!(!plain_res.svg.contains(":var(--"), "{}: unresolved style var()", pctx);
        assert!(!plain_res.svg.contains("color-mix("), "{}: unresolved color-mix()", pctx);
        assert_eq!(plain_res.width, raw_res.width);
        assert_eq!(plain_res.height, raw_res.height);
    }
    if Path::new("/home/muka/pele/tests/compat/flowchart").exists() {
        assert!(rendered_count > 300, "expected >300 rendered corpus flowcharts, got {}", rendered_count);
    }
}

#[test]
fn flowchart_features_and_pele_parity() {
    // Size, type, and direction
    let res = render("flowchart LR\n  A --> B", &raw_opts()).unwrap();
    assert_eq!(res.diagram, "flowchart");
    assert_eq!(res.mermaid, "flowchart-v2");
    assert!(res.width > 100.0);
    assert!(res.height > 30.0);
    assert!(res.svg.contains(&format!("viewBox=\"0 0 {} {}\"", res.width as i64, res.height as i64)));

    let tall = render("flowchart TD\n  A --> B --> C", &raw_opts()).unwrap();
    let wide = render("flowchart LR\n  A --> B --> C", &raw_opts()).unwrap();
    assert!(tall.height > tall.width);
    assert!(wide.width > wide.height);

    // Escaping and HTML tags
    let escaped = render("flowchart TD\n  A[\"<img src=x onerror=alert(1)> & more\"]", &raw_opts()).unwrap();
    assert!(!escaped.svg.contains("<img"));
    assert!(escaped.svg.contains("&amp; more"));
    assert_inert(&escaped.svg, "escaped");

    assert!(render("flowchart LR\n  A[\"List<T>\"]", &raw_opts()).unwrap().svg.contains("List&lt;T&gt;"));
    assert!(render("flowchart LR\n  A[\"<T>\"]", &raw_opts()).unwrap().svg.contains("&lt;T&gt;"));

    // Entity decoding once
    let ent = render("flowchart LR\n  A[\"x #amp;lt; y #38;gt; z\"]", &raw_opts()).unwrap();
    assert!(ent.svg.contains("x &amp;lt; y &amp;gt; z"));

    // style, classDef, linkStyle
    let styled = render(
        "flowchart TD\n  A --> B\n  style A fill:#f9f,stroke:#333,stroke-width:4px,color:#fff\n  classDef warn fill:#ff0\n  class B warn\n  linkStyle 0 stroke:#f00,stroke-width:3px",
        &raw_opts(),
    )
    .unwrap();
    assert!(styled.svg.contains("style=\"rx:var(--_r);fill:#f9f;stroke:#333;stroke-width:4px;\""));
    assert!(styled.svg.contains("style=\"fill:#fff;\""));
    assert!(styled.svg.contains("class=\"pele-node pele-shape-rect warn\" data-id=\"B\""));
    assert!(styled.svg.contains("style=\"stroke:#f00;stroke-width:3px;\""));
    assert!(styled.svg.contains("fill=\"#f00\""));

    // Same diagram in plain mode promotes rx and resolves vars
    let styled_plain = render(
        "flowchart TD\n  A --> B\n  style A rx:10,fill:#f9f,stroke:#333,stroke-width:4px,color:#fff",
        &plain_opts(),
    )
    .unwrap();
    assert!(styled_plain.svg.contains("rx=\"10\""));
    assert!(!styled_plain.svg.contains("rx:var("));

    // Links & internal-link
    let linked = render(
        "flowchart TD\n  A --> B --> C\n  click A \"https://example.com\" \"A tooltip\" _blank\n  click B \"javascript:alert(1)\"\n  click C call doSomething()",
        &raw_opts(),
    )
    .unwrap();
    assert!(linked.svg.contains("<a href=\"https://example.com/\" target=\"_blank\" rel=\"noopener\">"));
    assert!(linked.svg.contains("<title>A tooltip</title>"));
    assert!(!linked.svg.contains("about:blank"));
    assert!(!linked.svg.contains("doSomething"));
    assert_eq!(linked.links, vec![LinkInfo { id: "A".into(), href: "https://example.com/".into(), internal: false }]);

    let internal = render("flowchart TD\n  A[My note] --> B\n  class A internal-link;", &raw_opts()).unwrap();
    assert!(internal.svg.contains("<a class=\"internal-link\" href=\"My note\" rel=\"noopener\" data-href=\"My note\">"));
    assert_eq!(internal.links, vec![LinkInfo { id: "A".into(), href: "My note".into(), internal: true }]);

    // Hostile internal link omits href
    let bad_internal = render(
        "flowchart TD\n  A[\"javascript:alert(1)\"]:::internal-link",
        &RenderOptions { raw: true, link_schemes: Some(vec!["about".into()]), ..Default::default() },
    )
    .unwrap();
    assert!(bad_internal.svg.contains("<a class=\"internal-link\" data-href=\"javascript:alert(1)\">"));
    assert!(!bad_internal.svg.contains(" href="));

    // useMaxWidth: false
    let fixed_src = "---\nconfig:\n  flowchart:\n    useMaxWidth: false\n---\nflowchart TD\n  A --> B";
    assert!(!render(fixed_src, &raw_opts()).unwrap().svg.contains("max-width"));
    assert!(
        render(fixed_src, &RenderOptions { raw: true, responsive: Some(true), ..Default::default() })
            .unwrap()
            .svg
            .contains("max-width:100%")
    );

    // Sub/sup nesting
    let subsup = render("flowchart LR\n  A[\"<sup>a<sub>b<sup>c</sup>d</sub>e</sup>\"]", &raw_opts()).unwrap();
    assert!(subsup
        .svg
        .contains("<tspan baseline-shift=\"sub\" font-size=\"smaller\">d</tspan><tspan baseline-shift=\"super\" font-size=\"smaller\">e</tspan>"));

    // Contrasting ink & casings
    let fills = [
        Fill { bounds: [-20.0, -20.0, 100.0, 100.0], shape: " style=\"fill:white;\"".into() },
        Fill { bounds: [10.0, 0.0, 50.0, 50.0], shape: " style=\"fill:navy;\"".into() },
    ];
    assert!(matches!(fill_under("M0,10L8,10", &[0.0, 10.0, 15.0, 10.0], &fills, None), Under::Border));
    assert!(matches!(fill_under("M0,10L8,10", &[0.0, 10.0, 9.0, 10.0], &fills, None), Under::Fill(_)));

    assert_eq!(resolve_style(&["fill:url(http://example.com/x.png)", "stroke:red"]).shape, " style=\"stroke:red;\"");
    assert_eq!(resolve_style(&["fill:red;background:blue", "color:expression(alert(1))"]).shape, "");
}

#[test]
fn error_locations_match_written_source() {
    assert_eq!(detect_type("flowchart TD\n A"), Some("flowchart"));
    assert_eq!(detect_type("  graph LR\n A"), Some("flowchart"));
    assert_eq!(detect_type("%% comment\nsequenceDiagram\n A->>B: hi"), Some("sequence"));
    assert!(supports("flowchart TD\n A"));
    assert!(supports("sequenceDiagram\n A->>B: hi"));
    assert!(!supports("not a diagram\n hello"));

    let err1 = render("flowchart TD\nA[a#35;b] --> -->", &plain_opts()).unwrap_err();
    assert_eq!(err1.code, ErrorCode::Syntax);
    assert_eq!(err1.diagram, Some("flowchart"));
    assert_eq!(err1.line, 2);
    assert_eq!(err1.column, 15);
    assert_eq!(err1.snippet, "A[a#35;b] --> -->\n--------------^");
    assert!(err1.message.starts_with("Parse error on line 2:\nA[a#35;b] --> -->\n"));

    let err_shape = render("flowchart TD\na@{shape: \"#35;x\"}", &plain_opts()).unwrap_err();
    assert_eq!(err_shape.code, ErrorCode::Semantic);
    assert_eq!(err_shape.message, "No such shape: #35;x.");

    let err2 = render("---\ntitle: T\n---\n\n%% A comment\nflowchart TD\n  A --> B\n  C --> --> D", &plain_opts()).unwrap_err();
    assert_eq!(err2.line, 8);
    assert_eq!(err2.column, 9);
    assert!(err2.message.starts_with("Parse error on line 8:\n  C --> --> D\n--------^\n"));

    for end in ["", "\n", "\n%% A comment\n\n"] {
        let err_eof = render(&format!("flowchart LR\n  A -->{}", end), &plain_opts()).unwrap_err();
        assert_eq!((err_eof.line, err_eof.column), (2, 8), "end={:?}", end);
    }
}

#[test]
fn hostile_payloads_stay_inert() {
    let payloads = [
        "<script>alert(1)</script>",
        "\"><script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<svg onload=alert(1)>",
        "<a href=\"javascript:alert(1)\">x</a>",
        "</text></g></svg><script>alert(1)</script>",
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        " javascript:alert(1)",
        "jav&#x09;ascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "vbscript:msgbox(1)",
        "\" onmouseover=\"alert(1)",
        "#60;script#62;alert(1)#60;/script#62;",
        "#106;avascript:alert(1)",
        "url#40;javascript:alert#40;1#41;#41;",
        "url(javascript:alert(1))",
        "expression(alert(1))",
        "red;background:url(//evil.example/x)",
        "\u{0000}\u{0001}\u{0008}\u{000b}\u{001f}",
        "a\u{fffe}b\u{ffff}c#65534;f",
        "%0ajavascript:alert(1)",
        "file:///etc/passwd",
        "//evil.example/share",
        "\\\\evil.example\\share",
        "smb://evil.example/share",
        "\\\"><script>",
    ];
    let templates: &[fn(&str) -> String] = &[
        |p| format!("flowchart TD\n  A[{}] --> B", p),
        |p| format!("flowchart TD\n  A[\"{}\"] --> B", p),
        |p| format!("flowchart TD\n  A[\"`{}`\"] --> B", p),
        |p| format!("flowchart TD\n  A -->|{}| B", p),
        |p| format!("flowchart TD\n  subgraph s [\"{}\"]\n  A\n  end", p),
        |p| format!("flowchart TD\n  A --> B\n  style A fill:{},color:{},font-family:{}", p, p, p),
        |p| format!("flowchart TD\n  A --> B\n  click A href \"{}\" \"{}\" _blank", p, p),
        |p| format!("flowchart TD\n  A[\"{}\"]:::internal-link --> B", p),
        |p| format!("flowchart TD\n  accTitle: {}\n  accDescr: {}\n  A --> B", p, p),
        |p| format!("flowchart TD\n  A@{{ img: \"{}\", label: \"x\" }} --> B", p),
        |p| format!("---\ntitle: \"{}\"\n---\nflowchart TD\n  A --> B", p),
    ];

    let mut checked = 0usize;
    for tpl in templates {
        for &payload in &payloads {
            let src = tpl(payload);
            if let Ok(res) = render(&src, &plain_opts()) {
                assert_inert(&res.svg, &src);
                checked += 1;
            }
            if let Ok(res) = render(&src, &raw_opts()) {
                assert_inert(&res.svg, &src);
            }
        }
    }
    assert!(checked > 100);

    for &payload in &payloads {
        let opts = RenderOptions {
            id_prefix: Some(payload.into()),
            font_family: Some(payload.into()),
            font_family_mono: Some(payload.into()),
            theme: Theme { bg: Some(payload.into()), surface: Some(payload.into()), ..Default::default() },
            ..Default::default()
        };
        let res = render("flowchart TD\n  accTitle: t\n  A --> B", &opts).unwrap();
        assert_inert(&res.svg, payload);
    }
}
