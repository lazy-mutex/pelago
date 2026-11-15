use std::collections::HashMap;

use crate::chars::is_space;
use crate::errors::{ErrorCode, PeleError, Result};

use super::parser::{OptionValue, RadarAst};

#[derive(Debug, Clone)]
pub struct RadarAxis {
    pub name: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct RadarCurve {
    pub name: String,
    pub label: String,
    pub entries: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graticule {
    Circle,
    Polygon,
}

#[derive(Debug, Clone)]
pub struct RadarOptions {
    pub show_legend: bool,
    pub ticks: f64,
    pub max: Option<f64>,
    pub min: f64,
    pub graticule: Graticule,
}

#[derive(Debug, Clone)]
pub struct RadarModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub axes: Vec<RadarAxis>,
    pub curves: Vec<RadarCurve>,
    pub options: RadarOptions,
}

const MAX_TICKS: f64 = 32.0;

fn invalid(message: impl Into<String>) -> PeleError {
    PeleError::new(message, ErrorCode::Semantic).of("radar")
}

fn clean_acc_descr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut after_nl = false;
    for ch in s.chars() {
        if after_nl && is_space(ch) {
            continue;
        } else if ch == '\n' {
            out.push('\n');
            after_nl = true;
        } else {
            out.push(ch);
            after_nl = false;
        }
    }
    out
}

pub fn build_radar(ast: RadarAst, title: Option<String>) -> Result<RadarModel> {
    let axes: Vec<RadarAxis> = ast
        .axes
        .into_iter()
        .map(|axis| {
            let label = axis.label.unwrap_or_else(|| axis.name.clone());
            RadarAxis { name: axis.name, label }
        })
        .collect();

    let mut curves = Vec::with_capacity(ast.curves.len());
    for curve in ast.curves {
        let entries = if curve.entries[0].axis.is_none() {
            curve.entries.into_iter().map(|entry| entry.value).collect()
        } else {
            if axes.is_empty() {
                return Err(invalid("Axes must be populated before curves for reference entries"));
            }
            let mut by_axis: HashMap<String, f64> = HashMap::new();
            for entry in curve.entries {
                if let Some(axis_ref) = entry.axis {
                    by_axis.entry(axis_ref).or_insert(entry.value);
                }
            }
            let mut mapped = Vec::with_capacity(axes.len());
            for axis in &axes {
                let Some(&value) = by_axis.get(&axis.name) else {
                    return Err(invalid(format!("Missing entry for axis {}", axis.label)));
                };
                mapped.push(value);
            }
            mapped
        };
        let label = curve.label.unwrap_or_else(|| curve.name.clone());
        curves.push(RadarCurve { name: curve.name, label, entries });
    }

    let mut options = RadarOptions {
        show_legend: true,
        ticks: 5.0,
        max: None,
        min: 0.0,
        graticule: Graticule::Circle,
    };
    for opt in ast.options {
        match opt {
            OptionValue::ShowLegend(v) => options.show_legend = v,
            OptionValue::Graticule(v) => options.graticule = v,
            OptionValue::Ticks(v) => options.ticks = v,
            OptionValue::Max(v) => options.max = Some(v),
            OptionValue::Min(v) => options.min = v,
        }
    }
    if options.ticks > MAX_TICKS {
        options.ticks = MAX_TICKS;
    }

    let resolved_title = ast.title.filter(|t| !t.is_empty()).or(title);
    let acc_title = ast
        .acc_title
        .filter(|t| !t.is_empty())
        .map(|t| t.trim_start_matches(is_space).to_string());
    let acc_descr = ast
        .acc_descr
        .filter(|d| !d.is_empty())
        .map(|d| clean_acc_descr(&d));

    Ok(RadarModel {
        title: resolved_title,
        acc_title,
        acc_descr,
        axes,
        curves,
        options,
    })
}
