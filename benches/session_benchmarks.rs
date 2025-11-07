use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mcp_rs::session::{
    CreateSessionRequest, MemorySessionStorage, SecurityLevel, SessionManager,
    SessionManagerConfig, SessionState,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// セッション管理システムベンチマーク
///
/// 主要操作のパフォーマンス特性を測定し、
/// システムのスケーラビリティとボトルネックを特定します。

fn bench_session_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("session_creation");

    // 異なるサイズでのスループット測定
    for size in [100, 500, 1000, 2000].iter() {
        group.throughput(Throughput::Elements(*size as u64));

        group.bench_with_input(BenchmarkId::new("sequential", size), size, |b, &size| {
            b.to_async(&rt).iter(|| async {
                let storage = Arc::new(MemorySessionStorage::new());
                let manager = SessionManager::new(storage, SessionManagerConfig::default())
                    .await
                    .unwrap();

                for i in 0..size {
                    let request = CreateSessionRequest {
                        user_id: Some(format!("user_{}", i)),
                        ttl: Some(Duration::from_secs(3600)),
                        ip_address: None,
                        user_agent: None,
                        security_level: Some(SecurityLevel::Low),
                        initial_data: None,
                    };

                    let _session_id = manager.create_session(request).await.unwrap();
                }
            })
        });

        group.bench_with_input(BenchmarkId::new("concurrent", size), size, |b, &size| {
            b.to_async(&rt).iter(|| async {
                let storage = Arc::new(MemorySessionStorage::new());
                let manager = Arc::new(
                    SessionManager::new(storage, SessionManagerConfig::default())
                        .await
                        .unwrap(),
                );

                let mut handles = Vec::new();

                for i in 0..size {
                    let manager_clone = manager.clone();
                    let handle = tokio::spawn(async move {
                        let request = CreateSessionRequest {
                            user_id: Some(format!("user_{}", i)),
                            ttl: Some(Duration::from_secs(3600)),
                            ip_address: None,
                            user_agent: None,
                            security_level: Some(SecurityLevel::Low),
                            initial_data: None,
                        };

                        manager_clone.create_session(request).await
                    });

                    handles.push(handle);
                }

                // すべてのタスク完了を待機
                for handle in handles {
                    let _ = handle.await.unwrap();
                }
            })
        });
    }

    group.finish();
}

