//! NSH owns every bot window; native components own only their browser engine.
use crate::{i18n::Key, ui};
use nsh_core::{
    desktop::{TitleBarOptions, TitleBarStyle, WindowAttributes, WindowFrameStyle},
    element::desktop,
    prelude::*,
};
use serde_json::json;

use super::{
    BotWindows, Page,
    chrome::{self, Browser},
};

pub(super) fn host(windows: BotWindows, page: ReadSignal<Page>) -> impl IntoElement {
    let id = page.get_untracked().id;
    let mut title_bar = TitleBarOptions::default();
    title_bar.style = TitleBarStyle::Hidden;
    title_bar.height = Some(chrome::height());
    let mut attributes = WindowAttributes::default();
    attributes.title_bar = Some(title_bar);
    attributes.frame = Some(WindowFrameStyle::None);
    attributes.min_size = Some(Some(Size::new(px(380.0), px(360.0))));
    desktop::window()
        .id(format!(
            "bot:{}:{id}",
            windows.session.with(|key| key.clone())
        ))
        .default_size(Size::new(px(380.0), px(750.0)))
        .title(move || page.get().title)
        .attributes(attributes)
        .on_open_change(move |event| {
            if !event.open {
                windows.close(id);
            }
        })
        .content(move || content(windows, page))
}

fn content(windows: BotWindows, page: ReadSignal<Page>) -> impl IntoElement {
    let reference = element_ref();
    let i18n = crate::i18n::current();
    let current = desktop::current_window();
    let focus = chrome::FocusTargets::new();
    let browser = signal(Browser::new(&page.get_untracked()));
    let immersive = signal(page.get_untracked().immersive);
    let seen_focus = stored(page.get_untracked().focus);
    let focus_window = current.clone();
    effect(move || {
        let focus = page.get().focus;
        if seen_focus.replace(focus) != focus {
            focus_window.focus();
        }
    });
    let title_window = current.clone();
    effect(move || {
        let mut attributes = WindowAttributes::default();
        attributes.title = Some(chrome::title(page, browser));
        title_window.set(attributes);
    });
    for (key, command) in [("mod+r", "reload"), ("mod+[", "back"), ("mod+]", "forward")] {
        on_key_with(
            key,
            KeyOptions {
                when: nsh_core::keymap::When::Always,
                ..Default::default()
            },
            move |_| reference.command(command, json!(null)),
        );
    }
    let close = current.clone();
    on_key_with(
        "mod+w",
        KeyOptions {
            when: nsh_core::keymap::When::Always,
            ..Default::default()
        },
        move |_| close.close(),
    );
    let native = native("MixinBotWebView")
        .element_ref(&reference)
        .prop("dark", move || json!(dark()))
        .prop("session", windows.session.with(|key| json!(key)))
        .prop("page", move || page.with(|page| {
            let context = json!({
                "app_version": env!("CARGO_PKG_VERSION"), "immersive": immersive.get(),
                "platform": "Desktop", "locale": i18n.locale_tag(),
                "conversation_id": page.source_conversation.as_deref().unwrap_or_default(),
                "currency": page.currency,
            });
            json!({ "url": page.url, "title": page.title, "app_id": page.app_id,
                "shareable": page.shareable, "immersive": immersive.get(),
                "script": include_str!("bridge.js")
                    .replace("__MIXIN_POST__", if cfg!(target_os = "windows") {
                        "window.chrome.webview.postMessage"
                    } else if cfg!(target_os = "linux") {
                        "window.__mixinNativePost"
                    } else { "window.webkit.messageHandlers.mixinBot.postMessage" })
                    .replace("__MIXIN_CONTEXT__", &context.to_string()),
                "version": env!("CARGO_PKG_VERSION"), "locale": i18n.locale_tag(),
                "labels": {
                    "Webpage": i18n.t(Key::NshWebpage), "OK": i18n.t(Key::NshOK), "Cancel": i18n.t(Key::Cancel),
                    "Couldn’t download this file. Try again.": i18n.t(Key::NshCouldnTDownloadThisFileTryAgain),
                } })
        }))
        .on_native_event(move |event: &NativeEvent| {
            if windows.ending.get_untracked() { return; }
            match event.name.as_ref() {
                "closed" => current.close(),
                "focus-exit" => {
                    focus.exit_browser(event.data.as_bool() == Some(true), immersive.get_untracked(), browser.get_untracked().can_back);
                },
                "navigation" => browser.update(|state| state.navigation(&event.data)),
                "theme" => browser.update(|state| state.theme = chrome::theme_color(&event.data)),
                "failed" => browser.update(|state| { state.loading = false; state.failed = true; }),
                _ => windows.event(page.get_untracked(), reference, event.clone()),
            }
        })
        .flex_1().min_h_0().min_w_0();
    #[cfg(target_os = "windows")]
    let native = super::windows::input(native, reference);
    chrome::surface(
        page,
        browser,
        immersive,
        reference,
        focus,
        move |action| match action {
            chrome::Action::About => {
                let page = page.get_untracked();
                if let Some(app_id) = page.app_id {
                    desktop::main_window().focus();
                    windows
                        .links
                        .open(format!("mixin://users/{app_id}"), page.source_conversation);
                }
            }
            chrome::Action::Share => {
                let page = page.get_untracked();
                let state = browser.get_untracked();
                if page.shareable {
                    desktop::main_window().focus();
                    windows.links.share_web_page(
                        state.url,
                        state.title,
                        page.app_id,
                        page.shareable,
                    );
                }
            }
            chrome::Action::Browser => {
                let raw = browser.get_untracked().url;
                windows.event(
                    page.get_untracked(),
                    reference,
                    NativeEvent {
                        name: "browser".into(),
                        data: json!(raw),
                    },
                );
            }
        },
        native,
    )
    .bg(ui::surface_pane)
}
