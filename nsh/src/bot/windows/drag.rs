//! One press owns the browser until release or cancellation, even outside its bounds.
use nsh_core::{ContextMenuEvent, MouseEvent, PressEvent, TouchEvent, TouchPhase, prelude::*};
use serde_json::{Value, json};
use std::cell::Cell;

pub(crate) fn element<T: Interactive>(element: T, reference: ElementRef) -> T {
    nsh_core::element::desktop::current_window().on_focus_change(move |focused| {
        if !focused {
            reference.command("pointer-cancel", Value::Null);
        }
    });
    element.focusable(true)
        .on_focus(move || reference.command("focus", Value::Null))
        .on_press_down(move |e: &PressEvent| reference.command("pointer", json!({"kind":"down", "x":e.position.x.0, "y":e.position.y.0, "clicks":e.clicks, "shift":e.modifiers.shift, "ctrl":e.modifiers.ctrl})))
        // Windows keeps the initial touch path while its root HWND owns capture.
        // End/Cancel reach it outside the hit area, including before commands return.
        .on_touch(move |e: &TouchEvent| {
            let Some(point) = e.points.iter().find(|p| p.changed) else { return; };
            let kind = match e.phase {
                TouchPhase::Start => return, // PressDown carries the logical click count.
                TouchPhase::Move => "drag",
                TouchPhase::End => "up",
                TouchPhase::Cancel => { reference.command("pointer-cancel", Value::Null); return; },
            };
            reference.command("pointer", json!({"kind":kind, "x":point.location.x.0, "y":point.location.y.0, "shift":e.modifiers.shift, "ctrl":e.modifiers.ctrl}));
        })
        .on_mouse_move(move |e: &MouseEvent| reference.command("pointer", json!({"kind":"move", "x":e.position.x.0, "y":e.position.y.0, "shift":e.modifiers.shift, "ctrl":e.modifiers.ctrl})))
        .on_mouse_leave(move |_: &MouseEvent| reference.command("pointer", json!({"kind":"leave"})))
        .on_context_menu(move |e: &ContextMenuEvent| reference.command("pointer", json!({"kind":"context", "x":e.position.x.0, "y":e.position.y.0, "shift":e.modifiers.shift, "ctrl":e.modifiers.ctrl})))
}

#[derive(Default)]
pub(super) struct Drag(Cell<Option<(i32, i32)>>);
impl Drag {
    pub fn begin(&self, point: (i32, i32)) -> bool {
        if self.active() {
            return false;
        }
        self.0.set(Some(point));
        true
    }
    pub fn active(&self) -> bool {
        self.0.get().is_some()
    }
    pub fn moved(&self, point: (i32, i32)) -> bool {
        if !self.active() {
            return false;
        }
        self.0.set(Some(point));
        true
    }
    pub fn finish(&self) -> Option<(i32, i32)> {
        self.0.take()
    }
}

