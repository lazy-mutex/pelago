use std::fs;
use std::path::Path;
use pelago::value::{parse_json, Value};
use pelago::{detect_type, render, supports, RenderOptions};

fn expected_diagram_for_corpus(stem: &str) -> &'static str {
    match stem {
        "git" => "gitGraph",
        "quadrant" => "quadrantChart",
        "treeview" => "treeView",
        "agentflow" => "agentflow",
        "architecture" => "architecture",
        "block" => "block",
        "c4" => "c4",
        "class" => "class",
        "cynefin" => "cynefin",
        "er" => "er",
        "eventmodeling" => "eventmodeling",
        "flowchart" => "flowchart",
        "gantt" => "gantt",
        "info" => "info",
        "ishikawa" => "ishikawa",
        "journey" => "journey",
        "kanban" => "kanban",
        "mindmap" => "mindmap",
        "packet" => "packet",
        "pie" => "pie",
        "radar" => "radar",
        "railroad" => "railroad",
        "requirement" => "requirement",
        "sankey" => "sankey",
        "sequence" => "sequence",
        "state" => "state",
        "swimlane" => "swimlane",
        "timeline" => "timeline",
        "treemap" => "treemap",
        "usecase" => "usecase",
        "venn" => "venn",
        "wardley" => "wardley",
        "xychart" => "xychart",
        other => panic!("unexpected corpus stem: {other}"),
    }
}

fn validate_xml_well_formed(svg: &str, label: &str) {
    let bytes = svg.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    let mut stack: Vec<String> = Vec::new();
    while i < n {
        if bytes[i] == b'<' {
            if svg[i..].starts_with("<!--") {
                let end = svg[i + 4..]
                    .find("-->")
                    .unwrap_or_else(|| panic!("{label}: unclosed comment at {i}"));
                i += 4 + end + 3;
                continue;
            }
            i += 1;
            let closing = if i < n && bytes[i] == b'/' {
                i += 1;
                true
            } else {
                false
            };
            let name_start = i;
            while i < n && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' && bytes[i] != b'/' {
                assert!(
                    bytes[i] != b'"' && bytes[i] != b'\'' && bytes[i] != b'=',
                    "{label}: malformed tag name near byte {i}: {:?}",
                    &svg[name_start.saturating_sub(20)..(i + 30).min(n)]
                );
                i += 1;
            }
            let tag_name = &svg[name_start..i];
            assert!(!tag_name.is_empty(), "{label}: empty tag name at {i}");
            let mut self_closing = false;
            while i < n && bytes[i] != b'>' {
                while i < n && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < n && bytes[i] == b'>' {
                    break;
                }
                if i + 1 < n && bytes[i] == b'/' && bytes[i + 1] == b'>' {
                    self_closing = true;
                    i += 1;
                    break;
                }
                let attr_start = i;
                while i < n
                    && !bytes[i].is_ascii_whitespace()
                    && bytes[i] != b'='
                    && bytes[i] != b'>'
                    && bytes[i] != b'/'
                {
                    assert!(
                        bytes[i] != b'"' && bytes[i] != b'\'',
                        "{label}: unexpected quote in attr name near {i}: {:?}",
                        &svg[attr_start.saturating_sub(30)..(i + 30).min(n)]
                    );
                    i += 1;
                }
                let attr_name = &svg[attr_start..i];
                assert!(
                    !attr_name.is_empty(),
                    "{label}: expected attr name at {i}: {:?}",
                    &svg[i.saturating_sub(30)..(i + 30).min(n)]
                );
                while i < n && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                assert!(
                    i < n && bytes[i] == b'=',
                    "{label}: XML attr `{attr_name}` missing `=` at {i}: {:?}",
                    &svg[attr_start.saturating_sub(30)..(i + 30).min(n)]
                );
                i += 1;
                while i < n && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                assert!(
                    i < n && (bytes[i] == b'"' || bytes[i] == b'\''),
                    "{label}: XML attr `{attr_name}` missing opening quote at {i}"
                );
                let quote = bytes[i];
                i += 1;
                while i < n && bytes[i] != quote {
                    assert!(
                        bytes[i] != b'<',
                        "{label}: unescaped `<` inside attr `{attr_name}` near {i}: {:?}",
                        &svg[attr_start.saturating_sub(20)..(i + 30).min(n)]
                    );
                    i += 1;
                }
                assert!(i < n, "{label}: unclosed quote for attr `{attr_name}`");
                i += 1; // consume closing quote
                if i < n {
                    let next = bytes[i];
                    assert!(
                        next.is_ascii_whitespace() || next == b'>' || (next == b'/' && i + 1 < n && bytes[i + 1] == b'>'),
                        "{label}: missing space after attr `{attr_name}` at byte {i}: {:?}",
                        &svg[attr_start.saturating_sub(20)..(i + 40).min(n)]
                    );
                }
            }
            assert!(i < n && bytes[i] == b'>', "{label}: unclosed `<{tag_name}>`");
            i += 1;
            if closing {
                let open = stack
                    .pop()
                    .unwrap_or_else(|| panic!("{label}: unexpected closing </{tag_name}>"));
                assert_eq!(
                    open, tag_name,
                    "{label}: mismatched closing tag </{tag_name}> for <{open}>"
                );
            } else if !self_closing {
                stack.push(tag_name.to_string());
            }
        } else {
            i += 1;
        }
    }
    assert!(stack.is_empty(), "{label}: unclosed XML tags at EOF: {stack:?}");
}

