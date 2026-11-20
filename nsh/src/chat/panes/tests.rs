use super::*;
use crate::navigation::{ChatNavigation, Panel};
use nsh_engine::{Clock, EngineConfig, Root};
use nsh_testing::Headless;
use std::{
    panic::{PanicHookInfo, set_hook, take_hook},
    sync::{Arc, Mutex},
    thread::{self, ThreadId},
    time::SystemTime,
};

// NSH catches binding panics, and orphaned bindings only log their failure.
// Record this engine's panics as well as asserting its rendered content.
fn capture_engine_panics(run: impl FnOnce(Arc<Mutex<Option<ThreadId>>>)) -> Vec<String> {
    let engine_thread = Arc::new(Mutex::new(None));
    let messages = Arc::new(Mutex::new(Vec::new()));
    let previous = Arc::new(take_hook());
    let captured_thread = engine_thread.clone();
    let captured_messages = messages.clone();
    let forward = previous.clone();
    set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
        if *captured_thread.lock().unwrap() == Some(thread::current().id()) {
            captured_messages.lock().unwrap().push(info.to_string());
        }
        forward(info);
    }));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(engine_thread)));
    // Restore the original hook even when an assertion in the flow fails.
    drop(take_hook());
    set_hook(Arc::try_unwrap(previous).unwrap_or_else(|_| unreachable!()));
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
    Arc::try_unwrap(messages).unwrap().into_inner().unwrap()
}

#[component]
fn SwitchingConversations() -> impl IntoElement {
    provide_context(crate::i18n::I18n::for_test("en"));
    provide_context(crate::navigation::Navigation::new());
    let selected = signal(Some(conversation("first")));
    let navigation = ChatNavigation::new(
        selected,
        signal_with(None, SignalEquals::Never),
        signal(false),
    );
    provide_context(navigation);
    provide_context(crate::links::LinkNavigation::detached());
    let delayed = signal(false);
    view()
        .flex_1()
        .child(
            nsh_base::button()
                .on_press(move || navigation.select(conversation("second")))
                .child(text("Switch second")),
        )
        .child(
            nsh_base::button()
                .on_press(move || navigation.select(conversation("first")))
                .child(text("Switch first")),
        )
        .child(
            nsh_base::button()
                .on_press(move || navigation.toggle_panel(Panel::Pinned))
                .child(text("Open pinned")),
        )
        .child(
            nsh_base::button()
                .on_press(move || navigation.suspend_inspector(Panel::Pinned))
                .child(text("Suspend pinned")),
        )
        .child(
            nsh_base::button()
                .on_press(move || delayed.set(true))
                .child(text("Delay readiness")),
        )
        .child(
            nsh_base::button()
                .on_press(move || navigation.select(conversation("third")))
                .child(text("Switch third")),
        )
        .child(
            nsh_base::button()
                .on_press(move || selected.set(None))
                .child(text("Clear")),
        )
        .child(panes(
            selected,
            navigation.selected_id,
            || view().into_element(),
            move |conversation, ready| {
                let header = signal(conversation);
                let sender_names = computed(move || header.with(|row| row.category == "GROUP"));
                let loaded = signal(!delayed.get_untracked());
                if !loaded.get_untracked() {
                    set_timeout(READY_BOUND + Duration::from_millis(200), move || {
                        loaded.set(true)
                    });
                }
                effect(move || {
                    if loaded.get() {
                        untracked(|| ready());
                    }
                });
                let pinned = super::super::pinned::Pinned::detached(
                    signal(Some("Pinned preview".into())),
                    navigation.panel,
                );
                view()
                    .flex_1()
                    .child(super::super::header::bar(
                        header,
                        true,
                        || false,
                        || {},
                        pinned,
                        signal(800.0),
                        || {},
                    ))
                    .child(text(move || {
                        header.with(|row| format!("Chat {}", row.conversation_id))
                    }))
                    .child(
                        virtual_list(
                            || 5_usize,
                            move |at| {
                                let names = computed(move || sender_names.get());
                                text(move || format!("Row {} names {}", at.index(), names.get()))
                            },
                        )
                        .flex_1(),
                    )
                    .child(crate::inspector::overlay(navigation, move |_| {
                        ui::owned("pinned", move || {
                            let names = computed(move || sender_names.get());
                            view().child(
                                virtual_list(
                                    || 5_usize,
                                    move |at| {
                                        text(move || {
                                            format!(
                                                "Pinned row {} names {}",
                                                at.index(),
                                                names.get()
                                            )
                                        })
                                    },
                                )
                                .h_40(),
                            )
                        })
                        .into_element()
                    }))
            },
        ))
}

