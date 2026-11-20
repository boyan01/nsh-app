//! Keyboard-first presentation over the same process and account clients as the GUI.

use std::{error::Error, fs::File, path::Path};

use nsh_core::{element::desktop, prelude::*};

use crate::{Phase, i18n, login, restore_account, runtime, shell};

mod chat;
mod history;
mod host;
pub(crate) mod ui;

pub(crate) use host::image_supported;
pub use host::run;

fn prepare(directory: &Path) -> Result<File, Box<dyn Error>> {
    let lock = shell::instance::acquire_terminal(directory)?;
    mixin_desktop_api::init_file_logging(
        "Mixin".into(),
        env!("CARGO_PKG_VERSION").into(),
        "2".into(),
    )?;
    Ok(lock)
}

// The NSH development runner owns its transport. The installed host instead
// does process preflight and shutdown outside the engine.
pub(super) fn development_surface() -> AnyElement {
    let lock = mixin_desktop_api::data_directory()
        .map_err(Box::<dyn Error>::from)
        .and_then(|directory| prepare(&directory));
    match lock {
        Ok(lock) => {
            cleanup(move || drop(lock));
            desktop::on_app_quit(runtime::shutdown);
            surface().into_element()
        }
        Err(error) => ui::status(format!("Mixin startup failed: {error}")).into_element(),
    }
}

#[component]
fn TerminalApp() -> impl IntoElement {
    surface()
}

fn surface() -> impl IntoElement {
    let i18n = i18n::I18n::new();
    provide_context(i18n);
    let phase = signal_with(Phase::Launching, SignalEquals::Never);
    let error = signal(String::new());
    let retry = signal(0_u64);
    effect(move || {
        retry.get();
        phase.set(Phase::Launching);
        spawn(async move {
            let result = async {
                let desktop = runtime::run(mixin_desktop_api::DesktopClient::open()).await?;
                i18n.load(desktop.settings()).await;
                restore_account(desktop).await
            }
            .await;
            phase.set(result.unwrap_or_else(|failure| {
                error.set(failure.to_string());
                Phase::Failed
            }));
        });
    });
    view()
        .flex_1()
        .px(px(8.0))
        .child(dynamic(move || match phase.get() {
            Phase::Launching => ui::status("Opening Mixin…").into_element(),
            Phase::SignedOut(desktop) => view()
                .flex_1()
                .child(login::terminal(desktop, move || {
                    retry.update(|value| *value += 1)
                }))
                .child(ui::hint("Ctrl+C Quit"))
                .into_element(),
            Phase::SignedIn(account, privacy) => {
                let security = crate::security::mount(account.clone(), privacy);
                view()
                    .flex_1()
                    .child(dynamic(move || {
                        if security.blocked() {
                            crate::security::terminal(security).into_element()
                        } else {
                            chat::screen(account.clone()).into_element()
                        }
                    }))
                    .into_element()
            }
            Phase::Failed => {
                on_key("r", move |_| retry.update(|revision| *revision += 1));
                ui::status(move || {
                    format!(
                        "Couldn't open Mixin. R Retry   Ctrl+C Quit\n{}",
                        error.get()
                    )
                })
                .into_element()
            }
        }))
}
