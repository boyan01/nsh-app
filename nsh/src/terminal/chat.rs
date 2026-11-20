use std::{sync::Arc, time::Duration};

use futures::StreamExt as _;
use mixin_desktop_api::{AccountClient, ConversationListData};
use nsh_core::{keymap::When, prelude::*};

use super::{history, ui};
use crate::runtime;
use crate::ui::Typography as _;

pub(super) fn screen(account: Arc<AccountClient>) -> impl IntoElement {
    let conversations = signal(Vec::<ConversationListData>::new());
    let selected = signal(String::new());
    let open = signal(String::new());
    let focused = signal(false);
    let status = signal("Loading conversations…");
    let online = signal(false);
    let retry = signal(0_u64);
    effect({
        let account = account.clone();
        move || {
            retry.get();
            let account = account.clone();
            spawn(async move {
                let changes =
                    runtime::throttled(account.conversation_changes(), Duration::from_millis(120));
                futures::pin_mut!(changes);
                loop {
                    let list = account.conversation();
                    match runtime::run(async move { list.conversation_items().await }).await {
                        Ok(next) => {
                            batch(|| {
                                let id = selected.get_untracked();
                                if !next.iter().any(|row| row.conversation_id == id) {
                                    selected.set(next.first().map_or_else(String::new, |row| {
                                        row.conversation_id.clone()
                                    }));
                                }
                                if !open.with_untracked(|id| {
                                    next.iter().any(|row| row.conversation_id == *id)
                                }) {
                                    open.set(String::new());
                                    focused.set(false);
                                }
                                status.set(if next.is_empty() {
                                    "No conversations"
                                } else {
                                    ""
                                });
                                conversations.set(next);
                            })
                        }
                        Err(_) => status.set("Couldn't load conversations. R Retry"),
                    }
                    if changes.next().await.is_none() {
                        break;
                    }
                }
            });
        }
    });
    let connection = account.connection_status();
    spawn(async move {
        let connection = runtime::observe(connection);
        futures::pin_mut!(connection);
        while let Some(connected) = connection.next().await {
            online.set(connected);
        }
        online.set(false);
    });

    for (key, direction) in [("up", -1), ("down", 1)] {
        on_key_with(
            key,
            KeyOptions {
                when: When::NotEditing,
                repeat: true,
                ..Default::default()
            },
            move |_| {
                if focused.get_untracked() {
                    return;
                }
                let id = selected.get_untracked();
                let next = conversations.with_untracked(|rows| {
                    let at = index(rows, &id);
                    rows.get(
                        at.saturating_add_signed(direction)
                            .min(rows.len().saturating_sub(1)),
                    )
                    .map(|row| row.conversation_id.clone())
                });
                if let Some(next) = next {
                    selected.set(next);
                }
            },
        );
    }
    on_key_with(
        "enter",
        KeyOptions {
            when: When::NotEditing,
            ..Default::default()
        },
        move |_| {
            if !focused.get_untracked() && !selected.with_untracked(String::is_empty) {
                batch(|| {
                    open.set(selected.get_untracked());
                    focused.set(true);
                });
            }
        },
    );
    on_key_with(
        "escape",
        KeyOptions {
            when: When::Always,
            ..Default::default()
        },
        move |_| {
            focused.set(false);
        },
    );
    on_key_with(
        "r",
        KeyOptions {
            when: When::NotEditing,
            ..Default::default()
        },
        move |_| {
            if !focused.get_untracked() {
                retry.update(|revision| *revision += 1);
            }
        },
    );
    let title = computed(move || {
        open.with(|id| {
            conversations.with(|rows| {
                rows.iter()
                    .find(|row| row.conversation_id == *id)
                    .map_or_else(|| "Chat".to_string(), |row| row.name.clone())
            })
        })
    });
    view()
        .flex_1()
        .min_h_0()
        .child(
            ui::line(move || {
                if online.get() {
                    "Mixin  ·  Connected"
                } else {
                    "Mixin  ·  Offline"
                }
            })
            .text_emphasis(),
        )
        .child(
            view()
                .flex_1()
                .min_h_0()
                .flex_row()
                .child(
                    list(conversations, selected, status, focused)
                        .style_when(move || !ui::wide() && focused.get(), |style| style.hidden()),
                )
                .child(when(ui::wide, || {
                    ui::hint(|| "│\n".repeat(ui::rows().saturating_sub(2)))
                        .w(px(ui::COLUMN))
                        .h_full()
                }))
                .child(
                    view()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .style_when(
                            move || !ui::wide() && !focused.get(),
                            |style| style.hidden(),
                        )
                        .child(dynamic(move || {
                            let id = open.get();
                            if id.is_empty() {
                                ui::status("Select a conversation").into_element()
                            } else {
                                history::screen(account.clone(), id, title, focused).into_element()
                            }
                        })),
                ),
        )
        .child(ui::hint(move || {
            let keys = if focused.get() {
                "Enter Send   PgUp/PgDn History   Esc List   Ctrl+C Quit"
            } else {
                "↑/↓ Select   Enter Open   Ctrl+C Quit"
            };
            ui::single_line(keys, ui::columns().saturating_sub(2))
        }))
}

fn list(
    conversations: Signal<Vec<ConversationListData>>,
    selected: Signal<String>,
    status: Signal<&'static str>,
    focused: Signal<bool>,
) -> View {
    view()
        .min_h_0()
        .min_w_0()
        .overflow_hidden()
        .style_when(ui::wide, |style| {
            style
                .w(px(ui::LIST_COLUMNS as f32 * ui::COLUMN))
                .flex_shrink_0()
        })
        .style_when(|| !ui::wide(), |style| style.flex_1())
        .child(ui::line("Conversations").text_emphasis())
        .child(dynamic(move || {
            let rows_available = ui::rows().saturating_sub(3);
            let width = if ui::wide() {
                ui::LIST_COLUMNS
            } else {
                ui::columns().saturating_sub(2)
            };
            let id = selected.get();
            let list_active = !focused.get();
            conversations.with(|rows| {
                let selected = index(rows, &id);
                let start = selected
                    .saturating_sub(rows_available / 2)
                    .min(rows.len().saturating_sub(rows_available));
                let mut list = view().flex_1().min_h_0().overflow_hidden();
                let status = status.get();
                if !status.is_empty() {
                    return list.child(ui::hint(status));
                }
                for (index, row) in rows.iter().enumerate().skip(start).take(rows_available) {
                    let active = selected == index && list_active;
                    let pointer = if active { "> " } else { "  " };
                    let unread = if row.unseen_count > 0 {
                        format!(" ({})", row.unseen_count)
                    } else {
                        String::new()
                    };
                    let title = if row.name.is_empty() {
                        "Conversation"
                    } else {
                        &row.name
                    };
                    let title = ui::single_line(title, width.saturating_sub(2 + unread.len()));
                    let line = ui::line(format!("{pointer}{title}{unread}"));
                    list = list.child(if active || row.unseen_count > 0 {
                        line.text_emphasis()
                    } else {
                        line
                    });
                }
                list
            })
        }))
}

fn index(rows: &[ConversationListData], id: &str) -> usize {
    rows.iter()
        .position(|row| row.conversation_id == id)
        .unwrap_or(0)
}
