use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures::StreamExt as _;
use mixin_desktop_api::{AccountClient, ConversationListData};
use nsh_core::prelude::*;

use crate::{
    chat, conversation_list, create,
    i18n::{self, I18n, Key},
    navigation::Navigation,
    playback::Playback,
    runtime, settings,
    shell::Command,
    sidebar,
    ui::{self, Typography as _},
};

/// The signed-in session's view state. Business state stays in the account.
#[derive(Clone, Copy)]
struct Home {
    owner: Owner,
    i18n: I18n,
    account: Stored<Arc<AccountClient>>,
    nav: Navigation,
    selected: Signal<Option<ConversationListData>>,
    selected_id: Memo<Option<String>>,
    locate: Signal<Option<String>>,
    drafts: Signal<HashMap<String, String>>,
    pending: Signal<HashSet<String>>,
    window: conversation_list::Window,
    query: Signal<String>,
    composing: Signal<bool>,
    unread: Signal<bool>,
    search: ElementRef,
    online: Signal<bool>,
    health: Signal<String>,
    create_action: Signal<Option<create::Action>>,
    settings_page: Signal<settings::Page>,
    settings_detail: Signal<bool>,
    /// Keeps an unsaved profile edit when Settings closes or changes pages.
    profile_draft: Signal<Option<(String, String)>>,
}

