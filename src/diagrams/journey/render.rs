use super::db::{journey_actors, JourneyDb};
use crate::chars::is_space;
use crate::diagrams::common::fit::{fit_label, plain_label};
use crate::options::{turn_to_fit, RenderOptions, Rendered};
use crate::svg::builder::{esc_text, js_number, js_round, label_svg, num};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{series_color, ROUNDED};
use crate::text::label::Label;
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::value::Value;
use std::collections::HashMap;

const PAD_X: f64 = 10.0;
const PAD_Y: f64 = 8.0;
const MIN_TEXT: f64 = 124.0;
const MAX_TEXT: f64 = 200.0;
const GAP: f64 = 12.0;
const BAND_PAD: f64 = 6.0;
const GUTTER: f64 = 24.0;
const LEVEL: f64 = 26.0;
const FACE: f64 = 11.0;
const DOT: f64 = 5.0;
const DOT_STEP: f64 = 13.0;
const LEGEND_GAP: f64 = 20.0;

const EYES: &str = "<circle cx=\"-3.6\" cy=\"-2.8\" r=\"1.3\"/><circle cx=\"3.6\" cy=\"-2.8\" r=\"1.3\"/>";

#[derive(Clone, Copy)]
enum Mood {
    Happy,
    Neutral,
    Sad,
}

impl Mood {
    fn from_score(score: f64) -> Self {
        if score > 3.0 {
            Mood::Happy
        } else if score < 3.0 {
            Mood::Sad
        } else {
            Mood::Neutral
        }
    }

    fn name(self) -> &'static str {
        match self {
            Mood::Happy => "happy",
            Mood::Neutral => "neutral",
            Mood::Sad => "sad",
        }
    }

    fn mouth(self) -> &'static str {
        match self {
            Mood::Happy => "M-5,2A6,6 0 0 0 5,2",
            Mood::Neutral => "M-4.5,4.5H4.5",
            Mood::Sad => "M-5,6.5A6,6 0 0 1 5,6.5",
        }
    }
}

fn face(x: f64, y: f64, mood: Mood) -> String {
    format!(
        "<g class=\"pele-face pele-face-{}\" transform=\"translate({},{})\"><circle r=\"{}\" fill=\"var(--_bg)\" stroke=\"var(--_l)\"/><g fill=\"var(--_l)\">{}</g><path d=\"{}\" fill=\"none\" stroke=\"var(--_l)\" stroke-linecap=\"round\"/></g>",
        mood.name(),
        num(x),
        num(y),
        js_number(FACE),
        EYES,
        mood.mouth()
    )
}

struct LegendItem {
    name: String,
    label: Label,
    x: f64,
    y: f64,
    h: f64,
}

struct SectionRun {
    start: usize,
    count: usize,
    label: Option<Label>,
}

pub fn render_journey(model: &JourneyDb, config: &Value, options: &RenderOptions) -> Rendered {
    turn_to_fit(options, &|down| draw(model, config, options, down))
}

fn finish_svg(
    model: &JourneyDb,
    config: &Value,
    options: &RenderOptions,
    width: f64,
    height: f64,
    size: f64,
    content: &str,
) -> Rendered {
    let fixed = config.get("journey").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "journey",
        width,
        height,
        size,
        &Document {
            id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
            responsive: options.responsive.unwrap_or(!fixed),
            font_family: options.font_family.as_deref(),
            font_family_mono: options.font_family_mono.as_deref(),
            palette: "",
        },
        &Accessible {
            acc_title: model.acc_title.as_deref(),
            acc_descr: model.acc_descr.as_deref(),
        },
        content,
    );
    Rendered {
        svg,
        width,
        height,
        links: Vec::new(),
    }
}

