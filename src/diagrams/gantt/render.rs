use super::axis::{auto_ticks, interval_ticks, time_format};
use super::dates::{add, add_days, JsDate, Unit};
use super::db::{clock, GanttDb, GanttTask};
use crate::chars::is_space;
use crate::errors::Result;
use crate::options::{cap_width, LinkInfo, RenderOptions, Rendered};
use crate::svg::builder::{esc, esc_text, js_number, js_round, label_svg, num};
use crate::svg::root::{svg_document, Accessible, Document};
use crate::svg::theme::{resolve_style, CRITICAL, ROUNDED};
use crate::text::label::{layout_label, Label};
use crate::text::measurer::{style, MetricsMeasurer, TextMeasurer};
use crate::url::linked;
use crate::value::Value;
use std::collections::HashMap;

const WIDTH: f64 = 800.0;
const MIN_PLOT: f64 = 240.0;
const LABEL_GAP: f64 = 8.0;

fn setting(config: &Value, key: &str, min: f64, max: f64) -> Option<f64> {
    config
        .get("gantt")
        .get(key)
        .as_f64()
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(min, max))
}

fn text<'a>(config: &'a Value, key: &str) -> Option<&'a str> {
    config.get("gantt").get(key).as_str()
}

fn flag(config: &Value, key: &str) -> bool {
    config.get("gantt").get(key).as_bool() == Some(true)
}

