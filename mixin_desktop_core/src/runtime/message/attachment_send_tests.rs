use super::*;
use crate::{db::mixin::message::MediaStatus, runtime::MessageAccess};
use sdk::MessageStatus;

#[tokio::test]
async fn pending_upload_enqueue_returns_persisted_message_and_allows_another_send() {
    let (started, uploading) = tokio::sync::oneshot::channel();
    let (release, blocked) = tokio::sync::oneshot::channel();
    let fixture = Fixture::build_with_upload_gate(None, Some((started, blocked))).await;
    let database = &fixture.runtime.database;
    crate::db::query("INSERT INTO conversations (conversation_id, category, created_at, status) VALUES ('chat', 'CONTACT', ?, 2)")
        .bind(chrono::Utc::now()).execute(&database.message_dao.0).await.unwrap();
    let source = fixture._directory.path().join("source.txt");
    std::fs::write(&source, b"attachment bytes").unwrap();
    let access = MessageAccess::new(fixture.runtime.state.clone());
    let accepted = tokio::time::timeout(
        Duration::from_secs(2),
        access.enqueue_attachment(
            "chat".into(),
            source.to_string_lossy().into_owned(),
            "DATA".into(),
            "text/plain".into(),
            Some("source.txt".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        ),
    )
    .await;
    if accepted.is_err() {
        // Release the fixture before failing so no task or temporary file is leaked.
        let _ = release.send(());
        drop(access);
        fixture.close().await;
        panic!("local acceptance must return while remote upload is blocked");
    }
    let message_id = accepted.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(2), uploading)
        .await
        .unwrap()
        .unwrap();
    let message = database
        .message_dao
        .find_message_by_id(&message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(message.media_status, MediaStatus::Pending);
    assert_eq!(message.status, MessageStatus::Sending);
    assert!(fixture
        .runtime
        .attachment_downloads
        .lock()
        .unwrap()
        .contains_key(&message_id));
    let imported =
        crate::core::attachment::attachment_path(&fixture.runtime.account_data_dir, &message)
            .unwrap();
    // Clearing the tray may release its temporary source as soon as enqueue returns.
    std::fs::remove_file(source).unwrap();
    assert_eq!(std::fs::read(imported).unwrap(), b"attachment bytes");
    let text_id = tokio::time::timeout(
        Duration::from_secs(2),
        access.send_text("chat".into(), "next message".into(), None, false),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        database
            .message_dao
            .find_message_by_id(&text_id)
            .await
            .unwrap()
            .unwrap()
            .content
            .as_deref(),
        Some("next message")
    );
    // A disposed view/client cannot cancel the already accepted upload.
    drop(access);
    let mut events = fixture.runtime.conversation_changes.subscribe_events();
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            events.recv().await.unwrap();
            let message = database
                .message_dao
                .find_message_by_id(&message_id)
                .await
                .unwrap()
                .unwrap();
            if message.media_status == MediaStatus::Canceled {
                assert_eq!(message.status, MessageStatus::Sending);
                break;
            }
        }
    })
    .await
    .unwrap();
    // Retain the same row/file for the existing timeline upload retry command.
    assert!(database
        .message_dao
        .find_message_by_id(&message_id)
        .await
        .unwrap()
        .is_some());
    assert!(!fixture
        .runtime
        .attachment_downloads
        .lock()
        .unwrap()
        .contains_key(&message_id));
    fixture.close().await;
}

#[tokio::test]
async fn rejected_local_insert_enqueue_returns_error_without_starting_an_upload() {
    let fixture = Fixture::new().await;
    let database = &fixture.runtime.database;
    crate::db::query("INSERT INTO conversations (conversation_id, category, created_at, status) VALUES ('chat', 'CONTACT', ?, 2)")
        .bind(chrono::Utc::now())
        .execute(&database.message_dao.0)
        .await
        .unwrap();
    crate::db::query("CREATE TRIGGER reject_attachment BEFORE INSERT ON messages BEGIN SELECT RAISE(FAIL, 'fixture insert rejected'); END")
        .execute(&database.message_dao.0).await.unwrap();
    let source = fixture._directory.path().join("source.txt");
    std::fs::write(&source, b"attachment bytes").unwrap();
    let access = MessageAccess::new(fixture.runtime.state.clone());
    let result = access
        .enqueue_attachment(
            "chat".into(),
            source.to_string_lossy().into_owned(),
            "DATA".into(),
            "text/plain".into(),
            Some("source.txt".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            false,
        )
        .await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("fixture insert rejected"));
    let messages: i64 = crate::db::query_scalar("SELECT COUNT(*) FROM messages")
        .fetch_one(&database.message_dao.0)
        .await
        .unwrap();
    assert_eq!(messages, 0);
    assert!(fixture
        .runtime
        .attachment_downloads
        .lock()
        .unwrap()
        .is_empty());
    drop(access);
    fixture.close().await;
}

