use std::{sync::Arc, time::Duration};

use futures::{StreamExt as _, stream};
use mixin_desktop_api::{AccountClient, MessageListView};
use nsh_core::{keymap::When, prelude::*};

use super::ui;
use crate::ui::Typography as _;
use crate::{i18n, presentation, runtime};

const PAGE_SIZE: usize = 50;

#[derive(Clone, PartialEq)]
struct Cursor {
    time: i64,
    id: String,
}

#[derive(Clone, PartialEq)]
struct Page {
    messages: Vec<MessageListView>,
    has_older: bool,
}

#[derive(Clone, PartialEq)]
struct Anchor {
    id: String,
    line: usize,
}

#[derive(Clone, PartialEq)]
struct Line {
    anchor: Anchor,
    text: String,
    heading: bool,
}

pub(super) fn screen(
    account: Arc<AccountClient>,
    conversation: String,
    title: Memo<String>,
    focused: Signal<bool>,
) -> impl IntoElement {
    let i18n = i18n::current();
    let owner = Owner::current();
    let user = account.account_id();
    let page = signal(None::<Page>);
    let cursor = signal(None::<Cursor>);
    let newer = stored(Vec::<Option<Cursor>>::new());
    let anchor = signal(None::<Anchor>);
    let can_send = signal(false);
    let draft = signal(String::new());
    let sending = signal(false);
    let feedback = signal("");
    let failed = signal(false);
    let retry = signal(0_u64);
    effect({
        let account = account.clone();
        let conversation = conversation.clone();
        move || {
            let before = cursor.get();
            retry.get();
            let account = account.clone();
            let conversation = conversation.clone();
            spawn(async move {
                let id = conversation.clone();
                let permissions = account.conversation_changes().filter_map(move |change| {
                    std::future::ready(
                        (change.reload_all || change.conversation_ids.contains(&id)).then_some(()),
                    )
                });
                let changes = runtime::throttled(
                    stream::select(
                        Box::pin(account.message_changes(Some(conversation.clone()))),
                        Box::pin(permissions),
                    ),
                    Duration::from_millis(120),
                );
                futures::pin_mut!(changes);
                loop {
                    let client = account.clone();
                    let id = conversation.clone();
                    let before = before.clone();
                    let received = runtime::run(async move {
                        let messages = client.message();
                        let chats = client.conversation();
                        futures::try_join!(
                            messages.messages(
                                id.clone(),
                                before.as_ref().map(|key| key.time),
                                before.map(|key| key.id),
                                (PAGE_SIZE + 1) as i64,
                            ),
                            chats.can_send(id),
                        )
                    })
                    .await;
                    match received {
                        Ok((mut messages, allowed)) => {
                            let has_older = messages.len() > PAGE_SIZE;
                            messages.truncate(PAGE_SIZE);
                            messages.reverse();
                            batch(|| {
                                page.set(Some(Page {
                                    messages,
                                    has_older,
                                }));
                                can_send.set(allowed);
                                failed.set(false);
                            });
                        }
                        Err(_) => batch(|| {
                            failed.set(true);
                            can_send.set(false);
                        }),
                    }
                    if changes.next().await.is_none() {
                        break;
                    }
                }
            });
        }
    });
    let lines = computed(move || {
        let width = ui::chat_columns();
        page.with(|page| {
            let Some(page) = page else { return Vec::new() };
            page.messages
                .iter()
                .flat_map(|message| {
                    let own = message.sender_id == user;
                    let sender = if own {
                        "You"
                    } else if message.sender_name.is_empty() {
                        "Unknown sender"
                    } else {
                        &message.sender_name
                    };
                    let time = chrono::DateTime::from_timestamp_micros(message.created_at_micros)
                        .map(|time| {
                            time.with_timezone(&chrono::Local)
                                .format("%m-%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_default();
                    let delivery = if own {
                        match message.status.as_str() {
                            "SENDING" => " · Sending",
                            "FAILED" => " · Failed",
                            _ => "",
                        }
                    } else {
                        ""
                    };
                    let heading = ui::single_line(&format!("{time}  {sender}{delivery}"), width);
                    let body = if message.category.ends_with("_TEXT") {
                        message.content.clone()
                    } else {
                        presentation::preview(i18n, &message.category, &message.content)
                    };
                    std::iter::once(heading)
                        .chain(ui::wrap(&body, width))
                        .enumerate()
                        .map(move |(line, text)| Line {
                            anchor: Anchor {
                                id: message.message_id.clone(),
                                line,
                            },
                            text,
                            heading: line == 0,
                        })
                })
                .collect()
        })
    });

    let latest = move || {
        newer.replace(Vec::new());
        batch(|| {
            anchor.set(None);
            if cursor.get_untracked().is_some() {
                page.set(None);
                cursor.set(None);
            }
        });
    };
    on_key_with("ctrl+end", editing_key(), move |_| {
        if focused.get_untracked() {
            latest();
        }
    });
    for (key, direction) in [("pageup", -1), ("pagedown", 1)] {
        on_key_with(key, editing_key(), move |_| {
            if !focused.get_untracked() {
                return;
            }
            let height = height();
            let (start, bottom) = lines.with_untracked(|lines| {
                (
                    start(lines, height, anchor.get_untracked().as_ref()),
                    lines.len().saturating_sub(height),
                )
            });
            if direction < 0 && start == 0 {
                let older = page.with_untracked(|page| {
                    let first = page
                        .as_ref()
                        .filter(|page| page.has_older)?
                        .messages
                        .first()?;
                    Some(Cursor {
                        time: first.created_at_micros,
                        id: first.message_id.clone(),
                    })
                });
                if let Some(older) = older {
                    newer.with(|stack| stack.push(cursor.get_untracked()));
                    batch(|| {
                        anchor.set(None);
                        page.set(None);
                        cursor.set(Some(older));
                    });
                }
            } else if direction > 0 && start == bottom {
                if let Some(next) = newer.with(Vec::pop) {
                    batch(|| {
                        // A missing id resolves to the top of the next page.
                        anchor.set(Some(Anchor {
                            id: String::new(),
                            line: 0,
                        }));
                        page.set(None);
                        cursor.set(next);
                    });
                }
            } else {
                let next = start
                    .saturating_add_signed(direction * height.saturating_sub(1).max(1) as isize);
                anchor.set(
                    lines.with_untracked(|lines| {
                        (next < bottom).then(|| lines[next].anchor.clone())
                    }),
                );
            }
        });
    }
    on_key_with("f5", editing_key(), move |_| {
        if focused.get_untracked() {
            retry.update(|revision| *revision += 1);
        }
    });

    view()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .pl(move || px(if ui::wide() { ui::COLUMN } else { 0.0 }))
        .overflow_hidden()
        .child(ui::line(move || ui::single_line(&title.get(), ui::chat_columns())).text_emphasis())
        .child(dynamic(move || {
            let height = height();
            let current = anchor.get();
            lines.with(|lines| {
                let mut view = view().flex_1().min_h_0().overflow_hidden();
                if lines.is_empty() {
                    let label = if failed.get() {
                        "Couldn't load history"
                    } else if page.with(Option::is_none) {
                        "Loading messages…"
                    } else {
                        "No messages"
                    };
                    return view.child(ui::hint(label));
                }
                let first = start(lines, height, current.as_ref());
                for line in lines.iter().skip(first).take(height) {
                    let node = ui::line(line.text.clone());
                    view = view.child(if line.heading {
                        node.text_emphasis()
                    } else {
                        node
                    });
                }
                view
            })
        }))
        .child(ui::hint(move || {
            let status = if failed.get() {
                "Couldn't refresh chat. F5 Retry"
            } else if sending.get() {
                "Queuing message…"
            } else if !can_send.get() && page.with(Option::is_some) {
                "This conversation is read-only"
            } else {
                feedback.get()
            };
            let status = if cursor.get().is_some() {
                format!("Older messages   Ctrl+End Latest   {status}")
            } else {
                status.to_string()
            };
            ui::single_line(&status, ui::chat_columns())
        }))
        .child(dynamic(move || {
            if !focused.get() || !can_send.get() {
                return ui::line("").into_element();
            }
            let client = account.clone();
            let conversation = conversation.clone();
            ui::editor("Message", draft, move |text: &str| {
                if !focused.get_untracked()
                    || !can_send.get_untracked()
                    || sending.get_untracked()
                    || text.trim().is_empty()
                    || !draft.with_untracked(|current| current == text)
                {
                    return;
                }
                let client = client.clone();
                let conversation = conversation.clone();
                let submitted = text.to_owned();
                let outgoing = submitted.clone();
                sending.set(true);
                feedback.set("");
                owner.run(|| {
                    spawn(async move {
                        let result = runtime::commit(async move {
                            client
                                .message()
                                .send_text(conversation, outgoing, None, false)
                                .await
                        })
                        .await;
                        sending.set(false);
                        match result {
                            Ok(_) => {
                                if draft.with_untracked(|value| *value == submitted) {
                                    draft.set(String::new());
                                }
                                feedback.set("Message queued");
                                // Refresh the tail without waiting for the subscription.
                                latest();
                                retry.update(|revision| *revision += 1);
                            }
                            Err(_) => feedback.set("Couldn't send. Enter Retry"),
                        }
                    })
                });
            })
            .into_element()
        }))
}

fn height() -> usize {
    ui::rows().saturating_sub(5)
}

fn start(lines: &[Line], height: usize, anchor: Option<&Anchor>) -> usize {
    let bottom = lines.len().saturating_sub(height);
    anchor.map_or(bottom, |anchor| {
        lines
            .iter()
            .position(|line| line.anchor == *anchor)
            .or_else(|| lines.iter().rposition(|line| line.anchor.id == anchor.id))
            .unwrap_or(0)
            .min(bottom)
    })
}

fn editing_key() -> KeyOptions {
    KeyOptions {
        when: When::Always,
        repeat: true,
        ..Default::default()
    }
}