// Replaces `/<br\s*\/?>/gi` with `' '`.
fn replace_breaks(raw: &str) -> String {
    let s: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0usize;
    while i < s.len() {
        if s[i] == '<'
            && i + 2 < s.len()
            && s[i + 1].eq_ignore_ascii_case(&'b')
            && s[i + 2].eq_ignore_ascii_case(&'r')
        {
            let mut j = i + 3;
            while j < s.len() && is_space(s[j]) {
                j += 1;
            }
            if s.get(j) == Some(&'/') {
                j += 1;
            }
            if s.get(j) == Some(&'>') {
                out.push(' ');
                i = j + 1;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

// A task label sits on one row, so only its first line is kept.
fn one_line(raw: &str, measurer: &dyn TextMeasurer, size: f64) -> Label {
    let cleaned = replace_breaks(raw);
    let label = layout_label(Some(&cleaned), false, measurer, size, 4000.0, 0);
    if label.lines.len() <= 1 {
        return label;
    }
    let w0 = label.widths[0];
    Label {
        lines: vec![label.lines[0].clone()],
        widths: vec![w0],
        width: w0,
        height: label.line_height,
        size: label.size,
        line_height: label.line_height,
    }
}

fn scheduled(task: &GanttTask) -> bool {
    task.start_time.is_some_and(|d| d.time().is_finite())
        && task.end_time.is_some_and(|d| d.time().is_finite())
}

// Mermaid's compact mode: within a section, a task goes on the first row that is free when it starts.
fn pack_rows(
    all_tasks: &[GanttTask],
    group_indices: &[usize],
    offset: usize,
    row_of: &mut [Option<usize>],
) -> usize {
    let mut sorted: Vec<usize> = group_indices
        .iter()
        .copied()
        .filter(|&idx| scheduled(&all_tasks[idx]))
        .collect();
    sorted.sort_by(|&a, &b| {
        let sa = all_tasks[a].start_time.unwrap().time();
        let sb = all_tasks[b].start_time.unwrap().time();
        sa.total_cmp(&sb).then_with(|| all_tasks[a].order.cmp(&all_tasks[b].order))
    });

    let mut free: Vec<f64> = Vec::new();
    for idx in sorted {
        let start = all_tasks[idx].start_time.unwrap().time();
        let mut row = 0usize;
        while row < free.len() && start < free[row] {
            row += 1;
        }
        let end = all_tasks[idx].end_time.unwrap().time();
        if row == free.len() {
            free.push(end);
        } else {
            free[row] = end;
        }
        row_of[idx] = Some(offset + row);
    }

    let mut rows = free.len();
    for &idx in group_indices {
        if row_of[idx].is_none() {
            row_of[idx] = Some(offset + rows);
            rows += 1;
        }
    }
    rows.max(1)
}

struct Bar {
    task_idx: usize,
    label: Label,
    row: usize,
    x0: f64,
    x1: f64,
    label_x: f64,
    inside: bool,
}

pub fn render_gantt(
    model: &mut GanttDb,
    config: &Value,
    options: &RenderOptions,
) -> Result<Rendered> {
    let size = options.font_size.unwrap_or(16.0);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);
    let now = clock(None);

    let label_size = setting(config, "fontSize", 4.0, 72.0).unwrap_or_else(|| js_round(size * 0.8125));
    let section_size = setting(config, "sectionFontSize", 4.0, 72.0).unwrap_or(label_size);
    let tick_size = js_round(size * 0.75);
    let bar_height =
        setting(config, "barHeight", 4.0, 400.0).unwrap_or_else(|| js_round(label_size * 1.85));
    let bar_gap =
        setting(config, "barGap", 0.0, 400.0).unwrap_or_else(|| js_round(bar_height / 4.0));
    let row_height = bar_height + bar_gap;
    let section_styles =
        js_round(setting(config, "numberSectionStyles", 1.0, 1000.0).unwrap_or(4.0)) as usize;
    let compact = model.display_mode == "compact";
    let top_axis = model.top_axis || flag(config, "topAxis");

    model.get_tasks(Some(now))?;
    let tasks = model.tasks.clone();

    let row_indices: Vec<usize> = (0..tasks.len()).filter(|&i| !tasks[i].vert).collect();
    let mut vert_indices: Vec<usize> = (0..tasks.len())
        .filter(|&i| tasks[i].vert && scheduled(&tasks[i]))
        .collect();

    let mut categories: HashMap<String, usize> = HashMap::new();
    let mut category_order: Vec<String> = Vec::new();
    for &idx in &row_indices {
        let sec = &tasks[idx].section;
        if !categories.contains_key(sec) {
            let next_id = categories.len();
            categories.insert(sec.clone(), next_id);
            category_order.push(sec.clone());
        }
    }

    let mut row_of: Vec<Option<usize>> = vec![None; tasks.len()];
    let mut row_section: Vec<String> = Vec::new();
    if compact {
        let mut group_pos: HashMap<String, usize> = HashMap::new();
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for &idx in &row_indices {
            let sec = &tasks[idx].section;
            if let Some(&g_idx) = group_pos.get(sec) {
                groups[g_idx].1.push(idx);
            } else {
                group_pos.insert(sec.clone(), groups.len());
                groups.push((sec.clone(), vec![idx]));
            }
        }
        for (section, group) in groups {
            let rows = pack_rows(&tasks, &group, row_section.len(), &mut row_of);
            for _ in 0..rows {
                row_section.push(section.clone());
            }
        }
    } else {
        for &idx in &row_indices {
            row_of[idx] = Some(row_section.len());
            row_section.push(tasks[idx].section.clone());
        }
    }
    let row_count = row_section.len();

    let mut start = f64::INFINITY;
    let mut stop = f64::NEG_INFINITY;
    for task in &tasks {
        if !scheduled(task) {
            continue;
        }
        start = start.min(task.start_time.unwrap().time());
        stop = stop.max(task.end_time.unwrap().time());
    }
    let has_plot = start <= stop;

    let title = layout_label(model.title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 16.0 } else { 0.0 };

    let fixed = config.get("gantt").get("useMaxWidth").as_bool() == Some(false);
    let doc = Document {
        id_prefix: options.id_prefix.as_deref().unwrap_or("pele"),
        responsive: options.responsive.unwrap_or(!fixed),
        font_family: options.font_family.as_deref(),
        font_family_mono: options.font_family_mono.as_deref(),
        palette: "",
    };

    if !has_plot {
        let width = (title.width.max(1.0) + 2.0 * pad).ceil();
        let height = (title.height.max(1.0) + 2.0 * pad).ceil();
        let content = label_svg(
            &title,
            width / 2.0,
            pad + title.height / 2.0,
            " class=\"pele-title\" font-weight=\"var(--_tw)\"",
        );
        let acc = Accessible {
            acc_title: model.acc_title.as_deref(),
            acc_descr: model.acc_descr.as_deref(),
        };
        let svg = svg_document("gantt", width, height, size, &doc, &acc, &content);
        return Ok(Rendered { svg, width, height, links: Vec::new() });
    }

    let mut section_labels: HashMap<String, Label> = HashMap::new();
    let mut section_width = 0.0_f64;
    for section in &category_order {
        let label = layout_label(Some(section), false, measurer, section_size, 180.0, 0);
        section_width = section_width.max(label.width);
        section_labels.insert(section.clone(), label);
    }

    let axis_format = if !model.axis_format.is_empty() {
        model.axis_format.clone()
    } else if model.date_format == "D" {
        "%d".to_string()
    } else {
        text(config, "axisFormat").unwrap_or("%Y-%m-%d").to_string()
    };
    let tick_label = |t: f64| -> String { time_format(&axis_format, JsDate::from_ms(t)) };
    let sample_width = measurer
        .width(&tick_label(start), tick_size, 0)
        .max(measurer.width(&tick_label(stop), tick_size, 0));

    let default_left = (if section_width > 0.0 {
        section_width + 24.0
    } else {
        0.0
    })
    .max(sample_width / 2.0);
    let left = pad + setting(config, "leftPadding", 0.0, 2000.0).unwrap_or(default_left);
    let right_padding =
        setting(config, "rightPadding", 0.0, 2000.0).unwrap_or_else(|| (sample_width / 2.0).max(16.0));
    let target = cap_width(
        options,
        setting(config, "useWidth", 100.0, 20_000.0).unwrap_or(WIDTH),
        280.0,
        0.0,
    );
    let plot_width = (target - left - right_padding - pad).max(MIN_PLOT);
    let right = left + plot_width;
    let scale = if stop > start {
        plot_width / (stop - start)
    } else {
        0.0
    };
    let x = |t: f64| -> f64 {
        if scale == 0.0 {
            left + plot_width / 2.0
        } else {
            left + (t - start) * scale
        }
    };

    let tick_height = js_round(tick_size * 1.75);
    let top_padding = setting(config, "topPadding", 0.0, 2000.0);
    let grid_start = setting(config, "gridLineStartPadding", 0.0, 2000.0);
    let head_height = pad + title_height + if top_axis { tick_height } else { 0.0 };
    let plot_top = head_height.max(top_padding.unwrap_or(0.0));
    let grid_lead = match (top_padding, grid_start) {
        (Some(tp), Some(gs)) => (tp - gs).min(plot_top - head_height).max(0.0),
        _ => 0.0,
    };
    let plot_height = (row_count.max(1) as f64) * row_height;
    let plot_bottom = plot_top + plot_height;

    let mut bars: Vec<Bar> = Vec::with_capacity(row_indices.len());
    let mut extent = right + right_padding;
    for &idx in &row_indices {
        let task = &tasks[idx];
        let label = one_line(&task.task, measurer, label_size);
        let row = row_of[idx].unwrap_or(0);
        if !scheduled(task) {
            let label_x = left + LABEL_GAP + label.width / 2.0;
            extent = extent.max(label_x + label.width / 2.0);
            bars.push(Bar {
                task_idx: idx,
                label,
                row,
                x0: f64::NAN,
                x1: f64::NAN,
                label_x,
                inside: false,
            });
            continue;
        }
        let mut x0 = x(task.start_time.unwrap().time());
        let render_end = task.render_end_time.unwrap_or_else(|| task.end_time.unwrap()).time();
        let mut x1 = x(render_end).max(x0 + if task.milestone { 0.0 } else { 2.0 });
        if task.milestone {
            let mid = x0 + (x(task.end_time.unwrap().time()) - x0) / 2.0;
            x0 = mid - bar_height / 2.0;
            x1 = mid + bar_height / 2.0;
        }
        let inside = !task.milestone && label.width + 2.0 * LABEL_GAP <= x1 - x0;
        let mut label_x = (x0 + x1) / 2.0;
        if !inside {
            let fits_right = x1 + LABEL_GAP + label.width <= right + right_padding;
            let fits_left = x0 - LABEL_GAP - label.width >= left;
            label_x = if fits_right || !fits_left {
                x1 + LABEL_GAP + label.width / 2.0
            } else {
                x0 - LABEL_GAP - label.width / 2.0
            };
            extent = extent.max(label_x + label.width / 2.0);
        }
        bars.push(Bar { task_idx: idx, label, row, x0, x1, label_x, inside });
    }

    let interval = model
        .tick_interval
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| text(config, "tickInterval").filter(|s| !s.is_empty()))
        .unwrap_or("");
    let weekday = model
        .weekday
        .as_deref()
        .or_else(|| text(config, "weekday"))
        .unwrap_or("sunday");
    let count = ((plot_width / (sample_width + 24.0)).floor() as usize).clamp(2, 10);
    let ticks = interval_ticks(interval, weekday, start, stop, (plot_width / 3.0).ceil() as usize)
        .unwrap_or_else(|| auto_ticks(start, stop, count));

    let mut grid = String::new();
    let mut axis_bottom = String::new();
    let mut axis_top = String::new();
    let mut last_right = f64::NEG_INFINITY;
    let mut last_label = String::new();
    let bottom_y = plot_bottom + tick_height / 2.0 + 2.0;
    let top_y = plot_top - grid_lead - tick_height / 2.0 - 2.0;
    for t in ticks {
        let tx = x(t);
        grid.push_str(&format!("M{},{}V{}", num(tx), num(plot_top - grid_lead), num(plot_bottom)));
        let label = tick_label(t);
        let lw = measurer.width(&label, tick_size, 0);
        if tx - lw / 2.0 < last_right + 8.0 || label == last_label {
            continue;
        }
        last_right = tx + lw / 2.0;
        last_label = label.clone();
        extent = extent.max(last_right);
        let body = format!("text-anchor=\"middle\">{}</text>", esc(&label));
        axis_bottom.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" {}",
            num(tx),
            num(bottom_y + tick_size * 0.35),
            body
        ));
        if top_axis {
            axis_top.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" {}",
                num(tx),
                num(top_y + tick_size * 0.35),
                body
            ));
        }
    }

    // Vertical markers take no row. Their labels go under the axis, stacked to avoid overlap.
    vert_indices.sort_by(|&a, &b| {
        tasks[a]
            .start_time
            .unwrap()
            .time()
            .total_cmp(&tasks[b].start_time.unwrap().time())
    });
    let mut vert_lines = String::new();
    let mut vert_labels = String::new();
    let mut vert_ends: Vec<f64> = Vec::new();
    let vert_top = plot_bottom + tick_height + 4.0;
    let vert_line = js_round(label_size * 1.5);
    for idx in vert_indices {
        let task = &tasks[idx];
        let vx = x(task.start_time.unwrap().time());
        let label = one_line(&task.task, measurer, label_size);
        let cx = vx.max(pad + label.width / 2.0);
        let mut line = 0usize;
        while line < vert_ends.len() && cx - label.width / 2.0 < vert_ends[line] + 8.0 {
            line += 1;
        }
        let right_edge = cx + label.width / 2.0;
        if line == vert_ends.len() {
            vert_ends.push(right_edge);
        } else {
            vert_ends[line] = right_edge;
        }
        extent = extent.max(right_edge);
        vert_lines.push_str(&format!(
            "<path class=\"pele-vert\" data-id=\"{}\" d=\"M{},{}V{}\"/>",
            esc_text(&task.id),
            num(vx),
            num(plot_top - grid_lead),
            num(plot_bottom)
        ));
        vert_labels.push_str(&label_svg(
            &label,
            cx,
            vert_top + (line as f64) * vert_line + vert_line / 2.0,
            &format!(" class=\"pele-vert-label\" data-id=\"{}\"", esc_text(&task.id)),
        ));
    }

    let width = (extent.max(pad + title.width) + pad).ceil();
    let vert_extra = if !vert_ends.is_empty() {
        4.0 + (vert_ends.len() as f64) * vert_line
    } else {
        0.0
    };
    let height = (plot_bottom + tick_height + vert_extra + pad).ceil();

    // Section bands, one per run of rows that share a section, shaded on alternate sections.
    let mut bands = String::new();
    let mut section_out = String::new();
    let mut row = 0usize;
    while row < row_count {
        let section = &row_section[row];
        let mut end = row + 1;
        while end < row_count && &row_section[end] == section {
            end += 1;
        }
        let index = categories[section] % section_styles;
        let y = plot_top + (row as f64) * row_height;
        let h = ((end - row) as f64) * row_height;
        if categories.len() > 1 || !section.is_empty() {
            let fill = if index % 2 == 0 { "var(--_s)" } else { "none" };
            bands.push_str(&format!(
                "<rect class=\"pele-section pele-section-{}\" x=\"0\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                index,
                num(y),
                js_number(width),
                num(h),
                fill
            ));
        }
        let label = &section_labels[section];
        section_out.push_str(&label_svg(
            label,
            pad + 8.0 + label.width / 2.0,
            y + h / 2.0,
            &format!(" class=\"pele-section-label\" data-id=\"{}\"", esc_text(section)),
        ));
        row = end;
    }

    // Excluded days: a day at a time from the first task's start, skipped if chart spans > 5 years.
    let mut excluded = String::new();
    if (!model.excludes.is_empty() || !model.includes.is_empty())
        && add(JsDate::from_ms(start), 6.0, Unit::Y).time() > stop
    {
        let mut from: Option<JsDate> = None;
        let mut to: Option<JsDate> = None;
        let flush = |from: &mut Option<JsDate>, to: &mut Option<JsDate>, excluded: &mut String| {
            if let (Some(f), Some(t)) = (*from, *to) {
                let mut day_start = f;
                day_start.set_hours(0, 0, 0, 0);
                let mut day_end = t;
                day_end.set_hours(23, 59, 59, 999);
                let ex0 = left.max(x(day_start.time()));
                let ex1 = right.min(x(day_end.time()));
                if ex1 > ex0 {
                    excluded.push_str(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                        num(ex0),
                        num(plot_top),
                        num(ex1 - ex0),
                        num(plot_height)
                    ));
                }
            }
            *from = None;
            *to = None;
        };
        let mut day = JsDate::from_ms(start);
        while day.time() <= stop {
            if model.is_invalid_date(day)? {
                if from.is_none() {
                    from = Some(day);
                }
                to = Some(day);
            } else {
                flush(&mut from, &mut to, &mut excluded);
            }
            day = add_days(day, 1);
        }
        flush(&mut from, &mut to, &mut excluded);
    }

    let mut links: Vec<LinkInfo> = Vec::new();
    let mut tasks_out = String::new();
    for bar in &bars {
        let task = &tasks[bar.task_idx];
        let cy = plot_top + (bar.row as f64) * row_height + row_height / 2.0;
        let state = if task.active {
            "active"
        } else if task.done {
            "done"
        } else {
            ""
        };
        let mut classes = String::from("pele-task");
        if !state.is_empty() {
            classes.push_str(" pele-");
            classes.push_str(state);
        }
        if task.crit {
            classes.push_str(" pele-crit");
        }
        if task.milestone {
            classes.push_str(" pele-milestone");
        }
        if task.classes.iter().any(|c| c == "clickable") {
            classes.push_str(" pele-clickable");
        }

        let mut shape = String::new();
        let mut text_fill = "";
        if bar.x0.is_nan() {
            classes.push_str(" pele-unscheduled");
            text_fill = " fill=\"var(--_m)\"";
        } else {
            let color = if task.crit { CRITICAL } else { "var(--_c)" };
            let paint = if state == "done" {
                "fill=\"var(--_a)\"".to_string()
            } else if state == "active" {
                format!("fill=\"{}\" fill-opacity=\"0.25\"", color)
            } else {
                format!("fill=\"{}\"", color)
            };
            let stroke_color = if state == "done" && !task.crit {
                "var(--_b)"
            } else {
                color
            };
            let stroke = format!("stroke=\"{}\"", stroke_color);
            if task.milestone {
                let cx = (bar.x0 + bar.x1) / 2.0;
                let r = bar_height / 2.0;
                shape = format!(
                    "<path d=\"M{},{}L{},{}L{},{}L{},{}Z\" {} {} stroke-linejoin=\"round\"/>",
                    num(cx),
                    num(cy - r),
                    num(cx + r),
                    num(cy),
                    num(cx),
                    num(cy + r),
                    num(cx - r),
                    num(cy),
                    paint,
                    stroke
                );
            } else {
                shape = format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {} {}{}/>",
                    num(bar.x0),
                    num(cy - bar_height / 2.0),
                    num(bar.x1 - bar.x0),
                    num(bar_height),
                    paint,
                    stroke,
                    ROUNDED
                );
            }
            if bar.inside {
                if state.is_empty() {
                    text_fill = " fill=\"var(--_bg)\"";
                }
            } else if state == "done" {
                text_fill = " fill=\"var(--_m)\"";
            }
        }

        let mut body = format!(
            "{}{}",
            shape,
            label_svg(&bar.label, bar.label_x, cy, &format!(" class=\"pele-label\"{}", text_fill))
        );
        if let Some(url) = model.links.get(&task.id) {
            body = linked(body, url, &task.id, &mut links, options, "");
        }
        tasks_out.push_str(&format!(
            "<g class=\"{}\" data-id=\"{}\">{}</g>",
            classes,
            esc_text(&task.id),
            body
        ));
    }

    let mut today = String::new();
    if model.today_marker != "off" && now >= start && now <= stop && stop > start {
        let tx = x(now);
        let top = plot_top - grid_lead;
        let line_style = if !model.today_marker.is_empty() {
            let decls: Vec<&str> = model.today_marker.split(',').collect();
            resolve_style(&decls).line
        } else {
            String::new()
        };
        let cap = if line_style.is_empty() {
            format!(
                "<path d=\"M{},{}H{}L{},{}Z\" fill=\"var(--_c)\"/>",
                num(tx - 4.0),
                num(top - 5.0),
                num(tx + 4.0),
                num(tx),
                num(top + 1.0)
            )
        } else {
            String::new()
        };
        today = format!(
            "<g class=\"pele-today\"><path d=\"M{},{}V{}\" fill=\"none\" stroke=\"var(--_c)\" stroke-width=\"2\"{}/>{}</g>",
            num(tx),
            num(top),
            num(plot_bottom),
            line_style,
            cap
        );
    }

    let mut axis_line = format!("M{},{}H{}", num(left), num(plot_bottom), num(right));
    if top_axis {
        axis_line.push_str(&format!("M{},{}H{}", num(left), num(plot_top - grid_lead), num(right)));
    }

    let mut content = label_svg(
        &title,
        width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"var(--_tw)\"",
    );
    if !bands.is_empty() {
        content.push_str(&format!("<g class=\"pele-sections\">{}</g>", bands));
    }
    if !excluded.is_empty() {
        content.push_str(&format!("<g class=\"pele-excluded\" fill=\"var(--_a)\">{}</g>", excluded));
    }
    content.push_str(&format!(
        "<g class=\"pele-grid\" fill=\"none\"><path d=\"{}\" stroke=\"var(--_b)\" stroke-dasharray=\"1 3\"/><path d=\"{}\" stroke=\"var(--_b)\"/></g>",
        grid, axis_line
    ));
    if !vert_lines.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-verts\" fill=\"none\" stroke=\"var(--_l)\" stroke-width=\"1.5\" stroke-dasharray=\"4 3\">{}</g>",
            vert_lines
        ));
    }
    content.push_str(&today);
    content.push_str(&format!(
        "<g class=\"pele-tasks\" font-size=\"{}\">{}</g>",
        num(label_size),
        tasks_out
    ));
    content.push_str(&format!(
        "<g class=\"pele-section-labels\" font-size=\"{}\">{}</g>",
        num(section_size),
        section_out
    ));
    content.push_str(&format!(
        "<g class=\"pele-axis\" font-size=\"{}\" fill=\"var(--_m)\">{}{}</g>",
        js_number(tick_size),
        axis_bottom,
        axis_top
    ));
    if !vert_labels.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-vert-labels\" font-size=\"{}\">{}</g>",
            num(label_size),
            vert_labels
        ));
    }

    let acc = Accessible {
        acc_title: model.acc_title.as_deref(),
        acc_descr: model.acc_descr.as_deref(),
    };
    let svg = svg_document("gantt", width, height, size, &doc, &acc, &content);
    Ok(Rendered { svg, width, height, links })
}
