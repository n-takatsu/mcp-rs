use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        BarChart, Block, Borders, Clear, Gauge, LineGauge, List, ListItem, Paragraph, Sparkline,
        Tabs, Wrap,
    },
    Frame,
};
use std::time::Duration;

use super::app::{DashboardApp, DashboardState, DashboardTab};
use crate::canary_deployment::{CanaryEvent, DeploymentState, MetricsSnapshot};

/// ダッシュボードを描画
pub fn render_dashboard(f: &mut Frame, app: &DashboardApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // タブバー
            Constraint::Min(0),    // メインコンテンツ
            Constraint::Length(1), // ステータスバー
        ])
        .split(f.size());

    // タブバーを描画
    render_tabs(f, chunks[0], app);

    // メインコンテンツを描画
    match app.get_selected_tab() {
        0 => render_overview_tab(f, chunks[1], app),
        1 => render_metrics_tab(f, chunks[1], app),
        2 => render_events_tab(f, chunks[1], app),
        3 => render_control_tab(f, chunks[1], app),
        _ => render_overview_tab(f, chunks[1], app),
    }

    // ステータスバーを描画
    render_status_bar(f, chunks[2], app);

    // モーダルダイアログを描画（必要に応じて）
    match app.get_state() {
        DashboardState::Help => render_help_modal(f, app),
        DashboardState::ConfirmExit => render_exit_confirmation_modal(f, app),
        DashboardState::Configuration => render_configuration_modal(f, app),
        _ => {}
    }
}

/// タブバーを描画
fn render_tabs(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let tab_titles: Vec<Line> = DashboardTab::names()
        .iter()
        .map(|t| Line::from(*t))
        .collect();

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("🦅 Canary Deployment Dashboard"),
        )
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .select(app.get_selected_tab());

    f.render_widget(tabs, area);
}

/// 概要タブを描画
fn render_overview_tab(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // デプロイメント状態
            Constraint::Length(5), // トラフィック分散
            Constraint::Min(5),    // リアルタイムメトリクス
        ])
        .split(area);

    // デプロイメント状態を描画
    render_deployment_status(f, chunks[0], app);

    // トラフィック分散状況を描画
    render_traffic_split_status(f, chunks[1], app);

    // リアルタイムメトリクスを描画
    render_realtime_metrics(f, chunks[2], app);
}

