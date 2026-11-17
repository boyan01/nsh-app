use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
};

use block2::DynBlock;
use nsh_platform_macos::{
    ComponentContext,
    objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSView, NSWindow,
    },
};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained, Weak},
    runtime::{AnyObject, Bool},
    sel,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString, NSURL};
use serde_json::{Value, json};

pub(super) struct Ivars {
    context: ComponentContext,
    web: RefCell<Option<Weak<AnyObject>>>,
    generation: Cell<u64>,
    pending: RefCell<HashSet<String>>,
    labels: RefCell<Value>,
    locale: RefCell<String>,
    immersive: Cell<bool>,
    script: RefCell<String>,
    closed: Cell<bool>,
    dialogs: super::dialogs::Dialogs,
    downloads: super::downloads::Downloads,
    theme_key: RefCell<Option<(String, String)>>,
    observing: Cell<bool>,
    shareable: Cell<bool>,
}

define_class!(
    // SAFETY: all ivars are confined to AppKit's main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "MixinBotWebDelegate"]
    #[ivars = Ivars]
    pub(super) struct Delegate;

    // SAFETY: NSObjectProtocol adds no subclass requirements.
    unsafe impl NSObjectProtocol for Delegate {}

    impl Delegate {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn message(&self, _controller: &AnyObject, message: &AnyObject) {
            if self.ivars().closed.get() { return; }
            // SAFETY: these are the documented WKScriptMessage/WKFrameInfo getters.
            let (body, frame): (Retained<AnyObject>, Retained<AnyObject>) = unsafe {
                (msg_send![message, body], msg_send![message, frameInfo])
            };
            let main: Bool = unsafe { msg_send![&frame, isMainFrame] };
            if !main.as_bool() { return; }
            let string: Bool = unsafe { msg_send![&body, isKindOfClass: NSString::class()] };
            if !string.as_bool() { return; }
            // SAFETY: the kind check above establishes NSString.
            let body = unsafe { &*(Retained::as_ptr(&body).cast::<NSString>()) }.to_string();
            if body.len() > 1024 * 1024 { return; }
            let Ok(envelope) = serde_json::from_str::<Value>(&body) else { return; };
            let name = envelope.get("name").and_then(Value::as_str).unwrap_or_default();
            let data = envelope.get("data").cloned().unwrap_or(Value::Null);
            match name {
                "close" => self.close_window(),
                "openInBrowser" => {
                    if let Some(raw) = data.as_str() && is_web(raw) { self.emit("browser", data); }
                }
                "signBotSignature" => {
                    // This URL belongs to the native message frame, not page-supplied JSON.
                    let request: Retained<AnyObject> = unsafe { msg_send![&frame, request] };
                    let url: Option<Retained<NSURL>> = unsafe { msg_send![&request, URL] };
                    let Some(url) = url.and_then(|url| url.absoluteString()).map(|url| url.to_string()) else { return; };
                    if !is_web(&url) || self.current_url().as_deref() != Some(url.as_str()) { return; }
                    let Some(mut data) = data.as_object().cloned() else { return; };
                    let Some(id) = data.get("id").and_then(Value::as_str)
                        .filter(|id| !id.is_empty() && id.len() <= 256).map(str::to_owned) else { return; };
                    if self.ivars().pending.borrow().contains(&id) { return; }
                    if self.ivars().pending.borrow().len() >= 32 {
                        self.evaluate_reply(&id, &[json!("[]")]);
                        return;
                    }
                    self.ivars().pending.borrow_mut().insert(id);
                    data.insert("page_url".into(), Value::String(url));
                    data.insert("generation".into(), Value::from(self.ivars().generation.get()));
                    self.emit("signature", Value::Object(data));
                }
                "theme" => self.theme(data.get("color").unwrap_or(&Value::Null)),
                "reloadTheme" => {},
                _ => {},
            }
        }

        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide(&self, web: &AnyObject, action: &AnyObject, done: &DynBlock<dyn Fn(isize)>) {
            if self.ivars().closed.get() { done.call((0,)); return; }
            // SAFETY: WKNavigationAction and NSURLRequest getters.
            let (frame, request): (Option<Retained<AnyObject>>, Retained<AnyObject>) = unsafe {
                (msg_send![action, targetFrame], msg_send![action, request])
            };
            let supports_download: Bool = unsafe { msg_send![action, respondsToSelector: sel!(shouldPerformDownload)] };
            if supports_download.as_bool() {
                let download: Bool = unsafe { msg_send![action, shouldPerformDownload] };
                if download.as_bool() { done.call((2,)); return; }
            }
            let url: Option<Retained<NSURL>> = unsafe { msg_send![&request, URL] };
            let raw = url.and_then(|url| url.absoluteString()).map(|url| url.to_string()).unwrap_or_default();
            let main = frame.as_ref().is_none_or(|frame| unsafe { let value: Bool = msg_send![frame, isMainFrame]; value.as_bool() });
            let Some(url) = url::Url::parse(&raw).ok() else { done.call((0,)); return; };
            if is_mixin_action(&url) {
                done.call((0,));
                if main { self.emit("navigate", Value::String(raw)); }
                return;
            }
            if matches!(url.scheme(), "http" | "https") {
                if frame.is_none() {
                    done.call((0,));
                    // Flutter's desktop plugin loads target=_blank in this view.
                    unsafe { let _: Option<Retained<AnyObject>> = msg_send![web, loadRequest: &*request]; }
                } else { done.call((1,)); }
                return;
            }
            if matches!(url.scheme(), "about" | "data" | "blob") {
                done.call((1,));
                return;
            }
            done.call((0,));
            if main && url.scheme() != "file" && url.scheme() != "javascript" {
                self.emit("external", Value::String(raw));
            }
            let _ = web;
        }

        #[unsafe(method(webView:decidePolicyForNavigationResponse:decisionHandler:))]
        fn response_policy(&self, _web: &AnyObject, response: &AnyObject, done: &DynBlock<dyn Fn(isize)>) {
            if self.ivars().closed.get() { done.call((0,)); return; }
            let can_show: Bool = unsafe { msg_send![response, canShowMIMEType] };
            let native: Retained<AnyObject> = unsafe { msg_send![response, response] };
            let supports_header: Bool = unsafe { msg_send![&native, respondsToSelector: sel!(valueForHTTPHeaderField:)] };
            let disposition: Option<Retained<NSString>> = if supports_header.as_bool() {
                unsafe { msg_send![&native, valueForHTTPHeaderField: &*NSString::from_str("Content-Disposition")] }
            } else { None };
            done.call((if !can_show.as_bool() || disposition.is_some_and(|value| value.to_string().trim_start().to_ascii_lowercase().starts_with("attachment")) { 2 } else { 1 },));
        }

        #[unsafe(method(webView:navigationAction:didBecomeDownload:))]
        fn action_download(&self, _web: &AnyObject, _action: &AnyObject, download: &AnyObject) {
            self.begin_download(download);
        }

        #[unsafe(method(webView:navigationResponse:didBecomeDownload:))]
        fn response_download(&self, _web: &AnyObject, _response: &AnyObject, download: &AnyObject) {
            self.begin_download(download);
        }

        #[unsafe(method(download:decideDestinationUsingResponse:suggestedFilename:completionHandler:))]
        fn download_destination(&self, download: &AnyObject, _response: &AnyObject, name: &NSString, done: &DynBlock<dyn Fn(*mut NSURL)>) {
            let window = self.browser_window();
            self.ivars().downloads.destination(download, window.as_deref(), &self.ivars().dialogs, name, done);
        }

        #[unsafe(method(downloadDidFinish:))]
        fn download_finished(&self, download: &AnyObject) {
            if self.ivars().downloads.finish(download).is_err() { self.download_error(); }
        }

        #[unsafe(method(download:didFailWithError:resumeData:))]
        fn download_failed(&self, download: &AnyObject, error: &AnyObject, _resume: Option<&AnyObject>) {
            self.ivars().downloads.failed(download);
            let code: isize = unsafe { msg_send![error, code] };
            if code != -999 { self.download_error(); }
        }

        #[unsafe(method(webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:))]
        fn javascript_alert(&self, _web: &AnyObject, message: &NSString, frame: &AnyObject, done: &DynBlock<dyn Fn()>) {
            let labels = self.ivars().labels.borrow().clone();
            self.ivars().dialogs.javascript_alert(self.browser_window().as_deref(), &super::dialogs::frame_host(frame), message, &labels, done);
        }

        #[unsafe(method(webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:))]
        fn javascript_confirm(&self, _web: &AnyObject, message: &NSString, frame: &AnyObject, done: &DynBlock<dyn Fn(Bool)>) {
            let labels = self.ivars().labels.borrow().clone();
            self.ivars().dialogs.javascript_confirm(self.browser_window().as_deref(), &super::dialogs::frame_host(frame), message, &labels, done);
        }

        #[unsafe(method(webView:runJavaScriptTextInputPanelWithPrompt:defaultText:initiatedByFrame:completionHandler:))]
        fn javascript_prompt(&self, _web: &AnyObject, message: &NSString, default: Option<&NSString>, frame: &AnyObject, done: &DynBlock<dyn Fn(*mut NSString)>) {
            let labels = self.ivars().labels.borrow().clone();
            self.ivars().dialogs.javascript_prompt(self.browser_window().as_deref(), &super::dialogs::frame_host(frame), message, default, &labels, done);
        }

        #[unsafe(method(webView:runOpenPanelWithParameters:initiatedByFrame:completionHandler:))]
        fn open_panel(&self, _web: &AnyObject, parameters: &AnyObject, _frame: &AnyObject, done: &DynBlock<dyn Fn(*mut NSArray<NSURL>)>) {
            self.ivars().dialogs.open(self.browser_window().as_deref(), parameters, done);
        }

        #[unsafe(method(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn create_webview(&self, web: &AnyObject, _configuration: &AnyObject, action: &AnyObject, _features: &AnyObject) -> *mut AnyObject {
            if !self.ivars().closed.get() {
                let request: Retained<AnyObject> = unsafe { msg_send![action, request] };
                // Keep Flutter's in-place new-window behavior. The resulting
                // navigation passes the same native URL policy and signature gate.
                unsafe { let _: Option<Retained<AnyObject>> = msg_send![web, loadRequest: &*request]; }
            }
            std::ptr::null_mut()
        }

        #[unsafe(method(webViewDidClose:))]
        fn script_closed(&self, _web: &AnyObject) { self.close_window(); }

        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observed(&self, key: Option<&NSString>, object: Option<&AnyObject>, change: Option<&AnyObject>, context: *mut std::ffi::c_void) {
            if key.is_some_and(|key| key.to_string() == "title") {
                if !self.ivars().closed.get() {
                    self.update_title();
                }
            } else if key.is_some_and(|key| key.to_string() == "URL") {
                self.navigation_state(None);
            } else if key.is_some_and(|key| key.to_string() == "canGoBack") {
                if !self.ivars().closed.get() && let Some(web) = object {
                    let can_back: Bool = unsafe { msg_send![web, canGoBack] };
                    let _ = can_back;
                    self.navigation_state(None);
                }
            } else {
                unsafe { let _: () = msg_send![super(self), observeValueForKeyPath: key, ofObject: object, change: change, context: context]; }
            }
        }

        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn started(&self, _web: &AnyObject, _navigation: Option<&AnyObject>) {
            self.ivars().generation.set(self.ivars().generation.get().wrapping_add(1));
            self.ivars().pending.borrow_mut().clear();
            self.navigation_state(Some(true));
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn finished(&self, _web: &AnyObject, _navigation: Option<&AnyObject>) {
            if !self.ivars().closed.get() { self.update_locale(); self.navigation_state(Some(false)); }
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed_navigation(&self, _web: &AnyObject, _navigation: Option<&AnyObject>, error: &AnyObject) {
            self.navigation_error(error);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_provisional_navigation(&self, _web: &AnyObject, _navigation: Option<&AnyObject>, error: &AnyObject) {
            self.navigation_error(error);
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn terminated(&self, _web: &AnyObject) {
            self.ivars().generation.set(self.ivars().generation.get().wrapping_add(1));
            self.ivars().pending.borrow_mut().clear();
            self.failed("The web content process stopped");
        }


    }
);

impl Delegate {
    pub(super) fn new(mtm: MainThreadMarker, context: ComponentContext) -> Retained<Self> {
        let value = Self::alloc(mtm).set_ivars(Ivars {
            context,
            web: RefCell::new(None),
            generation: Cell::new(0),
            pending: RefCell::new(HashSet::new()),
            labels: RefCell::new(Value::Null),
            locale: RefCell::new(String::new()),
            immersive: Cell::new(false),
            script: RefCell::new(String::new()),
            closed: Cell::new(false),
            dialogs: super::dialogs::Dialogs::default(),
            downloads: super::downloads::Downloads::default(),
            theme_key: RefCell::new(None),
            observing: Cell::new(false),
            shareable: Cell::new(true),
        });
        // SAFETY: NSObject's initializer initializes this allocation once.
        unsafe { msg_send![super(value), init] }
    }
    pub(super) fn attach_theme(&self, session: &str, app: &str) {
        *self.ivars().theme_key.borrow_mut() = Some((session.into(), app.into()));
        self.emit("theme", json!(super::session::theme(session, app)));
    }
    fn theme(&self, value: &Value) {
        let color = super::super::chrome::theme_color(value);
        if let Some((session, app)) = self.ivars().theme_key.borrow().as_ref() {
            super::session::set_theme(session, app, color);
        }
        self.emit("theme", json!(color));
    }
    pub(super) fn system_dark(&self, dark: bool) {
        self.with_web(|web| unsafe {
            let view = &*(web as *const AnyObject).cast::<NSView>();
            view.setAppearance(
                NSAppearance::appearanceNamed(if dark {
                    NSAppearanceNameDarkAqua
                } else {
                    NSAppearanceNameAqua
                })
                .as_deref(),
            );
        });
    }
    fn browser_window(&self) -> Option<Retained<NSWindow>> {
        if self.ivars().closed.get() {
            return None;
        }
        let web = self.ivars().web.borrow().as_ref().and_then(Weak::load)?;
        let view = unsafe { &*(Retained::as_ptr(&web).cast::<NSView>()) };
        view.window()
    }
    fn cancel_native_work(&self) {
        let window = self.browser_window();
        self.ivars().dialogs.cancel_all(window.as_deref());
        self.ivars().downloads.cancel_all();
    }
    pub(super) fn navigate(&self, forward: bool) -> bool {
        let mut moved = false;
        self.with_web(|web| unsafe {
            let can: Bool = if forward {
                msg_send![web, canGoForward]
            } else {
                msg_send![web, canGoBack]
            };
            if can.as_bool() {
                if forward {
                    let _: Option<Retained<AnyObject>> = msg_send![web, goForward];
                } else {
                    let _: Option<Retained<AnyObject>> = msg_send![web, goBack];
                }
                moved = true;
            }
        });
        moved
    }
    pub(super) fn reload(&self) {
        self.with_web(|web| unsafe {
            let _: Option<Retained<AnyObject>> = msg_send![web, reload];
        });
    }
    pub(super) fn set_labels(&self, labels: &Value) {
        *self.ivars().labels.borrow_mut() = labels.clone();
    }
    pub(super) fn presentation(&self, page: &Value) -> Result<(), &'static str> {
        self.ivars().shareable.set(
            page.get("shareable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        );
        let script = page
            .get("script")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if *self.ivars().script.borrow() != script {
            let class = super::class(c"WKUserScript")?;
            self.with_web(|web| unsafe {
                // This controller belongs only to this bot window. Replacing its single
                // user script updates future documents without reloading the current one.
                let config: Retained<AnyObject> = msg_send![web, configuration];
                let content: Retained<AnyObject> = msg_send![&config, userContentController];
                let _: () = msg_send![&content, removeAllUserScripts];
                let allocation: Allocated<AnyObject> = msg_send![class, alloc];
                let injected: Retained<AnyObject> = msg_send![allocation, initWithSource: &*NSString::from_str(script), injectionTime: 0_isize, forMainFrameOnly: true];
                let _: () = msg_send![&content, addUserScript: &*injected];
            });
            *self.ivars().script.borrow_mut() = script.to_owned();
        }
        let locale = page
            .get("locale")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if *self.ivars().locale.borrow() != locale {
            *self.ivars().locale.borrow_mut() = locale.to_owned();
            self.update_locale();
        }
        let immersive = page
            .get("immersive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.ivars().immersive.set(immersive);
        self.update_locale();
        Ok(())
    }
    fn update_locale(&self) {
        // The bridge keeps the existing page and pending callbacks alive.
        let script = format!(
            "window.__mixinBotSetLocale?.({}); window.__mixinBotSetImmersive?.({});",
            json!(*self.ivars().locale.borrow()),
            self.ivars().immersive.get()
        );
        self.with_web(|web| unsafe {
            let _: () = msg_send![web, evaluateJavaScript: &*NSString::from_str(&script), completionHandler: std::ptr::null::<AnyObject>()];
        });
    }
    pub(super) fn attach(&self, web: &AnyObject) {
        *self.ivars().web.borrow_mut() = Some(Weak::new(web));
        if !self.ivars().observing.replace(true) {
            unsafe {
                let _: () = msg_send![web, addObserver: self, forKeyPath: &*NSString::from_str("canGoBack"), options: 5_usize, context: std::ptr::null_mut::<std::ffi::c_void>()];
                let _: () = msg_send![web, addObserver: self, forKeyPath: &*NSString::from_str("title"), options: 5_usize, context: std::ptr::null_mut::<std::ffi::c_void>()];
                let _: () = msg_send![web, addObserver: self, forKeyPath: &*NSString::from_str("URL"), options: 5_usize, context: std::ptr::null_mut::<std::ffi::c_void>()];
            }
        }
    }
    pub(super) fn detach(&self) {
        self.cancel_native_work();
        self.ivars().closed.set(true);
        self.ivars().pending.borrow_mut().clear();
        self.ivars()
            .generation
            .set(self.ivars().generation.get().wrapping_add(1));
        if self.ivars().observing.replace(false) {
            self.with_web(|web| unsafe {
                let _: () = msg_send![web, removeObserver: self, forKeyPath: &*NSString::from_str("canGoBack")];
                let _: () = msg_send![web, removeObserver: self, forKeyPath: &*NSString::from_str("title")];
                let _: () = msg_send![web, removeObserver: self, forKeyPath: &*NSString::from_str("URL")];
            });
        }
        self.ivars().web.borrow_mut().take();
    }
    fn emit(&self, name: &str, data: Value) {
        self.ivars().context.emit(name, data);
    }
    pub(super) fn failed(&self, _message: &str) {
        if !self.ivars().closed.get() {
            self.emit("failed", Value::Null);
        }
    }
    fn begin_download(&self, download: &AnyObject) {
        if self.ivars().closed.get() {
            unsafe {
                let _: () = msg_send![download, cancel: std::ptr::null::<AnyObject>()];
            }
        } else {
            self.ivars().downloads.begin(download, self);
        }
    }
    fn download_error(&self) {
        let Some(window) = self.browser_window() else {
            return;
        };
        let labels = self.ivars().labels.borrow().clone();
        let text = NSString::from_str(super::label(
            &labels,
            "Couldn’t download this file. Try again.",
        ));
        let done = block2::RcBlock::new(|| {});
        let host = self
            .current_url()
            .and_then(|url| url::Url::parse(&url).ok())
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();
        self.ivars()
            .dialogs
            .javascript_alert(Some(&window), &host, &text, &labels, &done);
    }
    fn navigation_error(&self, error: &AnyObject) {
        let code: isize = unsafe { msg_send![error, code] };
        // Deliberately blocked action links cancel a provisional navigation.
        if code != -999 {
            self.failed("Could not load the page");
        }
    }
    fn close_window(&self) {
        if !self.ivars().closed.get() {
            self.emit("closed", Value::Null);
        }
    }
    fn navigation_state(&self, loading: Option<bool>) {
        if self.ivars().closed.get() {
            return;
        }
        self.with_web(|web| {
            let can_back: Bool = unsafe { msg_send![web, canGoBack] };
            self.emit("navigation", json!({"url": self.current_url(), "title": self.page_title(), "can_back": can_back.as_bool(), "loading": loading}));
        });
    }
    fn with_web(&self, callback: impl FnOnce(&AnyObject)) {
        if let Some(web) = self.ivars().web.borrow().as_ref().and_then(Weak::load) {
            callback(&web);
        }
    }
    fn page_title(&self) -> Option<String> {
        let web = self.ivars().web.borrow().as_ref().and_then(Weak::load)?;
        let title: Option<Retained<NSString>> = unsafe { msg_send![&web, title] };
        title
            .map(|title| title.to_string())
            .filter(|title| !title.trim().is_empty())
    }
    fn update_title(&self) {
        self.navigation_state(None);
    }
    fn current_url(&self) -> Option<String> {
        let web = self.ivars().web.borrow().as_ref().and_then(Weak::load)?;
        // SAFETY: WKWebView's current committed URL.
        let url: Option<Retained<NSURL>> = unsafe { msg_send![&web, URL] };
        url.and_then(|url| url.absoluteString())
            .map(|url| url.to_string())
    }
    pub(super) fn reply(&self, reply: &Value) {
        if self.ivars().closed.get()
            || reply.get("generation").and_then(Value::as_u64)
                != Some(self.ivars().generation.get())
        {
            return;
        }
        let (Some(id), Some(args)) = (
            reply.get("id").and_then(Value::as_str),
            reply.get("args").and_then(Value::as_array),
        ) else {
            return;
        };
        // A completed request at an old SPA URL still releases its bounded slot.
        if !self.ivars().pending.borrow_mut().remove(id)
            || reply.get("page_url").and_then(Value::as_str) != self.current_url().as_deref()
        {
            return;
        }
        self.evaluate_reply(id, args);
    }
    fn evaluate_reply(&self, id: &str, args: &[Value]) {
        // JSON encoding keeps callback identifiers and signature text out of source syntax.
        let script = format!("window.__mixinBotReply?.({}, {});", json!(id), json!(args));
        self.with_web(|web| unsafe { let _: () = msg_send![web, evaluateJavaScript: &*NSString::from_str(&script), completionHandler: std::ptr::null::<AnyObject>()]; });
    }
}

fn is_web(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}
fn is_mixin_action(url: &url::Url) -> bool {
    url.scheme() == "mixin" || matches!(url.host_str(), Some("mixin.one" | "www.mixin.one"))
}
