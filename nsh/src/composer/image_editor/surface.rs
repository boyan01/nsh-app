use super::{Editor, Tool, geometry::Crop, raster_color};
use crate::i18n::Key;
use crate::ui::{self, Typography as _};
use mixin_desktop_api::ImageEdit;
use nsh_base::{SelectItem, SliderValue};
use nsh_core::prelude::*;

use crate::i18n::t;

pub(super) fn screen(
    editor: Editor,
    presence: crate::motion::Presence,
    done: impl Fn() + Copy + 'static,
    cancel: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    view()
        .child(when(
            move || {
                presence.interactive() && editor.focused.get() && editor.tool.get() == Tool::Crop
            },
            move || {
                for (key, dx, dy) in [
                    ("left", -1.0, 0.0),
                    ("right", 1.0, 0.0),
                    ("up", 0.0, -1.0),
                    ("down", 0.0, 1.0),
                ] {
                    on_key(key, move |_| editor.nudge_crop(dx, dy, false));
                    on_key(&format!("shift+{key}"), move |_| {
                        editor.nudge_crop(dx, dy, true)
                    });
                }
                view().hidden()
            },
        ))
        .child(
            ui::modal(cancel)
                .animation(nsh_core::ModalAnimation::None)
                .close_on_escape(false)
                .close_on_backdrop_press(false)
                .flex_1()
                .child(presence.surface(body(editor, done, cancel))),
        )
}

