# WebSocket 統合ガイド

MCP-RS WebSocket機能の包括的なガイド

## 目次

- [概要](#概要)
- [基本的な使い方](#基本的な使い方)
- [高度な機能](#高度な機能)
- [設定オプション](#設定オプション)
- [ベストプラクティス](#ベストプラクティス)
- [トラブルシューティング](#トラブルシューティング)

---

## 概要

MCP-RS WebSocketは、以下の機能を提供する高性能なWebSocketサーバー実装です：

### コア機能

- **WebSocketサーバー**: Axumベースの非同期WebSocketサーバー
- **LLMストリーミング**: 大規模言語モデルの応答ストリーミング
- **コネクションプール**: 自動スケーリング可能なコネクション管理
- **負荷分散**: 複数の戦略（RoundRobin、LeastConnections、Random）
- **フェイルオーバー**: 自動リトライとサーキットブレーカー
- **レート制限**: TokenBucket、LeakyBucket、SlidingWindow
- **圧縮**: Gzip、Deflate、Brotli、Zstd
- **メトリクス**: Prometheus互換のメトリクス収集

---

## 基本的な使い方

### 1. シンプルなWebSocketサーバー

```rust
use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};
use mcp_rs::transport::websocket::WebSocketMetrics;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // メトリクス初期化
    let metrics = Arc::new(WebSocketMetrics::new().unwrap());

    // ルーター構築
    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .with_state(metrics);

    // サーバー起動
    axum::Server::bind(&"127.0.0.1:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(metrics): State<Arc<WebSocketMetrics>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, metrics))
}

async fn handle_socket(mut socket: WebSocket, metrics: Arc<WebSocketMetrics>) {
    metrics.increment_connections();

    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            metrics.increment_messages_received();
            
            let response = format!("Echo: {}", text);
            socket.send(Message::Text(response)).await.unwrap();
            
            metrics.increment_messages_sent();
        }
    }

    metrics.decrement_connections();
}
```

### 2. レート制限の追加

```rust
use mcp_rs::transport::websocket::{
    RateLimitConfig, RateLimitStrategy, RateLimiter,
};

// レート制限設定（100 req/sec）
let rate_config = RateLimitConfig {
    strategy: RateLimitStrategy::TokenBucket,
    max_requests_per_second: 100,
    max_burst: 200,
    window_size_ms: 1000,
};

let rate_limiter = Arc::new(RateLimiter::new(rate_config));

// メッセージハンドラー内で使用
match rate_limiter.check_global_rate_limit().await {
    Ok(true) => {
        // リクエスト処理
    }
    Ok(false) => {
        // レート制限超過
        socket.send(Message::Text("Rate limit exceeded".to_string())).await.ok();
    }
    Err(e) => {
        // エラー処理
    }
}
```

### 3. 圧縮の追加

```rust
use mcp_rs::transport::websocket::{
    CompressionAlgorithm, CompressionConfig, CompressionManager,
};

// Brotli圧縮設定
let compression_config = CompressionConfig {
    algorithm: CompressionAlgorithm::Brotli,
    level: 4,
    min_compress_size: 1024,
    ..Default::default()
};

let compression = CompressionManager::new(compression_config).unwrap();

// メッセージ送信前に圧縮
let data = b"Large payload to compress...";
let compressed = compression.compress(data).await.unwrap();

socket.send(Message::Binary(compressed)).await.ok();

// 受信後に解凍
if let Ok(Message::Binary(compressed_data)) = msg {
    let decompressed = compression.decompress(&compressed_data).await.unwrap();
    // decompressed を処理
}
```

---

## 高度な機能

### コネクションプール

```rust
use mcp_rs::transport::websocket::{
    ConnectionPool, PoolConfig, LoadBalanceStrategy,
};

// プール設定
let pool_config = PoolConfig {
    min_connections: 5,
    max_connections: 50,
    load_balance_strategy: LoadBalanceStrategy::LeastConnections,
    enable_auto_scaling: true,
    scale_up_threshold: 0.8,
    scale_down_threshold: 0.2,
    health_check_interval_ms: 5000,
};

let pool = ConnectionPool::new(pool_config).unwrap();

// 接続追加
let conn_id = pool.add_connection().await.unwrap();

// 接続選択（負荷分散）
let selected = pool.select_connection().await.unwrap();

// 接続削除
pool.remove_connection(&conn_id).await.unwrap();

// プール統計
let stats = pool.get_stats().await.unwrap();
println!("Active: {}/{}", stats.active_connections, stats.max_connections);
```

### フェイルオーバー

```rust
use mcp_rs::transport::websocket::{
    ConnectionPool, FailoverConfig, PoolConfig,
};

// フェイルオーバー設定
let failover_config = FailoverConfig {
    max_retries: 3,
    retry_delay_ms: 100,
    enable_circuit_breaker: true,
    circuit_breaker_threshold: 5,
    circuit_breaker_timeout_ms: 30000,
};

// プールをフェイルオーバー付きで作成
let pool = ConnectionPool::new_with_failover(pool_config, failover_config).unwrap();

// 自動リトライとサーキットブレーカーが有効
```

### LLMストリーミング

```rust
use mcp_rs::transport::websocket::{LlmStreamConfig, LlmStreamer};

// ストリーマー設定
let llm_config = LlmStreamConfig {
    chunk_size: 20,        // 1チャンクあたり20文字
    delay_ms: 50,          // チャンク間50ms遅延
    ..Default::default()
};

let streamer = LlmStreamer::new(llm_config);

// ストリーミング開始
let response = "This is a long LLM response that will be streamed...";
let stream_id = streamer.start_stream(response).await.unwrap();

// チャンクを順次送信
loop {
    match streamer.next_chunk(&stream_id).await {
        Ok(Some(chunk)) => {
            // チャンクを送信
            socket.send(Message::Text(chunk)).await.ok();
        }
        Ok(None) => {
            // ストリーミング完了
            break;
        }
        Err(e) => {
            // エラー処理
            break;
        }
    }
}

// ストリーム終了
streamer.end_stream(&stream_id).await.ok();
```

---

## 設定オプション

### レート制限戦略

#### TokenBucket（推奨）

```rust
RateLimitConfig {
    strategy: RateLimitStrategy::TokenBucket,
    max_requests_per_second: 100,
    max_burst: 200,  // バースト許容
    window_size_ms: 1000,
}
```

#### LeakyBucket

```rust
RateLimitConfig {
    strategy: RateLimitStrategy::LeakyBucket,
    max_requests_per_second: 100,
    max_burst: 0,  // バーストなし
    window_size_ms: 1000,
}
```

#### SlidingWindow

```rust
RateLimitConfig {
    strategy: RateLimitStrategy::SlidingWindow,
    max_requests_per_second: 100,
    max_burst: 0,
    window_size_ms: 1000,
}
```

### 圧縮アルゴリズム

| アルゴリズム | 圧縮率 | 速度 | 推奨用途 |
| ---------- | ----- | ---- | ------- |
| Gzip | 中 | 高 | 汎用（デフォルト） |
| Deflate | 中 | 高 | 互換性重視 |
| Brotli | 高 | 中 | テキストデータ |
| Zstd | 高 | 高 | 高性能要求 |

```rust
// Gzip（デフォルト）
CompressionConfig {
    algorithm: CompressionAlgorithm::Gzip,
    level: 6,  // 1-9（デフォルト: 6）
    min_compress_size: 1024,  // 1KB未満は圧縮しない
    ..Default::default()
}

// Brotli（高圧縮率）
CompressionConfig {
    algorithm: CompressionAlgorithm::Brotli,
    level: 4,  // 1-11（デフォルト: 4）
    min_compress_size: 1024,
    ..Default::default()
}

// Zstd（高速）
CompressionConfig {
    algorithm: CompressionAlgorithm::Zstd,
    level: 3,  // 1-21（デフォルト: 3）
    min_compress_size: 1024,
    ..Default::default()
}
```

### 負荷分散戦略

```rust
// RoundRobin - 順番に選択
LoadBalanceStrategy::RoundRobin

// LeastConnections - 最も接続数が少ないものを選択（推奨）
LoadBalanceStrategy::LeastConnections

// WeightedRoundRobin - 重み付きラウンドロビン
LoadBalanceStrategy::WeightedRoundRobin

// Random - ランダム選択
LoadBalanceStrategy::Random
```

---

## ベストプラクティス

### 1. メトリクス収集

常にメトリクスを有効にし、Prometheusで監視：

```rust
// メトリクスエンドポイント追加
app.route("/metrics", get(metrics_handler));

async fn metrics_handler(State(metrics): State<Arc<WebSocketMetrics>>) -> impl IntoResponse {
    match metrics.export_text() {
        Ok(text) => (
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            text,
        ).into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export metrics: {}", e),
        ).into_response(),
    }
}
```

### 2. エラーハンドリング

適切なエラーハンドリングとロギング：

```rust
use tracing::{error, info, warn};

while let Some(msg) = socket.recv().await {
    match msg {
        Ok(Message::Text(text)) => {
            info!("Received: {}", text);
            // 処理
        }
        Ok(Message::Close(_)) => {
            info!("Client closed connection");
            break;
        }
        Err(e) => {
            error!("WebSocket error: {}", e);
            metrics.increment_errors();
            break;
        }
        _ => {}
    }
}
```

### 3. リソース管理

接続のライフサイクル管理：

```rust
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    // 接続開始
    let conn_id = state.pool.add_connection().await.unwrap();
    state.metrics.increment_connections();
    
    // メッセージループ
    // ...
    
    // 接続終了（必ず実行）
    let _ = state.pool.remove_connection(&conn_id).await;
    state.metrics.decrement_connections();
}
```

### 4. 自動スケーリング

プール自動スケーリングを有効化：

```rust
PoolConfig {
    enable_auto_scaling: true,
    scale_up_threshold: 0.8,    // 80%で拡張
    scale_down_threshold: 0.2,  // 20%で縮小
    // ...
}
```

### 5. 圧縮の最適化

小さいメッセージは圧縮しない：

```rust
CompressionConfig {
    min_compress_size: 1024,  // 1KB未満は圧縮しない
    // ...
}
```

---

## トラブルシューティング

### 問題: 接続が頻繁に切断される

**原因**: レート制限が厳しすぎる

**解決策**:
```rust
RateLimitConfig {
    max_requests_per_second: 200,  // 増やす
    max_burst: 400,                // バーストも増やす
    // ...
}
```

### 問題: メモリ使用量が高い

**原因**: コネクションプールが大きすぎる

**解決策**:
```rust
PoolConfig {
    max_connections: 20,  // 減らす
    enable_auto_scaling: true,
    // ...
}
```

### 問題: レイテンシが高い

**原因**: 圧縮レベルが高すぎる

**解決策**:
```rust
CompressionConfig {
    level: 1,  // レベルを下げる（Gzipの場合）
    // または
    algorithm: CompressionAlgorithm::Zstd,  // 高速アルゴリズムに変更
    // ...
}
```

### 問題: メトリクスが収集されない

**原因**: カスタムレジストリを使用していない

**解決策**:
```rust
// テスト環境ではカスタムレジストリを使用
let metrics = WebSocketMetrics::new().unwrap();  // カスタムレジストリ使用
```

---

## サンプルコード

- [エコーサーバー](../examples/websocket_echo_server.rs) - 基本的なWebSocketサーバー
- [LLMチャット](../examples/websocket_llm_chat.rs) - LLMストリーミング統合
- [負荷分散サーバー](../examples/websocket_load_balanced.rs) - 高度な負荷分散

---

## 関連ドキュメント

- [LLM統合ガイド](llm-integration-guide.md)
- [パフォーマンスチューニング](websocket-performance.md)
- [統合テスト](../tests/websocket_integration_tests.rs)
- [ベンチマーク](../benches/websocket_benchmarks.rs)

---

## ライセンス

MIT または Apache-2.0
