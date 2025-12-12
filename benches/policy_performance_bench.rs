//! ポリシーシステムパフォーマンスベンチマーク（簡易版）
//!
//! Issue #43の成功指標検証:
//! - <5秒 ポリシー適用時間
//! - 並行更新負荷テスト

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mcp_rs::policy::{
    dynamic_updater::{DynamicPolicyUpdater, UpdateConfig},
    hot_reload::{HotReloadManager, ReloadStrategy},
    rollback::RollbackManager,
    version_control::VersionManager,
};
use mcp_rs::policy_config::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::runtime::Runtime;

/// テスト用ポリシー作成
fn create_test_policy(version: &str) -> PolicyConfig {
    PolicyConfig {
        id: "bench-policy".to_string(),
        name: "Benchmark Policy".to_string(),
        version: version.to_string(),
        description: Some("Benchmark policy".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        security: SecurityPolicyConfig {
            enabled: true,
            encryption: EncryptionConfig {
                algorithm: "AES-256".to_string(),
                key_size: 256,
                pbkdf2_iterations: 100000,
            },
            tls: TlsConfig {
                enforce: true,
                min_version: "1.2".to_string(),
                cipher_suites: vec!["TLS_AES_256_GCM_SHA384".to_string()],
            },
            input_validation: InputValidationConfig {
                enabled: true,
                max_input_length: 1024,
                sql_injection_protection: true,
                xss_protection: true,
            },
            rate_limiting: RateLimitingConfig {
                enabled: true,
                requests_per_minute: 6000,
                burst_size: 10,
            },
        },
        monitoring: MonitoringPolicyConfig {
            interval_seconds: 60,
            alerts_enabled: true,
            log_level: "info".to_string(),
            metrics: MetricsConfig {
                enabled: true,
                sampling_rate: 1.0,
                buffer_size: 1000,
            },
        },
        authentication: AuthenticationPolicyConfig {
            enabled: true,
            method: "password".to_string(),
            session_timeout_seconds: 1800,
            require_mfa: false,
        },
        custom: HashMap::new(),
    }
}

/// ポリシー更新のベンチマーク
fn bench_policy_update(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("policy_update_basic", |b| {
        b.iter(|| {
            rt.block_on(async {
                let initial = create_test_policy("1.0.0");
                let updater = DynamicPolicyUpdater::new(initial, UpdateConfig::default());

                let new_policy = create_test_policy("2.0.0");
                updater.update_policy(black_box(new_policy)).await.unwrap();
            })
        });
    });
}

/// ホットリロード（Immediate戦略）のベンチマーク
fn bench_hot_reload_immediate(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("hot_reload_immediate", |b| {
        b.iter(|| {
            rt.block_on(async {
                let initial = create_test_policy("1.0.0");
                let manager = HotReloadManager::new(initial, ReloadStrategy::Immediate);

                let new_policy = create_test_policy("2.0.0");
                manager.reload(black_box(new_policy)).await.unwrap();
            })
        });
    });
}

/// ロールバックのベンチマーク
fn bench_rollback_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("create_rollback_point", |b| {
        b.iter(|| {
            rt.block_on(async {
                let initial = create_test_policy("1.0.0");
                let manager = RollbackManager::new(initial, 10);

                let policy = create_test_policy("2.0.0");
                manager
                    .create_rollback_point(black_box(policy), "checkpoint")
                    .await
                    .unwrap();
            })
        });
    });
}

/// バージョン作成のベンチマーク
fn bench_version_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("create_version", |b| {
        b.iter(|| {
            rt.block_on(async {
                let initial = create_test_policy("1.0.0");
                let manager = VersionManager::new(initial, 10);

                let policy = create_test_policy("2.0.0");
                manager
                    .create_version(black_box(policy), "user".to_string(), "update".to_string())
                    .await
                    .unwrap();
            })
        });
    });
}

