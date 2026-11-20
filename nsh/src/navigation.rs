//! Session navigation shared by messages, profiles, search, and native links.

use mixin_desktop_api::{AccountClient, ConversationListData};
use nsh_core::prelude::*;
use std::sync::Arc;

use crate::{runtime, ui};

const DRAWER_MAX_WIDTH: f32 = 384.0;
const COMPACT_MAX_WIDTH: f32 = 496.0;
const CHAT_SPLIT_MIN_WIDTH: f32 = 620.0;
const TERMINAL_SPLIT_MIN_COLUMNS: f32 = 100.0;

/// Category, circle and rail layout state shared by the account screen.
#[derive(Clone, Copy)]
pub(crate) struct Navigation {
    pub category: Signal<&'static str>,
    pub circle: Signal<Option<String>>,
    pub collapsed: Signal<bool>,
    pub settings: Signal<bool>,
    pub list_reset: Signal<u64>,
    drawer_open: Signal<bool>,
}

impl Navigation {
    pub fn new() -> Self {
        Self {
            category: signal("chats"),
            circle: signal(None),
            collapsed: signal(false),
            settings: signal(false),
            list_reset: signal(0),
            drawer_open: signal(false),
        }
    }
    pub fn drawer(self) -> bool {
        if crate::terminal::active() {
            crate::terminal::columns() < TERMINAL_SPLIT_MIN_COLUMNS
        } else {
            window().width.0 <= DRAWER_MAX_WIDTH
        }
    }
    pub fn forced_compact(self) -> bool {
        crate::terminal::active() || window().width.0 < COMPACT_MAX_WIDTH
    }
    pub fn narrow(self) -> bool {
        if crate::terminal::active() {
            crate::terminal::columns() < TERMINAL_SPLIT_MIN_COLUMNS
        } else {
            window().width.0 - self.width().0 < CHAT_SPLIT_MIN_WIDTH
        }
    }
    pub fn compact(self) -> bool {
        !self.drawer() && (self.collapsed.get() || self.forced_compact())
    }
    pub fn width(self) -> Points {
        if self.drawer() {
            Points::ZERO
        } else if self.compact() {
            ui::navigation_compact_width()
        } else {
            ui::navigation_width()
        }
    }
    /// The next width change eases every frame it moves, titles inside the panes included.
    pub fn animate_width(self) {
        if !reduce_motion() {
            nsh_core::element::animate_layout(nsh_core::LayoutAnimation {
                update: Some(crate::motion::PANEL_WIDTH),
                ..Default::default()
            });
        }
    }
    pub fn drawer_open(self) -> bool {
        self.drawer_open.get()
    }
    pub fn open_drawer(self) {
        if self.drawer() {
            self.drawer_open.set(true);
        }
    }
    pub fn close_drawer(self) {
        self.drawer_open.set(false);
    }
    pub fn select(self, category: &'static str, circle: Option<String>) {
        batch(|| {
            self.category.set(category);
            self.circle.set(circle);
            self.settings.set(false);
            self.list_reset
                .update(|revision| *revision = revision.wrapping_add(1));
            self.close_drawer();
        });
    }
}

/// The right-hand inspector destination for the open chat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Panel {
    Info,
    Search,
    Pinned,
}

#[derive(Clone, Copy)]
pub(crate) struct ChatNavigation {
    owner: Owner,
    selected: Signal<Option<ConversationListData>>,
    pub selected_id: Memo<Option<String>>,
    target: Signal<Option<String>>,
    settings: Signal<bool>,
    request: Signal<u64>,
    recent: Signal<Vec<String>>,
    search_highlight: Signal<Option<(String, String)>>,
    search_from_info: Signal<bool>,
    pub panel: Signal<Option<Panel>>,
    inspector_return: Signal<Option<Panel>>,
    pub return_target: Signal<Option<String>>,
}

impl ChatNavigation {
    /// Scrolls the open chat to its newest message, e.g. after the person sends.
    pub(crate) fn show_latest(self) {
        // `target` notifies on every write, so None reloads the latest rows.
        self.target.set(None);
    }

