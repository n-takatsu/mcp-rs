use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};
use uuid::Uuid;

use mcp_rs::canary_deployment::{CanaryDeploymentManager, CanaryEventType, RequestContext};
use mcp_rs::policy_config::PolicyConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定
    tracing_subscriber::fmt().with_env_filter("debug").init();

    println!("🐤 MCP-RS Canary Deployment System Demo");
    println!("========================================");

    // 初期ポリシーを作成
    let stable_policy = create_stable_policy();
    let canary_policy = create_canary_policy();

    // カナリアデプロイメント管理システムを初期化
    let manager = CanaryDeploymentManager::new(stable_policy);

    // イベント監視を開始
    let mut event_receiver = manager.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            match event.event_type {
                CanaryEventType::CanaryStarted { percentage } => {
                    println!(
                        "🐣 カナリア開始！ {}%のトラフィックで飛び立ちました",
                        percentage
                    );
                }
                CanaryEventType::TrafficSplitChanged {
                    old_percentage,
                    new_percentage,
                } => {
                    println!(
                        "🔄 カナリアが成長中: {:.1}% → {:.1}%",
                        old_percentage, new_percentage
                    );
                }
                CanaryEventType::MetricsUpdated => {
                    if let Some(metrics) = &event.metrics {
                        println!("📊 メトリクス更新:");
                        println!(
                            "   安定版: 成功率 {:.1}%, 平均応答時間 {:.1}ms",
                            metrics.stable_success_rate, metrics.stable_avg_response_time
                        );
                        println!(
                            "   カナリア版: 成功率 {:.1}%, 平均応答時間 {:.1}ms",
                            metrics.canary_success_rate, metrics.canary_avg_response_time
                        );
                    }
                }
                CanaryEventType::WarningDetected { warning_type } => {
                    println!("⚠️ 警告: カナリアが不安定になっています - {}", warning_type);
                }
                CanaryEventType::RollbackInitiated { reason } => {
                    println!("🚨 緊急事態！ロールバック開始: {}", reason);
                }
                _ => {
                    println!("📝 イベント: {}", event.message);
                }
            }
        }
    });

    // デモシナリオを実行
    demo_canary_deployment(&manager, canary_policy).await?;

    Ok(())
}

/// カナリアデプロイメントのデモシナリオ
async fn demo_canary_deployment(
    manager: &CanaryDeploymentManager,
    canary_policy: PolicyConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\\n🎭 === Phase 1: カナリアデプロイメント開始 ===");

    // 5%から開始
    manager.start_canary_deployment(canary_policy, 5.0).await?;
    sleep(Duration::from_secs(2)).await;

    // トラフィックシミュレーション
    simulate_traffic(manager, 100, 5.0).await;
    sleep(Duration::from_secs(1)).await;

    println!("\\n🎭 === Phase 2: 段階的拡大 25% ===");
    manager.update_traffic_split(25.0).await?;
    simulate_traffic(manager, 200, 25.0).await;
    sleep(Duration::from_secs(1)).await;

    println!("\\n🎭 === Phase 3: 段階的拡大 50% ===");
    manager.update_traffic_split(50.0).await?;
    simulate_traffic(manager, 300, 50.0).await;
    sleep(Duration::from_secs(1)).await;

    println!("\\n🎭 === Phase 4: ほぼ完全展開 90% ===");
    manager.update_traffic_split(90.0).await?;
    simulate_traffic(manager, 400, 90.0).await;
    sleep(Duration::from_secs(1)).await;

    println!("\\n🎉 === カナリアデプロイメント成功！ ===");
    println!("🦅 カナリアが空を舞い、新しいポリシーが安全に展開されました！");

    // 最終メトリクスを表示
    display_final_metrics(manager);

    Ok(())
}

/// トラフィックをシミュレート
async fn simulate_traffic(
    manager: &CanaryDeploymentManager,
    request_count: u32,
    expected_canary_rate: f32,
) {
    println!(
        "🚦 {}リクエストのトラフィックをシミュレーション中...",
        request_count
    );

    let mut canary_count = 0;
    let mut stable_count = 0;

    for i in 0..request_count {
        let context = RequestContext {
            request_id: Uuid::new_v4().to_string(),
            user_id: format!("user_{}", i % 1000), // 1000ユーザーをシミュレート
            ip_address: format!("192.168.1.{}", i % 254 + 1),
            user_agent: Some("MCP-Demo-Client/1.0".to_string()),
            custom_headers: HashMap::new(),
        };

        let use_canary = manager.should_use_canary(&context);

        if use_canary {
            canary_count += 1;
        } else {
            stable_count += 1;
        }

        // レスポンス時間とエラーをシミュレート
        let (success, response_time) = simulate_request_outcome(use_canary);
        manager.record_request_metrics(use_canary, success, response_time);

        // 短い間隔でリクエスト
        if i % 50 == 0 {
            tokio::task::yield_now().await;
        }
    }

    let actual_canary_rate = canary_count as f32 / request_count as f32 * 100.0;
    println!(
        "📈 結果: 安定版 {} リクエスト, カナリア版 {} リクエスト (実際のカナリア率: {:.1}%)",
        stable_count, canary_count, actual_canary_rate
    );

    let rate_diff = (actual_canary_rate - expected_canary_rate).abs();
    if rate_diff > 5.0 {
        warn!(
            "⚠️ カナリア率が期待値から大きく外れています: 期待 {:.1}%, 実際 {:.1}%",
            expected_canary_rate, actual_canary_rate
        );
    } else {
        info!("✅ カナリア率が期待範囲内です");
    }
}

