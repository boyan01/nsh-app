use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use mixin_desktop_api::{AccountClient, ClientError, ClientResult};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::{AbortOnDropHandle, TaskTracker};

// Outlive NSH's engine so quitting can drain the business runtime.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ACCEPTING_COMMITS: Mutex<bool> = Mutex::new(true);
static COMMITS: OnceLock<TaskTracker> = OnceLock::new();
static ACCOUNT: Mutex<Option<Arc<AccountClient>>> = Mutex::new(None);
static DRAIN_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static SHUTDOWN: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static STOPPING: OnceLock<CancellationToken> = OnceLock::new();

fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(|| Runtime::new().expect("create the business runtime"))
}

fn stopping() -> &'static CancellationToken {
    STOPPING.get_or_init(CancellationToken::new)
}

async fn until_shutdown<T>(future: impl Future<Output = T>) -> ClientResult<T> {
    tokio::select! {
        biased;
        _ = stopping().cancelled() => {
            Err(ClientError::Internal("Application is shutting down".into()))
        }
        result = future => Ok(result),
    }
}

/// Business work crosses to the runtime's threads on the desktop. Browser
/// builds have one thread, where the same UI code runs without this bound.
pub(crate) trait MaybeSend: Send {}
impl<T: Send> MaybeSend for T {}

/// Logs a failure where the UI asked for the work: views turn errors into a
/// state such as "failed", which alone would leave nothing to diagnose.
fn logged<T>(
    caller: &'static std::panic::Location<'static>,
    result: ClientResult<T>,
) -> ClientResult<T> {
    if let Err(error) = &result {
        log::warn!(
            target: "ui",
            "{}:{}: {error}",
            caller.file(),
            caller.line()
        );
    }
    result
}

#[track_caller]
pub(crate) fn run<T: Send + 'static>(
    future: impl Future<Output = ClientResult<T>> + Send + 'static,
) -> impl Future<Output = ClientResult<T>> {
    let caller = std::panic::Location::caller();
    // Disposing the UI owner must cancel its background work too.
    let task =
        AbortOnDropHandle::new(runtime().spawn(async move { until_shutdown(future).await? }));
    async move {
        logged(
            caller,
            task.await
                .map_err(|error| ClientError::Internal(format!("Background task failed: {error}")))
                .and_then(|result| result),
        )
    }
}

/// Polls business subscriptions on the business runtime, with bounded delivery.
/// Dropping the UI subscription aborts its producer and releases its source.
pub(crate) fn observe<S>(source: S) -> impl futures::Stream<Item = S::Item>
where
    S: futures::Stream + Send + 'static,
    S::Item: Send + 'static,
{
    use futures::StreamExt as _;
    let (sender, receiver) = tokio::sync::mpsc::channel(32);
    let task = AbortOnDropHandle::new(runtime().spawn(until_shutdown(async move {
        futures::pin_mut!(source);
        while let Some(item) = source.next().await {
            if sender.send(item).await.is_err() {
                break;
            }
        }
    })));
    futures::stream::unfold((receiver, task), |(mut receiver, task)| async move {
        receiver.recv().await.map(|item| (item, (receiver, task)))
    })
}

/// Wakes once per burst of `source` events, so a busy sync cannot keep restarting
/// debounced UI queries or queue one reload per message.
pub(crate) fn throttled<S>(source: S, interval: Duration) -> impl futures::Stream<Item = ()>
where
    S: futures::Stream + Send + 'static,
    S::Item: Send,
{
    use futures::{FutureExt as _, StreamExt as _};
    observe(futures::stream::unfold(
        Box::pin(source),
        move |mut source| async move {
            source.next().await?;
            tokio::time::sleep(interval).await;
            while source.next().now_or_never().flatten().is_some() {}
            Some(((), source))
        },
    ))
}

// A user-committed write must finish even if navigation disposes its view.
#[track_caller]
pub(crate) fn commit<T: Send + 'static>(
    future: impl Future<Output = ClientResult<T>> + Send + 'static,
) -> impl Future<Output = ClientResult<T>> {
    let caller = std::panic::Location::caller();
    let admission = ACCEPTING_COMMITS.lock().expect("commit admission");
    let task = if *admission {
        COMMITS
            .get_or_init(TaskTracker::new)
            .spawn_on(future, runtime().handle())
    } else {
        runtime().spawn(async { Err(ClientError::Internal("Application is shutting down".into())) })
    };
    drop(admission);
    async move {
        logged(
            caller,
            task.await
                .map_err(|error| ClientError::Internal(format!("Command failed: {error}")))
                .and_then(|result| result),
        )
    }
}

