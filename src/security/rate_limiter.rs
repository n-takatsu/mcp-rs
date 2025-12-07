use crate::config::RateLimitConfig;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};

/// トークンバケット方式によるレート制限
#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
    #[allow(dead_code)]
    semaphore: Arc<Semaphore>,
    /// IP-based authentication failure tracking
    ip_auth_failures: Arc<Mutex<HashMap<IpAddr, AuthFailureRecord>>>,
    /// User-based authentication failure tracking
    user_auth_failures: Arc<Mutex<HashMap<String, AuthFailureRecord>>>,
}

#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    capacity: f64,
    refill_rate: f64,
}

/// Authentication failure record for blocking
#[derive(Debug, Clone)]
struct AuthFailureRecord {
    /// Number of consecutive failures
    failure_count: u32,
    /// Time when blocked (if blocked)
    blocked_until: Option<Instant>,
    /// Time of last failure
    last_failure: Instant,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.burst_size as usize));

        Self {
            config,
            buckets: Arc::new(Mutex::new(HashMap::new())),
            semaphore,
            ip_auth_failures: Arc::new(Mutex::new(HashMap::new())),
            user_auth_failures: Arc::new(Mutex::new(HashMap::new())),
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

    /// Check if an IP is blocked due to authentication failures
    pub async fn check_ip_blocked(&self, ip: IpAddr) -> Result<(), String> {
        let mut failures = self.ip_auth_failures.lock().await;

        if let Some(record) = failures.get_mut(&ip) {
            // Check if still blocked
            if let Some(blocked_until) = record.blocked_until {
                if Instant::now() < blocked_until {
                    let remaining = blocked_until.duration_since(Instant::now()).as_secs();
                    return Err(format!(
                        "IP {} is blocked for {} more seconds due to authentication failures",
                        ip, remaining
                    ));
                } else {
                    // Block expired, reset
                    record.blocked_until = None;
                    record.failure_count = 0;
                }
            }
        }

        Ok(())
    }

    /// Check if a user is blocked due to authentication failures
    pub async fn check_user_blocked(&self, user_id: &str) -> Result<(), String> {
        let mut failures = self.user_auth_failures.lock().await;

        if let Some(record) = failures.get_mut(user_id) {
            // Check if still blocked
            if let Some(blocked_until) = record.blocked_until {
                if Instant::now() < blocked_until {
                    let remaining = blocked_until.duration_since(Instant::now()).as_secs();
                    return Err(format!(
                        "User {} is blocked for {} more seconds due to authentication failures",
                        user_id, remaining
                    ));
                } else {
                    // Block expired, reset
                    record.blocked_until = None;
                    record.failure_count = 0;
                }
            }
        }

        Ok(())
    }

    /// Record an authentication failure for an IP
    /// Returns true if the IP should be blocked
    pub async fn record_auth_failure_ip(
        &self,
        ip: IpAddr,
        max_failures: u32,
        block_duration: Duration,
    ) -> bool {
        let mut failures = self.ip_auth_failures.lock().await;

        let record = failures.entry(ip).or_insert_with(|| AuthFailureRecord {
            failure_count: 0,
            blocked_until: None,
            last_failure: Instant::now(),
        });

        record.failure_count += 1;
        record.last_failure = Instant::now();

        if record.failure_count >= max_failures {
            record.blocked_until = Some(Instant::now() + block_duration);
            true
        } else {
            false
        }
    }

    /// Record an authentication failure for a user
    /// Returns true if the user should be blocked
    pub async fn record_auth_failure_user(
        &self,
        user_id: String,
        max_failures: u32,
        block_duration: Duration,
    ) -> bool {
        let mut failures = self.user_auth_failures.lock().await;

        let record = failures
            .entry(user_id)
            .or_insert_with(|| AuthFailureRecord {
                failure_count: 0,
                blocked_until: None,
                last_failure: Instant::now(),
            });

        record.failure_count += 1;
        record.last_failure = Instant::now();

        if record.failure_count >= max_failures {
            record.blocked_until = Some(Instant::now() + block_duration);
            true
        } else {
            false
        }
    }

    /// Reset authentication failures for an IP (on successful authentication)
    pub async fn reset_auth_failures_ip(&self, ip: IpAddr) {
        let mut failures = self.ip_auth_failures.lock().await;
        failures.remove(&ip);
    }

    /// Reset authentication failures for a user (on successful authentication)
    pub async fn reset_auth_failures_user(&self, user_id: &str) {
        let mut failures = self.user_auth_failures.lock().await;
        failures.remove(user_id);
    }

    /// Get authentication failure count for an IP
    pub async fn get_auth_failure_count_ip(&self, ip: IpAddr) -> u32 {
        let failures = self.ip_auth_failures.lock().await;
        failures.get(&ip).map_or(0, |record| record.failure_count)
    }

    /// Get authentication failure count for a user
    pub async fn get_auth_failure_count_user(&self, user_id: &str) -> u32 {
        let failures = self.user_auth_failures.lock().await;
        failures
            .get(user_id)
            .map_or(0, |record| record.failure_count)
    }

    /// Clean up expired authentication failure records
    pub async fn cleanup_auth_failures(&self, expiry_duration: Duration) {
        let now = Instant::now();

        // Clean IP records
        {
            let mut failures = self.ip_auth_failures.lock().await;
            failures.retain(|_, record| {
                // Keep if recently failed or still blocked
                now.duration_since(record.last_failure) < expiry_duration
                    || record.blocked_until.is_some_and(|until| now < until)
            });
        }

        // Clean user records
        {
            let mut failures = self.user_auth_failures.lock().await;
            failures.retain(|_, record| {
                // Keep if recently failed or still blocked
                now.duration_since(record.last_failure) < expiry_duration
                    || record.blocked_until.is_some_and(|until| now < until)
            });
        }
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

#[tokio::test]
async fn test_ip_auth_failure_blocking() {
    use std::net::IpAddr;
    use std::time::Duration;

    let config = RateLimitConfig {
        requests_per_second: 10,
        burst_size: 10,
        enabled: true,
    };

    let limiter = RateLimiter::new(config);
    let ip: IpAddr = "192.168.1.1".parse().unwrap();
    let max_failures = 3;
    let block_duration = Duration::from_secs(60);

    // Should not be blocked initially
    assert!(limiter.check_ip_blocked(ip).await.is_ok());

    // Record failures
    for i in 0..max_failures {
        let should_block = limiter
            .record_auth_failure_ip(ip, max_failures, block_duration)
            .await;
        if i < max_failures - 1 {
            assert!(!should_block, "Should not block before reaching max");
        } else {
            assert!(should_block, "Should block after reaching max");
        }
    }

    // Should be blocked now
    assert!(limiter.check_ip_blocked(ip).await.is_err());

    // Reset should unblock
    limiter.reset_auth_failures_ip(ip).await;
    assert!(limiter.check_ip_blocked(ip).await.is_ok());
}

#[tokio::test]
async fn test_user_auth_failure_blocking() {
    use std::time::Duration;

    let config = RateLimitConfig {
        requests_per_second: 10,
        burst_size: 10,
        enabled: true,
    };

    let limiter = RateLimiter::new(config);
    let user_id = "test_user";
    let max_failures = 5;
    let block_duration = Duration::from_secs(300);

    // Should not be blocked initially
    assert!(limiter.check_user_blocked(user_id).await.is_ok());

    // Record failures
    for _ in 0..max_failures {
        limiter
            .record_auth_failure_user(user_id.to_string(), max_failures, block_duration)
            .await;
    }

    // Should be blocked now
    assert!(limiter.check_user_blocked(user_id).await.is_err());

    // Check failure count
    let count = limiter.get_auth_failure_count_user(user_id).await;
    assert_eq!(count, max_failures);
}

#[tokio::test]
async fn test_auth_failure_cleanup() {
    use std::net::IpAddr;
    use std::time::Duration;

    let config = RateLimitConfig {
        requests_per_second: 10,
        burst_size: 10,
        enabled: true,
    };

    let limiter = RateLimiter::new(config);
    let ip: IpAddr = "192.168.1.2".parse().unwrap();

    // Record a failure
    limiter
        .record_auth_failure_ip(ip, 5, Duration::from_secs(60))
        .await;
    assert_eq!(limiter.get_auth_failure_count_ip(ip).await, 1);

    // Cleanup with very short expiry
    limiter
        .cleanup_auth_failures(Duration::from_millis(1))
        .await;

    // Wait a bit
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Cleanup again
    limiter
        .cleanup_auth_failures(Duration::from_millis(1))
        .await;

    // Should be cleaned up
    assert!(limiter.check_ip_blocked(ip).await.is_ok());
}
