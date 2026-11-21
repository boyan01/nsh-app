use super::{Command, Handle};
use crate::i18n::Key;
use crate::i18n::t;
use nsh_core::{
    desktop::MenuRole,
    element::desktop::{MenuItem, set_application_menu},
    prelude::*,
};

fn quit_item(shell: Handle) -> MenuItem {
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if !crate::terminal::active() {
        return MenuItem::new(t(Key::QuitMixin))
            .shortcut("mod+q")
            .on_select(move || shell.quit());
    }
    let _ = shell;
    MenuItem::role(MenuRole::Quit).label(t(Key::QuitMixin))
}

pub(super) fn mount(shell: Handle) {
    effect(move || {
        let terminal = crate::terminal::active();
        let signed = shell.signed_in.get() && !shell.blocked.get();
        let chat = signed && shell.has_chat.get();
        let action = move |key: Key, command: Command, enabled: bool| {
            MenuItem::new(t(key))
                .disabled(!enabled)
                .on_select(move || shell.dispatch(command))
        };
        let help = |key, url: &'static str| {
            MenuItem::new(t(key)).on_select(move || {
                spawn(async move {
                    let _ = open_url(url).await;
                });
            })
        };
        let mut app = vec![
            MenuItem::role(MenuRole::About).label(t(Key::NshAboutMixinMessenger)),
            MenuItem::separator(),
            action(Key::NshSettingsMessage, Command::Preferences, signed).shortcut("mod+,"),
            action(
                Key::Lock,
                Command::Lock,
                shell.signed_in.get() && shell.can_lock.get(),
            )
            .shortcut("mod+shift+l"),
            MenuItem::separator(),
            action(Key::NshQuickSearchMessage, Command::QuickSearch, signed).shortcut("mod+k"),
        ];
        if !terminal {
            app.extend([
                MenuItem::new(t(Key::HideMixin))
                    .shortcut("mod+h")
                    .on_select(move || shell.hide()),
                MenuItem::new(t(Key::ShowMixin)).on_select(move || shell.show()),
            ]);
        }
        app.extend([MenuItem::separator(), quit_item(shell)]);
        let mut file = vec![
            action(Key::NshNewConversation, Command::NewChat, signed).shortcut("mod+n"),
            action(Key::NshNewGroup, Command::NewGroup, signed).shortcut("mod+shift+n"),
            action(Key::NshNewCircleMessage, Command::NewCircle, signed),
            action(
                Key::NshChatBackupAndRestoreMessage,
                Command::Transfer,
                signed,
            ),
        ];
        if !terminal {
            file.extend([MenuItem::separator(), close_item(shell)]);
        }
        let mut window_items = Vec::new();
        if !terminal {
            window_items.extend([
                MenuItem::role(MenuRole::Minimize),
                MenuItem::role(MenuRole::Zoom),
                MenuItem::separator(),
            ]);
        }
        window_items.extend([
            action(Key::PreviousConversation, Command::PreviousChat, signed)
                .shortcut("mod+up")
                .shortcut_when(nsh_core::keymap::When::NotEditing),
            action(Key::NextConversation, Command::NextChat, signed)
                .shortcut("mod+down")
                .shortcut_when(nsh_core::keymap::When::NotEditing),
        ]);
        if !terminal {
            window_items.extend([
                MenuItem::separator(),
                MenuItem::new(t(Key::ShowMixin))
                    .shortcut("mod+o")
                    .on_select(move || shell.show()),
            ]);
        }
        set_application_menu(vec![
            MenuItem::submenu("Mixin", app),
            MenuItem::submenu(t(Key::File), file),
            MenuItem::submenu(
                t(Key::Edit),
                [
                    MenuItem::role(MenuRole::Undo),
                    MenuItem::role(MenuRole::Redo),
                    MenuItem::separator(),
                    MenuItem::role(MenuRole::Cut),
                    MenuItem::role(MenuRole::Copy),
                    MenuItem::role(MenuRole::Paste),
                    MenuItem::role(MenuRole::SelectAll),
                ],
            ),
            MenuItem::submenu(
                t(Key::Conversation),
                [
                    action(
                        if shell.muted.get() {
                            Key::Unmute
                        } else {
                            Key::NshMute
                        },
                        Command::Mute,
                        chat,
                    ),
                    action(Key::NshSearch, Command::SearchConversation, chat),
                    action(Key::NshDeleteChat, Command::DeleteConversation, chat),
                    action(
                        if shell.pinned.get() {
                            Key::Unpin
                        } else {
                            Key::PinTitle
                        },
                        Command::Pin,
                        chat,
                    ),
                    action(Key::ToggleChatInfo, Command::ToggleInfo, chat),
                ],
            ),
            MenuItem::submenu(t(Key::Window), window_items),
            MenuItem::submenu(
                t(Key::Help),
                [
                    help(Key::NshHelpCenter, "https://support.mixin.one/"),
                    help(Key::NshTermsOfService, "https://mixin.one/pages/terms"),
                    help(Key::NshPrivacyPolicy, "https://mixin.one/pages/privacy"),
                ],
            ),
        ]);
    });
}

fn close_item(shell: Handle) -> MenuItem {
    #[cfg(target_os = "macos")]
    {
        let _ = shell;
        // AppKit routes performClose: to the key window, including auxiliary
        // windows. The main window's keep-alive policy retains its draft.
        MenuItem::role(MenuRole::Close).label(t(Key::CloseWindow))
    }
    #[cfg(not(target_os = "macos"))]
    {
        MenuItem::new(t(Key::CloseWindow))
            .shortcut("mod+w")
            .on_select(move || shell.hide())
    }
}
