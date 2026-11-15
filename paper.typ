#import "lib.typ": mermaid, supports, detect-type

#set document(
  title: "pelago: Deterministic Mermaid & Pele Diagram Rendering in Typst",
  author: "pelago contributors",
  date: none,
)

#set page(
  paper: "a4",
  margin: (top: 1.65cm, bottom: 1.65cm, left: 1.55cm, right: 1.55cm),
  columns: 2,
  header: context {
    if counter(page).get().first() > 1 [
      #set text(size: 7.5pt, fill: rgb("#64748b"), font: ("Inter", "Liberation Sans"))
      *pelago v0.1.0* — Native Mermaid & Pele Diagrams in Typst
      #h(1fr)
      `github.com/lazy-mutex/pelago`
      #v(-4pt)
      #line(length: 100%, stroke: 0.4pt + rgb("#cbd5e1"))
    ]
  },
  footer: context [
    #set text(size: 7.5pt, fill: rgb("#64748b"), font: ("Inter", "Liberation Sans"))
    MIT License · Based on Pele (`pele.run`) by `@kepano`
    #h(1fr)
    Page #counter(page).display("1 / 1", both: true)
  ],
)

#set par(justify: true, leading: 0.55em, spacing: 0.8em)
#set text(font: ("Libertinus Serif", "Linux Libertine O"), size: 9.2pt, fill: rgb("#0f172a"))
#show raw: set text(font: ("JetBrains Mono", "DejaVu Sans Mono"), size: 7.5pt)

#set heading(numbering: "1.")
#show heading.where(level: 1): it => block(above: 1.0em, below: 0.48em)[
  #set text(font: ("Inter", "Liberation Sans"), size: 10.2pt, weight: "bold", fill: rgb("#0f172a"))
  #it
]
#show heading.where(level: 2): it => block(above: 0.85em, below: 0.4em)[
  #set text(font: ("Inter", "Liberation Sans"), size: 9pt, weight: "semibold", fill: rgb("#1e3a8a"))
  #it
]

// Helper for crisp framed figures (always numbered as Figure, even for raw blocks)
#let diagram-card(body, caption: none, bg: rgb("#ffffff"), border: rgb("#e2e8f0"), pad-amt: 5.5pt) = figure(
  kind: image,
  supplement: [Figure],
  block(
    width: 100%,
    fill: bg,
    stroke: 0.6pt + border,
    radius: 4pt,
    inset: pad-amt,
    clip: true,
    align(center, body),
  ),
  caption: if caption != none { text(size: 7.8pt, fill: rgb("#475569"), caption) } else { none },
  gap: 0.42em,
)

#let navy-theme = (
  bg: rgb("#f8fafc"),
  fg: rgb("#0f172a"),
  accent: rgb("#2563eb"),
  surface: rgb("#eff6ff"),
  border: rgb("#93c5fd"),
  muted: rgb("#475569"),
)

