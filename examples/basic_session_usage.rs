use mcp_rs::session::{
    CreateSessionRequest, MemorySessionStorage, SecurityLevel, SessionManager, SessionManagerConfig,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// 基本的なセッション使用例
///
/// このサンプルは、セッション管理システムの基本的な機能を実演します：
/// - セッション作成
/// - セッション取得・更新
/// - セッションライフサイクル管理
/// - 統計取得

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ初期化
    tracing_subscriber::fmt().with_env_filter("debug").init();

    println!("🧪 セッション管理システム - 基本使用例");
    println!("========================================");

    // Step 1: セッションマネージャー初期化
    println!("\n📦 Step 1: セッションマネージャー初期化");

    let storage = Arc::new(MemorySessionStorage::new());
    let config = SessionManagerConfig {
        default_ttl: Duration::from_secs(3600),     // 1時間
        cleanup_interval: Duration::from_secs(300), // 5分
        max_sessions_per_user: 5,
        enable_background_cleanup: true,
        stats_cache_duration: Duration::from_secs(60),
    };

    let manager = SessionManager::new(storage, config).await?;
    println!("✅ セッションマネージャーが初期化されました");

    // Step 2: セッション作成
    println!("\n👤 Step 2: ユーザーセッション作成");

    let request = CreateSessionRequest {
        user_id: Some("user@example.com".to_string()),
        ttl: Some(Duration::from_secs(7200)), // 2時間
        ip_address: Some("192.168.1.100".parse().unwrap()),
        user_agent: Some("Example Client v1.0".to_string()),
        security_level: Some(SecurityLevel::Medium),
        initial_data: Some(json!({
            "preferences": {
                "theme": "dark",
                "language": "ja",
                "notifications": true
            },
            "metadata": {
                "client_version": "1.0.0",
                "platform": "desktop"
            }
        })),
    };

    let session_id = manager.create_session(request).await?;
    println!("✅ セッションが作成されました: {}", session_id.as_str());

    // Step 3: セッション取得・表示
    println!("\n🔍 Step 3: セッション詳細取得");

    if let Some(session) = manager.get_session(&session_id).await? {
        println!("📋 セッション情報:");
        println!("   ID: {}", session.id.as_str());
        println!("   ユーザー: {:?}", session.user_id);
        println!("   状態: {:?}", session.state);
        println!(
            "   セキュリティレベル: {:?}",
            session.security.security_level
        );
        println!(
            "   作成日時: {}",
            session.metadata.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        println!(
            "   有効期限: {}",
            session.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        println!(
            "   データ: {}",
            serde_json::to_string_pretty(&session.data)?
        );
    }

    // Step 4: セッション更新
    println!("\n📝 Step 4: セッション使用・更新シミュレーション");

    for i in 1..=5 {
        if let Some(mut session) = manager.get_session(&session_id).await? {
            // セッションデータ更新
            session.metadata.request_count += 1;
            session.metadata.bytes_transferred += 1024 * i;
            session.metadata.last_accessed = chrono::Utc::now();

            // アプリケーションデータ更新
            session.data["last_action"] = json!(format!("action_{}", i));
            session.data["request_history"] = json!(session.metadata.request_count);

            manager.update_session(&session).await?;

            println!(
                "   📊 更新 {}: リクエスト数={}, 転送量={}KB",
                i,
                session.metadata.request_count,
                session.metadata.bytes_transferred / 1024
            );
        }

        // リクエスト間隔をシミュレート
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Step 5: セッション延長
    println!("\n⏰ Step 5: セッション延長");

    let extended_ttl = Duration::from_secs(10800); // 3時間に延長
    manager.extend_session(&session_id, extended_ttl).await?;

    if let Some(session) = manager.get_session(&session_id).await? {
        println!("✅ セッションが延長されました");
        println!(
            "   新しい有効期限: {}",
            session.expires_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }

    // Step 6: 複数ユーザーセッション作成
    println!("\n👥 Step 6: 複数ユーザーセッション作成");

    let users = vec![
        "alice@example.com",
        "bob@example.com",
        "charlie@example.com",
    ];
    let mut user_sessions = Vec::new();

    for (i, user_email) in users.iter().enumerate() {
        let request = CreateSessionRequest {
            user_id: Some(user_email.to_string()),
            ttl: Some(Duration::from_secs(3600)),
            ip_address: Some(format!("192.168.1.{}", 101 + i).parse().unwrap()),
            user_agent: Some(format!("Client-{}", i + 1)),
            security_level: Some(if i == 0 {
                SecurityLevel::High
            } else {
                SecurityLevel::Medium
            }),
            initial_data: Some(json!({
                "user_type": if i == 0 { "admin" } else { "regular" },
                "session_number": i + 1
            })),
        };

        let session_id = manager.create_session(request).await?;
        user_sessions.push((user_email, session_id));

        println!("   ✅ {}のセッション作成完了", user_email);
    }

    // Step 7: システム統計表示
    println!("\n📊 Step 7: システム統計");

    let stats = manager.get_stats(true).await?;
    println!("📈 現在の統計:");
    println!("   総セッション数: {}", stats.total_sessions);
    println!("   アクティブセッション数: {}", stats.active_sessions);
    println!("   期限切れセッション数: {}", stats.expired_sessions);
    println!("   本日作成セッション数: {}", stats.sessions_created_today);
    println!(
        "   平均セッション継続時間: {:.1}分",
        stats.average_duration_minutes
    );
    println!("   総転送量: {}KB", stats.total_bytes_transferred / 1024);
    println!(
        "   統計計算日時: {}",
        stats.calculated_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    // Step 8: セッション検索
    println!("\n🔍 Step 8: セッション検索");

    use mcp_rs::session::SessionFilter;

    // アクティブセッション検索
    let active_filter = SessionFilter {
        user_id: None,
        state: Some(mcp_rs::session::SessionState::Active),
        expired_before: None,
        created_after: None,
        limit: Some(10),
    };

    let active_sessions = manager.find_sessions(&active_filter).await?;
    println!("🟢 アクティブセッション: {}個", active_sessions.len());

    // 特定ユーザーのセッション検索
    let user_filter = SessionFilter {
        user_id: Some("alice@example.com".to_string()),
        state: None,
        expired_before: None,
        created_after: None,
        limit: None,
    };

    let alice_sessions = manager.find_sessions(&user_filter).await?;
    println!("👤 aliceのセッション: {}個", alice_sessions.len());

    // Step 9: セッション無効化
    println!("\n❌ Step 9: セッション無効化");

    // 最初のセッションを無効化
    manager.invalidate_session(&session_id).await?;
    println!("✅ 初期セッションが無効化されました");

    if let Some(session) = manager.get_session(&session_id).await? {
        println!("   状態: {:?}", session.state);
    }

    // Step 10: 最終統計表示
    println!("\n📊 Step 10: 最終統計");

    let final_stats = manager.get_stats(true).await?;
    println!("📈 最終統計:");
    println!("   総セッション数: {}", final_stats.total_sessions);
    println!("   アクティブセッション数: {}", final_stats.active_sessions);
    println!(
        "   無効化されたセッション数: {}",
        final_stats.total_sessions - final_stats.active_sessions - final_stats.expired_sessions
    );

    println!("\n🎉 基本使用例が完了しました！");
    println!("このサンプルでは以下の機能を実演しました：");
    println!("   ✓ セッション作成・管理");
    println!("   ✓ セッション更新・延長");
    println!("   ✓ 複数ユーザー対応");
    println!("   ✓ セッション検索・フィルタリング");
    println!("   ✓ 統計情報取得");
    println!("   ✓ セッション無効化");

    Ok(())
}