/// 並行更新負荷テスト（10並列）
fn bench_concurrent_updates_10(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("concurrent_updates_10", |b| {
        b.iter(|| {
            rt.block_on(async {
                let initial = create_test_policy("1.0.0");
                let updater = Arc::new(DynamicPolicyUpdater::new(initial, UpdateConfig::default()));

                let mut handles = vec![];
                for i in 0..10 {
                    let updater_clone = Arc::clone(&updater);
                    let handle = tokio::spawn(async move {
                        let policy = create_test_policy(&format!("2.{}.0", i));
                        updater_clone.update_policy(policy).await
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    handle.await.unwrap().unwrap();
                }
            })
        });
    });
}

/// エンドツーエンドシナリオベンチマーク（<5秒目標検証）
fn bench_end_to_end_scenario(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("end_to_end_full_workflow", |b| {
        b.iter(|| {
            rt.block_on(async {
                // 完全なワークフロー: 更新 → ロールバック → バージョン確認
                let initial = create_test_policy("1.0.0");

                // 1. 更新
                let updater = DynamicPolicyUpdater::new(initial.clone(), UpdateConfig::default());
                updater.update_policy(create_test_policy("2.0.0")).await.unwrap();

                // 2. ロールバックポイント作成
                let rollback_mgr = RollbackManager::new(create_test_policy("2.0.0"), 10);
                rollback_mgr
                    .create_rollback_point(create_test_policy("3.0.0"), "v3")
                    .await
                    .unwrap();

                // 3. ロールバック
                rollback_mgr.rollback_to_latest().await.unwrap();

                // 4. バージョン管理
                let version_mgr = VersionManager::new(initial, 10);
                version_mgr
                    .create_version(create_test_policy("2.0.0"), "user".to_string(), "v2".to_string())
                    .await
                    .unwrap();

                // 5. diff計算
                let v1_id = version_mgr.get_current_version().await.unwrap().id;
                let v2_id = version_mgr
                    .create_version(create_test_policy("3.0.0"), "user".to_string(), "v3".to_string())
                    .await
                    .unwrap();
                version_mgr.diff(&v1_id, &v2_id).await.unwrap();
            })
        });
    });
}

/// 5秒以内の適用時間を直接測定
fn measure_policy_application_time() {
    let rt = Runtime::new().unwrap();
    
    println!("\n========================================");
    println!("ポリシー適用時間測定（目標: <5秒）");
    println!("========================================\n");

    // 1. 基本更新
    let start = Instant::now();
    rt.block_on(async {
        let initial = create_test_policy("1.0.0");
        let updater = DynamicPolicyUpdater::new(initial, UpdateConfig::default());
        updater.update_policy(create_test_policy("2.0.0")).await.unwrap();
    });
    let elapsed = start.elapsed();
    println!("✓ 基本更新: {:.3}秒", elapsed.as_secs_f64());
    assert!(elapsed.as_secs() < 5, "基本更新が5秒を超えました！");

    // 2. ホットリロード (Immediate)
    let start = Instant::now();
    rt.block_on(async {
        let initial = create_test_policy("1.0.0");
        let manager = HotReloadManager::new(initial, ReloadStrategy::Immediate);
        manager.reload(create_test_policy("2.0.0")).await.unwrap();
    });
    let elapsed = start.elapsed();
    println!("✓ ホットリロード (Immediate): {:.3}秒", elapsed.as_secs_f64());
    assert!(elapsed.as_secs() < 5, "ホットリロードが5秒を超えました！");

    // 3. エンドツーエンド
    let start = Instant::now();
    rt.block_on(async {
        let initial = create_test_policy("1.0.0");
        
        let updater = DynamicPolicyUpdater::new(initial.clone(), UpdateConfig::default());
        updater.update_policy(create_test_policy("2.0.0")).await.unwrap();

        let rollback_mgr = RollbackManager::new(create_test_policy("2.0.0"), 10);
        rollback_mgr.create_rollback_point(create_test_policy("3.0.0"), "v3").await.unwrap();
        rollback_mgr.rollback_to_latest().await.unwrap();

        let version_mgr = VersionManager::new(initial, 10);
        version_mgr.create_version(create_test_policy("2.0.0"), "user".to_string(), "v2".to_string()).await.unwrap();
    });
    let elapsed = start.elapsed();
    println!("✓ エンドツーエンド: {:.3}秒", elapsed.as_secs_f64());
    assert!(elapsed.as_secs() < 5, "エンドツーエンドが5秒を超えました！");

    println!("\n✅ 全てのテストで <5秒 目標を達成！\n");
}

criterion_group!(
    benches,
    bench_policy_update,
    bench_hot_reload_immediate,
    bench_rollback_operations,
    bench_version_creation,
    bench_concurrent_updates_10,
    bench_end_to_end_scenario,
);

criterion_main!(benches);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_application_time() {
        measure_policy_application_time();
    }
}
