//! An app-owned WebKit host. Native frame provenance never comes from page JSON.
use std::{cell::RefCell, ffi::CStr, rc::Rc};

use nsh_platform_macos::{
    ComponentContext, Components, NativeComponent,
    objc2_app_kit::{NSAutoresizingMaskOptions as Mask, NSView},
};
use objc2::{
    MainThreadMarker, MainThreadOnly, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject},
};
use objc2_foundation::{NSPoint, NSRect, NSString, NSURL};
use serde_json::Value;

mod delegate;
mod dialogs;
mod downloads;
use delegate::Delegate;
mod resources;
mod session;

// Link the system framework before querying its Objective-C classes.
#[link(name = "WebKit", kind = "framework")]
unsafe extern "C" {}

pub(crate) fn register(components: &Components) {
    components.register(
        "MixinBotSession",
        Rc::new(|context| Box::new(session::Session::new(context))),
    );
    components.register(
        "MixinBotWebView",
        Rc::new(|context| Box::new(Host::new(context))),
    );
}

fn class(name: &CStr) -> Result<&'static AnyClass, &'static str> {
    AnyClass::get(name).ok_or("WebKit runtime is unavailable")
}

struct Host {
    holder: Retained<NSView>,
    resources: Rc<RefCell<resources::Resources>>,
    delegate: Retained<Delegate>,
    dark: bool,
    session: String,
    pending_page: Option<Value>,
}

impl Host {
    fn new(context: ComponentContext) -> Self {
        let mtm = MainThreadMarker::new().expect("Native factories run on AppKit's main thread");
        let delegate = Delegate::new(mtm, context);
        Self {
            holder: NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO),
            resources: Rc::new(RefCell::new(resources::Resources {
                web: None,
                delegate: objc2::rc::Weak::new(&delegate),
            })),
            delegate,
            dark: false,
            session: String::new(),
            pending_page: None,
        }
    }

    fn open(&mut self, page: &Value) -> Result<(), &'static str> {
        let raw = page
            .get("url")
            .and_then(Value::as_str)
            .ok_or("Missing page URL")?;
        let parsed = url::Url::parse(raw).map_err(|_| "Invalid page URL")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("Unsupported page URL");
        }
        let store = session::store(&self.session).ok_or("Bot website session is not ready")?;
        let url = NSURL::URLWithString(&NSString::from_str(raw)).ok_or("Invalid page URL")?;
        self.delegate
            .set_labels(page.get("labels").unwrap_or(&Value::Null));
        let size = self.holder.bounds().size;
        let config: Retained<AnyObject> =
            unsafe { msg_send![class(c"WKWebViewConfiguration")?, new] };
        unsafe {
            let _: () = msg_send![&config, setWebsiteDataStore: &*store];
        }
        let content: Retained<AnyObject> = unsafe { msg_send![&config, userContentController] };
        unsafe {
            let _: () = msg_send![&content, addScriptMessageHandler: &*self.delegate, name: &*NSString::from_str("mixinBot")];
            let _: () = msg_send![&config, setApplicationNameForUserAgent: &*NSString::from_str(&format!("Mixin/{}", page.get("version").and_then(Value::as_str).unwrap_or_default()))];
        }
        let web: Retained<AnyObject> = unsafe {
            let allocation: Allocated<AnyObject> = msg_send![class(c"WKWebView")?, alloc];
            msg_send![allocation, initWithFrame: NSRect::new(NSPoint::ZERO, size), configuration: &*config]
        };
        unsafe {
            let _: () = msg_send![&web, setNavigationDelegate: &*self.delegate];
            let _: () = msg_send![&web, setUIDelegate: &*self.delegate];
        }
        let web_view: &NSView = unsafe { &*(Retained::as_ptr(&web).cast::<NSView>()) };
        web_view.setAutoresizingMask(Mask::ViewWidthSizable | Mask::ViewHeightSizable);
        web_view.setFrame(NSRect::new(NSPoint::ZERO, size));
        self.holder.addSubview(web_view);
        self.delegate.attach(&web);
        self.delegate.system_dark(self.dark);
        self.delegate.attach_theme(
            &self.session,
            page.get("app_id")
                .and_then(Value::as_str)
                .unwrap_or(&parsed.origin().ascii_serialization()),
        );
        self.delegate.presentation(page)?;
        unsafe {
            let request: Retained<AnyObject> =
                msg_send![class(c"NSURLRequest")?, requestWithURL: &*url];
            let _: Option<Retained<AnyObject>> = msg_send![&web, loadRequest: &*request];
        }
        self.resources.borrow_mut().web = Some(web);
        session::register_window(&self.session, &self.resources);
        Ok(())
    }
}

impl NativeComponent for Host {
    fn view(&self) -> Retained<NSView> {
        self.holder.clone()
    }
    fn set_prop(&mut self, name: &str, value: Option<&Value>) {
        if name == "session" {
            self.session = value.and_then(Value::as_str).unwrap_or_default().to_owned();
            if let Some(page) = self.pending_page.take() {
                self.set_prop("page", Some(&page));
            }
            return;
        }
        if name == "dark" {
            self.dark = value.and_then(Value::as_bool).unwrap_or_default();
            self.delegate.system_dark(self.dark);
            return;
        }
        if name != "page" {
            return;
        }
        let Some(page) = value else {
            return;
        };
        if self.session.is_empty() {
            self.pending_page = Some(page.clone());
            return;
        }
        if self.resources.borrow().web.is_none() {
            if let Err(error) = self.open(page) {
                self.delegate.failed(error);
            }
        } else {
            self.delegate
                .set_labels(page.get("labels").unwrap_or(&Value::Null));
            if let Err(error) = self.delegate.presentation(page) {
                self.delegate.failed(error);
            }
        }
    }
    fn command(&mut self, name: &str, args: &Value) {
        if name == "reply" {
            self.delegate.reply(args);
        } else if name == "back" {
            self.delegate.navigate(false);
        } else if name == "forward" {
            self.delegate.navigate(true);
        } else if name == "reload" {
            self.delegate.reload();
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.resources.borrow_mut().close();
    }
}

fn label<'a>(labels: &'a Value, key: &'a str) -> &'a str {
    labels.get(key).and_then(Value::as_str).unwrap_or(key)
}
