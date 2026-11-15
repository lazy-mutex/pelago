use super::model::InfoModel;
use crate::options::{RenderOptions, Rendered};
use crate::svg::builder::label_svg;
use crate::svg::root::{svg_document, Accessible, Document};
use crate::text::label::layout_label;
use crate::text::measurer::{style, MetricsMeasurer};
use crate::value::Value;

const PELE_VERSION: &str = "1.13.0";

pub fn render_info(model: &InfoModel, config: &Value, options: &RenderOptions) -> Rendered {
    let size = options.font_size.unwrap_or(16.0);
    let measurer = options.measurer.unwrap_or(&MetricsMeasurer);
    let pad = options.padding.unwrap_or(8.0);

    let title = layout_label(model.title.as_deref(), false, measurer, size, 4000.0, style::BOLD);
    let title_height = if title.height > 0.0 { title.height + 8.0 } else { 0.0 };
    let version_text = format!("Pele v{}", PELE_VERSION);
    let text = layout_label(Some(&version_text), false, measurer, size, 4000.0, 0);

    let width = (title.width.max(text.width) + 2.0 * pad).ceil();
    let height = (title_height + text.height + 2.0 * pad).ceil();
    let content = format!(
        "{}{}",
        label_svg(&title, width / 2.0, pad + title.height / 2.0, " class=\"pele-title\" font-weight=\"var(--_tw)\""),
        label_svg(&text, width / 2.0, pad + title_height + text.height / 2.0, " class=\"pele-version\" fill=\"var(--_m)\"")
    );

    let fixed = config.get("info").get("useMaxWidth").as_bool() == Some(false);
    let svg = svg_document(
        "info",
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
        &content,
    );

    Rendered { svg, width, height, links: Vec::new() }
}
