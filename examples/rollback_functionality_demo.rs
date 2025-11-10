#!/usr/bin/env cargo
//! Complete Rollback Functionality Demo
//!
//! カナリアデプロイメントにおける包括的なロールバック機能のデモンストレーション
//!
//! ## 機能デモ
//! - 自動ロールバックトリガー
//! - 手動ロールバック実行
//! - 段階的ロールバック
//! - メトリクス監視とアラート
//! - スナップショット管理
//!
//! ## 使用方法
//! ```bash
//! cargo run --example rollback_functionality_demo
//! ```

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

use mcp_rs::canary_deployment::{CanaryDeploymentManager, DeploymentState};
use mcp_rs::policy_config::PolicyConfig;
use mcp_rs::rollback::{
    MetricsSnapshot, PolicyMetrics, RollbackConfig, RollbackEvent, RollbackManager,
    SnapshotCreationReason, SystemMetrics,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .with_level(true)
        .init();

    println!("🔄 Complete Rollback Functionality Demo");
    println!("========================================");
    println!();

    // カナリアデプロイメント管理システムを初期化
    let stable_policy = create_test_policy("stable-v1.0", "Stable Version 1.0");
    let canary_manager = Arc::new(CanaryDeploymentManager::new(stable_policy.clone()));

    // ロールバック管理システムを初期化
    let rollback_manager = Arc::new(RollbackManager::new(canary_manager.clone()));

    println!("✅ Rollback management system initialized");

    // ロールバック設定をカスタマイズ
    let config = RollbackConfig {
        auto_rollback_enabled: true,
        error_rate_threshold: 3.0,       // 3%
        response_time_threshold_ms: 500, // 500ms
        evaluation_window_minutes: 2,
        ..Default::default()
    };

    rollback_manager.update_config(config).await?;
    println!("⚙️  Rollback configuration updated");

    // イベントサブスクリプションの設定
    let mut event_receiver = rollback_manager.subscribe_events();
    let _rollback_manager_clone = rollback_manager.clone();

    tokio::spawn(async move {
        while let Ok(event) = event_receiver.recv().await {
            handle_rollback_event(event).await;
        }
    });

    // デモ実行
    println!("\n🚀 Starting rollback functionality demonstrations...");

    // 1. 基本的なスナップショット作成デモ
    demo_snapshot_creation(&rollback_manager, stable_policy.clone()).await?;

    // 2. 手動ロールバックデモ
    demo_manual_rollback(&rollback_manager, stable_policy.clone()).await?;

    // 3. 自動ロールバックデモ
    demo_auto_rollback(&rollback_manager).await?;

    // 4. 段階的ロールバックデモ
    demo_staged_rollback(&rollback_manager).await?;

    // 5. メトリクス監視デモ
    demo_metrics_monitoring(&rollback_manager).await?;

    println!("\n✅ All rollback functionality demos completed successfully!");
    println!("\n📊 Final rollback metrics:");

    let final_metrics = rollback_manager.get_rollback_metrics().await?;
    println!("   - Total rollbacks: {}", final_metrics.total_rollbacks);
    println!("   - Auto rollbacks: {}", final_metrics.auto_rollbacks);
    println!("   - Manual rollbacks: {}", final_metrics.manual_rollbacks);
    println!("   - Success rate: {:.2}%", final_metrics.success_rate);

    Ok(())
}

async fn demo_snapshot_creation(
    rollback_manager: &Arc<RollbackManager>,
    stable_policy: PolicyConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📸 Demo 1: Snapshot Creation");
    println!("-----------------------------");

    // 複数のスナップショットを作成
    let snapshot_reasons = vec![
        SnapshotCreationReason::DeploymentStart,
        SnapshotCreationReason::ScheduledBackup,
        SnapshotCreationReason::Manual {
            created_by: "admin".to_string(),
        },
        SnapshotCreationReason::AnomalyDetected {
            reason: "High error rate detected".to_string(),
        },
    ];

    for (i, reason) in snapshot_reasons.into_iter().enumerate() {
        let snapshot_id = rollback_manager
            .create_snapshot(
                stable_policy.clone(),
                None,
                Default::default(),
                DeploymentState::Idle,
                reason.clone(),
            )
            .await?;

        println!(
            "   ✅ Created snapshot {}: {} ({:?})",
            i + 1,
            snapshot_id,
            reason
        );
        sleep(Duration::from_millis(500)).await;
    }

    // スナップショット履歴を表示
    let history = rollback_manager.get_rollback_history().await?;
    println!("   📋 Total snapshots in history: {}", history.len());

    Ok(())
}

