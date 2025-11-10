use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

use crate::canary_deployment::CanaryEvent;
use crate::error::McpError;

/// ダッシュボードイベント
#[derive(Debug, Clone)]
pub enum DashboardEvent {
    /// キーボード入力
    Key(KeyCode),
    /// ティック（定期更新）
    Tick,
    /// カナリアイベント
    CanaryEvent(CanaryEvent),
    /// 終了要求
    Quit,
}

/// イベントハンドラー
#[derive(Debug)]
pub struct EventHandler {
    /// イベント送信チャンネル
    sender: tokio::sync::mpsc::UnboundedSender<DashboardEvent>,
    /// イベント受信チャンネル
    receiver: tokio::sync::mpsc::UnboundedReceiver<DashboardEvent>,
    /// カナリアイベント購読
    canary_receiver: broadcast::Receiver<CanaryEvent>,
}

impl EventHandler {
    /// 新しいイベントハンドラーを作成
    pub fn new(canary_receiver: broadcast::Receiver<CanaryEvent>) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        Self {
            sender,
            receiver,
            canary_receiver,
        }
    }

    /// イベント処理を開始
    pub async fn run(&mut self) -> Result<(), McpError> {
        let tick_rate = Duration::from_millis(250);
        let last_tick = Instant::now();

        // ティックタイマーの残り時間を計算
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        // イベントを待機（タイムアウト付き）
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let _ = self.sender.send(DashboardEvent::Key(key.code));
                }
            }
        }

        // ティック処理
        if last_tick.elapsed() >= tick_rate {
            let _ = self.sender.send(DashboardEvent::Tick);
            // last_tickの更新は必要ないため削除（一度だけの実行）
        }

        // カナリアイベントをチェック
        while let Ok(canary_event) = self.canary_receiver.try_recv() {
            let _ = self.sender.send(DashboardEvent::CanaryEvent(canary_event));
        }

        Ok(())
    }

    /// 次のイベントを取得
    pub async fn next(&mut self) -> Option<DashboardEvent> {
        self.receiver.recv().await
    }

    /// イベントを送信
    pub fn send(&self, event: DashboardEvent) -> Result<(), McpError> {
        self.sender.send(event).map_err(|_| {
            McpError::CanaryDeployment("Failed to send dashboard event".to_string())
        })?;
        Ok(())
    }
}
