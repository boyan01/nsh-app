//! Avatars and the frames they come in.

use mixin_desktop_api::GroupAvatar;
use nsh_base::{AvatarElement, avatar};
use nsh_core::{element::ImageSrc, prelude::*};
use unicode_segmentation::UnicodeSegmentation;

use super::tokens::{
    TextRole, Typography, accent, avatar_empty, fg_on_accent, fg_primary, fg_secondary, number,
};

/// Flutter's `avatarColors`.
const AVATAR_COLORS: [u32; 30] = [
    0xFFD659, 0xFFC168, 0xF58268, 0xF4979C, 0xEC7F87, 0xFF78CB, 0xC377E0, 0x8BAAFF, 0x78DCFA,
    0x88E5B9, 0xBFF199, 0xC5E1A5, 0xCD907D, 0xBE938E, 0xB68F91, 0xBC987B, 0xA69E8E, 0xD4C99E,
    0x93C2E6, 0x92C3D9, 0x8FBFC5, 0x80CBC4, 0xA4DBDB, 0xB2C8BD, 0xF7C8C9, 0xDCC6E4, 0xBABAE8,
    0xBABCD5, 0xAD98DA, 0xC097D9,
];

/// Flutter's `nameColors`.
const NAME_COLORS: [u32; 50] = [
    0x8C8DFF, 0x7983C2, 0x6D8DDE, 0x5979F0, 0x6695DF, 0x8F7AC5, 0x9D77A5, 0x8A64D0, 0xAA66C3,
    0xA75C96, 0xC8697D, 0xB74D62, 0xBD637C, 0xB3798E, 0x9B6D77, 0xB87F7F, 0xC5595A, 0xAA4848,
    0xB0665E, 0xB76753, 0xBB5334, 0xC97B46, 0xBE6C2C, 0xCB7F40, 0xA47758, 0xB69370, 0xA49373,
    0xAA8A46, 0xAA8220, 0x76A048, 0x9CAD23, 0xA19431, 0xAA9100, 0xA09555, 0xC49B4B, 0x5FB05F,
    0x6AB48F, 0x71B15C, 0xB3B357, 0xA3B561, 0x909F45, 0x93B289, 0x3D98D0, 0x429AB6, 0x4EABAA,
    0x6BC0CE, 0x64B5D9, 0x3E9CCB, 0x2887C4, 0x52A98B,
];

/// Flutter's `circleColors`.
const CIRCLE_COLORS: [u32; 16] = [
    0x8E7BFF, 0x657CFB, 0xA739C2, 0xBD6DDA, 0xFD89F1, 0xFA7B95, 0xE94156, 0xFA9652, 0xF1D22B,
    0xBAE361, 0x5EDD5E, 0x4BE6FF, 0x45B7FE, 0x00ECD0, 0xFFCCC0, 0xCEA06B,
];