    pub(crate) fn new(
        selected: Signal<Option<ConversationListData>>,
        target: Signal<Option<String>>,
        settings: Signal<bool>,
    ) -> Self {
        let request = signal(0_u64);
        let recent = signal(Vec::<String>::new());
        let search_highlight = signal(None::<(String, String)>);
        let panel = signal(None);
        let inspector_return = signal(None);
        let search_from_info = signal(false);
        let selected_id = computed(move || {
            selected.with(|row| row.as_ref().map(|row| row.conversation_id.clone()))
        });
        effect(move || {
            let _scope = (selected_id.get(), target.get(), settings.get());
            search_from_info.set(false);
        });
        effect(move || {
            if panel.get() != Some(Panel::Search) {
                search_from_info.set(false);
            }
        });
        effect(move || {
            let destination = panel.get();
            let returning = inspector_return.get();
            // Existing entry points such as the inline latest pin set the destination directly.
            if destination.is_some() && returning.is_some() && destination != returning {
                inspector_return.set(None);
            }
        });
        effect(move || {
            let id = selected_id.get();
            panel.set(None);
            inspector_return.set(None);
            if search_highlight.with_untracked(|value| {
                value
                    .as_ref()
                    .is_some_and(|(highlighted, _)| Some(highlighted) != id.as_ref())
            }) {
                search_highlight.set(None);
            }
            if let Some(id) = id {
                recent.update(|ids| {
                    ids.retain(|existing| existing != &id);
                    ids.insert(0, id);
                    ids.truncate(5);
                });
            }
        });
        // Invalidate a pending link when another entry point changes navigation.
        // Search, create, and native menus still own their existing selection signals.
        effect(move || {
            selected.with(|_| ());
            let _target = target.get();
            let _settings = settings.get();
            request.update(|revision| *revision += 1);
        });
        Self {
            owner: Owner::current(),
            selected,
            selected_id,
            target,
            settings,
            request,
            recent,
            search_highlight,
            search_from_info,
            panel,
            inspector_return,
            return_target: signal(None),
        }
    }

    pub(crate) fn cancel_pending(self) {
        self.request.update(|revision| *revision += 1);
    }

    pub(crate) fn open_search_from_info(self) {
        batch(|| {
            self.clear_inspector_return();
            self.search_from_info.set(true);
            self.panel.set(Some(Panel::Search));
        });
    }

    pub(crate) fn close_conversation(self) {
        self.selected.set(None);
    }

    pub(crate) fn back_panel(self) {
        batch(|| {
            self.clear_inspector_return();
            let next = if self.panel.get_untracked() == Some(Panel::Search)
                && self.search_from_info.get_untracked()
            {
                Some(Panel::Info)
            } else {
                None
            };
            self.search_from_info.set(false);
            self.panel.set(next);
        });
    }

    pub(crate) fn toggle_panel(self, mode: Panel) {
        if self.panel.get_untracked() == Some(mode) {
            self.back_panel();
        } else {
            batch(|| {
                self.clear_inspector_return();
                self.search_from_info.set(false);
                self.panel.set(Some(mode));
            });
        }
    }

    pub(crate) fn inspector_return(self) -> Option<Panel> {
        self.inspector_return.get()
    }

    /// Keep the current inspector's owner and native list alive during a timeline jump.
    pub(crate) fn suspend_inspector(self, mode: Panel) {
        if matches!(mode, Panel::Search | Panel::Pinned) && self.panel.get_untracked() == Some(mode)
        {
            batch(|| {
                self.search_from_info.set(false);
                self.inspector_return.set(Some(mode));
                self.panel.set(None);
            });
        }
    }

    pub(crate) fn resume_inspector(self) {
        if let Some(mode) = self.inspector_return.get_untracked() {
            self.panel.set(Some(mode));
        }
    }

    fn clear_inspector_return(self) {
        self.inspector_return.set(None);
    }

    /// Outside dismissal always closes the inspector, including Search opened from Info.
    pub(crate) fn dismiss_inspector(self) {
        batch(|| {
            self.clear_inspector_return();
            self.search_from_info.set(false);
            self.panel.set(None);
        });
    }

    pub(crate) fn search_term(self, conversation_id: &str) -> String {
        self.search_highlight.with(|highlight| {
            highlight
                .as_ref()
                .filter(|(id, _)| id == conversation_id)
                .map(|(_, query)| query.clone())
                .unwrap_or_default()
        })
    }

    pub(crate) fn highlight_search(self, conversation_id: String, query: String) {
        self.search_highlight
            .set(Some((conversation_id, query.trim().to_owned())));
    }

    pub(crate) fn jump_in_selected(
        self,
        conversation_id: &str,
        message_id: String,
        origin: Option<String>,
    ) -> bool {
        if !self.is_selected(conversation_id) {
            return false;
        }
        batch(|| {
            self.cancel_pending();
            self.settings.set(false);
            self.panel.set(None);
            self.clear_inspector_return();
            self.search_highlight.set(None);
            self.return_target.set(origin);
            self.target.set(Some(message_id));
        });
        true
    }

    pub(crate) fn recent_ids(self) -> Vec<String> {
        self.recent.get()
    }

    fn is_selected(self, conversation_id: &str) -> bool {
        self.selected_id
            .with_untracked(|id| id.as_deref() == Some(conversation_id))
    }