fn assert_valid_svg(svg: &str, raw: bool, label: &str) {
    assert!(svg.starts_with("<svg "), "{label}: SVG must start with <svg");
    assert!(svg.trim_end().ends_with("</svg>"), "{label}: SVG must end with </svg>");
    assert!(!svg.contains("NaN"), "{label}: SVG contains NaN:\n{svg}");
    assert!(!svg.contains("Infinity"), "{label}: SVG contains Infinity:\n{svg}");
    assert!(!svg.contains("undefined"), "{label}: SVG contains literal 'undefined':\n{svg}");
    validate_xml_well_formed(svg, label);
    if !raw {
        assert!(
            !svg.contains("var(--"),
            "{label}: resolved SVG should not contain unresolved CSS var(--):\n{svg}"
        );
        assert!(
            !svg.contains("color-mix("),
            "{label}: resolved SVG should not contain unresolved color-mix():\n{svg}"
        );
    }
}

#[test]
fn all_33_diagram_types_detect_and_render_smoke() {
    let samples: [(&str, &str); 33] = [
        ("agentflow", "agentflow-beta TB\n  a[\"Input\"]@{ shape: input } --> b[\"Task\"]@{ shape: task }"),
        ("architecture", "architecture-beta\n  service api(server)[API]\n  service db(database)[DB]\n  api:R --> L:db"),
        ("block", "block-beta\n  columns 2\n  a[\"One\"] b[\"Two\"]\n  a --> b"),
        ("c4", "C4Context\n  Person(user, \"User\", \"A user\")\n  System(sys, \"System\", \"Core\")\n  Rel(user, sys, \"Uses\")"),
        ("class", "classDiagram\n  Animal <|-- Duck\n  class Animal {\n    +int age\n    +isMammal() bool\n  }"),
        ("cynefin", "cynefin-beta\n  title Sense-making\n  clear\n    \"Best practice\"\n  complex\n    \"Probe first\""),
        ("er", "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER ||--|{ LINE-ITEM : contains"),
        ("eventmodeling", "eventmodeling\n  tf 01 ui LoginScreen\n  tf 02 cmd Login\n  tf 03 evt LoggedIn"),
        ("flowchart", "flowchart LR\n  A[Start] --> B{Check} --> C([Done])"),
        ("gantt", "gantt\n  title Release Plan\n  dateFormat YYYY-MM-DD\n  section Core\n  Build :a1, 2026-01-01, 5d\n  Test  :after a1, 3d"),
        ("gitGraph", "gitGraph\n  commit\n  branch feature\n  checkout feature\n  commit\n  checkout main\n  merge feature"),
        ("info", "info"),
        ("ishikawa", "ishikawa-beta\n  Late delivery\n    People\n      Training\n    Process\n      Handoffs"),
        ("journey", "journey\n  title Working day\n  section Morning\n    Make tea: 5: Me\n    Read mail: 3: Me"),
        ("kanban", "kanban\n  Todo\n    [Write tests]\n  Done\n    [Ship plugin]"),
        ("mindmap", "mindmap\n  root((pelago))\n    Diagrams\n      Flowchart\n      Sequence\n    WASM"),
        ("packet", "packet-beta\n  0-15: \"Source Port\"\n  16-31: \"Destination Port\""),
        ("pie", "pie title Pets\n  \"Dogs\" : 42\n  \"Cats\" : 28"),
        ("quadrantChart", "quadrantChart\n  title Reach vs Engagement\n  x-axis Low --> High\n  y-axis Low --> High\n  Campaign A: [0.3, 0.6]"),
        ("radar", "radar-beta\n  axis Speed, Reliability, Simplicity\n  curve pelago{90, 95, 85}"),
        ("railroad", "railroad-ebnf-beta\n  expr = term , { \"+\" , term } ;"),
        ("requirement", "requirementDiagram\n  requirement test_req {\n    id: 1\n    text: Must render offline\n    risk: low\n    verifymethod: test\n  }"),
        ("sankey", "sankey-beta\nSolar,Grid,120\nWind,Grid,80\nGrid,Homes,180\nGrid,Loss,20"),
        ("sequence", "sequenceDiagram\n  Alice->>Bob: Hello Bob!\n  Bob-->>Alice: Hi Alice!"),
        ("state", "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Active : start\n  Active --> [*] : stop"),
        ("swimlane", "swimlane-beta LR\n  subgraph App\n    req[Request]\n  end\n  subgraph API\n    res[Response]\n  end\n  req --> res"),
        ("timeline", "timeline\n  title History\n  2024 : Prototype\n  2026 : Full 33-diagram release"),
        ("treemap", "treemap-beta\n\"Root\"\n  \"Frontend\": 40\n  \"Backend\": 60"),
        ("treeView", "treeView-beta\n  \"src\"\n    \"lib.rs\"\n    \"diagrams\""),
        ("usecase", "usecase-beta\n  actor User\n  UC1(\"Render Diagram\")\n  User --> UC1"),
        ("venn", "venn-beta\n  set A\n  set B\n  union A, B"),
        ("wardley", "wardley-beta\n  title Tea Shop\n  anchor Public [0.95, 0.65]\n  component Cup of Tea [0.79, 0.61]\n  Public -> Cup of Tea"),
        ("xychart", "xychart-beta\n  title \"Revenue\"\n  x-axis [Q1, Q2, Q3, Q4]\n  y-axis \"USD\" 0 --> 100\n  bar [30, 55, 70, 90]\n  line [25, 50, 68, 88]"),
    ];

    assert_eq!(samples.len(), pelago::SUPPORTED.len());
    for (expected, src) in samples {
        assert_eq!(detect_type(src), Some(expected), "detect_type mismatch for {expected}");
        assert!(supports(src), "supports({expected}) must be true");

        let raw = render(src, &RenderOptions { raw: true, ..Default::default() })
            .unwrap_or_else(|e| panic!("{expected} failed in raw mode: {e}"));
        assert_eq!(raw.diagram, expected);
        assert!(raw.width.is_finite() && raw.width > 0.0, "{expected}: bad width {}", raw.width);
        assert!(raw.height.is_finite() && raw.height > 0.0, "{expected}: bad height {}", raw.height);
        assert_valid_svg(&raw.svg, true, &format!("{expected} (raw)"));

        let plain = render(src, &RenderOptions::default())
            .unwrap_or_else(|e| panic!("{expected} failed in plain mode: {e}"));
        assert_eq!(plain.diagram, expected);
        assert_eq!(plain.width, raw.width);
        assert_eq!(plain.height, raw.height);
        assert_valid_svg(&plain.svg, false, &format!("{expected} (plain)"));
    }
}