pub(crate) fn screen(
    account: Arc<AccountClient>,
    initial_security: mixin_desktop_api::ScreenLockState,
    on_signed_out: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    provide_context(crate::attachment_progress::AttachmentProgress::mount(
        &account,
    ));
    let locate = signal_with(None::<String>, SignalEquals::Never);
    let selected = signal_with(None::<ConversationListData>, SignalEquals::Never);
    let security = crate::security::mount(account.clone(), initial_security);
    provide_context(security);
    let reedit = crate::chat::reedit::Reedit::mount();
    provide_context(reedit);
    effect(move || {
        if security.blocked() {
            reedit.clear();
        }
    });
    provide_context(crate::appearance::mount());
    provide_context(crate::drafts::Drafts::mount(account.clone()));
    let transfer = crate::transfer::mount(account.clone());
    provide_context(transfer);
    let toasts = ui::Toasts::mount();
    provide_context(toasts);
    let nav = Navigation::new();
    provide_context(nav);
    let circles = sidebar::Circles::new(account.clone(), nav);
    let commands = conversation_list::Commands::new(account.clone(), selected, circles);
    provide_context(commands);
    let chat_navigation = crate::navigation::ChatNavigation::new(selected, locate, nav.settings);
    provide_context(chat_navigation);
    chat_navigation.track(account.clone());
    let links = crate::links::LinkNavigation::new(account.clone());
    provide_context(links);
    let bots = crate::bot::mount(account.clone());
    provide_context(bots);
    links.on_open_app(move |link| bots.open(link));
    let playback = crate::playback::mount(account.clone());
    provide_context(playback);
    let on_account_ended = move || {
        playback.stop();
        crate::bot::end_session(bots, on_signed_out);
    };
    let notification_account = account.clone();
    effect(move || {
        if !bots.ending() {
            // Retraction and click invalidation must not wait for a browser
            // provider's asynchronous data purge to finish or recover.
            crate::notifications::attach(notification_account.clone(), selected, chat_navigation);
        }
    });
    let query = signal(String::new());
    let composing = signal(false);
    let unread = signal(false);
    let home = Home {
        owner: Owner::current(),
        i18n: i18n::current(),
        account: stored(account.clone()),
        nav,
        selected,
        selected_id: chat_navigation.selected_id,
        locate,
        drafts: signal(HashMap::new()),
        pending: signal(HashSet::new()),
        window: conversation_list::Window::mount(account.clone(), nav, query, composing, unread),
        query,
        composing,
        unread,
        search: element_ref(),
        // The status stream reports the truth at once; do not flash a banner first.
        online: signal(true),
        health: signal(String::from("ready")),
        create_action: signal(None),
        settings_page: signal(settings::Page::Profile),
        settings_detail: signal(false),
        profile_draft: signal(None),
    };
    let search_hidden = computed(move || {
        nav.settings.get() || (home.narrow() && home.selected_id.with(Option::is_some))
    });
    effect(move || {
        if search_hidden.get() {
            home.search.blur();
            composing.set(false);
        }
    });
    let palette_open = signal(false);
    let terminal = crate::terminal::active();
    if terminal {
        home.window.follow_terminal_focus(move || {
            !nav.settings.get()
                && !nav.drawer_open()
                && !home.searching()
                && !composing.get()
                && !palette_open.get()
                && !security.blocked()
                && !bots.ending()
        });
    }
    on_key("mod+k", move |_| {
        if !security.blocked() {
            palette_open.set(true);
        }
    });
    let shell = use_context::<crate::shell::Handle>().expect("App provides the native shell");
    crate::shell::attach(
        shell,
        account.clone(),
        selected,
        move || security.blocked() || bots.ending(),
        move || security.enabled(),
        move |command| match command {
            Command::QuickSearch => palette_open.set(true),
            Command::Lock => security.lock(),
            Command::Mute | Command::Pin | Command::DeleteConversation => {
                if let Some(row) = selected.get_untracked() {
                    match command {
                        Command::Mute => commands.toggle_mute(row),
                        Command::Pin => commands.toggle_pin(row),
                        _ => commands.remove(row),
                    }
                }
            }
            Command::SearchConversation => chat_navigation
                .panel
                .set(Some(crate::navigation::Panel::Search)),
            Command::ToggleInfo => chat_navigation.panel.update(|panel| {
                *panel = if *panel == Some(crate::navigation::Panel::Info) {
                    None
                } else {
                    Some(crate::navigation::Panel::Info)
                }
            }),
            command => home.dispatch(command),
        },
    );
    home.observe_account(account, on_account_ended);
    effect(move || {
        if !nav.settings.get() {
            home.settings_detail.set(false);
            home.profile_draft.set(None);
        } else if !home.narrow() {
            home.settings_detail.set(true);
        }
    });
    view()
        .flex_1()
        .child(crate::bot::session(bots))
        .child(when(
            move || !security.blocked() && !bots.ending(),
            move || {
                view()
                    .flex_1()
                    .flex_row()
                    .child(sidebar::screen(home.client(), nav, circles))
                    .child(home.list_column(playback))
                    .child(home.detail_pane(on_account_ended))
                    .child(create::surface(home.client(), home.create_action, selected))
                    .child(crate::transfer::surface(transfer))
                    .child(conversation_list::command_surface(commands))
                    .child(crate::search::palette(home.client(), palette_open))
                    .child(links.surface())
                    .child(crate::bot::surface(bots))
            },
        ))
        .child(when(
            move || terminal && !security.blocked() && !bots.ending() && !nav.settings.get(),
            move || {
                let i18n = home.i18n;
                text(move || {
                    if home.selected_id.with(Option::is_some) {
                        format!(
                            "Enter {}   PgUp/PgDn {}   Esc {}",
                            i18n.t(Key::Send),
                            i18n.t(Key::NshHistory),
                            i18n.t(Key::NshBack)
                        )
                    } else {
                        format!(
                            "↑↓ {}   Enter {}   F10",
                            i18n.t(Key::NshConversations),
                            i18n.t(Key::Open)
                        )
                    }
                })
                .h(px(crate::terminal::ROW))
                .line_height(px(crate::terminal::ROW))
                .px(px(crate::terminal::COLUMN))
                .number_of_lines(1)
                .bg(ui::surface_list)
                .text_color(ui::fg_secondary)
            },
        ))
        .child(toasts.surface())
        .child(crate::security::surface(security))
        .child(when(
            move || bots.ending(),
            move || crate::bot::ending_surface(bots),
        ))
}

impl Home {
    fn client(self) -> Arc<AccountClient> {
        self.account.with(|account| account.clone())
    }

    fn narrow(self) -> bool {
        self.nav.narrow()
    }

