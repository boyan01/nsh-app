//! A sized WebKitGTK child; NSH owns its containing window.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use gtk::{gio, glib};
use nsh_platform_linux::{ComponentContext, gtk4 as gtk};
use serde_json::{Value, json};
use webkit6::{
    UserContentInjectedFrames, UserContentManager, UserScript, UserScriptInjectionTime, WebView,
    prelude::*,
};

use super::{dialogs::Dialogs, downloads::Downloads, events, protocol::Gate, session::Entry};

pub(super) struct State {
    pub context: ComponentContext,
    pub web: WebView,
    pub content: UserContentManager,
    pub entry: Rc<Entry>,
    pub page: RefCell<Value>,
    pub closed: Cell<bool>,
    pub gate: Gate,
    pub handlers: RefCell<Vec<glib::SignalHandlerId>>,
    pub dialogs: Dialogs,
    pub downloads: Downloads,
    timeout: RefCell<Option<glib::SourceId>>,
    script: RefCell<String>,
}

impl State {
    pub fn open(
        context: ComponentContext,
        page: Value,
        entry: Rc<Entry>,
    ) -> Result<Rc<Self>, &'static str> {
        if !entry.accepting.get() {
            return Err("Browser session is closing");
        }
        if !entry.extension_ready {
            return Err("The native browser extension is not installed");
        }
        let content = UserContentManager::new();
        let web = WebView::builder()
            .web_context(&entry.web_context)
            .network_session(&entry.network)
            .user_content_manager(&content)
            .build();
        web.set_hexpand(true);
        web.set_vexpand(true);
        let state = Rc::new(Self {
            context,
            web,
            content,
            entry,
            page: RefCell::new(page),
            closed: Cell::new(false),
            gate: Gate::default(),
            handlers: RefCell::default(),
            dialogs: Dialogs::default(),
            downloads: Downloads::default(),
            timeout: RefCell::default(),
            script: RefCell::default(),
        });
        state.entry.register(&state);
        events::install(&state);
        state.presentation();
        state.arm_timeout();
        let url = state
            .page
            .borrow()
            .get("url")
            .and_then(Value::as_str)
            .filter(|url| is_web(url))
            .ok_or("Invalid browser URL")?
            .to_owned();
        state.web.load_uri(&url);
        Ok(state)
    }

    pub fn label(&self, key: &str) -> String {
        self.page
            .borrow()
            .get("labels")
            .and_then(|labels| labels.get(key))
            .and_then(Value::as_str)
            .unwrap_or(key)
            .into()
    }

    pub fn emit(&self, name: &str, data: Value) {
        if !self.closed.get() {
            self.context.emit(name, data);
        }
    }

    pub fn close(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.stop_timeout();
        self.gate.invalidate(events::next_generation());
        self.dialogs.close();
        self.downloads.close();
        for id in self.handlers.borrow_mut().drain(..) {
            self.web.disconnect(id);
        }
        self.web.stop_loading();
        self.content.remove_all_scripts();
        if let Some(parent) = self.web.parent().and_downcast::<gtk::Box>() {
            parent.remove(&self.web);
        }
        self.context.emit("closed", Value::Null);
    }

    pub fn stop_timeout(&self) {
        if let Some(timer) = self.timeout.borrow_mut().take() {
            timer.remove();
        }
    }

    pub fn arm_timeout(self: &Rc<Self>) {
        self.stop_timeout();
        let weak = Rc::downgrade(self);
        *self.timeout.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_secs(20),
            move || {
                if let Some(state) = weak.upgrade().filter(|state| !state.closed.get()) {
                    state.timeout.borrow_mut().take();
                    state.fail();
                }
            },
        ));
    }

    pub fn fail(&self) {
        self.gate.invalidate(events::next_generation());
        self.emit("failed", Value::Null);
    }

    pub fn presentation(&self) {
        let page = self.page.borrow();
        let script = page
            .get("script")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if *self.script.borrow() != script {
            self.content.remove_all_scripts();
            self.content.add_script(&UserScript::new(
                script,
                UserContentInjectedFrames::TopFrame,
                UserScriptInjectionTime::Start,
                &[],
                &[],
            ));
            *self.script.borrow_mut() = script.into();
        }
        if let Some(settings) = webkit6::prelude::WebViewExt::settings(&self.web) {
            // Each history navigation must create a fresh native document token.
            // HTTP cache remains enabled; only suspended JS document reuse is off.
            settings.set_enable_page_cache(false);
            settings.set_user_agent_with_application_details(
                Some("Mixin"),
                page.get("version").and_then(Value::as_str),
            );
        }
        drop(page);
        self.update_context();
        self.update();
    }

    pub fn update_context(&self) {
        let page = self.page.borrow();
        let locale = page.get("locale").and_then(Value::as_str).unwrap_or("en");
        let immersive = page
            .get("immersive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.evaluate(&format!(
            "window.__mixinBotSetLocale?.({}); window.__mixinBotSetImmersive?.({immersive});",
            json!(locale)
        ));
    }

    pub fn window(&self) -> Option<gtk::Window> {
        self.web.root().and_downcast::<gtk::Window>()
    }

    pub fn update(&self) {
        self.emit("navigation", json!({"url": self.web.uri().as_deref(), "title": self.web.title().as_deref(), "can_back": self.web.can_go_back(), "loading": self.web.is_loading()}));
    }

    pub fn evaluate(&self, script: &str) {
        if !self.closed.get() {
            self.web
                .evaluate_javascript(script, None, None, gio::Cancellable::NONE, |_| {});
        }
    }

    pub fn reply(&self, reply: &Value) {
        if !self.closed.get()
            && let (Some(id), Some(args), Some(uri), Some(generation)) = (
                reply.get("id").and_then(Value::as_str),
                reply.get("args").and_then(Value::as_array),
                reply.get("page_url").and_then(Value::as_str),
                reply.get("generation").and_then(Value::as_u64),
            )
            && self
                .gate
                .reply(id, uri, generation, self.web.uri().as_deref())
        {
            self.evaluate(&format!(
                "window.__mixinBotReply?.({},{});",
                json!(id),
                json!(args)
            ));
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) fn is_web(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}