#[cfg(test)]
mod tests {
    use super::Drag;
    #[test]
    fn a_drag_keeps_outside_coordinates_and_finishes_only_once() {
        let drag = Drag::default();
        assert!(drag.begin((10, 20)));
        assert!(drag.moved((-30, 900)));
        assert!(
            !drag.begin((15, 25)),
            "reentry must not start another press"
        );
        assert_eq!(drag.finish(), Some((-30, 900)));
        assert_eq!(drag.finish(), None, "release after cancellation is ignored");
        assert!(!drag.moved((0, 0)));
        assert!(drag.begin((30, 40)), "a later gesture gets new ownership");
        assert_eq!(drag.finish(), Some((30, 40)));
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::element;
    use nsh_core::{
        Command, EventKind, EventPayload, PointerType, PressEvent, ROOT_ID, TouchEvent, TouchPhase,
        TouchPoint,
        platform::{Environment, Os},
        prelude::*,
    };
    use nsh_engine::{EngineConfig, Root};
    use nsh_testing::Headless;
    use std::{
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };

    thread_local! { static SETUP: std::cell::RefCell<Option<Arc<Gate>>> = const { std::cell::RefCell::new(None) }; }

    struct Gate {
        entered: mpsc::SyncSender<()>,
        resume: Mutex<mpsc::Receiver<()>>,
    }

    #[component]
    fn Fixture() -> impl IntoElement {
        nsh_core::element::desktop::window()
            .id("main")
            .content(fixture_content)
    }

    fn fixture_content() -> impl IntoElement {
        let gate = SETUP.with(|setup| setup.borrow_mut().take()).unwrap();
        let reference = element_ref();
        view()
            .child(element(
                native("TestBrowser")
                    .element_ref(&reference)
                    .test_id("browser")
                    .size(px(100.0)),
                reference,
            ))
            .child(
                nsh_base::button()
                    .child(text("Block engine"))
                    .on_press(move || {
                        gate.entered.send(()).unwrap();
                        gate.resume
                            .lock()
                            .unwrap()
                            .recv_timeout(Duration::from_secs(5))
                            .unwrap();
                    }),
            )
    }

    fn replay(
        ending: TouchPhase,
        queued: bool,
        end: (f32, f32),
        moved: bool,
    ) -> Vec<(String, serde_json::Value)> {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = mpsc::sync_channel(1);
        let gate = Arc::new(Gate {
            entered: entered_tx,
            resume: Mutex::new(resume_rx),
        });
        let mut app = Headless::start(EngineConfig {
            environment: Environment {
                os: Os::Windows,
                ..Default::default()
            },
            ..EngineConfig::new(Root::new::<Fixture>(move || {
                SETUP.with(|setup| *setup.borrow_mut() = Some(gate.clone()));
            }))
        })
        .unwrap();
        app.wait_for("Block engine").unwrap();
        let browser = app.screen().id("id:browser").unwrap();
        let blocker = app.screen().press_target("Block engine").unwrap().unwrap();
        app.clear_commands();
        let control = app.engine().control();
        control.dispatch_event(blocker, EventKind::Press, EventPayload::None);
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Platform events are queued before any handler or native command runs.
        let touch = |phase, x, y| {
            EventPayload::Touch(Box::new(TouchEvent {
                phase,
                timestamp: if phase == TouchPhase::Start { 1.0 } else { 2.0 },
                points: vec![TouchPoint {
                    id: 17,
                    target: browser,
                    path: if phase == TouchPhase::Start {
                        vec![browser]
                    } else {
                        vec![]
                    },
                    page: Point::new(px(x), px(y)),
                    location: Point::new(px(x), px(y)),
                    changed: true,
                    pointer_type: PointerType::Mouse,
                    pressure: TouchPoint::default_pressure(phase, true),
                    tilt_x: 0.0,
                    tilt_y: 0.0,
                }],
                activated: None,
                modifiers: Default::default(),
            }))
        };
        control.dispatch_event(
            ROOT_ID,
            EventKind::Touch,
            touch(TouchPhase::Start, 10.0, 20.0),
        );
        control.dispatch_event(
            browser,
            EventKind::PressDown,
            EventPayload::Press(PressEvent {
                position: Point::new(px(10.0), px(20.0)),
                clicks: 2,
                ..Default::default()
            }),
        );
        let pointer_commands = |screen: &nsh_testing::Screen| {
            screen.commands().iter().filter(|(_, cmd)| matches!(cmd, Command::Native { name, .. } if name.as_ref() == "pointer" || name.as_ref() == "pointer-cancel")).count()
        };
        if !queued {
            resume_tx.send(()).unwrap();
            app.wait_until("down handoff", Duration::from_secs(1), |screen| {
                pointer_commands(screen) >= 1
            })
            .unwrap();
        }
        if moved {
            control.dispatch_event(
                ROOT_ID,
                EventKind::Touch,
                touch(TouchPhase::Move, end.0, end.1),
            );
        }
        // The native root supplies End/Cancel on the original path, regardless of hit.
        control.dispatch_event(ROOT_ID, EventKind::Touch, touch(ending, end.0, end.1));
        if queued {
            resume_tx.send(()).unwrap();
        }
        app.wait_until(
            "ordered pointer completion",
            Duration::from_secs(1),
            |screen| pointer_commands(screen) >= 2 + usize::from(moved),
        )
        .unwrap();
        app.screen()
            .commands()
            .iter()
            .filter_map(|(_, cmd)| match cmd {
                Command::Native { name, args } if name.as_ref() == "pointer" => {
                    Some((args["kind"].as_str().unwrap().to_owned(), args.clone()))
                }
                Command::Native { name, args } if name.as_ref() == "pointer-cancel" => {
                    Some(("cancel".into(), args.clone()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn native_end_and_cancel_keep_their_meaning_across_the_engine_handoff() {
        for (ending, queued, end, moved) in [
            (TouchPhase::End, true, (10.0, 20.0), false),
            (TouchPhase::End, true, (-20.0, 130.0), true),
            (TouchPhase::End, false, (-20.0, 130.0), true),
            (TouchPhase::Cancel, true, (10.0, 20.0), false),
            (TouchPhase::Cancel, false, (-20.0, 130.0), true),
        ] {
            let commands = replay(ending, queued, end, moved);
            let mut expected = vec!["down"];
            if moved {
                expected.push("drag");
            }
            expected.push(if ending == TouchPhase::End {
                "up"
            } else {
                "cancel"
            });
            assert_eq!(
                commands
                    .iter()
                    .map(|(kind, _)| kind.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                commands[0].1["clicks"], 2,
                "keep native logical double-click metadata"
            );
            if ending == TouchPhase::End {
                let args = &commands.last().unwrap().1;
                assert_eq!(args["x"], end.0);
                assert_eq!(args["y"], end.1);
            }
        }
    }
}