    pub(crate) fn track(self, account: Arc<AccountClient>) {
        use futures::StreamExt as _;
        self.owner.run(|| {
            spawn(async move {
                let mut changes = Box::pin(runtime::observe(account.conversation_changes()));
                while let Some(event) = changes.next().await {
                    let Some(id) = self.selected_id.get() else {
                        continue;
                    };
                    if !event.reload_all && !event.conversation_ids.contains(&id) {
                        continue;
                    }
                    let client = account.clone();
                    let requested_id = id.clone();
                    if let Ok(rows) = runtime::run(async move {
                        client
                            .conversation()
                            .conversation_items_by_ids(vec![requested_id])
                            .await
                    })
                    .await
                        && rows.is_empty()
                        && self.is_selected(&id)
                    {
                        batch(|| {
                            self.selected.set(None);
                            self.target.set(None);
                            self.panel.set(None);
                            self.clear_inspector_return();
                            self.return_target.set(None);
                        });
                    }
                }
            });
        });
    }

    pub(crate) fn select(self, conversation: ConversationListData) {
        batch(|| {
            self.cancel_pending();
            self.target.set(None);
            self.panel.set(None);
            self.clear_inspector_return();
            self.return_target.set(None);
            self.search_highlight.set(None);
            self.settings.set(false);
            self.selected.set(Some(conversation));
        });
    }

    pub(crate) fn open_conversation(
        self,
        account: Arc<AccountClient>,
        conversation_id: String,
        target_message_id: Option<String>,
        failed: Signal<bool>,
    ) {
        self.open_message(
            account,
            conversation_id,
            target_message_id,
            None,
            failed,
            || {},
        );
    }

    pub(crate) fn open_search_message(
        self,
        account: Arc<AccountClient>,
        conversation_id: String,
        message_id: String,
        query: String,
        failed: Signal<bool>,
        opened: impl FnOnce() + 'static,
    ) {
        self.open_message(
            account,
            conversation_id,
            Some(message_id),
            Some(query),
            failed,
            opened,
        );
    }

    fn open_message(
        self,
        account: Arc<AccountClient>,
        conversation_id: String,
        target_message_id: Option<String>,
        search_term: Option<String>,
        failed: Signal<bool>,
        opened: impl FnOnce() + 'static,
    ) {
        self.request.update(|revision| *revision += 1);
        let request = self.request.get_untracked();
        self.owner.run(|| {
            spawn(async move {
                let result = runtime::run(async move {
                    let row = account
                        .conversation()
                        .conversation_items_by_ids(vec![conversation_id])
                        .await?
                        .into_iter()
                        .next()
                        .ok_or(mixin_desktop_api::ClientError::NotFound)?;
                    // A deleted message must not leave the chat waiting on a missing target.
                    let target = if let Some(id) = target_message_id {
                        account
                            .message()
                            .message_items_by_ids(vec![id])
                            .await?
                            .into_iter()
                            .find(|message| message.conversation_id == row.conversation_id)
                            .map(|message| message.message_id)
                    } else {
                        None
                    };
                    Ok((row, target))
                })
                .await;
                if self.request.get_untracked() != request {
                    return;
                }
                match result {
                    Ok((row, target)) => {
                        batch(|| {
                            self.settings.set(false);
                            self.panel.set(None);
                            self.clear_inspector_return();
                            self.return_target.set(None);
                            self.search_highlight.set(
                                search_term
                                    .filter(|_| target.is_some())
                                    .map(|term| (row.conversation_id.clone(), term)),
                            );
                            self.selected.set(Some(row));
                            self.target.set(target);
                        });
                        opened();
                    }
                    Err(_) => failed.set(true),
                }
            });
        });
    }

    pub(crate) fn open_user(
        self,
        account: Arc<AccountClient>,
        user_id: String,
        failed: Signal<bool>,
    ) {
        self.open_user_then(account, user_id, failed, || {});
    }

