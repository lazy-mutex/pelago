// Mermaid's detectors, in the order Mermaid registers them. The first match wins.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    // Pele's name for the type, such as `flowchart`.
    pub diagram: &'static str,
    // The type as Mermaid reports it, such as `flowchart-v2` for `graph TD`.
    pub mermaid: &'static str,
}

#[derive(Clone, Copy)]
enum Tail {
    Any,
    // `\b`
    Word,
    // `(?:\s|$)`
    Space,
    // `(?:[\s:]|$)`
    SpaceOrColon,
}

struct Detector {
    diagram: &'static str,
    mermaid: &'static str,
    keywords: &'static [&'static str],
    ignore_case: bool,
    tail: Tail,
}

const fn d(diagram: &'static str, mermaid: &'static str, keywords: &'static [&'static str]) -> Detector {
    Detector { diagram, mermaid, keywords, ignore_case: false, tail: Tail::Any }
}

const fn di(diagram: &'static str, mermaid: &'static str, keywords: &'static [&'static str], tail: Tail) -> Detector {
    Detector { diagram, mermaid, keywords, ignore_case: true, tail }
}

const fn dt(diagram: &'static str, mermaid: &'static str, keywords: &'static [&'static str], tail: Tail) -> Detector {
    Detector { diagram, mermaid, keywords, ignore_case: false, tail }
}

const DETECTORS: &[Detector] = &[
    d("flowchart", "flowchart-elk", &["flowchart-elk"]),
    d("mindmap", "mindmap", &["mindmap"]),
    d("architecture", "architecture", &["architecture"]),
    dt("agentflow", "agentflow", &["agentflow-beta"], Tail::Word),
    d("c4", "c4", &["C4Context", "C4Container", "C4Component", "C4Dynamic", "C4Deployment"]),
    d("kanban", "kanban", &["kanban"]),
    d("class", "classDiagram", &["classDiagram"]),
    d("er", "er", &["erDiagram"]),
    d("gantt", "gantt", &["gantt"]),
    d("info", "info", &["info"]),
    d("pie", "pie", &["pie"]),
    d("requirement", "requirement", &["requirement"]),
    d("sequence", "sequence", &["sequenceDiagram"]),
    dt("swimlane", "swimlane", &["swimlane-beta"], Tail::Word),
    d("flowchart", "flowchart-v2", &["graph", "flowchart"]),
    d("timeline", "timeline", &["timeline"]),
    d("gitGraph", "gitGraph", &["gitGraph"]),
    d("state", "stateDiagram", &["stateDiagram"]),
    d("journey", "journey", &["journey"]),
    d("quadrantChart", "quadrantChart", &["quadrantChart"]),
    d("sankey", "sankey", &["sankey"]),
    d("packet", "packet", &["packet"]),
    d("xychart", "xychart", &["xychart"]),
    d("block", "block", &["block"]),
    d("eventmodeling", "eventmodeling", &["eventmodeling"]),
    d("treeView", "treeView", &["treeView-beta"]),
    d("radar", "radar", &["radar-beta"]),
    // `ishikawa(-beta)?\b`: a word boundary after either form.
    di("ishikawa", "ishikawa", &["ishikawa-beta", "ishikawa"], Tail::Word),
    d("treemap", "treemap", &["treemap"]),
    di("railroad", "railroad", &["railroad-beta"], Tail::Any),
    di("railroad", "railroadEbnf", &["railroad-ebnf-beta"], Tail::Any),
    di("railroad", "railroadAbnf", &["railroad-abnf-beta"], Tail::Any),
    di("railroad", "railroadPeg", &["railroad-peg-beta"], Tail::Any),
    d("venn", "venn", &["venn-beta"]),
    di("wardley", "wardley", &["wardley-beta"], Tail::Any),
    dt("cynefin", "cynefin", &["cynefin-beta"], Tail::SpaceOrColon),
    dt("usecase", "usecase", &["usecase-beta"], Tail::Space),
];

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn starts_with(text: &str, keyword: &str, ignore_case: bool) -> bool {
    match text.get(..keyword.len()) {
        Some(head) if ignore_case => head.eq_ignore_ascii_case(keyword),
        Some(head) => head == keyword,
        None => false,
    }
}

pub fn detect_diagram(cleaned: &str) -> Option<Detected> {
    let text = cleaned.trim_start();
    for det in DETECTORS {
        for keyword in det.keywords {
            if !starts_with(text, keyword, det.ignore_case) {
                continue;
            }
            let next = text[keyword.len()..].chars().next();
            let ok = match det.tail {
                Tail::Any => true,
                Tail::Word => next.map_or(true, |c| !is_word(c)),
                Tail::Space => next.map_or(true, |c| c.is_whitespace()),
                Tail::SpaceOrColon => next.map_or(true, |c| c.is_whitespace() || c == ':'),
            };
            if ok {
                return Some(Detected { diagram: det.diagram, mermaid: det.mermaid });
            }
        }
    }
    None
}
