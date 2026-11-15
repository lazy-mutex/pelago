use crate::text::measurer::TextMeasurer;

#[derive(Debug, Clone)]
pub struct Fitted {
    pub text: String,
    pub size: f64,
    pub width: f64,
}

// Fits one line of text into a width: first by shrinking the font down to `min`, then by cutting
// the text and adding an ellipsis. Returns None when not even one character fits.
pub fn fit_text(
    text: &str,
    max: f64,
    size: f64,
    min: f64,
    measurer: &dyn TextMeasurer,
    style: u8,
) -> Option<Fitted> {
    if text.is_empty() || !(max > 0.0) {
        return None;
    }
    let mut width = measurer.width(text, size, style);
    if width <= max {
        return Some(Fitted { text: text.to_string(), size, width });
    }

    let mut fitted = min.max(((size * max) / width).floor());
    while fitted >= min {
        width = measurer.width(text, fitted, style);
        if width <= max {
            return Some(Fitted { text: text.to_string(), size: fitted, width });
        }
        fitted -= 1.0;
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let mut low = 0usize;
    let mut high = chars.len() - 1;
    while low < high {
        let mid = (low + high + 1) >> 1;
        let prefix: String = chars[..mid].iter().collect();
        let candidate = format!("{}…", prefix.trim_end());
        if measurer.width(&candidate, min, style) <= max {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    if low == 0 {
        return None;
    }
    let prefix: String = chars[..low].iter().collect();
    let cut = format!("{}…", prefix.trim_end());
    width = measurer.width(&cut, min, style);
    (width <= max).then_some(Fitted { text: cut, size: min, width })
}
