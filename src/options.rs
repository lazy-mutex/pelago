use crate::text::measurer::TextMeasurer;
use crate::value::Value;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Theme {
    pub bg: Option<String>,
    pub fg: Option<String>,
    pub muted: Option<String>,
    pub line: Option<String>,
    pub surface: Option<String>,
    pub surface_alt: Option<String>,
    pub border: Option<String>,
    pub accent: Option<String>,
    pub fg_on_light: Option<String>,
    pub fg_on_dark: Option<String>,
    pub critical: Option<String>,
    pub radius: Option<String>,
    pub title_weight: Option<String>,
    pub heading_weight: Option<String>,
    pub bold_weight: Option<String>,
    pub series: [Option<String>; 8],
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: Some("#18181b".into()),
            fg: Some("#f4f4f5".into()),
            muted: Some("#a1a1aa".into()),
            line: Some("#e4e4e7".into()),
            surface: Some("#27272a".into()),
            surface_alt: Some("#3f3f46".into()),
            border: Some("#71717a".into()),
            accent: Some("#60a5fa".into()),
            fg_on_light: Some("#1f1f1f".into()),
            fg_on_dark: Some("#ffffff".into()),
            critical: Some("#f87171".into()),
            ..Default::default()
        }
    }

    pub fn from_value(val: &Value) -> Self {
        if let Some(preset) = val.as_str() {
            return match preset {
                "dark" => Self::dark(),
                _ => Self::default(),
            };
        }
        if !val.is_object() {
            return Self::default();
        }
        let pick = |keys: &[&str]| -> Option<String> {
            for k in keys {
                if let Some(s) = val.get(k).as_str().filter(|s| !s.trim().is_empty()) {
                    return Some(s.trim().to_string());
                }
            }
            None
        };
        let mut series: [Option<String>; 8] = Default::default();
        if let Value::Arr(items) = val.get("series") {
            for (i, item) in items.iter().take(8).enumerate() {
                if let Some(s) = item.as_str().filter(|s| !s.trim().is_empty()) {
                    series[i] = Some(s.trim().to_string());
                }
            }
        }
        for i in 0..8 {
            let k1 = format!("series-{}", i + 1);
            let k2 = format!("series_{}", i + 1);
            if let Some(s) = pick(&[&k1, &k2]) {
                series[i] = Some(s);
            }
        }
        Self {
            bg: pick(&["bg", "--pele-bg"]),
            fg: pick(&["fg", "--pele-fg"]),
            muted: pick(&["muted", "--pele-muted"]),
            line: pick(&["line", "--pele-line"]),
            surface: pick(&["surface", "--pele-surface"]),
            surface_alt: pick(&["surface_alt", "surface-alt", "surfaceAlt", "--pele-surface-alt"]),
            border: pick(&["border", "--pele-border"]),
            accent: pick(&["accent", "--pele-accent"]),
            fg_on_light: pick(&["fg_on_light", "fg-on-light", "fgOnLight", "--pele-fg-on-light"]),
            fg_on_dark: pick(&["fg_on_dark", "fg-on-dark", "fgOnDark", "--pele-fg-on-dark"]),
            critical: pick(&["critical", "--pele-critical"]),
            radius: pick(&["radius", "--pele-radius"]),
            title_weight: pick(&["title_weight", "title-weight", "titleWeight", "--pele-title-weight"]),
            heading_weight: pick(&["heading_weight", "heading-weight", "headingWeight", "--pele-heading-weight"]),
            bold_weight: pick(&["bold_weight", "bold-weight", "boldWeight", "--pele-bold-weight"]),
            series,
        }
    }
}

#[derive(Clone, Default)]
pub struct RenderOptions<'a> {
    // Measures label text. Defaults to a table of Arial advances.
    pub measurer: Option<&'a dyn TextMeasurer>,
    // The fonts labels are drawn in, where the page sets no --pele-font or --pele-font-mono.
    pub font_family: Option<String>,
    pub font_family_mono: Option<String>,
    pub font_size: Option<f64>,
    pub id_prefix: Option<String>,
    pub responsive: Option<bool>,
    // The width the host has for the diagram, in pixels. A type that can draw itself narrower
    // does, so that its text is not shrunk with it.
    pub max_width: Option<f64>,
    // Whether a diagram that runs across may be drawn running down when it does not fit
    // `max_width`, and the width under which that is done. On by default, under 640 pixels.
    pub auto_direction: Option<bool>,
    pub direction_breakpoint: Option<f64>,
    pub padding: Option<f64>,
    pub limit: Option<usize>,
    pub output_limit: Option<usize>,
    // The most edges a flowchart may have. One short line can ask for millions of them.
    pub max_edges: Option<usize>,
    pub links: Option<bool>,
    pub images: Option<bool>,
    pub link_schemes: Option<Vec<String>>,
    pub image_schemes: Option<Vec<String>>,
    // Link `rel` attribute. Defaults to `noopener`; an empty string omits it.
    pub link_rel: Option<String>,
    // Mermaid configuration, under what the diagram's own front matter and directives set.
    pub config: Option<Value>,
    // Keeps Pele's CSS custom properties instead of resolving them to plain SVG colors/attributes.
    pub raw: bool,
    // Theme overrides applied when resolving CSS custom properties (`raw == false`).
    pub theme: Theme,
}

