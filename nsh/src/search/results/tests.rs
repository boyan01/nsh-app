use super::*;
use crate::presentation::PreviewIcon;

fn conversation() -> ConversationListData {
    ConversationListData {
        conversation_id: "chat".into(),
        owner_id: "other".into(),
        name: "Other".into(),
        avatar_url: String::new(),
        category: "CONTACT".into(),
        draft: "unsent draft".into(),
        status: 2,
        last_read_message_id: None,
        last_message: "hello".into(),
        last_message_category: Some("PLAIN_TEXT".into()),
        last_message_status: Some("SENT".into()),
        last_message_sender_id: Some("self".into()),
        last_message_sender_name: Some("Alice".into()),
        last_message_action: None,
        last_message_participant_id: None,
        last_message_participant_name: None,
        updated_at_millis: 0,
        unseen_count: 0,
        mention_count: 0,
        is_muted: false,
        is_verified: false,
        is_scam: false,
        is_bot: false,
        is_bot_group: false,
        membership: None,
        is_pinned: false,
        pin_time_millis: 0,
        relationship: "FRIEND".into(),
        identity_number: String::new(),
        circle_ids: Vec::new(),
        participant_count: 0,
        group_avatars: Vec::new(),
        expire_in: 0,
    }
}

#[test]
fn conversation_search_with_a_draft_renders_the_last_message_and_category_icon() {
    let runtime = nsh_core::element::Runtime::default();
    let _entered = runtime.enter();
    let i18n = i18n::I18n::for_test("en");
    let mut conversation = conversation();
    conversation.last_message_category = Some("PLAIN_IMAGE".into());
    for draft in ["unsent draft", "", " "] {
        conversation.draft = draft.into();
        let row = Row::Conversation(Box::new(conversation.clone()));
        assert_eq!(subtitle(i18n, &row, "self"), "[Image]");
        assert_eq!(category_icon(&row), Some(PreviewIcon::Image));
    }
}

#[test]
fn conversation_search_with_a_draft_keeps_system_and_pin_previews() {
    let runtime = nsh_core::element::Runtime::default();
    let _entered = runtime.enter();
    let i18n = i18n::I18n::for_test("en");
    let mut conversation = conversation();
    conversation.last_message_category = Some("SYSTEM_CONVERSATION".into());
    conversation.last_message_action = Some("CREATE".into());
    let row = Row::Conversation(Box::new(conversation.clone()));
    assert_eq!(subtitle(i18n, &row, "self"), "You created this group");
    assert_eq!(category_icon(&row), None);

    conversation.last_message_category = Some("MESSAGE_PIN".into());
    conversation.last_message =
        r#"{"message_id":"m","category":"PLAIN_TEXT","content":"pinned text"}"#.into();
    let row = Row::Conversation(Box::new(conversation));
    assert_eq!(subtitle(i18n, &row, "self"), "Alice pinned pinned text");
    assert_eq!(category_icon(&row), None);
}

#[test]
fn contact_search_omits_sender_prefix_while_group_search_and_conversation_list_keep_it() {
    let runtime = nsh_core::element::Runtime::default();
    let _entered = runtime.enter();
    let i18n = i18n::I18n::for_test("en");
    for (sender_id, sender_name, expected_sender) in
        [("self", "Alice", "You"), ("third", "Bob", "Bob")]
    {
        let mut conversation = conversation();
        conversation.last_message_sender_id = Some(sender_id.into());
        conversation.last_message_sender_name = Some(sender_name.into());
        let row = Row::Conversation(Box::new(conversation.clone()));
        assert_eq!(subtitle(i18n, &row, "self"), "hello");
        assert_eq!(
            presentation::conversation_preview(i18n, &conversation, "self"),
            format!("{expected_sender}: hello")
        );

        conversation.category = "GROUP".into();
        let row = Row::Conversation(Box::new(conversation.clone()));
        assert_eq!(
            subtitle(i18n, &row, "self"),
            format!("{expected_sender}: hello")
        );
        assert_eq!(
            presentation::conversation_preview(i18n, &conversation, "self"),
            format!("{expected_sender}: hello")
        );
    }
}