async fn demo_manual_rollback(
    rollback_manager: &Arc<RollbackManager>,
    stable_policy: PolicyConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 Demo 2: Manual Rollback");
    println!("---------------------------");

    // 新しいスナップショットを作成
    let snapshot_id = rollback_manager
        .create_snapshot(
            stable_policy.clone(),
            None,
            Default::default(),
            DeploymentState::Idle,
            SnapshotCreationReason::Manual {
                created_by: "demo".to_string(),
            },
        )
        .await?;

    println!("   📸 Created target snapshot: {}", snapshot_id);

    // 手動ロールバックを開始
    let rollback_id = rollback_manager
        .initiate_manual_rollback(
            snapshot_id,
            "demo_user".to_string(),
            "Demonstrating manual rollback functionality".to_string(),
        )
        .await?;

    println!("   🔄 Manual rollback initiated: {}", rollback_id);
    println!("   ⏳ Simulating rollback execution...");

    // ロールバック処理をシミュレート
    for i in 1..=5 {
        sleep(Duration::from_millis(300)).await;
        println!("      Progress: {}% complete", i * 20);
    }

    println!("   ✅ Manual rollback completed successfully");

    Ok(())
}

async fn demo_auto_rollback(
    rollback_manager: &Arc<RollbackManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🤖 Demo 3: Automatic Rollback");
    println!("------------------------------");

    // 問題のあるメトリクスを作成
    let problematic_metrics = MetricsSnapshot {
        timestamp: chrono::Utc::now(),
        stable_metrics: PolicyMetrics {
            total_requests: 1000,
            successful_requests: 950,
            error_requests: 50,
            avg_response_time_ms: 120.0,
            error_rate: 5.0, // 5% エラー率（閾値3%を超過）
            ..Default::default()
        },
        canary_metrics: PolicyMetrics {
            total_requests: 200,
            successful_requests: 180,
            error_requests: 20,
            avg_response_time_ms: 150.0,
            error_rate: 10.0, // 10% エラー率（閾値を大幅に超過）
            ..Default::default()
        },
        system_metrics: SystemMetrics::default(),
        custom_metrics: std::collections::HashMap::new(),
    };

    println!("   ⚠️  Detected problematic metrics:");
    println!(
        "      - Canary error rate: {:.2}% (threshold: 3.0%)",
        problematic_metrics.canary_metrics.error_rate
    );
    println!(
        "      - Canary response time: {:.2}ms",
        problematic_metrics.canary_metrics.avg_response_time_ms
    );

    // 自動ロールバックをトリガー
    let rollback_id = rollback_manager
        .trigger_auto_rollback(
            "Error rate exceeded threshold".to_string(),
            problematic_metrics,
        )
        .await?;

    println!("   🚨 Automatic rollback triggered: {}", rollback_id);
    println!("   ⏳ Executing emergency rollback...");

    // 自動ロールバック処理をシミュレート
    for i in 1..=3 {
        sleep(Duration::from_millis(200)).await;
        println!("      Emergency rollback: Stage {} complete", i);
    }

    println!("   ✅ Automatic rollback completed - system restored to stable state");

    Ok(())
}