impl RenderOptions<'static> {
    pub fn from_json_value(val: &Value) -> Self {
        if !val.is_object() {
            return Self::default();
        }
        let str_opt = |keys: &[&str]| -> Option<String> {
            for k in keys {
                if let Some(s) = val.get(k).as_str() {
                    return Some(s.to_string());
                }
            }
            None
        };
        let num_opt = |keys: &[&str]| -> Option<f64> {
            for k in keys {
                if let Some(n) = val.get(k).as_f64() {
                    return Some(n);
                }
            }
            None
        };
        let bool_opt = |keys: &[&str]| -> Option<bool> {
            for k in keys {
                if let Some(b) = val.get(k).as_bool() {
                    return Some(b);
                }
            }
            None
        };
        let list_opt = |keys: &[&str]| -> Option<Vec<String>> {
            for k in keys {
                if let Value::Arr(arr) = val.get(k) {
                    return Some(arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());
                }
            }
            None
        };
        let config = val.get("config").clone();
        Self {
            measurer: None,
            font_family: str_opt(&["font_family", "font-family", "fontFamily", "font"]),
            font_family_mono: str_opt(&["font_family_mono", "font-family-mono", "fontFamilyMono", "font-mono", "font_mono"]),
            font_size: num_opt(&["font_size", "font-size", "fontSize"]),
            id_prefix: str_opt(&["id_prefix", "id-prefix", "idPrefix"]),
            responsive: bool_opt(&["responsive"]),
            max_width: num_opt(&["max_width", "max-width", "maxWidth"]),
            auto_direction: bool_opt(&["auto_direction", "auto-direction", "autoDirection"]),
            direction_breakpoint: num_opt(&["direction_breakpoint", "direction-breakpoint", "directionBreakpoint"]),
            padding: num_opt(&["padding"]),
            limit: num_opt(&["limit"]).filter(|&n| n >= 0.0).map(|n| n as usize),
            output_limit: num_opt(&["output_limit", "output-limit", "outputLimit"]).filter(|&n| n >= 0.0).map(|n| n as usize),
            max_edges: num_opt(&["max_edges", "max-edges", "maxEdges"]).filter(|&n| n >= 0.0).map(|n| n as usize),
            links: bool_opt(&["links"]),
            images: bool_opt(&["images"]),
            link_schemes: list_opt(&["link_schemes", "link-schemes", "linkSchemes"]),
            image_schemes: list_opt(&["image_schemes", "image-schemes", "imageSchemes"]),
            link_rel: str_opt(&["link_rel", "link-rel", "linkRel"]),
            config: config.is_object().then_some(config),
            raw: bool_opt(&["raw"]).unwrap_or(false),
            theme: Theme::from_value(val.get("theme")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkInfo {
    pub id: String,
    pub href: String,
    pub internal: bool,
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub svg: String,
    pub width: f64,
    pub height: f64,
    pub links: Vec<LinkInfo>,
}

fn available(options: &RenderOptions) -> Option<f64> {
    options.max_width.filter(|&w| w > 0.0)
}

// Draws a chart whose plot can be any width so that the whole diagram fits the width the host has.
pub fn fit_width<F>(options: &RenderOptions, natural: f64, least: f64, mut draw: F) -> Rendered
where
    F: FnMut(f64, bool) -> Rendered,
{
    let mut result = draw(natural, false);
    let Some(avail) = available(options) else {
        return result;
    };
    if result.width <= avail {
        return result;
    }
    let floor = least.min(natural);
    let mut plot = natural;
    result = draw(plot, true);
    let mut round = 0;
    while round < 3 && result.width > avail && plot > floor {
        plot = floor.max(plot - (result.width - avail));
        result = draw(plot, true);
        round += 1;
    }
    result
}

// For a chart that is drawn to a set width: its own, or the host's when that is smaller.
pub fn cap_width(options: &RenderOptions, natural: f64, least: f64, around: f64) -> f64 {
    match available(options) {
        Some(avail) => least.min(natural).max(natural.min(avail - around)),
        None => natural,
    }
}

// The width a title may take before it wraps.
pub fn title_room(options: &RenderOptions) -> f64 {
    available(options).map_or(4000.0, |w| (w - 2.0 * options.padding.unwrap_or(8.0)).max(80.0))
}

const BREAKPOINT: f64 = 640.0;

// For a diagram that can run across or down: draws it across, and down instead when the host
// allows it, the width it has is under the breakpoint, across does not fit, and down is narrower.
pub fn turn_to_fit(options: &RenderOptions, draw: &dyn Fn(bool) -> Rendered) -> Rendered {
    let across = draw(false);
    let Some(room) = available(options) else { return across };
    if options.auto_direction == Some(false) || across.width <= room {
        return across;
    }
    if room >= options.direction_breakpoint.filter(|&b| b > 0.0).unwrap_or(BREAKPOINT) {
        return across;
    }
    let down = draw(true);
    if down.width < across.width {
        down
    } else {
        across
    }
}

pub fn runs_across(direction: Option<&str>) -> bool {
    matches!(direction, Some("LR") | Some("RL"))
}

// Draws a diagram with room to spare, and tightly instead when that does not fit the width the
// host has and tight is narrower.
pub fn tighten(options: &RenderOptions, draw: &dyn Fn(bool) -> Rendered) -> Rendered {
    let loose = draw(false);
    let Some(room) = available(options) else { return loose };
    if loose.width <= room {
        return loose;
    }
    let tight = draw(true);
    if tight.width < loose.width {
        tight
    } else {
        loose
    }
}
