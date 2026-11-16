use nsh_core::prelude::*;

use crate::{
    i18n::{self, I18n, Key},
    notifications::Handle,
    notifications::native::{Availability, Permission},
    ui,
    ui::parts,
};

pub(super) fn screen() -> impl IntoElement {
    let i18n = i18n::current();
    let handle = use_context::<Handle>().expect("App provides native notifications");
    let available = move || handle.availability == Availability::Available;
    let settings_failed = signal(false);
    scroll().flex_1().min_h_0().child(
        parts::page_body()
            .child(
                ui::card()
                    .child(ui::static_row(move || permission_summary(i18n, handle)))
                    .child(when(
                        move || {
                            available()
                                && handle.permission.get() == Some(Permission::NotDetermined)
                        },
                        move || {
                            ui::settings_row(None, move || i18n.t(Key::NshAllowNotifications))
                                .disabled(handle.busy)
                                .on_press(move || handle.request_permission())
                        },
                    ))
                    .child(when(available, move || {
                        ui::settings_row(None, move || i18n.t(Key::NshRefreshPermission))
                            .disabled(handle.busy)
                            .on_press(move || handle.refresh_permission())
                    }))
                    .child(when(
                        move || {
                            available() && cfg!(any(target_os = "macos", target_os = "windows"))
                        },
                        move || system_settings(settings_failed),
                    )),
            )
            .child(when(available, move || preview_setting(handle)))
            .child(when(handle.failed, move || {
                ui::error_text(move || i18n.t(Key::NshNotificationsUnavailable))
            }))
            .child(when(settings_failed, move || {
                ui::error_text(move || i18n.t(Key::NshCouldnTCompleteThisActionTryAgain))
            })),
    )
}

fn permission_summary(i18n: I18n, handle: Handle) -> &'static str {
    if handle.availability == Availability::MissingAppBundle {
        return i18n.t(Key::NshNotificationsPackagedAppRequired);
    }
    if handle.availability != Availability::Available {
        return i18n.t(Key::NshNotificationsAreNotAvailableOnThisPlatformYet);
    }
    match handle.permission.get() {
        #[cfg(target_os = "linux")]
        Some(Permission::NotRequired) => {
            i18n.t(Key::NshNoAppPermissionPromptIsRequiredOnThisSystem)
        }
        Some(Permission::Authorized | Permission::Provisional) => {
            i18n.t(Key::NshNotificationsAreEnabled)
        }
        Some(Permission::Denied) => i18n.t(Key::NshNotificationsAreDisabledInSystemSettings),
        Some(Permission::NotDetermined) => {
            i18n.t(Key::NshAllowNotificationsToReceiveNewMessageAlerts)
        }
        Some(Permission::Unknown) => {
            i18n.t(Key::NshTheSystemReturnedAnUnknownNotificationPermission)
        }
        None => i18n.t(Key::NshCheckingNotificationPermission),
    }
}

fn preview_setting(handle: Handle) -> impl IntoElement {
    let i18n = i18n::current();
    view()
        .gap_2()
        .child(ui::card().child(ui::toggle_row(
            move || i18n.t(Key::NshMessagePreview),
            move || handle.preview.get(),
            move || !handle.preview_ready.get() || handle.preview_busy.get(),
            move |value| handle.set_preview(value),
        )))
        .child(parts::note(move || {
            i18n.t(Key::NshNotificationPreviewDescription)
        }))
        .child(when(handle.preview_failed, move || {
            load_error(move || handle.refresh_preview())
        }))
}

fn system_settings(failed: Signal<bool>) -> impl IntoElement {
    let i18n = i18n::current();
    ui::settings_row(None, move || i18n.t(Key::NshOpenNotificationSettings))
        .child(ui::chevron())
        .on_press(move || {
            spawn(async move {
                failed.set(
                    !open_url(if cfg!(target_os = "windows") {
                        "ms-settings:notifications"
                    } else {
                        "x-apple.systempreferences:com.apple.preference.notifications"
                    })
                    .await,
                );
            });
        })
}

pub(crate) fn load_error(retry: impl Fn() + 'static) -> View {
    let i18n = crate::i18n::current();
    parts::failure(
        move || i18n.t(Key::NshCouldnTCompleteThisActionTryAgain),
        retry,
    )
}

#[cfg(debug_assertions)]
pub(crate) fn preview(failed: Signal<bool>) -> impl IntoElement {
    provide_context(crate::notifications::detached(failed));
    screen()
}
