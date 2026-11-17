//! Account-isolated WebView2 windows on the native STA thread.
use std::rc::Rc;

use nsh_platform_windows::Components;

mod drag;
mod input;
mod visual;
pub(super) use input::element as input;
mod dispatch;
mod downloads;
mod events;
mod focus;
mod session;
mod window;

pub(crate) fn register(components: &Components) {
    components.register(
        "MixinBotSession",
        Rc::new(|context| Box::new(session::Session::new(context))),
    );
    components.register(
        "MixinBotWebView",
        Rc::new(|context| Box::new(window::Host::new(context))),
    );
}