fn bench_session_retrieval(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("session_retrieval");

    // セッション数による検索パフォーマンスの変化を測定
    for session_count in [1000, 5000, 10000, 20000].iter() {
        group.throughput(Throughput::Elements(1000)); // 1000回の検索操作

        group.bench_with_input(
            BenchmarkId::new("random_access", session_count),
            session_count,
            |b, &session_count| {
                b.to_async(&rt).iter(|| async {
                    // セットアップ
                    let storage = Arc::new(MemorySessionStorage::new());
                    let manager = SessionManager::new(storage, SessionManagerConfig::default())
                        .await
                        .unwrap();

                    // セッション作成
                    let mut session_ids = Vec::new();
                    for i in 0..session_count {
                        let request = CreateSessionRequest {
                            user_id: Some(format!("user_{}", i)),
                            ttl: Some(Duration::from_secs(3600)),
                            ip_address: None,
                            user_agent: None,
                            security_level: Some(SecurityLevel::Low),
                            initial_data: Some(json!({"index": i})),
                        };

                        let session_id = manager.create_session(request).await.unwrap();
                        session_ids.push(session_id);
                    }

                    // ランダムアクセス測定
                    use rand::Rng;
                    let mut rng = rand::thread_rng();

                    for _ in 0..1000 {
                        let random_index = rng.gen_range(0..session_ids.len());
                        let session_id = &session_ids[random_index];
                        let _session = manager.get_session(session_id).await.unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_session_updates(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("session_updates");

    for update_count in [100, 500, 1000, 2000].iter() {
        group.throughput(Throughput::Elements(*update_count as u64));

        group.bench_with_input(
            BenchmarkId::new("sequential_updates", update_count),
            update_count,
            |b, &update_count| {
                b.to_async(&rt).iter(|| async {
                    let storage = Arc::new(MemorySessionStorage::new());
                    let manager = SessionManager::new(storage, SessionManagerConfig::default())
                        .await
                        .unwrap();

                    // テスト用セッション作成
                    let request = CreateSessionRequest {
                        user_id: Some("update_user".to_string()),
                        ttl: Some(Duration::from_secs(3600)),
                        ip_address: None,
                        user_agent: None,
                        security_level: Some(SecurityLevel::Medium),
                        initial_data: Some(json!({"counter": 0})),
                    };

                    let session_id = manager.create_session(request).await.unwrap();

                    // 連続更新
                    for i in 0..update_count {
                        let mut session = manager.get_session(&session_id).await.unwrap().unwrap();

                        session.metadata.request_count = i + 1;
                        session.metadata.bytes_transferred = (i + 1) * 1024;
                        session.data["counter"] = json!(i);
                        session.data["timestamp"] = json!(chrono::Utc::now().timestamp());

                        manager.update_session(&session).await.unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_operations");

    for worker_count in [10, 25, 50, 100].iter() {
        group.throughput(Throughput::Elements(*worker_count as u64 * 100)); // 各ワーカーが100操作

        group.bench_with_input(
            BenchmarkId::new("mixed_operations", worker_count),
            worker_count,
            |b, &worker_count| {
                b.to_async(&rt).iter(|| async {
                    let storage = Arc::new(MemorySessionStorage::new());
                    let manager = Arc::new(
                        SessionManager::new(storage, SessionManagerConfig::default())
                            .await
                            .unwrap(),
                    );

                    let mut handles = Vec::new();

                    for worker_id in 0..worker_count {
                        let manager_clone = manager.clone();

                        let handle = tokio::spawn(async move {
                            let mut worker_sessions = Vec::new();

                            // 各ワーカーが複数の操作を実行
                            for op_id in 0..100 {
                                match op_id % 4 {
                                    // セッション作成 (25%)
                                    0 => {
                                        let request = CreateSessionRequest {
                                            user_id: Some(format!(
                                                "worker_{}_{}",
                                                worker_id, op_id
                                            )),
                                            ttl: Some(Duration::from_secs(3600)),
                                            ip_address: None,
                                            user_agent: None,
                                            security_level: Some(SecurityLevel::Low),
                                            initial_data: Some(json!({
                                                "worker_id": worker_id,
                                                "op_id": op_id
                                            })),
                                        };

                                        let session_id =
                                            manager_clone.create_session(request).await?;
                                        worker_sessions.push(session_id);
                                    }
                                    // セッション取得 (25%)
                                    1 => {
                                        if !worker_sessions.is_empty() {
                                            let session_id =
                                                &worker_sessions[op_id % worker_sessions.len()];
                                            let _session =
                                                manager_clone.get_session(session_id).await?;
                                        }
                                    }
                                    // セッション更新 (25%)
                                    2 => {
                                        if !worker_sessions.is_empty() {
                                            let session_id =
                                                &worker_sessions[op_id % worker_sessions.len()];
                                            if let Some(mut session) =
                                                manager_clone.get_session(session_id).await?
                                            {
                                                session.metadata.request_count += 1;
                                                session.data["op_id"] = json!(op_id);
                                                manager_clone.update_session(&session).await?;
                                            }
                                        }
                                    }
                                    // セッション無効化 (25%)
                                    3 => {
                                        if worker_sessions.len() > 5 {
                                            // 最低5個は保持
                                            let session_id = worker_sessions.remove(0);
                                            let _ =
                                                manager_clone.invalidate_session(&session_id).await;
                                        }
                                    }
                                    _ => unreachable!(),
                                }
                            }

                            Ok::<(), mcp_rs::session::SessionError>(())
                        });

                        handles.push(handle);
                    }

                    // すべてのワーカー完了待機
                    for handle in handles {
                        let _ = handle.await.unwrap();
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_usage");

    for data_size in [100, 1000, 10000].iter() {
        // バイト単位
        group.throughput(Throughput::Bytes(*data_size as u64 * 1000)); // 1000セッション

        group.bench_with_input(
            BenchmarkId::new("large_session_data", data_size),
            data_size,
            |b, &data_size| {
                b.to_async(&rt).iter(|| async {
                    let storage = Arc::new(MemorySessionStorage::new());
                    let manager = SessionManager::new(storage, SessionManagerConfig::default())
                        .await
                        .unwrap();

                    // 大きなデータを持つセッションを作成
                    let large_data = "x".repeat(data_size);

                    for i in 0..1000 {
                        let request = CreateSessionRequest {
                            user_id: Some(format!("memory_user_{}", i)),
                            ttl: Some(Duration::from_secs(3600)),
                            ip_address: None,
                            user_agent: None,
                            security_level: Some(SecurityLevel::Low),
                            initial_data: Some(json!({
                                "id": i,
                                "large_data": large_data,
                                "metadata": {
                                    "size": data_size,
                                    "created_at": chrono::Utc::now().to_rfc3339()
                                }
                            })),
                        };

                        let _session_id = manager.create_session(request).await.unwrap();
                    }

                    // 統計取得（メモリ使用量の間接測定）
                    let _stats = manager.get_stats(true).await.unwrap();
                })
            },
        );
    }

    group.finish();
}

fn bench_cleanup_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("cleanup_performance");

    for total_sessions in [5000, 10000, 20000].iter() {
        let expired_sessions = total_sessions / 2; // 50%を期限切れに
        group.throughput(Throughput::Elements(expired_sessions as u64));

        group.bench_with_input(
            BenchmarkId::new("expired_cleanup", total_sessions),
            total_sessions,
            |b, &total_sessions| {
                b.to_async(&rt).iter(|| async {
                    let storage = Arc::new(MemorySessionStorage::new());
                    let manager = SessionManager::new(
                        storage.clone(),
                        SessionManagerConfig {
                            enable_background_cleanup: false, // 手動制御
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap();

                    // セッション作成（半分を期限切れに設定）
                    for i in 0..total_sessions {
                        let ttl = if i < total_sessions / 2 {
                            Duration::from_millis(1) // すぐ期限切れ
                        } else {
                            Duration::from_secs(3600) // 有効
                        };

                        let request = CreateSessionRequest {
                            user_id: Some(format!("cleanup_user_{}", i)),
                            ttl: Some(ttl),
                            ip_address: None,
                            user_agent: None,
                            security_level: Some(SecurityLevel::Low),
                            initial_data: None,
                        };

                        let _session_id = manager.create_session(request).await.unwrap();
                    }

                    // 期限切れ状態にするため少し待機
                    tokio::time::sleep(Duration::from_millis(10)).await;

                    // クリーンアップ実行（ベンチマーク対象）
                    let cleaned_count = storage.cleanup_expired().await.unwrap();

                    assert!(cleaned_count >= total_sessions / 3); // 最低限のクリーンアップを確認
                })
            },
        );
    }

    group.finish();
}

fn bench_statistical_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("statistical_operations");

    for session_count in [1000, 5000, 10000].iter() {
        group.throughput(Throughput::Elements(1)); // 統計計算1回

        group.bench_with_input(
            BenchmarkId::new("stats_calculation", session_count),
            session_count,
            |b, &session_count| {
                b.to_async(&rt).iter(|| async {
                    let storage = Arc::new(MemorySessionStorage::new());
                    let manager = SessionManager::new(
                        storage,
                        SessionManagerConfig {
                            stats_cache_duration: Duration::from_millis(1), // キャッシュ無効
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap();

                    // 様々な状態のセッションを作成
                    for i in 0..session_count {
                        let request = CreateSessionRequest {
                            user_id: Some(format!("stats_user_{}", i)),
                            ttl: Some(Duration::from_secs(3600)),
                            ip_address: None,
                            user_agent: None,
                            security_level: Some(SecurityLevel::Medium),
                            initial_data: Some(json!({
                                "category": i % 5,
                                "priority": if i % 3 == 0 { "high" } else { "normal" }
                            })),
                        };

                        let session_id = manager.create_session(request).await.unwrap();

                        // 一部のセッションを更新して使用状況をシミュレート
                        if i % 10 == 0 {
                            let mut session =
                                manager.get_session(&session_id).await.unwrap().unwrap();
                            session.metadata.request_count = (i / 10) + 1;
                            session.metadata.bytes_transferred = (i / 10 + 1) * 1024;
                            manager.update_session(&session).await.unwrap();
                        }
                    }

                    // 統計計算（ベンチマーク対象）
                    let _stats = manager.get_stats(true).await.unwrap();
                })
            },
        );
    }

    group.finish();
}

criterion_group!(
    session_benches,
    bench_session_creation,
    bench_session_retrieval,
    bench_session_updates,
    bench_concurrent_operations,
    bench_memory_usage,
    bench_cleanup_performance,
    bench_statistical_operations
);

criterion_main!(session_benches);
