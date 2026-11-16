//! The production conversation chrome over fixed rows and account feedback.

use crate::i18n::Key;
use mixin_desktop_api::ConversationListData;
use nsh_core::prelude::*;

use super::{Actions, fixtures};
use crate::{home, navigation::Navigation, ui};

pub(super) const ACTIONS: &[&str] = &[
    "Unread",
    "Select",
    "Scroll",
    "Search text",
    "Composition",
    "Connecting",
    "Health",
    "Both notices",
    "Error",
    "Command error",
    "Empty",
    "Loading",
    "Reset",
];

pub(super) fn scene(actions: Actions) -> View {
    let rows = signal(
        fixtures::conversations::stress_rows()
            .into_iter()
            .take(20)
            .collect(),
    );
    column(rows, signal(None), element_ref(), Some(actions))
}

pub(super) fn column(
    rows: Signal<Vec<ConversationListData>>,
    selected: Signal<Option<ConversationListData>>,
    list: ElementRef,
    actions: Option<Actions>,
) -> View {
    super::messages::provide_services();
    let nav = use_context::<Navigation>().expect("the preview provides navigation");
    let navigation = crate::navigation::ChatNavigation::new(selected, signal(None), nav.settings);
    provide_context(navigation);
    let query = signal(String::new());
    let composing = signal(false);
    let unread = signal(false);
    let online = signal(true);
    let health = signal("ready".to_owned());
    let height = signal(0.0_f32);
    let state = signal("Normal");
    if let Some(actions) = actions {
        let reset = move || {
            batch(|| {
                state.set("Normal");
                unread.set(false);
                online.set(true);
                health.set("ready".into());
                query.set(String::new());
                composing.set(false);
                selected.set(None);
            });
            list.scroll_to_offset(Points::ZERO, false);
        };
        for &name in ACTIONS {
            actions.on(name, reset, move || match name {
                "Unread" => unread.set(true),
                "Select" => {
                    if let Some(row) = rows.with(|rows| rows.get(1).cloned()) {
                        navigation.select(row);
                    }
                }
                "Scroll" => list.scroll_to_index(10, percentage(0.0), Points::ZERO, true),
                "Search text" | "Composition" => {
                    query.set("设计".into());
                    composing.set(name == "Composition");
                }
                "Connecting" => online.set(false),
                "Health" => health.set("time_inaccurate".into()),
                "Both notices" => batch(|| {
                    online.set(false);
                    health.set("time_inaccurate".into());
                }),
                "Reset" => reset(),
                name => state.set(name),
            });
        }
    }
    let empty = move || matches!(state.get(), "Empty" | "Loading");
    view()
        .flex_1()
        .flex_row()
        .bg(ui::surface_pane)
        .child(
            view()
                .w(move || nav.width())
                .flex_shrink_0()
                .style_when(move || nav.drawer(), |s| s.hidden())
                .child(
                    crate::sidebar::Rail::detached(
                        nav,
                        signal_with(fixtures::profile(), SignalEquals::Never),
                        signal(fixtures::unseen_counts()),
                    )
                    .content(),
                ),
        )
        .child(
            view()
                .w(ui::sidebar_width)
                .style_when(move || nav.drawer(), |s| s.flex_1().min_w_0())
                .flex_shrink_0()
                .min_h_0()
                .relative()
                .overflow_hidden()
                .bg(ui::surface_list)
                .child(
                    crate::conversation_list::detached(
                        fixtures::USER_ID,
                        rows,
                        selected,
                        list,
                        move || px(height.get()),
                    )
                    .style_when(empty, |s| s.hidden()),
                )
                .child(when(empty, move || {
                    view().flex_1().pt(move || px(height.get())).child(
                        when(
                            move || state.get() == "Loading",
                            || ui::placeholder(|| crate::i18n::t(Key::NshLoadingConversations)),
                        )
                        .otherwise(move || {
                            home::empty_filtered(crate::i18n::current(), nav, unread)
                        }),
                    )
                }))
                .child(home::list_chrome(
                    height,
                    home::list_header(nav, query, composing, unread, element_ref(), signal(None)),
                    view()
                        .pointer_events(PointerEvents::BoxNone)
                        .child(home::health_banner(health))
                        .child(home::connection_banner(online, move || online.set(true)))
                        .child(when(
                            move || state.get() == "Error",
                            move || {
                                home::list_notice().child(
                                    home::load_error(move || state.set("Reset"), || false).p_0(),
                                )
                            },
                        ))
                        .child(when(
                            move || state.get() == "Command error",
                            || crate::conversation_list::command_error(|| {}),
                        )),
                )),
        )
        .child(
            view()
                .flex_1()
                .style_when(move || nav.drawer(), |s| s.hidden())
                .child(ui::placeholder(|| "Mixin Messenger")),
        )
}
