use nsh_core::prelude::*;

use super::{Command, Direction, Handle, Status};
use crate::{
    i18n::{self, I18n, Key},
    ui::parts,
    ui::{self, Typography as _},
};

pub(crate) fn screen(handle: Handle) -> impl IntoElement {
    let i18n = i18n::current();
    scroll().flex_1().min_h_0().child(
        parts::page_body()
            .child(parts::note(move || {
                i18n.t(Key::NshTransferNetworkInstructions)
            }))
            .child(
                ui::card().children(
                    [Direction::Restore, Direction::Backup]
                        .into_iter()
                        .map(move |direction| transfer_row(handle, direction)),
                ),
            ),
    )
}

fn transfer_row(handle: Handle, direction: Direction) -> impl IntoElement {
    let i18n = i18n::current();
    let job = handle.job(direction);
    let icon = match direction {
        Direction::Restore => ui::Icon::Download,
        Direction::Backup => ui::Icon::Upload,
    };
    view()
        .child(
            ui::settings_row(Some(icon), move || title(i18n, direction))
                .on_press(move || {
                    handle.send(
                        direction,
                        match direction {
                            Direction::Restore => Command::PullToRemote,
                            Direction::Backup => Command::PushToRemote,
                        },
                    )
                })
                .disabled(move || {
                    handle.restore_busy.get()
                        || handle.backup_busy.get()
                        || handle.restore.get().status.active()
                        || handle.backup.get().status.active()
                }),
        )
        .child(when(
            move || job.get().status != Status::Idle,
            move || {
                view()
                    .px(ui::space_lg())
                    .pb(ui::space_md())
                    .gap(ui::space_sm())
                    .child(status(handle, direction))
                    .child(when(
                        move || job.get().status.active(),
                        move || {
                            ui::action(
                                move || i18n.t(Key::NshShowTransfer),
                                move || handle.visible.set(Some(direction)),
                            )
                        },
                    ))
            },
        ))
}

pub(crate) fn surface(handle: Handle) -> impl IntoElement {
    let current = computed(move || {
        if let Some(id) = handle.restore_request.get() {
            Some((Some(id), Direction::Restore))
        } else if let Some(id) = handle.backup_request.get() {
            Some((Some(id), Direction::Backup))
        } else {
            handle.visible.get().map(|direction| (None, direction))
        }
    });
    let shown = signal(None::<(Option<u64>, Direction)>);
    effect(move || {
        if let Some(current) = current.get() {
            shown.set(Some(current));
        }
    });
    let presence = crate::motion::Presence::modal(move || current.with(Option::is_some));
    when(
        move || presence.mounted(),
        move || {
            cleanup(move || {
                let _ = shown.try_set(None);
            });
            dynamic(move || {
                let shown = shown.get();
                untracked(|| match shown {
                    Some((Some(id), direction)) => {
                        approval(handle, direction, id, presence).into_element()
                    }
                    Some((None, direction)) => progress(handle, direction, presence).into_element(),
                    None => view().hidden().into_element(),
                })
            })
        },
    )
}

fn title(i18n: I18n, direction: Direction) -> &'static str {
    match direction {
        Direction::Restore => i18n.t(Key::NshSyncFromPhone),
        Direction::Backup => i18n.t(Key::NshSyncToPhone),
    }
}

