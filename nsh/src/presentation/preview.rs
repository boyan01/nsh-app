//! Pure subtitle rules shared by conversation and search presentation.
use nsh_base::text_view::Parser;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewIcon {
    Transfer,
    Sticker,
    Image,
    Video,
    Live,
    File,
    Location,
    Audio,
    App,
    Contact,
    Call,
    Recall,
}

pub(crate) fn active_draft(status: i32, draft: &str) -> &str {
    if status == 3 { "" } else { draft }
}

fn call(category: &str) -> bool {
    matches!(
        category,
        "WEBRTC_AUDIO_CANCEL"
            | "WEBRTC_AUDIO_DECLINE"
            | "WEBRTC_AUDIO_END"
            | "WEBRTC_AUDIO_BUSY"
            | "WEBRTC_AUDIO_FAILED"
    )
}

fn group_call(category: &str) -> bool {
    matches!(
        category,
        "KRAKEN_END" | "KRAKEN_DECLINE" | "KRAKEN_CANCEL" | "KRAKEN_INVITE"
    )
}

// Match Flutter's allowlist, including its exclusion of call/group-call events.
// The separate icon mapper still recognizes one-to-one call outcomes.
fn legal_category(category: &str) -> bool {
    if let Some((prefix, kind)) = category.split_once('_')
        && matches!(prefix, "PLAIN" | "SIGNAL" | "ENCRYPTED")
    {
        return matches!(category, "SIGNAL_KEY" | "PLAIN_JSON")
            || matches!(
                kind,
                "TEXT"
                    | "IMAGE"
                    | "VIDEO"
                    | "LIVE"
                    | "DATA"
                    | "POST"
                    | "LOCATION"
                    | "AUDIO"
                    | "STICKER"
                    | "CONTACT"
                    | "TRANSCRIPT"
            );
    }
    matches!(
        category,
        "SIGNAL_KEY"
            | "PLAIN_JSON"
            | "STRANGER"
            | "SECRET"
            | "SYSTEM_CONVERSATION"
            | "SYSTEM_USER"
            | "SYSTEM_CIRCLE"
            | "SYSTEM_SESSION"
            | "MESSAGE_PIN"
    ) || matches!(
        category,
        "SYSTEM_ACCOUNT_SNAPSHOT"
            | "SYSTEM_SAFE_SNAPSHOT"
            | "SYSTEM_SAFE_INSCRIPTION"
            | "APP_BUTTON_GROUP"
            | "APP_CARD"
            | "MESSAGE_RECALL"
    )
}

pub(crate) fn shows_delivery(
    sender: Option<&str>,
    category: Option<&str>,
    account_id: &str,
    drafted: bool,
) -> bool {
    !drafted
        && !account_id.is_empty()
        && sender == Some(account_id)
        && !category.is_some_and(|category| {
            matches!(
                category,
                "SYSTEM_CONVERSATION"
                    | "SYSTEM_ACCOUNT_SNAPSHOT"
                    | "MESSAGE_RECALL"
                    | "MESSAGE_PIN"
            ) || call(category)
                || group_call(category)
        })
}

pub(crate) fn preview_icon(
    status: Option<&str>,
    category: &str,
    drafted: bool,
) -> Option<PreviewIcon> {
    if drafted || status == Some("FAILED") || category.ends_with("_TEXT") {
        return None;
    }
    Some(match category {
        "SYSTEM_ACCOUNT_SNAPSHOT" | "SYSTEM_SAFE_SNAPSHOT" | "SYSTEM_SAFE_INSCRIPTION" => {
            PreviewIcon::Transfer
        }
        "APP_BUTTON_GROUP" | "APP_CARD" => PreviewIcon::App,
        "MESSAGE_RECALL" => PreviewIcon::Recall,
        _ if call(category) => PreviewIcon::Call,
        _ if category.ends_with("_STICKER") => PreviewIcon::Sticker,
        _ if category.ends_with("_IMAGE") => PreviewIcon::Image,
        _ if category.ends_with("_VIDEO") => PreviewIcon::Video,
        _ if category.ends_with("_LIVE") => PreviewIcon::Live,
        _ if category.ends_with("_LOCATION") => PreviewIcon::Location,
        _ if category.ends_with("_AUDIO") => PreviewIcon::Audio,
        _ if category.ends_with("_CONTACT") => PreviewIcon::Contact,
        _ if ["_DATA", "_POST", "_TRANSCRIPT"]
            .iter()
            .any(|suffix| category.ends_with(suffix)) =>
        {
            PreviewIcon::File
        }
        _ => return None,
    })
}

