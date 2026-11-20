use crate::i18n::{self, Key};
use crate::{
    qr, runtime,
    ui::{self, Typography as _},
};
use mixin_desktop_api::{ClientError, ClientResult, DesktopClient, LoginClient};
use nsh_core::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, PartialEq)]
enum LoginPhase {
    Creating,
    Waiting(SharedString, Option<u32>),
    Failed,
}

pub(crate) fn screen(
    desktop: DesktopClient,
    on_authenticated: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    let (state, retry) = mount(desktop, on_authenticated);
    content(state, retry)
}

fn mount(
    desktop: DesktopClient,
    on_authenticated: impl Fn() + Copy + 'static,
) -> (Signal<LoginPhase>, Signal<u64>) {
    let state = signal(LoginPhase::Creating);
    let retry = signal(0_u64);
    let active = Rc::new(RefCell::new(None::<Arc<LoginClient>>));
    let cancel = active.clone();
    cleanup(move || {
        if let Some(login) = cancel.borrow_mut().take() {
            login.cancel();
        }
    });
    effect(move || {
        let _revision = retry.get();
        let desktop = desktop.clone();
        let active = active.clone();
        state.set(LoginPhase::Creating);
        spawn(async move {
            match authorize(desktop, state, active).await {
                Ok(()) => on_authenticated(),
                Err(error) => {
                    log::warn!("Sign-in failed: {error}");
                    state.set(LoginPhase::Failed);
                }
            }
        });
    });
    (state, retry)
}

async fn authorize(
    desktop: DesktopClient,
    state: Signal<LoginPhase>,
    active: Rc<RefCell<Option<Arc<LoginClient>>>>,
) -> ClientResult<()> {
    let terminal = crate::terminal::active();
    loop {
        let start = desktop.clone();
        let login = runtime::run(async move { start.begin_login().await.map(Arc::new) }).await?;
        if let Some(previous) = active.replace(Some(login.clone())) {
            previous.cancel();
        }
        let url = login.auth_url();
        let (source, side) = runtime::run(async move {
            let code = if terminal {
                qr::terminal_source(&url).map(|(source, side)| (source, Some(side)))
            } else {
                qr::source(&url, 256).map(|source| (source, None))
            };
            code.ok_or_else(|| ClientError::Internal("Could not render the sign-in code".into()))
        })
        .await?;
        state.set(LoginPhase::Waiting(source.into(), side));
        log::info!("NSH sign-in QR code ready");
        match runtime::run(async move {
            let account = Arc::new(login.wait().await?);
            runtime::retain_account(account);
            Ok(())
        })
        .await
        {
            Ok(()) => {
                log::info!("NSH sign-in completed");
                // The root restores this active account and resolves privacy.
                // Post-authentication failures must retry startup, not begin_login.
                return Ok(());
            }
            Err(mixin_desktop_api::ClientError::TimedOut) => continue,
            Err(error) => {
                if error.to_string().contains("login_provisioning_error:") {
                    runtime::run(async move { desktop.abort_saved_login().await }).await?;
                }
                return Err(error);
            }
        }
    }
}

