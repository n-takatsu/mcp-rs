use crate::config::RateLimitConfig;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, Semaphore};

/// トークンバケット方式によるレート制限
#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    #[allow(dead_code)]
    semaphore: Arc<Semaphore>,
}

#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    capacity: f64,
    refill_rate: f64,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.burst_size as usize));

        Self {
            config,
            buckets: Arc::new(Mutex::new(HashMap::new())),
            semaphore,
        }
    }

    /// リクエストの実行許可をチェック
    pub async fn check_rate_limit(&self, client_id: &str) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }

        // トークンバケットによるレート制限チェック
        let mut buckets = self.buckets.lock().await;
        let bucket = buckets.entry(client_id.to_string()).or_insert_with(|| {
            TokenBucket::new(
                self.config.burst_size as f64,
                self.config.requests_per_second as f64,
            )
        });

        if bucket.consume_token() {
            Ok(())
        } else {
            Err(format!(
                "Rate limit exceeded for client: {}. Max {} requests/sec",
                client_id, self.config.requests_per_second
            ))
        }
    }

    /// 設定情報を取得
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// 現在のバケット状態を取得（デバッグ用）
    pub async fn bucket_status(&self, client_id: &str) -> Option<(f64, f64)> {
        let buckets = self.buckets.lock().await;
        buckets
            .get(client_id)
            .map(|bucket| (bucket.tokens, bucket.capacity))
    }
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            last_refill: Instant::now(),
            capacity,
            refill_rate,
        }
    }

    fn refill_tokens(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();

        let tokens_to_add = elapsed * self.refill_rate;
        self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
        self.last_refill = now;
    }

    fn consume_token(&mut self) -> bool {
        self.refill_tokens();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_creation() {
        let config = RateLimitConfig {
            requests_per_second: 5,
            burst_size: 10,
            enabled: true,
        };

        let limiter = RateLimiter::new(config);
        assert_eq!(limiter.config().requests_per_second, 5);
        assert_eq!(limiter.config().burst_size, 10);
        assert!(limiter.config().enabled);
    }

    #[tokio::test]
    async fn test_rate_limit_allows_initial_requests() {
        let config = RateLimitConfig {
            requests_per_second: 2,
            burst_size: 5,
            enabled: true,
        };

        let limiter = RateLimiter::new(config);

        // 初期バーストリクエストは許可される
        for i in 0..5 {
            let result = limiter.check_rate_limit("test_client").await;
            assert!(result.is_ok(), "Request {} should be allowed", i);
        }
    }

    #[tokio::test]
    async fn test_rate_limit_blocks_excess_requests() {
        let config = RateLimitConfig {
            requests_per_second: 1,
            burst_size: 2,
            enabled: true,
        };

        let limiter = RateLimiter::new(config);

        // 最初の2つは成功
        assert!(limiter.check_rate_limit("test_client").await.is_ok());
        assert!(limiter.check_rate_limit("test_client").await.is_ok());

        // 3つ目は制限される
        let result = limiter.check_rate_limit("test_client").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Rate limit exceeded"));
    }

    #[tokio::test]
    async fn test_rate_limit_disabled() {
        let config = RateLimitConfig {
            requests_per_second: 1,
            burst_size: 1,
            enabled: false, // 無効化
        };

        let limiter = RateLimiter::new(config);

        // 無効化されている場合はすべて許可
        for _ in 0..10 {
            assert!(limiter.check_rate_limit("test_client").await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_different_clients_independent_limits() {
        let config = RateLimitConfig {
            requests_per_second: 1,
            burst_size: 1,
            enabled: true,
        };

        let limiter = RateLimiter::new(config);

        // 異なるクライアントは独立して制限
        assert!(limiter.check_rate_limit("client1").await.is_ok());
        assert!(limiter.check_rate_limit("client2").await.is_ok());

        // それぞれが制限に達する
        assert!(limiter.check_rate_limit("client1").await.is_err());
        assert!(limiter.check_rate_limit("client2").await.is_err());
    }
}