#[test]
fn pele_docs_corpus_renders_across_all_33_diagrams() {
    let corpus_dir = Path::new("/home/muka/pele/tests/corpus");
    if !corpus_dir.exists() {
        eprintln!("skipping external Pele corpus test: /home/muka/pele/tests/corpus not found");
        return;
    }

    let mut files: Vec<_> = fs::read_dir(corpus_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("-docs.json"))
        })
        .collect();
    files.sort();
    assert_eq!(files.len(), 33, "expected 33 *-docs.json files in Pele corpus");

    let mut total_rendered = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in files {
        let fname = path.file_name().unwrap().to_str().unwrap();
        let stem = fname.strip_suffix("-docs.json").unwrap();
        let expected_diagram = expected_diagram_for_corpus(stem);
        let text = fs::read_to_string(&path).unwrap();
        let val = parse_json(&text).unwrap_or_else(|e| panic!("failed to parse {fname}: {e}"));
        let Value::Arr(items) = val else {
            panic!("{fname} root is not an array");
        };
        assert!(!items.is_empty(), "{fname} should not be empty");

        for (i, item) in items.iter().enumerate() {
            let Value::Str(src) = item else { continue };
            let label = format!("{fname}#{i}");

            for max_width in [None, Some(320.0)] {
                let raw_opts = RenderOptions {
                    raw: true,
                    max_width,
                    ..Default::default()
                };
                match render(src, &raw_opts) {
                    Ok(rendered) => {
                        assert_eq!(rendered.diagram, expected_diagram, "{label} diagram mismatch");
                        assert!(
                            rendered.width.is_finite() && rendered.width > 0.0,
                            "{label} invalid width {}",
                            rendered.width
                        );
                        assert!(
                            rendered.height.is_finite() && rendered.height > 0.0,
                            "{label} invalid height {}",
                            rendered.height
                        );
                        assert_valid_svg(&rendered.svg, true, &format!("{label} raw mw={max_width:?}"));

                        let plain_opts = RenderOptions {
                            raw: false,
                            max_width,
                            ..Default::default()
                        };
                        let plain = render(src, &plain_opts)
                            .unwrap_or_else(|e| panic!("{label} plain render failed after raw succeeded: {e}"));
                        assert_valid_svg(&plain.svg, false, &format!("{label} plain mw={max_width:?}"));
                        total_rendered += 1;
                    }
                    Err(err) => {
                        failures.push(format!("{label} (mw={max_width:?}): {err}"));
                    }
                }
            }
        }
    }

    if !failures.is_empty() {
        eprintln!("Corpus render errors ({} total):", failures.len());
        for f in &failures {
            eprintln!("  - {f}");
        }
    }
    assert!(
        total_rendered > 800,
        "expected >800 successful corpus renders across 33 diagrams, got {total_rendered} (errors: {})",
        failures.len()
    );
}