    pub(crate) fn open_user_then(
        self,
        account: Arc<AccountClient>,
        user_id: String,
        failed: Signal<bool>,
        opened: impl FnOnce() + 'static,
    ) {
        self.request.update(|revision| *revision += 1);
        let request = self.request.get_untracked();
        self.owner.run(|| {
            spawn(async move {
                let result = runtime::commit(async move {
                    let access = account.conversation();
                    let id = access.open_user_conversation(user_id).await?;
                    access
                        .conversation_items_by_ids(vec![id])
                        .await?
                        .into_iter()
                        .next()
                        .ok_or(mixin_desktop_api::ClientError::NotFound)
                })
                .await;
                if self.request.get_untracked() != request {
                    return;
                }
                match result {
                    Ok(row) => {
                        self.select(row);
                        opened();
                    }
                    Err(_) => failed.set(true),
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsh_core::element::{Runtime, create_root};

    fn with_navigation(run: impl FnOnce(ChatNavigation)) {
        let runtime = Runtime::default();
        let _entered = runtime.enter();
        let (root, navigation) =
            create_root(|| ChatNavigation::new(signal(None), signal(None), signal(false)));
        run(navigation);
        root.dispose();
    }

    #[test]
    fn open_search_or_pinned_when_suspended_can_return_repeatedly_to_the_same_panel() {
        for mode in [Panel::Search, Panel::Pinned] {
            with_navigation(|navigation| {
                navigation.toggle_panel(mode);
                for _ in 0..2 {
                    navigation.suspend_inspector(mode);
                    assert_eq!(navigation.panel.get_untracked(), None);
                    assert_eq!(navigation.inspector_return(), Some(mode));
                    navigation.resume_inspector();
                    assert_eq!(navigation.panel.get_untracked(), Some(mode));
                    assert_eq!(navigation.inspector_return(), Some(mode));
                }
            });
        }
    }

    #[test]
    fn return_session_when_dismissed_open_or_closed_cannot_reopen_the_inspector() {
        for mode in [Panel::Search, Panel::Pinned] {
            for reopen in [false, true] {
                with_navigation(|navigation| {
                    navigation.toggle_panel(mode);
                    navigation.suspend_inspector(mode);
                    if reopen {
                        navigation.resume_inspector();
                    }
                    navigation.dismiss_inspector();
                    navigation.resume_inspector();
                    assert_eq!(navigation.panel.get_untracked(), None);
                    assert_eq!(navigation.inspector_return(), None);
                });
            }
        }
    }

    #[test]
    fn returned_panel_when_closed_normally_or_replaced_clears_the_return_session() {
        for mode in [Panel::Search, Panel::Pinned] {
            with_navigation(|navigation| {
                navigation.toggle_panel(mode);
                navigation.suspend_inspector(mode);
                navigation.resume_inspector();
                navigation.back_panel();
                assert_eq!(navigation.panel.get_untracked(), None);
                assert_eq!(navigation.inspector_return(), None);

                navigation.toggle_panel(mode);
                navigation.suspend_inspector(mode);
                navigation.panel.set(Some(Panel::Info));
                assert_eq!(navigation.panel.get_untracked(), Some(Panel::Info));
                assert_eq!(navigation.inspector_return(), None);
            });
        }
    }

    #[test]
    fn search_opened_from_info_when_suspended_and_returned_closes_without_returning_to_info() {
        with_navigation(|navigation| {
            navigation.open_search_from_info();
            navigation.suspend_inspector(Panel::Search);
            navigation.resume_inspector();
            navigation.back_panel();
            assert_eq!(navigation.panel.get_untracked(), None);
            assert_eq!(navigation.inspector_return(), None);
        });
    }

    #[test]
    fn active_conversation_when_updated_keeps_return_but_switching_conversations_resets_it() {
        with_navigation(|navigation| {
            let row = conversation("first");
            navigation.selected.set(Some(row.clone()));
            navigation.toggle_panel(Panel::Search);
            navigation.suspend_inspector(Panel::Search);

            let mut updated = row;
            updated.name = "Updated name".into();
            navigation.selected.set(Some(updated));
            assert_eq!(navigation.inspector_return(), Some(Panel::Search));

            navigation.selected.set(Some(conversation("second")));
            navigation.resume_inspector();
            assert_eq!(navigation.panel.get_untracked(), None);
            assert_eq!(navigation.inspector_return(), None);
        });
    }

    fn conversation(id: &str) -> ConversationListData {
        ConversationListData {
            conversation_id: id.into(),
            owner_id: String::new(),
            name: String::new(),
            avatar_url: String::new(),
            category: "GROUP".into(),
            draft: String::new(),
            status: 2,
            last_read_message_id: None,
            last_message: String::new(),
            last_message_category: None,
            last_message_status: None,
            last_message_sender_id: None,
            last_message_sender_name: None,
            last_message_action: None,
            last_message_participant_id: None,
            last_message_participant_name: None,
            updated_at_millis: 0,
            unseen_count: 0,
            mention_count: 0,
            is_muted: false,
            is_verified: false,
            is_scam: false,
            is_bot: false,
            is_bot_group: false,
            membership: None,
            is_pinned: false,
            pin_time_millis: 0,
            relationship: String::new(),
            identity_number: String::new(),
            circle_ids: Vec::new(),
            participant_count: 0,
            group_avatars: Vec::new(),
            expire_in: 0,
        }
    }

    #[test]
    fn info_or_a_different_panel_when_suspended_does_not_create_a_return_session() {
        with_navigation(|navigation| {
            navigation.toggle_panel(Panel::Info);
            navigation.suspend_inspector(Panel::Info);
            navigation.suspend_inspector(Panel::Search);
            assert_eq!(navigation.panel.get_untracked(), Some(Panel::Info));
            assert_eq!(navigation.inspector_return(), None);
        });
    }
}
