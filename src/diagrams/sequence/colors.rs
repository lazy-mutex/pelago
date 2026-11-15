use crate::chars::is_space;

// Mermaid asks the browser whether the first word of a box title is a color. Pele has no browser,
// so it knows the color keywords and the rgb() and hsl() notations. Other functions, such as calc()
// and var(), are not recognized.
const KEYWORDS: &str = " aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood \
cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray \
darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen \
darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue \
firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew \
hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan \
lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray \
lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue \
mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred \
midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid \
palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple \
rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue \
slateblue slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white \
whitesmoke yellow yellowgreen transparent currentcolor inherit initial unset revert \
accentcolor accentcolortext activetext buttonborder buttonface buttontext canvas canvastext field fieldtext \
graytext highlight highlighttext linktext mark marktext selecteditem selecteditemtext visitedtext \
activeborder activecaption appworkspace background buttonhighlight buttonshadow captiontext inactiveborder \
inactivecaption inactivecaptiontext infobackground infotext menu menutext scrollbar threeddarkshadow threedface \
threedhighlight threedlightshadow threedshadow window windowframe windowtext ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum K {
    Bad,
    Number,
    Percent,
    Angle,
    None,
}

// /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?(%|deg|grad|rad|turn)?$/i
fn kind(term: &str) -> K {
    if term.eq_ignore_ascii_case("none") {
        return K::None;
    }
    let b = term.as_bytes();
    let n = b.len();
    let mut i = 0;
    if i < n && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if i < n && b[i].is_ascii_digit() {
        while i < n && b[i].is_ascii_digit() {
            i += 1;
        }
        if i < n && b[i] == b'.' {
            i += 1;
            if i >= n || !b[i].is_ascii_digit() {
                return K::Bad;
            }
            while i < n && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    } else if i < n && b[i] == b'.' {
        i += 1;
        if i >= n || !b[i].is_ascii_digit() {
            return K::Bad;
        }
        while i < n && b[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return K::Bad;
    }

    if i < n && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < n && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < n && b[j].is_ascii_digit() {
            while j < n && b[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }

    let unit = &term[i..];
    if unit.is_empty() {
        K::Number
    } else if unit == "%" {
        K::Percent
    } else if unit.eq_ignore_ascii_case("deg")
        || unit.eq_ignore_ascii_case("grad")
        || unit.eq_ignore_ascii_case("rad")
        || unit.eq_ignore_ascii_case("turn")
    {
        K::Angle
    } else {
        K::Bad
    }
}

fn is_alpha(term: &str, modern: bool) -> bool {
    let k = kind(term);
    k == K::Number || k == K::Percent || (modern && k == K::None)
}

fn is_function(color: &str) -> bool {
    let hsl = color.starts_with("hsl");
    let Some(open) = color.find('(') else {
        return false;
    };
    let name = &color[..open];
    if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") {
        return false;
    }
    if color.chars().count() > 120 || color.find(')') != Some(color.len() - 1) {
        return false;
    }
    let inner = color[open + 1..color.len() - 1].trim_matches(is_space);
    let legacy = inner.contains(',');
    let mut parts: Vec<&str>;
    let mut alpha: Option<&str> = None;
    if legacy {
        parts = inner.split(',').map(|s| s.trim_matches(is_space)).collect();
        if parts.len() == 4 {
            alpha = parts.pop();
        }
    } else {
        let slash: Vec<&str> = inner.split('/').collect();
        if slash.len() > 2 {
            return false;
        }
        let head = slash[0].trim_matches(is_space);
        parts = if head.is_empty() {
            vec![""]
        } else {
            head.split(is_space).filter(|s| !s.is_empty()).collect()
        };
        if slash.len() == 2 {
            alpha = Some(slash[1].trim_matches(is_space));
        }
    }
    if parts.len() != 3 || alpha.is_some_and(|a| !is_alpha(a, !legacy)) {
        return false;
    }
    let kinds = [kind(parts[0]), kind(parts[1]), kind(parts[2])];
    if legacy {
        if hsl {
            return (kinds[0] == K::Number || kinds[0] == K::Angle)
                && kinds[1] == K::Percent
                && kinds[2] == K::Percent;
        }
        return (kinds[0] == K::Number || kinds[0] == K::Percent)
            && kinds[1] == kinds[0]
            && kinds[2] == kinds[0];
    }
    for (i, &k) in kinds.iter().enumerate() {
        let ok = k == K::Number
            || k == K::None
            || if hsl && i == 0 {
                k == K::Angle
            } else {
                k == K::Percent
            };
        if !ok {
            return false;
        }
    }
    true
}

pub fn is_css_color(color: &str) -> bool {
    if color.contains('(') {
        return is_function(color);
    }
    let lower = color.to_lowercase();
    !lower.is_empty() && !lower.contains(' ') && KEYWORDS.contains(&format!(" {} ", lower))
}
