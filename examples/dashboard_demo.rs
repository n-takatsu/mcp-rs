use chrono::Utc;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

use mcp_rs::{
    canary_deployment::{CanaryDeploymentManager, RequestContext},
    dashboard::run_dashboard,
    error::McpError,
    policy_config::{
        AuthenticationPolicyConfig, MonitoringPolicyConfig, PolicyConfig, SecurityPolicyConfig,
    },
};

/// ダッシュボード統合デモプログラム
///
/// このプログラムは以下の機能を実証します：
/// 1. カナリアデプロイメントシステムの開始
/// 2. リアルタイム監視ダッシュボード
/// 3. 模擬トラフィック生成
/// 4. インタラクティブなコントロール
#[tokio::main]
async fn main() -> Result<(), McpError> {
    // ログ設定を初期化（DEBUGレベルを削減）
    tracing_subscriber::fmt()
        .with_env_filter("info,mcp_rs::canary_deployment=warn")
        .init();

    info!("🚀 Starting Canary Deployment Dashboard Demo");

    // 安定版ポリシーを作成
    let stable_policy = PolicyConfig {
        id: "stable-policy-v1.2".to_string(),
        name: "stable-policy-v1.2".to_string(),
        version: "1.2.0".to_string(),
        description: Some("Stable production policy".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        security: SecurityPolicyConfig::default(),
        monitoring: MonitoringPolicyConfig::default(),
        authentication: AuthenticationPolicyConfig::default(),
        custom: std::collections::HashMap::new(),
    };

    // カナリア版ポリシーを作成
    let canary_policy = PolicyConfig {
        id: "canary-policy-v2.0".to_string(),
        name: "canary-policy-v2.0".to_string(),
        version: "2.0.0".to_string(),
        description: Some("Canary deployment policy with enhanced features".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        security: SecurityPolicyConfig::default(),
        monitoring: MonitoringPolicyConfig::default(),
        authentication: AuthenticationPolicyConfig::default(),
        custom: {
            let mut custom = std::collections::HashMap::new();
            custom.insert(
                "deployment_type".to_string(),
                Value::String("canary".to_string()),
            );
            custom.insert(
                "rollout_strategy".to_string(),
                Value::String("gradual".to_string()),
            );
            custom.insert(
                "canary_version".to_string(),
                Value::String("2.0".to_string()),
            );
            custom
        },
    };

    // カナリアデプロイメント管理システムを初期化
    info!("🔧 Initializing Canary Deployment Manager");
    let canary_manager = Arc::new(CanaryDeploymentManager::new(stable_policy));

    // カナリアデプロイメントを開始
    info!("🐤 Starting canary deployment with 10% traffic");
    canary_manager
        .start_canary_deployment(canary_policy, 10.0)
        .await?;

    // ダッシュボードを起動（先にダッシュボードを開始）
    info!("🖥️  Launching real-time monitoring dashboard");
    info!("💡 Dashboard Controls:");
    info!("   - Tab: Switch between tabs");
    info!("   - h: Show help");
    info!("   - c: Configuration mode");
    info!("   - r: Refresh manually");
    info!("   - q: Quit");

    // バックグラウンドでトラフィック生成（ダッシュボード起動後）
    let traffic_manager = canary_manager.clone();
    let traffic_handle = tokio::spawn(async move {
        // ダッシュボードが起動するまで少し待機
        sleep(Duration::from_secs(3)).await;
        generate_traffic(traffic_manager).await
    });

    let dashboard_result = run_dashboard(canary_manager.clone()).await;

    // トラフィック生成を停止
    traffic_handle.abort();

    match dashboard_result {
        Ok(_) => info!("✅ Dashboard closed successfully"),
        Err(e) => error!("❌ Dashboard error: {}", e),
    }

    info!("🏁 Demo completed");
    Ok(())
}

/// 模擬トラフィックを生成
async fn generate_traffic(canary_manager: Arc<CanaryDeploymentManager>) {
    let mut request_id = 0u64;
    let user_ids = ["user1", "user2", "user3", "user4", "user5"];
    let ip_addresses = [
        "192.168.1.1",
        "192.168.1.2",
        "192.168.1.3",
        "192.168.1.4",
        "10.0.0.1",
    ];

    loop {
        // 1秒間に1-3リクエストを生成（大幅に削減）
        let requests_per_cycle = 1 + (request_id % 3);

        for _ in 0..requests_per_cycle {
            request_id += 1;

            // リクエストコンテキストを作成
            let context = RequestContext {
                request_id: format!("req_{}", request_id),
                user_id: user_ids[(request_id as usize) % user_ids.len()].to_string(),
                ip_address: ip_addresses[(request_id as usize) % ip_addresses.len()].to_string(),
                user_agent: Some("DashboardDemo/1.0".to_string()),
                custom_headers: std::collections::HashMap::new(),
            };

            // トラフィック分散の決定
            let use_canary = canary_manager.should_use_canary(&context);

            // 模擬レスポンス時間（35-60ms）
            let response_time = 35 + (request_id % 25);

            // 模擬成功率（カナリアは99%、安定版は99.5%）
            let success = if use_canary {
                (request_id % 100) < 99 // 99% success rate for canary
            } else {
                (request_id % 200) < 199 // 99.5% success rate for stable
            };

            // メトリクスを記録
            canary_manager.record_request_metrics(use_canary, success, response_time);

            // リクエスト間の待機（負荷軽減）
            sleep(Duration::from_millis(100)).await;
        }

        // 3秒待機（更に間隔を延長）
        sleep(Duration::from_secs(3)).await;
    }
}
