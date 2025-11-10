//! Test Data
//!
//! テスト用の共通データとヘルパー関数

use serde_json::{json, Value};

/// テスト用のサンプル設定データ
pub fn sample_config() -> Value {
    json!({
        "server": {
            "bind_addr": "127.0.0.1:8080",
            "timeout": 30
        },
        "plugins": {
            "search_paths": ["./test_plugins"],
            "auto_load": true
        }
    })
}

/// テスト用のセッションデータ
pub fn sample_session_data() -> Value {
    json!({
        "user_id": "test_user_123",
        "session_type": "testing",
        "metadata": {
            "test_run": true,
            "environment": "test"
        }
    })
}

/// テスト用のポリシーデータ
pub fn sample_policy_data() -> Value {
    json!({
        "id": "test-policy-001",
        "name": "Test Policy",
        "version": "1.0.0",
        "security": {
            "enabled": true,
            "encryption": {
                "enabled": true,
                "algorithm": "AES-256-GCM"
            }
        }
    })
}

/// テスト用のWordPressデータ
pub fn sample_wordpress_post() -> Value {
    json!({
        "title": "Test Post",
        "content": "This is a test post content.",
        "status": "publish",
        "author": 1,
        "categories": [1, 2],
        "tags": ["test", "sample"]
    })
}
