use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use tempfile::TempDir;
use tokio::runtime::Runtime;

use mcp_rs::policy_application::PolicyApplicationEngine;
use mcp_rs::policy_config::{PolicyConfig, PolicyLoader};
use mcp_rs::policy_validation::{PolicyValidationEngine, ValidationLevel};

/// Policy Hot-Reload システムのパフォーマンスベンチマーク
///
/// 実行方法:
/// ```bash
/// cargo bench --bench policy_hot_reload_bench
/// ```
fn create_test_policy(id: &str, version: &str) -> PolicyConfig {
    let mut policy = PolicyConfig {
        id: id.to_string(),
        name: format!("Benchmark Policy {}", id),
        version: version.to_string(),
        description: Some(format!("ベンチマーク用ポリシー: {}", id)),
        ..Default::default()
    };

    // 現実的な設定値
    policy.security.rate_limiting.requests_per_minute = 100;
    policy.security.rate_limiting.burst_size = 20;
    policy.security.encryption.pbkdf2_iterations = 100_000;
    policy.monitoring.interval_seconds = 60;
    policy.authentication.session_timeout_seconds = 3600;

    policy
}

/// ポリシー検証のベンチマーク
fn bench_policy_validation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("policy_validation");

    // 検証レベル別のベンチマーク
    let validation_levels = vec![
        (ValidationLevel::Basic, "Basic"),
        (ValidationLevel::Standard, "Standard"),
        (ValidationLevel::Strict, "Strict"),
        (ValidationLevel::Custom, "Custom"),
    ];

    for (level, level_name) in validation_levels {
        group.bench_with_input(
            BenchmarkId::new("validation_level", level_name),
            &level,
            |b, level| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut validation_engine = PolicyValidationEngine::new();
                        let policy = create_test_policy("bench-policy", "1.0.0");

                        black_box(
                            validation_engine
                                .validate_policy(&policy, level.clone())
                                .await,
                        )
                    })
                });
            },
        );
    }

    group.finish();
}

/// ポリシーファイル読み込みのベンチマーク
fn bench_policy_file_loading(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("policy_file_loading");

    // ファイル形式別のベンチマーク
    let formats = vec![
        ("toml", "test_policy.toml"),
        ("yaml", "test_policy.yaml"),
        ("json", "test_policy.json"),
    ];

    for (format_name, filename) in formats {
        group.bench_with_input(
            BenchmarkId::new("file_format", format_name),
            filename,
            |b, filename| {
                b.iter(|| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let policy_file = temp_dir.path().join(filename);

                        // テストポリシーをファイルに保存
                        let policy = create_test_policy("load-bench", "1.0.0");
                        PolicyLoader::save_to_file(&policy, &policy_file)
                            .await
                            .unwrap();

                        // ファイルから読み込み
                        black_box(PolicyLoader::load_from_file(&policy_file).await.unwrap())
                    })
                })
            },
        );
    }

    group.finish();
}

/// ポリシー適用エンジンの初期化ベンチマーク
fn bench_engine_initialization(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("engine_initialization");

    // 監視ファイル数別のベンチマーク
    let file_counts = vec![1, 5, 10, 20];

    for file_count in file_counts {
        group.bench_with_input(
            BenchmarkId::new("file_count", file_count),
            &file_count,
            |b, &file_count| {
                b.iter(|| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();

                        // 複数のポリシーファイルを作成
                        let mut policy_files = Vec::new();
                        for i in 1..=file_count {
                            let policy_file = temp_dir.path().join(format!("policy_{}.toml", i));
                            let policy = create_test_policy(&format!("bench-{}", i), "1.0.0");
                            PolicyLoader::save_to_file(&policy, &policy_file)
                                .await
                                .unwrap();
                            policy_files.push(policy_file);
                        }

                        // エンジンを初期化
                        let mut engine = PolicyApplicationEngine::new(temp_dir.path());
                        for policy_file in &policy_files {
                            engine.add_policy_file(policy_file);
                        }

                        // 初期化時間を測定
                        engine.start().await.unwrap();
                        black_box(());
                        engine.stop();
                    })
                })
            },
        );
    }

    group.finish();
}

