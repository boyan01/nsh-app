use crate::i18n::Key;
use mixin_desktop_api::CircleItem;

use super::*;

pub(super) fn screen(state: State) -> impl IntoElement {
    let circles = signal(Vec::<CircleItem>::new());
    let loaded = signal(false);
    let client = state.client();
    spawn(async move {
        let mut changes = Box::pin(crate::runtime::observe(client.circle_changes()));
        while let Some(rows) = changes.next().await {
            batch(|| {
                circles.set(rows);
                loaded.set(true);
            });
        }
    });
    let name = signal(String::new());
    let created = signal(None::<String>);
    let busy = signal(false);
    let failed = signal(false);
    let create = move || {
        if busy.get_untracked() || state.busy.get_untracked() {
            return;
        }
        let Some(conversation) = state.row.get_untracked() else {
            return;
        };
        let name_value = name.get_untracked().trim().to_owned();
        if name_value.is_empty() {
            return;
        }
        busy.set(true);
        failed.set(false);
        let client = state.client();
        let existing = created.get_untracked();
        let task = crate::runtime::commit(async move {
            let access = client.conversation();
            let id = match existing {
                Some(id) => id,
                None => access.create_circle(name_value).await?.circle_id,
            };
            let added = access
                .edit_circle_conversation(
                    id.clone(),
                    conversation.conversation_id,
                    conversation.owner_id,
                    conversation.category == "GROUP",
                    true,
                )
                .await;
            Ok((id, added.is_ok()))
        });
        spawn(async move {
            let result = task.await;
            busy.set(false);
            match result {
                Ok((_, true)) => {
                    created.set(None);
                    name.set(String::new());
                    state.reload();
                }
                Ok((id, false)) => {
                    created.set(Some(id));
                    failed.set(true);
                }
                Err(_) => failed.set(true),
            }
        });
    };
    content(state, circles, loaded, name, created, busy, failed, create)
}

#[allow(clippy::too_many_arguments)]
fn content(
    state: State,
    circles: Signal<Vec<CircleItem>>,
    loaded: Signal<bool>,
    name: Signal<String>,
    created: Signal<Option<String>>,
    busy: Signal<bool>,
    failed: Signal<bool>,
    create: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    view()
        .flex_1()
        .min_h_0()
        .child(
            parts::page_body()
                .child(parts::note(move || t(Key::NshChooseCirclesForThisChat)))
                .child(
                    ui::field()
                        .label(move || t(Key::NshNewCircleName))
                        .placeholder(move || t(Key::NshNewCircleName).to_owned())
                        .max_length(64)
                        .readonly(move || busy.get() || created.with(Option::is_some))
                        .value(move || name.get())
                        .on_change(move |text: &str| name.set(text.into())),
                )
                .child(
                    ui::action(
                        move || {
                            if created.with(Option::is_some) {
                                t(Key::NshRetryAddingChat)
                            } else {
                                t(Key::NshCreateCircle)
                            }
                        },
                        create,
                    )
                    .disabled(move || {
                        busy.get() || state.busy.get() || name.with(|name| name.trim().is_empty())
                    }),
                )
                .child(when(busy, move || {
                    parts::status(move || {
                        if created.with_untracked(Option::is_some) {
                            t(Key::NshAddingChat)
                        } else {
                            t(Key::NshCreatingCircleAndAddingChat)
                        }
                    })
                }))
                .child(when(failed, move || {
                    ui::error_text(move || {
                        if created.with(Option::is_some) {
                            t(Key::NshCircleCreatedButThisChatCouldnTBeAddedRetryAddingIt)
                        } else {
                            t(Key::NshCouldnTCreateTheCircleTryAgain)
                        }
                    })
                })),
        )
        .child(
            virtual_list(
                move || circles.with(Vec::len),
                move |at| {
                    let circle = move || circles.with(|rows| rows.get(at.index()).cloned());
                    let checked = move || {
                        circle().is_some_and(|circle| {
                            state.row.with(|row| {
                                row.as_ref()
                                    .is_some_and(|row| row.circle_ids.contains(&circle.circle_id))
                            })
                        })
                    };
                    parts::compact_row()
                        .selected(checked)
                        .disabled(move || state.busy.get() || busy.get())
                        .accessibility_label(move || {
                            circle().map(|circle| {
                                format!(
                                    "{}: {}",
                                    circle.name,
                                    if checked() {
                                        t(Key::Added)
                                    } else {
                                        t(Key::NshNotAdded)
                                    }
                                )
                                .into()
                            })
                        })
                        .on_press(move || {
                            if let (Some(circle), Some(value)) =
                                (circle(), state.row.get_untracked())
                            {
                                let add = !checked();
                                let client = state.client();
                                state.submit(
                                    async move {
                                        client
                                            .conversation()
                                            .edit_circle_conversation(
                                                circle.circle_id,
                                                value.conversation_id,
                                                value.owner_id,
                                                value.category == "GROUP",
                                                add,
                                            )
                                            .await
                                    },
                                    || {},
                                );
                            }
                        })
                        .child(
                            text(move || circle().map(|circle| circle.name).unwrap_or_default())
                                .text_role(ui::TextRole::Label)
                                .flex_1(),
                        )
                        .child(when(checked, || ui::icon(ui::Icon::Check)))
                },
            )
            .keys(move || {
                circles.with(|rows| {
                    rows.iter()
                        .map(|circle| circle.circle_id.clone())
                        .collect::<Vec<_>>()
                })
            })
            .estimated_item_size(parts::row_height())
            .flex_1()
            .min_h_0()
            .style_when(move || circles.with(Vec::is_empty), |style| style.hidden()),
        )
        .child(when(
            move || loaded.get() && circles.with(Vec::is_empty),
            || ui::placeholder(move || t(Key::NshNoCirclesYet)),
        ))
}
#[cfg(debug_assertions)]
pub(super) fn detached(state: State) -> impl IntoElement {
    content(
        state,
        signal(vec![CircleItem {
            circle_id: "preview-circle".into(),
            name: "Design review".into(),
            conversation_count: 12,
        }]),
        signal(true),
        signal(String::new()),
        signal(None),
        signal(false),
        signal(false),
        || {},
    )
}
