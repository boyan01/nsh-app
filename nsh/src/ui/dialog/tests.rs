//! Exercise modal dismissal against the window-wide chat shortcut and input focus.
use super::*;
use nsh_core::element::input;
use nsh_core::platform::{Environment, Os};
use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;

#[component]
fn DialogOverChat() -> impl IntoElement {
    let open = signal(true);
    let nested = signal(false);
    let blocked = signal(false);
    let background_escapes = signal(0);
    on_key_with(
        "escape",
        KeyOptions {
            when: nsh_core::keymap::When::NotEditing,
            ..Default::default()
        },
        move |_| background_escapes.update(|count| *count += 1),
    );
    let presence = motion::Presence::modal(move || open.get());
    let nested_presence = motion::Presence::modal(move || nested.get());
    view()
        .child(text(move || {
            format!("Background escapes: {}", background_escapes.get())
        }))
        .child(when(
            move || presence.mounted(),
            move || {
                dialog(
                    presence,
                    move || {
                        if !blocked.get_untracked() {
                            open.set(false);
                        }
                    },
                    view()
                        .child(text("User dialog"))
                        .child(input().test_id("dialog-input"))
                        .child(
                            nsh_base::button()
                                .on_press(move || blocked.set(true))
                                .child(text("Block closing")),
                        )
                        .child(
                            nsh_base::button()
                                .on_press(move || nested.set(true))
                                .child(text("Open nested")),
                        )
                        .child(when(
                            move || nested_presence.mounted(),
                            move || {
                                held_dialog(
                                    nested_presence,
                                    move || nested.set(false),
                                    view().child(text("Nested dialog")),
                                )
                            },
                        )),
                )
            },
        ))
}

fn start() -> Headless {
    let mut app = Headless::start(EngineConfig {
        environment: Environment {
            os: Os::MacOs,
            reduce_motion: true,
            ..Default::default()
        },
        ..EngineConfig::new(Root::of::<DialogOverChat>())
    })
    .unwrap();
    app.wait_for("User dialog").unwrap();
    app
}

#[test]
fn an_open_dialog_takes_escape_before_the_background_chat_with_or_without_input_focus() {
    for editing in [false, true] {
        let mut app = start();
        if editing {
            app.type_text("id:dialog-input", "draft").unwrap();
        }
        app.press_key("escape").unwrap();
        assert!(!app.shows("User dialog"));
        assert!(app.shows("Background escapes: 0"));
        app.press_key("escape").unwrap();
        assert!(app.shows("Background escapes: 1"));
    }
}

#[test]
fn escape_closes_only_the_top_dialog_and_restores_the_parent_dismissal_handler() {
    let mut app = start();
    app.press("Open nested").unwrap();
    app.wait_for("Nested dialog").unwrap();
    app.press_key("escape").unwrap();
    assert!(!app.shows("Nested dialog"));
    assert!(app.shows("User dialog"));
    app.press_key("escape").unwrap();
    assert!(!app.shows("User dialog"));
    assert!(app.shows("Background escapes: 0"));
}

#[test]
fn a_dialog_that_rejects_cancellation_consumes_escape_without_changing_the_background() {
    let mut app = start();
    app.press("Block closing").unwrap();
    app.press_key("escape").unwrap();
    assert!(app.shows("User dialog"));
    assert!(app.shows("Background escapes: 0"));
}

#[test]
fn native_cancel_and_escape_use_the_same_dialog_close_path() {
    let mut app = start();
    app.back().unwrap();
    assert!(!app.shows("User dialog"));
    assert!(app.shows("Background escapes: 0"));
}
