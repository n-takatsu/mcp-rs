//! WebSocket Rate Limiting Module
//!
//! WebSocket接続のレート制限機能を提供

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// レート制限戦略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateLimitStrategy {
    /// トークンバケット方式
    TokenBucket,
    /// リーキーバケット方式
    LeakyBucket,
    /// スライディングウィンドウ方式
    SlidingWindow,
}

/// レート制限設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// レート制限戦略
    pub strategy: RateLimitStrategy,
    /// 1秒あたりの最大リクエスト数
    pub max_requests_per_second: u32,
    /// 最大バーストサイズ
    pub max_burst: u32,
    /// スライディングウィンドウのサイズ（ミリ秒）
    pub window_size_ms: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 100,
            max_burst: 200,
            window_size_ms: 1000,
        }
    }
}

/// レート制限マネージャー
#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    limiters: Arc<Mutex<HashMap<String, LimiterState>>>,
}

impl RateLimiter {
    /// 新しいレート制限マネージャーを作成
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            limiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// リクエストを許可するかチェック
    pub async fn check_rate_limit(&self, key: &str) -> Result<bool> {
        let mut limiters = self.limiters.lock().await;

        let state = limiters
            .entry(key.to_string())
            .or_insert_with(|| LimiterState::new(&self.config));

        match self.config.strategy {
            RateLimitStrategy::TokenBucket => state.check_token_bucket(&self.config),
            RateLimitStrategy::LeakyBucket => state.check_leaky_bucket(&self.config),
            RateLimitStrategy::SlidingWindow => state.check_sliding_window(&self.config),
        }
    }

    /// グローバルレート制限をチェック（全接続共通）
    pub async fn check_global_rate_limit(&self) -> Result<bool> {
        self.check_rate_limit("__global__").await
    }

    /// 接続単位のレート制限をチェック
    pub async fn check_connection_rate_limit(&self, connection_id: &str) -> Result<bool> {
        self.check_rate_limit(&format!("conn:{}", connection_id))
            .await
    }

    /// レート制限をリセット
    pub async fn reset(&self, key: &str) {
        let mut limiters = self.limiters.lock().await;
        limiters.remove(key);
    }

    /// 全てのレート制限をリセット
    pub async fn reset_all(&self) {
        let mut limiters = self.limiters.lock().await;
        limiters.clear();
    }

    /// 統計情報を取得
    pub async fn get_stats(&self, key: &str) -> Option<LimiterStats> {
        let limiters = self.limiters.lock().await;
        limiters.get(key).map(|state| state.stats())
    }
}

/// レート制限の内部状態
#[derive(Debug)]
struct LimiterState {
    /// トークン数（TokenBucket用）
    tokens: f64,
    /// 最後のリフィル時刻（TokenBucket用）
    last_refill: Instant,
    /// キュー内のリクエスト（LeakyBucket用）
    queued_requests: Vec<Instant>,
    /// リクエスト履歴（SlidingWindow用）
    request_history: Vec<Instant>,
    /// 総リクエスト数
    total_requests: u64,
    /// 拒否されたリクエスト数
    rejected_requests: u64,
}

impl LimiterState {
    fn new(config: &RateLimitConfig) -> Self {
        Self {
            tokens: config.max_burst as f64,
            last_refill: Instant::now(),
            queued_requests: Vec::new(),
            request_history: Vec::new(),
            total_requests: 0,
            rejected_requests: 0,
        }
    }

    /// トークンバケット方式のチェック
    fn check_token_bucket(&mut self, config: &RateLimitConfig) -> Result<bool> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        // トークンをリフィル
        let refill_amount = elapsed * config.max_requests_per_second as f64;
        self.tokens = (self.tokens + refill_amount).min(config.max_burst as f64);
        self.last_refill = now;

        self.total_requests += 1;

        // トークンが1以上あればリクエストを許可
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(true)
        } else {
            self.rejected_requests += 1;
            Ok(false)
        }
    }

    /// リーキーバケット方式のチェック
    fn check_leaky_bucket(&mut self, config: &RateLimitConfig) -> Result<bool> {
        let now = Instant::now();
        let leak_interval = Duration::from_secs_f64(1.0 / config.max_requests_per_second as f64);

        // 古いリクエストをリーク（削除）
        self.queued_requests
            .retain(|&req_time| now.duration_since(req_time) < leak_interval * config.max_burst);

        self.total_requests += 1;

        // キューに空きがあればリクエストを許可
        if self.queued_requests.len() < config.max_burst as usize {
            self.queued_requests.push(now);
            Ok(true)
        } else {
            self.rejected_requests += 1;
            Ok(false)
        }
    }

    /// スライディングウィンドウ方式のチェック
    fn check_sliding_window(&mut self, config: &RateLimitConfig) -> Result<bool> {
        let now = Instant::now();
        let window = Duration::from_millis(config.window_size_ms);

        // ウィンドウ外のリクエストを削除
        self.request_history
            .retain(|&req_time| now.duration_since(req_time) < window);

        self.total_requests += 1;

        // ウィンドウ内のリクエスト数をチェック
        let requests_in_window = self.request_history.len();
        let max_in_window = (config.max_requests_per_second as f64 * config.window_size_ms as f64
            / 1000.0) as usize;

        if requests_in_window < max_in_window {
            self.request_history.push(now);
            Ok(true)
        } else {
            self.rejected_requests += 1;
            Ok(false)
        }
    }

    /// 統計情報を取得
    fn stats(&self) -> LimiterStats {
        LimiterStats {
            total_requests: self.total_requests,
            rejected_requests: self.rejected_requests,
            acceptance_rate: if self.total_requests > 0 {
                (self.total_requests - self.rejected_requests) as f64 / self.total_requests as f64
            } else {
                1.0
            },
            current_tokens: self.tokens,
            queued_requests: self.queued_requests.len(),
            window_requests: self.request_history.len(),
        }
    }
}

