//! Replay ordered logical input; NSH owns native mouse capture and touch endings.
use super::{focus, window::State};
use serde_json::Value;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use windows::Win32::Foundation::POINT;

pub(crate) use super::drag::element;

pub(super) fn pointer(state: &State, args: &Value) {
    if state.closed.get() || state.placement.get().is_none() {
        return;
    }
    let Some(composition) = state.composition.borrow().clone() else {
        return;
    };
    let point = POINT {
        x: (args.get("x").and_then(Value::as_f64).unwrap_or_default() * state.scale()).round()
            as i32,
        y: (args.get("y").and_then(Value::as_f64).unwrap_or_default() * state.scale()).round()
            as i32,
    };
    let mut keys = 0;
    if args.get("shift").and_then(Value::as_bool) == Some(true) {
        keys |= 4;
    }
    if args.get("ctrl").and_then(Value::as_bool) == Some(true) {
        keys |= 8;
    }
    state
        .hovered
        .set(args.get("kind").and_then(Value::as_str) != Some("leave"));
    let kind = match args.get("kind").and_then(Value::as_str) {
        Some("down") => {
            if !state.drag.begin((point.x, point.y)) {
                return;
            }
            focus::take(state);
            keys |= 1;
            if args.get("clicks").and_then(Value::as_u64) == Some(2) {
                COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOUBLE_CLICK
            } else {
                COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN
            }
        }
        Some("up") => {
            finish(state, point, keys);
            return;
        }
        Some("drag") => {
            if !state.drag.moved((point.x, point.y)) {
                return;
            }
            keys |= 1;
            COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE
        }
        Some("move") => {
            if state.drag.active() {
                return;
            } // The tracked touch path owns drag moves.
            COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE
        }
        Some("leave") => {
            if state.drag.active() {
                return;
            }
            COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE
        }
        Some("context") => {
            focus::take(state);
            unsafe {
                let _ = composition.SendMouseInput(
                    COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
                    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys | 2),
                    0,
                    point,
                );
            }
            COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP
        }
        _ => return,
    };
    unsafe {
        let _ =
            composition.SendMouseInput(kind, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys), 0, point);
        if state.hovered.get() {
            let mut cursor = windows::Win32::UI::WindowsAndMessaging::HCURSOR::default();
            if composition.Cursor(&mut cursor).is_ok() && !cursor.is_invalid() {
                windows::Win32::UI::WindowsAndMessaging::SetCursor(Some(cursor));
            }
        }
    }
}

fn finish(state: &State, point: POINT, keys: i32) {
    state.drag.moved((point.x, point.y));
    let Some((x, y)) = state.drag.finish() else {
        return;
    };
    release(state, POINT { x, y }, keys);
}

fn release(state: &State, point: POINT, keys: i32) {
    let composition = state.composition.borrow().clone();
    if let Some(composition) = composition {
        unsafe {
            let _ = composition.SendMouseInput(
                COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
                COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys & !1),
                0,
                point,
            );
            let bounds = state.native.bounds();
            let inside =
                point.x >= 0 && point.y >= 0 && point.x < bounds.right && point.y < bounds.bottom;
            state.hovered.set(inside);
            if !inside {
                let _ = composition.SendMouseInput(
                    COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
                    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(0),
                    0,
                    point,
                );
            }
        }
    }
}

pub(super) fn cancel(state: &State) {
    let Some((x, y)) = state.drag.finish() else {
        return;
    };
    let bounds = state.native.bounds();
    let point = if x < 0 || y < 0 || x >= bounds.right || y >= bounds.bottom {
        POINT { x, y }
    } else {
        POINT { x: -1, y: -1 }
    };
    // WebView2 has no mouse cancel kind. Leaving before releasing outside the
    // view clears the pressed state without activating the pressed link/control.
    state.hovered.set(false);
    let composition = state.composition.borrow().clone();
    if let Some(composition) = composition {
        unsafe {
            let _ = composition.SendMouseInput(
                COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
                COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(0),
                0,
                POINT::default(),
            );
        }
    }
    release(state, point, 0);
}

// NativeComponent supplies wheel notches, including fractional touchpad deltas.
pub(super) fn wheel(state: &State, event: &nsh_platform_windows::Wheel) -> bool {
    let Some(composition) = state.composition.borrow().clone() else {
        return false;
    };
    let point = POINT {
        x: (f64::from(event.x) * state.scale()).round() as i32,
        y: (f64::from(event.y) * state.scale()).round() as i32,
    };
    let keys = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(
        (if event.shift { 4 } else { 0 }) | (if event.ctrl { 8 } else { 0 }),
    );
    for (delta, kind) in [
        (
            event.notches_x,
            COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL,
        ),
        (-event.notches_y, COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL),
    ] {
        if delta != 0.0 {
            unsafe {
                let _ = composition.SendMouseInput(
                    kind,
                    keys,
                    (delta * 120.0).round() as i32 as u32,
                    point,
                );
            }
        }
    }
    true
}
