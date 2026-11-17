//! Shared Bot / webpage chrome, including history, immersive and error states.
use super::Actions;
use crate::appearance::IdentityBadgeKind as Badge;
use nsh_core::prelude::*;

pub(super) const ACTIONS: &[&str] = &["Back", "Immersive", "Error", "Verified", "Member", "Reset"];

fn scene(actions: Actions, bot: bool) -> View {
    let immersive = signal(false);
    let back = signal(false);
    let failed = signal(false);
    let badge = signal(Badge::Bot);
    actions.on(
        "Verified",
        move || badge.set(Badge::Bot),
        move || badge.set(Badge::Verified),
    );
    actions.on(
        "Member",
        move || badge.set(Badge::Verified),
        move || badge.set(Badge::Standard),
    );
    actions.on("Back", move || back.set(false), move || back.set(true));
    actions.on(
        "Immersive",
        move || immersive.set(false),
        move || immersive.set(true),
    );
    actions.on("Error", move || failed.set(false), move || failed.set(true));
    actions.on(
        "Reset",
        move || {
            immersive.set(true);
            back.set(true);
            failed.set(true);
        },
        move || {
            immersive.set(false);
            back.set(false);
            failed.set(false);
            badge.set(Badge::Bot);
        },
    );
    crate::bot::preview_surface(bot, immersive, back, failed, badge)
}
pub(super) fn bot(actions: Actions) -> View {
    scene(actions, true)
}
pub(super) fn webpage(actions: Actions) -> View {
    scene(actions, false)
}
