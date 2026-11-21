//! Capabilities and lifecycle of the terminal host for the shared application.

use nsh_core::{platform::Idiom, prelude::*};

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod host;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use host::run;

pub(crate) fn active() -> bool {
    idiom() == Idiom::Terminal
}

#[derive(Clone, Copy)]
pub(crate) struct Shortcuts {
    pub available: Memo<bool>,
    pub palette_open: Signal<bool>,
}

pub(crate) fn key_options() -> KeyOptions {
    KeyOptions {
        when: nsh_core::keymap::When::Always,
        for_focus: true,
        ..Default::default()
    }
}

// The NSH development host uses the same default cell dimensions.
pub(crate) const COLUMN: f32 = 8.0;
pub(crate) const ROW: f32 = 16.0;

pub(crate) fn cols(n: f32) -> Points {
    px(n * COLUMN)
}

pub(crate) fn rows(n: f32) -> Points {
    px(n * ROW)
}

pub(crate) fn spacer() -> View {
    view().h(rows(1.0)).flex_shrink_0()
}

pub(crate) fn columns() -> f32 {
    window().width.0 / COLUMN
}

pub(crate) fn viewport_rows() -> f32 {
    window().height.0 / ROW
}

pub(crate) fn image_supported() -> bool {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use nsh_platform_tui::ColorDepth;
        matches!(
            host::color_depth(),
            ColorDepth::TrueColor | ColorDepth::Ansi256
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn surface(render: impl FnOnce() -> AnyElement) -> AnyElement {
    if host::installed() {
        return render();
    }

    // The generated development host owns the terminal transport.
    let lock = mixin_desktop_api::data_directory()
        .map_err(Box::<dyn std::error::Error>::from)
        .and_then(|directory| host::prepare(&directory));
    match lock {
        Ok(lock) => {
            let lock = std::rc::Rc::new(lock);
            nsh_core::element::desktop::on_app_quit(move || {
                let lock = lock.clone();
                async move {
                    crate::runtime::shutdown().await;
                    log::info!(target: "nsh::mixin", "Terminal development shutdown drained");
                    log::logger().flush();
                    drop(lock);
                }
            });
            render()
        }
        Err(error) => view()
            .flex_1()
            .p_2()
            .child(text(format!("Mixin startup failed: {error}")))
            .into_element(),
    }
}