#[test]
fn switching_conversations_disposes_old_bindings_without_reading_their_signals() {
    let panics = capture_engine_panics(|engine_thread| {
        let mut app = Headless::start(EngineConfig {
            clock: Clock::manual(SystemTime::UNIX_EPOCH),
            ..EngineConfig::new(Root::new::<SwitchingConversations>(move || {
                *engine_thread.lock().unwrap() = Some(thread::current().id());
            }))
        })
        .unwrap();
        app.wait_for("Chat first").unwrap();
        app.press("Open pinned").unwrap();
        app.wait_for("Pinned row 0 names true").unwrap();
        app.press("Suspend pinned").unwrap();
        app.press("Switch second").unwrap();
        app.wait_for("Chat second").unwrap();
        app.fast_forward(Duration::from_secs(1)).unwrap();
        assert!(!app.shows("Chat first"));
        app.press("Switch first").unwrap();
        app.wait_for("Chat first").unwrap();
        app.fast_forward(Duration::from_secs(1)).unwrap();
        assert!(!app.shows("Chat second"));
        assert!(app.shows("Row 0 names true"));

        // Readiness after the bound, including a superseded waiting pane.
        app.press("Delay readiness").unwrap();
        app.press("Switch second").unwrap();
        app.press("Switch third").unwrap();
        app.fast_forward(Duration::from_secs(1)).unwrap();
        assert!(app.shows("Chat third"));
        assert!(!app.shows("Chat second"));
        assert!(!app.shows("Chat first"));

        // A pending timeout/ready frame must not resurrect a closed chat.
        app.press("Switch first").unwrap();
        app.press("Clear").unwrap();
        app.fast_forward(Duration::from_secs(1)).unwrap();
        assert!(!app.shows("Chat first"));
        assert!(!app.shows("Chat third"));
        drop(app);
    });
    assert!(panics.is_empty(), "{panics:?}");
}

// Run the entire remount sequence within a commit, before the display frame
// queued by the first waiting mount can run.
#[component]
fn RemountingConversation() -> impl IntoElement {
    let selected = signal(Some(conversation("anchor")));
    let selected_id =
        computed(move || selected.with(|row| row.as_ref().map(|row| row.conversation_id.clone())));
    let mounts = stored(0_u32);
    let release = signal(false);
    view()
        .flex_1()
        .child(
            nsh_base::button()
                .on_press(move || selected.set(Some(conversation("pending"))))
                .child(text("Remount pending")),
        )
        .child(
            nsh_base::button()
                .on_press(move || release.set(true))
                .child(text("Ready remount")),
        )
        .child(panes(
            selected,
            selected_id,
            || view().into_element(),
            move |row, ready| {
                let id = row.conversation_id;
                let mount = if id == "pending" {
                    mounts.with(|mount| {
                        *mount += 1;
                        *mount
                    })
                } else {
                    0
                };
                let visibility = use_context::<Visibility>().unwrap();
                let label = id.clone();
                effect(move || match (id.as_str(), mount) {
                    ("pending", 1) => {
                        ready();
                        selected.set(Some(conversation("intermediate")));
                    }
                    ("intermediate", _) => selected.set(Some(conversation("pending"))),
                    ("pending", _) if release.get() => ready(),
                    ("anchor", _) => ready(),
                    _ => {}
                });
                text(move || {
                    format!(
                        "{} {label} mount {mount}",
                        if visibility.0.get() {
                            "Shown"
                        } else {
                            "Waiting"
                        }
                    )
                })
            },
        ))
}

#[test]
fn a_remounted_conversation_waits_for_its_own_readiness_instead_of_an_old_ready_frame() {
    let mut app = Headless::start(EngineConfig {
        clock: Clock::manual(SystemTime::UNIX_EPOCH),
        ..EngineConfig::new(Root::of::<RemountingConversation>())
    })
    .unwrap();
    app.wait_for("Shown anchor mount 0").unwrap();
    app.press("Remount pending").unwrap();
    app.fast_forward(Duration::from_millis(50)).unwrap();
    assert!(
        app.shows("Waiting pending mount 2"),
        "{:?}",
        app.screen().texts()
    );
    assert!(app.shows("Shown anchor mount 0"));
    assert!(!app.shows("Shown pending mount 2"));
    app.press("Ready remount").unwrap();
    app.fast_forward(Duration::from_millis(50)).unwrap();
    assert!(app.shows("Shown pending mount 2"));
    assert!(!app.shows("Shown anchor mount 0"));
}

fn conversation(id: &str) -> ConversationListData {
    ConversationListData {
        conversation_id: id.into(),
        owner_id: String::new(),
        name: String::new(),
        avatar_url: String::new(),
        category: "GROUP".into(),
        draft: String::new(),
        status: 2,
        last_read_message_id: None,
        last_message: String::new(),
        last_message_category: None,
        last_message_status: None,
        last_message_sender_id: None,
        last_message_sender_name: None,
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
        relationship: String::new(),
        identity_number: String::new(),
        circle_ids: Vec::new(),
        participant_count: 0,
        group_avatars: Vec::new(),
        expire_in: 0,
    }
}