pub(super) fn message(
    tr: impl Fn(&'static str, &'static str) -> &'static str + Copy,
    category: &str,
    content: &str,
    status: Option<&str>,
    current_user: bool,
) -> String {
    let unsupported = tr(
        "This type of message is not supported, please upgrade Mixin to the latest version.",
        "不支持此类消息，请升级 Mixin 到最新版本。",
    );
    if !legal_category(category) {
        return unsupported.into();
    }
    if status == Some("FAILED") {
        return tr("Waiting for this message.", "正在等待这条消息。").into();
    }
    if status == Some("UNKNOWN") {
        return unsupported.into();
    }
    let content = content.trim();
    if category.ends_with("_TEXT") {
        return content.into();
    }
    if category.ends_with("_POST") {
        let source = content
            .lines()
            .take(10)
            .collect::<Vec<_>>()
            .join("\r\n")
            .chars()
            .take(1024)
            .collect::<String>();
        return Parser::new()
            .markdown(&source)
            .plain_text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
    }
    if category == "APP_BUTTON_GROUP" {
        return serde_json::from_str::<serde_json::Value>(content)
            .ok()
            .and_then(|value| {
                value.as_array().and_then(|buttons| {
                    buttons
                        .iter()
                        .map(|button| {
                            // Flutter's ActionData requires all three strings.
                            button["color"].as_str()?;
                            button["action"].as_str()?;
                            Some(format!("[{}]", button["label"].as_str()?))
                        })
                        .collect::<Option<String>>()
                })
            })
            .unwrap_or_default();
    }
    if category == "APP_CARD" {
        let title = serde_json::from_str::<serde_json::Value>(content)
            .ok()
            .and_then(|value| {
                // Both strings are required by Flutter's AppCardData contract.
                value["description"].as_str()?;
                for field in ["app_id", "icon_url", "action", "updated_at", "cover_url"] {
                    if value
                        .get(field)
                        .is_some_and(|value| !value.is_null() && !value.is_string())
                    {
                        return None;
                    }
                }
                if value
                    .get("shareable")
                    .is_some_and(|value| !value.is_null() && !value.is_boolean())
                {
                    return None;
                }
                if let Some(actions) = value.get("actions").filter(|value| !value.is_null()) {
                    for action in actions.as_array()? {
                        for field in ["label", "color", "action"] {
                            action[field].as_str()?;
                        }
                    }
                }
                if let Some(cover) = value.get("cover").filter(|value| !value.is_null()) {
                    cover["url"].as_str()?;
                    cover["mime_type"].as_str()?;
                    cover["width"].as_number()?;
                    cover["height"].as_number()?;
                    if cover
                        .get("thumbnail")
                        .is_some_and(|value| !value.is_null() && !value.is_string())
                    {
                        return None;
                    }
                }
                value["title"].as_str().map(str::to_owned)
            });
        return format!(
            "[{}]",
            title.as_deref().unwrap_or_else(|| tr("Card", "卡片"))
        );
    }
    let label = match category {
        "SYSTEM_ACCOUNT_SNAPSHOT" | "SYSTEM_SAFE_SNAPSHOT" => tr("Transfer", "转账"),
        "SYSTEM_SAFE_INSCRIPTION" => tr("Collectible", "藏品"),
        "MESSAGE_RECALL" if current_user => tr("You deleted this message", "你撤回了一条消息"),
        "MESSAGE_RECALL" => tr("This message was deleted", "此消息已撤回"),
        _ if category.ends_with("_STICKER") => tr("Sticker", "贴纸"),
        _ if category.ends_with("_IMAGE") => tr("Image", "图像"),
        _ if category.ends_with("_VIDEO") => tr("Video", "视频"),
        _ if category.ends_with("_LIVE") => tr("Live", "直播"),
        _ if category.ends_with("_DATA") => tr("File", "文件"),
        _ if category.ends_with("_LOCATION") => tr("Location", "位置"),
        _ if category.ends_with("_AUDIO") => tr("Audio", "语音"),
        _ if category.ends_with("_CONTACT") => tr("Contact", "联系人"),
        _ if category.ends_with("_TRANSCRIPT") => tr("Transcript", "聊天记录"),
        _ => return unsupported.into(),
    };
    format!("[{label}]")
}

pub(super) fn pin_message(
    tr: impl Fn(&'static str, &'static str) -> &'static str + Copy,
    content: &str,
) -> Option<String> {
    let pin = serde_json::from_str::<serde_json::Value>(content).ok()?;
    pin["message_id"].as_str()?;
    let category = pin["category"].as_str()?;
    let content = match pin.get("content") {
        None | Some(serde_json::Value::Null) => "",
        Some(value) => value.as_str()?,
    };
    Some(if category.ends_with("_TEXT") {
        content.into()
    } else {
        // Flutter's pin helper requests a group preview without a sender name.
        let body = message(tr, category, content, None, false);
        if body.is_empty() {
            body
        } else {
            format!(": {body}")
        }
    })
}

#[cfg(test)]
mod tests;