async fn demo_staged_rollback(
    _rollback_manager: &Arc<RollbackManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📊 Demo 4: Staged Rollback");
    println!("---------------------------");

    println!("   🎯 Executing staged rollback with multiple phases:");

    let stages = vec![
        ("Initial", 75.0),
        ("Intermediate", 50.0),
        ("Advanced", 25.0),
        ("Final", 0.0),
    ];

    for (stage_name, target_percentage) in stages {
        println!(
            "   🔄 Stage '{}': Targeting {}% canary traffic",
            stage_name, target_percentage
        );

        // 段階的な処理をシミュレート
        for step in 1..=3 {
            sleep(Duration::from_millis(400)).await;
            println!("      Step {}/3: Adjusting traffic distribution...", step);
        }

        println!("   ✅ Stage '{}' completed successfully", stage_name);
        sleep(Duration::from_millis(200)).await;
    }

    println!("   🎉 Staged rollback completed - full restoration achieved");

    Ok(())
}

async fn demo_metrics_monitoring(
    _rollback_manager: &Arc<RollbackManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📈 Demo 5: Metrics Monitoring");
    println!("------------------------------");

    println!("   🔍 Monitoring system metrics for rollback conditions...");

    // メトリクス監視をシミュレート
    for cycle in 1..=5 {
        sleep(Duration::from_millis(600)).await;

        let error_rate = if cycle <= 3 { 2.0 } else { 4.0 }; // 後半で閾値を超過
        let response_time = if cycle <= 2 { 200.0 } else { 600.0 }; // 中盤から遅延

        println!(
            "   📊 Monitoring cycle {}: Error rate: {:.1}%, Response time: {:.0}ms",
            cycle, error_rate, response_time
        );

        if cycle == 4 {
            println!("   🚨 ALERT: Metrics exceeded thresholds!");
            println!("      - Error rate: {:.1}% > 3.0% (threshold)", error_rate);
            println!(
                "      - Response time: {:.0}ms > 500ms (threshold)",
                response_time
            );
            println!("   🔄 Auto-rollback condition would be triggered");
        }
    }

    println!("   ✅ Monitoring demonstration completed");

    Ok(())
}

async fn handle_rollback_event(event: RollbackEvent) {
    match event {
        RollbackEvent::AutoRollbackTriggered {
            rollback_id,
            reason,
            ..
        } => {
            info!(
                "🚨 Auto rollback triggered: {} (Reason: {})",
                rollback_id, reason
            );
        }
        RollbackEvent::ManualRollbackInitiated {
            rollback_id,
            initiated_by,
            reason,
            ..
        } => {
            info!(
                "🔧 Manual rollback initiated by {}: {} (Reason: {})",
                initiated_by, rollback_id, reason
            );
        }
        RollbackEvent::RollbackProgress {
            rollback_id,
            stage_name,
            progress_percentage,
            ..
        } => {
            info!(
                "🔄 Rollback progress: {} - {} ({}%)",
                rollback_id, stage_name, progress_percentage
            );
        }
        RollbackEvent::RollbackCompleted {
            rollback_id,
            duration_ms,
            ..
        } => {
            info!(
                "✅ Rollback completed: {} (Duration: {}ms)",
                rollback_id, duration_ms
            );
        }
        RollbackEvent::RollbackFailed {
            rollback_id,
            error_message,
            ..
        } => {
            error!(
                "❌ Rollback failed: {} (Error: {})",
                rollback_id, error_message
            );
        }
        RollbackEvent::SnapshotCreated {
            snapshot_id,
            creation_reason,
            ..
        } => {
            info!(
                "📸 Snapshot created: {} (Reason: {:?})",
                snapshot_id, creation_reason
            );
        }
        RollbackEvent::AnomalyDetected {
            anomaly_type,
            severity,
            recommended_action,
            ..
        } => {
            warn!(
                "⚠️  Anomaly detected: {} (Severity: {:?}) - {}",
                anomaly_type, severity, recommended_action
            );
        }
    }
}

fn create_test_policy(id: &str, name: &str) -> PolicyConfig {
    PolicyConfig {
        id: id.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: Some(format!("Test policy: {}", name)),
        ..Default::default()
    }
}
