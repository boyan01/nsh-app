//! Account-free chrome fixture. Native previews embed a loopback WebView; web uses a mock.
use super::{Page, chrome};
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use crate::ui::{self, Typography as _};
use nsh_core::prelude::*;

pub(super) fn page(bot: bool) -> Page {
    Page {
        id: 1,
        key: String::new(),
        app_id: bot.then(|| "84cc76e1-4e21-4e77-a06c-5c15c9b52378".into()),
        title: if bot { "Mixin Bot" } else { "Example page" }.into(),
        immersive: false,
        currency: "USD".into(),
        source_conversation: None,
        url: "https://example.com/".into(),
        focus: 0,
        icon_data: None,
        shareable: true,
        identity_number: bot.then(|| "7000101793".into()),
        is_verified: false,
        membership: None,
    }
}

pub(crate) fn surface(
    bot: bool,
    immersive: Signal<bool>,
    back: Signal<bool>,
    failed: Signal<bool>,
    badge: Signal<crate::appearance::IdentityBadgeKind>,
) -> View {
    let page = signal(page(bot));
    let browser = signal(chrome::Browser::new(&page.get_untracked()));
    effect(move || {
        use crate::appearance::IdentityBadgeKind as Badge;
        let badge = badge.get();
        page.update(|p| {
            p.is_verified = badge == Badge::Verified || badge == Badge::Standard;
            p.membership = (badge == Badge::Standard).then(|| {
                serde_json::json!({"plan":"STANDARD", "expired_at":"2126-01-01T00:00:00Z"})
                    .to_string()
            });
        });
    });
    effect(move || {
        browser.update(|state| {
            state.can_back = back.get();
            state.failed = failed.get();
            state.loading = false;
        });
    });
    let reference = element_ref();
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    let web = {
        let session = if bot {
            "84cc76e1-4e21-4e77-a06c-5c15c9b52378:chrome-preview-bot"
        } else {
            "84cc76e1-4e21-4e77-a06c-5c15c9b52378:chrome-preview-page"
        };
        let body = native("MixinBotWebView").key(session).element_ref(&reference).flex_1().min_h_0().min_w_0()
            .prop("session", serde_json::json!(session))
            .prop("dark", move || serde_json::json!(dark()))
            .prop("page", move || serde_json::json!({
                "url": "http://127.0.0.1:8851/", "title": page.get().title,
                "app_id": page.get().app_id, "version": env!("CARGO_PKG_VERSION"),
                "immersive": immersive.get(), "locale": crate::i18n::current().locale_tag(),
                "script": include_str!("bridge.js").replace("__MIXIN_POST__", if cfg!(target_os="windows") { "window.chrome.webview.postMessage" } else if cfg!(target_os="linux") { "window.__mixinNativePost" } else { "window.webkit.messageHandlers.mixinBot.postMessage" }).replace("__MIXIN_CONTEXT__", &serde_json::json!({ "immersive":immersive.get(), "platform":"Desktop", "locale":crate::i18n::current().locale_tag() }).to_string()),
                "labels": {}
            }))
            .on_native_event(move |event: &NativeEvent| { match event.name.as_ref() {
                "navigation" => browser.update(|b| b.navigation(&event.data)),
                "theme" => browser.update(|b| b.theme = chrome::theme_color(&event.data)),
                "failed" => browser.update(|b| { b.loading = false; b.failed = true; }),
                _ => {}
            }});
        #[cfg(target_os = "windows")]
        let body = super::windows::input(body, reference);
        view()
            .flex_1()
            .child(
                native("MixinBotSession")
                    .key(format!("session:{session}"))
                    .prop("session", serde_json::json!(session))
                    .w_0()
                    .h_0(),
            )
            .child(body)
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let web = view().flex_1().bg(ui::surface_pane).p(ui::space_lg())
            .justify_center().gap(ui::space_md())
            .child(text("Web content").text_role(ui::TextRole::Title))
            .child(text("Fixture for shared Bot and webpage chrome. Native engine composition is verified separately.").text_role(ui::TextRole::Body).text_color(ui::fg_secondary));
    chrome::surface(
        page.read_only(),
        browser,
        immersive,
        reference,
        chrome::FocusTargets::new(),
        |_| {},
        web,
    )
}