#[test]
fn pele_docs_corpus_svgs_compile_cleanly_in_typst_usvg() {
    use std::process::Command;

    let corpus_dir = Path::new("/home/muka/pele/tests/corpus");
    if !corpus_dir.exists() {
        return;
    }
    let typst = if Command::new("typst").arg("--version").output().is_ok() {
        "typst".to_string()
    } else if let Ok(home) = std::env::var("HOME") {
        let p = Path::new(&home).join(".cargo/bin/typst");
        if Command::new(&p).arg("--version").output().is_ok() {
            p.to_string_lossy().into_owned()
        } else {
            return;
        }
    } else {
        return;
    };

    let tmp = std::env::temp_dir().join(format!("pelago_corpus_usvg_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    let mut files: Vec<_> = fs::read_dir(corpus_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("-docs.json"))
        })
        .collect();
    files.sort();

    let mut typ_doc = String::from("#set page(width: auto, height: auto, margin: 8pt)\n");
    let mut count = 0usize;
    for path in files {
        let fname = path.file_name().unwrap().to_str().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let Ok(Value::Arr(items)) = parse_json(&text) else { continue };
        for (i, item) in items.iter().enumerate() {
            let Value::Str(src) = item else { continue };
            let Ok(rendered) = render(src, &RenderOptions::default()) else { continue };
            // Typst's hermetic compiler rejects external HTTP `<image href="https://...">` links.
            if rendered.svg.contains("<image href=\"http") {
                continue;
            }
            let svg_name = format!("d_{count}_{i}.svg");
            fs::write(tmp.join(&svg_name), &rendered.svg).unwrap();
            typ_doc.push_str(&format!("// {fname}#{i}\n#image(\"{svg_name}\")\n"));
            count += 1;
        }
    }
    assert!(count > 400, "expected >400 corpus SVGs, got {count}");

    let typ_path = tmp.join("all_corpus.typ");
    let pdf_path = tmp.join("all_corpus.pdf");
    fs::write(&typ_path, &typ_doc).unwrap();

    let out = Command::new(&typst)
        .arg("compile")
        .arg(&typ_path)
        .arg(&pdf_path)
        .output()
        .expect("run typst compile on corpus SVGs");
    assert!(
        out.status.success(),
        "typst usvg failed on corpus SVGs:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(fs::read(&pdf_path).unwrap().starts_with(b"%PDF-"));
    let _ = fs::remove_dir_all(&tmp);
}

