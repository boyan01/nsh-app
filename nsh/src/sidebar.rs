use futures::StreamExt as _;
use mixin_desktop_api::{AccountClient, AccountProfile, ConversationUnseenCount};
use nsh_core::{animation::Easing, element::AnimatedValue, prelude::*};
use std::sync::Arc;

use crate::{
    i18n::{self, Key},
    runtime,
    ui::{self, Typography as _},
};

mod circle_editor;
mod circles;
mod preference;

pub(crate) use circles::{Circles, add_to_circle_item, remove_from_circle_item};

use crate::navigation::Navigation;
use crate::terminal::{self, COLUMN, ROW};

fn rail_width(nav: Navigation) -> Points {
    if terminal::active() && !nav.drawer() {
        px(18.0 * COLUMN)
    } else {
        nav.width()
    }
}

fn rail_button(button: nsh_base::ButtonElement, reference: ElementRef) -> nsh_base::ButtonElement {
    button.when(terminal::active(), |row| {
        let focused = signal(false);
        reference.on_focus_in(move || focused.set(true));
        reference.on_focus_out(move || focused.set(false));
        row.element_ref(&reference)
            .child(
                text(">")
                    .w(px(COLUMN))
                    .style_when(move || !focused.get(), |s| s.invisible()),
            )
            .w_full()
            .min_h(px(ROW))
            .px(px(COLUMN))
            .py_0()
            .gap(px(COLUMN))
            .focus(|s| s.bg(ui::fg_primary).text_color(ui::surface_rail))
            .selected_style(|s| s.bg(ui::accent).text_color(ui::fg_on_accent))
    })
}

pub(crate) fn screen(
    account: Arc<AccountClient>,
    nav: Navigation,
    circles: Circles,
) -> impl IntoElement {
    let profile = signal_with(account.profile(), SignalEquals::Never);
    let counts = signal(Vec::<ConversationUnseenCount>::new());
    let client = account.clone();
    spawn(async move {
        let mut stream = Box::pin(runtime::observe(client.profile_changes()));
        while let Some(value) = stream.next().await {
            profile.set(value);
        }
    });
    spawn(async move {
        let mut stream = Box::pin(runtime::observe(account.unseen_count_changes()));
        while let Some(value) = stream.next().await {
            counts.set(value);
        }
    });
    let rail = Rail {
        nav,
        profile,
        counts,
        labels: Labels::new(nav),
        services: Some(Services {
            circles,
            preference: preference::Preference::new(nav),
        }),
    };
    rail.surface_with_feedback(view().hidden())
        .child(circles.surface())
}

pub(crate) fn trigger(nav: Navigation) -> impl IntoElement {
    let i18n = i18n::current();
    when(
        move || nav.drawer(),
        move || {
            ui::glass_button(
                ui::round_button()
                    .on_press(move || nav.open_drawer())
                    .accessibility_label(move || Some(i18n.t(Key::NshOpenNavigation).into()))
                    .child(ui::icon(ui::Icon::Menu).when(terminal::active(), |s| s.hidden()))
                    .child(when(terminal::active, || text("F6"))),
            )
        },
    )
}

// Labels share one native animation; changing the rail width never animates layout.
#[derive(Clone, Copy)]
pub(super) struct Labels {
    visible: Signal<bool>,
    opacity: AnimatedValue,
}

impl Labels {
    fn new(nav: Navigation) -> Self {
        let labels = Self {
            visible: signal(terminal::active() || !nav.compact()),
            opacity: animated_value(if !terminal::active() && nav.compact() {
                0.0
            } else {
                1.0
            }),
        };
        let generation = stored(0_u64);
        let compact = computed(move || !terminal::active() && nav.compact());
        let layout = computed(move || (nav.drawer(), nav.forced_compact()));
        let previous_layout = stored(layout.get());
        effect(move || {
            let compact = compact.get();
            let layout = layout.get();
            let reduced_motion = reduce_motion();
            let immediate = previous_layout.replace(layout) != layout || reduced_motion;
            untracked(|| {
                let change = generation.get().wrapping_add(1);
                generation.replace(change);
                // Window breakpoints change width immediately. Settle labels in the
                // same pass and invalidate an earlier collapse animation's completion.
                if immediate {
                    labels.opacity.set(if compact { 0.0 } else { 1.0 });
                    labels.visible.set(!compact);
                    return;
                }
                if !compact {
                    labels.visible.set(true);
                }
                let end = labels.opacity.timing(
                    if compact { 0.0 } else { 1.0 },
                    crate::motion::duration(if compact {
                        crate::motion::RAIL_LABELS_EXIT_MS
                    } else {
                        crate::motion::RAIL_LABELS_ENTER_MS
                    }),
                    Easing::EaseOut,
                );
                if compact {
                    spawn(async move {
                        if end.await && generation.get() == change {
                            labels.visible.set(false);
                        }
                    });
                }
            });
        });
        labels
    }

    fn bind(self, content: View) -> View {
        content
            .bind_opacity(&self.opacity)
            .style_when(move || !self.visible.get(), |style| style.hidden())
    }

