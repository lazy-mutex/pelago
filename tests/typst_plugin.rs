use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn find_typst() -> Option<PathBuf> {
    if let Ok(bin) = std::env::var("TYPST") {
        let p = PathBuf::from(bin);
        if Command::new(&p).arg("--version").output().is_ok() {
            return Some(p);
        }
    }
    if Command::new("typst").arg("--version").output().is_ok() {
        return Some(PathBuf::from("typst"));
    }
    if let Ok(home) = std::env::var("HOME") {
        let cargo_typst = Path::new(&home).join(".cargo/bin/typst");
        if Command::new(&cargo_typst).arg("--version").output().is_ok() {
            return Some(cargo_typst);
        }
    }
    None
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pelago_test_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn cli_renders_plain_raw_dark_and_reports_errors() {
    let bin = env!("CARGO_BIN_EXE_pelago");

    // 1. Default mode via stdin resolves CSS variables and promotes rx attributes.
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pelago");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"flowchart LR\n  A[Start] --> B(Rounded)\n")
        .unwrap();
    let out = child.wait_with_output().expect("wait pelago");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let svg = String::from_utf8(out.stdout).unwrap();
    assert!(svg.starts_with("<svg "));
    assert!(!svg.contains("var(--"), "default CLI output should resolve CSS vars");
    assert!(!svg.contains("color-mix("), "default CLI output should resolve color-mix");
    assert!(svg.contains("rx=\""), "rounded node should have promoted rx attribute");

    // 2. --raw preserves Pele's CSS custom properties.
    let dir = temp_dir("cli");
    let input_path = dir.join("diagram.mmd");
    fs::write(&input_path, "flowchart TD\n  A[One] --> B{Two}\n").unwrap();

    let raw_out = Command::new(bin)
        .arg("--raw")
        .arg(&input_path)
        .output()
        .expect("run pelago --raw");
    assert!(raw_out.status.success());
    let raw_svg = String::from_utf8(raw_out.stdout).unwrap();
    assert!(raw_svg.contains("--_bg:var(--pele-bg,#fff)"));
    assert!(raw_svg.contains("var(--_"));

    // 3. --dark resolves using the dark theme palette.
    let dark_out = Command::new(bin)
        .arg("--dark")
        .arg(&input_path)
        .output()
        .expect("run pelago --dark");
    assert!(dark_out.status.success());
    let dark_svg = String::from_utf8(dark_out.stdout).unwrap();
    assert!(dark_svg.contains("fill=\"#f4f4f5\""));
    assert!(dark_svg.contains("fill=\"#27272a\""));
    assert!(!dark_svg.contains("var(--"));

    // 4. Invalid Mermaid syntax exits with code 1.
    let bad_path = dir.join("bad.mmd");
    fs::write(&bad_path, "flowchart TD\n  A -->\n").unwrap();
    let bad_out = Command::new(bin).arg(&bad_path).output().unwrap();
    assert_eq!(bad_out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_out.stderr).contains("pelago:"));

    // 5. Missing file exits with code 2.
    let missing_out = Command::new(bin).arg(dir.join("nope.mmd")).output().unwrap();
    assert_eq!(missing_out.status.code(), Some(2));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn typst_plugin_compiles_all_package_entrypoints_and_33_diagrams() {
    let root = project_root();
    assert!(root.join("pelago.wasm").exists(), "pelago.wasm must exist at package root");
    assert!(root.join("lib.typ").exists(), "lib.typ must exist at package root");
    assert!(root.join("typst.toml").exists(), "typst.toml must exist at package root");

    let Some(typst) = find_typst() else {
        eprintln!("skipping Typst integration test: `typst` binary not found");
        return;
    };

    let dir = temp_dir("typst");

    // 1. Compile a comprehensive Typst document exercising string, raw block, show rule,
    //    themes, Typst color/length normalization, config, mermaid-svg, supports, detect-type,
    //    and all 33 diagram types through `pelago.wasm` + `usvg`.
    let suite_dir = dir.join("pkg");
    fs::create_dir_all(&suite_dir).unwrap();
    fs::copy(root.join("pelago.wasm"), suite_dir.join("pelago.wasm")).unwrap();
    fs::copy(root.join("lib.typ"), suite_dir.join("lib.typ")).unwrap();
    fs::copy(root.join("typst.toml"), suite_dir.join("typst.toml")).unwrap();

    let doc_path = suite_dir.join("suite.typ");
    fs::write(
        &doc_path,
        r##"#import "lib.typ": mermaid, mermaid-svg, mermaid-bytes, supports, detect-type

#assert(supports("flowchart LR\n  A --> B"))
#assert(supports("sequenceDiagram\n  A->>B: Hi"))
#assert(supports("swimlane-beta LR\n  subgraph A\n    x\n  end"))
#assert(not supports("not a diagram"))
#assert.eq(detect-type("graph TD\n  A --> B"), "flowchart")
#assert.eq(detect-type("sequenceDiagram\n  A->>B: Hi"), "sequence")
#assert.eq(detect-type("not a diagram"), none)

#let plain-svg = mermaid-svg("flowchart LR\n  A[Plain] --> B(Rounded)")
#assert(not plain-svg.contains("var(--"))
#assert(plain-svg.contains("rx=\""))

#let raw-svg = mermaid-svg("flowchart LR\n  A[Raw] --> B(Rounded)", raw: true)
#assert(raw-svg.contains("var(--_"))

#show raw.where(lang: "mermaid"): it => mermaid(it, width: 90%)

= Direct String
#mermaid("
flowchart LR
  subgraph S1 [Frontend]
    A[Client] --> B([Gateway])
  end
  B --> C[(Database)]
")

= Fenced Code Block
```mermaid
flowchart TD
  Start([Begin]) --> Check{Valid?}
  Check -- Yes --> Done["<b>Success</b><br/><code>200 OK</code>"]
  Check -- No --> Fail[Retry]
```

= Dark Theme & Custom Theme
#mermaid(
  "flowchart LR\n  X[Dark] --> Y((Node))",
  theme: "dark",
  padding: 16pt,
  font-size: 11pt,
)

#mermaid(
  "flowchart LR\n  P[Custom] ==> Q{Styled}",
  theme: (
    bg: rgb("#f8fafc"),
    fg: rgb("#0f172a"),
    accent: rgb("#2563eb"),
    surface: rgb("#dbeafe"),
    border: rgb("#3b82f6"),
  ),
  config: (flowchart: (curve: "stepAfter", nodeSpacing: 36)),
  auto-direction: false,
)

= Multi-Diagram Gallery (All 33 Diagram Types)
#mermaid("agentflow-beta TB\n  a[\"Input\"]@{ shape: input } --> b[\"Task\"]@{ shape: task }")
#mermaid("architecture-beta\n  service api(server)[API]\n  service db(database)[DB]\n  api:R --> L:db")
#mermaid("block-beta\n  columns 2\n  a[\"One\"] b[\"Two\"]\n  a --> b")
#mermaid("C4Context\n  Person(user, \"User\", \"A user\")\n  System(sys, \"System\", \"Core\")\n  Rel(user, sys, \"Uses\")")
#mermaid("classDiagram\n  Animal <|-- Duck\n  class Animal {\n    +int age\n  }")
#mermaid("cynefin-beta\n  title Sense-making\n  clear\n    \"Best practice\"\n  complex\n    \"Probe first\"")
#mermaid("erDiagram\n  USER ||--o{ ORDER : places")
#mermaid("eventmodeling\n  tf 01 ui LoginScreen\n  tf 02 cmd Login\n  tf 03 evt LoggedIn")
#mermaid("gantt\n  title Plan\n  dateFormat YYYY-MM-DD\n  Task :a1, 2026-01-01, 5d")
#mermaid("gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n  checkout main\n  merge dev")
#mermaid("info")
#mermaid("ishikawa-beta\n  Late delivery\n    People\n      Training\n    Process\n      Handoffs")
#mermaid("journey\n  title Working day\n  section Morning\n    Make tea: 5: Me")
#mermaid("kanban\n  Todo\n    [Write tests]\n  Done\n    [Ship plugin]")
#mermaid("mindmap\n  root((pelago))\n    WASM\n    SVG")
#mermaid("packet-beta\n  0-15: \"Source Port\"\n  16-31: \"Destination Port\"")
#mermaid("pie title Share\n  \"A\" : 60\n  \"B\" : 40")
#mermaid("quadrantChart\n  title Matrix\n  x-axis Low --> High\n  y-axis Low --> High\n  Item A: [0.3, 0.7]")
#mermaid("radar-beta\n  axis Speed, Quality, Safety\n  curve v1{90, 85, 95}")
#mermaid("railroad-ebnf-beta\n  expr = term , { \"+\" , term } ;")
#mermaid("requirementDiagram\n  requirement r1 {\n    id: 1\n    text: Offline SVG\n    risk: low\n    verifymethod: test\n  }")
#mermaid("sankey-beta\nA,B,50\nB,C,50")
#mermaid("sequenceDiagram\n  Alice->>Bob: Hello!\n  Bob-->>Alice: Ready")
#mermaid("stateDiagram-v2\n  [*] --> Idle\n  Idle --> Active : run\n  Active --> [*]")
#mermaid("swimlane-beta LR\n  subgraph App\n    req[Request]\n  end\n  subgraph API\n    res[Response]\n  end\n  req --> res")
#mermaid("timeline\n  title Milestones\n  2024 : Start\n  2026 : Complete")
#mermaid("treemap-beta\n\"Root\"\n  \"Core\": 60\n  \"Diagrams\": 40")
#mermaid("treeView-beta\n  \"src\"\n    \"lib.rs\"\n    \"diagrams\"")
#mermaid("usecase-beta\n  actor User\n  UC1(\"Render\")\n  User --> UC1")
#mermaid("venn-beta\n  set A\n  set B\n  union A, B")
#mermaid("wardley-beta\n  title Tea Shop\n  anchor Public [0.95, 0.65]\n  component Tea [0.79, 0.61]\n  Public -> Tea")
#mermaid("xychart-beta\n  x-axis [Q1, Q2, Q3]\n  y-axis 0 --> 100\n  bar [30, 60, 90]")
"##,
    )
    .unwrap();

    let suite_pdf = suite_dir.join("suite.pdf");
    let suite_out = Command::new(&typst)
        .arg("compile")
        .arg(&doc_path)
        .arg(&suite_pdf)
        .output()
        .expect("compile suite.typ");
    assert!(
        suite_out.status.success(),
        "suite.typ failed to compile:\n{}",
        String::from_utf8_lossy(&suite_out.stderr)
    );
    assert!(fs::read(&suite_pdf).unwrap().starts_with(b"%PDF-"));

    // 3. Invalid Mermaid syntax surfaces an error during `typst compile`.
    let bad_doc = suite_dir.join("bad.typ");
    fs::write(&bad_doc, "#import \"lib.typ\": mermaid\n#mermaid(\"flowchart TD\\n  A -->\")\n").unwrap();
    let bad_pdf = suite_dir.join("bad.pdf");
    let bad_out = Command::new(&typst).arg("compile").arg(&bad_doc).arg(&bad_pdf).output().unwrap();
    assert!(!bad_out.status.success(), "expected invalid Mermaid diagram to fail compilation");
    let stderr = String::from_utf8_lossy(&bad_out.stderr);
    assert!(
        stderr.contains("Parse error") || stderr.contains("Expecting") || stderr.contains("plugin errored"),
        "unexpected error message: {}",
        stderr
    );

    let _ = fs::remove_dir_all(&dir);
}