#[tokio::test]
async fn pending_upload_normal_shutdown_cancels_without_waiting_for_remote_completion() {
    let (started, uploading) = tokio::sync::oneshot::channel();
    let (release, blocked) = tokio::sync::oneshot::channel();
    let fixture = Fixture::build_with_upload_gate(None, Some((started, blocked))).await;
    let database = &fixture.runtime.database;
    crate::db::query("INSERT INTO conversations (conversation_id, category, created_at, status) VALUES ('chat', 'CONTACT', ?, 2)")
        .bind(chrono::Utc::now()).execute(&database.message_dao.0).await.unwrap();
    let source = fixture._directory.path().join("source.txt");
    std::fs::write(&source, b"attachment bytes").unwrap();
    let access = MessageAccess::new(fixture.runtime.state.clone());
    let message_id = access
        .enqueue_attachment(
            "chat".into(),
            source.to_string_lossy().into_owned(),
            "DATA".into(),
            "text/plain".into(),
            Some("source.txt".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            false,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), uploading)
        .await
        .unwrap()
        .unwrap();
    let stopped = tokio::time::timeout(Duration::from_secs(2), fixture.runtime.shutdown()).await;
    if stopped.is_err() {
        // Clean up the intentionally stalled upload before reporting the regression.
        let _ = release.send(());
        drop(access);
        fixture.close().await;
        panic!("normal shutdown must cancel uploads before waiting for their mutation guard");
    }
    assert!(
        !release.is_closed(),
        "remote gate must remain held through shutdown"
    );
    assert!(!fixture
        .runtime
        .active
        .load(std::sync::atomic::Ordering::Acquire));
    assert!(fixture
        .runtime
        .attachment_downloads
        .lock()
        .unwrap()
        .is_empty());
    let message = database
        .message_dao
        .find_message_by_id(&message_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(message.media_status, MediaStatus::Canceled);
    assert_eq!(message.status, MessageStatus::Sending);
    let imported =
        crate::core::attachment::attachment_path(&fixture.runtime.account_data_dir, &message)
            .unwrap();
    assert_eq!(std::fs::read(imported).unwrap(), b"attachment bytes");
    assert!(database.job_dao.sending_jobs().await.unwrap().is_empty());
    use futures::StreamExt as _;
    let (_stop, live) = watch::channel(false);
    let progress = fixture.runtime.attachment_progresses.changes(live);
    futures::pin_mut!(progress);
    assert!(progress.next().await.unwrap().is_empty());
    assert!(
        access
            .enqueue_attachment(
                "chat".into(),
                fixture
                    ._directory
                    .path()
                    .join("source.txt")
                    .to_string_lossy()
                    .into_owned(),
                "DATA".into(),
                "text/plain".into(),
                Some("source.txt".into()),
                None,
                None,
                None,
                None,
                None,
                None,
                false,
            )
            .await
            .is_err(),
        "shutdown must stop accepting new sends"
    );
    drop(access);
    // The local server is stopped by teardown; never release its remote response.
    let _held_gate = release;
    fixture.close().await;
}

#[tokio::test]
async fn admitted_upload_registered_after_shutdown_scan_is_already_canceled() {
    let fixture = Fixture::new().await;
    // Model an admitted send still importing its local file: it owns the same
    // read guard and has not registered the upload when shutdown scans transfers.
    let mutation = fixture.runtime.mutation_gate.read().await;
    let mut shutdown = Box::pin(fixture.runtime.shutdown());
    assert!(futures::poll!(shutdown.as_mut()).is_pending());
    assert!(!fixture
        .runtime
        .active
        .load(std::sync::atomic::Ordering::Acquire));
    assert!(fixture
        .runtime
        .attachment_downloads
        .lock()
        .unwrap()
        .is_empty());
    let cancellation = fixture.runtime.register_attachment_upload("late-upload");
    assert!(
        cancellation.is_cancelled(),
        "a late upload must observe shutdown even after its cancellation scan"
    );
    fixture
        .runtime
        .attachment_downloads
        .lock()
        .unwrap()
        .remove("late-upload");
    drop(mutation);
    tokio::time::timeout(Duration::from_secs(2), shutdown)
        .await
        .unwrap();
    fixture.close().await;
}
