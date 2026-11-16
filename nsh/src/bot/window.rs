use crate::i18n::Key;
use nsh_core::prelude::*;
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
use serde_json::json;

use super::{BotWindows, Page};

pub(super) fn host(windows: BotWindows, page: ReadSignal<Page>) -> impl IntoElement {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        let reference = element_ref();
        let i18n = crate::i18n::current();
        native("MixinBotWindow")
            .element_ref(&reference)
            .prop("dark", move || json!(dark()))
            .prop("session", windows.session.with(|key| json!(key)))
            .prop("page", move || page.with(|page| {
                let context = json!({
                    "app_version": env!("CARGO_PKG_VERSION"), "immersive": page.immersive,
                    "platform": "Desktop", "locale": i18n.locale_tag(),
                    "conversation_id": page.source_conversation.as_deref().unwrap_or_default(),
                    "currency": page.currency,
                });
                json!({ "url": page.url, "title": page.title, "icon_data": page.icon_data, "app_id": page.app_id,
                    "shareable": page.shareable,
                    "immersive": page.immersive,
                    "script": include_str!("bridge.js")
                        .replace("__MIXIN_POST__", if cfg!(target_os = "windows") {
                            "window.chrome.webview.postMessage"
                        } else if cfg!(target_os = "linux") {
                            "window.__mixinNativePost"
                        } else {
                            "window.webkit.messageHandlers.mixinBot.postMessage"
                        })
                        .replace("__MIXIN_CONTEXT__", &context.to_string()),
                    "version": env!("CARGO_PKG_VERSION"), "focus": page.focus,
                    "locale": i18n.locale_tag(),
                    "labels": {
                        "Back": i18n.t(Key::NshBack),
                        "More": i18n.t(Key::More),
                        "Close": i18n.t(Key::Close),
                        "Refresh": i18n.t(Key::Refresh),
                        "Forward…": i18n.t(Key::NshForward),
                        "Copy link": i18n.t(Key::NshCopyLink),
                        "Open in browser": i18n.t(Key::NshOpenInBrowser),
                        "Webpage": i18n.t(Key::NshWebpage),
                        "OK": i18n.t(Key::NshOK),
                        "Cancel": i18n.t(Key::Cancel),
                        "Couldn’t download this file. Try again.": i18n.t(Key::NshCouldnTDownloadThisFileTryAgain),
                    } })
            }))
            .on_native_event(move |event: &NativeEvent| windows.event(page.get_untracked(), reference, event.clone()))
            .w_0().h_0().into_element()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = page;
        effect(move || windows.failed.set(true));
        view().w_0().h_0().into_element()
    }
}
