//! Policy Hot-Reload Live Demonstration
//!
//! このデモは以下を実演します:
//! 1. リアルタイムポリシーファイル監視
//! 2. 設定変更の即座反映
//! 3. エラー処理とフォールバック
//! 4. ログ出力とイベント追跡

use mcp_rs::policy_watcher::{PolicyFileWatcher, PolicyChangeEvent};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    println!("🎬 MCP-RS Policy Hot-Reload Live Demonstration");
    println!("===============================================");
    println!();
    
    // デモ用ディレクトリの監視を開始
    let demo_path = "./demo-policies";
    let watcher = PolicyFileWatcher::new(demo_path);
    let mut receiver = watcher.subscribe();
    
    info!("📁 監視開始: {}", demo_path);
    println!("📁 Monitoring directory: {}", demo_path);
    
    // ファイル監視開始
    if let Err(e) = watcher.start_watching().await {
        error!("❌ 監視開始に失敗: {}", e);
        return Err(e.into());
    }
    
    println!("✅ File watcher started successfully");
    println!();
    println!("🔄 Demonstration Instructions:");
    println!("   1. Edit files in ./demo-policies/ directory");
    println!("   2. Save changes to see real-time detection");
    println!("   3. Try different file formats (.toml, .yaml, .json)");
    println!("   4. Press Ctrl+C to stop the demonstration");
    println!();
    
    // デモ実行ループ
    let mut change_count = 0;
    let start_time = std::time::Instant::now();
    
    loop {
        tokio::select! {
            // ファイル変更イベントの処理
            event_result = receiver.recv() => {
                match event_result {
                    Ok(event) => {
                        change_count += 1;
                        handle_policy_change(event, change_count).await;
                    }
                    Err(e) => {
                        warn!("⚠️ イベント受信エラー: {}", e);
                    }
                }
            }
            
            // 定期的なステータス表示
            _ = sleep(Duration::from_secs(10)) => {
                let elapsed = start_time.elapsed();
                println!("📊 Status: {} changes detected in {:.1}s | Monitoring active...", 
                    change_count, elapsed.as_secs_f64());
            }
        }
    }
}

async fn handle_policy_change(event: PolicyChangeEvent, count: usize) {
    let change_type_emoji = match event.change_type {
        mcp_rs::policy_watcher::PolicyChangeType::Created => "➕",
        mcp_rs::policy_watcher::PolicyChangeType::Modified => "📝",
        mcp_rs::policy_watcher::PolicyChangeType::Deleted => "🗑️",
    };
    
    let file_name = std::path::Path::new(&event.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    
    println!();
    println!("🔥 POLICY CHANGE DETECTED #{}", count);
    println!("   {} File: {}", change_type_emoji, file_name);
    println!("   📁 Path: {}", event.file_path);
    println!("   🕒 Time: {}", event.timestamp.format("%H:%M:%S"));
    println!("   🔄 Action: {:?}", event.change_type);
    
    // 実際のアプリケーションではここで設定を再読み込み
    info!("🔄 Simulating policy reload for: {}", file_name);
    
    // ファイル内容の簡単な検証デモ
    if let Ok(content) = std::fs::read_to_string(&event.file_path) {
        let line_count = content.lines().count();
        let size = content.len();
        
        println!("   📄 Content: {} lines, {} bytes", line_count, size);
        
        // 設定ファイルの種類に応じた処理デモ
        match event.file_path.split('.').last() {
            Some("toml") => {
                println!("   🔧 Processing TOML configuration...");
                // 実際の環境では toml::from_str() でパース
            }
            Some("yaml") | Some("yml") => {
                println!("   🔧 Processing YAML configuration...");
                // 実際の環境では serde_yaml::from_str() でパース
            }
            Some("json") => {
                println!("   🔧 Processing JSON configuration...");
                // 実際の環境では serde_json::from_str() でパース
            }
            _ => {
                warn!("   ⚠️ Unknown file format");
            }
        }
    } else {
        warn!("   ⚠️ Could not read file content (may be deleted)");
    }
    
    println!("   ✅ Policy update processing complete");
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_demo_file_detection() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_string_lossy().to_string();

        let watcher = PolicyFileWatcher::new(&temp_path);
        let mut receiver = watcher.subscribe();

        // 監視開始
        watcher.start_watching().await.unwrap();

        // デモファイル作成
        let demo_file = temp_dir.path().join("demo.toml");
        fs::write(&demo_file, "demo = true").unwrap();

        // イベント受信確認
        let event = tokio::time::timeout(
            Duration::from_secs(3), 
            receiver.recv()
        ).await.unwrap().unwrap();

        assert!(event.file_path.contains("demo.toml"));
    }
}