pub(crate) fn retain_account(account: Arc<AccountClient>) {
    *ACCOUNT.lock().expect("account lock") = Some(account);
}

pub(crate) fn release_account() {
    ACCOUNT.lock().expect("account lock").take();
}

/// Closes a persistent write queue at shutdown and drains accepted items,
/// even while the UI still retains its sender.
pub(crate) async fn receive_committed<T>(
    receiver: &mut tokio::sync::mpsc::Receiver<T>,
) -> Option<T> {
    tokio::select! {
        biased;
        _ = stopping().cancelled() => {
            receiver.close();
            receiver.recv().await
        }
        item = receiver.recv() => item,
    }
}

pub(crate) fn stop() {
    *ACCEPTING_COMMITS.lock().expect("commit admission") = false;
    // Release queries holding account locks and close persistent write queues
    // before waiting for committed local work. UI disposal comes after draining.
    stopping().cancel();
    if let Some(account) = ACCOUNT.lock().expect("account lock").take() {
        *SHUTDOWN.lock().expect("shutdown lock") = Some(runtime().spawn(async move {
            if let Some(commits) = COMMITS.get() {
                commits.close();
                commits.wait().await;
            }
            account.shutdown().await;
            log::info!("NSH account shutdown completed");
        }));
    }
}

/// Unlike view-owned work, the shutdown drain must survive its own stop signal.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
pub(crate) async fn shutdown() {
    if let Err(error) = runtime().spawn(drain_shutdown()).await {
        log::warn!("Runtime shutdown failed: {error}");
    }
}

/// Routes terminal termination through the native loop so committed work can drain.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn quit_on_signal(quit: fn()) -> std::io::Result<AbortOnDropHandle<()>> {
    use tokio::signal::unix::{SignalKind, signal};
    let _entered = runtime().enter();
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    Ok(AbortOnDropHandle::new(runtime().spawn(async move {
        tokio::select! {
            _ = terminate.recv() => {},
            _ = interrupt.recv() => {},
        }
        quit();
    })))
}

/// Drains account shutdown after the native event loop returns.
pub fn finish_shutdown() {
    runtime().block_on(drain_shutdown());
}

/// Completes committed writes and account shutdown before disposing the UI engine.
pub async fn drain_shutdown() {
    let _gate = DRAIN_GATE.lock().await;
    stop();
    let task = SHUTDOWN.lock().expect("shutdown lock").take();

    if !drain_tasks(COMMITS.get().cloned(), task).await {
        log::warn!("Timed out waiting for runtime shutdown");
    }
}

