use super::*;

fn en(en: &'static str, _: &'static str) -> &'static str {
    en
}
fn zh(_: &'static str, zh: &'static str) -> &'static str {
    zh
}

#[test]
fn media_and_snapshot_categories_render_bracketed_localized_labels() {
    for (suffix, english, chinese, icon) in [
        ("IMAGE", "Image", "图像", PreviewIcon::Image),
        ("VIDEO", "Video", "视频", PreviewIcon::Video),
        ("AUDIO", "Audio", "语音", PreviewIcon::Audio),
        ("LIVE", "Live", "直播", PreviewIcon::Live),
        ("DATA", "File", "文件", PreviewIcon::File),
        ("STICKER", "Sticker", "贴纸", PreviewIcon::Sticker),
        ("LOCATION", "Location", "位置", PreviewIcon::Location),
        ("CONTACT", "Contact", "联系人", PreviewIcon::Contact),
        ("TRANSCRIPT", "Transcript", "聊天记录", PreviewIcon::File),
    ] {
        for prefix in ["PLAIN", "SIGNAL", "ENCRYPTED"] {
            let category = format!("{prefix}_{suffix}");
            assert_eq!(
                message(en, &category, "raw payload", None, false),
                format!("[{english}]")
            );
            assert_eq!(
                message(zh, &category, "raw payload", None, false),
                format!("[{chinese}]")
            );
            assert_eq!(preview_icon(None, &category, false), Some(icon));
        }
    }
    for (category, english, chinese) in [
        ("SYSTEM_ACCOUNT_SNAPSHOT", "Transfer", "转账"),
        ("SYSTEM_SAFE_SNAPSHOT", "Transfer", "转账"),
        ("SYSTEM_SAFE_INSCRIPTION", "Collectible", "藏品"),
    ] {
        assert_eq!(
            message(en, category, "", None, false),
            format!("[{english}]")
        );
        assert_eq!(
            message(zh, category, "", None, false),
            format!("[{chinese}]")
        );
        assert_eq!(
            preview_icon(None, category, false),
            Some(PreviewIcon::Transfer)
        );
    }
}

#[test]
fn text_trims_without_brackets_and_has_no_category_icon() {
    for category in ["PLAIN_TEXT", "SIGNAL_TEXT", "ENCRYPTED_TEXT"] {
        assert_eq!(
            message(en, category, "  hello @7001 \n", None, false),
            "hello @7001"
        );
        assert_eq!(message(en, category, " \n ", None, false), "");
        assert_eq!(preview_icon(None, category, false), None);
    }
}

#[test]
fn failed_unknown_and_illegal_categories_obey_reference_text_precedence() {
    for (category, status, expected) in [
        ("PLAIN_IMAGE", "FAILED", "Waiting for this message."),
        ("SIGNAL_KEY", "FAILED", "Waiting for this message."),
        ("PLAIN_JSON", "FAILED", "Waiting for this message."),
        (
            "PLAIN_IMAGE",
            "UNKNOWN",
            "This type of message is not supported, please upgrade Mixin to the latest version.",
        ),
        (
            "FUTURE_IMAGE",
            "FAILED",
            "This type of message is not supported, please upgrade Mixin to the latest version.",
        ),
    ] {
        assert_eq!(
            message(en, category, "secret", Some(status), false),
            expected
        );
        assert_eq!(
            preview_icon(Some(status), category, false),
            if status == "UNKNOWN" {
                Some(PreviewIcon::Image)
            } else {
                None
            }
        );
    }
    assert_eq!(
        message(zh, "SIGNAL_TEXT", "secret", Some("FAILED"), false),
        "正在等待这条消息。"
    );
    for category in [
        "",
        "UNKNOWN",
        "FUTURE_TEXT",
        "PLAIN_JSON",
        "SYSTEM_USER",
        "SYSTEM_CONVERSATION",
        "MESSAGE_PIN",
    ] {
        assert_eq!(
            message(zh, category, "secret", None, false),
            "不支持此类消息，请升级 Mixin 到最新版本。"
        );
        assert_eq!(preview_icon(None, category, false), None);
    }
}

#[test]
fn button_groups_parse_all_labels_and_reject_malformed_payloads_atomically() {
    let content = r##"[{"label":"Open","color":"#fff","action":"https://mixin.one"},{"label":"关闭","color":"","action":""}]"##;
    assert_eq!(
        message(en, "APP_BUTTON_GROUP", content, None, false),
        "[Open][关闭]"
    );
    for content in [
        "",
        "null",
        "{}",
        "[1]",
        r#"[{"label":"A"}]"#,
        r#"[{"label":"A","color":"","action":""},{"label":2}]"#,
    ] {
        assert_eq!(message(en, "APP_BUTTON_GROUP", content, None, false), "");
    }
    assert_eq!(message(en, "APP_BUTTON_GROUP", "[]", None, false), "");
    assert_eq!(
        preview_icon(None, "APP_BUTTON_GROUP", false),
        Some(PreviewIcon::App)
    );
}

