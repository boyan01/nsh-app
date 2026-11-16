mod datetime;
pub(crate) mod message_window;
mod preview;
pub(crate) mod system_message;
pub(crate) use preview::{PreviewIcon, active_draft, preview_icon, shows_delivery};

use crate::i18n::{I18n, Key};
use chrono::{DateTime, Local};
use mixin_desktop_api::{ConversationListData, MessageListView};

pub(crate) fn conversation_preview(
    i18n: I18n,
    row: &ConversationListData,
    current_user_id: &str,
) -> String {
    let sender_id = row.last_message_sender_id.as_deref().unwrap_or_default();
    conversation_preview_with_sender(
        i18n,
        row,
        current_user_id,
        row.category == "GROUP" || (!sender_id.is_empty() && sender_id != row.owner_id),
    )
}

pub(crate) fn search_conversation_preview(
    i18n: I18n,
    row: &ConversationListData,
    current_user_id: &str,
) -> String {
    conversation_preview_with_sender(i18n, row, current_user_id, row.category == "GROUP")
}

fn conversation_preview_with_sender(
    i18n: I18n,
    row: &ConversationListData,
    current_user_id: &str,
    include_sender: bool,
) -> String {
    let Some(category) = row.last_message_category.as_deref() else {
        return String::new();
    };
    let sender_id = row.last_message_sender_id.as_deref().unwrap_or_default();
    let sender = system_message::actor(
        i18n,
        sender_id,
        row.last_message_sender_name.as_deref().unwrap_or_default(),
        current_user_id,
    );
    if category == "SYSTEM_CONVERSATION" {
        let participant = system_message::actor(
            i18n,
            row.last_message_participant_id
                .as_deref()
                .unwrap_or_default(),
            row.last_message_participant_name
                .as_deref()
                .unwrap_or_default(),
            current_user_id,
        );
        return system_message::text(
            i18n,
            &sender,
            &participant,
            row.last_message_action.as_deref(),
            &row.last_message,
        );
    }
    if category == "MESSAGE_PIN" {
        return pinned_preview(
            i18n,
            row.last_message_sender_name.as_deref().unwrap_or_default(),
            &row.last_message,
        );
    }
    let body = message_preview(
        i18n,
        category,
        &row.last_message,
        row.last_message_status.as_deref(),
        !sender_id.is_empty() && sender_id == current_user_id,
    );
    if !body.is_empty() && include_sender {
        format!("{sender}: {body}")
    } else {
        body
    }
}

fn pinned_preview(i18n: I18n, sender: &str, content: &str) -> String {
    let message = preview::pin_message(|en, zh| i18n.tr(en, zh), content)
        .unwrap_or_else(|| i18n.tr("a message", "一条消息").into());
    i18n.format(
        "{sender} pinned {message}",
        "{sender}置顶了{message}",
        &[("sender", sender), ("message", &message)],
    )
}

pub(crate) fn search_message_preview(
    i18n: I18n,
    row: &MessageListView,
    current_user_id: &str,
) -> String {
    let sender = system_message::actor(i18n, &row.sender_id, &row.sender_name, current_user_id);
    if row.category == "SYSTEM_CONVERSATION" {
        let participant = system_message::actor(
            i18n,
            row.participant_id.as_deref().unwrap_or_default(),
            row.participant_full_name.as_deref().unwrap_or_default(),
            current_user_id,
        );
        return system_message::text(
            i18n,
            &sender,
            &participant,
            row.action.as_deref(),
            &row.content,
        );
    }
    if row.category == "MESSAGE_PIN" {
        return pinned_preview(i18n, &row.sender_name, &row.content);
    }
    let body = message_preview(
        i18n,
        &row.category,
        &row.content,
        Some(&row.status),
        row.sender_id == current_user_id,
    );
    if !body.is_empty()
        && (row.conversation_category.as_deref() == Some("GROUP")
            || row
                .conversation_owner_id
                .as_deref()
                .is_some_and(|id| id != row.sender_id))
    {
        format!("{sender}: {body}")
    } else {
        body
    }
}

pub(crate) fn preview(i18n: I18n, category: &str, content: &str) -> String {
    if category.ends_with("_TEXT") {
        return content.to_owned();
    }
    let key = if category.ends_with("_IMAGE") {
        Key::Photo
    } else if category.ends_with("_VIDEO") {
        Key::Video
    } else if category.ends_with("_AUDIO") {
        Key::Audio
    } else if category.ends_with("_DATA") {
        Key::File
    } else if category.ends_with("_STICKER") {
        Key::Sticker
    } else if category.ends_with("_CONTACT") {
        Key::Contact
    } else if category.ends_with("_LOCATION") {
        Key::Location
    } else if category.ends_with("_POST") {
        Key::Post
    } else if category.ends_with("_TRANSCRIPT") {
        Key::Transcript
    } else if category.starts_with("SYSTEM_ACCOUNT_SNAPSHOT") {
        Key::Transfer
    } else {
        Key::Message
    };
    i18n.text(key).to_owned()
}

pub(crate) fn message_preview(
    i18n: I18n,
    category: &str,
    content: &str,
    status: Option<&str>,
    current_user: bool,
) -> String {
    preview::message(
        |en, zh| i18n.tr(en, zh),
        category,
        content,
        status,
        current_user,
    )
}

pub(crate) fn time(i18n: I18n, millis: i64) -> String {
    let Some(date) = DateTime::from_timestamp_millis(millis).filter(|_| millis > 0) else {
        return String::new();
    };
    let date = date.with_timezone(&Local);
    let today = Local::now().date_naive();
    if date.date_naive() == today {
        date.format("%H:%M").to_string()
    } else {
        datetime::short(date.date_naive(), today, i18n.locale_tag())
    }
}

pub(crate) fn date(i18n: I18n, millis: i64) -> String {
    DateTime::from_timestamp_millis(millis)
        .filter(|_| millis > 0)
        .map(|date| datetime::full(date.with_timezone(&Local).date_naive(), i18n.locale_tag()))
        .unwrap_or_default()
}

/// A message's time of day: its row already sits under its day.
pub(crate) fn clock(millis: i64) -> String {
    DateTime::from_timestamp_millis(millis)
        .filter(|_| millis > 0)
        .map(|date| date.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

pub(crate) fn day(i18n: I18n, micros: i64) -> String {
    let Some(date) = DateTime::from_timestamp_micros(micros) else {
        return String::new();
    };
    let date = date.with_timezone(&Local).date_naive();
    let today = Local::now().date_naive();
    if date == today {
        i18n.text(Key::Today).into()
    } else if today.signed_duration_since(date).num_days() == 1 {
        i18n.text(Key::Yesterday).into()
    } else {
        datetime::day(date, today, i18n.locale_tag())
    }
}
