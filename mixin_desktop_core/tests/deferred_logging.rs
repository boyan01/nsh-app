//! The native host may attempt logger installation between discovery and launch.
use mixin_desktop_core::{db::path, runtime::logging};

struct HostLogger;
impl log::Log for HostLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, _: &log::Record<'_>) {}
    fn flush(&self) {}
}

#[test]
fn discovery_reserves_logging_without_touching_data_then_launch_writes_records() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    path::configure_data_directory(data.clone()).unwrap();
    logging::install_deferred().unwrap();
    assert!(!data.exists(), "discovery must not open application data");
    assert!(
        log::set_logger(&HostLogger).is_err(),
        "the host must retain application logging"
    );
    logging::init("Mixin".into(), "test".into(), "1".into()).unwrap();
    logging::init("Mixin".into(), "test".into(), "1".into()).unwrap();
    log::info!("deferred-logger-info-regression");
    log::warn!("deferred-logger-warning-regression");
    log::logger().flush();
    let text = std::fs::read_to_string(data.join("log/log_0.log")).unwrap();
    assert!(text.contains("deferred-logger-info-regression"));
    assert!(text.contains("deferred-logger-warning-regression"));
    assert_eq!(text.matches("# app: Mixin").count(), 1);
}

#[test]
fn file_only_logging_keeps_stdout_clean_after_default_initialization() {
    const DATA_DIR: &str = "MIXIN_FILE_LOGGING_TEST_DIR";
    const MARKER: &str = "file-only-logging-regression";
    if let Some(data) = std::env::var_os(DATA_DIR) {
        path::configure_data_directory(data.into()).unwrap();
        logging::install_deferred().unwrap();
        logging::init_with_console("Mixin".into(), "test".into(), "1".into(), false).unwrap();
        logging::init("Mixin".into(), "test".into(), "1".into()).unwrap();
        log::info!("{MARKER}");
        log::logger().flush();
        return;
    }

    // Process-global logger ownership must be fresh for this scenario.
    let directory = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "file_only_logging_keeps_stdout_clean_after_default_initialization",
            "--nocapture",
        ])
        .env(DATA_DIR, directory.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(MARKER));
    let text = std::fs::read_to_string(directory.path().join("log/log_0.log")).unwrap();
    assert!(text.contains(MARKER));
}
