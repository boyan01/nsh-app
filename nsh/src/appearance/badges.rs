//! Shared identity and conversation-status assets from the reference client.
use std::sync::LazyLock;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mixin_desktop_api::active_membership_plan;
use nsh_core::{
    TransformKind,
    animation::Easing,
    element::{AdaptiveColor, AnimatedValue, ImageSrc, Timing},
    prelude::*,
};

use crate::{
    i18n::{self, Key, tr},
    ui,
};

/// The displayed identity, independent of the recycled conversation DTO.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityBadgeKind {
    Group,
    Basic,
    Standard,
    Premium,
    Verified,
    Bot,
}

impl IdentityBadgeKind {
    pub(crate) fn from_identity(
        group: bool,
        verified: bool,
        bot: bool,
        membership: Option<&str>,
    ) -> Option<Self> {
        if group {
            return Some(Self::Group);
        }
        if let Some(plan) = active_membership_plan(membership) {
            return match plan {
                "BASIC" => Some(Self::Basic),
                "STANDARD" => Some(Self::Standard),
                "PREMIUM" => Some(Self::Premium),
                _ => None,
            };
        }
        if verified {
            Some(Self::Verified)
        } else if bot {
            Some(Self::Bot)
        } else {
            None
        }
    }

    fn raster(self) -> bool {
        matches!(self, Self::Basic | Self::Standard | Self::Premium)
    }

    fn source(self) -> ImageSrc {
        let markup = match self {
            Self::Group => include_str!("../../assets/badges/group_badge.svg"),
            Self::Verified => include_str!("../../assets/badges/verified.svg"),
            Self::Bot => include_str!("../../assets/badges/bot_fill.svg"),
            Self::Basic => {
                static SOURCE: LazyLock<SharedString> = LazyLock::new(|| {
                    format!(
                        "data:image/png;base64,{}",
                        STANDARD.encode(include_bytes!("../../assets/badges/plan_basic.png"))
                    )
                    .into()
                });
                return SOURCE.clone().into();
            }
            Self::Standard => {
                static SOURCE: LazyLock<SharedString> = LazyLock::new(|| {
                    format!(
                        "data:image/png;base64,{}",
                        STANDARD.encode(include_bytes!("../../assets/badges/plan_standard.png"))
                    )
                    .into()
                });
                return SOURCE.clone().into();
            }
            Self::Premium => {
                static SOURCE: LazyLock<SharedString> = LazyLock::new(|| {
                    format!(
                        "data:image/gif;base64,{}",
                        STANDARD.encode(include_bytes!("../../assets/badges/plan_premium.gif"))
                    )
                    .into()
                });
                return SOURCE.clone().into();
            }
        };
        ImageSrc::svg_markup(markup)
    }

    fn label(self, i18n: i18n::I18n) -> &'static str {
        match self {
            Self::Group => i18n.tr("Group", "群组"),
            Self::Basic => i18n.tr("Basic member", "基础会员"),
            Self::Standard => i18n.tr("Standard member", "标准会员"),
            Self::Premium => i18n.tr("Premium member", "高级会员"),
            Self::Verified => i18n.tr("Verified", "已认证"),
            Self::Bot => i18n.tr("Bot", "机器人"),
        }
    }

    fn size(self) -> Points {
        // The existing size-3 and size-3.5 identity variants.
        px(if matches!(self, Self::Verified | Self::Bot) {
            12.0
        } else {
            14.0
        })
    }
}

pub(crate) fn identity_badge(
    group: bool,
    verified: bool,
    bot: bool,
    membership: Option<&str>,
) -> AnyElement {
    let Some(badge) = IdentityBadgeKind::from_identity(group, verified, bot, membership) else {
        return view().hidden().into_element();
    };
    let label = badge.label(i18n::current());
    if badge.raster() {
        image(badge.source())
            .size_3p5()
            .object_fit(ResizeMode::Contain)
            .label(label)
            .into_element()
    } else {
        svg(badge.source())
            .size(badge.size())
            .label(label)
            .when(badge == IdentityBadgeKind::Group, |s| {
                s.text_color(ui::fg_secondary)
            })
            .into_element()
    }
}

/// Keeps one native glyph or image while a cell's identity changes.
pub(crate) fn identity_badge_reactive(badge: Memo<Option<IdentityBadgeKind>>) -> AnyElement {
    let i18n = i18n::current();
    let shape = computed(move || badge.get().map(IdentityBadgeKind::raster));
    let source = move || {
        badge
            .get()
            .map(IdentityBadgeKind::source)
            .unwrap_or_else(|| ImageSrc::from(""))
    };
    let label = move || badge.get().map_or("", |badge| badge.label(i18n));
    dynamic(move || {
        let shape = shape.get();
        untracked(|| match shape {
            None => None,
            Some(true) => Some(
                image(source)
                    .size_3p5()
                    .object_fit(ResizeMode::Contain)
                    .label(label)
                    .into_element(),
            ),
            Some(false) => Some(
                svg(source)
                    .size(move || badge.get().map_or(Points::ZERO, IdentityBadgeKind::size))
                    .text_color(move || {
                        if badge.get() == Some(IdentityBadgeKind::Group) {
                            ui::fg_secondary()
                        } else {
                            ui::fg_primary()
                        }
                    })
                    .label(label)
                    .into_element(),
            ),
        })
    })
}

pub(crate) fn status_icon(pinned: bool) -> impl IntoElement {
    status_icon_reactive(move || pinned)
}

