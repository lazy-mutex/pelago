#let _plugin = plugin("pelago.wasm")

#let _extract-source(source) = {
  if type(source) == str {
    source
  } else if type(source) == content and source.func() == raw {
    source.text
  } else {
    panic("mermaid expected a string or raw block, got " + str(type(source)))
  }
}

#let _to-px(val) = {
  if type(val) == length {
    val.pt() * (96.0 / 72.0)
  } else {
    val
  }
}

#let _normalize-theme(theme) = {
  if theme == none or type(theme) == str {
    return theme
  }
  let out = (:)
  for (k, v) in theme.pairs() {
    if v == none {
      continue
    } else if type(v) == color {
      out.insert(k, v.to-hex())
    } else if type(v) == length {
      out.insert(k, str(v.pt() * (96.0 / 72.0)) + "px")
    } else if type(v) == array {
      out.insert(
        k,
        v.map(item => if type(item) == color { item.to-hex() } else { str(item) }),
      )
    } else {
      out.insert(k, str(v))
    }
  }
  out
}

#let _build-options(
  theme: none,
  config: none,
  font: none,
  font-mono: none,
  font-size: none,
  padding: none,
  max-width: none,
  auto-direction: none,
  raw: false,
) = {
  let opts = (:)
  if theme != none {
    opts.insert("theme", _normalize-theme(theme))
  }
  if config != none {
    opts.insert("config", config)
  }
  if font != none {
    opts.insert("font", if type(font) == array { font.join(", ") } else { str(font) })
  }
  if font-mono != none {
    opts.insert("font-mono", if type(font-mono) == array { font-mono.join(", ") } else { str(font-mono) })
  }
  if font-size != none {
    opts.insert("font-size", _to-px(font-size))
  }
  if padding != none {
    opts.insert("padding", _to-px(padding))
  }
  if max-width != none {
    opts.insert("max-width", _to-px(max-width))
  }
  if auto-direction != none {
    opts.insert("auto-direction", auto-direction)
  }
  if raw {
    opts.insert("raw", true)
  }
  opts
}

/// Renders Mermaid source to SVG bytes using `pelago`.
#let mermaid-bytes(
  source,
  theme: none,
  config: none,
  font: none,
  font-mono: none,
  font-size: none,
  padding: none,
  max-width: none,
  auto-direction: none,
  raw: false,
) = {
  let text = _extract-source(source)
  let opts = _build-options(
    theme: theme,
    config: config,
    font: font,
    font-mono: font-mono,
    font-size: font-size,
    padding: padding,
    max-width: max-width,
    auto-direction: auto-direction,
    raw: raw,
  )
  if opts.len() == 0 {
    _plugin.render(bytes(text))
  } else {
    _plugin.render_with_options(bytes(text), bytes(json.encode(opts, pretty: false)))
  }
}

/// Renders Mermaid source to an SVG string.
#let mermaid-svg(..args) = str(mermaid-bytes(..args))

/// Detects the Mermaid diagram type of `source`, or returns `none` if unrecognized.
#let detect-type(source) = {
  let kind = str(_plugin.detect_type(bytes(_extract-source(source))))
  if kind == "" { none } else { kind }
}

/// Reports whether `pelago` can render the diagram in `source`.
#let supports(source) = str(_plugin.supports(bytes(_extract-source(source)))) == "1"

/// Renders a Mermaid diagram (string or `raw` block) as an SVG image in Typst.
#let mermaid(
  source,
  width: auto,
  height: auto,
  scaling: auto,
  fit: "cover",
  alt: none,
  theme: none,
  config: none,
  font: none,
  font-mono: none,
  font-size: none,
  padding: none,
  max-width: none,
  auto-direction: none,
) = {
  let svg = mermaid-bytes(
    source,
    theme: theme,
    config: config,
    font: font,
    font-mono: font-mono,
    font-size: font-size,
    padding: padding,
    max-width: max-width,
    auto-direction: auto-direction,
  )
  image(
    svg,
    format: "svg",
    width: width,
    height: height,
    scaling: scaling,
    fit: fit,
    alt: alt,
  )
}
