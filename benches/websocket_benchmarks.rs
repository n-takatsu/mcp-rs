//! WebSocket Performance Benchmarks
//!
//! WebSocket機能のパフォーマンスベンチマーク

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use mcp_rs::transport::websocket::{
    BalancingStrategy, CompressionConfig, CompressionManager, CompressionType, RateLimitConfig,
    RateLimitStrategy, RateLimiter, WebSocketMetrics,
};
use std::time::Duration;

/// メトリクス収集のベンチマーク
fn bench_metrics_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("metrics");

    group.bench_function("increment_connections", |b| {
        let metrics = WebSocketMetrics::new().unwrap();
        b.iter(|| {
            metrics.increment_connections();
        });
    });

    group.bench_function("increment_messages", |b| {
        let metrics = WebSocketMetrics::new().unwrap();
        b.iter(|| {
            metrics.increment_messages_sent();
            metrics.increment_messages_received();
        });
    });

    group.bench_function("observe_latency", |b| {
        let metrics = WebSocketMetrics::new().unwrap();
        b.iter(|| {
            metrics.observe_latency(black_box(0.05)); // 50ms
        });
    });

    group.finish();
}

/// レート制限のベンチマーク
fn bench_rate_limiting(c: &mut Criterion) {
    let mut group = c.benchmark_group("rate_limiting");

    group.bench_function("token_bucket_check", |b| {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 1000,
            max_burst: 2000,
            window_size_ms: 1000,
        };
        let limiter = RateLimiter::new(config);
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(rt).iter(|| async {
            let _ = limiter.check_rate_limit("bench_key").await;
        });
    });

    group.bench_function("leaky_bucket_check", |b| {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::LeakyBucket,
            max_requests_per_second: 1000,
            max_burst: 2000,
            window_size_ms: 1000,
        };
        let limiter = RateLimiter::new(config);
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(rt).iter(|| async {
            let _ = limiter.check_rate_limit("bench_key").await;
        });
    });

    group.bench_function("sliding_window_check", |b| {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::SlidingWindow,
            max_requests_per_second: 1000,
            max_burst: 1000,
            window_size_ms: 1000,
        };
        let limiter = RateLimiter::new(config);
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(rt).iter(|| async {
            let _ = limiter.check_rate_limit("bench_key").await;
        });
    });

    group.finish();
}

/// 圧縮のベンチマーク
fn bench_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression");
    group.throughput(Throughput::Bytes(10240)); // 10KB

    let data = vec![b'A'; 10240]; // 10KB of data

    group.bench_function("gzip_compress", |b| {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 100,
        };
        let manager = CompressionManager::new(config);

        b.iter(|| {
            let _ = manager.compress(black_box(&data));
        });
    });

    group.bench_function("gzip_decompress", |b| {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 100,
        };
        let manager = CompressionManager::new(config);
        let compressed = manager.compress(&data).unwrap();

        b.iter(|| {
            let _ = manager.decompress(black_box(&compressed));
        });
    });

    group.bench_function("brotli_compress", |b| {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 100,
        };
        let manager = CompressionManager::new(config);

        b.iter(|| {
            let _ = manager.compress_brotli(black_box(&data));
        });
    });

    group.bench_function("brotli_decompress", |b| {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 100,
        };
        let manager = CompressionManager::new(config);
        let compressed = manager.compress_brotli(&data).unwrap();

        b.iter(|| {
            let _ = manager.decompress_brotli(black_box(&compressed));
        });
    });

    group.bench_function("zstd_compress", |b| {
        let config = CompressionConfig {
            compression_type: CompressionType::Zstd,
            level: 3,
            min_size: 100,
        };
        let manager = CompressionManager::new(config);

        b.iter(|| {
            let _ = manager.compress(black_box(&data));
        });
    });

    group.bench_function("zstd_decompress", |b| {
        let config = CompressionConfig {
            compression_type: CompressionType::Zstd,
            level: 3,
            min_size: 100,
        };
        let manager = CompressionManager::new(config);
        let compressed = manager.compress(&data).unwrap();

        b.iter(|| {
            let _ = manager.decompress(black_box(&compressed));
        });
    });

    group.finish();
}

/// 圧縮レベル別のベンチマーク
fn bench_compression_levels(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_levels");
    group.throughput(Throughput::Bytes(10240));

    let data = vec![b'A'; 10240];

    for level in [1, 3, 6, 9] {
        group.bench_function(format!("gzip_level_{}", level), |b| {
            let config = CompressionConfig {
                compression_type: CompressionType::Gzip,
                level,
                min_size: 100,
            };
            let manager = CompressionManager::new(config);

            b.iter(|| {
                let _ = manager.compress(black_box(&data));
            });
        });
    }

    group.finish();
}

/// 負荷分散戦略のベンチマーク
fn bench_load_balancing(c: &mut Criterion) {
    use mcp_rs::transport::websocket::{BalancerManager, Endpoint};

    let mut group = c.benchmark_group("load_balancing");

    let endpoints = vec![
        Endpoint::new("server1".to_string(), "ws://localhost:8081".to_string()),
        Endpoint::new("server2".to_string(), "ws://localhost:8082".to_string()),
        Endpoint::new("server3".to_string(), "ws://localhost:8083".to_string()),
    ];

    group.bench_function("round_robin_select", |b| {
        let manager = BalancerManager::new(endpoints.clone(), BalancingStrategy::RoundRobin);

        b.iter(|| {
            let _ = manager.select_endpoint();
        });
    });

    group.bench_function("least_connections_select", |b| {
        let manager =
            BalancerManager::new(endpoints.clone(), BalancingStrategy::LeastConnections);

        b.iter(|| {
            let _ = manager.select_endpoint();
        });
    });

    group.bench_function("random_select", |b| {
        let manager = BalancerManager::new(endpoints.clone(), BalancingStrategy::Random);

        b.iter(|| {
            let _ = manager.select_endpoint();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_metrics_collection,
    bench_rate_limiting,
    bench_compression,
    bench_compression_levels,
    bench_load_balancing
);
criterion_main!(benches);
