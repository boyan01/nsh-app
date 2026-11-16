//! Localized system-message copy shared by messages and notifications.
use crate::i18n::I18n;
use crate::i18n::Key;

pub(crate) fn actor(i18n: I18n, id: &str, name: &str, current_user_id: &str) -> String {
    if !id.is_empty() && current_user_id == id {
        i18n.t(Key::You).into()
    } else if name.is_empty() {
        i18n.t(Key::NshAGroupMember).into()
    } else {
        name.into()
    }
}

pub(crate) fn text(
    i18n: I18n,
    sender: &str,
    participant: &str,
    action: Option<&str>,
    content: &str,
) -> String {
    let key = match action {
        Some("JOIN") => Key::NshParticipantJoinedTheGroup,
        Some("EXIT") => Key::NshParticipantLeftTheGroup,
        Some("ADD") => Key::NshSenderAddedParticipant,
        Some("REMOVE") => Key::NshSenderRemovedParticipant,
        Some("CREATE") => Key::NshSenderCreatedThisGroup,
        Some("ROLE") => Key::NshParticipantBecameAnAdmin,
        Some("EXPIRE") => {
            return match content.parse::<i64>() {
                Ok(seconds) if seconds > 0 => {
                    let duration = expire_duration(i18n, seconds);
                    i18n.format(
                        Key::NshSenderSetDisappearingMessagesToDuration,
                        &[("sender", sender), ("duration", &duration)],
                    )
                }
                Ok(_) => i18n.format(
                    Key::NshSenderDisabledDisappearingMessages,
                    &[("sender", sender)],
                ),
                Err(_) => i18n.format(
                    Key::NshSenderChangedDisappearingMessageSettings,
                    &[("sender", sender)],
                ),
            };
        }
        _ => {
            return i18n.t(Key::NshThisMessageTypeIsNotSupported).into();
        }
    };
    i18n.format(key, &[("sender", sender), ("participant", participant)])
}

pub(crate) fn expire_duration(i18n: I18n, seconds: i64) -> String {
    let (count, key) = if seconds < 60 {
        (seconds, Key::NshCountSeconds)
    } else if seconds < 3600 {
        (seconds / 60, Key::NshCountMinutes)
    } else if seconds < 86400 {
        (seconds / 3600, Key::NshCountHours)
    } else if seconds < 604800 {
        (seconds / 86400, Key::NshCountDays)
    } else {
        (seconds / 604800, Key::NshCountWeeks)
    };
    let key = if count == 1 {
        match key {
            Key::NshCountSeconds => Key::NshCountSecond,
            Key::NshCountMinutes => Key::NshCountMinute,
            Key::NshCountHours => Key::NshCountHour,
            Key::NshCountDays => Key::NshCountDay,
            _ => Key::NshCountWeek,
        }
    } else {
        key
    };
    i18n.format(key, &[("count", &count.to_string())])
}
