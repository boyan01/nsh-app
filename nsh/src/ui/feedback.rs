//! Quiet content: counts, headings, dividers, and error, busy and empty states.

use nsh_core::{element::ImageSrc, prelude::*};

use super::{
    icon::logo,
    tokens::{
        TextRole, Typography, accent, badge_height, badge_min_width, badge_quiet, fg_on_accent,
        fg_primary, fg_secondary, hairline, hairline_width, icon_sm, space_md, space_sm, space_xl,
        space_xs, state_selected,
    },
};

/// The count pill of list rows and the navigation rail. Quiet when not emphasized.
pub(crate) fn count_pill(
    label: impl Fn() -> String + 'static,
    emphasized: impl Fn() -> bool + Copy + 'static,
) -> View {
    view()
        .min_w(badge_min_width())
        .h(badge_height())
        .px(space_xs())
        .flex_shrink_0()
        .rounded_full()
        .items_center()
        .justify_center()
        .bg(move || {
            if emphasized() {
                accent()
            } else {
                badge_quiet()
            }
        })
        .child(
            text(label)
                .text_role(TextRole::Caption)
                .tabular_nums()
                .text_color(move || {
                    if emphasized() {
                        fg_on_accent()
                    } else {
                        fg_primary()
                    }
                }),
        )
}

/// The heading of a group, lined up with the labels of its rows.
pub(crate) fn section_label(title: impl Fn() -> &'static str + 'static) -> Text {
    text(title)
        .px(space_md())
        .text_role(TextRole::Body)
        .text_color(fg_secondary)
        .role(AccessibilityRole::Header)
}

/// A headed group of cards.
pub(crate) fn section(title: impl Fn() -> &'static str + 'static) -> View {
    view().gap(space_md()).child(section_label(title))
}

/// A boundary between groups in one pane.
pub(crate) fn divider() -> View {
    view()
        .h(hairline_width())
        .mx(space_sm())
        .my(space_sm())
        .bg(hairline)
}

/// A recoverable failure, stated quietly next to what it affects.
pub(crate) fn error_text(message: impl Fn() -> &'static str + 'static) -> Text {
    text(message)
        .text_role(TextRole::Body)
        .text_color(fg_secondary)
}

/// A quiet message that fills an empty pane.
pub(crate) fn placeholder(message: impl Fn() -> &'static str + 'static) -> View {
    view()
        .flex_1()
        .min_h_0()
        .p(space_xl())
        .items_center()
        .justify_center()
        .child(
            text(message)
                .text_role(TextRole::Body)
                .text_center()
                .text_color(fg_secondary),
        )
}

/// Mount only while actual work is pending. Never manufacture a percentage.
pub(crate) fn busy_indicator(
    busy: impl Fn() -> bool + 'static,
    label: impl Fn() -> SharedString + 'static,
) -> impl IntoElement {
    let label = stored(label);
    when(busy, move || spinner(move || label.with(|label| label())))
}

fn spinner(label: impl Fn() -> SharedString + 'static) -> Svg {
    svg(ImageSrc::svg_markup(crate::assets::svg::ICONS_SPINNER))
        .size(icon_sm())
        .flex_shrink_0()
        .role(AccessibilityRole::Progressbar)
        .label(label)
        .style_with(|s| {
            if reduce_motion() {
                s.animate_none()
            } else {
                s.animate_spin()
            }
        })
}

/// Flutter Landing's initialization content inside the shared landing card.
pub(crate) fn landing_loading(
    title: impl Fn() -> &'static str + Copy + 'static,
    message: impl Fn() -> &'static str + 'static,
) -> View {
    view()
        .w(super::loading_content_width())
        .max_w(relative(1.0))
        .items_center()
        .gap(space_xl())
        .text_color(fg_primary)
        .child(spinner(move || title().into()).size_10())
        .child(
            view()
                .w_full()
                .items_center()
                .gap(space_sm())
                .child(text(title).text_role(TextRole::Headline).text_center())
                .child(
                    text(message)
                        .text_role(TextRole::Label)
                        .text_center()
                        .text_color(super::fg_landing_hint),
                ),
        )
}

/// A state with nothing to show yet: the mark, what is empty and what to do next.
pub(crate) fn empty(
    title: impl IntoReactive<SharedString>,
    body: impl IntoReactive<SharedString>,
) -> View {
    view()
        .flex_1()
        .min_h_0()
        .p_8()
        .gap(space_md())
        .items_center()
        .justify_center()
        .child(
            view()
                .size_16()
                .rounded_2xl()
                .bg(state_selected)
                .items_center()
                .justify_center()
                .text_color(fg_secondary)
                .child(logo().size_12().opacity(0.65)),
        )
        .child(text(title).text_role(TextRole::Headline).text_center())
        .child(
            text(body)
                .max_w_80()
                .text_role(TextRole::Body)
                .text_center()
                .text_color(fg_secondary),
        )
}
