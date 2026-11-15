use crate::chars::is_space;

#[derive(Debug, Clone)]
pub struct XyText {
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DataPoint {
    pub value: f64,
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum XAxis {
    Band { title: String, categories: Vec<String> },
    Linear { title: String, min: f64, max: f64 },
}

impl XAxis {
    pub fn title(&self) -> &str {
        match self {
            XAxis::Band { title, .. } | XAxis::Linear { title, .. } => title,
        }
    }

    pub fn set_title(&mut self, new_title: String) {
        match self {
            XAxis::Band { title, .. } | XAxis::Linear { title, .. } => *title = new_title,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinearAxis {
    pub title: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotType {
    Line,
    Bar,
}

pub type PlotData = Vec<(String, Option<f64>)>;

#[derive(Debug, Clone)]
pub struct Plot {
    pub kind: PlotType,
    pub title: String,
    pub data: PlotData,
    pub point_labels: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct XyChartDb {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub orientation: Option<String>,
    pub x_axis: XAxis,
    pub y_axis: LinearAxis,
    pub has_set_x_axis: bool,
    pub has_set_y_axis: bool,
    pub plots: Vec<Plot>,
}

impl Default for XyChartDb {
    fn default() -> Self {
        Self {
            title: None,
            acc_title: None,
            acc_descr: None,
            orientation: None,
            x_axis: XAxis::Band { title: String::new(), categories: Vec::new() },
            y_axis: LinearAxis { title: String::new(), min: f64::INFINITY, max: f64::NEG_INFINITY },
            has_set_x_axis: false,
            has_set_y_axis: false,
            plots: Vec::new(),
        }
    }
}

pub fn js_float_str(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".to_string() } else { "-Infinity".to_string() };
    }
    let abs = v.abs();
    if (1e-6..1e21).contains(&abs) {
        crate::svg::builder::js_number(v)
    } else {
        let s = format!("{:e}", v);
        if let Some(e_pos) = s.find('e') {
            if !s[e_pos + 1..].starts_with('-') {
                return format!("{}e+{}", &s[..e_pos], &s[e_pos + 1..]);
            }
        }
        s
    }
}

fn collapse_newline_spaces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' && i + 1 < chars.len() && is_space(chars[i + 1]) {
            out.push('\n');
            i += 2;
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

impl XyChartDb {
    pub fn set_orientation(&mut self, orientation: &str) {
        self.orientation = Some(if orientation == "horizontal" { "horizontal".into() } else { "vertical".into() });
    }

    pub fn set_x_axis_title(&mut self, title: XyText) {
        self.x_axis.set_title(title.text.trim_matches(is_space).to_string());
    }

    pub fn set_x_axis_range_data(&mut self, min: f64, max: f64) {
        let title = self.x_axis.title().to_string();
        self.x_axis = XAxis::Linear { title, min, max };
        self.has_set_x_axis = true;
    }

    pub fn set_x_axis_band(&mut self, categories: Vec<XyText>) {
        let title = self.x_axis.title().to_string();
        let cats = categories.into_iter().map(|c| c.text.trim_matches(is_space).to_string()).collect();
        self.x_axis = XAxis::Band { title, categories: cats };
        self.has_set_x_axis = true;
    }

    pub fn set_y_axis_title(&mut self, title: XyText) {
        self.y_axis.title = title.text.trim_matches(is_space).to_string();
    }

    pub fn set_y_axis_range_data(&mut self, min: f64, max: f64) {
        self.y_axis.min = min;
        self.y_axis.max = max;
        self.has_set_y_axis = true;
    }

    fn plot_data(&mut self, mut values: Vec<f64>) -> PlotData {
        if values.is_empty() {
            return Vec::new();
        }
        if !self.has_set_x_axis {
            self.set_x_axis_range_data(1.0, values.len() as f64);
        }
        if let XAxis::Band { categories, .. } = &self.x_axis {
            if values.len() > categories.len() {
                values.truncate(categories.len());
            }
        }
        if !self.has_set_y_axis {
            let mut min = self.y_axis.min;
            let mut max = self.y_axis.max;
            for &v in &values {
                if v < min {
                    min = v;
                }
                if v > max {
                    max = v;
                }
            }
            self.y_axis.min = min;
            self.y_axis.max = max;
        }
        match &self.x_axis {
            XAxis::Band { categories, .. } => categories
                .iter()
                .enumerate()
                .map(|(i, cat)| (cat.clone(), values.get(i).copied()))
                .collect(),
            XAxis::Linear { min, max, .. } => {
                let x_min = *min;
                let x_max = *max;
                if values.len() == 1 {
                    return vec![(js_float_str(x_min), Some(values[0]))];
                }
                let step = (x_max - x_min) / (values.len() - 1) as f64;
                values
                    .into_iter()
                    .enumerate()
                    .map(|(i, val)| (js_float_str(x_min + i as f64 * step), Some(val)))
                    .collect()
            }
        }
    }

    fn add_plot(&mut self, kind: PlotType, title: XyText, data: Vec<DataPoint>) {
        let values: Vec<f64> = data.iter().map(|d| d.value).collect();
        let pdata = self.plot_data(values);
        let point_labels = if kind == PlotType::Line && data.iter().any(|d| !d.label.trim_matches(is_space).is_empty()) {
            Some(data.into_iter().map(|d| d.label.trim_matches(is_space).to_string()).collect())
        } else {
            None
        };
        self.plots.push(Plot {
            kind,
            title: title.text.trim_matches(is_space).to_string(),
            data: pdata,
            point_labels,
        });
    }

    pub fn set_line_data(&mut self, title: XyText, data: Vec<DataPoint>) {
        self.add_plot(PlotType::Line, title, data);
    }

    pub fn set_bar_data(&mut self, title: XyText, data: Vec<DataPoint>) {
        self.add_plot(PlotType::Bar, title, data);
    }

    pub fn set_diagram_title(&mut self, text: String) {
        self.title = Some(text);
    }

    pub fn set_acc_title(&mut self, text: &str) {
        self.acc_title = Some(text.trim_start_matches(is_space).to_string());
    }

    pub fn set_acc_description(&mut self, text: &str) {
        self.acc_descr = Some(collapse_newline_spaces(text));
    }
}
