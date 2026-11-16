//! One retained SVG slot for category changes in recycled subtitle rows.
use nsh_core::{element::ImageSrc, prelude::*};

use crate::{presentation::PreviewIcon, ui};

pub(crate) fn preview_icon_reactive(icon: Memo<Option<PreviewIcon>>) -> impl IntoElement {
    svg(move || {
        let markup = match icon.get() {
            Some(PreviewIcon::Transfer) => crate::assets::svg::QUOTE_TRANSFER,
            Some(PreviewIcon::Sticker) => crate::assets::svg::IC_STICKER,
            Some(PreviewIcon::Image) => crate::assets::svg::QUOTE_IMAGE,
            Some(PreviewIcon::Video) => crate::assets::svg::QUOTE_VIDEO,
            Some(PreviewIcon::Live) => crate::assets::svg::QUOTE_LIVE,
            Some(PreviewIcon::File) => crate::assets::svg::QUOTE_FILE,
            Some(PreviewIcon::Location) => crate::assets::svg::QUOTE_LOCATION,
            Some(PreviewIcon::Audio) => crate::assets::svg::QUOTE_AUDIO,
            Some(PreviewIcon::App) => crate::assets::svg::QUOTE_APP_BUTTON,
            Some(PreviewIcon::Contact) => crate::assets::svg::QUOTE_CONTACT,
            Some(PreviewIcon::Call) => crate::assets::svg::QUOTE_VIDEO_CALL,
            Some(PreviewIcon::Recall) => crate::assets::svg::QUOTE_RECALL,
            None => super::EMPTY_SVG,
        };
        ImageSrc::svg_markup(markup)
    })
    .size_3p5()
    .flex_shrink_0()
    .text_color(ui::fg_secondary)
    .style_when(move || icon.get().is_none(), |style| style.hidden())
}
