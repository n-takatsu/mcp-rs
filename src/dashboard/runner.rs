use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

use super::{
    app::DashboardApp,
    events::{DashboardEvent, EventHandler},
    ui::render_dashboard,
};
use crate::canary_deployment::CanaryDeploymentManager;
use crate::error::McpError;

/// ダッシュボードを実行
pub async fn run_dashboard(canary_manager: Arc<CanaryDeploymentManager>) -> Result<(), McpError> {
    // ターミナルを初期化
    enable_raw_mode()
        .map_err(|e| McpError::CanaryDeployment(format!("Failed to enable raw mode: {}", e)))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| McpError::CanaryDeployment(format!("Failed to setup terminal: {}", e)))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)
        .map_err(|e| McpError::CanaryDeployment(format!("Failed to create terminal: {}", e)))?;

    // ダッシュボードアプリケーションを初期化
    let mut app = DashboardApp::new(canary_manager.clone());

    // イベントハンドラーを初期化
    let canary_receiver = canary_manager.subscribe();
    let mut event_handler = EventHandler::new(canary_receiver);

    // メインループを実行
    let result = run_main_loop(&mut terminal, &mut app, &mut event_handler).await;

    // ターミナルを復元
    disable_raw_mode()
        .map_err(|e| McpError::CanaryDeployment(format!("Failed to disable raw mode: {}", e)))?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(|e| McpError::CanaryDeployment(format!("Failed to restore terminal: {}", e)))?;
    terminal
        .show_cursor()
        .map_err(|e| McpError::CanaryDeployment(format!("Failed to show cursor: {}", e)))?;

    result
}

/// メインループを実行
async fn run_main_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut DashboardApp,
    _event_handler: &mut EventHandler,
) -> Result<(), McpError> {
    use crossterm::event::{Event, KeyCode, KeyEventKind};

    // 定期更新タイマー
    let mut update_interval = interval(Duration::from_millis(500));

    loop {
        // UIを描画
        terminal
            .draw(|f| render_dashboard(f, app))
            .map_err(|e| McpError::CanaryDeployment(format!("Failed to draw UI: {}", e)))?;

        // イベントを処理
        tokio::select! {
            // 定期更新
            _ = update_interval.tick() => {
                app.check_for_events();

                // 保留中のトラフィック分散設定を処理
                if let Some(percentage) = app.take_pending_traffic_split() {
                    if let Err(e) = app.apply_traffic_split(percentage).await {
                        // エラーハンドリング（ログ出力など）
                        eprintln!("Failed to apply traffic split: {}", e);
                    }
                }
            }

            // キーボード入力を直接処理
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if crossterm::event::poll(Duration::from_millis(0))
                    .map_err(|e| McpError::CanaryDeployment(format!("Failed to poll events: {}", e)))?
                {
                    let event = crossterm::event::read()
                        .map_err(|e| McpError::CanaryDeployment(format!("Failed to read event: {}", e)))?;

                    if let Event::Key(key) = event {
                        if key.kind == KeyEventKind::Press && app.handle_event(Event::Key(key))? {
                            break; // 終了要求
                        }
                    }
                }

                app.check_for_events();
            }
        }
    }

    Ok(())
}
