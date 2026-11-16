use crate::i18n::Key;
use base64::Engine as _;

use super::*;
use crate::motion::Presence;

pub(super) fn dialog(
    state: State,
    presence: Presence,
    close: impl Fn() + Copy + 'static,
    failed: Signal<bool>,
) -> impl IntoElement {
    ui::dialog(presence, close, card(state, close, failed))
}

pub(super) fn card(state: State, close: impl Fn() + Copy + 'static, failed: Signal<bool>) -> View {
    let links = use_context::<crate::links::LinkNavigation>().expect("Home provides link routing");
    let link = move || {
        state.detail.with(|detail| {
            detail
                .as_ref()
                .map(|detail| detail.code_url.clone())
                .unwrap_or_default()
        })
    };
    let unavailable = move || state.busy.get() || link().is_empty();
    let header = view()
        .flex_row()
        .flex_none()
        .items_center()
        .px_6()
        .py_4()
        .gap_2()
        .child(view().size(ui::dialog_close_size()))
        .child(
            ui::dialog_title(move || t(Key::NshGroupInviteLink))
                .flex_1()
                .min_w_0()
                .text_center(),
        )
        .child(ui::dialog_close(close).m_0());
    let actions = view()
        .w_full()
        .flex_row()
        .items_stretch()
        .gap_2()
        .mt_12()
        .child(
            action(
                ui::icon(ui::Icon::Share).size(ui::icon_lg()),
                move || t(Key::NshShareInviteLink),
                move || {
                    if unavailable() {
                        return;
                    }
                    let mut request = url::Url::parse("mixin://send").expect("static Mixin URI");
                    request
                        .query_pairs_mut()
                        .append_pair("category", "text")
                        .append_pair(
                            "data",
                            &base64::engine::general_purpose::STANDARD.encode(link()),
                        );
                    close();
                    links.open(request.into(), None);
                },
            )
            .disabled(unavailable),
        )
        .child(
            action(
                ui::icon(ui::Icon::Copy).size(ui::icon_lg()),
                move || t(Key::NshCopyInviteLink),
                move || set_clipboard(link()),
            )
            .disabled(unavailable),
        )
        .child(
            action(
                dynamic(move || {
                    if state.busy.get() {
                        ui::busy_indicator(|| true, || String::new().into()).into_element()
                    } else {
                        ui::icon(ui::Icon::Rotate)
                            .size(ui::icon_lg())
                            .into_element()
                    }
                }),
                move || t(Key::NshResetInviteLink),
                move || reset(state, failed),
            )
            .disabled(move || state.busy.get() || !state.can_manage()),
        );
    let body = view()
        .w_full()
        .items_center()
        .px_6()
        .pt_12()
        .pb_12()
        .child(ui::conversation_avatar(
            move || state.row.get(),
            ui::AvatarSize::Profile,
        ))
        .child(
            text(move || {
                state
                    .row
                    .with(|row| row.as_ref().map(|row| row.name.clone()).unwrap_or_default())
            })
            .w_full()
            .mt_4()
            .text_role(ui::TextRole::Headline)
            .text_center(),
        )
        .child(
            text(move || {
                let link = link();
                if link.is_empty() {
                    t(Key::NshInvitationIsUnavailable).to_owned()
                } else {
                    link
                }
            })
            .w_full()
            .max_w(ui::dialog_width_compact())
            .mt_3()
            .text_role(ui::TextRole::Body)
            .text_center()
            .select_text(),
        )
        .child(
            text(move || t(Key::NshGroupInviteDescription))
                .w_full()
                .max_w(ui::dialog_width_compact())
                .mt_2()
                .text_role(ui::TextRole::Caption)
                .text_color(ui::fg_secondary)
                .text_center(),
        )
        .child(when(failed, move || {
            ui::error_text(move || t(Key::NshCouldnTResetTheLinkTryAgain))
                .mt_4()
                .text_center()
        }))
        .child(actions);
    ui::dialog_card(ui::DialogWidth::Regular)
        .w(ui::group_invite_width())
        .h(ui::group_invite_height())
        .p_0()
        .gap_0()
        .overflow_hidden()
        .child(header)
        .child(scroll().flex_1().min_h_0().child(body))
}

fn action(
    icon: impl IntoElement,
    label: impl Fn() -> &'static str + Copy + 'static,
    press: impl Fn() + 'static,
) -> nsh_base::ButtonElement {
    ui::button(ui::ButtonTone::Ghost, ui::ButtonSize::Regular)
        .flex_1()
        .min_w_0()
        .flex_col()
        .items_center()
        .justify_start()
        .gap_4()
        .px_1()
        .py_2()
        .accessibility_label(move || Some(label().into()))
        .on_press(press)
        .child(
            view()
                .size(ui::icon_lg())
                .flex_none()
                .items_center()
                .justify_center()
                .child(icon),
        )
        .child(
            text(label)
                .w_full()
                .text_role(ui::TextRole::Body)
                .text_center(),
        )
}

fn reset(state: State, failed: Signal<bool>) {
    if state.busy.get_untracked() || !state.can_manage() {
        return;
    }
    state.owner.run(|| {
        spawn(async move {
            if !confirm(Prompt::new(
                t(Key::NshResetInvitationLink),
                t(Key::NshThePreviousLinkWillStopWorking),
                t(Key::Reset),
            ))
            .await
                || state.busy.get_untracked()
                || !state.can_manage()
            {
                return;
            }
            let client = state.client();
            let id = state.id();
            state.busy.set(true);
            failed.set(false);
            // The screen owns the command so closing only the dialog does not lose its result.
            let result =
                runtime::commit(async move { client.conversation().rotate_group_invite(id).await })
                    .await;
            state.busy.set(false);
            if result.is_ok() {
                state.reload();
            } else {
                let _ = failed.try_set(true);
            }
        });
    });
}