/// デプロイメント状態を描画
fn render_deployment_status(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let deployment_state = app.get_deployment_state();
    let (status_text, status_color) = match deployment_state {
        DeploymentState::Idle => ("🔵 Idle - No active deployment", Color::Blue),
        DeploymentState::Validation => {
            ("🟡 Validation - Initial checks in progress", Color::Yellow)
        }
        DeploymentState::CanaryActive { percentage, .. } => (
            &*format!("🟡 Canary Active - {}% traffic to canary", percentage),
            Color::Yellow,
        ),
        DeploymentState::Scaling { .. } => (
            "🟠 Scaling - Adjusting traffic distribution",
            Color::Magenta,
        ),
        DeploymentState::FullyDeployed => (
            "🟢 Fully Deployed - Canary promoted to stable",
            Color::Green,
        ),
        DeploymentState::RollingBack => {
            ("🔴 Rolling Back - Reverting to stable version", Color::Red)
        }
        DeploymentState::Failed(_) => ("❌ Failed - Deployment failed", Color::Red),
    };

    let uptime = app.get_uptime();
    let uptime_text = format!(
        "⏰ Uptime: {:02}:{:02}:{:02}",
        uptime.as_secs() / 3600,
        (uptime.as_secs() % 3600) / 60,
        uptime.as_secs() % 60
    );

    let text = vec![
        Line::from(vec![
            Span::styled("🦅 Deployment Status: ", Style::default().fg(Color::White)),
            Span::styled(
                status_text,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(uptime_text, Style::default().fg(Color::Cyan))),
        Line::from(Span::styled(
            "📊 Press 'r' to refresh manually",
            Style::default().fg(Color::Gray),
        )),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Deployment Overview"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// トラフィック分散状況を描画
fn render_traffic_split_status(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let metrics = app.get_current_metrics();
    let (canary_percentage, stable_percentage) = if let Some(metrics) = metrics {
        (
            metrics.traffic_split_percentage as f64,
            (100.0 - metrics.traffic_split_percentage) as f64,
        )
    } else {
        (0.0, 100.0)
    };

    let stable_gauge = LineGauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("🔵 Stable Traffic"),
        )
        .filled_style(Style::default().fg(Color::Blue))
        .ratio(stable_percentage / 100.0);

    let canary_gauge = LineGauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("🟡 Canary Traffic"),
        )
        .filled_style(Style::default().fg(Color::Yellow))
        .ratio(canary_percentage / 100.0);

    let traffic_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    f.render_widget(stable_gauge, traffic_chunks[0]);
    f.render_widget(canary_gauge, traffic_chunks[1]);
}

/// リアルタイムメトリクスを描画
fn render_realtime_metrics(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let metrics = app.get_current_metrics();

    let text = if let Some(metrics) = metrics {
        vec![
            Line::from(vec![Span::styled(
                "🎯 Success Rates:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("  🔵 Stable: ", Style::default().fg(Color::Blue)),
                Span::styled(
                    format!("{:.2}%", metrics.stable_success_rate),
                    Style::default().fg(if metrics.stable_success_rate > 95.0 {
                        Color::Green
                    } else {
                        Color::Yellow
                    }),
                ),
                Span::styled("  🟡 Canary: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{:.2}%", metrics.canary_success_rate),
                    Style::default().fg(if metrics.canary_success_rate > 95.0 {
                        Color::Green
                    } else {
                        Color::Yellow
                    }),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "⚡ Response Times:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled("  🔵 Stable: ", Style::default().fg(Color::Blue)),
                Span::styled(
                    format!("{:.1}ms", metrics.stable_avg_response_time),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled("  🟡 Canary: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{:.1}ms", metrics.canary_avg_response_time),
                    Style::default().fg(Color::Cyan),
                ),
            ]),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                "📊 No metrics available yet",
                Style::default().fg(Color::Gray),
            )),
            Line::from(Span::styled(
                "🔄 Waiting for deployment data...",
                Style::default().fg(Color::Gray),
            )),
        ]
    };

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Real-time Metrics"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// メトリクスタブを描画
fn render_metrics_tab(f: &mut Frame, area: Rect, _app: &DashboardApp) {
    let text = vec![
        Line::from("📈 Detailed Metrics View"),
        Line::from(""),
        Line::from("⚠️  Under Development"),
        Line::from("📊 Advanced charts and graphs will be available here"),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Detailed Metrics"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// イベントタブを描画
fn render_events_tab(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let events = app.get_event_history();
    let items: Vec<ListItem> = events
        .iter()
        .rev() // 最新のイベントを上に表示
        .take(20) // 最新20件のみ表示
        .map(|event| {
            let timestamp = format!(
                "{:02}:{:02}:{:02}",
                event.timestamp.elapsed().as_secs() / 3600,
                (event.timestamp.elapsed().as_secs() % 3600) / 60,
                event.timestamp.elapsed().as_secs() % 60
            );
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("[{}] ", timestamp),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(&event.message, Style::default().fg(Color::White)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Event Log (Latest 20)"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(list, area);
}

/// コントロールタブを描画
fn render_control_tab(f: &mut Frame, area: Rect, _app: &DashboardApp) {
    let text = vec![
        Line::from("🎮 Deployment Control Panel"),
        Line::from(""),
        Line::from("Press 'c' to enter configuration mode for:"),
        Line::from("  • Traffic split adjustment"),
        Line::from("  • Manual rollback"),
        Line::from("  • Emergency controls"),
        Line::from(""),
        Line::from("⚠️  Control features under development"),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Control Panel"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// ステータスバーを描画
fn render_status_bar(f: &mut Frame, area: Rect, app: &DashboardApp) {
    let help_text = match app.get_state() {
        DashboardState::Monitoring => {
            "Tab: Switch tabs | h: Help | c: Config | q: Quit | r: Refresh"
        }
        DashboardState::Configuration => {
            "Esc: Back | 1-5: Set traffic split (10%, 25%, 50%, 75%, 100%)"
        }
        DashboardState::Help => "Esc/h: Close help",
        DashboardState::ConfirmExit => "y: Confirm exit | n/Esc: Cancel",
    };

    let paragraph =
        Paragraph::new(help_text).style(Style::default().fg(Color::White).bg(Color::DarkGray));

    f.render_widget(paragraph, area);
}

/// ヘルプモーダルを描画
fn render_help_modal(f: &mut Frame, _app: &DashboardApp) {
    let area = centered_rect(60, 70, f.size());
    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from("🦅 Canary Deployment Dashboard Help"),
        Line::from(""),
        Line::from("📋 Navigation:"),
        Line::from("  Tab / Shift+Tab  - Switch between tabs"),
        Line::from("  h               - Show/hide this help"),
        Line::from("  q               - Quit application"),
        Line::from("  r               - Refresh data manually"),
        Line::from(""),
        Line::from("🎮 Control:"),
        Line::from("  c               - Enter configuration mode"),
        Line::from(""),
        Line::from("📊 Tabs:"),
        Line::from("  Overview        - Deployment status & metrics"),
        Line::from("  Metrics         - Detailed performance data"),
        Line::from("  Events          - Real-time event log"),
        Line::from("  Control         - Manual deployment controls"),
        Line::from(""),
        Line::from("Press h or Esc to close this help"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// 終了確認モーダルを描画
fn render_exit_confirmation_modal(f: &mut Frame, _app: &DashboardApp) {
    let area = centered_rect(30, 20, f.size());
    f.render_widget(Clear, area);

    let text = vec![
        Line::from(""),
        Line::from("Are you sure you want to exit?"),
        Line::from(""),
        Line::from("y: Yes, exit"),
        Line::from("n: No, stay"),
    ];

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Confirm Exit"))
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

/// 設定モーダルを描画
fn render_configuration_modal(f: &mut Frame, _app: &DashboardApp) {
    let area = centered_rect(50, 30, f.size());
    f.render_widget(Clear, area);

    let text = vec![
        Line::from("⚙️  Configuration Mode"),
        Line::from(""),
        Line::from("🎯 Traffic Split Settings:"),
        Line::from("  1 - Set to 10%"),
        Line::from("  2 - Set to 25%"),
        Line::from("  3 - Set to 50%"),
        Line::from("  4 - Set to 75%"),
        Line::from("  5 - Set to 100%"),
        Line::from(""),
        Line::from("Esc - Return to monitoring"),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Configuration"),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// 中央に配置された矩形を作成
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