// ─── Title, Abstract & Full-Width Architecture Figure ───────────────────────
#place(
  top + center,
  scope: "parent",
  float: true,
  clearance: 1.05em,
)[
  #align(center)[
    #block(
      fill: rgb("#eff6ff"),
      stroke: 0.5pt + rgb("#bfdbfe"),
      radius: 99pt,
      inset: (x: 9pt, y: 3pt),
    )[
      #text(font: ("Inter", "Liberation Sans"), size: 7.2pt, weight: "semibold", fill: rgb("#1d4ed8"))[
        TYPST WEBASSEMBLY PLUGIN  ·  ZERO-DEPENDENCY RUST  ·  33 DIAGRAM TYPES
      ]
    ]
    #v(4pt)
    #text(font: ("Inter", "Liberation Sans"), size: 15.2pt, weight: "bold", fill: rgb("#0f172a"))[
      pelago: Deterministic Mermaid & Pele Diagram Rendering in Typst
    ]
    #v(3pt)
    #text(font: ("Inter", "Liberation Sans"), size: 9pt, fill: rgb("#334155"))[
      *pelago contributors* · Pure Rust Port of *Pele* (`obsidianmd/pele` by Steph Ango / `@kepano`) · `github.com/lazy-mutex/pelago`
    ]
  ]

  #v(6pt)
  #block(
    width: 100%,
    fill: rgb("#f8fafc"),
    stroke: (left: 2.2pt + rgb("#2563eb"), rest: 0.5pt + rgb("#e2e8f0")),
    radius: (right: 3pt),
    inset: (x: 10pt, y: 6.5pt),
  )[
    #set par(leading: 0.5em)
    #text(font: ("Inter", "Liberation Sans"), size: 8pt, weight: "bold", fill: rgb("#1e3a8a"))[Abstract — ]
    #text(size: 8.4pt, fill: rgb("#1e293b"))[
      Technical authors using Typst frequently need declarative text-to-diagram workflows without sacrificing sandboxing, offline reproducibility, or vector typography. Existing Mermaid workflows either shell out to headless browsers (`mermaid-cli`) or embed heavyweight JavaScript runtimes inside WebAssembly. We present *pelago*, a zero-dependency Rust port of Kepano's *Pele* diagram engine compiled directly to `wasm32-unknown-unknown` for Typst's native plugin protocol. `pelago` parses, lays out, and renders all *33* Mermaid and Pele diagram types into self-contained SVGs while resolving CSS custom properties (`var(--...)`) and `color-mix(in srgb, ...)` expressions into concrete attributes compatible with Typst's `usvg` backend.
    ]
  ]

  #v(6pt)
  #diagram-card(
    bg: rgb("#f8fafc"),
    border: rgb("#cbd5e1"),
    pad-amt: 5pt,
    mermaid(
      "flowchart LR
        subgraph Input [Typst Document]
          A[\"raw mermaid block\"] --> B[\"lib.typ wrapper\"]
        end
        subgraph Engine [pelago.wasm — Pure Rust]
          B --> C[\"33 Lexers & Parsers\"]
          C --> D[\"Sugiyama / Tree / Radial Layout\"]
          D --> E[\"CSS var() & color-mix Resolver\"]
        end
        subgraph Output [Typst Compiler]
          E --> F([\"usvg Vector PDF / PNG\"])
        end",
      width: 97%,
      theme: navy-theme,
      padding: 8pt,
      font-size: 9.5pt,
    ),
    caption: [End-to-end `pelago` compilation pipeline inside Typst's deterministic WebAssembly sandbox (rendered live by `pelago`).],
  ) <fig-pipeline>
]

= Introduction

Declarative diagramming languages such as Mermaid allow authors to version-control system architectures, protocols, formal grammars, and quantitative charts alongside prose. Yet integrating Mermaid into Typst has posed an architectural mismatch: standard `mermaid.js` depends on browser DOM text measurement and dynamic CSS engines, whereas Typst guarantees fast, hermetic, reproducible compilation without external processes.

Recently, Steph Ango (`@kepano`) and the Obsidian team introduced *Pele* (`pele.run`), demonstrating that Mermaid diagrams can be rendered synchronously with clean aesthetics and CSS-variable theming. Inspired by the `r/typst` community discussion exploring how Pele might run inside Typst, *pelago* ports the complete Pele rendering suite directly to idiomatic, dependency-free Rust. Because `pelago` compiles straight to a standalone `wasm32-unknown-unknown` binary with no JavaScript interpreter overhead (@fig-pipeline), diagrams render in milliseconds inside `typst compile` and `typst watch`.

= Architecture & `usvg` Resolution

Pele's web output relies extensively on modern browser CSS features: root-level custom properties (`--_bg`, `--_fg`, `--_accent`), nested `color-mix(in srgb, ...)` tint calculations, CSS `rx`/`ry` geometry properties, and generic font stacks. Typst renders embedded SVGs via `usvg`, which expects static SVG presentation attributes rather than dynamic browser stylesheets.

`pelago` bridges this gap with a dedicated post-layout resolution pass (`src/svg/resolve.rs`):
- *Custom property evaluation:* Resolves chained `var(--name, fallback)` lookups against Light, Dark, or user-supplied Typst `rgb(...)` palettes.
- *Perceptual sRGB mixing:* Evaluates nested `color-mix(in srgb, ...)` blends into exact hex colors and `opacity` values at compile time.
- *Geometry promotion & sanitization:* Promotes CSS corner radii to native SVG `rx`/`ry` attributes and strips unsafe URLs or script payloads.

#diagram-card(
  mermaid(
    "sequenceDiagram
      participant T as Typst Document
      participant L as lib.typ
      participant W as pelago.wasm
      T->>L: mermaid raw block
      L->>W: render_with_options(src, json)
      W->>W: Detect -> Parse -> Layout
      W->>W: Resolve var() & color-mix()
      W-->>L: Self-contained SVG bytes
      L-->>T: Native vector image()",
    width: 100%,
    padding: 6pt,
    font-size: 8.5pt,
  ),
  caption: [`sequenceDiagram`: Host–plugin invocation across Typst's WASM boundary.],
) <fig-seq>

= Drop-In Typst Integration

Activating `pelago` across an entire manuscript requires only two lines (@fig-seq). Registering a `show` rule on `raw.where(lang: "mermaid")` transforms every fenced code block into a crisp vector figure:

#block(
  width: 100%,
  fill: rgb("#f8fafc"),
  stroke: 0.5pt + rgb("#e2e8f0"),
  radius: 3pt,
  inset: 6pt,
)[
  ```typst
  #import "@local/pelago:0.1.0": mermaid
  #show raw.where(lang: "mermaid"): it => mermaid(it, width: 100%)
  ```
]

// Activate the show rule live for the fenced block below!
#show raw.where(lang: "mermaid"): it => mermaid(it, width: 98%, padding: 6pt, font-size: 9pt)

The state machine in @fig-state is compiled directly from a standard `mermaid` fenced block via the `show` rule above:

#diagram-card(
  ```mermaid
  stateDiagram-v2
    direction LR
    [*] --> Parse
    Parse --> Layout : AST
    Layout --> Render : SVG
    Render --> [*]
  ```,
  caption: [`stateDiagram-v2`: Rendered automatically via Typst `show raw` rule.],
) <fig-state>

= Multi-Paradigm Diagram Gallery

Unlike single-purpose flowchart packages, `pelago` ships specialized layout engines spanning software architecture, process modeling, formal grammars, and quantitative visualization (@fig-themes–@fig-git).

// ─── Page 2: Spanning Theme Comparison + Multi-Domain Gallery ───────────────
#place(
  top + center,
  scope: "parent",
  float: true,
  clearance: 0.95em,
)[
  #figure(
    kind: image,
    supplement: [Figure],
    grid(
      columns: (1fr, 1fr, 1fr),
      gutter: 7pt,
      block(
        width: 100%,
        fill: rgb("#ffffff"),
        stroke: 0.6pt + rgb("#cbd5e1"),
        radius: 4pt,
        inset: 5pt,
      )[
        #text(font: ("Inter", "Liberation Sans"), size: 7.2pt, weight: "semibold", fill: rgb("#475569"))[Default Light Palette]
        #v(2pt)
        #mermaid(
          "flowchart LR\n  S([Source]) --> P{Parse}\n  P -- OK --> R[Render]\n  P -- Err --> D[Report]",
          width: 100%,
          padding: 6pt,
          font-size: 8.5pt,
        )
      ],
      block(
        width: 100%,
        fill: rgb("#18181b"),
        stroke: 0.6pt + rgb("#3f3f46"),
        radius: 4pt,
        inset: 5pt,
      )[
        #text(font: ("Inter", "Liberation Sans"), size: 7.2pt, weight: "semibold", fill: rgb("#e4e4e7"))[`theme: "dark"`]
        #v(2pt)
        #mermaid(
          "flowchart LR\n  S([Source]) --> P{Parse}\n  P -- OK --> R[Render]\n  P -- Err --> D[Report]",
          width: 100%,
          theme: "dark",
          padding: 6pt,
          font-size: 8.5pt,
        )
      ],
      block(
        width: 100%,
        fill: rgb("#fdf4ff"),
        stroke: 0.6pt + rgb("#e879f9"),
        radius: 4pt,
        inset: 5pt,
      )[
        #text(font: ("Inter", "Liberation Sans"), size: 7.2pt, weight: "semibold", fill: rgb("#86198f"))[Custom `rgb(...)` Dictionary]
        #v(2pt)
        #mermaid(
          "flowchart LR\n  S([Source]) ==> P{Parse}\n  P -- OK --> R[Render]\n  P -- Err --> D[Report]",
          width: 100%,
          theme: (
            bg: rgb("#fdf4ff"),
            fg: rgb("#4a044e"),
            accent: rgb("#a21caf"),
            surface: rgb("#fae8ff"),
            border: rgb("#d946ef"),
          ),
          config: (flowchart: (curve: "stepAfter")),
          padding: 6pt,
          font-size: 8.5pt,
        )
      ],
    ),
    caption: text(size: 7.8pt, fill: rgb("#475569"))[
      Compile-time palette resolution: the same Mermaid source rendered in Default Light, built-in Dark, and a custom Typst color dictionary with `stepAfter` edge routing.
    ],
    gap: 0.42em,
  ) <fig-themes>
]

