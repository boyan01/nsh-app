use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

use nsh_platform_windows::{ComponentContext, NativeComponent};
use serde_json::{Value, json};
use webview2_com::{
    AddScriptToExecuteOnDocumentCreatedCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler, Microsoft::Web::WebView2::Win32::*,
    take_pwstr,
};
use windows::{
    Win32::UI::Shell::IFileSaveDialog,
    core::{HSTRING, Interface, PWSTR},
};

use super::{dispatch, events, session, visual::Visual};

pub(super) struct State {
    pub context: ComponentContext,
    pub native: Rc<dispatch::Window>,
    pub page: RefCell<Value>,
    pub web: RefCell<Option<ICoreWebView2>>,
    pub controller: RefCell<Option<ICoreWebView2Controller>>,
    pub removers: RefCell<Vec<Box<dyn FnOnce()>>>,
    pub generation: Cell<u64>,
    pub closed: Cell<bool>,
    pub dark: Cell<bool>,
    pub hovered: Cell<bool>,
    pub drag: super::drag::Drag,
    pub download_started: Cell<bool>,
    pub visual: Rc<Visual>,
    pub placement: Cell<Option<nsh_platform_windows::Placement>>,
    pub composition: RefCell<Option<ICoreWebView2CompositionController>>,
    pub pending: RefCell<HashSet<String>>,
    pub saving: RefCell<Option<IFileSaveDialog>>,
    pub downloads: RefCell<Vec<ICoreWebView2DownloadOperation>>,
    script: RefCell<Option<String>>,
    script_revision: Cell<u64>,
    navigated: Cell<bool>,
}

