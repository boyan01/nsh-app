use super::*;
use crate::{appearance, presentation};
use std::collections::HashMap;

/// What a recycled cell draws; a row of the same shape only rebinds its text.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Header(Section, bool),
    Conversation,
    Item,
    Missing,
}

pub(super) fn list(
    state: query::State,
    pending: Signal<bool>,
    open: impl Fn(Row) + Copy + 'static,
) -> nsh_core::element::VirtualList {
    let rows = state.rows;
    let account = use_context::<Stored<Option<Arc<AccountClient>>>>()
        .and_then(|account| account.with(|account| account.clone()));
    let account_id = stored(
        account
            .as_ref()
            .map(|account| account.account_id())
            .unwrap_or_default(),
    );
    let i18n = i18n::current();
    let previews = signal(HashMap::<String, (String, String)>::new());
    if let Some(account) = account {
        effect(move || {
            let sources = rows.with(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        let source = account_id.with(|id| subtitle(i18n, row, id));
                        source.contains('@').then(|| (row.key(), source))
                    })
                    .collect::<Vec<_>>()
            });
            if sources.is_empty() {
                previews.set(HashMap::new());
                return;
            }
            let contents = sources.iter().map(|(_, source)| source.clone()).collect();
            let account = account.clone();
            // Owner-scoped cancellation and source matching guard query refreshes.
            spawn(async move {
                match runtime::run(async move { account.user().replace_mentions(contents).await })
                    .await
                {
                    Ok(values) => previews.set(
                        sources
                            .into_iter()
                            .zip(values)
                            .map(|((id, source), value)| (id, (source, value)))
                            .collect(),
                    ),
                    Err(error) => log::warn!("Could not resolve search mentions: {error}"),
                }
            });
        });
    }
    virtual_list(
        move || rows.with(Vec::len),
        move |at| {
            let row = move || rows.with(|rows| rows.get(at.index()).cloned());
            let shape = computed(move || {
                rows.with(|rows| match rows.get(at.index()) {
                    Some(Row::Header(section, more)) => Shape::Header(*section, *more),
                    Some(Row::Conversation(_)) => Shape::Conversation,
                    Some(_) => Shape::Item,
                    None => Shape::Missing,
                })
            });
            dynamic(move || {
                let shape = shape.get();
                untracked(|| match shape {
                    Shape::Header(section, more) => {
                        header(state, section, more, open).into_element()
                    }
                    Shape::Conversation => crate::conversation_list::row_menu(
                        use_context().expect("Home provides conversation commands"),
                        move || match row() {
                            Some(Row::Conversation(value)) => Some(*value),
                            _ => None,
                        },
                        item(row, pending, open, account_id, previews),
                    )
                    .into_element(),
                    Shape::Item => item(row, pending, open, account_id, previews).into_element(),
                    Shape::Missing => view().hidden().into_element(),
                })
            })
        },
    )
    .keys(move || rows.with(|rows| rows.iter().map(Row::key).collect::<Vec<_>>()))
    .on_load_more(move || state.load_more())
    .element_ref(&state.list)
    .estimated_item_size(ui::list_row_height())
    .flex_1()
    .min_h_0()
}

fn header(
    state: query::State,
    section: Section,
    more: bool,
    open: impl Fn(Row) + Copy + 'static,
) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .pr(ui::space_lg())
        .py(ui::space_sm())
        .gap(ui::space_sm())
        .child(ui::section_label(move || section.title()).flex_1())
        .child(when(
            move || state.section.get() == Section::All && more,
            move || {
                ui::button(ui::ButtonTone::Ghost, ui::ButtonSize::Small)
                    .on_press(move || open(Row::Header(section, more)))
                    .child(text(move || tr("More", "更多")).text_role(ui::TextRole::Caption))
            },
        ))
}

fn item(
    row: impl Fn() -> Option<Row> + Copy + 'static,
    pending: Signal<bool>,
    open: impl Fn(Row) + Copy + 'static,
    account_id: Stored<String>,
    previews: Signal<HashMap<String, (String, String)>>,
) -> impl IntoElement {
    let i18n = i18n::current();
    let icon = computed(move || row().as_ref().and_then(category_icon));
    let preview = computed(move || {
        row()
            .map(|row| {
                let source = account_id.with(|id| subtitle(i18n, &row, id));
                previews.with(|previews| {
                    previews
                        .get(&row.key())
                        .filter(|(original, _)| *original == source)
                        .map(|(_, value)| value.clone())
                        .unwrap_or(source)
                })
            })
            .unwrap_or_default()
    });
    ui::list_row()
        .disabled(pending)
        .on_press(move || {
            if let Some(value) = row() {
                open(value);
            }
        })
        .child(ui::avatar_view(
            move || row().map(|row| row.avatar_id()).unwrap_or_default(),
            move || row().map(|row| row.title()).unwrap_or_default().into(),
            move || row().map(|row| row.avatar_url()).unwrap_or_default(),
            ui::AvatarSize::Medium,
        ))
        .child(
            view()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    text(move || row().map(|row| row.title()).unwrap_or_default())
                        .text_role(ui::TextRole::Label)
                        .number_of_lines(1),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .min_w_0()
                        .child(appearance::preview_icon_reactive(icon))
                        .child(
                            text(move || preview.get())
                                .flex_1()
                                .min_w_0()
                                .text_role(ui::TextRole::Body)
                                .number_of_lines(2)
                                .text_color(ui::fg_secondary),
                        ),
                ),
        )
}

fn subtitle(i18n: i18n::I18n, row: &Row, account_id: &str) -> String {
    match row {
        Row::User(user) => user.identity_number.clone(),
        Row::Conversation(row) => presentation::search_conversation_preview(i18n, row, account_id),
        Row::Message(message) => presentation::search_message_preview(i18n, message, account_id),
        Row::Header(..) => String::new(),
    }
}

fn category_icon(row: &Row) -> Option<presentation::PreviewIcon> {
    match row {
        Row::Conversation(row) => presentation::preview_icon(
            row.last_message_status.as_deref(),
            row.last_message_category.as_deref().unwrap_or_default(),
            false,
        ),
        Row::Message(message) => {
            presentation::preview_icon(Some(&message.status), &message.category, false)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