/// A single status slot can change between mute and pin without replacing it.
pub(crate) fn status_icon_reactive(pinned: impl Fn() -> bool + Copy + 'static) -> impl IntoElement {
    let i18n = i18n::current();
    svg(move || {
        ImageSrc::svg_markup(if pinned() {
            include_str!("../../assets/badges/pin.svg")
        } else {
            include_str!("../../assets/badges/mute.svg")
        })
    })
    .size_3p5()
    .label(move || {
        if pinned() {
            i18n.tr("Pinned", "已置顶")
        } else {
            i18n.tr("Muted", "已静音")
        }
    })
    .text_color(ui::fg_secondary)
}

/// Delivery icons preserve the reference geometry; the status remains accessible.
pub(crate) fn delivery_icon(status: &str) -> AnyElement {
    delivery(status, false)
}

pub(crate) fn delivery_icon_on_media(status: &str) -> AnyElement {
    delivery(status, true)
}

/// Over a picture the mark takes the media overlay's foreground.
fn quiet_foreground(on_media: bool) -> AdaptiveColor {
    if on_media {
        ui::media_overlay_foreground()
    } else {
        ui::message_secondary()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeliveryGlyph {
    Read,
    Delivered,
    Sent,
    Failed,
}

impl DeliveryGlyph {
    fn from_status(status: &str) -> Option<Self> {
        match status {
            "READ" => Some(Self::Read),
            "DELIVERED" => Some(Self::Delivered),
            "SENT" => Some(Self::Sent),
            "FAILED" => Some(Self::Failed),
            _ => None,
        }
    }

    fn source(self) -> ImageSrc {
        ImageSrc::svg_markup(match self {
            Self::Read => include_str!("../../assets/badges/read.svg"),
            Self::Delivered => include_str!("../../assets/badges/delivered.svg"),
            Self::Sent => include_str!("../../assets/badges/sent.svg"),
            Self::Failed => include_str!("../../assets/message/warning.svg"),
        })
    }

    fn label(self, i18n: i18n::I18n) -> &'static str {
        match self {
            Self::Read => i18n.text(Key::Read),
            Self::Delivered => i18n.tr("Delivered", "已送达"),
            Self::Sent => i18n.text(Key::Sent),
            Self::Failed => i18n.text(Key::Failed),
        }
    }

    fn height(self) -> Points {
        if self == Self::Failed {
            ui::icon_xs()
        } else {
            ui::message_status_icon()
        }
    }

    fn color(self, on_media: bool) -> AdaptiveColor {
        match self {
            Self::Read => ui::accent(),
            Self::Failed => ui::danger(),
            _ => quiet_foreground(on_media),
        }
    }
}

fn delivery(status: &str, on_media: bool) -> AnyElement {
    let Some(glyph) = DeliveryGlyph::from_status(status) else {
        return sending_clock(on_media).into_element();
    };
    let i18n = i18n::current();
    svg(glyph.source())
        .w(ui::icon_xs())
        .h(glyph.height())
        .flex_shrink_0()
        .label(move || glyph.label(i18n))
        .text_color(move || glyph.color(on_media))
        .into_element()
}

/// Reuses the glyph for status changes; a sending branch owns and stops its clock.
pub(crate) fn delivery_icon_reactive(status: Memo<Option<String>>) -> AnyElement {
    let i18n = i18n::current();
    let glyph = computed(move || {
        status.with(|status| status.as_deref().and_then(DeliveryGlyph::from_status))
    });
    let shape =
        computed(move || status.with(|status| status.as_ref().map(|_| glyph.get().is_some())));
    dynamic(move || {
        let shape = shape.get();
        untracked(|| match shape {
            None => None,
            Some(false) => Some(sending_clock(false).into_element()),
            Some(true) => Some(
                svg(move || {
                    glyph
                        .get()
                        .map(DeliveryGlyph::source)
                        .unwrap_or_else(|| ImageSrc::from(""))
                })
                .w(ui::icon_xs())
                .h(move || glyph.get().map_or(Points::ZERO, DeliveryGlyph::height))
                .flex_shrink_0()
                .label(move || glyph.get().map_or("", |glyph| glyph.label(i18n)))
                .text_color(move || {
                    glyph
                        .get()
                        .map_or_else(ui::fg_secondary, |glyph| glyph.color(false))
                })
                .into_element(),
            ),
        })
    })
}

/// The clock of a message on its way. Its hands turn as in the reference client: the
/// minute hand once in 600 ms, the hour hand once in 2.4 s.
fn sending_clock(on_media: bool) -> View {
    let hand = |markup: &'static str, turn: &AnimatedValue| {
        svg(ImageSrc::svg_markup(markup))
            .absolute()
            .inset_0()
            .text_color(move || quiet_foreground(on_media))
            .bind_transform(TransformKind::Rotate, turn)
    };
    let minute = animated_value(0.0);
    let hour = animated_value(0.0);
    if !reduce_motion() {
        let spin = |turn: &AnimatedValue, ms: f32| {
            Animation::timing(turn, 360.0, Timing::new(ms, Easing::Linear))
                .looped(None)
                .start()
        };
        // Both runs end only when they are stopped with the row.
        drop(spin(&minute, 600.0));
        drop(spin(&hour, 2400.0));
        cleanup(move || {
            minute.stop();
            hour.stop();
        });
    }
    view()
        .size(ui::icon_xs())
        .flex_shrink_0()
        .relative()
        .child(
            svg(ImageSrc::svg_markup(include_str!(
                "../../assets/message/sending_frame.svg"
            )))
            .absolute()
            .inset_0()
            .label(tr("Sending", "发送中"))
            .text_color(move || quiet_foreground(on_media)),
        )
        .child(hand(
            include_str!("../../assets/message/sending_hour.svg"),
            &hour,
        ))
        .child(hand(
            include_str!("../../assets/message/sending_minute.svg"),
            &minute,
        ))
}