/// リクエストの成功/失敗とレスポンス時間をシミュレート
fn simulate_request_outcome(is_canary: bool) -> (bool, u64) {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    // カナリア版は若干パフォーマンスが良い設定
    let (success_rate, base_response_time) = if is_canary {
        (0.995, 45.0) // 99.5%成功率、平均45ms
    } else {
        (0.990, 50.0) // 99.0%成功率、平均50ms
    };

    let success = rng.gen::<f64>() < success_rate;

    // レスポンス時間は正規分布風にシミュレート
    let response_time = if success {
        let variation = rng.gen::<f64>() * 20.0 - 10.0; // ±10msの変動
        (base_response_time + variation).max(1.0) as u64
    } else {
        // エラー時は応答時間が長い傾向
        (base_response_time * 2.0 + rng.gen::<f64>() * 100.0) as u64
    };

    (success, response_time)
}

/// 最終メトリクスを表示
fn display_final_metrics(manager: &CanaryDeploymentManager) {
    println!("\\n📊 最終メトリクス レポート");
    println!("========================");

    let state = manager.get_deployment_state();
    println!("デプロイメント状態: {:?}", state);

    // メトリクス詳細は実装が必要（今回は簡易版）
    println!("✅ カナリアデプロイメントが正常に完了しました！");
}

/// 安定版ポリシーを作成
fn create_stable_policy() -> PolicyConfig {
    let mut policy = PolicyConfig {
        id: "stable-policy-v1.0".to_string(),
        name: "Stable Production Policy".to_string(),
        version: "1.0.0".to_string(),
        description: Some("現在の安定稼働ポリシー".to_string()),
        ..Default::default()
    };

    // 保守的な設定
    policy.security.rate_limiting.requests_per_minute = 100;
    policy.security.rate_limiting.burst_size = 20;
    policy.monitoring.interval_seconds = 60;
    policy.authentication.require_mfa = true;

    policy
}

/// カナリア版ポリシーを作成
fn create_canary_policy() -> PolicyConfig {
    let mut policy = PolicyConfig {
        id: "canary-policy-v2.0".to_string(),
        name: "Canary New Policy".to_string(),
        version: "2.0.0".to_string(),
        description: Some("新機能を含むカナリア版ポリシー".to_string()),
        ..Default::default()
    };

    // より積極的な設定
    policy.security.rate_limiting.requests_per_minute = 150; // 増加
    policy.security.rate_limiting.burst_size = 30; // 増加
    policy.monitoring.interval_seconds = 30; // より頻繁
    policy.authentication.require_mfa = true;

    // 新機能の追加（カスタムフィールド）
    policy.custom.insert(
        "new_feature_flag".to_string(),
        serde_json::Value::Bool(true),
    );
    policy.custom.insert(
        "performance_mode".to_string(),
        serde_json::Value::String("optimized".to_string()),
    );

    policy
}

/// 簡易的なrand機能（依存関係を避けるため）
mod rand {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn thread_rng() -> ThreadRng {
        ThreadRng::new()
    }

    pub struct ThreadRng {
        state: u64,
    }

    impl ThreadRng {
        fn new() -> Self {
            let mut hasher = DefaultHasher::new();
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .hash(&mut hasher);
            Self {
                state: hasher.finish(),
            }
        }

        fn next(&mut self) -> u64 {
            // Linear congruential generator
            self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
            self.state
        }
    }

    pub trait Rng {
        fn gen<T>(&mut self) -> T
        where
            T: SampleUniform;
    }

    impl Rng for ThreadRng {
        fn gen<T>(&mut self) -> T
        where
            T: SampleUniform,
        {
            T::sample_uniform(self.next())
        }
    }

    pub trait SampleUniform {
        fn sample_uniform(value: u64) -> Self;
    }

    impl SampleUniform for f64 {
        fn sample_uniform(value: u64) -> Self {
            (value as f64) / (u64::MAX as f64)
        }
    }
}