fn content(state: Signal<LoginPhase>, retry: Signal<u64>) -> impl IntoElement {
    let i18n = i18n::current();
    let terminal = crate::terminal::active();
    view()
        .flex_1()
        .bg(ui::surface_landing)
        .child(ui::title_bar())
        .child(
            view().flex_1().items_center().justify_center().p_4()
                .style_when(move || terminal, |s| s.p_0())
                .child(
                view()
                    .w(ui::login_width)
                    .max_w(relative(1.0))
                    .h(ui::login_height())
                    .bg(ui::surface_popover)
                    .rounded(ui::radius_login())
                    .shadow_lg()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .p_8()
                    .style_when(move || terminal, |s| s.w_full().h_full().p_1().gap_0())
                    .child(
                        view()
                            .style_when(move || !terminal, |s| s.size_40())
                            .rounded(ui::radius_md())
                            .overflow_hidden()
                            .items_center()
                            .justify_center()
                            .style_when(move || terminal, |s| s.w_full())
                            .child(dynamic(move || match state.get() {
                                LoginPhase::Creating => view()
                                    .size_40()
                                    .gap_3()
                                    .items_center()
                                    .justify_center()
                                    .child(ui::busy_indicator(
                                        || true,
                                        move || i18n.t(Key::NshPreparingYourCode).into(),
                                    ))
                                    .child(
                                        text(move || i18n.t(Key::NshPreparingYourCode))
                                            .text_role(ui::TextRole::Body)
                                            .text_center()
                                            .text_color(ui::fg_secondary),
                                    )
                                    .into_element(),
                                LoginPhase::Waiting(source, Some(side)) => {
                                    let width = crate::terminal::COLUMN * side as f32;
                                    let required_rows = side / 2 + 5;
                                    let rows = window().height.0 / crate::terminal::ROW;
                                    let required_columns = side + 2;
                                    if !crate::terminal::image_supported() {
                                        text("Use a 256-color terminal to scan the code, or sign in from the desktop app.")
                                            .text_center().into_element()
                                    } else if rows < required_rows as f32 || crate::terminal::columns() < required_columns as f32 {
                                        text(format!("Enlarge to {required_columns} columns × {required_rows} rows to scan the code."))
                                            .text_center().into_element()
                                    } else {
                                        image(source).size(px(width)).flex_shrink_0()
                                            .label(move || i18n.t(Key::NshScanToSignIn)).into_element()
                                    }
                                }
                                LoginPhase::Waiting(source, None) => image(source)
                                    .size_40()
                                    .label(move || i18n.t(Key::NshScanToSignIn))
                                    .on_load(|_: &Size<DevicePixels>| {
                                        log::info!(
                                            target: "nsh::mixin",
                                            "Sign-in QR image loaded"
                                        );
                                    })
                                    .into_element(),
                                LoginPhase::Failed => view()
                                    .size_40()
                                    .gap_3()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        ui::error_text(move || {
                                            i18n.t(Key::NshCouldnTCreateASignInCode)
                                        })
                                        .text_center(),
                                    )
                                    .child(ui::link(
                                        move || i18n.t(Key::NshTryAgain),
                                        move || retry.update(|v| *v += 1),
                                    ))
                                    .into_element(),
                            })),
                    )
                    .child(
                        text(move || i18n.t(Key::NshScanToSignIn)).text_role(ui::TextRole::Label),
                    )
                    .child(
                        view()
                            .gap_1()
                            .max_w_96()
                            .style_when(move || terminal, |s| s.hidden())
                            .child(
                                text(move || {
                                    format!("1. {}", i18n.t(Key::NshOpenMixinOnYourPhone))
                                })
                                .text_role(ui::TextRole::Body)
                                .text_color(ui::fg_secondary),
                            )
                            .child(
                                text(move || {
                                    format!("2. {}", i18n.t(Key::NshScanTheQRCodeAndConfirmSignIn))
                                })
                                .text_role(ui::TextRole::Body)
                                .text_color(ui::fg_secondary),
                            ),
                    ),
            ),
        )
        .child(
            view()
                .flex_row()
                .justify_between()
                .items_center()
                .px_4()
                .py_3()
                .child(i18n.picker())
                .child(
                    text(env!("CARGO_PKG_VERSION"))
                        .text_role(ui::TextRole::Body)
                        .text_color(ui::fg_secondary),
                ),
        )
}

#[cfg(debug_assertions)]
pub(crate) fn preview(failed: Signal<bool>) -> impl IntoElement {
    let state = signal(LoginPhase::Failed);
    effect(move || {
        state.set(if failed.get() {
            LoginPhase::Failed
        } else {
            LoginPhase::Waiting(
                crate::qr::source("https://example.com/preview-login", 320)
                    .expect("fixture QR")
                    .into(),
                None,
            )
        })
    });
    content(state, signal(0))
}
