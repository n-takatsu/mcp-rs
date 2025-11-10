/// 安全なポリシー監視システムのデモンストレーション
///
/// このサンプルは、適切なライフサイクル管理を備えた
/// ポリシーファイル監視システムの使用方法を示します。
use mcp_rs::policy_watcher::PolicyFileWatcher;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定
    tracing_subscriber::fmt::init();

    info!("安全なポリシー監視システムのデモを開始");

    // 一時ディレクトリを作成（実際の使用では実際のポリシーディレクトリを指定）
    let temp_dir = tempfile::TempDir::new()?;
    let watch_path = temp_dir.path().to_string_lossy().to_string();

    // ポリシー監視システムを作成
    let watcher = PolicyFileWatcher::new(&watch_path);
    let mut receiver = watcher.subscribe();

    info!("ポリシー監視を開始: {}", watch_path);

    // 監視を開始（バックグラウンドで実行）
    watcher.start_watching().await?;

    // サンプルポリシーファイルを作成
    let policy_file = temp_dir.path().join("sample-policy.toml");
    tokio::fs::write(
        &policy_file,
        r#"
[security]
enabled = true
encryption = "AES-256"

[monitoring]
interval = "5s"
alerts = true
"#,
    )
    .await?;

    info!("サンプルポリシーファイルを作成: {:?}", policy_file);

    // イベント監視（5秒間）
    info!("ポリシー変更イベントを監視中（5秒間）...");

    let monitoring_task = tokio::spawn(async move {
        let mut event_count = 0;

        while event_count < 3 {
            match timeout(Duration::from_secs(2), receiver.recv()).await {
                Ok(Ok(event)) => {
                    info!("✓ ポリシー変更を検知: {:?}", event.change_type);
                    info!("  ファイル: {}", event.file_path);
                    info!("  時刻: {}", event.timestamp);
                    event_count += 1;
                }
                Ok(Err(e)) => {
                    warn!("イベント受信エラー: {}", e);
                    break;
                }
                Err(_) => {
                    info!("タイムアウト - 新しいイベントはありません");
                    break;
                }
            }
        }

        info!("監視タスクを終了 ({}個のイベントを処理)", event_count);
    });

    // ファイルを更新してイベントを生成
    tokio::time::sleep(Duration::from_millis(500)).await;
    tokio::fs::write(
        &policy_file,
        r#"
[security]
enabled = true
encryption = "AES-256-GCM"  # 更新
log_level = "debug"         # 新規追加

[monitoring]
interval = "3s"             # 更新
alerts = true
threshold = 0.8             # 新規追加
"#,
    )
    .await?;

    info!("ポリシーファイルを更新");

    // 監視タスクの完了を待機
    monitoring_task.await?;

    // 🔥 重要: 適切な終了処理
    info!("ポリシー監視を停止中...");
    watcher.stop();

    // 少し待ってからプロセス終了（リソース解放の確認）
    tokio::time::sleep(Duration::from_millis(100)).await;

    info!("✓ デモ完了 - すべてのリソースが適切に解放されました");

    Ok(())
}
