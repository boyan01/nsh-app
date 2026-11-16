//! The app's one icon family, and its raster brand assets.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use nsh_core::{
    element::{Image, ImageSrc},
    prelude::*,
};

use super::tokens::icon_md;

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Chat,
    Search,
    ChevronDown,
    ChevronRight,
    ChevronLeft,
    Rotate,
    ZoomIn,
    ZoomOut,
    Copy,
    ArrowLeft,
    Send,
    Globe,
    Lock,
    Check,
    Clock,
    Pin,
    Contacts,
    Groups,
    Bot,
    Strangers,
    Circle,
    Collapse,
    Expand,
    Filter,
    Add,
    Sticker,
    StickerStore,
    StickerFavorite,
    Microphone,
    Original,
    Archive,
    Attachment,
    File,
    Image,
    Post,
    Stop,
    Pause,
    Volume,
    VolumeOff,
    FullScreen,
    ExitFullScreen,
    Menu,
    More,
    Play,
    Download,
    Upload,
    Close,
    CloseCircle,
    Person,
    Bell,
    Database,
    Shield,
    Brush,
    InfoCircle,
    /// Chat backup and restore.
    Backup,
    /// A conversation whose messages disappear.
    Disappearing,
    /// Share: a box with an arrow leaving it.
    Share,
    Forward,
    /// Two speech bubbles: open the conversation.
    Conversation,
    /// A person: open the full profile.
    Profile,
}

pub(crate) fn icon(icon: Icon) -> Svg {
    let asset = |markup: &str| {
        svg(ImageSrc::svg_markup(markup))
            .size(icon_md())
            .flex_shrink_0()
    };
    match icon {
        Icon::Chat => asset(crate::assets::svg::CHAT),
        Icon::Contacts => asset(crate::assets::svg::SLIDE_CONTACTS),
        Icon::Groups => asset(crate::assets::svg::GROUP),
        Icon::Bot => asset(crate::assets::svg::BOT),
        Icon::Strangers => asset(crate::assets::svg::STRANGERS),
        Icon::Circle => asset(crate::assets::svg::CIRCLE),
        Icon::Collapse => asset(crate::assets::svg::COLLAPSE),
        Icon::Expand => asset(crate::assets::svg::EXPANDED),
        Icon::Filter => asset(crate::assets::svg::FILTER_UNSEEN),
        Icon::Add => asset(crate::assets::svg::IC_ADD),
        Icon::Search => asset(crate::assets::svg::IC_SEARCH_SMALL),
        Icon::Sticker => asset(crate::assets::svg::IC_STICKER),
        Icon::StickerStore => asset(crate::assets::svg::STICKER_STORE),
        Icon::StickerFavorite => asset(crate::assets::svg::PERSONAL_STICKER),
        Icon::Original => asset(crate::assets::svg::ICONS_ORIGINAL),
        Icon::Archive => asset(crate::assets::svg::ICONS_ARCHIVE),
        Icon::Attachment => asset(crate::assets::svg::ICONS_ATTACHMENT),
        Icon::File => asset(crate::assets::svg::ICONS_FILE),
        Icon::Image => asset(crate::assets::svg::ICONS_IMAGE),
        Icon::Post => asset(crate::assets::svg::ICONS_POST),
        Icon::Stop => asset(crate::assets::svg::ICONS_STOP),
        Icon::Volume => asset(crate::assets::svg::ICONS_VOLUME),
        Icon::VolumeOff => asset(crate::assets::svg::ICONS_VOLUME_OFF),
        Icon::FullScreen => asset(crate::assets::svg::ICONS_FULL_SCREEN),
        Icon::ExitFullScreen => asset(crate::assets::svg::ICONS_EXIT_FULL_SCREEN),
        Icon::Pause => asset(crate::assets::svg::ICONS_PAUSE),
        Icon::Microphone => asset(crate::assets::svg::MICROPHONE),
        Icon::Forward => asset(crate::assets::svg::MENU_FORWARD),
        Icon::Share => asset(crate::assets::svg::PROFILE_SHARE),
        Icon::Conversation => asset(crate::assets::svg::PROFILE_CHAT),
        Icon::Profile => asset(crate::assets::svg::PROFILE_INFO),
        Icon::ChevronDown => asset(crate::assets::svg::ICONS_CHEVRON_DOWN),
        Icon::ChevronRight => asset(crate::assets::svg::ICONS_CHEVRON_RIGHT),
        Icon::ChevronLeft => asset(crate::assets::svg::ICONS_CHEVRON_LEFT),
        Icon::Rotate => asset(crate::assets::svg::ICONS_ROTATE),
        Icon::ZoomIn => asset(crate::assets::svg::ICONS_ZOOM_IN),
        Icon::ZoomOut => asset(crate::assets::svg::ICONS_ZOOM_OUT),
        Icon::Copy => asset(crate::assets::svg::ICONS_COPY),
        Icon::ArrowLeft => asset(crate::assets::svg::IC_BACK),
        Icon::More => asset(crate::assets::svg::ICONS_MORE),
        Icon::Menu => asset(crate::assets::svg::ICONS_MENU),
        Icon::Play => asset(crate::assets::svg::ICONS_PLAY),
        Icon::Download => asset(crate::assets::svg::ICONS_DOWNLOAD),
        Icon::Upload => asset(crate::assets::svg::ICONS_UPLOAD),
        Icon::Disappearing => asset(crate::assets::svg::DISAPPEARING_MESSAGE_INDICATOR),
        Icon::Backup => asset(crate::assets::svg::IC_BACKUP),
        Icon::CloseCircle => asset(crate::assets::svg::ICONS_CLOSE_CIRCLE),
        Icon::Close => asset(crate::assets::svg::IC_CLOSE),
        // Center the plane's visual mass, which otherwise reads high and right.
        // The inset keeps the shifted strokes inside the icon's view box.
        Icon::Send => asset(crate::assets::svg::ICONS_SEND),
        Icon::Globe => asset(crate::assets::svg::PROXY),
        Icon::Lock => asset(crate::assets::svg::ICONS_LOCK),
        Icon::Clock => asset(crate::assets::svg::ICONS_CLOCK),

        Icon::Check => asset(crate::assets::svg::ICONS_CHECK),
        Icon::Pin => asset(crate::assets::svg::ICONS_PIN),
        Icon::Person => asset(crate::assets::svg::IC_PROFILE),
        Icon::Bell => asset(crate::assets::svg::IC_NOTIFICATION),
        Icon::Database => asset(crate::assets::svg::IC_STORAGE_USAGE),
        Icon::Shield => asset(crate::assets::svg::SHIELD),
        Icon::Brush => asset(crate::assets::svg::IC_APPEARANCE),
        Icon::InfoCircle => asset(crate::assets::svg::IC_ABOUT),
    }
}

