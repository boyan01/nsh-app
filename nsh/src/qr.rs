//! Local QR rendering shared by invitations, links and text messages.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{DynamicImage, GrayImage, ImageFormat, Luma};
use qrcode::{EcLevel, QrCode};
use std::io::Cursor;

pub(crate) fn source(raw: &str, minimum_size: u32) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let code = QrCode::new(raw).ok()?;
    let bitmap = code
        .render::<Luma<u8>>()
        .min_dimensions(minimum_size, minimum_size)
        .quiet_zone(true)
        .build();
    png_source(bitmap)
}

/// The terminal draws one module per half-block pixel. Keep the source and
/// destination dimensions equal so the renderer does not blur the modules.
pub(crate) fn terminal_source(raw: &str) -> Option<(String, u32)> {
    let code = QrCode::with_error_correction_level(raw, EcLevel::L).ok()?;
    let modules = code
        .render::<Luma<u8>>()
        .module_dimensions(1, 1)
        .quiet_zone(true)
        .build();
    let side: u32 = modules.width().next_multiple_of(2);
    let mut padded = GrayImage::from_pixel(side, side, Luma([255]));
    image::imageops::replace(&mut padded, &modules, 0, 0);
    Some((png_source(padded)?, side))
}

fn png_source(bitmap: GrayImage) -> Option<String> {
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(bitmap)
        .write_to(&mut png, ImageFormat::Png)
        .ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.into_inner())
    ))
}
