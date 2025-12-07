//! 動的トランスポート切り替えデモ
//!
//! このデモでは以下の機能を実演します:
//! - STDIOからHTTPへの動的切り替え
//! - HTTPからSTDIOへの動的切り替え
//! - 統計情報の取得

use mcp_rs::transport::{DynamicTransportManager, TransportConfig};
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("=== 動的トランスポート切り替えデモ ===\n");

    // 1. DynamicTransportManagerの作成 (初期はSTDIO)
    println!("1. DynamicTransportManagerを作成 (初期: STDIO)");
    let config = TransportConfig::default(); // デフォルトはSTDIO
    let manager = DynamicTransportManager::new(config)?;
    println!("   ✓ STDIO transport初期化完了\n");

    // 2. STDIOからHTTPへ切り替え
    println!("2. STDIOからHTTPへ切り替え");
    let addr = "127.0.0.1:3000".parse()?;
    manager.switch_to_http(addr).await?;
    println!("   ✓ HTTPサーバー起動: http://127.0.0.1:3000\n");

    // HTTPサーバーが起動するまで待機
    sleep(Duration::from_millis(500)).await;

    // 3. サンプルリクエスト送信 (実際の環境では別クライアントから送信)
    println!("3. HTTPエンドポイントテスト");
    println!("   curl -X POST http://127.0.0.1:3000/message \\");
    println!("     -H 'Content-Type: application/json' \\");
    println!(
        "     -d '{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{}}}}'\n"
    );

    // 4. しばらく実行
    println!("4. HTTPサーバーを5秒間実行...");
    sleep(Duration::from_secs(5)).await;

    // 5. HTTPからSTDIOへ切り替え
    println!("5. HTTPからSTDIOへ切り替え");
    manager.switch_to_stdio().await?;
    println!("   ✓ HTTPサーバー停止");
    println!("   ✓ STDIO transport再開\n");

    println!("=== デモ完了 ===");
    Ok(())
}
