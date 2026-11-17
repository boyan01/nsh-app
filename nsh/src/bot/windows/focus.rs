//! NSH header controls and WebView2 share the native keyboard focus cycle.
use super::window::State;
use std::rc::Rc;
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CursorChangedEventHandler,
    Microsoft::Web::WebView2::Win32::*, MoveFocusRequestedEventHandler,
};
use windows::{
    Win32::UI::{
        Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_MENU, VK_SHIFT},
        WindowsAndMessaging::{GA_ROOT, GetAncestor},
    },
    core::Result,
};

pub(super) fn take(state: &State) {
    if let Some(controller) = state.controller.borrow().as_ref() {
        unsafe {
            let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
        }
    }
}

pub(super) fn browser(state: &Rc<State>, controller: &ICoreWebView2Controller) -> Result<()> {
    let weak = Rc::downgrade(state);
    let moving = MoveFocusRequestedEventHandler::create(Box::new(move |_, args| {
        let (Some(state), Some(args)) = (weak.upgrade().filter(|s| !s.closed.get()), args) else {
            return Ok(());
        };
        let mut reason = COREWEBVIEW2_MOVE_FOCUS_REASON_NEXT;
        unsafe {
            args.Reason(&mut reason)?;
            let _ = SetFocus(Some(GetAncestor(state.native.hwnd.get(), GA_ROOT)));
            args.SetHandled(true)?;
        }
        state.emit(
            "focus-exit",
            (reason == COREWEBVIEW2_MOVE_FOCUS_REASON_PREVIOUS).into(),
        );
        Ok(())
    }));
    let mut token = 0;
    unsafe {
        controller.add_MoveFocusRequested(&moving, &mut token)?;
    }
    let live = controller.clone();
    state.removers.borrow_mut().push(Box::new(move || unsafe {
        let _ = live.remove_MoveFocusRequested(token);
    }));
    if let Some(composition) = state.composition.borrow().clone() {
        let weak = Rc::downgrade(state);
        let changed = CursorChangedEventHandler::create(Box::new(move |sender, _| {
            if let (Some(state), Some(sender)) = (
                weak.upgrade()
                    .filter(|s| !s.closed.get() && s.hovered.get()),
                sender,
            ) {
                let mut cursor = windows::Win32::UI::WindowsAndMessaging::HCURSOR::default();
                unsafe {
                    sender.Cursor(&mut cursor)?;
                    if !cursor.is_invalid() {
                        windows::Win32::UI::WindowsAndMessaging::SetCursor(Some(cursor));
                    }
                }
                let _ = state;
            }
            Ok(())
        }));
        unsafe {
            composition.add_CursorChanged(&changed, &mut token)?;
        }
        state.removers.borrow_mut().push(Box::new(move || unsafe {
            let _ = composition.remove_CursorChanged(token);
        }));
    }
    // WebView2 owns key/IME input once focused. Its native accelerator callback
    // preserves browser commands without relying on NSH seeing those keys.
    let weak = Rc::downgrade(state);
    let accelerators = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
        let (Some(state), Some(args)) = (weak.upgrade().filter(|s| !s.closed.get()), args) else {
            return Ok(());
        };
        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN;
        let mut key = 0;
        unsafe {
            args.KeyEventKind(&mut kind)?;
            args.VirtualKey(&mut key)?;
        }
        if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
            || unsafe { GetKeyState(VK_CONTROL.0 as i32) } >= 0
            || unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0
            || unsafe { GetKeyState(VK_MENU.0 as i32) } < 0
        {
            return Ok(());
        }
        if !matches!(key, 0x52 | 0x57 | 0xDB | 0xDD) {
            return Ok(());
        }
        unsafe {
            args.SetHandled(true)?;
        }
        // Never make a synchronous WebView2 call inside an accelerator handler.
        let weak = Rc::downgrade(&state);
        state.native.later(move || {
            let Some(state) = weak.upgrade().filter(|s| !s.closed.get()) else {
                return;
            };
            if key == 0x57 {
                state.close();
                return;
            }
            if let Some(web) = state.web.borrow().as_ref() {
                unsafe {
                    match key {
                        0x52 => {
                            let _ = web.Reload();
                        }
                        0xDB => {
                            let _ = web.GoBack();
                        }
                        _ => {
                            let _ = web.GoForward();
                        }
                    }
                }
            }
        });
        Ok(())
    }));
    unsafe {
        controller.add_AcceleratorKeyPressed(&accelerators, &mut token)?;
    }
    let live = controller.clone();
    state.removers.borrow_mut().push(Box::new(move || unsafe {
        let _ = live.remove_AcceleratorKeyPressed(token);
    }));
    Ok(())
}