impl State {
    fn open(
        context: ComponentContext,
        page: Value,
        entry: Rc<session::Entry>,
        dark: bool,
        native: Rc<dispatch::Window>,
        visual: Rc<Visual>,
    ) -> Result<Rc<Self>, &'static str> {
        let raw = page
            .get("url")
            .and_then(Value::as_str)
            .ok_or("Missing page URL")?;
        if !is_web(raw) {
            return Err("Invalid browser URL");
        }
        let state = Rc::new(Self {
            context,
            native,
            page: RefCell::new(page),
            web: RefCell::default(),
            controller: RefCell::default(),
            removers: RefCell::default(),
            generation: Cell::new(0),
            closed: Cell::new(false),
            dark: Cell::new(dark),
            hovered: Cell::new(false),
            drag: Default::default(),
            download_started: Cell::new(false),
            visual,
            placement: Cell::new(None),
            composition: RefCell::default(),
            pending: RefCell::default(),
            saving: RefCell::default(),
            downloads: RefCell::default(),
            script: RefCell::default(),
            script_revision: Cell::new(0),
            navigated: Cell::new(false),
        });
        let weak = Rc::downgrade(&state);
        *state.native.closing.borrow_mut() = Some(Box::new(move || {
            if let Some(state) = weak.upgrade() {
                state.close();
            }
        }));
        let weak = Rc::downgrade(&state);
        *state.native.resize.borrow_mut() = Some(Box::new(move || {
            if let Some(state) = weak.upgrade() {
                state.resize();
            }
        }));
        let weak = Rc::downgrade(&state);
        state.native.timeout(move || {
            if let Some(state) = weak.upgrade().filter(|state| !state.closed.get()) {
                state.fail();
                state.close();
            }
        });
        entry.register(&state);
        let weak = Rc::downgrade(&state);
        entry.environment(move |environment| {
            let Some(state) = weak.upgrade().filter(|state| !state.closed.get()) else {
                return;
            };
            let Some(environment) = environment else {
                state.fail();
                state.close();
                return;
            };
            let weak = Rc::downgrade(&state);
            let done = CreateCoreWebView2CompositionControllerCompletedHandler::create(Box::new(
                move |result, controller| {
                    let Some(state) = weak.upgrade().filter(|state| !state.closed.get()) else {
                        if let Some(controller) = controller {
                            if let Ok(controller) = controller.cast::<ICoreWebView2Controller>() {
                                unsafe {
                                    let _ = controller.Close();
                                }
                            }
                        }
                        return Ok(());
                    };
                    let Some(controller) = result.ok().and(controller) else {
                        state.fail();
                        state.close();
                        return Ok(());
                    };
                    if state.attach(controller).is_err() {
                        state.fail();
                        state.close();
                    }
                    Ok(())
                },
            ));
            if unsafe {
                environment
                    .cast::<ICoreWebView2Environment3>()
                    .and_then(|environment| {
                        environment.CreateCoreWebView2CompositionController(state.parent(), &done)
                    })
            }
            .is_err()
            {
                state.fail();
                state.close();
            }
        });
        Ok(state)
    }

    fn attach(
        self: &Rc<Self>,
        composition: ICoreWebView2CompositionController,
    ) -> windows::core::Result<()> {
        let controller = composition.cast::<ICoreWebView2Controller>()?;
        *self.controller.borrow_mut() = Some(controller.clone());
        unsafe {
            composition.SetRootVisualTarget(&self.visual.target)?;
        }
        *self.composition.borrow_mut() = Some(composition);
        let web = unsafe { controller.CoreWebView2()? };
        let settings = unsafe { web.Settings()? };
        unsafe {
            settings.SetIsScriptEnabled(true)?;
            settings.SetIsWebMessageEnabled(true)?;
            settings.SetIsStatusBarEnabled(false)?;
            // Native WebView2 dialogs provide alert, confirm and prompt without
            // running a reentrant modal loop inside an application event handler.
            settings.SetAreDefaultScriptDialogsEnabled(true)?;
            settings.SetAreDevToolsEnabled(cfg!(debug_assertions))?;
        }
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings2>() {
            let page = self.page.borrow();
            let version = page
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or_default();
            unsafe {
                let mut original = PWSTR::null();
                settings.UserAgent(&mut original)?;
                let original = take_pwstr(original);
                settings.SetUserAgent(&HSTRING::from(format!("{original} Mixin/{version}")))?;
            }
        }
        *self.web.borrow_mut() = Some(web.clone());
        super::focus::browser(self, &controller)?;
        events::install(self, &web)?;
        self.resize();
        self.presentation();
        Ok(())
    }

    pub fn url(&self) -> Option<String> {
        let web = self.web.borrow().clone()?;
        let mut source = PWSTR::null();
        unsafe {
            web.Source(&mut source).ok()?;
        }
        Some(take_pwstr(source))
    }

    pub fn title(&self) -> String {
        let Some(web) = self.web.borrow().clone() else {
            return String::new();
        };
        let mut title = PWSTR::null();
        if unsafe { web.DocumentTitle(&mut title) }.is_ok() {
            take_pwstr(title)
        } else {
            String::new()
        }
    }

    pub fn emit(&self, name: &str, value: Value) {
        if !self.closed.get() {
            self.context.emit(name, value);
        }
    }
    pub fn fail(&self) {
        self.emit("failed", Value::Null);
    }

    pub fn close(&self) {
        if self.closed.replace(true) {
            return;
        }
        super::input::cancel(self);
        self.generation.set(self.generation.get().wrapping_add(1));
        self.pending.borrow_mut().clear();
        let dialog = self.saving.borrow_mut().take();
        if let Some(dialog) = dialog {
            unsafe {
                let _ = dialog.Close(windows::Win32::Foundation::E_ABORT);
            }
        }
        let downloads = self.downloads.borrow_mut().drain(..).collect::<Vec<_>>();
        for operation in downloads {
            unsafe {
                let _ = operation.Cancel();
            }
        }
        for remove in self.removers.borrow_mut().drain(..).collect::<Vec<_>>() {
            remove();
        }
        if let Some(web) = self.web.borrow_mut().take() {
            unsafe {
                let _ = web.Stop();
            }
        }
        if let Some(controller) = self.controller.borrow_mut().take() {
            unsafe {
                let _ = controller.Close();
            }
        }
        if let Some(composition) = self.composition.borrow_mut().take() {
            unsafe {
                let _ = composition.SetRootVisualTarget(None::<&windows::core::IUnknown>);
            }
        }
        self.native.close();
        self.context.emit("closed", Value::Null);
    }

    pub fn parent(&self) -> windows::Win32::Foundation::HWND {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetAncestor(
                self.native.hwnd.get(),
                windows::Win32::UI::WindowsAndMessaging::GA_ROOT,
            )
        }
    }

    pub fn resize(&self) {
        let rect = self
            .placement
            .get()
            .map(|p| {
                let left = p.matrix.M31.round() as i32;
                let top = p.matrix.M32.round() as i32;
                windows::Win32::Foundation::RECT {
                    left,
                    top,
                    right: left + p.size.0,
                    bottom: top + p.size.1,
                }
            })
            .unwrap_or_else(|| self.native.bounds());
        if let Some(controller) = self.controller.borrow().as_ref() {
            unsafe {
                // Bounds are relative to the NSH window, so IME candidates,
                // select menus and native dialogs follow the embedded page.
                let _ = controller.SetParentWindow(self.parent());
                let _ = controller.SetBounds(rect);
                let _ = controller.NotifyParentWindowPositionChanged();
                if let Ok(controller) = controller.cast::<ICoreWebView2Controller3>() {
                    let _ = controller.SetShouldDetectMonitorScaleChanges(false);
                    let _ = controller.SetRasterizationScale(self.scale());
                }
            }
        }
    }

    pub fn scale(&self) -> f64 {
        f64::from(
            unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(self.native.hwnd.get()) }.max(96),
        ) / 96.0
    }

    pub fn update(&self, loading: Option<bool>) {
        let mut can_back = windows::core::BOOL(0);
        if let Some(web) = self.web.borrow().as_ref() {
            unsafe {
                let _ = web.CanGoBack(&mut can_back);
            }
        }
        self.emit("navigation", json!({"url": self.url(), "title": self.title(), "can_back": can_back.as_bool(), "loading": loading}));
    }

    pub fn presentation(self: &Rc<Self>) {
        self.update(None);
        let Some(web) = self.web.borrow().clone() else {
            return;
        };
        let page = self.page.borrow();
        let script = page
            .get("script")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        // Main-frame-only injection is defense in depth. Message source and
        // generation are still taken from native WebView2 events below.
        let script = format!("if (window === window.top) {{\n{script}\n}}");
        let revision = self.script_revision.get().wrapping_add(1);
        self.script_revision.set(revision);
        if let Some(id) = self.script.borrow_mut().take() {
            unsafe {
                let _ = web.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id));
            }
        }
        let weak = Rc::downgrade(self);
        let done = AddScriptToExecuteOnDocumentCreatedCompletedHandler::create(Box::new(
            move |result, id| {
                let Some(state) = weak.upgrade().filter(|state| !state.closed.get()) else {
                    return Ok(());
                };
                if revision != state.script_revision.get() {
                    if let Some(web) = state.web.borrow().as_ref() {
                        unsafe {
                            let _ = web.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(id));
                        }
                    }
                    return Ok(());
                }
                if result.is_err() || id.is_empty() {
                    state.fail();
                    return Ok(());
                }
                *state.script.borrow_mut() = Some(id);
                if !state.navigated.replace(true) {
                    let url = state
                        .page
                        .borrow()
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    state.navigate(&url);
                }
                Ok(())
            },
        ));
        if unsafe { web.AddScriptToExecuteOnDocumentCreated(&HSTRING::from(script), &done) }
            .is_err()
        {
            self.fail();
        }
        self.update_context();
        if let Ok(profile) = unsafe { web.cast::<ICoreWebView2_13>().and_then(|web| web.Profile()) }
        {
            unsafe {
                let _ = profile.SetPreferredColorScheme(if self.dark.get() {
                    COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK
                } else {
                    COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT
                });
            }
        }
    }

    pub fn update_context(&self) {
        let page = self.page.borrow();
        let locale = page
            .get("locale")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let immersive = page
            .get("immersive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(web) = self.web.borrow().as_ref() {
            unsafe {
                let _ = web.ExecuteScript(&HSTRING::from(format!("window.__mixinBotSetLocale?.({}); window.__mixinBotSetImmersive?.({immersive});",json!(locale))), None);
            }
        }
    }

    pub fn navigate(&self, raw: &str) {
        if !self.closed.get()
            && is_web(raw)
            && let Some(web) = self.web.borrow().as_ref()
            && unsafe { web.Navigate(&HSTRING::from(raw)) }.is_err()
        {
            self.fail();
        }
    }

    pub fn reply(&self, reply: &Value) {
        if self.closed.get()
            || reply.get("generation").and_then(Value::as_u64) != Some(self.generation.get())
            || reply.get("page_url").and_then(Value::as_str) != self.url().as_deref()
        {
            return;
        }
        let (Some(id), Some(args)) = (
            reply.get("id").and_then(Value::as_str),
            reply.get("args").and_then(Value::as_array),
        ) else {
            return;
        };
        if id.len() > 256 {
            return;
        }
        self.pending.borrow_mut().remove(id);
        let script = format!("window.__mixinBotReply?.({}, {});", json!(id), json!(args));
        if let Some(web) = self.web.borrow().as_ref() {
            unsafe {
                let _ = web.ExecuteScript(&HSTRING::from(script), None);
            }
        }
    }
}

