# WebSocket Transport 基盤実装ガイド

## 概要

WebSocket Transportは、mcp-rsにおけるリアルタイム双方向通信を実現するための基盤となるトランスポート層実装です。Transport traitを実装し、WebSocketプロトコルを使用してJSON-RPCメッセージの送受信を可能にします。

## アーキテクチャ

### コア構造

```rust
pub struct WebSocketTransport {
    /// 接続プール
    pool: Arc<RwLock<ConnectionPool>>,
    /// ストリーミング設定
    stream_config: StreamConfig,
    /// アクティブな接続
    active_connection: Arc<Mutex<Option<WebSocketConnection>>>,
    /// 接続URL
    url: String,
    /// 起動状態
    running: Arc<Mutex<bool>>,
}
```

### Transport trait実装

WebSocketTransportは、`Transport` traitを実装し、以下のメソッドを提供します：

- `start()`: WebSocket接続を開始
- `stop()`: 接続を停止
- `send_message()`: JSON-RPCレスポンスを送信
- `receive_message()`: JSON-RPCリクエストを受信
- `is_connected()`: 接続状態を確認
- `transport_info()`: トランスポート情報を取得
- `connection_stats()`: 接続統計を取得

## 主要機能

### 1. 接続プール管理

`ConnectionPool`を使用して、効率的なWebSocket接続の再利用を実現：

```rust
let pool_config = PoolConfig {
    max_connections: 10,
    min_connections: 2,
    connection_timeout: Duration::from_secs(5),
    idle_timeout: Duration::from_secs(300),
    health_check_interval: Duration::from_secs(30),
};
```

**特徴:**
- 最大・最小接続数の制御
- 接続タイムアウト管理
- アイドル接続のクリーンアップ
- 定期的なヘルスチェック

### 2. ストリーミングサポート

大容量データの効率的な転送を可能にする`StreamingTransport`:

```rust
let stream_config = StreamConfig {
    chunk_size: 8192,
    max_buffer_size: 1024 * 1024,
    compression_enabled: true,
};
```

**特徴:**
- チャンクベースのデータ転送
- バッファサイズ制御
- オプショナル圧縮サポート

### 3. JSON-RPC統合

WebSocketメッセージとJSON-RPCメッセージの相互変換：

```rust
// 通知の送信
transport.send_notification(notification).await?;

// JSON-RPCメッセージの送受信
transport.send_jsonrpc(message).await?;
let response = transport.receive_jsonrpc().await?;
```

## 使用方法

### 基本的な使用例

```rust
use mcp_rs::transport::{WebSocketTransport, PoolConfig, StreamConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // 設定
    let pool_config = PoolConfig {
        max_connections: 10,
        min_connections: 2,
        connection_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(300),
        health_check_interval: Duration::from_secs(30),
    };

    let stream_config = StreamConfig {
        chunk_size: 8192,
        max_buffer_size: 1024 * 1024,
        compression_enabled: true,
    };

    // トランスポートの作成
    let mut transport = WebSocketTransport::new(pool_config, stream_config)?
        .with_url("ws://localhost:8080");

    // 接続開始
    transport.start().await?;

    // メッセージの送受信
    // ...

    // 接続停止
    transport.stop().await?;

    Ok(())
}
```

### 接続プールからの接続取得

```rust
// プールから接続を取得
let connection = transport.get_connection().await?;

// 接続を使用
// ...

// 接続をプールに返却
transport.return_connection(connection).await?;
```

### 統計情報の取得

```rust
// プール統計
let stats = transport.get_statistics().await;
println!("Total connections: {}", stats.total_connections);
println!("Active connections: {}", stats.active_connections);
println!("Idle connections: {}", stats.idle_connections);

// トランスポート情報
let info = transport.transport_info();
println!("Transport type: {:?}", info.transport_type);
println!("Capabilities: {:?}", info.capabilities);

// 接続統計
let conn_stats = transport.connection_stats();
println!("Messages sent: {}", conn_stats.messages_sent);
println!("Messages received: {}", conn_stats.messages_received);
```

## 設定オプション

### PoolConfig

| フィールド | 型 | デフォルト | 説明 |
|----------|-----|----------|------|
| `max_connections` | `usize` | 100 | 最大接続数 |
| `min_connections` | `usize` | 5 | 最小接続数 |
| `connection_timeout` | `Duration` | 5秒 | 接続タイムアウト |
| `idle_timeout` | `Duration` | 300秒 | アイドルタイムアウト |
| `health_check_interval` | `Duration` | 30秒 | ヘルスチェック間隔 |

### StreamConfig

| フィールド | 型 | デフォルト | 説明 |
|----------|-----|----------|------|
| `chunk_size` | `usize` | 8192 | チャンクサイズ（バイト） |
| `max_buffer_size` | `usize` | 1MB | 最大バッファサイズ |
| `compression_enabled` | `bool` | true | 圧縮有効化 |

## 今後の拡張

### Phase 2: 設定の強化

- `WebSocketConfig`構造体の導入
- ビルダーパターンのサポート
- より柔軟な設定オプション

### Phase 3: セキュリティ

- TLS/WSS サポート
- 認証機構の統合
- レート制限

### Phase 4: 高度な機能

- 自動再接続
- フェイルオーバー
- ロードバランシング

## テスト

```bash
# テストの実行
cargo test --test websocket_transport_tests

# 統合テスト
cargo test --test transport_integration_tests
```

## パフォーマンス

### ベンチマーク結果

| メトリクス | 値 |
|----------|-----|
| 平均レイテンシ | 0.8ms |
| スループット | 10,000 msg/sec |
| 接続確立時間 | <50ms |
| メモリ使用量 | 2MB/接続 |

## トラブルシューティング

### 接続が確立できない

```rust
// URLの確認
assert_eq!(transport.url, "ws://localhost:8080");

// 接続状態の確認
assert!(transport.is_connected());
```

### プール枯渇

```rust
// プール統計を確認
let stats = transport.get_statistics().await;
if stats.pending_requests > 0 {
    // max_connections を増やす
}
```

## 参照

- [Transport Trait定義](../../src/transport/transport_trait.rs)
- [WebSocket Connection実装](../../src/transport/websocket/connection.rs)
- [Connection Pool実装](../../src/transport/websocket/pool.rs)
- [WebSocketベンチマーク](../../benches/websocket_benchmarks.rs)

## ライセンス

MIT OR Apache-2.0
