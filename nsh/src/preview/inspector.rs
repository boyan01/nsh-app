//! The production floating inspector over chat chrome and fixed message rows.
use nsh_core::prelude::*;

use super::{Actions, fixtures, messages};
use crate::{chat, inspector, navigation::Panel};

pub(super) const ACTIONS: &[&str] = &[
    "Open info",
    "Open search",
    "Open pinned",
    "Close",
    "Jump from search",
    "Jump from pinned",
    "Return to inspector",
    "Dismiss return",
    "Fold pin",
    "Expand pin",
    "Remove pin",
    "Replace pin",
    "Long pin",
];

pub(super) fn scene(actions: Actions) -> View {
    messages::provide_services();
    let navigation = use_context::<crate::navigation::ChatNavigation>().unwrap();
    let panel = navigation.panel;
    on_key_with(
        "escape",
        KeyOptions {
            when: nsh_core::keymap::When::NotEditing,
            ..KeyOptions::default()
        },
        move |_| navigation.back_panel(),
    );
    for (action, destination) in [
        ("Open info", Panel::Info),
        ("Open search", Panel::Search),
        ("Open pinned", Panel::Pinned),
    ] {
        actions.on(
            action,
            move || navigation.dismiss_inspector(),
            move || panel.set(Some(destination)),
        );
    }
    actions.on(
        "Close",
        move || panel.set(Some(Panel::Info)),
        move || navigation.dismiss_inspector(),
    );
    for (action, mode) in [
        ("Jump from search", Panel::Search),
        ("Jump from pinned", Panel::Pinned),
    ] {
        actions.on(
            action,
            move || {
                navigation.dismiss_inspector();
                panel.set(Some(mode));
            },
            move || navigation.suspend_inspector(mode),
        );
    }
    for (action, resume) in [("Return to inspector", true), ("Dismiss return", false)] {
        actions.on(
            action,
            move || {
                navigation.dismiss_inspector();
                panel.set(Some(Panel::Search));
                navigation.suspend_inspector(Panel::Search);
            },
            move || {
                if resume {
                    navigation.resume_inspector();
                } else {
                    navigation.dismiss_inspector();
                }
            },
        );
    }
    let pin = signal(Some("Ava pinned \"Release at 5 pm\"".into()));
    let shown = signal(true);
    provide_context(chat::pinned::PreviewVisibility(shown));
    actions.on(
        "Fold pin",
        move || shown.set(true),
        move || shown.set(false),
    );
    actions.on(
        "Expand pin",
        move || shown.set(false),
        move || shown.set(true),
    );
    actions.on(
        "Remove pin",
        move || {
            shown.set(false);
            pin.set(Some("Ava pinned \"Release at 5 pm\"".into()));
        },
        move || pin.set(None),
    );
    actions.on(
        "Replace pin",
        move || {
            shown.set(false);
            pin.set(Some("Ava pinned \"Release at 5 pm\"".into()));
        },
        move || pin.set(Some("Carlos pinned \"Updated release plan\"".into())),
    );
    actions.on(
        "Long pin",
        move || shown.set(true),
        move || pin.set(Some("Carlos pinned the updated release plan: review the desktop header, check the narrow window layout, and confirm the final release schedule with everyone".into())),
    );
    let mut row = fixtures::conversations::stress_rows().remove(0);
    row.name = "Desktop design and release planning".into();
    let mut rows = fixtures::messages::text_rows();
    let text_count = rows.len();
    let newest = rows[0].created_at_micros;
    // Preserve text quote targets and give the other groups distinct IDs.
    for group in [
        fixtures::messages::photo_rows(),
        fixtures::messages::media_rows(),
        fixtures::messages::file_rows(),
        fixtures::messages::card_rows(),
        fixtures::messages::app_rows(),
    ] {
        rows.extend(group);
    }
    for (index, row) in rows.iter_mut().enumerate() {
        if index >= text_count {
            row.message_id = format!("00000000-0000-4000-8000-{:012}", index + 1000);
        }
        row.created_at_micros = newest - index as i64 * 60_000_000;
        row.pinned = true;
    }
    let pinned = stored(rows.clone());
    let mut list = chat::DetachedMessages::new(signal(rows));
    let locate = move |id: String| {
        if let Some(index) = list
            .messages
            .with(|rows| rows.iter().position(|row| row.message_id == id))
        {
            list.highlight.set(Some(id.clone()));
            chat::place_jump(
                list.history,
                list.reveal,
                index,
                chat::FOCUS,
                None,
                move || list.highlight.with(|target| target.as_ref() == Some(&id)),
            );
            spawn(chat::blink(list.highlight_opacity));
        }
        navigation.suspend_inspector(Panel::Pinned);
    };
    messages::timeline_view(chat::preview_chrome(
        row,
        pin,
        move |top, bottom| {
            list.insets = (top, bottom);
            chat::detached_message_list(
                fixtures::USER_ID,
                list,
                true,
                fixtures::messages::mention_names(),
            )
        },
        super::composer::editor_with_escape(false, actions, move || navigation.back_panel()),
    ))
    .child(inspector::overlay(navigation, move |destination| {
        let close = move || navigation.back_panel();
        match destination {
            Panel::Info => {
                let mut row = fixtures::conversations::stress_rows().remove(0);
                row.category = "GROUP".into();
                row.name = "设计评审".into();
                crate::conversation::preview_page(
                    "info",
                    fixtures::USER_ID.into(),
                    row,
                    fixtures::participants(),
                    fixtures::users(),
                    signal(false),
                    fixtures::messages::photo_rows(),
                    close,
                )
                .into_element()
            }
            Panel::Search => inspector::preview_search(
                search_rows(),
                fixtures::participants(),
                signal("Reset"),
                close,
            )
            .into_element(),
            Panel::Pinned => inspector::preview_pinned(
                pinned.get(),
                fixtures::USER_ID.into(),
                signal(false),
                close,
                locate,
                computed(|| true),
            )
            .into_element(),
        }
    }))
}

/// Search fixtures cover truncation, literal keyword emphasis and identity changes.
pub(super) fn search_rows() -> Vec<mixin_desktop_api::MessageListView> {
    fixtures::messages::text_rows().into_iter().enumerate().map(|(index, mut row)| {
        row.conversation_category = Some("GROUP".into());
        row.content = match index {
            0 => "Design review: 今天一起测试会话搜索".into(),
            1 => "A longer DESIGN result with emoji 👩‍💻 and 测试, followed by enough text to truncate in the narrow inspector".into(),
            _ => "确认 design 测试结果与发送者、日期和高亮显示".into(),
        };
        if index == 1 {
            row.sender_is_verified = true;
            row.sender_name = "A very long sender name for the desktop design review".into();
        }
        if index == 3 { row.sender_is_bot = true; }
        row
    }).collect()
}
