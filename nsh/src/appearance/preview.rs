//! One retained SVG slot for category changes in recycled subtitle rows.
use nsh_core::{element::ImageSrc, prelude::*};

use crate::{presentation::PreviewIcon, ui};

pub(crate) fn preview_icon_reactive(icon: Memo<Option<PreviewIcon>>) -> impl IntoElement {
    svg(move || {
        let markup = match icon.get() {
            Some(PreviewIcon::Transfer) => include_str!("../../assets/quote/transfer.svg"),
            Some(PreviewIcon::Sticker) => include_str!("../../assets/ic_sticker.svg"),
            Some(PreviewIcon::Image) => include_str!("../../assets/quote/image.svg"),
            Some(PreviewIcon::Video) => include_str!("../../assets/quote/video.svg"),
            Some(PreviewIcon::Live) => include_str!("../../assets/quote/live.svg"),
            Some(PreviewIcon::File) => include_str!("../../assets/quote/file.svg"),
            Some(PreviewIcon::Location) => include_str!("../../assets/quote/location.svg"),
            Some(PreviewIcon::Audio) => include_str!("../../assets/quote/audio.svg"),
            Some(PreviewIcon::App) => include_str!("../../assets/quote/app_button.svg"),
            Some(PreviewIcon::Contact) => include_str!("../../assets/quote/contact.svg"),
            Some(PreviewIcon::Call) => include_str!("../../assets/quote/video_call.svg"),
            Some(PreviewIcon::Recall) => include_str!("../../assets/quote/recall.svg"),
            None => return ImageSrc::from(""),
        };
        ImageSrc::svg_markup(markup)
    })
    .size_3p5()
    .flex_shrink_0()
    .text_color(ui::fg_secondary)
    .style_when(move || icon.get().is_none(), |style| style.hidden())
}