/// ポリシー更新の連続処理ベンチマーク
fn bench_continuous_policy_updates(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("continuous_updates");
    group.throughput(Throughput::Elements(1));

    // 更新回数別のベンチマーク
    let update_counts = vec![10, 50, 100];

    for update_count in update_counts {
        group.bench_with_input(
            BenchmarkId::new("update_count", update_count),
            &update_count,
            |b, &update_count| {
                b.iter(|| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let policy_file = temp_dir.path().join("continuous_test.toml");

                        // 初期ポリシーを作成
                        let initial_policy = create_test_policy("continuous-bench", "1.0.0");
                        PolicyLoader::save_to_file(&initial_policy, &policy_file)
                            .await
                            .unwrap();

                        // エンジンを開始
                        let mut engine = PolicyApplicationEngine::with_validation_level(
                            temp_dir.path(),
                            ValidationLevel::Basic, // 高速化のため基本検証のみ
                        );
                        engine.add_policy_file(&policy_file);
                        engine.start().await.unwrap();

                        // 連続してポリシーを更新
                        for i in 1..=update_count {
                            let policy = create_test_policy(
                                &format!("continuous-bench-{}", i),
                                &format!("1.0.{}", i),
                            );
                            PolicyLoader::save_to_file(&policy, &policy_file)
                                .await
                                .unwrap();

                            // 短い待機時間で次の更新へ
                            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                        }

                        engine.stop();
                        black_box(update_count);
                    })
                })
            },
        );
    }

    group.finish();
}

/// メモリ使用量測定用のベンチマーク
fn bench_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_usage");

    // ポリシーサイズ別のベンチマーク
    let sizes = vec![
        (100, "small"),   // 小さなポリシー
        (1000, "medium"), // 中程度のポリシー
        (5000, "large"),  // 大きなポリシー
    ];

    for (custom_fields, size_name) in sizes {
        group.bench_with_input(
            BenchmarkId::new("policy_size", size_name),
            &custom_fields,
            |b, &custom_fields| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut policy = create_test_policy("memory-bench", "1.0.0");

                        // カスタムフィールドを追加してポリシーサイズを増加
                        for i in 1..=custom_fields {
                            policy.custom.insert(
                                format!("custom_field_{}", i),
                                serde_json::Value::String(format!("value_{}", i)),
                            );
                        }

                        let mut validation_engine = PolicyValidationEngine::new();
                        black_box(
                            validation_engine
                                .validate_policy(&policy, ValidationLevel::Standard)
                                .await,
                        )
                    })
                })
            },
        );
    }

    group.finish();
}

/// 同時実行の負荷テスト
fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_operations");

    // 同時実行数別のベンチマーク
    let concurrency_levels = vec![1, 4, 8, 16];

    for concurrency in concurrency_levels {
        group.bench_with_input(
            BenchmarkId::new("concurrency", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::new();

                        for i in 0..concurrency {
                            let handle = tokio::spawn(async move {
                                let mut validation_engine = PolicyValidationEngine::new();
                                let policy =
                                    create_test_policy(&format!("concurrent-{}", i), "1.0.0");

                                validation_engine
                                    .validate_policy(&policy, ValidationLevel::Standard)
                                    .await
                            });
                            handles.push(handle);
                        }

                        // 全てのタスクの完了を待機
                        for handle in handles {
                            black_box(handle.await.unwrap());
                        }
                    })
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_policy_validation,
    bench_policy_file_loading,
    bench_engine_initialization,
    bench_continuous_policy_updates,
    bench_memory_usage,
    bench_concurrent_operations
);

criterion_main!(benches);
