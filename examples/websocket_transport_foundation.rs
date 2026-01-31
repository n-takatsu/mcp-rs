//! WebSocket Transport 基盤実装サンプル
//!
//! このサンプルは、WebSocketTransportの基本的な使用方法を示します。
//!
//! 実行方法:
//! ```bash
//! cargo run --example websocket_transport_foundation
//! ```

use mcp_rs::transport::websocket::{PoolConfig, StreamConfig, WebSocketTransport};
use mcp_rs::transport::Transport;
use mcp_rs::types::JsonRpcResponse;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ロガーの初期化
    env_logger::init();

    println!("=== WebSocket Transport 基盤実装サンプル ===\n");

    // Step 1: プール設定の作成
    println!("Step 1: プール設定の作成");
    let pool_config = PoolConfig {
        max_connections: 10,
        min_connections: 2,
        connection_timeout: Duration::from_secs(5),
        idle_timeout: Duration::from_secs(300),
        health_check_interval: Duration::from_secs(30),
    };
    println!("  - 最大接続数: {}", pool_config.max_connections);
    println!("  - 最小接続数: {}", pool_config.min_connections);
    println!("  - 接続タイムアウト: {:?}", pool_config.connection_timeout);
    println!();

    // Step 2: ストリーム設定の作成
    println!("Step 2: ストリーム設定の作成");
    let stream_config = StreamConfig {
        chunk_size: 8192,
        max_buffer_size: 1024 * 1024,
        compression_enabled: true,
    };
    println!("  - チャンクサイズ: {} バイト", stream_config.chunk_size);
    println!(
        "  - 最大バッファサイズ: {} バイト",
        stream_config.max_buffer_size
    );
    println!("  - 圧縮有効化: {}", stream_config.compression_enabled);
    println!();

    // Step 3: WebSocketTransportの作成
    println!("Step 3: WebSocketTransportの作成");
    let transport = WebSocketTransport::new(pool_config, stream_config)?
        .with_url("ws://localhost:8080");
    println!("  ✓ WebSocketTransportを作成しました");
    println!();

    // Step 4: トランスポート情報の表示
    println!("Step 4: トランスポート情報の表示");
    let info = transport.transport_info();
    println!("  - トランスポートタイプ: {:?}", info.transport_type);
    println!("  - 説明: {}", info.description);
    println!("  - 機能:");
    println!("    - 双方向通信: {}", info.capabilities.bidirectional);
    println!("    - 多重化: {}", info.capabilities.multiplexing);
    println!("    - 圧縮: {}", info.capabilities.compression);
    println!(
        "    - 最大メッセージサイズ: {:?}",
        info.capabilities.max_message_size
    );
    println!();

    // Step 5: 接続統計の表示
    println!("Step 5: 接続統計の表示");
    let stats = transport.connection_stats();
    println!("  - 送信メッセージ数: {}", stats.messages_sent);
    println!("  - 受信メッセージ数: {}", stats.messages_received);
    println!("  - 送信バイト数: {}", stats.bytes_sent);
    println!("  - 受信バイト数: {}", stats.bytes_received);
    println!("  - 稼働時間: {:?}", stats.uptime);
    println!();

    // Step 6: プール統計の表示
    println!("Step 6: プール統計の表示");
    let pool_stats = transport.get_statistics().await;
    println!("  - 総接続数: {}", pool_stats.total_connections);
    println!("  - アクティブ接続数: {}", pool_stats.active_connections);
    println!("  - アイドル接続数: {}", pool_stats.idle_connections);
    println!("  - 待機リクエスト数: {}", pool_stats.pending_requests);
    println!("  - 総リクエスト数: {}", pool_stats.total_requests);
    println!("  - 失敗リクエスト数: {}", pool_stats.failed_requests);
    println!(
        "  - 平均待機時間: {:.2}ms",
        pool_stats.avg_wait_time_ms
    );
    println!();

    // Note: 実際の接続開始は、WebSocketサーバーが起動している必要があります
    println!("Note: 実際の接続開始には、WebSocketサーバーの起動が必要です");
    println!("サーバーが起動していない場合、以下のコードはエラーになります:\n");
    println!("```rust");
    println!("// 接続開始");
    println!("transport.start().await?;");
    println!();
    println!("// メッセージの送信例");
    println!("let response = JsonRpcResponse {{");
    println!("    jsonrpc: \"2.0\".to_string(),");
    println!("    id: serde_json::Value::Number(1.into()),");
    println!("    result: Some(serde_json::json!({{\"status\": \"ok\"}})),");
    println!("    error: None,");
    println!("}};");
    println!("transport.send_message(response).await?;");
    println!();
    println!("// 接続停止");
    println!("transport.stop().await?;");
    println!("```");
    println!();

    println!("=== サンプル完了 ===");

    Ok(())
}
