//! Observable window-registry and browser presentation regressions, without accounts.
use super::*;
use nsh_core::platform::{Environment, Os};
use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;

#[component]
fn RegistryFixture() -> impl IntoElement {
    nsh_core::element::desktop::window()
        .id("main")
        .content(registry_fixture_content)
}

fn registry_fixture_content() -> impl IntoElement {
    provide_context(crate::navigation::ChatNavigation::new(
        signal(None),
        signal(None),
        signal(false),
    ));
    provide_context(LinkNavigation::detached());
    let windows = detached();
    let link = AppLink {
        app_id: Some("84cc76e1-4e21-4e77-a06c-5c15c9b52378".into()),
        allow_generic: false,
        url: "https://example.com/".into(),
        source_conversation: Some("conversation".into()),
        app_card_data: None,
    };
    let mut page = preview::page(true);
    page.key = json!([link.app_id, link.source_conversation, link.url]).to_string();
    windows.pages.set(vec![page.clone()]);
    let page = stored(page);
    let reference = element_ref();
    view()
        .child(
            nsh_base::button()
                .child(text("Reopen"))
                .on_press(move || windows.open(link.clone())),
        )
        .child(
            nsh_base::button()
                .child(text("Close"))
                .on_press(move || windows.close(1)),
        )
        .child(
            nsh_base::button()
                .child(text("Late event"))
                .on_press(move || {
                    windows.event(
                        page.get(),
                        reference,
                        NativeEvent {
                            name: "external".into(),
                            data: json!("https://external.example/"),
                        },
                    )
                }),
        )
        .child(
            nsh_base::button()
                .child(text("End session"))
                .on_press(move || windows.ending.set(true)),
        )
        .child(text(move || {
            format!(
                "pages={} focus={} external={}",
                windows.pages.with(|p| p.len()),
                windows
                    .pages
                    .with(|p| p.first().map(|p| p.focus).unwrap_or_default()),
                windows.external.with(|url| url.is_some())
            )
        }))
}
fn registry() -> Headless {
    let mut app = Headless::start(EngineConfig {
        environment: Environment {
            os: Os::MacOs,
            reduce_motion: true,
            ..Default::default()
        },
        ..EngineConfig::new(Root::of::<RegistryFixture>())
    })
    .unwrap();
    app.wait_for("pages=1 focus=0 external=false").unwrap();
    app
}
#[test]
fn reopening_the_same_app_conversation_and_url_focuses_the_existing_window() {
    let mut app = registry();
    app.press("Reopen").unwrap();
    assert!(app.shows("pages=1 focus=1 external=false"));
    app.press("Reopen").unwrap();
    assert!(app.shows("pages=1 focus=2 external=false"));
}
#[test]
fn closing_a_window_removes_it_and_ignores_its_late_native_events() {
    let mut app = registry();
    app.press("Close").unwrap();
    app.press("Late event").unwrap();
    assert!(app.shows("pages=0 focus=0 external=false"));
}
#[test]
fn ending_a_session_prevents_reopen_and_native_events() {
    let mut app = registry();
    app.press("End session").unwrap();
    app.press("Reopen").unwrap();
    app.press("Late event").unwrap();
    assert!(app.shows("pages=1 focus=0 external=false"));
}
#[test]
fn failed_navigation_stays_visible_through_history_updates_until_a_new_load() {
    let mut browser = chrome::Browser::new(&preview::page(false));
    browser.failed = true;
    browser
        .navigation(&json!({"url":"https://example.com/error", "loading":false, "can_back":true}));
    assert!(browser.failed);
    assert!(browser.can_back);
    assert_eq!(browser.url, "https://example.com/error");
    browser.navigation(&json!({"loading":true}));
    assert!(!browser.failed);
    assert!(browser.loading);
}
#[test]
fn document_titles_update_plain_pages_but_keep_the_bot_identity() {
    let mut page = preview::page(true);
    assert_eq!(chrome::title_untracked(&page, "Document"), "Mixin Bot");
    page.app_id = None;
    assert_eq!(chrome::title_untracked(&page, "Document"), "Document");
    assert_eq!(chrome::title_untracked(&page, "  "), "Mixin Bot");
}

#[test]
fn bot_document_titles_follow_spa_changes_and_clear_without_changing_the_identity() {
    let page = preview::page(true);
    let mut browser = chrome::Browser::new(&page);
    assert!(
        browser.title.is_empty(),
        "the business layer handles missing share titles"
    );
    for title in ["First article", "Second article", ""] {
        browser.navigation(&json!({"title": title}));
        assert_eq!(
            browser.title, title,
            "forwarding retains the raw document title"
        );
        assert_eq!(chrome::title_untracked(&page, &browser.title), "Mixin Bot");
    }
}

#[component]
fn FocusFixture() -> impl IntoElement {
    nsh_core::element::desktop::window()
        .id("main")
        .content(focus_fixture_content)
}

fn focus_fixture_content() -> impl IntoElement {
    let immersive = signal(false);
    let history = signal(false);
    let selected = signal("none");
    let focus = chrome::FocusTargets::new();
    view()
        .children(
            [
                ("Normal empty", false, false),
                ("Normal history", false, true),
                ("Immersive empty", true, false),
                ("Immersive history", true, true),
            ]
            .map(|(name, hidden, back)| {
                nsh_base::button().child(text(name)).on_press(move || {
                    immersive.set(hidden);
                    history.set(back);
                })
            }),
        )
        .child(when(
            move || !immersive.get() && history.get(),
            move || {
                nsh_base::button()
                    .element_ref(&focus.back)
                    .child(text("Back target"))
                    .on_press(move || selected.set("Back"))
            },
        ))
        .child(
            nsh_base::button()
                .element_ref(&focus.first)
                .child(text("More target"))
                .on_press(move || selected.set("More")),
        )
        .child(
            nsh_base::button()
                .element_ref(&focus.last)
                .child(text("Close target"))
                .on_press(move || selected.set("Close")),
        )
        .child(
            nsh_base::button()
                .child(text("Forward exit"))
                .on_press(move || {
                    focus.exit_browser(false, immersive.get_untracked(), history.get_untracked())
                }),
        )
        .child(
            nsh_base::button()
                .child(text("Reverse exit"))
                .on_press(move || {
                    focus.exit_browser(true, immersive.get_untracked(), history.get_untracked())
                }),
        )
        .child(text(move || format!("selection={}", selected.get())))
}

#[test]
fn browser_focus_exit_targets_mounted_controls_after_header_and_history_changes() {
    let mut app = Headless::start(EngineConfig {
        environment: Environment {
            os: Os::MacOs,
            reduce_motion: true,
            ..Default::default()
        },
        ..EngineConfig::new(Root::of::<FocusFixture>())
    })
    .unwrap();
    app.wait_for("selection=none").unwrap();
    for (mode, expected) in [
        ("Normal empty", "More"),
        ("Normal history", "Back"),
        ("Immersive history", "More"),
        ("Immersive empty", "More"),
        ("Normal history", "Back"),
    ] {
        app.press(mode).unwrap();
        app.press("Forward exit").unwrap();
        app.press_key("Enter").unwrap();
        assert!(app.shows(&format!("selection={expected}")), "{mode}");
        app.press("Reverse exit").unwrap();
        app.press_key("Enter").unwrap();
        assert!(app.shows("selection=Close"), "{mode}, reverse");
    }
}
