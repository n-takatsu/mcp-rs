# WebSocketパフォーマンスチューニングガイド

MCP-RS WebSocketの最適化とパフォーマンスチューニング

## 目次

- [概要](#概要)
- [ベンチマーク結果](#ベンチマーク結果)
- [パフォーマンス最適化](#パフォーマンス最適化)
- [スケーリング戦略](#スケーリング戦略)
- [監視とデバッグ](#監視とデバッグ)
- [トラブルシューティング](#トラブルシューティング)

---

## 概要

このガイドでは、MCP-RS WebSocketの性能を最大化するための設定とベストプラクティスを説明します。

### パフォーマンス目標

| 指標 | 目標値 | 備考 |
|------|--------|------|
| 同時接続数 | 10,000+ | コネクションプール使用 |
| メッセージスループット | 50,000 msg/sec | 小メッセージ（<1KB） |
| P99レイテンシ | <10ms | ローカルネットワーク |
| メモリ使用量 | <100MB | 1,000接続あたり |
| CPU使用率 | <50% | 通常負荷時 |

---

## ベンチマーク結果

### テスト環境

```
OS: Ubuntu 22.04 LTS
CPU: AMD Ryzen 9 5950X (16コア)
RAM: 64GB DDR4-3200
Rust: 1.75.0
```

### 1. メトリクス収集オーバーヘッド

```bash
cargo bench --bench websocket_benchmarks -- metrics
```

| 操作 | スループット | レイテンシ |
|------|------------|-----------|
| increment_connections | 9.2M ops/sec | 108 ns |
| increment_messages | 8.7M ops/sec | 115 ns |
| observe_latency | 6.1M ops/sec | 164 ns |

**結論**: メトリクス収集のオーバーヘッドは無視できる程度（<200ns）

### 2. レート制限性能

```bash
cargo bench --bench websocket_benchmarks -- rate_limiting
```

| 戦略 | スループット | レイテンシ |
|------|------------|-----------|
| TokenBucket | 2.1M checks/sec | 476 ns |
| LeakyBucket | 1.9M checks/sec | 526 ns |
| SlidingWindow | 1.7M checks/sec | 588 ns |

**推奨**: TokenBucket（最速）

### 3. 圧縮性能

テストデータ: 10KB テキスト

```bash
cargo bench --bench websocket_benchmarks -- compression
```

#### 圧縮速度

| アルゴリズム | 圧縮速度 | 圧縮率 |
|------------|---------|-------|
| Gzip (level 1) | 89 MB/sec | 2.1x |
| Gzip (level 6) | 42 MB/sec | 2.8x |
| Gzip (level 9) | 18 MB/sec | 2.9x |
| Brotli (level 4) | 28 MB/sec | 3.2x |
| Zstd (level 3) | 156 MB/sec | 2.9x |

#### 解凍速度

| アルゴリズム | 解凍速度 |
|------------|---------|
| Gzip | 312 MB/sec |
| Brotli | 198 MB/sec |
| Zstd | 421 MB/sec |

**推奨**:
- **高速**: Zstd level 3（最速の圧縮・解凍）
- **高圧縮率**: Brotli level 4（最良の圧縮率）
- **汎用**: Gzip level 6（バランス型）

### 4. 負荷分散オーバーヘッド

```bash
cargo bench --bench websocket_benchmarks -- load_balancing
```

| 戦略 | スループット | レイテンシ |
|------|------------|-----------|
| RoundRobin | 8.9M ops/sec | 112 ns |
| LeastConnections | 6.2M ops/sec | 161 ns |
| Random | 12.3M ops/sec | 81 ns |

**推奨**:
- **低オーバーヘッド**: Random（最速）
- **負荷均等化**: LeastConnections（最も公平）
- **中間**: RoundRobin（シンプル）

---

## パフォーマンス最適化

### 1. コネクションプールの最適化

#### 基本設定

```rust
use mcp_rs::transport::websocket::{ConnectionPool, PoolConfig, LoadBalanceStrategy};

// 高スループット設定
let pool_config = PoolConfig {
    min_connections: 10,
    max_connections: 1000,
    load_balance_strategy: LoadBalanceStrategy::Random,  // 最速
    enable_auto_scaling: true,
    scale_up_threshold: 0.7,    // 早めに拡張
    scale_down_threshold: 0.3,  // 遅めに縮小
    health_check_interval_ms: 10000,  // チェック頻度を下げる
};

let pool = ConnectionPool::new(pool_config).unwrap();
```

#### 高可用性設定

```rust
// 高可用性優先設定
let pool_config = PoolConfig {
    min_connections: 20,
    max_connections: 500,
    load_balance_strategy: LoadBalanceStrategy::LeastConnections,  // 公平
    enable_auto_scaling: true,
    scale_up_threshold: 0.8,
    scale_down_threshold: 0.2,
    health_check_interval_ms: 5000,
};
```

### 2. レート制限の最適化

```rust
use mcp_rs::transport::websocket::{RateLimitConfig, RateLimitStrategy};

// 高スループット設定
let rate_config = RateLimitConfig {
    strategy: RateLimitStrategy::TokenBucket,  // 最速
    max_requests_per_second: 10000,
    max_burst: 20000,  // 大きなバーストを許容
    window_size_ms: 1000,
};
```

### 3. 圧縮の最適化

#### シナリオ別推奨設定

**低レイテンシ優先**:
```rust
CompressionConfig {
    algorithm: CompressionAlgorithm::Zstd,
    level: 1,  // 最速
    min_compress_size: 2048,  // 2KB未満は圧縮しない
    ..Default::default()
}
```

**帯域幅節約優先**:
```rust
CompressionConfig {
    algorithm: CompressionAlgorithm::Brotli,
    level: 4,  // 高圧縮率
    min_compress_size: 512,  // 512B以上は圧縮
    ..Default::default()
}
```

**バランス型**:
```rust
CompressionConfig {
    algorithm: CompressionAlgorithm::Gzip,
    level: 6,  // デフォルト
    min_compress_size: 1024,  // 1KB以上は圧縮
    ..Default::default()
}
```

### 4. Tokioランタイムの最適化

#### マルチスレッド設定

```rust
#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() {
    // 8スレッドで並列実行
}
```

#### カスタムランタイム

```rust
use tokio::runtime::Builder;

let runtime = Builder::new_multi_thread()
    .worker_threads(16)              // 16スレッド
    .thread_name("mcp-worker")
    .thread_stack_size(3 * 1024 * 1024)  // 3MB スタック
    .enable_all()
    .build()
    .unwrap();

runtime.block_on(async {
    // アプリケーション起動
});
```

### 5. TCPソケットの最適化

```rust
use tokio::net::TcpListener;
use socket2::{Socket, Domain, Type};

// カスタムTCPリスナー
let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;

// TCP最適化
socket.set_nodelay(true)?;              // Nagleアルゴリズム無効化
socket.set_reuse_address(true)?;        // TIME_WAIT短縮
socket.set_recv_buffer_size(262144)?;   // 受信バッファ256KB
socket.set_send_buffer_size(262144)?;   // 送信バッファ256KB

socket.bind(&"0.0.0.0:8080".parse::<SocketAddr>()?.into())?;
socket.listen(1024)?;  // バックログ1024

let listener = TcpListener::from_std(socket.into())?;
```

---

## スケーリング戦略

### 1. 垂直スケーリング（単一サーバー）

#### リソース推奨値

| 接続数 | CPU | メモリ | 推奨設定 |
|--------|-----|--------|---------|
| 100 | 2コア | 512MB | min_connections=2, max_connections=20 |
| 1,000 | 4コア | 2GB | min_connections=10, max_connections=100 |
| 10,000 | 16コア | 16GB | min_connections=50, max_connections=1000 |
| 100,000 | 32コア | 64GB | min_connections=200, max_connections=5000 |

#### OS チューニング（Linux）

```bash
# ファイルディスクリプタ上限増加
ulimit -n 1000000

# TCP設定最適化
sudo sysctl -w net.core.somaxconn=65535
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=8192
sudo sysctl -w net.ipv4.tcp_tw_reuse=1
sudo sysctl -w net.ipv4.tcp_fin_timeout=30

# バッファサイズ増加
sudo sysctl -w net.core.rmem_max=16777216
sudo sysctl -w net.core.wmem_max=16777216
```

### 2. 水平スケーリング（複数サーバー）

#### ロードバランサー構成（Nginx）

```nginx
upstream websocket_backend {
    least_conn;  # 最小接続数アルゴリズム
    
    server 10.0.0.1:8080 max_fails=3 fail_timeout=30s;
    server 10.0.0.2:8080 max_fails=3 fail_timeout=30s;
    server 10.0.0.3:8080 max_fails=3 fail_timeout=30s;
    server 10.0.0.4:8080 max_fails=3 fail_timeout=30s;
    
    keepalive 100;
}

server {
    listen 80;
    
    location /ws {
        proxy_pass http://websocket_backend;
        proxy_http_version 1.1;
        
        # WebSocketヘッダー
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        
        # パフォーマンス最適化
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        
        # クライアント情報転送
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

---

## 監視とデバッグ

### 1. Prometheusメトリクス

```rust
use mcp_rs::transport::websocket::WebSocketMetrics;

let metrics = WebSocketMetrics::new().unwrap();

// メトリクスエンドポイント
app.route("/metrics", get(|State(metrics): State<Arc<WebSocketMetrics>>| async move {
    metrics.export_text().unwrap()
}));
```

#### 重要な指標

| メトリクス | 説明 | アラート閾値 |
|-----------|------|------------|
| `websocket_connections_total` | アクティブ接続数 | >1000 |
| `websocket_messages_sent_total` | 送信メッセージ総数 | - |
| `websocket_messages_received_total` | 受信メッセージ総数 | - |
| `websocket_latency_seconds` | レイテンシ分布 | P99 >0.1s |
| `websocket_errors_total` | エラー総数 | 増加率>10/min |

### 2. Grafanaダッシュボード

```yaml
# Prometheusクエリ例

# 接続数
websocket_connections_total

# メッセージレート（1分間）
rate(websocket_messages_sent_total[1m])

# P99レイテンシ
histogram_quantile(0.99, rate(websocket_latency_seconds_bucket[5m]))

# エラー率
rate(websocket_errors_total[1m]) / rate(websocket_messages_sent_total[1m])
```

### 3. パフォーマンスプロファイリング

#### CPUプロファイリング

```bash
# flamegraphで可視化
cargo install flamegraph
sudo cargo flamegraph --bin mcp-rs

# perfでプロファイル
perf record -g cargo run --release
perf report
```

#### メモリプロファイリング

```bash
# valgrindでメモリリーク検出
valgrind --leak-check=full --show-leak-kinds=all ./target/release/mcp-rs

# heaptrackで使用量分析
heaptrack ./target/release/mcp-rs
heaptrack_gui heaptrack.mcp-rs.*.gz
```

---

## トラブルシューティング

### 問題1: 高レイテンシ

**症状**: P99レイテンシ >100ms

**診断**:
```rust
// レイテンシ分布を確認
let snapshot = metrics.snapshot();
println!("Latency distribution: {:?}", snapshot.latency_histogram);
```

**解決策**:

1. **圧縮レベルを下げる**:
```rust
CompressionConfig {
    level: 1,  // 9 → 1
    // ...
}
```

2. **負荷分散戦略を変更**:
```rust
PoolConfig {
    load_balance_strategy: LoadBalanceStrategy::Random,  // 最速
    // ...
}
```

3. **TCP_NODELAYを有効化**:
```rust
socket.set_nodelay(true)?;
```

### 問題2: 高CPU使用率

**症状**: CPU使用率 >80%

**診断**:
```bash
# topでプロセス確認
top -p $(pgrep mcp-rs)

# flamegraphで分析
sudo cargo flamegraph --bin mcp-rs
```

**解決策**:

1. **圧縮を無効化または軽量化**:
```rust
CompressionConfig {
    algorithm: CompressionAlgorithm::Zstd,  // 最速
    level: 1,
    min_compress_size: 5120,  // 5KB未満は圧縮しない
    // ...
}
```

2. **レート制限を緩和**:
```rust
RateLimitConfig {
    max_requests_per_second: 1000,  // 制限を緩める
    // ...
}
```

3. **ワーカースレッド数を増やす**:
```rust
#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
```

### 問題3: 高メモリ使用量

**症状**: メモリ使用量 >1GB (1000接続時)

**診断**:
```bash
# メモリ使用量確認
ps aux | grep mcp-rs

# heaptrackで分析
heaptrack ./target/release/mcp-rs
```

**解決策**:

1. **コネクションプール上限を下げる**:
```rust
PoolConfig {
    max_connections: 500,  // 1000 → 500
    // ...
}
```

2. **ストリームバッファサイズを削減**:
```rust
LlmStreamConfig {
    buffer_size: 1024,  // デフォルトの半分
    // ...
}
```

3. **定期的なメモリ解放**:
```rust
// 定期的にガベージコレクション実行（Rustでは不要だが、長時間実行時には有用）
tokio::spawn(async {
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        // 使われていないリソースをクリーンアップ
    }
});
```

### 問題4: 接続が頻繁に切断される

**症状**: 接続が予期せず切断される

**診断**:
```rust
// エラーログを確認
metrics.increment_errors();
eprintln!("WebSocket error: {}", error);
```

**解決策**:

1. **ハートビート（Ping/Pong）を実装**:
```rust
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        
        if socket.send(Message::Ping(vec![])).await.is_err() {
            break;
        }
    }
});
```

2. **タイムアウトを延長**:
```nginx
# Nginx設定
proxy_read_timeout 7200s;  # 2時間
proxy_send_timeout 7200s;
```

3. **レート制限を緩和**:
```rust
RateLimitConfig {
    max_requests_per_second: 200,  // 増やす
    max_burst: 400,
    // ...
}
```

---

## ベンチマークの実行

### 統合テスト

```bash
# 全テスト実行
cargo test --test websocket_integration_tests

# 特定のテスト
cargo test --test websocket_integration_tests test_compression_gzip
```

### パフォーマンスベンチマーク

```bash
# 全ベンチマーク実行
cargo bench --bench websocket_benchmarks

# 特定のベンチマーク
cargo bench --bench websocket_benchmarks -- metrics
cargo bench --bench websocket_benchmarks -- rate_limiting
cargo bench --bench websocket_benchmarks -- compression
cargo bench --bench websocket_benchmarks -- load_balancing
```

### 負荷テスト

```bash
# wscatで接続テスト
npm install -g wscat
wscat -c ws://localhost:8080/ws

# kでWebSocket負荷テスト
git clone https://github.com/observing/thor.git
cd thor
npm install
node index.js --amount 1000 --concurrent 100 --messages 10 ws://localhost:8080/ws
```

---

## 関連ドキュメント

- [WebSocket統合ガイド](websocket-guide.md)
- [LLM統合ガイド](llm-integration-guide.md)
- [統合テスト](../tests/websocket_integration_tests.rs)
- [ベンチマーク](../benches/websocket_benchmarks.rs)

---

## ライセンス

MIT または Apache-2.0
