//! The group announcement banner and its conversation-scoped dismissal.

use crate::i18n::Key;
use std::sync::Arc;

use futures::StreamExt as _;
use mixin_desktop_api::{AccountClient, SettingsClient};
use nsh_core::prelude::*;

use crate::{
    i18n, runtime,
    ui::{self, Typography},
};

pub(super) fn banner(account: Arc<AccountClient>, conversation_id: String) -> impl IntoElement {
    let owner = Owner::current();
    let announcement = signal(String::new());
    let dismissed = signal(None::<String>);
    let busy = signal(false);
    let settings = crate::settings_client();
    let setting_key = stored(SettingsClient::dismissed_announcement_key(
        &account.account_id(),
        &conversation_id,
    ));
    let preferences = settings.clone();
    effect(move || {
        let settings = preferences.clone();
        let key = setting_key.with(|key| key.clone());
        spawn(async move {
            let mut changes = Box::pin(runtime::observe(settings.subscribe_setting(key)));
            while let Some(value) = changes.next().await {
                if !owner.is_live() {
                    return;
                }
                match value {
                    Ok(value) => dismissed.set(value),
                    Err(error) => log::warn!("Could not load announcement dismissal: {error}"),
                }
            }
        });
    });
    effect(move || {
        let account = account.clone();
        let conversation_id = conversation_id.clone();
        spawn(async move {
            let mut changes = Box::pin(runtime::observe(account.conversation_changes()));
            loop {
                let client = account.clone();
                let id = conversation_id.clone();
                let result = runtime::run(async move {
                    client.conversation().local_conversation_detail(id).await
                })
                .await;
                if !owner.is_live() {
                    return;
                }
                match result {
                    Ok(detail) => announcement.set(detail.announcement),
                    Err(error) => log::warn!("Could not load announcement: {error}"),
                }
                loop {
                    let Some(change) = changes.next().await else {
                        return;
                    };
                    if change.reload_all || change.conversation_ids.contains(&conversation_id) {
                        break;
                    }
                }
            }
        });
    });
    let unread = computed(move || {
        announcement.with(|announcement| {
            dismissed.with(|dismissed| {
                SettingsClient::unread_announcement(announcement, dismissed.as_deref())
                    .map(str::to_owned)
            })
        })
    });
    let settings = stored(settings);
    when(
        move || unread.with(Option::is_some),
        move || {
            dismissible(
                None,
                move || unread.get().unwrap_or_default(),
                busy,
                move || {
                    let value = announcement.get_untracked();
                    save(settings, setting_key, value, busy, owner)
                },
            )
        },
    )
}

/// The warning over a chat with an account many users reported as a scam.
pub(super) fn scam_banner(account: Arc<AccountClient>, user_id: String) -> impl IntoElement {
    let i18n = i18n::current();
    let owner = Owner::current();
    let dismissed = signal(None::<String>);
    let loaded = signal(false);
    let busy = signal(false);
    let settings = crate::settings_client();
    let setting_key = stored(SettingsClient::scam_warning_key(
        &account.account_id(),
        &user_id,
    ));
    let preferences = settings.clone();
    effect(move || {
        let settings = preferences.clone();
        let key = setting_key.with(|key| key.clone());
        spawn(async move {
            let mut changes = Box::pin(runtime::observe(settings.subscribe_setting(key)));
            while let Some(value) = changes.next().await {
                if !owner.is_live() {
                    return;
                }
                match value {
                    Ok(value) => {
                        dismissed.set(value);
                        loaded.set(true);
                    }
                    Err(error) => log::warn!("Could not load scam warning dismissal: {error}"),
                }
            }
        });
    });
    let settings = stored(settings);
    when(
        move || {
            loaded.get()
                && dismissed.with(|dismissed| {
                    SettingsClient::shows_scam_warning(
                        true,
                        dismissed.as_deref(),
                        chrono::Utc::now().timestamp_millis(),
                    )
                })
        },
        move || {
            dismissible(
                Some(ui::Icon::InfoCircle),
                move || i18n.t(Key::ScamWarning).to_owned(),
                busy,
                move || {
                    let now = chrono::Utc::now().timestamp_millis().to_string();
                    save(settings, setting_key, now, busy, owner)
                },
            )
        },
    )
}

fn save(
    settings: Stored<SettingsClient>,
    key: Stored<String>,
    value: String,
    busy: Signal<bool>,
    owner: Owner,
) {
    if busy.get_untracked() {
        return;
    }
    let settings = settings.with(|settings| settings.clone());
    let key = key.with(|key| key.clone());
    busy.set(true);
    let command = runtime::commit(async move { settings.set_setting(key, Some(value)).await });
    spawn(async move {
        let result = command.await;
        if owner.is_live() {
            busy.set(false);
            if let Err(error) = result {
                log::warn!("Could not dismiss banner: {error}");
            }
        }
    });
}

fn dismissible(
    icon: Option<ui::Icon>,
    message: impl Fn() -> String + 'static,
    busy: Signal<bool>,
    close: impl Fn() + 'static,
) -> View {
    let i18n = i18n::current();
    ui::glass(view())
        .flex_row()
        .items_center()
        .gap_2()
        .mx_3()
        .mb_2()
        .px(ui::space_md())
        .py_2()
        .rounded(ui::radius_lg())
        .children(icon.map(|icon| ui::icon(icon).size(ui::icon_sm())))
        .child(
            text(message)
                .text_role(ui::TextRole::Body)
                .number_of_lines(2)
                .flex_1()
                .min_w_0(),
        )
        .child(
            ui::icon_button()
                .size(ui::pin_close_size())
                .px_0()
                .py_0()
                .disabled(busy)
                .accessibility_label(move || Some(i18n.t(Key::Close).into()))
                .on_press(close)
                .child(ui::icon(ui::Icon::Close).size(ui::icon_sm())),
        )
}