/// Flutter's `uuidHashcode`, which is Java's `UUID.hashCode`. An id that is not a
/// UUID has none.
fn uuid_hash(id: &str) -> Option<i64> {
    let parts = id
        .trim()
        .split('-')
        .map(|part| i64::from_str_radix(part, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    let [a, b, c, d, e] = parts[..] else {
        return None;
    };
    let hilo = ((a << 32) | (b << 16) | c) ^ ((d << 48) | e);
    Some((hilo >> 32) ^ i64::from(hilo as i32))
}

fn pick(id: &str, palette: &[u32]) -> Option<u32> {
    let hash = uuid_hash(id)?.unsigned_abs() as usize;
    Some(palette[hash % palette.len()])
}

fn palette_color(id: &str, palette: &[u32], other: fn() -> AdaptiveColor) -> AdaptiveColor {
    pick(id, palette).map_or_else(other, |color| rgb(color).into())
}

/// A circle's color. Circles have none of their own, so the id picks a stable one.
pub(crate) fn circle_color(id: &str) -> AdaptiveColor {
    palette_color(id, &CIRCLE_COLORS, fg_secondary)
}

/// The color of a sender's name in the timeline.
pub(crate) fn name_color(user_id: &str) -> AdaptiveColor {
    palette_color(user_id, &NAME_COLORS, accent)
}

/// What stands for a picture: the first grapheme of the name, as it is, so a name
/// that starts with a space shows none.
fn placeholder_letter(name: &str) -> String {
    name.graphemes(true)
        .next()
        .unwrap_or_default()
        .to_uppercase()
}

#[derive(Clone, Copy)]
pub(crate) enum AvatarSize {
    /// A rail or menu identity.
    Small,
    /// A dense row: a mention, a message sender.
    Compact,
    /// The identity in a title bar.
    Bar,
    /// A row of a dialog or list.
    Medium,
    /// A conversation list cell.
    Large,
    /// The person a dialog is about.
    Hero,
    /// The identity at the head of a page.
    Profile,
}

impl AvatarSize {
    fn points(self) -> Points {
        px(number(match self {
            Self::Small => "avatar-sm",
            Self::Compact => "avatar-compact",
            Self::Bar => "avatar-bar",
            Self::Medium => "avatar-md",
            Self::Large => "avatar-lg",
            Self::Hero => "avatar-hero",
            Self::Profile => "avatar-profile",
        }))
    }
}

/// `id` picks the placeholder's color: the user's, or the owner's for a conversation.
pub(crate) fn avatar_view(
    id: impl Fn() -> String + 'static,
    name: impl Fn() -> SharedString + 'static,
    source: impl Fn() -> String + 'static,
    size: AvatarSize,
) -> AvatarElement {
    let name = computed(name);
    let id = computed(id);
    let color = computed(move || palette_color(&id.get(), &AVATAR_COLORS, avatar_empty));
    avatar()
        .name(move || Some(name.get()))
        .src(move || {
            let url = source();
            (!url.is_empty()).then(|| ImageSrc::from(url))
        })
        .size(size.points())
        .flex_shrink_0()
        .rounded_full()
        .overflow_hidden()
        .image_class("rounded-full object-cover")
        .fallback(
            text(move || placeholder_letter(&name.get()))
                .text_role(TextRole::Headline)
                .text_color(move || {
                    if uuid_hash(&id.get()).is_some() {
                        fg_on_accent()
                    } else {
                        fg_primary()
                    }
                }),
        )
        .fallback_style(move |s| s.items_center().justify_center().bg(color))
}

/// Use the same one-to-four participant puzzle as the Flutter conversation list.
pub(crate) fn conversation_avatar(
    row: impl Fn() -> Option<mixin_desktop_api::ConversationListData> + 'static,
    size: AvatarSize,
) -> AnyElement {
    // A new message changes the row, not its picture; keep the loaded images.
    let identity = computed(move || {
        row().map(|row| {
            // A group photo that an owner or admin uploaded replaces the puzzle.
            let photo = ["https://", "http://"]
                .iter()
                .any(|scheme| row.avatar_url.starts_with(scheme));
            let puzzle = row.category == "GROUP" && !photo && !row.group_avatars.is_empty();
            let avatars = puzzle.then(|| row.group_avatars.into_iter().take(4).collect::<Vec<_>>());
            (row.owner_id, row.name, row.avatar_url, avatars)
        })
    });
    // A recycled cell changes identity, not structure. Keep the avatar's native
    // views and bind their data; only a different puzzle shape needs rebuilding.
    let shape = computed(move || {
        identity.with(|identity| {
            identity
                .as_ref()
                .map(|(_, _, _, avatars)| avatars.as_ref().map_or(0, Vec::len))
        })
    });
    let frame = move || {
        view()
            .size(size.points())
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden()
    };
    dynamic(move || {
        let shape = shape.get();
        untracked(|| match shape {
            None => frame().into_element(),
            Some(0) => avatar_view(
                move || identity.with(|v| v.as_ref().map(|v| v.0.clone()).unwrap_or_default()),
                move || {
                    identity.with(|v| v.as_ref().map(|v| v.1.clone().into()).unwrap_or_default())
                },
                move || identity.with(|v| v.as_ref().map(|v| v.2.clone()).unwrap_or_default()),
                size,
            )
            .into_element(),
            Some(count) => puzzle_tiles(frame(), count, move |at| {
                identity.with(|identity| {
                    identity
                        .as_ref()
                        .and_then(|v| v.3.as_ref())
                        .and_then(|avatars| avatars.get(at))
                        .cloned()
                })
            })
            .into_element(),
        })
    })
}

/// One to four tiles filling their parent, which clips them.
pub(crate) fn avatar_puzzle(avatars: Vec<GroupAvatar>) -> View {
    puzzle_in(view().flex_1(), avatars)
}

/// The tiles in `frame`, laid out as Flutter's puzzle.
fn puzzle_in(frame: View, avatars: Vec<GroupAvatar>) -> View {
    let count = avatars.len();
    let avatars = stored(avatars);
    puzzle_tiles(frame, count, move |at| {
        avatars.with(|avatars| avatars.get(at).cloned())
    })
}

fn puzzle_tiles(
    frame: View,
    count: usize,
    person: impl Fn(usize) -> Option<GroupAvatar> + Copy + 'static,
) -> View {
    let tile = |at: usize| {
        let person = computed(move || person(at));
        avatar_view(
            move || person.with(|v| v.as_ref().map(|v| v.user_id.clone()).unwrap_or_default()),
            move || {
                person.with(|v| {
                    v.as_ref()
                        .map(|v| v.name.clone().into())
                        .unwrap_or_default()
                })
            },
            move || person.with(|v| v.as_ref().map(|v| v.avatar_url.clone()).unwrap_or_default()),
            AvatarSize::Small,
        )
        .size_full()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .rounded_none()
        .image_class("object-cover")
    };
    let column = |first: usize| {
        view()
            .flex_1()
            .min_w_0()
            .child(tile(first))
            .child(tile(first + 1))
    };
    match count {
        1 => frame.child(tile(0)),
        2 => frame.flex_row().child(tile(0)).child(tile(1)),
        3 => frame.flex_row().child(tile(0)).child(column(1)),
        _ => frame.flex_row().child(column(0)).child(column(2)),
    }
}

#[cfg(test)]
mod tests {
    use super::{AVATAR_COLORS, CIRCLE_COLORS, NAME_COLORS, pick, uuid_hash};

    #[test]
    fn a_uuid_picks_the_hash_and_colors_flutter_picks() {
        // The ids and hashes are mixin_bot_sdk_dart's `uuidHashcode` test vectors.
        let cases = [
            (
                "ea91421a-98bb-41d2-abcf-af013d8b874b",
                -462541950,
                0xFFD659,
                0x8C8DFF,
                0xFFCCC0,
            ),
            (
                "0364f490-49cc-4988-88c2-481707687e5b",
                -989689004,
                0xB68F91,
                0x6695DF,
                0x45B7FE,
            ),
            (
                " 8df4972f-702f-4ae9-bc76-68e489351357 ",
                -929520011,
                0xC5E1A5,
                0xB74D62,
                0x4BE6FF,
            ),
        ];
        for (id, hash, avatar, name, circle) in cases {
            assert_eq!(uuid_hash(id), Some(hash));
            assert_eq!(pick(id, &AVATAR_COLORS), Some(avatar));
            assert_eq!(pick(id, &NAME_COLORS), Some(name));
            assert_eq!(pick(id, &CIRCLE_COLORS), Some(circle));
        }
    }
}