#[test]
fn cards_render_titles_and_use_localized_fallback_for_invalid_titles() {
    for (content, expected) in [
        (r#"{"title":"Title","description":"body"}"#, "[Title]"),
        (r#"{"title":"","description":""}"#, "[]"),
    ] {
        assert_eq!(message(en, "APP_CARD", content, None, false), expected);
    }
    for content in [
        "",
        "null",
        "[]",
        "{",
        "{}",
        r#"{"title":1}"#,
        r#"{"title":"Title"}"#,
        r#"{"title":"Title","description":1}"#,
        r#"{"title":"Title","description":"","actions":{}}"#,
        r#"{"title":"Title","description":"","actions":[{"label":"Open"}]}"#,
        r#"{"title":"Title","description":"","cover":{}}"#,
        r#"{"title":"Title","description":"","cover":{"url":"url","mime_type":"image/png","width":"1","height":1}}"#,
        r#"{"title":"Title","description":"","shareable":"true"}"#,
        r#"{"title":"Title","description":"","icon_url":1}"#,
    ] {
        assert_eq!(message(en, "APP_CARD", content, None, false), "[Card]");
        assert_eq!(message(zh, "APP_CARD", content, None, false), "[卡片]");
    }
}

#[test]
fn recall_localizes_and_brackets_the_current_or_other_sender() {
    assert_eq!(
        message(en, "MESSAGE_RECALL", "", None, true),
        "[You deleted this message]"
    );
    assert_eq!(
        message(en, "MESSAGE_RECALL", "", None, false),
        "[This message was deleted]"
    );
    assert_eq!(
        message(zh, "MESSAGE_RECALL", "", None, true),
        "[你撤回了一条消息]"
    );
    assert_eq!(
        message(zh, "MESSAGE_RECALL", "", None, false),
        "[此消息已撤回]"
    );
    assert_eq!(
        preview_icon(None, "MESSAGE_RECALL", false),
        Some(PreviewIcon::Recall)
    );
}

#[test]
fn markdown_posts_flatten_markup_and_bound_the_source() {
    for category in ["PLAIN_POST", "SIGNAL_POST", "ENCRYPTED_POST"] {
        let result = message(
            en,
            category,
            "# Title\n\n**bold** and [link](https://example.org)",
            None,
            false,
        );
        assert!(result.contains("Title"));
        assert!(result.contains("bold"));
        assert!(result.contains("link"));
        assert!(!result.contains("https://"));
        assert!(!result.contains(['#', '*', '[', ']']));
        assert_eq!(message(en, category, "", None, false), "");
        assert_eq!(preview_icon(None, category, false), Some(PreviewIcon::File));
        assert_eq!(
            message(en, category, &"x".repeat(2048), None, false).len(),
            1024
        );
        assert!(
            !message(
                en,
                category,
                &format!("{}\nexcluded", ["line"; 10].join("\n")),
                None,
                false
            )
            .contains("excluded")
        );
    }
}

#[test]
fn pin_payloads_preview_nested_categories_and_preserve_text_mentions() {
    assert_eq!(
        pin_message(
            en,
            r#"{"message_id":"m","category":"PLAIN_TEXT","content":"\u00407001"}"#
        )
        .as_deref(),
        Some("@7001")
    );
    assert_eq!(
        pin_message(
            en,
            r#"{"message_id":"m","category":"PLAIN_TEXT","content":" @7001 hello "}"#
        )
        .as_deref(),
        Some(" @7001 hello ")
    );
    assert_eq!(
        pin_message(
            zh,
            r#"{"message_id":"m","category":"PLAIN_IMAGE","content":null}"#
        )
        .as_deref(),
        Some(": [图像]")
    );
    assert_eq!(
        pin_message(
            en,
            r#"{"message_id":"m","category":"APP_CARD","content":"{\"title\":\"Card title\",\"description\":\"\"}"}"#
        )
        .as_deref(),
        Some(": [Card title]")
    );
    for content in [
        "",
        "{}",
        "[]",
        r#"{"category":"PLAIN_TEXT"}"#,
        r#"{"message_id":"m","category":"PLAIN_TEXT","content":2}"#,
    ] {
        assert_eq!(pin_message(en, content), None);
    }
}

#[test]
fn draft_text_hides_category_and_delivery_icons_until_cleared_or_quit() {
    for (status, draft, expected) in [
        (0, "draft", "draft"),
        (0, " ", " "),
        (0, "", ""),
        (3, "draft", ""),
    ] {
        let active = active_draft(status, draft);
        assert_eq!(active, expected);
        assert_eq!(
            preview_icon(None, "PLAIN_IMAGE", !active.is_empty()).is_none(),
            !active.is_empty()
        );
        assert_eq!(
            shows_delivery(
                Some("self"),
                Some("PLAIN_IMAGE"),
                "self",
                !active.is_empty()
            ),
            active.is_empty()
        );
    }
}

#[test]
fn delivery_excludes_reference_system_call_recall_and_pin_categories() {
    for category in [
        "SYSTEM_CONVERSATION",
        "SYSTEM_ACCOUNT_SNAPSHOT",
        "MESSAGE_RECALL",
        "MESSAGE_PIN",
        "WEBRTC_AUDIO_CANCEL",
        "WEBRTC_AUDIO_DECLINE",
        "WEBRTC_AUDIO_END",
        "WEBRTC_AUDIO_BUSY",
        "WEBRTC_AUDIO_FAILED",
        "KRAKEN_END",
        "KRAKEN_DECLINE",
        "KRAKEN_CANCEL",
        "KRAKEN_INVITE",
    ] {
        assert!(
            !shows_delivery(Some("self"), Some(category), "self", false),
            "{category}"
        );
    }
    for category in [
        "PLAIN_TEXT",
        "SYSTEM_SAFE_SNAPSHOT",
        "SYSTEM_SAFE_INSCRIPTION",
        "APP_CARD",
    ] {
        assert!(
            shows_delivery(Some("self"), Some(category), "self", false),
            "{category}"
        );
        assert!(!shows_delivery(
            Some("other"),
            Some(category),
            "self",
            false
        ));
    }
    assert!(!shows_delivery(None, None, "self", false));
    assert!(!shows_delivery(Some(""), None, "", false));
}

#[test]
fn call_and_group_call_previews_follow_the_reference_category_allowlist() {
    // Flutter's isIllegalMessageCategory runs before the voice-call label branch.
    for category in [
        "WEBRTC_AUDIO_CANCEL",
        "WEBRTC_AUDIO_DECLINE",
        "WEBRTC_AUDIO_END",
        "WEBRTC_AUDIO_BUSY",
        "WEBRTC_AUDIO_FAILED",
        "KRAKEN_END",
        "KRAKEN_DECLINE",
        "KRAKEN_CANCEL",
        "KRAKEN_INVITE",
        "WEBRTC_AUDIO_OFFER",
        "KRAKEN_PUBLISH",
    ] {
        assert_eq!(
            message(en, category, "", None, false),
            "This type of message is not supported, please upgrade Mixin to the latest version."
        );
        assert_eq!(
            preview_icon(None, category, false),
            call(category).then_some(PreviewIcon::Call)
        );
    }
}

#[test]
fn unknown_status_keeps_category_icons_while_preview_text_is_unsupported() {
    for (category, icon) in [
        ("PLAIN_IMAGE", PreviewIcon::Image),
        ("SIGNAL_LIVE", PreviewIcon::Live),
    ] {
        assert_eq!(preview_icon(Some("UNKNOWN"), category, false), Some(icon));
        assert_eq!(
            message(en, category, "secret", Some("UNKNOWN"), false),
            "This type of message is not supported, please upgrade Mixin to the latest version."
        );
        assert_eq!(preview_icon(Some("UNKNOWN"), category, true), None);
    }
}

#[test]
fn unknown_prefix_with_a_known_suffix_keeps_icons_but_renders_unsupported_text() {
    for (category, icon) in [
        ("FUTURE_IMAGE", PreviewIcon::Image),
        ("FUTURE_LIVE", PreviewIcon::Live),
        ("FUTURE_POST", PreviewIcon::File),
        ("FUTURE_AUDIO", PreviewIcon::Audio),
    ] {
        assert_eq!(preview_icon(Some("SENT"), category, false), Some(icon));
        assert_eq!(
            message(en, category, "secret", Some("SENT"), false),
            "This type of message is not supported, please upgrade Mixin to the latest version."
        );
        assert_eq!(preview_icon(Some("FAILED"), category, false), None);
        assert_eq!(preview_icon(Some("SENT"), category, true), None);
    }
    assert_eq!(preview_icon(Some("SENT"), "FUTURE_TEXT", false), None);
}
