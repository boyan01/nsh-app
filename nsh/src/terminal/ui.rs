//! Whole-cell controls and text layout for the terminal.

use nsh_base::theme;
use nsh_core::prelude::*;
use unicode_segmentation::UnicodeSegmentation as _;
use unicode_width::UnicodeWidthStr as _;

pub(crate) const COLUMN: f32 = 8.0;
pub(super) const ROW: f32 = 16.0;
pub(super) const LIST_COLUMNS: usize = 30;

pub(crate) fn columns() -> usize {
    (window().width.0 / COLUMN) as usize
}

pub(crate) fn rows() -> usize {
    (window().height.0 / ROW) as usize
}

pub(super) fn wide() -> bool {
    columns() >= 100
}

pub(super) fn chat_columns() -> usize {
    columns()
        .saturating_sub(if wide() { LIST_COLUMNS + 4 } else { 2 })
        .max(1)
}

pub(crate) fn line(value: impl IntoReactive<SharedString>) -> Text {
    text(value).h(px(ROW)).flex_shrink_0().overflow_hidden()
}

pub(crate) fn hint(value: impl IntoReactive<SharedString>) -> Text {
    line(value).text_color(|| theme().muted_foreground)
}

pub(crate) fn status(value: impl IntoReactive<SharedString>) -> View {
    view().flex_1().child(text(value))
}

pub(crate) fn editor(
    placeholder: &'static str,
    draft: Signal<String>,
    submit: impl Fn(&str) + 'static,
) -> Input {
    nsh_core::element::input()
        .h(px(ROW))
        .flex_shrink_0()
        .w_full()
        .placeholder(placeholder)
        .label(placeholder)
        .value(move || draft.get())
        .on_change(move |value: &str| draft.set(value.to_owned()))
        .on_submit(submit)
        .submit_behavior(SubmitBehavior::Submit)
        .auto_focus(true)
}

// Controls in remote names or messages have no presentation meaning. Leave
// line breaks to the caller and remove the rest before measuring the text.
pub(super) fn single_line(value: &str, width: usize) -> String {
    let value: String = value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if value.width() <= width {
        return value;
    }
    let mut used = 0;
    let limit = width.saturating_sub(1);
    let mut result = String::new();
    for cluster in value.graphemes(true) {
        used += cluster.width();
        if used > limit {
            break;
        }
        result.push_str(cluster);
    }
    if width > 0 {
        result.push('…');
    }
    result
}

pub(super) fn wrap(value: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in value.split('\n') {
        let mut line = String::new();
        let mut used = 0;
        for cluster in paragraph.graphemes(true) {
            let cluster = match cluster {
                "\r" => continue,
                "\t" => "    ",
                cluster if cluster.chars().any(char::is_control) => continue,
                cluster if cluster.width() > width => "�",
                cluster => cluster,
            };
            let cells = cluster.width();
            if used > 0 && used + cells > width {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push_str(cluster);
            used += cells;
        }
        lines.push(line);
    }
    lines
}
