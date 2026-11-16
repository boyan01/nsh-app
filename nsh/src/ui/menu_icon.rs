//! The glyphs beside menu items: Flutter's `IconFonts` glyphs and its
//! `context_menu_*` images as SVG, drawn in the menu's text color.
use nsh_core::element::ImageSrc;

#[derive(Clone, Copy)]
pub(crate) enum MenuIcon {
    Chat,
    Circle,
    CircleOutline,
    Copy,
    CreateConversation,
    CreateGroup,
    Delete,
    Download,
    Edit,
    Forward,
    ManageCircle,
    ManageUser,
    Link,
    Mute,
    Pin,
    PositionToChat,
    QrCode,
    Recall,
    Reply,
    SearchUser,
    Select,
    Sticker,
    Stop,
    UnMute,
    UnPin,
}

impl MenuIcon {
    pub(crate) fn src(self) -> ImageSrc {
        ImageSrc::svg_markup(match self {
            Self::Chat => crate::assets::svg::MENU_CHAT,
            Self::Circle => crate::assets::svg::MENU_CIRCLE,
            Self::CircleOutline => crate::assets::svg::MENU_CIRCLE_OUTLINE,
            Self::Copy => crate::assets::svg::MENU_COPY,
            Self::CreateConversation => crate::assets::svg::MENU_CREATE_CONVERSATION,
            Self::CreateGroup => crate::assets::svg::MENU_CREATE_GROUP,
            Self::Delete => crate::assets::svg::MENU_DELETE,
            Self::Download => crate::assets::svg::MENU_DOWNLOAD,
            Self::Edit => crate::assets::svg::MENU_EDIT,
            Self::Forward => crate::assets::svg::MENU_FORWARD,
            Self::ManageCircle => crate::assets::svg::MENU_MANAGE_CIRCLE,
            Self::ManageUser => crate::assets::svg::MENU_MANAGE_USER,
            Self::Link => crate::assets::svg::MENU_LINK,
            Self::Mute => crate::assets::svg::MENU_MUTE,
            Self::Pin => crate::assets::svg::MENU_PIN,
            Self::PositionToChat => crate::assets::svg::MENU_POSITION_TO_CHAT,
            Self::QrCode => crate::assets::svg::MENU_QR_CODE,
            Self::Recall => crate::assets::svg::MENU_RECALL,
            Self::Reply => crate::assets::svg::MENU_REPLY,
            Self::SearchUser => crate::assets::svg::MENU_SEARCH_USER,
            Self::Select => crate::assets::svg::MENU_SELECT,
            Self::Sticker => crate::assets::svg::MENU_STICKER,
            Self::Stop => crate::assets::svg::MENU_STOP,
            Self::UnMute => crate::assets::svg::MENU_UN_MUTE,
            Self::UnPin => crate::assets::svg::MENU_UN_PIN,
        })
    }
}