    fn searching(self) -> bool {
        !self.query.with(|query| query.trim().is_empty())
    }

    fn dispatch(self, command: Command) {
        let nav = self.nav;
        let create = move |action| {
            if self.create_action.get_untracked().is_none() {
                self.create_action.set(Some(action));
            }
        };
        match command {
            Command::Preferences => nav.settings.set(true),
            Command::NewChat => create(create::Action::Chat),
            Command::NewGroup => create(create::Action::Group),
            Command::NewCircle => create(create::Action::Circle),
            Command::Transfer => batch(|| {
                self.settings_page.set(settings::Page::Transfer);
                self.settings_detail.set(true);
                nav.settings.set(true);
            }),
            Command::PreviousChat | Command::NextChat
                if !nav.settings.get_untracked() && !untracked(|| self.searching()) =>
            {
                self.window.step(matches!(command, Command::NextChat));
            }
            _ => {}
        }
    }

    fn observe_account(self, account: Arc<AccountClient>, on_account_ended: impl Fn() + 'static) {
        let client = account.clone();
        spawn(async move {
            let mut status = Box::pin(runtime::observe(client.connection_status()));
            while let Some(connected) = status.next().await {
                self.online.set(connected);
                log::info!(
                    "NSH connection state: {}",
                    if connected {
                        "connected"
                    } else {
                        "reconnecting"
                    }
                );
            }
            self.online.set(false);
        });
        spawn(async move {
            let mut updates = Box::pin(runtime::observe(account.account_health()));
            while let Some(value) = updates.next().await {
                if value == "unauthorized" {
                    on_account_ended();
                    break;
                }
                self.health.set(value);
            }
        });
    }

    fn list_column(self, playback: Playback) -> impl IntoElement {
        let Self {
            nav,
            settings_page,
            settings_detail,
            selected,
            ..
        } = self;
        let hidden = move || {
            self.narrow()
                && if nav.settings.get() {
                    settings_detail.get()
                } else {
                    selected.with(Option::is_some)
                }
        };
        view()
            .w(ui::sidebar_width)
            .flex_shrink_0()
            .min_h_0()
            .bg(ui::surface_list)
            .style_with(move |s| {
                if self.narrow() {
                    s.flex_1().min_w_0()
                } else {
                    s
                }
            })
            .style_when(hidden, |s| s.hidden())
            .accessibility_elements_hidden(hidden)
            .child(when(
                move || nav.settings.get(),
                move || {
                    settings::navigation(
                        self.client(),
                        settings_page,
                        move || self.narrow(),
                        move || settings_detail.set(true),
                        move || nav.settings.set(false),
                    )
                },
            ))
            // Stays mounted under Settings: returning keeps the list's scroll position.
            .child(
                self.conversations(playback)
                    .style_when(move || nav.settings.get(), |s| s.hidden())
                    .accessibility_elements_hidden(move || nav.settings.get()),
            )
    }

