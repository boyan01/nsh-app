//! Shared bot/page chrome. Native engines report presentation state, never UI.
use crate::{
    i18n::{self, Key},
    ui::{self, Typography as _},
};
use nsh_core::{element::desktop, prelude::*};
use serde_json::Value;

use super::Page;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Browser {
    pub url: String,
    /// Live document title used for sharing; the Bot identity stays in Page.
    pub title: String,
    pub can_back: bool,
    pub loading: bool,
    pub failed: bool,
    pub theme: Option<[u8; 3]>,
}
impl Browser {
    pub fn new(page: &Page) -> Self {
        Self {
            url: page.url.clone(),
            title: String::new(),
            can_back: false,
            loading: true,
            failed: false,
            theme: None,
        }
    }
    pub fn navigation(&mut self, value: &Value) {
        if let Some(url) = value.get("url").and_then(Value::as_str) {
            self.url = url.into();
        }
        if let Some(title) = value.get("title") {
            self.title = title.as_str().unwrap_or_default().into();
        }
        if let Some(back) = value.get("can_back").and_then(Value::as_bool) {
            self.can_back = back;
        }
        if let Some(loading) = value.get("loading").and_then(Value::as_bool) {
            self.loading = loading;
            if loading {
                self.failed = false;
            }
        }
    }
}

pub(super) fn theme_color(value: &Value) -> Option<[u8; 3]> {
    let channels = value.as_array().filter(|channels| channels.len() == 3)?;
    Some([
        u8::try_from(channels[0].as_u64()?).ok()?,
        u8::try_from(channels[1].as_u64()?).ok()?,
        u8::try_from(channels[2].as_u64()?).ok()?,
    ])
}

pub(super) fn title_untracked(page: &Page, document_title: &str) -> String {
    if page.app_id.is_some() || document_title.trim().is_empty() {
        page.title.clone()
    } else {
        document_title.into()
    }
}
pub(super) fn title(page: ReadSignal<Page>, browser: Signal<Browser>) -> String {
    page.with(|page| browser.with(|browser| title_untracked(page, &browser.title)))
}
pub(super) fn height() -> Points {
    px(ui::glass_bar_height().0 + 2.0 * ui::space_sm().0)
}

fn header_foreground(browser: Signal<Browser>) -> AdaptiveColor {
    let fg = ui::fg_primary();
    browser.with(|b| match b.theme {
        Some([r, g, b])
            if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 < 128000 =>
        {
            fg.dark.into()
        }
        Some(_) => fg.light.into(),
        None => fg,
    })
}

#[derive(Clone, Copy)]
pub(super) enum Action {
    Share,
    About,
    Browser,
}

fn avatar(page: ReadSignal<Page>, size: ui::AvatarSize) -> impl IntoElement {
    ui::avatar_view(
        move || page.get().app_id.unwrap_or_default(),
        move || page.get().title.into(),
        move || {
            page.with(|p| {
                p.icon_data
                    .as_ref()
                    .map(|data| format!("data:image/png;base64,{data}"))
                    .unwrap_or_default()
            })
        },
        size,
    )
}