pub(super) fn is_web(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

pub(super) struct Host {
    context: ComponentContext,
    native: Option<Rc<dispatch::Window>>,
    visual: Option<Rc<Visual>>,
    session: String,
    pending: Option<Value>,
    state: Option<Rc<State>>,
    dark: bool,
}
impl Host {
    pub fn new(context: ComponentContext) -> Self {
        let native = dispatch::Window::embedded().ok();
        if native.is_none() {
            context.emit("failed", Value::Null);
        }
        Self {
            context,
            native,
            visual: None,
            session: String::new(),
            pending: None,
            state: None,
            dark: false,
        }
    }
    fn page(&mut self, page: &Value) {
        if self.session.is_empty() || self.visual.is_none() {
            self.pending = Some(page.clone());
            return;
        }
        if let Some(state) = &self.state {
            *state.page.borrow_mut() = page.clone();
            state.presentation();
            return;
        }
        let Some(native) = self.native.clone() else {
            return;
        };
        let Some(entry) = session::get(&self.session) else {
            self.context.emit("failed", Value::Null);
            return;
        };
        match State::open(
            self.context.clone(),
            page.clone(),
            entry,
            self.dark,
            native,
            self.visual.as_ref().unwrap().clone(),
        ) {
            Ok(state) => self.state = Some(state),
            Err(_) => self.context.emit("failed", Value::Null),
        }
    }
}
impl NativeComponent for Host {
    fn window(&self) -> Option<windows::Win32::Foundation::HWND> {
        self.native.as_ref().map(|native| native.hwnd.get())
    }
    fn composed(&self) -> bool {
        true
    }
    fn visual(
        &mut self,
        device: &windows::Win32::Graphics::DirectComposition::IDCompositionDesktopDevice,
    ) -> Option<windows::Win32::Graphics::DirectComposition::IDCompositionVisual2> {
        let visual = match Visual::new(device) {
            Ok(visual) => Rc::new(visual),
            Err(_) => {
                self.context.emit("failed", Value::Null);
                return None;
            }
        };
        let result = visual.outer.clone();
        self.visual = Some(visual);
        if let Some(page) = self.pending.take() {
            self.page(&page);
        }
        Some(result)
    }
    fn placed(&mut self, at: Option<nsh_platform_windows::Placement>) {
        if at.is_none()
            && let Some(state) = &self.state
        {
            super::input::cancel(state);
        }
        if let Some(visual) = &self.visual {
            visual.place(at);
        }
        if let Some(state) = &self.state {
            state.placement.set(at);
            if let Some(controller) = state.controller.borrow().as_ref() {
                unsafe {
                    let _ = controller.SetIsVisible(at.is_some());
                }
            }
            state.resize();
        }
    }
    fn wheel(&mut self, event: &nsh_platform_windows::Wheel) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| super::input::wheel(state, event))
    }
    fn set_prop(&mut self, name: &str, value: Option<&Value>) {
        match name {
            "session" => {
                self.session = value.and_then(Value::as_str).unwrap_or_default().to_owned();
                if let Some(page) = self.pending.take() {
                    self.page(&page);
                }
            }
            "dark" => {
                self.dark = value.and_then(Value::as_bool).unwrap_or_default();
                if let Some(state) = &self.state {
                    state.dark.set(self.dark);
                    state.presentation();
                }
            }
            "page" => {
                if let Some(page) = value {
                    self.page(page);
                }
            }
            _ => {}
        }
    }
    fn command(&mut self, name: &str, args: &Value) {
        if let Some(state) = &self.state {
            match name {
                "reply" => state.reply(args),
                "focus" => super::focus::take(state),
                "pointer" => super::input::pointer(state, args),
                "pointer-cancel" => super::input::cancel(state),
                "reload" | "back" | "forward" => {
                    if let Some(web) = state.web.borrow().as_ref() {
                        unsafe {
                            match name {
                                "reload" => {
                                    let _ = web.Reload();
                                }
                                "back" => {
                                    let _ = web.GoBack();
                                }
                                _ => {
                                    let _ = web.GoForward();
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            state.close();
        }
        if let Some(native) = &self.native {
            native.close();
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.close();
    }
}