pub(crate) fn logo() -> Image {
    static SOURCE: std::sync::LazyLock<SharedString> = std::sync::LazyLock::new(|| {
        format!(
            "data:image/png;base64,{}",
            STANDARD.encode(include_bytes!("../../assets/logo.png"))
        )
        .into()
    });
    image(SOURCE.clone())
        .size_10()
        .flex_shrink_0()
        .label("Mixin")
}

pub(crate) fn wallpaper() -> Image {
    // Flutter chat_page.dart uses srcIn: preserve the bitmap alpha and replace
    // its RGB with black (light) or white (dark). Opacity is applied by the caller.
    static SOURCES: std::sync::LazyLock<[SharedString; 2]> = std::sync::LazyLock::new(|| {
        [0, 255].map(|tint| {
            let mut bitmap =
                ::image::load_from_memory(include_bytes!("../../assets/chat_background.png"))
                    .expect("Bundled chat wallpaper is valid")
                    .to_rgba8();
            for pixel in bitmap.pixels_mut() {
                pixel.0[..3].fill(tint);
            }
            use ::image::{
                ImageEncoder,
                codecs::png::{CompressionType, FilterType, PngEncoder},
            };
            let mut png = Vec::new();
            PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::NoFilter)
                .write_image(
                    bitmap.as_raw(),
                    bitmap.width(),
                    bitmap.height(),
                    ::image::ExtendedColorType::Rgba8,
                )
                .expect("Wallpaper encodes as PNG");
            format!("data:image/png;base64,{}", STANDARD.encode(png)).into()
        })
    });
    image(move || SOURCES[usize::from(dark())].clone())
}

pub(crate) fn inscription_background() -> Image {
    static SOURCE: std::sync::LazyLock<SharedString> = std::sync::LazyLock::new(|| {
        format!(
            "data:image/png;base64,{}",
            STANDARD.encode(include_bytes!("../../assets/inscription/text_bg.png"))
        )
        .into()
    });
    image(SOURCE.clone())
}