fn draw(model: &JourneyDb, config: &Value, options: &RenderOptions, down: bool) -> Rendered {
    if down {
        return vertical_journey(model, config, options);
    }
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let tiny = js_round(size * 0.75);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let tasks = &model.tasks;
    let n = tasks.len();

    // `Task: 5:` names an actor with no name; Mermaid gives it a dot and an empty legend row.
    let names: Vec<String> = journey_actors(tasks)
        .into_iter()
        .filter(|name| !name.is_empty())
        .collect();
    let actor: HashMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();

    // Cards widen, up to a point, for a word that would not fit; past that the word is cut.
    let mut text_w = MIN_TEXT;
    let mut labels: Vec<Label> = tasks
        .iter()
        .map(|task| plain_label(Some(&task.task), measurer, small, text_w, 0))
        .collect();
    let mut widest = 0.0f64;
    for label in &labels {
        widest = widest.max(label.width);
    }
    if widest > text_w {
        text_w = widest.ceil().min(MAX_TEXT);
        labels = tasks
            .iter()
            .map(|task| fit_label(Some(&task.task), measurer, small, text_w, 0))
            .collect();
    }

    let card_w = text_w + 2.0 * PAD_X;
    let pitch = card_w + GAP;
    let plot_w = if n > 0 { n as f64 * pitch - GAP } else { 0.0 };

    let line_h = js_round(small * 1.5);
    let limit = plot_w.max(240.0);
    let mut legend: Vec<LegendItem> = Vec::new();
    let mut legend_w = 0.0f64;
    let mut legend_h = 0.0f64;
    let mut lx = 0.0f64;
    for name in &names {
        let label = fit_label(Some(name), measurer, small, limit - 2.0 * DOT - 6.0, 0);
        let w = 2.0 * DOT + 6.0 + label.width;
        if lx > 0.0 && lx + w > limit {
            lx = 0.0;
            legend_h += 4.0;
        }
        let y = if lx > 0.0 {
            legend.last().unwrap().y
        } else {
            legend_h
        };
        let h = label.height.max(line_h);
        legend.push(LegendItem {
            name: name.clone(),
            label,
            x: lx,
            y,
            h,
        });
        lx += w;
        legend_w = legend_w.max(lx);
        legend_h = legend_h.max(y + h);
        lx += LEGEND_GAP;
    }

    let body_w = if n > 0 { GUTTER + plot_w } else { 0.0 };
    let title = fit_label(
        model.title.as_deref(),
        measurer,
        size,
        body_w.max(320.0),
        style::BOLD,
    );
    let title_h = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let width = body_w
        .max(title.width)
        .max(if n > 0 { GUTTER + legend_w } else { 0.0 });
    let left = pad + (width - body_w) / 2.0 + GUTTER;
    let top = pad + title_h;

    // A band covers a run of tasks that share a section, as in Mermaid; a section with no tasks draws nothing.
    let mut runs: Vec<SectionRun> = Vec::new();
    let mut band_h = 0.0f64;
    let mut i = 0usize;
    while i < n {
        let mut end = i + 1;
        while end < n && tasks[end].section == tasks[i].section {
            end += 1;
        }
        let label = if !tasks[i].section.is_empty() {
            let lbl = fit_label(
                Some(&tasks[i].section),
                measurer,
                small,
                (end - i) as f64 * pitch - GAP - 2.0 * PAD_X,
                style::BOLD,
            );
            band_h = band_h.max(lbl.height.max(line_h) + 2.0 * BAND_PAD);
            Some(lbl)
        } else {
            None
        };
        runs.push(SectionRun {
            start: i,
            count: end - i,
            label,
        });
        i = end;
    }

    let mut text_h = line_h;
    for label in &labels {
        text_h = text_h.max(label.height);
    }
    let dots_h = if !names.is_empty() {
        2.0 * DOT + 6.0
    } else {
        0.0
    };
    let card_h = text_h + dots_h + 2.0 * PAD_Y;
    let cards_y = top + if band_h > 0.0 { band_h + BAND_PAD } else { 0.0 };
    let chart_top = cards_y + card_h + FACE + 16.0;
    let chart_bottom = chart_top + 4.0 * LEVEL;
    let right = left + plot_w;

    let mut body = String::new();
    let mut bottom = top;
    let small_str = js_number(small);
    if n > 0 {
        let mut grid = String::new();
        let mut scale = String::new();
        for score in (1..=5).rev() {
            let y = chart_top + (5 - score) as f64 * LEVEL;
            grid.push_str(&format!("M{},{}H{}", num(left - 6.0), num(y), num(right)));
            scale.push_str(&format!(
                "<text x=\"{}\" y=\"{}\">{}</text>",
                num(left - 12.0),
                num(y + tiny * 0.35),
                score
            ));
        }
        body.push_str(&format!(
            "<path class=\"pele-grid\" d=\"{}\" fill=\"none\" stroke=\"var(--_a)\"/><g class=\"pele-scale\" font-size=\"{}\" text-anchor=\"end\" fill=\"var(--_m)\">{}</g>",
            grid,
            js_number(tiny),
            scale
        ));

        let mut line = String::new();
        let mut stems = String::new();
        let mut faces = String::new();
        let mut cards: Vec<String> = Vec::with_capacity(n);
        for (idx, task) in tasks.iter().enumerate() {
            let x = left + idx as f64 * pitch;
            let cx = x + card_w / 2.0;
            let score = task.score;
            let level = if score.is_nan() {
                3.0
            } else {
                score.max(1.0).min(5.0)
            };
            let y = chart_top + (5.0 - level) * LEVEL;
            line.push_str(&format!(
                "{}{},{}",
                if idx == 0 { "M" } else { "L" },
                num(cx),
                num(y)
            ));
            stems.push_str(&format!(
                "M{},{}V{}",
                num(cx),
                num(cards_y + card_h),
                num(y - FACE)
            ));
            faces.push_str(&face(cx, y, Mood::from_score(score)));

            let mut dots = String::new();
            let people: Vec<&str> = task
                .people
                .iter()
                .map(|s| s.as_str())
                .filter(|s| !s.is_empty())
                .collect();
            let denom = if people.len() > 1 {
                (people.len() - 1) as f64
            } else {
                1.0
            };
            let step = DOT_STEP.min((card_w - 2.0 * PAD_X - 2.0 * DOT) / denom);
            for (k, &name) in people.iter().enumerate() {
                let offset = (k as f64 - (people.len() as f64 - 1.0) / 2.0) * step;
                let color_idx = *actor.get(name).unwrap_or(&0) as i64;
                dots.push_str(&format!(
                    "<circle class=\"pele-actor\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" stroke=\"var(--_s)\"><title>{}</title></circle>",
                    num(cx + offset),
                    num(cards_y + card_h - PAD_Y - DOT),
                    js_number(DOT),
                    series_color(color_idx),
                    esc_text(name)
                ));
            }
            let label_attr = format!(" font-size=\"{}\"", small_str);
            cards.push(format!(
                "<g class=\"pele-node pele-task\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>{}{}</g>",
                esc_text(task.task.trim_matches(is_space)),
                num(x),
                num(cards_y),
                num(card_w),
                num(card_h),
                ROUNDED,
                label_svg(&labels[idx], cx, cards_y + PAD_Y + text_h / 2.0, &label_attr),
                dots
            ));
        }

        body.push_str(&format!(
            "<path class=\"pele-stems\" d=\"{}\" fill=\"none\" stroke=\"var(--_b)\" stroke-dasharray=\"2 3\"/>",
            stems
        ));
        if n > 1 {
            body.push_str(&format!(
                "<path class=\"pele-edge\" d=\"{}\" fill=\"none\" stroke=\"var(--_l)\" stroke-linejoin=\"round\"/>",
                line
            ));
        }
        body.push_str(&faces);

        let band_label_attr = format!(" font-size=\"{}\" font-weight=\"var(--_hw)\"", small_str);
        for run in &runs {
            let members = cards[run.start..run.start + run.count].join("");
            let Some(ref lbl) = run.label else {
                body.push_str(&members);
                continue;
            };
            let x = left + run.start as f64 * pitch;
            let w = run.count as f64 * pitch - GAP;
            body.push_str(&format!(
                "<g class=\"pele-cluster pele-section\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_a)\"{}/>{}{}</g>",
                esc_text(tasks[run.start].section.trim_matches(is_space)),
                num(x),
                num(top),
                num(w),
                num(band_h),
                ROUNDED,
                label_svg(lbl, x + w / 2.0, top + band_h / 2.0, &band_label_attr),
                members
            ));
        }
        bottom = chart_bottom + FACE;

        if !legend.is_empty() {
            let x0 = left + (plot_w - legend_w) / 2.0;
            let y0 = bottom + 16.0;
            let mut items = String::new();
            for item in &legend {
                let cy = y0 + item.y + item.h / 2.0;
                let color_idx = *actor.get(item.name.as_str()).unwrap_or(&0) as i64;
                items.push_str(&format!(
                    "<g class=\"pele-legend-item\" data-id=\"{}\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>{}</g>",
                    esc_text(&item.name),
                    num(x0 + item.x + DOT),
                    num(cy),
                    js_number(DOT),
                    series_color(color_idx),
                    label_svg(
                        &item.label,
                        x0 + item.x + 2.0 * DOT + 6.0 + item.label.width / 2.0,
                        cy,
                        ""
                    )
                ));
            }
            body.push_str(&format!(
                "<g class=\"pele-legend\" font-size=\"{}\">{}</g>",
                small_str, items
            ));
            bottom = y0 + legend_h;
        }
    }

    let total_width = (width + 2.0 * pad).ceil();
    let total_height = (bottom.max(pad) + pad).ceil();
    let content = format!(
        "{}{}",
        label_svg(
            &title,
            pad + width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        body
    );
    finish_svg(
        model,
        config,
        options,
        total_width,
        total_height,
        size,
        &content,
    )
}

fn vertical_journey(model: &JourneyDb, config: &Value, options: &RenderOptions) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let small = js_round(size * 0.875);
    let small_str = js_number(small);
    let measurer: &dyn TextMeasurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let room = (options.max_width.unwrap_or(320.0) - 2.0 * pad).max(140.0);
    let title = fit_label(model.title.as_deref(), measurer, size, room, style::BOLD);
    let names: Vec<String> = journey_actors(&model.tasks)
        .into_iter()
        .filter(|name| !name.is_empty())
        .collect();
    let actors: HashMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();

    let mut y = pad + if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let mut section: Option<&str> = None;
    let mut body = String::new();
    let mut width = title.width;
    let actor_label_attr = format!(" class=\"pele-journey-actor\" font-size=\"{}\"", small_str);

    for (index, task) in model.tasks.iter().enumerate() {
        if section != Some(task.section.as_str()) {
            section = Some(task.section.as_str());
            let head = fit_label(
                Some(&task.section),
                measurer,
                size,
                room - 16.0,
                style::BOLD,
            );
            if head.height > 0.0 {
                body.push_str(&label_svg(
                    &head,
                    pad + 8.0 + head.width / 2.0,
                    y + head.height / 2.0,
                    " class=\"pele-cluster-label\" font-weight=\"var(--_hw)\"",
                ));
                width = width.max(head.width + 16.0);
                y += head.height + 8.0;
            }
        }
        let label = fit_label(Some(&task.task), measurer, small, room - 72.0, 0);
        let people: Vec<(&str, Label)> = task
            .people
            .iter()
            .filter(|name| !name.is_empty())
            .map(|name| {
                (
                    name.as_str(),
                    fit_label(Some(name), measurer, small, room - 44.0, 0),
                )
            })
            .collect();
        let text_h = label.height.max(42.0);
        let mut h = 2.0 * PAD_Y + text_h;
        for (_, person_label) in &people {
            h += person_label.height.max(16.0) + 4.0;
        }
        let w = room.max(label.width + 72.0);
        body.push_str(&format!(
            "<g class=\"pele-node pele-journey-step\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--_s)\" stroke=\"var(--_b)\"{}/>",
            index,
            num(pad),
            num(y),
            num(w),
            num(h),
            ROUNDED
        ));
        body.push_str(&label_svg(
            &label,
            pad + PAD_X + label.width / 2.0,
            y + PAD_Y + label.height / 2.0,
            " class=\"pele-label\"",
        ));
        let cx = pad + w - 28.0;
        body.push_str(&face(cx, y + PAD_Y + FACE, Mood::from_score(task.score)));
        if !task.score.is_nan() {
            body.push_str(&format!(
                "<text class=\"pele-journey-score\" x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\">{}</text>",
                num(cx),
                num(y + PAD_Y + 2.0 * FACE + small),
                small_str,
                num(task.score.max(1.0).min(5.0))
            ));
        }
        let mut py = y + PAD_Y + text_h + 4.0;
        for (person_name, person_label) in &people {
            let ph = person_label.height.max(16.0);
            let color_idx = *actors.get(person_name).unwrap_or(&0) as i64;
            body.push_str(&format!(
                "<circle class=\"pele-actor\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>{}",
                num(pad + 16.0),
                num(py + ph / 2.0),
                js_number(DOT),
                series_color(color_idx),
                label_svg(
                    person_label,
                    pad + 28.0 + person_label.width / 2.0,
                    py + ph / 2.0,
                    &actor_label_attr
                )
            ));
            py += ph + 4.0;
        }
        body.push_str("</g>");
        y += h;
        if index + 1 < model.tasks.len() {
            body.push_str(&format!(
                "<path class=\"pele-edge\" d=\"M{},{}v16\" fill=\"none\" stroke=\"var(--_l)\"/>",
                num(pad + w / 2.0),
                num(y)
            ));
        }
        y += 16.0;
        width = width.max(w);
    }

    let total_width = (width + 2.0 * pad).ceil();
    let trailing = if !model.tasks.is_empty() { 16.0 } else { 0.0 };
    let height = ((pad + title.height).max(y - trailing) + pad).ceil();
    let content = format!(
        "{}{}",
        label_svg(
            &title,
            total_width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\""
        ),
        body
    );
    finish_svg(model, config, options, total_width, height, size, &content)
}