    fn conversations(self, playback: Playback) -> View {
        let Self {
            i18n,
            window,
            selected,
            ..
        } = self;
        let rows = window.rows;
        let terminal = crate::terminal::active();
        let chrome_height = signal(0.0_f32);
        let inset = move || {
            if terminal {
                Points::ZERO
            } else {
                px(chrome_height.get())
            }
        };
        let chrome = list_chrome(
            chrome_height,
            list_header(
                self.nav,
                self.query,
                self.composing,
                self.unread,
                self.search,
                self.create_action,
            ),
            view()
                .pointer_events(PointerEvents::BoxNone)
                .child(health_banner(self.health))
                .child(connection_banner(self.online, move || {
                    self.client().retry_connection()
                }))
                .child(when(
                    move || window.failed.get() && !rows.with(Vec::is_empty) && !self.searching(),
                    move || list_notice().child(load_error(move || window.retry(), || false).p_0()),
                )),
        );
        let (flow_chrome, floating_chrome) = if terminal {
            (Some(chrome), None)
        } else {
            (None, Some(chrome))
        };
        view()
            .flex_1()
            .min_h_0()
            .children(flow_chrome)
            .child(
                view()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .overflow_hidden()
                    .child(when(
                        move || self.searching(),
                        move || {
                            crate::search::screen(
                                self.client(),
                                self.query,
                                self.composing,
                                selected,
                                self.unread,
                                inset,
                            )
                        },
                    ))
                    .child(when(
                        move || rows.with(Vec::is_empty) && !self.searching(),
                        move || {
                            view()
                                .flex_1()
                                .min_h_0()
                                .pt(inset)
                                .child(when(window.loading, move || {
                                    ui::placeholder(move || i18n.t(Key::NshLoadingConversations))
                                }))
                                .child(when(window.failed, move || {
                                    load_error(move || window.retry(), || true)
                                }))
                                .child(when(
                                    move || !window.loading.get() && !window.failed.get(),
                                    move || self.list_empty(),
                                ))
                        },
                    ))
                    .child(
                        conversation_list::screen(
                            self.client(),
                            rows,
                            self.drafts,
                            selected,
                            window,
                            inset,
                        )
                        .style_when(
                            move || rows.with(Vec::is_empty) || self.searching(),
                            |s| s.hidden(),
                        ),
                    )
                    .children(floating_chrome),
            )
            .child(crate::playback::bar(playback, selected))
    }

    fn list_empty(self) -> impl IntoElement {
        let Self {
            i18n, nav, unread, ..
        } = self;
        empty_filtered(i18n, nav, unread)
    }

    fn detail_pane(self, on_account_ended: impl Fn() + Copy + 'static) -> impl IntoElement {
        let Self {
            owner,
            i18n,
            nav,
            selected,
            selected_id,
            locate,
            drafts,
            pending,
            settings_page,
            settings_detail,
            profile_draft,
            ..
        } = self;
        let chat_navigation = use_context::<crate::navigation::ChatNavigation>()
            .expect("Home provides chat navigation");
        let hidden = move || {
            self.narrow()
                && if nav.settings.get() {
                    !settings_detail.get()
                } else {
                    selected.with(Option::is_none)
                }
        };
        view()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .style_when(hidden, |s| s.hidden())
            .accessibility_elements_hidden(hidden)
            // Stays mounted under Settings: returning keeps the timeline position and draft.
            .child(
                view()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .style_when(move || nav.settings.get(), |s| s.hidden())
                    .accessibility_elements_hidden(move || nav.settings.get())
                    .child(chat::panes(
                        selected,
                        selected_id,
                        move || {
                            view()
                                .flex_1()
                                .min_h_0()
                                .bg(ui::surface_chat)
                                .child(ui::title_bar())
                                .child(ui::placeholder(move || {
                                    i18n.t(Key::NshSelectAConversationToStartMessaging)
                                }))
                                .into_element()
                        },
                        move |conversation, ready| {
                            chat::screen(
                                self.client(),
                                conversation,
                                locate,
                                drafts,
                                pending,
                                owner,
                                move || chat_navigation.close_conversation(),
                                ready,
                            )
                        },
                    )),
            )
            .child(
                view()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .style_when(move || !nav.settings.get(), |s| s.hidden())
                    .accessibility_elements_hidden(move || !nav.settings.get())
                    .child(when(
                        move || nav.settings.get(),
                        move || {
                            settings::screen(
                                self.client(),
                                settings_page,
                                profile_draft,
                                move || self.narrow(),
                                move || settings_detail.set(false),
                                on_account_ended,
                                owner,
                            )
                        },
                    )),
            )
    }
}

/// Floating controls and notices; empty gaps pass input through to the rows.
pub(crate) fn list_chrome(
    height: Signal<f32>,
    header: impl IntoElement,
    notices: impl IntoElement,
) -> View {
    let chrome =
        view()
            .pointer_events(PointerEvents::BoxNone)
            .on_layout(move |bounds: &Bounds<Points>| {
                let value = bounds.size.height.0;
                if height.get_untracked() != value {
                    height.set(value);
                }
            });
    let chrome = if crate::terminal::active() {
        chrome.flex_shrink_0()
    } else {
        chrome
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .child(ui::edge_fade(true, ui::surface_list))
    };
    chrome.child(header).child(notices)
}

