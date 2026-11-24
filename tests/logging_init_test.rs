//! `init_logging`が全体のtracingのsubscriberを設定するかを、実際に設定して
//! 確かめる（#760、PR #770のレビュー）。全体のsubscriberは1つのプロセスで
//! 1回しか設定できないので、ほかのテストと分けて、このファイルに1つだけ置く

use mcp_rs::logging::{current_log_destination, init_logging, LogConfig};

/// `with_default`で一時的にsubscriberを使った後でも、全体のsubscriberが
/// 無ければ設定でき、起動マーカーを書いて出力先を公開する。設定した後に
/// もう一度呼ぶと、panicせずにエラーを返し、何も書かない
#[test]
fn test_init_logging_after_a_scoped_subscriber_and_twice() {
    // tracing-coreは、`with_default`だけでも「設定済み」の印を立て、
    // 終わっても戻さない。以前は、この印で「既にある」と判定していた
    tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
        tracing::info!("scoped");
    });

    let temp_dir = tempfile::tempdir().unwrap();
    let log_dir = temp_dir.path().join("logs");
    let config = LogConfig::default()
        .with_log_dir(&log_dir)
        .with_console(false)
        .with_file(true);
    let guard = init_logging(&config).expect("no global subscriber is set yet");
    assert!(guard.is_some());

    let destination = current_log_destination().expect("the destination is published");
    assert_eq!(destination.dir.as_deref(), Some(log_dir.as_path()));
    let written: String = std::fs::read_dir(&log_dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert!(written.contains(" started at "), "{written}");

    // 2回目は、全体のsubscriberが既にあるので、エラー。起動マーカーを書かず、
    // 出力先も変えない
    let other_dir = temp_dir.path().join("other");
    let result = init_logging(
        &LogConfig::default()
            .with_log_dir(&other_dir)
            .with_console(false)
            .with_file(true),
    );
    assert!(result.is_err());
    assert!(!other_dir.exists());
    assert_eq!(
        current_log_destination().unwrap().dir.as_deref(),
        Some(log_dir.as_path())
    );
    drop(guard);
}