    fn bind_text(self, label: Text) -> Text {
        label
            .text_role(ui::TextRole::Body)
            .flex_1()
            .number_of_lines(1)
            .bind_opacity(&self.opacity)
            .style_when(move || !self.visible.get(), |style| style.hidden())
    }
}

/// The account's circles and persisted Collapse state.
#[derive(Clone, Copy)]
struct Services {
    circles: Circles,
    preference: preference::Preference,
}

/// Without services the rail lists no circles and Collapse is not persisted.
#[derive(Clone, Copy)]
pub(crate) struct Rail {
    nav: Navigation,
    profile: Signal<AccountProfile>,
    counts: Signal<Vec<ConversationUnseenCount>>,
    labels: Labels,
    services: Option<Services>,
}

impl Rail {
    pub(crate) fn surface_with_feedback(self, feedback: impl IntoElement) -> View {
        let nav = self.nav;
        effect(move || {
            if !nav.drawer() {
                nav.close_drawer();
            }
        });
        view()
            .w(move || rail_width(nav))
            .flex_shrink_0()
            .child(
                self.content_with_feedback(feedback)
                    .style_when(move || nav.drawer(), |s| s.hidden())
                    .accessibility_elements_hidden(move || nav.drawer()),
            )
            // Only the modal changes owners at the breakpoint; the rail keeps its images.
            .child(when(move || nav.drawer(), move || self.drawer()))
    }

    /// A rail over the given sources, without account services, for the preview.
    #[cfg(debug_assertions)]
    pub(crate) fn detached(
        nav: Navigation,
        profile: Signal<AccountProfile>,
        counts: Signal<Vec<ConversationUnseenCount>>,
    ) -> Self {
        Self {
            nav,
            profile,
            counts,
            labels: Labels::new(nav),
            services: None,
        }
    }

    fn drawer(self) -> impl IntoElement {
        let nav = self.nav;
        let presence = crate::motion::Presence::panel(move || nav.drawer_open());
        when(
            move || {
                if terminal::active() {
                    nav.drawer_open()
                } else {
                    presence.mounted()
                }
            },
            move || {
                ui::modal(move || nav.close_drawer())
                    .animation(nsh_core::ModalAnimation::None)
                    .close_on_escape(false)
                    .close_on_backdrop_press(false)
                    .flex_1()
                    .child(
                        view()
                            .flex_1()
                            .flex_row()
                            .child(
                                presence.backdrop(
                                    ui::scrim()
                                        .on_press(move || nav.close_drawer())
                                        .when(terminal::active(), |v| v.focusable(false)),
                                ),
                            )
                            .child(presence.surface(self.content())),
                    )
            },
        )
    }

    pub(crate) fn content(self) -> View {
        self.content_with_feedback(view().hidden())
    }

    pub(crate) fn content_with_feedback(self, feedback: impl IntoElement) -> View {
        let nav = self.nav;
        let content = view()
            .w(move || {
                if nav.drawer() {
                    ui::navigation_width()
                } else {
                    rail_width(nav)
                }
            })
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .bg(ui::surface_rail)
            .px_3()
            .pb_2()
            .gap_1()
            .when(terminal::active(), |v| {
                v.flex_1().px(px(COLUMN)).py(px(ROW)).gap_0()
            })
            .child(ui::title_bar().when(terminal::active(), |v| v.hidden()))
            .child(self.account_item())
            .child(view().h_2().when(terminal::active(), |v| v.h(px(ROW))))
            .child(self.category(ui::Icon::Chat, Key::NshAllChats, "chats"))
            .child(ui::divider())
            .child(self.category(ui::Icon::Contacts, Key::ContactTitle, "contacts"))
            .child(self.category(ui::Icon::Groups, Key::Groups, "groups"))
            .child(self.category(ui::Icon::Bot, Key::BotsTitle, "bots"))
            .child(self.category(ui::Icon::Strangers, Key::Strangers, "strangers"))
            .child(match self.services {
                Some(services) => services
                    .circles
                    .list(self.counts, self.labels)
                    .into_element(),
                None => view().flex_1().min_h_0().into_element(),
            })
            .child(when(terminal::active, move || {
                text(move || {
                    if nav.drawer() {
                        "↑↓  Enter  Esc"
                    } else {
                        "F6  ↑↓  Enter"
                    }
                })
                .px(px(COLUMN))
                .number_of_lines(1)
                .text_color(ui::fg_secondary)
            }))
            .child(self.collapse_item())
            .child(self.services.map(|services| {
                view()
                    .child(services.preference.feedback())
                    .child(services.circles.feedback())
            }))
            .child(feedback);
        if terminal::active() {
            // The PTY moves focus spatially, including into virtual circle rows.
            view()
                .h_full()
                .child(content.basis(move || window().height - px(ROW)))
        } else {
            content
        }
    }

