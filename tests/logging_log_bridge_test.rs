//! 使う側が既に`log`クレートのloggerを設定していても、`init_logging`が成功し、
//! ファイルに書く（PR #770のレビュー）。`log`のloggerも全体のsubscriberも1つの
//! プロセスで1回しか設定できないので、ほかのテストと分けて、このファイルに
//! 1つだけ置く

use mcp_rs::logging::{current_log_destination, init_logging, LogConfig};

/// 何もしない`log`のlogger（使う側が先に設定したもの）
struct HostLogger;

impl log::Log for HostLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        false
    }
    fn log(&self, _record: &log::Record) {}
    fn flush(&self) {}
}

static HOST_LOGGER: HostLogger = HostLogger;

/// `log`の橋渡しを設定できなくても、全体のsubscriberは設定したままエラーに
/// せず、ファイルの書き先も決める。以前は`try_init()`がエラーを返して途中で
/// 戻り、書き先の無いsubscriberが残って、以後のログがすべて捨てられた
#[test]
fn test_init_logging_succeeds_when_a_log_logger_is_already_set() {
    log::set_logger(&HOST_LOGGER).expect("the first logger of this process");

    let temp_dir = tempfile::tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");
    let config = LogConfig::default()
        .with_log_dir(&log_dir)
        .with_console(false)
        .with_file(true);
    let guard = init_logging(&config).expect("the log bridge failure is only a warning");
    assert!(guard.is_some());
    assert_eq!(
        current_log_destination().unwrap().dir.as_deref(),
        Some(log_dir.as_path())
    );

    // 設定の後のログも、ファイルに書かれる（書き先が決まっている）
    tracing::warn!("written after the bridge failed");
    drop(guard); // 書き残しを流す
    let written: String = std::fs::read_dir(&log_dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert!(written.contains(" started at "), "{written}");
    assert!(
        written.contains("written after the bridge failed"),
        "{written}"
    );
}