fn ratios(editor: Editor) -> impl IntoElement {
    const CHOICES: [(&str, Option<f32>); 8] = [
        ("free", None),
        ("1:1", Some(1.0)),
        ("3:2", Some(1.5)),
        ("2:3", Some(2.0 / 3.0)),
        ("4:3", Some(4.0 / 3.0)),
        ("3:4", Some(0.75)),
        ("16:9", Some(16.0 / 9.0)),
        ("9:16", Some(9.0 / 16.0)),
    ];
    ui::select()
        .items(|| {
            CHOICES
                .iter()
                .map(|(name, _)| {
                    SelectItem::new(
                        *name,
                        if *name == "free" {
                            t(Key::NshOriginalImage)
                        } else {
                            name
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .value(move || {
            Some(SharedString::from(
                CHOICES
                    .iter()
                    .find(|(_, value)| match (*value, editor.ratio.get()) {
                        (Some(a), Some(b)) => (a - b).abs() < 0.001,
                        (None, None) => true,
                        _ => false,
                    })
                    .map(|(name, _)| *name)
                    .unwrap_or("free"),
            ))
        })
        .disabled(editor.loading)
        .accessibility_label(move || Some(t(Key::NshCropRatio).into()))
        .trigger(
            ui::select_trigger(move || SharedString::from(t(Key::NshAspectRatio)))
                .min_h(ui::button_height()),
        )
        .on_change(move |value: &Option<SharedString>| {
            if let Some((_, ratio)) = value
                .as_ref()
                .and_then(|value| CHOICES.iter().find(|(name, _)| *name == value.as_str()))
            {
                let (width, height) = editor.dimensions();
                editor.ratio.set(*ratio);
                editor.crop.set(Crop::centered(width, height, *ratio));
            }
        })
}

fn colors(editor: Editor) -> impl IntoElement {
    // Palette values are user-selected raster data, not application theme colors.
    const COLORS: [([u8; 4], Key); 8] = [
        ([255, 255, 255, 255], Key::NshWhite),
        ([0, 0, 0, 255], Key::NshBlack),
        ([142, 142, 147, 255], Key::NshGray),
        ([232, 77, 61, 255], Key::NshRed),
        ([248, 205, 62, 255], Key::NshYellow),
        ([100, 211, 79, 255], Key::NshGreen),
        ([48, 119, 255, 255], Key::NshBlue),
        ([172, 104, 222, 255], Key::NshPurple),
    ];
    let custom = signal(false);
    let initial: nsh_core::color::Hsla = raster_color(editor.color.get_untracked()).into();
    let hue = signal(f64::from(initial.h) * 360.0);
    let bar_width = signal(1.0_f32);
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_center()
        .gap_2()
        .child(
            ui::command()
                .disabled(editor.loading)
                .selected(custom)
                .on_press(move || {
                    if !custom.get_untracked() {
                        let current: nsh_core::color::Hsla =
                            raster_color(editor.color.get_untracked()).into();
                        hue.set(f64::from(current.h) * 360.0);
                    }
                    custom.update(|custom| *custom = !*custom);
                })
                .child(text(move || t(Key::NshCustomColor))),
        )
        .child(when(
            move || !custom.get(),
            move || {
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .children(COLORS.into_iter().map(move |(color, key)| {
                        ui::command()
                            .size_8()
                            .px_1()
                            .py_1()
                            .disabled(editor.loading)
                            .selected(move || editor.color.get() == color)
                            // A swatch's fill is its content, so selection is its accent frame.
                            .selected_style(|style| style.border_2().border_color(ui::accent))
                            .accessibility_label(move || Some(t(key).into()))
                            .on_press(move || editor.color.set(color))
                            .child(
                                view()
                                    .size(ui::icon_md())
                                    .rounded(ui::radius_full())
                                    .border_1()
                                    .border_color(ui::border)
                                    .bg(raster_color(color)),
                            )
                    }))
            },
        ))
        .child(when(custom, move || {
            view()
                .relative()
                .w_56()
                .h_8()
                .on_layout(move |bounds: &Bounds<Points>| {
                    bar_width.set(bounds.size.width.0.max(1.0))
                })
                .child(
                    canvas(move || {
                        let width = bar_width.get();
                        let mut painter = Painter::new();
                        // The hue scale is data, not an application color.
                        for hue in 0..360 {
                            painter = painter
                                .fill_color(nsh_core::color::hsla(
                                    hue as f32 / 360.0,
                                    1.0,
                                    0.5,
                                    1.0,
                                ))
                                .fill_rect(
                                    width * hue as f32 / 360.0,
                                    0.0,
                                    width / 360.0 + 1.0,
                                    8.0,
                                );
                        }
                        painter
                    })
                    .absolute()
                    .top_3()
                    .w_full()
                    .h_2()
                    .rounded(ui::radius_full())
                    .overflow_hidden(),
                )
                .child(
                    ui::slider_over_scale()
                        .w_full()
                        .min(0.0)
                        .max(360.0)
                        .step(1.0)
                        .value(move || Some(SliderValue::Single(hue.get())))
                        .disabled(editor.loading)
                        .accessibility_label(move || Some(t(Key::NshCustomDrawingColor).into()))
                        .on_change(move |value: &SliderValue| {
                            if let SliderValue::Single(value) = value {
                                hue.set(*value);
                                // The chosen hue becomes raster data, as in the palette above.
                                let color: Rgba =
                                    nsh_core::color::hsla(*value as f32 / 360.0, 1.0, 0.5, 1.0)
                                        .into();
                                editor.color.set([
                                    (color.r * 255.0).round() as u8,
                                    (color.g * 255.0).round() as u8,
                                    (color.b * 255.0).round() as u8,
                                    255,
                                ]);
                            }
                        }),
                )
        }))
}

pub(super) fn body(
    editor: Editor,
    done: impl Fn() + Copy + 'static,
    cancel: impl Fn() + Copy + 'static,
) -> View {
    view()
        .flex_1()
        .px(ui::space_xl())
        .gap_0()
        .bg(move || ui::surface_pane().opacity(0.96))
        .child(view().h_14())
        .child(
            view()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .items_center()
                .justify_center()
                .on_layout(move |bounds: &Bounds<Points>| {
                    editor
                        .viewport
                        .set((bounds.size.width.0.max(1.0), bounds.size.height.0.max(1.0)))
                })
                .child(
                    view()
                        .relative()
                        // The image's own size, scaled to the viewport.
                        .w(move || px(editor.dimensions().0 as f32 * editor.scale()))
                        .h(move || px(editor.dimensions().1 as f32 * editor.scale()))
                        .child(
                            canvas(move || super::preview::image(editor))
                                .w_full()
                                .h_full()
                                .label(move || t(Key::NshImagePreview)),
                        )
                        .child(
                            canvas(move || super::preview::foreground(editor))
                                .absolute()
                                .top_0()
                                .left_0()
                                .w_full()
                                .h_full()
                                .focusable(true)
                                .focus(ui::ring)
                                .on_focus(move || editor.focused.set(true))
                                .on_blur(move || editor.focused.set(false))
                                .label(move || {
                                    if editor.tool.get() == Tool::Crop {
                                        t(Key::NshDragCropCornersOrMoveTheCrop)
                                    } else {
                                        t(Key::NshDrawOnTheImage)
                                    }
                                })
                                .claim_touches(TouchClaim::on_start())
                                .keep_touches(true)
                                .on_responder_grant(move |event: &ResponderEvent| {
                                    editor.start(event)
                                })
                                .on_responder_move(move |event: &ResponderEvent| {
                                    editor.moved(event)
                                })
                                .on_responder_release(move |_: &ResponderEvent| editor.end())
                                .on_responder_terminate(move |_: &ResponderEvent| {
                                    editor.stroke.set(Vec::new());
                                    editor.drag.set(None);
                                }),
                        ),
                ),
        )
        .child(when(editor.loading, || {
            text(move || t(Key::NshPreparingImage))
                .text_role(ui::TextRole::Body)
                .text_center()
                .text_color(ui::fg_secondary)
        }))
        .child(when(editor.failed, move || {
            view()
                .items_center()
                .gap_2()
                .child(ui::error_text(move || {
                    t(Key::NshCouldnTEditThisImageUndoTheLastEditOrTryAgain)
                }))
                .child(ui::link(move || t(Key::Retry), done))
        }))
        .child(when(
            move || editor.tool.get() == Tool::Crop,
            move || {
                view().items_center().mt(ui::space_xl()).child(
                    ui::action(move || t(Key::Reset), move || editor.reset())
                        .disabled(editor.loading),
                )
            },
        ))
        .child(when(
            move || editor.tool.get() != Tool::Crop,
            move || view().mt(ui::space_xl()).child(colors(editor)),
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_center()
                .mt_2()
                .gap_2()
                .child(ui::action(move || t(Key::Cancel), cancel))
                .child(when(
                    move || editor.tool.get() == Tool::Crop,
                    move || {
                        view()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                ui::action(
                                    move || t(Key::NshRotate),
                                    move || editor.edit(ImageEdit::RotateCounterClockwise),
                                )
                                .disabled(editor.loading),
                            )
                            .child(
                                ui::action(
                                    move || t(Key::NshFlip),
                                    move || editor.edit(ImageEdit::FlipHorizontal),
                                )
                                .disabled(editor.loading),
                            )
                            .child(ratios(editor))
                            .child(
                                ui::action(move || t(Key::NshDraw), move || editor.enter_drawing())
                                    .disabled(editor.loading),
                            )
                    },
                ))
                .child(when(
                    move || editor.tool.get() != Tool::Crop,
                    move || {
                        view()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                ui::command()
                                    .selected(move || editor.tool.get() == Tool::Brush)
                                    .disabled(editor.loading)
                                    .on_press(move || editor.tool.set(Tool::Brush))
                                    .child(text(move || t(Key::NshBrush))),
                            )
                            .child(
                                ui::command()
                                    .selected(move || editor.tool.get() == Tool::Eraser)
                                    .disabled(editor.loading)
                                    .on_press(move || editor.tool.set(Tool::Eraser))
                                    .child(text(move || t(Key::NshEraser))),
                            )
                            .child(
                                ui::action(move || t(Key::NshUndo), move || editor.undo())
                                    .disabled(move || {
                                        editor.loading.get() || !editor.has_drawing()
                                    }),
                            )
                            .child(
                                ui::action(move || t(Key::NshRedo), move || editor.redo())
                                    .disabled(move || {
                                        editor.loading.get() || editor.redo.with(Vec::is_empty)
                                    }),
                            )
                    },
                ))
                .child(
                    ui::primary()
                        .on_press(done)
                        .disabled(editor.loading)
                        .child(text(move || t(Key::Done))),
                ),
        )
        .child(view().h_14())
}