    fn account_item(self) -> impl IntoElement {
        let i18n = i18n::current();
        let Self {
            nav,
            profile,
            labels,
            ..
        } = self;
        let entry = element_ref();
        if terminal::active() {
            on_key("ctrl+b", move |_| {
                if nav.drawer() {
                    nav.open_drawer();
                } else {
                    entry.focus();
                }
            });
            if nav.drawer_open() {
                after_answer(move || entry.focus());
            }
        }
        // Inset so its avatar centers on the rows' icon column, also when collapsed.
        ui::card().child(
            rail_button(
                ui::card_row()
                    .min_h_11()
                    .px(ui::space_xs())
                    .py(ui::space_xs()),
                entry,
            )
            .selected(nav.settings)
            .on_press(move || {
                batch(|| {
                    nav.settings.set(true);
                    nav.close_drawer();
                })
            })
            .accessibility_label(move || Some(i18n.t(Key::NshSettings).into()))
            .child(when(
                || !terminal::active(),
                move || {
                    ui::avatar_view(
                        move || profile.with(|p| p.user_id.clone()),
                        move || profile.with(|p| p.full_name.clone().into()),
                        move || profile.with(|p| p.avatar_url.clone()),
                        ui::AvatarSize::Compact,
                    )
                },
            ))
            .child(
                labels
                    .bind(view().flex_1().min_w_0())
                    .child(
                        text(move || {
                            if terminal::active() {
                                i18n.t(Key::NshSettings).into()
                            } else {
                                profile.with(|p| p.full_name.clone())
                            }
                        })
                        .text_role(ui::TextRole::Body)
                        .text_emphasis()
                        .number_of_lines(1),
                    )
                    .child(
                        text(move || profile.with(|p| p.identity_number.clone()))
                            .when(terminal::active(), |s| s.hidden())
                            .text_role(ui::TextRole::Caption)
                            .number_of_lines(1)
                            .text_color(ui::fg_secondary),
                    ),
            ),
        )
    }

    fn category(self, icon: ui::Icon, label: Key, value: &'static str) -> impl IntoElement {
        let i18n = i18n::current();
        let Self {
            nav,
            counts,
            labels,
            ..
        } = self;
        let unseen = computed(move || {
            counts.with(|counts| {
                counts
                    .iter()
                    .find(|count| count.category == value && count.circle_id.is_none())
                    .map_or((0, 0), |count| (count.count, count.muted_count))
            })
        });
        let count = move || unseen.get().0;
        let muted = move || unseen.get().1;
        let item = rail_button(ui::nav_row(), element_ref())
            .accessibility_label(move || Some(i18n.t(label).into()))
            .selected(move || !nav.settings.get() && nav.category.get() == value)
            .on_press(move || nav.select(value, None))
            .child(navigation_icon(ui::icon(icon), nav, count, muted))
            .child(labels.bind_text(text(move || i18n.t(label))))
            .child(when(
                move || count() > 0 && (terminal::active() || !nav.compact()),
                move || ui::count_pill(move || count().to_string(), || false),
            ));
        view()
            .w_full()
            .tooltip(
                ui::hint(move || {
                    if count() > 0 {
                        format!("{} ({})", i18n.t(label), count())
                    } else {
                        i18n.t(label).into()
                    }
                })
                .disabled(move || !nav.compact()),
            )
            .child(item.w_full())
    }

    fn collapse_item(self) -> impl IntoElement {
        let i18n = i18n::current();
        let Self {
            nav,
            services,
            labels,
            ..
        } = self;
        let preference = services.map(|services| services.preference);
        ui::nav_row()
            .disabled(move || terminal::active() || preference.is_some_and(|p| p.disabled()))
            .accessibility_label(move || {
                Some(
                    i18n.t(if nav.compact() {
                        Key::NshExpand
                    } else {
                        Key::Collapse
                    })
                    .into(),
                )
            })
            .style_when(move || nav.forced_compact(), |style| style.hidden())
            .on_press(move || {
                if let Some(preference) = preference {
                    preference.toggle();
                } else {
                    nav.animate_width();
                    nav.collapsed.update(|collapsed| *collapsed = !*collapsed);
                }
            })
            .child(dynamic(move || {
                ui::icon(if nav.compact() {
                    ui::Icon::Expand
                } else {
                    ui::Icon::Collapse
                })
            }))
            .child(labels.bind_text(text(move || i18n.t(Key::Collapse))))
    }
}

fn navigation_icon(
    icon: Svg,
    nav: Navigation,
    count: impl Fn() -> i64 + Copy + 'static,
    muted: impl Fn() -> i64 + Copy + 'static,
) -> View {
    view()
        .relative()
        .size_6()
        .when(terminal::active(), |v| v.hidden())
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .child(icon)
        .child(when(
            move || nav.compact() && count() > 0,
            move || {
                view()
                    .absolute()
                    .top_0()
                    .right_0()
                    .size_1p5()
                    .rounded(ui::radius_full())
                    .bg(move || {
                        if count() == muted() {
                            ui::fg_secondary()
                        } else {
                            ui::danger()
                        }
                    })
            },
        ))
}

#[cfg(debug_assertions)]
pub(crate) use {circle_editor::preview as preview_circle_editor, circles::preview_rename};

#[cfg(debug_assertions)]
pub(crate) use preference::preference_error;
