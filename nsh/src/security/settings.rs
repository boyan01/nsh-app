use nsh_base::SelectItem;
use nsh_core::prelude::*;

use super::{Handle, passcode};
use crate::{
    i18n::{self, Key},
    ui,
    ui::parts::{self, Prompt, confirm},
};

pub(crate) fn settings(handle: Handle) -> impl IntoElement {
    let i18n = i18n::current();
    let setup = signal(false);
    let presence = crate::motion::Presence::modal(move || setup.get());
    effect(move || {
        if handle.enabled() {
            setup.set(false);
        }
    });
    let available = move |expected: bool| {
        handle.state.with(|state| {
            state
                .as_ref()
                .is_some_and(|state| state.available == expected)
        })
    };
    scroll().flex_1().min_h_0().child(
        parts::page_body()
            .child(
                ui::section(move || i18n.t(Key::NshScreenPrivacy))
                    .child(parts::note(move || {
                        i18n.t(Key::NshScreenPasscodeDescription)
                    }))
                    .child(when(
                        move || available(false),
                        move || {
                            parts::note(move || {
                                i18n.t(Key::NshScreenPrivacyIsNotAvailableOnThisPlatformYet)
                            })
                        },
                    ))
                    .child(when(
                        move || available(true),
                        move || {
                            ui::card()
                                .child(ui::toggle_row(
                                    move || i18n.t(Key::NshScreenPasscode),
                                    move || handle.enabled(),
                                    move || handle.busy.get(),
                                    move |enabled| {
                                        if enabled {
                                            setup.set(true);
                                        } else {
                                            disable_passcode(handle);
                                        }
                                    },
                                ))
                                .child(when(move || handle.enabled(), move || lock_options(handle)))
                        },
                    )),
            )
            .child(when(handle.failed, move || {
                ui::error_text(move || i18n.t(Key::NshCouldnTCompleteThisActionTryAgain))
            }))
            .child(when(
                move || presence.mounted(),
                move || setup_dialog(handle, setup, presence),
            )),
    )
}

fn disable_passcode(handle: Handle) {
    let i18n = i18n::current();
    let client = handle.client();
    spawn(async move {
        let prompt = Prompt::new(
            i18n.t(Key::NshTurnOffScreenPasscode),
            i18n.t(Key::NshAccountContentWillNoLongerBeHiddenWhenMixinLocks),
            i18n.t(Key::NshTurnOff),
        );
        if confirm(prompt).await {
            handle.command(async move { client.disable_passcode().await });
        }
    });
}

fn lock_options(handle: Handle) -> impl IntoElement {
    let i18n = i18n::current();
    let minutes = move || {
        handle
            .state
            .with(|state| state.as_ref().map_or(0, |state| state.auto_lock_minutes))
    };
    view()
        .child(
            ui::static_row(move || i18n.t(Key::NshAutomaticLock)).child(
                view().w_40().child(
                    ui::select()
                        .items(move || {
                            [0_u32, 1, 5, 60, 300]
                                .into_iter()
                                .map(|minutes| {
                                    SelectItem::new(minutes.to_string(), duration(i18n, minutes))
                                })
                                .collect::<Vec<_>>()
                        })
                        .value(move || Some(SharedString::from(minutes().to_string())))
                        .disabled(handle.busy)
                        .accessibility_label(move || Some(i18n.t(Key::NshAutomaticLock).into()))
                        .trigger(ui::select_trigger(move || duration(i18n, minutes())))
                        .on_change(move |next: &Option<SharedString>| {
                            if let Some(minutes) =
                                next.as_ref().and_then(|next| next.parse::<u32>().ok())
                            {
                                let client = handle.client();
                                handle.command(async move {
                                    client.set_auto_lock_minutes(minutes).await
                                });
                            }
                        }),
                ),
            ),
        )
        .child(parts::note(move || i18n.t(Key::NshAutomaticLockDescription)).pb(ui::space_sm()))
        .child(ui::toggle_row(
            move || i18n.t(Key::NshBiometricUnlock),
            move || {
                handle
                    .state
                    .with(|state| state.as_ref().is_some_and(|state| state.biometric_enabled))
            },
            move || handle.busy.get(),
            move |enabled| {
                let client = handle.client();
                handle.command(async move { client.set_biometric_enabled(enabled).await });
            },
        ))
        .child(
            ui::settings_row(None, move || i18n.t(Key::NshLockNow))
                .disabled(handle.busy)
                .on_press(move || handle.lock()),
        )
}

fn duration(i18n: i18n::I18n, minutes: u32) -> String {
    if minutes == 0 {
        i18n.t(Key::Disabled).into()
    } else if minutes < 60 {
        i18n.format(Key::NshMinutesMin, &[("minutes", &minutes.to_string())])
    } else {
        i18n.format(Key::NshHoursH, &[("hours", &(minutes / 60).to_string())])
    }
}

fn setup_card(handle: Handle, setup: Signal<bool>, presence: crate::motion::Presence) -> View {
    let i18n = i18n::current();
    let first = signal(None::<String>);
    let draft = signal(String::new());
    let mismatch = signal(false);
    let submit = move || {
        let value = draft.get_untracked();
        if !presence.interactive() || value.len() != 6 || handle.busy.get_untracked() {
            return;
        }
        match first.get_untracked() {
            None => {
                first.set(Some(value));
                draft.set(String::new());
            }
            Some(previous) if previous == value => {
                first.set(None);
                draft.set(String::new());
                let client = handle.client();
                handle.command(async move { client.set_passcode(value).await });
            }
            Some(_) => {
                first.set(None);
                draft.set(String::new());
                mismatch.set(true);
            }
        }
    };
    let cancel = move || {
        if !handle.busy.get_untracked() {
            setup.set(false);
        }
    };
    ui::dialog_card(ui::DialogWidth::Compact)
        .child(ui::dialog_title(move || {
            if first.with(Option::is_some) {
                i18n.t(Key::NshConfirmYourPasscode)
            } else {
                i18n.t(Key::NshSetASixDigitPasscode)
            }
        }))
        .child(passcode::input(
            draft,
            move || handle.busy.get(),
            move || {
                mismatch.set(false);
                handle.failed.set(false);
            },
            move |_| submit(),
        ))
        .child(when(mismatch, move || {
            ui::error_text(move || i18n.t(Key::NshThePasscodesDidnTMatchTryAgain))
        }))
        .child(when(handle.failed, move || {
            ui::error_text(move || i18n.t(Key::NshCouldnTCompleteThisActionTryAgain))
        }))
        .child(
            parts::dialog_actions()
                .child(ui::action(move || i18n.t(Key::Cancel), cancel).disabled(handle.busy)),
        )
}

#[cfg(debug_assertions)]
pub(super) fn preview_setup(handle: Handle) -> impl IntoElement {
    setup_card(
        handle,
        signal(true),
        crate::motion::Presence::modal(|| true),
    )
}

fn setup_dialog(
    handle: Handle,
    setup: Signal<bool>,
    presence: crate::motion::Presence,
) -> impl IntoElement {
    ui::dialog(
        presence,
        move || {
            if !handle.busy.get_untracked() {
                setup.set(false);
            }
        },
        setup_card(handle, setup, presence),
    )
}
