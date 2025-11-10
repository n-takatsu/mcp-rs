use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    terminal::Terminal,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, LineGauge, Paragraph, Tabs},
    Frame,
};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

use crate::canary_deployment::{
    CanaryDeploymentManager, CanaryEvent, DeploymentState, MetricsSnapshot,
};
use crate::error::McpError;

/// ダッシュボードアプリケーションの状態管理
#[derive(Debug)]
pub struct DashboardApp {
    /// カナリアデプロイメント管理システムへの参照
    canary_manager: Arc<CanaryDeploymentManager>,
    /// イベント受信チャンネル
    event_receiver: broadcast::Receiver<CanaryEvent>,
    /// アプリケーション状態
    state: DashboardState,
    /// 最新のメトリクス
    current_metrics: Option<MetricsSnapshot>,
    /// イベント履歴（最新50件）
    event_history: Vec<CanaryEvent>,
    /// 選択中のタブ
    selected_tab: usize,
    /// 開始時刻
    started_at: Instant,
    /// 保留中のトラフィック分散設定
    pending_traffic_split: Option<f32>,
}

/// ダッシュボードの状態
#[derive(Debug, Clone, PartialEq)]
pub enum DashboardState {
    /// 通常の監視モード
    Monitoring,
    /// 設定変更モード
    Configuration,
    /// ヘルプ表示モード
    Help,
    /// 終了確認
    ConfirmExit,
}

/// ダッシュボードタブ
#[derive(Debug, Clone)]
pub enum DashboardTab {
    Overview,
    Metrics,
    Events,
    Control,
}

impl DashboardApp {
    /// 新しいダッシュボードアプリケーションを作成
    pub fn new(canary_manager: Arc<CanaryDeploymentManager>) -> Self {
        let event_receiver = canary_manager.subscribe();

        Self {
            canary_manager,
            event_receiver,
            state: DashboardState::Monitoring,
            current_metrics: None,
            event_history: Vec::with_capacity(50),
            selected_tab: 0,
            started_at: Instant::now(),
            pending_traffic_split: None,
        }
    }

    /// イベントを処理
    pub fn handle_event(&mut self, event: Event) -> Result<bool, McpError> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match self.state {
                DashboardState::Monitoring => self.handle_monitoring_keys(key.code),
                DashboardState::Configuration => self.handle_configuration_keys(key.code),
                DashboardState::Help => self.handle_help_keys(key.code),
                DashboardState::ConfirmExit => self.handle_exit_confirmation_keys(key.code),
            },
            _ => Ok(false),
        }
    }

    /// 監視モードでのキー入力処理
    fn handle_monitoring_keys(&mut self, key: KeyCode) -> Result<bool, McpError> {
        match key {
            KeyCode::Char('q') => {
                self.state = DashboardState::ConfirmExit;
                Ok(false)
            }
            KeyCode::Char('h') => {
                self.state = DashboardState::Help;
                Ok(false)
            }
            KeyCode::Char('c') => {
                self.state = DashboardState::Configuration;
                Ok(false)
            }
            KeyCode::Tab => {
                self.selected_tab = (self.selected_tab + 1) % 4;
                Ok(false)
            }
            KeyCode::BackTab => {
                self.selected_tab = if self.selected_tab == 0 {
                    3
                } else {
                    self.selected_tab - 1
                };
                Ok(false)
            }
            KeyCode::Char('r') => {
                // 手動更新
                self.update_metrics()?;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// 設定モードでのキー入力処理
    fn handle_configuration_keys(&mut self, key: KeyCode) -> Result<bool, McpError> {
        match key {
            KeyCode::Esc => {
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            KeyCode::Char('1') => {
                // トラフィック分散を10%に設定（非同期処理は後で実行）
                self.pending_traffic_split = Some(10.0);
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            KeyCode::Char('2') => {
                // トラフィック分散を25%に設定
                self.pending_traffic_split = Some(25.0);
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            KeyCode::Char('3') => {
                // トラフィック分散を50%に設定
                self.pending_traffic_split = Some(50.0);
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            KeyCode::Char('4') => {
                // トラフィック分散を75%に設定
                self.pending_traffic_split = Some(75.0);
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            KeyCode::Char('5') => {
                // トラフィック分散を100%に設定
                self.pending_traffic_split = Some(100.0);
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// ヘルプモードでのキー入力処理
    fn handle_help_keys(&mut self, key: KeyCode) -> Result<bool, McpError> {
        match key {
            KeyCode::Esc | KeyCode::Char('h') => {
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// 終了確認でのキー入力処理
    fn handle_exit_confirmation_keys(&mut self, key: KeyCode) -> Result<bool, McpError> {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => Ok(true), // 終了
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.state = DashboardState::Monitoring;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// メトリクスを更新
    fn update_metrics(&mut self) -> Result<(), McpError> {
        // カナリアデプロイメント管理システムから最新のメトリクスを取得
        // この機能は将来のバージョンで実装予定
        Ok(())
    }

    /// 新しいイベントをチェック
    pub fn check_for_events(&mut self) {
        while let Ok(event) = self.event_receiver.try_recv() {
            // イベント履歴を更新（最新50件を保持）
            self.event_history.push(event.clone());
            if self.event_history.len() > 50 {
                self.event_history.remove(0);
            }

            // メトリクスが含まれている場合は更新
            if let Some(metrics) = event.metrics {
                self.current_metrics = Some(metrics);
            }
        }
    }

    /// 現在の状態を取得
    pub fn get_state(&self) -> &DashboardState {
        &self.state
    }

    /// 選択中のタブを取得
    pub fn get_selected_tab(&self) -> usize {
        self.selected_tab
    }

    /// カナリアデプロイメントの状態を取得
    pub fn get_deployment_state(&self) -> DeploymentState {
        self.canary_manager.get_deployment_state()
    }

    /// 現在のメトリクスを取得
    pub fn get_current_metrics(&self) -> Option<&MetricsSnapshot> {
        self.current_metrics.as_ref()
    }

    /// イベント履歴を取得
    pub fn get_event_history(&self) -> &[CanaryEvent] {
        &self.event_history
    }

    /// 実行時間を取得
    pub fn get_uptime(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// 保留中のトラフィック分散設定を取得してクリア
    pub fn take_pending_traffic_split(&mut self) -> Option<f32> {
        self.pending_traffic_split.take()
    }

    /// トラフィック分散率を更新（非同期版）
    pub async fn apply_traffic_split(&self, percentage: f32) -> Result<(), McpError> {
        self.canary_manager.update_traffic_split(percentage).await
    }
}

/// ダッシュボードタブの名前を取得
impl DashboardTab {
    pub fn names() -> Vec<&'static str> {
        vec!["Overview", "Metrics", "Events", "Control"]
    }
}