pub(super) fn surface(
    page: ReadSignal<Page>,
    browser: Signal<Browser>,
    immersive: Signal<bool>,
    reference: ElementRef,
    focus: FocusTargets,
    action: impl Fn(Action) + Copy + 'static,
    web: impl IntoElement,
) -> View {
    let i18n = i18n::current();
    let current = desktop::current_window();
    let is_bot = page.get_untracked().app_id.is_some();
    let menu_open = signal(false);
    let FocusTargets {
        first: more_ref,
        last: close_ref,
        back: back_ref,
    } = focus;
    let close = current.clone();
    let normal_header = when(
        move || !immersive.get(),
        move || {
            view()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(height())
                .px(ui::space_md())
                .flex_row()
                .items_center()
                .gap(ui::space_sm())
                .window_drag(true)
                .bg(move || {
                    browser.with(|b| {
                        b.theme
                            .map(|[r, g, b]| {
                                rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
                                    .into()
                            })
                            .unwrap_or_else(ui::surface_pane)
                    })
                })
                .text_color(move || header_foreground(browser))
                .child(when(
                    move || browser.get().can_back,
                    move || {
                        ui::round_button()
                            .element_ref(&back_ref)
                            .text_color(move || header_foreground(browser))
                            .size(ui::glass_bar_height())
                            .accessibility_label(move || Some(i18n.t(Key::NshBack).into()))
                            .on_press(move || reference.command("back", Value::Null))
                            .child(ui::icon(ui::Icon::ArrowLeft))
                    },
                ))
                .child(when(
                    move || is_bot && !browser.get().can_back,
                    move || avatar(page, ui::AvatarSize::Bar),
                ))
                .child(
                    text(move || title(page, browser))
                        .flex_1()
                        .min_w_0()
                        .number_of_lines(1)
                        .text_role(ui::TextRole::Headline),
                )
                .child(view().w(px(ui::glass_bar_height().0 * 2.0 + ui::space_sm().0)))
        },
    );
    let menu = ui::glass_menu()
        .open(move || Some(menu_open.get()))
        .on_open_change(move |event: &nsh_base::MenuEvent| {
            if let nsh_base::MenuEvent::OpenChange(open, _) = event {
                menu_open.set(*open);
            }
        })
        .align(Align::Start)
        .margin(ui::space_sm())
        .offset(ui::space_sm())
        .trigger(
            ui::round_button()
                .element_ref(&more_ref)
                .size(ui::glass_bar_height())
                .accessibility_label(move || Some(i18n.t(Key::More).into()))
                .selected(move || menu_open.get())
                .child(ui::icon(ui::Icon::More)),
        )
        .child(dynamic(move || {
            view()
                .w(ui::bot_menu_width())
                .min_w_0()
                .child(
                    view()
                        .p(ui::space_md())
                        .flex_row()
                        .items_center()
                        .gap(ui::space_sm())
                        .child(when(is_bot, move || avatar(page, ui::AvatarSize::Small)))
                        .child(
                            view()
                                .flex_1()
                                .min_w_0()
                                .gap(ui::space_xs())
                                .child(
                                    view()
                                        .flex_row()
                                        .items_center()
                                        .gap(ui::space_xs())
                                        .child(
                                            text(move || title(page, browser))
                                                .min_w_0()
                                                .flex_shrink(1.0)
                                                .number_of_lines(1)
                                                .text_role(ui::TextRole::Body),
                                        )
                                        .child(when(is_bot, move || {
                                            dynamic(move || {
                                                page.with(|p| {
                                                    crate::appearance::identity_badge(
                                                        false,
                                                        p.is_verified,
                                                        true,
                                                        p.membership.as_deref(),
                                                    )
                                                })
                                            })
                                        })),
                                )
                                .child(
                                    text(move || {
                                        if is_bot {
                                            page.with(|p| {
                                                p.identity_number
                                                    .as_ref()
                                                    .map(|id| {
                                                        i18n.format(
                                                            Key::ContactMixinId,
                                                            &[("arg0", id)],
                                                        )
                                                    })
                                                    .unwrap_or_default()
                                            })
                                        } else {
                                            browser.get().url
                                        }
                                    })
                                    .number_of_lines(1)
                                    .text_role(ui::TextRole::Caption)
                                    .text_color(ui::fg_glass_secondary),
                                ),
                        ),
                )
                .child(ui::divider())
                .child(when(
                    move || page.get().shareable,
                    move || {
                        ui::menu_row_symbol(ui::Icon::Forward, move || i18n.t(Key::NshForward))
                            .on_select(move || action(Action::Share))
                    },
                ))
                .child(
                    ui::menu_row_symbol(ui::Icon::Rotate, move || i18n.t(Key::Refresh))
                        .on_select(move || reference.command("reload", Value::Null)),
                )
                .child(when(is_bot, move || {
                    view().child(ui::divider()).child(
                        ui::menu_row_symbol(ui::Icon::Person, move || i18n.t(Key::About))
                            .on_select(move || action(Action::About)),
                    )
                }))
                .child(when(!is_bot, move || {
                    view()
                        .child(
                            ui::menu_row_symbol(ui::Icon::Copy, move || i18n.t(Key::NshCopyLink))
                                .on_select(move || set_clipboard(browser.get_untracked().url)),
                        )
                        .child(
                            ui::menu_row_symbol(ui::Icon::Globe, move || {
                                i18n.t(Key::NshOpenInBrowser)
                            })
                            .on_select(move || action(Action::Browser)),
                        )
                }))
                .child(when(is_bot, move || {
                    view().child(ui::divider()).child(
                        ui::menu_row_symbol(ui::Icon::FullScreen, move || {
                            i18n.t(if immersive.get() {
                                Key::NshShowBotHeader
                            } else {
                                Key::NshHideBotHeader
                            })
                        })
                        .on_select(move || immersive.update(|v| *v = !*v)),
                    )
                }))
        }));
    view()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .relative()
        .child(
            view()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .mt(move || if immersive.get() { px(0.0) } else { height() })
                .child(web),
        )
        .child(normal_header)
        .child(when(
            move || immersive.get(),
            || {
                view()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(ui::space_lg())
                    .window_drag(true)
            },
        ))
        .child(
            view()
                .absolute()
                .top(ui::space_sm())
                .right(ui::space_sm())
                .child(
                    ui::glass_pill()
                        .child(menu)
                        .child(
                            view()
                                .w(ui::hairline_width())
                                .h(ui::icon_md())
                                .bg(ui::hairline),
                        )
                        .child(
                            ui::round_button()
                                .element_ref(&close_ref)
                                .size(ui::glass_bar_height())
                                .accessibility_label(move || Some(i18n.t(Key::Close).into()))
                                .on_press(move || close.close())
                                .child(ui::icon(ui::Icon::Close)),
                        ),
                ),
        )
        .child(when(
            move || browser.get().loading,
            move || {
                view()
                    .absolute()
                    .top(height())
                    .left(ui::space_md())
                    .child(ui::busy_indicator(
                        || true,
                        move || i18n.t(Key::Loading).into(),
                    ))
            },
        ))
        .child(when(
            move || browser.get().failed,
            move || {
                view()
                    .absolute()
                    .inset_0()
                    .mt(height())
                    .bg(ui::surface_pane)
                    .items_center()
                    .justify_center()
                    .p(ui::space_lg())
                    .gap(ui::space_md())
                    .child(ui::error_text(move || i18n.t(Key::NshCouldnTOpenThePage)))
                    .child(ui::action(
                        move || i18n.t(Key::Retry),
                        move || {
                            browser.update(|b| {
                                b.failed = false;
                                b.loading = true;
                            });
                            reference.command("reload", Value::Null);
                        },
                    ))
            },
        ))
}

#[derive(Clone, Copy)]
pub(super) struct FocusTargets {
    pub first: ElementRef,
    pub last: ElementRef,
    pub back: ElementRef,
}

impl FocusTargets {
    pub fn exit_browser(self, reverse: bool, immersive: bool, can_back: bool) {
        if reverse {
            self.last.focus();
        } else if !immersive && can_back {
            self.back.focus();
        } else {
            self.first.focus();
        }
    }
    pub fn new() -> Self {
        Self {
            first: element_ref(),
            last: element_ref(),
            back: element_ref(),
        }
    }
}
