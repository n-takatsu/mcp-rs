//! Mock Services
//!
//! テスト用のモックサービスとユーティリティ

use std::sync::Arc;
use tokio::sync::Mutex;

/// モックHTTPクライアント
pub struct MockHttpClient {
    responses: Arc<Mutex<Vec<MockResponse>>>,
}

pub struct MockResponse {
    pub status_code: u16,
    pub body: String,
    pub headers: std::collections::HashMap<String, String>,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_response(&self, response: MockResponse) {
        self.responses.lock().await.push(response);
    }

    pub async fn get_next_response(&self) -> Option<MockResponse> {
        self.responses.lock().await.pop()
    }
}

impl Default for MockHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// テスト用のセットアップヘルパー
pub async fn setup_test_environment() -> Result<(), Box<dyn std::error::Error>> {
    // 環境変数やテスト用ディレクトリのセットアップ
    std::env::set_var("TEST_MODE", "true");
    Ok(())
}

/// テスト用のクリーンアップヘルパー
pub async fn cleanup_test_environment() -> Result<(), Box<dyn std::error::Error>> {
    // テスト後のクリーンアップ
    std::env::remove_var("TEST_MODE");
    Ok(())
}