pub(crate) fn list_notice() -> View {
    ui::glass(view())
        .mx_3()
        .mb_2()
        .px_3()
        .py_2()
        .rounded(ui::radius_lg())
}

pub(crate) fn list_header(
    nav: Navigation,
    query: Signal<String>,
    composing: Signal<bool>,
    unread: Signal<bool>,
    search: ElementRef,
    create_action: Signal<Option<create::Action>>,
) -> impl IntoElement {
    let i18n = i18n::current();
    ui::title_bar()
        .pl(move || ui::title_bar_leading(nav.width()))
        .pr_3()
        .py_2()
        .gap_2()
        .child(sidebar::trigger(nav))
        .child(search_field(query, composing, unread, search))
        .child(ui::glass_button(
            ui::round_button()
                .selected(unread)
                .accessibility_label(move || Some(i18n.t(Key::NshUnread).into()))
                .on_press(move || unread.update(|value| *value = !*value))
                .child(ui::icon(ui::Icon::Filter).text_color(move || {
                    if unread.get() {
                        ui::accent()
                    } else {
                        ui::fg_primary()
                    }
                })),
        ))
        .child(create::trigger(create_action))
}

fn search_field(
    query: Signal<String>,
    composing: Signal<bool>,
    unread: Signal<bool>,
    search: ElementRef,
) -> impl IntoElement {
    let i18n = i18n::current();
    ui::glass_search_pill()
        .child(
            ui::search_input()
                .element_ref(&search)
                .placeholder(move || {
                    if unread.get() {
                        i18n.t(Key::SearchUnread).to_owned()
                    } else {
                        format!("{} ({})", i18n.t(Key::Search), key_label("mod+k"))
                    }
                })
                .label(move || i18n.t(Key::Search))
                .value(move || query.get())
                .on_change(move |value: &str| query.set(value.to_owned()))
                .on_key_press(move |key: &str| {
                    if key == "Escape" && !composing.get_untracked() {
                        query.set(String::new());
                    }
                })
                .on_composition_change(move |event: &Composition| composing.set(event.composing)),
        )
        .child(when(
            move || !query.with(String::is_empty),
            move || {
                ui::round_button()
                    .size_6()
                    .disabled(composing)
                    .accessibility_label(move || Some(i18n.t(Key::Clear).into()))
                    .on_press(move || {
                        query.set(String::new());
                        search.focus();
                    })
                    .child(ui::icon(ui::Icon::Close).size_4())
            },
        ))
}

pub(crate) fn health_banner(health: Signal<String>) -> impl IntoElement {
    let i18n = i18n::current();
    when(
        move || health.get() != "ready",
        move || {
            list_notice()
                .gap_2()
                .flex_row()
                .items_center()
                .child(
                    ui::error_text(move || match health.get().as_str() {
                        "time_inaccurate" => i18n.t(Key::NshCheckYourSystemClockAndTryAgain),
                        "update_required" => i18n.t(Key::NshUpdateMixinMessengerToContinue),
                        _ => i18n.t(Key::NshTheAccountIsTemporarilyUnavailable),
                    })
                    .text_role(ui::TextRole::Caption)
                    .flex_1(),
                )
                .child(when(
                    move || health.get() == "update_required",
                    move || {
                        ui::button(ui::ButtonTone::Ghost, ui::ButtonSize::Small)
                            .on_press(|| {
                                spawn(async {
                                    if !open_url("https://mixin.one/messenger").await {
                                        log::warn!("Could not open the download page");
                                    }
                                });
                            })
                            .child(
                                text(move || i18n.t(Key::Upgrade)).text_role(ui::TextRole::Caption),
                            )
                    },
                ))
        },
    )
}

