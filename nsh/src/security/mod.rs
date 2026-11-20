//! Session-owned presentation for Rust's authoritative screen privacy state.

use crate::i18n::Key;
use std::{future::Future, sync::Arc, time::Duration};

use futures::StreamExt as _;
use mixin_desktop_api::{AccountClient, ClientResult, ScreenLockClient, ScreenLockState};
use nsh_core::{platform::Lifecycle, prelude::*};

use crate::{i18n, runtime};

mod passcode;
mod settings;
mod surface;
pub(crate) use settings::settings;
pub(crate) use surface::surface;

#[derive(Clone, Copy)]
pub(crate) struct Handle {
    owner: Owner,
    client: Stored<Option<ScreenLockClient>>,
    state: Signal<Option<ScreenLockState>>,
    revision: Signal<u64>,
    load_failed: Signal<bool>,
    lock_pending: Signal<bool>,
    busy: Signal<bool>,
    failed: Signal<bool>,
    cooldown_seconds: Signal<u64>,
}

pub(crate) fn mount(account: Arc<AccountClient>, initial: ScreenLockState) -> Handle {
    let handle = Handle::new(Some(account.screen_lock()), initial);
    effect(move || {
        let _revision = handle.revision.get();
        let client = handle.client();
        handle.load_failed.set(false);
        spawn(async move {
            let mut changes = Box::pin(runtime::observe(client.changes()));
            while let Some(next) = changes.next().await {
                match next {
                    Ok(state) => {
                        batch(|| {
                            handle.state.set(Some(state));
                            handle.load_failed.set(false);
                        });
                    }
                    Err(_) => {
                        handle.state.set(None);
                        handle.load_failed.set(true);
                    }
                }
            }
        });
    });
    effect(move || {
        let inactive = lifecycle() != Lifecycle::Active;
        let state = handle.state.get();
        let Some(state) = state.filter(|state| {
            state.enabled && !state.locked && state.auto_lock_minutes > 0 && inactive
        }) else {
            return;
        };
        spawn(async move {
            timer(Duration::from_secs(u64::from(state.auto_lock_minutes) * 60)).await;
            if lifecycle() != Lifecycle::Active {
                handle.lock();
            }
        });
    });
    effect(move || {
        let millis = handle
            .state
            .with(|state| state.as_ref().map_or(0, |state| state.retry_after_millis));
        handle.cooldown_seconds.set(millis.div_ceil(1000));
        if millis == 0 {
            return;
        }
        spawn(async move {
            let mut remaining = millis;
            while remaining > 0 {
                let step = remaining.min(1000);
                timer(Duration::from_millis(step)).await;
                remaining -= step;
                handle.cooldown_seconds.set(remaining.div_ceil(1000));
            }
        });
    });
    on_key("mod+shift+l", move |_| handle.lock());
    handle
}

impl Handle {
    fn new(client: Option<ScreenLockClient>, initial: ScreenLockState) -> Self {
        let cooldown_seconds = initial.retry_after_millis.div_ceil(1000);
        Self {
            owner: Owner::current(),
            client: stored(client),
            // Launch already resolved privacy. Subscribing must not introduce
            // another blocked/loading frame for an unlocked account.
            state: signal(Some(initial)),
            revision: signal(0),
            load_failed: signal(false),
            lock_pending: signal(false),
            busy: signal(false),
            failed: signal(false),
            cooldown_seconds: signal(cooldown_seconds),
        }
    }
    pub(crate) fn blocked(self) -> bool {
        self.lock_pending.get()
            || self
                .state
                .with(|state| state.as_ref().is_none_or(|state| state.locked))
    }
    pub(crate) fn enabled(self) -> bool {
        self.state
            .with(|state| state.as_ref().is_some_and(|state| state.enabled))
    }
    pub(crate) fn lock(self) {
        if !self.enabled() || self.lock_pending.get_untracked() {
            return;
        }
        self.lock_pending.set(true);
        self.submit_lock();
    }
    fn submit_lock(self) {
        let client = self.client();
        self.load_failed.set(false);
        self.owner.run(|| {
            spawn(async move {
                let result = runtime::commit(async move { client.lock().await }).await;
                let client = self.client();
                let state = runtime::run(async move { client.state().await }).await;
                match (result, state) {
                    (Ok(()), Ok(state)) => batch(|| {
                        self.state.set(Some(state));
                        self.lock_pending.set(false);
                        self.load_failed.set(false);
                    }),
                    _ => self.load_failed.set(true),
                }
            })
        });
    }
    fn client(self) -> ScreenLockClient {
        self.client.with(|client| {
            client
                .clone()
                .expect("a detached preview runs no security commands")
        })
    }
    fn retry(self) {
        if self.lock_pending.get_untracked() {
            self.submit_lock();
        } else {
            self.revision.update(|revision| *revision += 1);
        }
    }
    fn command(
        self,
        future: impl Future<Output = ClientResult<()>> + crate::runtime::MaybeSend + 'static,
    ) {
        if self.busy.get_untracked() {
            return;
        }
        self.busy.set(true);
        self.failed.set(false);
        self.owner.run(|| {
            spawn(async move {
                let result = runtime::commit(future).await;
                self.finish(result).await;
            })
        });
    }
    fn unlock(self, passcode: String) {
        if self.cooldown_seconds.get_untracked() > 0 {
            return;
        }
        let client = self.client();
        self.command(async move { client.unlock(passcode).await });
    }
    fn authenticate(self) {
        if self.busy.get_untracked() {
            return;
        }
        let client = self.client();
        let i18n = i18n::current();
        let reason = i18n.t(Key::UnlockMixinMessenger).to_owned();
        self.busy.set(true);
        self.failed.set(false);
        self.owner.run(|| {
            spawn(async move {
                // Cancellation invalidates the native authentication context so
                // quitting cannot wait forever for an abandoned system prompt.
                let result =
                    runtime::run(async move { client.unlock_with_biometrics(reason).await }).await;
                self.finish(result).await;
            })
        });
    }
    async fn finish(self, result: ClientResult<()>) {
        let client = self.client();
        match runtime::run(async move { client.state().await }).await {
            Ok(state) => self.state.set(Some(state)),
            Err(_) => {
                self.state.set(None);
                self.load_failed.set(true);
            }
        }
        self.failed.set(result.is_err());
        self.busy.set(false);
    }
}