fn approval_card(
    handle: Handle,
    direction: Direction,
    request_id: u64,
    presence: crate::motion::Presence,
) -> View {
    let i18n = i18n::current();
    let reject = move || {
        if !presence.interactive() {
            return;
        }
        handle.send_request(
            direction,
            match direction {
                Direction::Restore => Command::CancelRestoreRequest,
                Direction::Backup => Command::CancelBackupRequest,
            },
            request_id,
        )
    };
    // Only the buttons answer: a stray press outside must not refuse the request.
    ui::dialog_card(ui::DialogWidth::Regular)
        .child(ui::dialog_title(move || title(i18n, direction)))
        .child(
            text(move || match direction {
                Direction::Restore => i18n.t(Key::NshAllowYourPhoneToSendChatHistoryToThisDevice),
                Direction::Backup => i18n.t(Key::NshAllowThisDeviceToSendChatHistoryToYourPhone),
            })
            .text_role(ui::TextRole::Body),
        )
        .child(
            parts::dialog_actions()
                .child(
                    ui::action(move || i18n.t(Key::Cancel), reject)
                        .disabled(handle.busy(direction)),
                )
                .child(
                    ui::primary()
                        .disabled(handle.busy(direction))
                        .child(text(move || i18n.t(Key::NshAllow)))
                        .on_press(move || {
                            if !presence.interactive() {
                                return;
                            }
                            handle.send_request(
                                direction,
                                match direction {
                                    Direction::Restore => Command::ConfirmRestore,
                                    Direction::Backup => Command::ConfirmBackup,
                                },
                                request_id,
                            )
                        }),
                ),
        )
        .child(when(
            move || handle.response_error(direction).get() == Some(request_id),
            move || {
                ui::error_text(move || i18n.t(Key::NshCouldnTRespondToTheTransferRequestTryAgain))
            },
        ))
}

fn progress_card(handle: Handle, direction: Direction) -> View {
    let i18n = i18n::current();
    let job = handle.job(direction);
    ui::dialog_card(ui::DialogWidth::Regular)
        .child(ui::dialog_title(move || title(i18n, direction)))
        .child(status(handle, direction))
        .child(when(
            move || job.get().status.active(),
            move || {
                view()
                    .gap(ui::space_lg())
                    .child(
                        text(move || {
                            i18n.t(Key::NshKeepBothDevicesConnectedUntilTheTransferFinishes)
                        })
                        .text_role(ui::TextRole::Body)
                        .text_color(ui::fg_secondary),
                    )
                    .child(
                        ui::action(
                            move || i18n.t(Key::Cancel),
                            move || handle.cancel(direction),
                        )
                        .disabled(handle.busy(direction)),
                    )
            },
        ))
        .child(when(
            move || !job.get().status.active(),
            move || ui::action(move || i18n.t(Key::Done), move || handle.visible.set(None)),
        ))
}

fn status(handle: Handle, direction: Direction) -> impl IntoElement {
    let i18n = i18n::current();
    let job = handle.job(direction);
    view()
        .gap(ui::space_sm())
        .child(
            text(move || match job.get().status {
                Status::Idle => "",
                Status::Waiting => i18n.t(Key::NshWaitingForTheOtherDevice),
                Status::Transferring => i18n.t(Key::NshTransferringChats),
                Status::Completed => i18n.t(Key::TransferCompleted),
                Status::Failed => i18n.t(Key::NshTransferFailedCheckBothDevicesAndTryAgain),
                Status::VersionMismatch => i18n.t(Key::NshTransferVersionMismatch),
                Status::Cancelled => i18n.t(Key::NshTransferCancelled),
            })
            .text_role(ui::TextRole::Body),
        )
        .child(when(
            move || job.get().status == Status::Transferring,
            move || {
                text(move || {
                    let current = job.get();
                    format!(
                        "{:.1}% · {:.1} KB/s",
                        current.progress,
                        current.speed / 1024.0
                    )
                })
                .text_role(ui::TextRole::Body)
                .text_color(ui::fg_secondary)
            },
        ))
}

#[cfg(debug_assertions)]
pub(super) fn preview_approval(handle: Handle) -> impl IntoElement {
    approval_card(
        handle,
        Direction::Restore,
        1,
        crate::motion::Presence::modal(|| true),
    )
}

fn approval(
    handle: Handle,
    direction: Direction,
    request_id: u64,
    presence: crate::motion::Presence,
) -> impl IntoElement {
    ui::dialog(
        presence,
        || {},
        approval_card(handle, direction, request_id, presence),
    )
}
fn progress(
    handle: Handle,
    direction: Direction,
    presence: crate::motion::Presence,
) -> impl IntoElement {
    let job = handle.job(direction);
    ui::dialog(
        presence,
        move || {
            if !job.get_untracked().status.active() {
                handle.visible.set(None);
            }
        },
        progress_card(handle, direction),
    )
}
#[cfg(debug_assertions)]
pub(super) fn preview_progress(handle: Handle) -> impl IntoElement {
    progress_card(handle, Direction::Restore)
}