async fn drain_tasks(commits: Option<TaskTracker>, task: Option<JoinHandle<()>>) -> bool {
    let task = task.map(AbortOnDropHandle::new);
    let drain = async move {
        // Process settings can be committed from the sign-in screen too.
        // Their lifetime must not depend on retaining a signed-in account.
        if let Some(commits) = commits {
            commits.close();
            commits.wait().await;
        }
        if let Some(task) = task {
            let _ = task.await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), drain)
        .await
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn shutdown_rejects_new_commits_without_executing_them() {
        const CHILD: &str = "MIXIN_COMMIT_ADMISSION_TEST";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "runtime::tests::shutdown_rejects_new_commits_without_executing_them",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        drain_shutdown().await;
        let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = executed.clone();
        assert!(
            commit(async move {
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .await
            .is_err()
        );
        assert!(!executed.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_drains_writes_beyond_the_engine_quit_budget() {
        let tracker = TaskTracker::new();
        let write = tracker.spawn(async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            42
        });
        assert!(drain_tasks(Some(tracker), None).await);
        assert_eq!(write.await.unwrap(), 42);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_bounds_a_stalled_account_and_aborts_it() {
        let task = tokio::spawn(std::future::pending::<()>());
        let abort = task.abort_handle();
        assert!(!drain_tasks(None, Some(task)).await);
        tokio::task::yield_now().await;
        assert!(abort.is_finished());
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_committed_write_still_aborts_the_waiting_account_shutdown() {
        let tracker = TaskTracker::new();
        let write = tracker.spawn(std::future::pending::<()>());
        let task = tokio::spawn(std::future::pending::<()>());
        let abort = task.abort_handle();
        assert!(!drain_tasks(Some(tracker), Some(task)).await);
        tokio::task::yield_now().await;
        assert!(abort.is_finished());
        write.abort();
        assert!(write.await.unwrap_err().is_cancelled());
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_closes_a_retained_write_queue_and_drains_accepted_items() {
        const CHILD: &str = "MIXIN_SHUTDOWN_QUEUE_TEST";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "runtime::tests::shutdown_closes_a_retained_write_queue_and_drains_accepted_items",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }

        let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
        sender.try_send(1).unwrap();
        sender.try_send(2).unwrap();
        let tracker = TaskTracker::new();
        let writer = tracker.spawn(async move {
            let mut written = Vec::new();
            while let Some(item) = receive_committed(&mut receiver).await {
                // Local persistence must finish, even beyond the engine budget.
                tokio::time::sleep(Duration::from_secs(1)).await;
                written.push(item);
            }
            written
        });
        let started = tokio::time::Instant::now();
        // Reproduce the production order: stop admission, drain, dispose UI.
        drain_shutdown().await;
        let drained = drain_tasks(Some(tracker), None).await;
        let elapsed = started.elapsed();
        drop(sender);
        assert_eq!(writer.await.unwrap(), vec![1, 2]);
        assert!(
            drained,
            "retained UI sender exhausted the 10-second shutdown budget"
        );
        assert_eq!(elapsed, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn shutdown_cancels_background_work_before_draining_committed_writes() {
        const CHILD: &str = "MIXIN_SHUTDOWN_BACKGROUND_TEST";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "runtime::tests::shutdown_cancels_background_work_before_draining_committed_writes",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }

        // Core operations can retain a mutation read guard across network I/O.
        // Neither a pending query nor a subscription may block the final write.
        let gate = Arc::new(tokio::sync::RwLock::new(()));
        let query_gate = gate.clone();
        let (started, ready) = oneshot::channel();
        let query = tokio::spawn(run(async move {
            let _guard = query_gate.read().await;
            started.send(()).unwrap();
            std::future::pending::<ClientResult<()>>().await
        }));
        let source_gate = gate.clone();
        let (started, source_ready) = oneshot::channel();
        let source = observe(futures::stream::once(async move {
            let _guard = source_gate.read().await;
            started.send(()).unwrap();
            std::future::pending::<()>().await
        }));
        ready.await.unwrap();
        source_ready.await.unwrap();

        let write = commit(async move {
            let _guard = gate.write().await;
            Ok(42)
        });
        let drained = tokio::time::timeout(Duration::from_millis(500), shutdown()).await;
        // Clean up on the failing path as well, before asserting the deadline.
        query.abort();
        let _ = query.await;
        drop(source);
        assert_eq!(write.await.unwrap(), 42);
        assert!(
            drained.is_ok(),
            "background work blocked shutdown past 500 ms"
        );
    }

    struct NotifyOnDrop(Option<oneshot::Sender<()>>);

    impl Drop for NotifyOnDrop {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    #[tokio::test]
    async fn disposing_an_awaiting_ui_task_cancels_its_business_future() {
        let (started, ready) = oneshot::channel();
        let (cancelled, dropped) = oneshot::channel();
        let task = tokio::spawn(run(async move {
            let _guard = NotifyOnDrop(Some(cancelled));
            started.send(()).expect("test is waiting for startup");
            std::future::pending::<ClientResult<()>>().await
        }));
        tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(2), dropped)
            .await
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn committed_writes_finish_when_the_view_stops_waiting() {
        let (started, ready) = oneshot::channel();
        let (resume, proceed) = oneshot::channel();
        let (completed, done) = oneshot::channel();
        let waiting = commit(async move {
            started.send(()).unwrap();
            proceed.await.unwrap();
            completed.send(()).unwrap();
            Ok(())
        });
        drop(waiting);
        tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        resume.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), done)
            .await
            .unwrap()
            .unwrap();
    }
}
