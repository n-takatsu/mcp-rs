use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::error::McpError;

/// ファイル監視システム
/// 
/// ポリシーファイルの変更を監視し、変更時に通知を送信する
pub struct PolicyFileWatcher {
    /// 変更通知を送信するチャンネル
    sender: broadcast::Sender<PolicyChangeEvent>,
    /// 監視対象のディレクトリパス
    watch_path: String,
}

/// ポリシー変更イベント
#[derive(Debug, Clone)]
pub struct PolicyChangeEvent {
    /// 変更されたファイルのパス
    pub file_path: String,
    /// 変更の種類
    pub change_type: PolicyChangeType,
    /// 変更検知時刻
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// ポリシー変更の種類
#[derive(Debug, Clone)]
pub enum PolicyChangeType {
    /// ファイル作成
    Created,
    /// ファイル更新
    Modified,
    /// ファイル削除
    Deleted,
}

impl PolicyFileWatcher {
    /// 新しいファイル監視インスタンスを作成
    pub fn new(watch_path: impl Into<String>) -> Self {
        let (sender, _) = broadcast::channel(100);
        
        Self {
            sender,
            watch_path: watch_path.into(),
        }
    }

    /// ファイル監視を開始
    pub async fn start_watching(&self) -> Result<(), McpError> {
        let path = Path::new(&self.watch_path);
        
        if !path.exists() {
            warn!("監視対象パスが存在しません: {}", self.watch_path);
            return Err(McpError::ConfigError(format!("監視対象パスが存在しません: {}", self.watch_path)));
        }

        let (tx, rx) = mpsc::channel();
        let sender_clone = self.sender.clone();

        // ファイル監視の設定
        let mut watcher = RecommendedWatcher::new(
            move |result: Result<Event, notify::Error>| {
                if let Err(e) = tx.send(result) {
                    error!("ファイル監視イベントの送信に失敗: {}", e);
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(1)),
        ).map_err(|e| McpError::ConfigError(format!("ファイル監視の初期化に失敗: {}", e)))?;

        // 監視開始
        watcher.watch(path, RecursiveMode::Recursive)
            .map_err(|e| McpError::ConfigError(format!("ファイル監視の開始に失敗: {}", e)))?;

        info!("ポリシーファイル監視を開始: {}", self.watch_path);

        // 別スレッドでイベント処理
        let watch_path_clone = self.watch_path.clone();
        tokio::spawn(async move {
            while let Ok(result) = rx.recv() {
                match result {
                    Ok(event) => {
                        if let Some(change_event) = Self::process_event(event, &watch_path_clone) {
                            info!("ポリシー変更を検知: {:?}", change_event);
                            
                            if let Err(e) = sender_clone.send(change_event) {
                                error!("ポリシー変更イベントの送信に失敗: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("ファイル監視エラー: {}", e);
                    }
                }
            }
        });

        // watcherを生きたままにするため、無限ループ
        tokio::spawn(async move {
            let _watcher = watcher;
            loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });

        Ok(())
    }

    /// ファイル監視イベントを処理
    fn process_event(event: Event, watch_path: &str) -> Option<PolicyChangeEvent> {
        use notify::EventKind;

        let change_type = match event.kind {
            EventKind::Create(_) => PolicyChangeType::Created,
            EventKind::Modify(_) => PolicyChangeType::Modified,
            EventKind::Remove(_) => PolicyChangeType::Deleted,
            _ => return None,
        };

        // ポリシーファイル（.toml, .yaml, .json）のみを対象
        for path in event.paths {
            if let Some(extension) = path.extension() {
                let ext = extension.to_string_lossy().to_lowercase();
                if ext == "toml" || ext == "yaml" || ext == "yml" || ext == "json" {
                    return Some(PolicyChangeEvent {
                        file_path: path.to_string_lossy().to_string(),
                        change_type,
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }

        None
    }

    /// 変更通知を受信するためのレシーバーを取得
    pub fn subscribe(&self) -> broadcast::Receiver<PolicyChangeEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_policy_file_watcher() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        let watcher = PolicyFileWatcher::new(&temp_path);
        let mut receiver = watcher.subscribe();

        // 監視開始
        watcher.start_watching().await.unwrap();

        // テストファイル作成
        let test_file = temp_dir.path().join("test_policy.toml");
        fs::write(&test_file, "test = true").unwrap();

        // 変更イベントを待機（最大3秒）
        let result = timeout(Duration::from_secs(3), receiver.recv()).await;
        
        assert!(result.is_ok());
        let event = result.unwrap().unwrap();
        assert!(event.file_path.contains("test_policy.toml"));
        assert!(matches!(event.change_type, PolicyChangeType::Created | PolicyChangeType::Modified));
    }
}