#diagram-card(
  mermaid(
    "swimlane-beta LR
      subgraph Author
        write[Write .typ]
        review[Inspect PDF]
      end
      subgraph Compiler
        wasm[pelago.wasm]
        usvg[usvg Engine]
      end
      write --> wasm --> usvg --> review",
    width: 100%,
    theme: navy-theme,
    padding: 6pt,
    font-size: 8.5pt,
  ),
  caption: [`swimlane-beta`: Cross-lane Sugiyama routing with partition bands.],
) <fig-swimlane>

#diagram-card(
  mermaid(
    "architecture-beta
      group edge(cloud)[Edge Tier]
      group core(server)[Data Tier]
      service cdn(internet)[CDN] in edge
      service api(server)[Gateway] in edge
      service db(database)[Primary DB] in core
      service cache(disk)[Cache] in core
      cdn:R --> L:api
      api:R --> L:db
      db:R --> L:cache",
    width: 98%,
    padding: 6pt,
    font-size: 8.5pt,
  ),
  caption: [`architecture-beta`: Cloud topology with built-in vector service icons.],
) <fig-arch>

#diagram-card(
  mermaid(
    "railroad-ebnf-beta
      expr = term , { ( \"+\" | \"-\" ) , term } ;
      term = factor , { ( \"*\" | \"/\" ) , factor } ;",
    width: 98%,
    padding: 5pt,
    font-size: 8.5pt,
  ),
  caption: [`railroad-ebnf-beta`: Syntax railroad tracks for EBNF, ABNF, and PEG grammars.],
) <fig-railroad>

#diagram-card(
  mermaid(
    "xychart-beta
      title \"Throughput Across Layout Engines\"
      x-axis [Flow, Seq, Arch, Gantt, XY]
      y-axis 0 --> 1200
      bar [980, 1120, 910, 1050, 1180]
      line [980, 1120, 910, 1050, 1180]",
    width: 94%,
    theme: navy-theme,
    padding: 5pt,
    font-size: 8pt,
  ),
  caption: [`xychart-beta`: Combined bar and line chart with automatic tick scaling.],
) <fig-xy>

#diagram-card(
  mermaid(
    "gitGraph
      commit id: \"init\"
      branch wasm-plugin
      checkout wasm-plugin
      commit id: \"parsers\"
      commit id: \"resolve-css\"
      checkout main
      merge wasm-plugin tag: \"v0.1.0\"",
    width: 98%,
    padding: 5pt,
    font-size: 8pt,
  ),
  caption: [`gitGraph`: Commit DAG with branch lanes and release tags.],
) <fig-git>

= Complete 33-Diagram Coverage

@tbl-coverage summarizes every diagram family compiled into `pelago.wasm` (validated against 300+ parity and security tests).

#figure(
  {
    set par(justify: false, leading: 0.46em)
    table(
      columns: (auto, 1fr),
      inset: (x: 5pt, y: 3.6pt),
      stroke: 0.45pt + rgb("#cbd5e1"),
      fill: (_, row) => if row == 0 { rgb("#eff6ff") } else if calc.odd(row) { rgb("#f8fafc") } else { white },
      align: (left + horizon, left + horizon),
      text(font: ("Inter", "Liberation Sans"), size: 7.2pt, weight: "bold", fill: rgb("#1e3a8a"))[Category],
      text(font: ("Inter", "Liberation Sans"), size: 7.2pt, weight: "bold", fill: rgb("#1e3a8a"))[Supported Syntax Identifiers (33 Total)],
      text(size: 7.3pt, weight: "semibold")[Graph & UML (9)],
      text(size: 7.1pt)[`flowchart`/`graph`, `sequenceDiagram`, `classDiagram`, `stateDiagram-v2`, `erDiagram`, `C4Context`, `usecase-beta`, `requirementDiagram`, `agentflow-beta`],
      text(size: 7.3pt, weight: "semibold")[Systems & Ops (6)],
      text(size: 7.1pt)[`architecture-beta`, `block-beta`, `packet-beta`, `swimlane-beta`, `eventmodeling`, `gitGraph`],
      text(size: 7.3pt, weight: "semibold")[Hierarchy (4)],
      text(size: 7.1pt)[`mindmap`, `treemap-beta`, `treeView-beta`, `ishikawa-beta`],
      text(size: 7.3pt, weight: "semibold")[Planning & Strat. (6)],
      text(size: 7.1pt)[`gantt`, `timeline`, `journey`, `kanban`, `wardley-beta`, `cynefin-beta`],
      text(size: 7.3pt, weight: "semibold")[Data & Grammar (8)],
      text(size: 7.1pt)[`xychart-beta`, `sankey-beta`, `pie`, `quadrantChart`, `radar-beta`, `venn-beta`, `railroad-beta` (`ebnf`/`abnf`/`peg`), `info`],
    )
  },
  caption: text(size: 7.8pt, fill: rgb("#475569"))[All 33 Pele & Mermaid diagram types compiled inside `pelago.wasm`.],
  gap: 0.42em,
) <tbl-coverage>

= Conclusion & Acknowledgements

`pelago` brings the full breadth of Mermaid and Pele diagramming to Typst as a single self-contained WebAssembly module—requiring no Node.js toolchain, no headless browser, and no JavaScript runtime. It is equally usable as a Typst `@local` plugin, a standalone CLI (`pelago --dark diagram.mmd`), or an embeddable Rust crate (`pelago::render`).

We gratefully credit Steph Ango (`@kepano`) and `obsidianmd/pele` for the original Pele design, visual language, and test corpus, as well as the `r/typst` community for sparking this port.

#v(2pt)
#block(
  width: 100%,
  fill: rgb("#f8fafc"),
  stroke: 0.5pt + rgb("#cbd5e1"),
  radius: 4pt,
  inset: (x: 8pt, y: 6pt),
)[
  #set text(font: ("Inter", "Liberation Sans"), size: 7.4pt, fill: rgb("#334155"))
  #grid(
    columns: (1fr, 1fr, 1fr),
    gutter: 6pt,
    align(center)[*33 / 33*\ Diagram Types],
    align(center)[*0*\ Runtime Deps],
    align(center)[*3.0 MB*\ `pelago.wasm`],
  )
]