#[cfg(debug_assertions)]
pub(crate) fn preview(kind: &str, failed: Signal<bool>) -> impl IntoElement {
    let handle = Handle {
        owner: Owner::current(),
        client: stored(None),
        state: signal(Some({
            let mut state = ScreenLockState::default();
            state.available = true;
            state.enabled = true;
            state.locked = true;
            state.auto_lock_minutes = 5;
            state.biometrics_available = true;
            state.biometric_enabled = true;
            state
        })),
        revision: signal(0),
        load_failed: signal(false),
        lock_pending: signal(false),
        busy: signal(false),
        failed: signal(false),
        cooldown_seconds: signal(0),
    };
    let locked = handle.state.get_untracked();
    effect(move || {
        handle.load_failed.set(failed.get());
        handle
            .state
            .set(if failed.get() { None } else { locked.clone() });
    });
    match kind {
        "unavailable" => {
            handle.state.set(None);
            handle.load_failed.set(true);
            surface(handle).into_element()
        }
        "setup" => settings::preview_setup(handle).into_element(),
        _ => surface::preview_inline(handle).into_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsh_core::element::{Runtime, create_root};

    #[test]
    fn resolved_launch_privacy_mounts_unlocked_accounts_without_a_blocked_frame() {
        let graph = Runtime::default();
        let _entered = graph.enter();
        let (root, ()) = create_root(|| {
            for (enabled, locked) in [(false, false), (true, false), (true, true)] {
                let mut initial = ScreenLockState::default();
                initial.available = true;
                initial.enabled = enabled;
                initial.locked = locked;
                let handle = Handle::new(None, initial.clone());

                assert_eq!(handle.state.get_untracked(), Some(initial));
                assert_eq!(handle.blocked(), locked);
                assert_eq!(handle.enabled(), enabled);
                assert!(!handle.load_failed.get_untracked());
            }
        });
        root.dispose();
    }

    #[test]
    fn unlocked_launch_blocks_on_later_lock_or_missing_state_and_recovers_on_unlock() {
        let graph = Runtime::default();
        let _entered = graph.enter();
        let (root, ()) = create_root(|| {
            let mut unlocked = ScreenLockState::default();
            unlocked.available = true;
            unlocked.enabled = true;
            let handle = Handle::new(None, unlocked.clone());
            assert!(!handle.blocked());

            handle.lock_pending.set(true);
            assert!(handle.blocked());
            let mut locked = unlocked.clone();
            locked.locked = true;
            handle.state.set(Some(locked));
            handle.lock_pending.set(false);
            assert!(handle.blocked());

            handle.state.set(None);
            assert!(handle.blocked());
            handle.state.set(Some(unlocked));
            assert!(!handle.blocked());
        });
        root.dispose();
    }

    #[test]
    fn locked_launch_preserves_passcode_cooldown_before_effects_run() {
        let graph = Runtime::default();
        let _entered = graph.enter();
        let (root, ()) = create_root(|| {
            let mut initial = ScreenLockState::default();
            initial.enabled = true;
            initial.locked = true;
            initial.retry_after_millis = 1501;
            let handle = Handle::new(None, initial);
            assert!(handle.blocked());
            assert_eq!(handle.cooldown_seconds.get_untracked(), 2);
        });
        root.dispose();
    }
}
