use pelago::{detect_type, render, supports, RenderOptions};

fn raw_opts() -> RenderOptions<'static> {
    RenderOptions { raw: true, ..Default::default() }
}

#[test]
fn swimlane_detection_lanes_directions_and_handoffs() {
    let order = "swimlane-beta LR
  subgraph Customer
    browse[Browse] --> pay[Pay]
  end
  subgraph Warehouse
    pick[Pick] --> ship[Ship]
  end
  pay -- paid --> pick";

    assert_eq!(detect_type(order), Some("swimlane"));
    assert!(supports(order));
    assert_ne!(detect_type("swimlane-betax\n  a --> b"), Some("swimlane"));

    let raw = render(order, &raw_opts()).expect("render swimlane raw");
    assert_eq!(raw.diagram, "swimlane");
    assert!(raw.svg.contains("class=\"pele pele-swimlane\""));
    assert!(raw.svg.contains("aria-roledescription=\"swimlane\""));
    assert!(raw.svg.contains("class=\"pele-cluster pele-lane\" data-id=\"Customer\""));
    assert!(raw.svg.contains("class=\"pele-cluster pele-lane\" data-id=\"Warehouse\""));
    assert!(raw.svg.contains("transform=\"rotate(-90 "));
    assert!(raw.svg.contains(">paid<"));

    // Plain resolved mode strips CSS vars for Typst / usvg.
    let plain = render(order, &RenderOptions::default()).expect("render swimlane plain");
    assert!(!plain.svg.contains("var(--"));
    assert!(!plain.svg.contains("color-mix("));

    // Vertical direction omits sideways label rotation.
    let td = render(&order.replace("LR", "TD"), &raw_opts()).expect("render swimlane TD");
    assert!(!td.svg.contains("rotate("));

    // Loose nodes outside lanes share a single unnamed lane.
    let loose = render("swimlane-beta TD\n  start --> a\n  subgraph Team\n    a --> b\n  end", &raw_opts()).unwrap();
    assert_eq!(loose.svg.matches("pele-lane").count(), 2);

    // Subgraph direction inside a lane is ignored.
    let plain_td = render("swimlane-beta TD\n  subgraph L\n    a --> b\n  end", &raw_opts()).unwrap();
    let dir_in_lane = render("swimlane-beta TD\n  subgraph L\n    direction LR\n    a --> b\n  end", &raw_opts()).unwrap();
    assert_eq!(plain_td.svg, dir_in_lane.svg);

    // Contrasting lane band ink and edge ink over filled lane bands.
    let styled = render(
        "swimlane-beta LR\n  subgraph L[Light]\n    a\n  end\n  subgraph D[Dark]\n    b\n  end\n  style L fill:#BBDEFB\n  style D fill:#1f3a5f",
        &raw_opts(),
    )
    .unwrap();
    assert!(styled.svg.contains("fill=\"var(--pele-fg-on-light,#1f1f1f)\""));
    assert!(styled.svg.contains("fill=\"var(--pele-fg-on-dark,#fff)\""));
}

#[test]
fn railroad_notations_and_layout_parity() {
    let sources = [
        "railroad-beta\nr = terminal(\"a\") ;",
        "railroad-ebnf-beta\nr = \"a\" ;",
        "railroad-abnf-beta\nr = \"a\" ;",
        "railroad-peg-beta\nr <- \"a\" ;",
    ];
    let mut svgs = Vec::new();
    for src in sources {
        assert_eq!(detect_type(src), Some("railroad"));
        assert!(supports(src));
        let res = render(src, &raw_opts()).expect(src);
        assert_eq!(res.diagram, "railroad");
        assert!(res.svg.contains("class=\"pele pele-railroad\""));
        assert_eq!(res.svg.matches("class=\"pele-node pele-terminal\"").count(), 1);
        svgs.push(res.svg);
    }
    // All four notations produce identical SVG for equivalent rules.
    for w in svgs.windows(2) {
        assert_eq!(w[0], w[1]);
    }

    // Case-insensitive detection, case-sensitive grammar keyword check.
    assert_eq!(detect_type("Railroad-EBNF-Beta\nr = \"a\" ;"), Some("railroad"));
    assert!(render("Railroad-EBNF-Beta\nr = \"a\" ;", &raw_opts()).is_err());

    // ABNF repeat counts, PEG predicates, and EBNF exceptions/special sequences.
    let abnf = |rule: &str| render(&format!("railroad-abnf-beta\n{}", rule), &raw_opts()).unwrap().svg;
    assert!(abnf("r = 3a ;").contains(">3×</text>"));
    assert!(abnf("r = 2*4a ;").contains(">2–4×</text>"));
    assert!(abnf("r = 2*a ;").contains(">≥2×</text>"));
    assert!(abnf("r = *5a ;").contains(">≤5×</text>"));
    assert!(abnf(&format!("r = {}a ;", "9".repeat(400))).contains(">∞×</text>"));
    assert_eq!(abnf("r = 1a ;"), abnf("r = a ;"));
    assert_eq!(abnf("r = 0*1a ;"), abnf("r = [ a ] ;"));

    let peg = render("railroad-peg-beta\nR <- !Keyword &\"k\" . !( a b ) ;", &raw_opts()).unwrap().svg;
    assert_eq!(peg.matches("pele-special").count(), 4);
    assert!(peg.contains(">!Keyword<"));
    assert!(peg.contains(">&amp;&quot;k&quot;<"));
    assert!(peg.contains(">!(...)<"));

    let ebnf_special = render("railroad-ebnf-beta\nr = ? any character ? - \"x\" ;", &RenderOptions::default()).unwrap();
    assert!(!ebnf_special.svg.contains("var(--"));
    assert!(ebnf_special.svg.contains("rx=\"4\""));
}
