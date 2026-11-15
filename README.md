# pelago

Render all **33** Mermaid & Pele diagram types to deterministic, self-contained SVG in [Typst](https://typst.app), from the command line, or as a zero-dependency Rust library. `pelago` is a Rust port of [Pele](https://pele.run) compiled to WebAssembly for Typst's plugin protocol.

- **Complete diagram coverage (33 types)** — `flowchart` / `graph`, `sequenceDiagram`, `classDiagram`, `stateDiagram` / `stateDiagram-v2`, `erDiagram`, `gantt`, `gitGraph`, `pie`, `mindmap`, `timeline`, `journey`, `quadrantChart`, `xychart-beta`, `sankey-beta`, `requirementDiagram`, `C4Context` (`c4`), `block-beta`, `packet-beta`, `kanban`, `architecture-beta`, `radar-beta`, `treemap-beta`, `venn-beta`, `ishikawa-beta`, `wardley-beta`, `treeView-beta`, `cynefin-beta`, `eventmodeling`, `usecase-beta`, `agentflow-beta`, `swimlane-beta`, `railroad-beta` (`ebnf` / `abnf` / `peg`), and `info`.
- **Zero runtime dependencies** — pure Rust parsers, Sugiyama / hierarchical / force / radial layout engines, text metrics, URL/CSS sanitizer, and SVG renderers.
- **Typst-native SVG output** — resolves Pele's CSS custom properties (`var(--...)`), `color-mix(in srgb, ...)`, generic font stacks, and `rx`/`ry` corner radii into plain SVG attributes compatible with `usvg`.
- **Pele parity (`--raw`)** — pass `raw: true` (or `--raw` on the CLI) to emit Pele's un-resolved CSS custom property stylesheet verbatim.

## Typst Usage

Import `mermaid` from `lib.typ` (or from your local Typst package namespace):

```typst
#import "lib.typ": mermaid

#mermaid("
flowchart TD
  A[Mermaid text] --> B{Flowchart?}
  B -- Yes --> C[pelago]
  B -- No --> D[Mermaid]
  C --> E([SVG])
  D --> E
")
```

### Fenced `mermaid` Code Blocks

Render every ```` ```mermaid ```` block automatically with a `show` rule:

```typst
#import "lib.typ": mermaid
#show raw.where(lang: "mermaid"): mermaid

```mermaid
flowchart LR
  Client([Browser]) --> API[Gateway] --> DB[(Postgres)]
```
```

### Options & Theming

`mermaid` accepts standard `image` sizing options (`width`, `height`, `scaling`, `fit`, `alt`) alongside rendering options:

```typst
#mermaid(
  "flowchart LR\n  A[Source] ==> B{Valid?}\n  B -- Yes --> C([Publish])",
  width: 85%,
  theme: "dark", // or a dictionary: (bg: rgb("#f8fafc"), fg: rgb("#0f172a"), accent: rgb("#2563eb"), surface: rgb("#dbeafe"), border: rgb("#3b82f6"))
  config: (flowchart: (curve: "stepAfter", nodeSpacing: 36)),
  padding: 20pt,
  font-size: 11pt,
)
```

Additional helpers exported by `lib.typ`:
- `mermaid-svg(source, ..options)` — returns the rendered `<svg>...</svg>` string (`raw: true` preserves CSS variables).
- `mermaid-bytes(source, ..options)` — returns the rendered SVG as `bytes`.
- `supports(source)` — returns `true` when `pelago` can render the diagram type (all 33 Pele diagram types).
- `detect-type(source)` — returns the detected Mermaid diagram type string (e.g. `"flowchart"`, `"sequence"`, `"swimlane"`) or `none`.

## CLI Usage

```sh
cargo run --release -- diagram.mmd > diagram.svg
cat diagram.mmd | cargo run --release > diagram.svg

# Dark palette
cargo run --release -- --dark diagram.mmd > dark.svg

# Preserve Pele's CSS custom properties instead of resolving them
cargo run --release -- --raw diagram.mmd > pele.svg
```

## Rust Library Usage

```rust
use pelago::{render, RenderOptions};

let result = render("flowchart LR\n  A[Hello] --> B[World]", &RenderOptions::default())?;
println!("{} ({}x{})", result.diagram, result.width, result.height);
println!("{}", result.svg);
```

## Building `pelago.wasm`

Requires the `wasm32-unknown-unknown` Rust target:

```sh
rustup target add wasm32-unknown-unknown
make wasm
# or directly:
cargo build --release --lib --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/pelago.wasm pelago.wasm
```

Run the full test suite (unit tests, 300+ Pele parity & security corpus tests, CLI tests, and end-to-end `typst compile` tests):

```sh
cargo test
```