/// レート制限統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimiterStats {
    /// 総リクエスト数
    pub total_requests: u64,
    /// 拒否されたリクエスト数
    pub rejected_requests: u64,
    /// 許可率（0.0-1.0）
    pub acceptance_rate: f64,
    /// 現在のトークン数（TokenBucket用）
    pub current_tokens: f64,
    /// キュー内リクエスト数（LeakyBucket用）
    pub queued_requests: usize,
    /// ウィンドウ内リクエスト数（SlidingWindow用）
    pub window_requests: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 10,
            max_burst: 20,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        // 最初の20リクエスト（バーストサイズ）は許可される
        for _ in 0..20 {
            assert!(limiter.check_rate_limit("test").await.unwrap());
        }

        // 次のリクエストは拒否される（トークンがない）
        assert!(!limiter.check_rate_limit("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 100,
            max_burst: 10,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        // バーストサイズまで消費
        for _ in 0..10 {
            assert!(limiter.check_rate_limit("test").await.unwrap());
        }

        // すぐには拒否される
        assert!(!limiter.check_rate_limit("test").await.unwrap());

        // 少し待つ（トークンがリフィルされる）
        tokio::time::sleep(Duration::from_millis(50)).await;

        // リフィルされたトークンで許可される
        assert!(limiter.check_rate_limit("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_leaky_bucket() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::LeakyBucket,
            max_requests_per_second: 10,
            max_burst: 5,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        // バーストサイズまで許可される
        for _ in 0..5 {
            assert!(limiter.check_rate_limit("test").await.unwrap());
        }

        // バーストサイズを超えると拒否される
        assert!(!limiter.check_rate_limit("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_sliding_window() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::SlidingWindow,
            max_requests_per_second: 10,
            max_burst: 10,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        // ウィンドウ内の制限（10リクエスト/秒）まで許可される
        for _ in 0..10 {
            assert!(limiter.check_rate_limit("test").await.unwrap());
        }

        // 制限を超えると拒否される
        assert!(!limiter.check_rate_limit("test").await.unwrap());

        // ウィンドウが経過すると再び許可される
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(limiter.check_rate_limit("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_global_rate_limit() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 5,
            max_burst: 5,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        for _ in 0..5 {
            assert!(limiter.check_global_rate_limit().await.unwrap());
        }

        assert!(!limiter.check_global_rate_limit().await.unwrap());
    }

    #[tokio::test]
    async fn test_connection_rate_limit() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 3,
            max_burst: 3,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        // 接続1: 3リクエストまで許可
        for _ in 0..3 {
            assert!(limiter.check_connection_rate_limit("conn1").await.unwrap());
        }
        assert!(!limiter.check_connection_rate_limit("conn1").await.unwrap());

        // 接続2: 別の接続なので3リクエストまで許可
        for _ in 0..3 {
            assert!(limiter.check_connection_rate_limit("conn2").await.unwrap());
        }
        assert!(!limiter.check_connection_rate_limit("conn2").await.unwrap());
    }

    #[tokio::test]
    async fn test_reset() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 2,
            max_burst: 2,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        // 2リクエストまで許可
        for _ in 0..2 {
            assert!(limiter.check_rate_limit("test").await.unwrap());
        }
        assert!(!limiter.check_rate_limit("test").await.unwrap());

        // リセット
        limiter.reset("test").await;

        // リセット後は再び許可される
        assert!(limiter.check_rate_limit("test").await.unwrap());
    }

    #[tokio::test]
    async fn test_stats() {
        let config = RateLimitConfig {
            strategy: RateLimitStrategy::TokenBucket,
            max_requests_per_second: 5,
            max_burst: 5,
            window_size_ms: 1000,
        };

        let limiter = RateLimiter::new(config);

        // 7リクエスト（5許可、2拒否）
        for _ in 0..7 {
            let _ = limiter.check_rate_limit("test").await;
        }

        let stats = limiter.get_stats("test").await.unwrap();
        assert_eq!(stats.total_requests, 7);
        assert_eq!(stats.rejected_requests, 2);
        assert!((stats.acceptance_rate - 5.0 / 7.0).abs() < 0.001);
    }
}
