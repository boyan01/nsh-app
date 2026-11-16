use nsh_core::prelude::*;

use super::{Handle, passcode};
use crate::{
    i18n::{self, Key},
    ui::parts,
    ui::{self, Typography as _},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Hidden,
    Locked,
    Unavailable,
}

pub(crate) fn surface(handle: Handle) -> impl IntoElement {
    // A memo: rebuilding on every state change would wipe a passcode being typed.
    let mode = computed(move || {
        let pending = handle.lock_pending.get();
        handle.state.with(|state| match state {
            Some(_) if pending => Mode::Unavailable,
            Some(state) if state.locked => Mode::Locked,
            Some(_) => Mode::Hidden,
            None => Mode::Unavailable,
        })
    });
    dynamic(move || {
        let mode = mode.get();
        untracked(|| match mode {
            Mode::Unavailable => unavailable(handle).into_element(),
            Mode::Locked => lock_screen(handle).into_element(),
            Mode::Hidden => view().hidden().into_element(),
        })
    })
}

fn unavailable(handle: Handle) -> impl IntoElement {
    let i18n = i18n::current();
    view()
        .flex_1()
        .p_8()
        .gap_4()
        .items_center()
        .justify_center()
        .bg(ui::surface_pane)
        .child(ui::icon(ui::Icon::Lock).size_12())
        .child(
            text(move || {
                if handle.load_failed.get() {
                    i18n.t(Key::NshScreenPrivacyLoadFailed)
                } else {
                    if handle.lock_pending.get() {
                        i18n.t(Key::NshLockingMixin)
                    } else {
                        i18n.t(Key::NshCheckingScreenPrivacy)
                    }
                }
            })
            .text_role(ui::TextRole::Body)
            .text_center(),
        )
        .child(when(handle.load_failed, move || {
            ui::link(move || i18n.t(Key::NshTryAgain), move || handle.retry())
        }))
}

fn lock_body(handle: Handle) -> impl IntoElement {
    let i18n = i18n::current();
    let passcode = signal(String::new());
    view()
        .flex_1()
        .p_8()
        .items_center()
        .justify_center()
        .bg(ui::surface_pane)
        .child(
            view()
                .w_80()
                .max_w(relative(1.0))
                .gap(ui::space_xl())
                .items_center()
                .child(
                    ui::icon(ui::Icon::Lock)
                        .size_16()
                        .text_color(ui::fg_secondary),
                )
                .child(
                    text(move || i18n.t(Key::NshUnlockWithPasscode))
                        .text_role(ui::TextRole::Headline),
                )
                .child(passcode::input(
                    passcode,
                    move || handle.busy.get() || handle.cooldown_seconds.get() > 0,
                    move || handle.failed.set(false),
                    move |value| {
                        passcode.set(String::new());
                        handle.unlock(value);
                    },
                ))
                .child(when(handle.busy, move || {
                    parts::status(move || i18n.t(Key::NshUnlocking))
                }))
                .child(when(handle.failed, move || {
                    ui::error_text(move || {
                        i18n.t(Key::NshCouldnTUnlockCheckYourPasscodeAndTryAgain)
                    })
                    .text_center()
                }))
                .child(when(
                    move || handle.cooldown_seconds.get() > 0,
                    move || {
                        text(move || {
                            let seconds = handle.cooldown_seconds.get();
                            i18n.format(
                                Key::NshTryAgainInSecondsS,
                                &[("seconds", &seconds.to_string())],
                            )
                        })
                        .text_role(ui::TextRole::Body)
                        .text_color(ui::fg_secondary)
                    },
                ))
                .child(when(
                    move || {
                        handle.state.with(|state| {
                            state.as_ref().is_some_and(|state| {
                                state.biometric_enabled && state.biometrics_available
                            })
                        })
                    },
                    move || {
                        ui::action(
                            move || i18n.t(Key::NshUseBiometrics),
                            move || handle.authenticate(),
                        )
                        .disabled(handle.busy)
                    },
                )),
        )
}

fn lock_screen(handle: Handle) -> impl IntoElement {
    // Consume Escape without exposing the account or dismissing a covered dialog.
    ui::modal(|| {})
        .animation(nsh_core::ModalAnimation::None)
        .close_on_escape(false)
        .close_on_backdrop_press(false)
        .flex_1()
        .child(lock_body(handle))
}

#[cfg(debug_assertions)]
pub(super) fn preview_inline(handle: Handle) -> impl IntoElement {
    dynamic(move || {
        if handle.load_failed.get() {
            unavailable(handle).into_element()
        } else {
            lock_body(handle).into_element()
        }
    })
}