pub(crate) fn connection_banner(
    online: Signal<bool>,
    retry: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    let i18n = i18n::current();
    // The banner fades in, and the list below eases down to make room for it.
    let presence = crate::motion::Presence::fade(move || !online.get());
    let slot = element_ref();
    effect(move || {
        online.get();
        if !reduce_motion() {
            slot.animate_layout(crate::motion::PANEL_WIDTH);
        }
    });
    view().element_ref(&slot).child(when(
        move || presence.mounted(),
        move || {
            presence.surface(
                list_notice()
                    .gap_2()
                    .flex_row()
                    .items_center()
                    .child(
                        text(move || i18n.t(Key::NshReconnecting))
                            .text_role(ui::TextRole::Caption)
                            .flex_1(),
                    )
                    .child(
                        ui::button(ui::ButtonTone::Ghost, ui::ButtonSize::Small)
                            .on_press(retry)
                            .child(
                                text(move || i18n.t(Key::NshReconnect))
                                    .text_role(ui::TextRole::Caption),
                            ),
                    ),
            )
        },
    ))
}

fn empty_title(i18n: I18n, category: &str, circle: bool, unread: bool) -> &'static str {
    if unread {
        return i18n.t(Key::NshNoUnreadConversations);
    }
    if circle {
        return i18n.t(Key::NshNoConversationsInThisCircle);
    }
    match category {
        "contacts" => i18n.t(Key::NshNoContactConversations),
        "groups" => i18n.t(Key::NshNoGroupConversations),
        "bots" => i18n.t(Key::NshNoBotConversations),
        "strangers" => i18n.t(Key::NshNoStrangerConversations),
        _ => i18n.t(Key::NshNoConversationsYet),
    }
}

fn empty_body(i18n: I18n, category: &str, circle: bool, unread: bool) -> &'static str {
    match (unread, circle, category) {
        (true, true, _) => i18n.t(Key::NshThereAreNoUnreadConversationsInThisCircle),
        (true, false, "chats") => i18n.t(Key::NshYouHaveNoUnreadConversations),
        (true, false, _) => i18n.t(Key::NshThereAreNoUnreadConversationsInThisCategory),
        (false, true, _) => i18n.t(Key::NshConversationsAddedToThisCircleWillAppearHere),
        (false, false, "contacts") => i18n.t(Key::NshConversationsWithYourContactsWillAppearHere),
        (false, false, "groups") => i18n.t(Key::NshGroupConversationsWillAppearHere),
        (false, false, "bots") => i18n.t(Key::NshConversationsWithBotsWillAppearHere),
        (false, false, "strangers") => i18n.t(Key::NshStrangerConversationsEmptyDescription),
        (false, false, _) => i18n.t(Key::NshYourConversationsWillAppearHereAsTheySync),
    }
}

pub(crate) fn load_error(retry: impl Fn() + 'static, empty: impl Fn() -> bool + 'static) -> View {
    let i18n = crate::i18n::current();
    view()
        .p_4()
        .gap_3()
        .style_when(empty, |s| s.flex_1())
        .child(ui::error_text(move || {
            i18n.t(Key::NshCouldnTLoadConversations)
        }))
        .child(ui::link(move || i18n.t(Key::NshTryAgain), retry))
}

pub(crate) fn empty_filtered(
    i18n: I18n,
    nav: Navigation,
    unread: Signal<bool>,
) -> impl IntoElement {
    let scope = move || {
        let circle = nav.circle.with(Option::is_some) || nav.category.get() == "circle";
        (nav.category.get(), circle)
    };
    ui::empty(
        move || {
            let (category, circle) = scope();
            empty_title(i18n, category, circle, unread.get())
        },
        move || {
            let (category, circle) = scope();
            empty_body(i18n, category, circle, unread.get())
        },
    )
    .child(when(unread, move || {
        ui::action(
            move || i18n.t(Key::NshClearFilters),
            move || unread.set(false),
        )
    }))
}
#[cfg(debug_assertions)]
pub(crate) fn preview_empty_filtered() -> impl IntoElement {
    empty_filtered(i18n::current(), Navigation::new(), signal(true))
}
