//! IP Blocklist
//!
//! IDSの検知結果に基づき、IPアドレスを一時的または永続的にブロックする
//! IPS（侵入防止）の実行部分。

use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// ブロックエントリ
#[derive(Debug, Clone)]
struct BlockEntry {
    reason: String,
    blocked_at: DateTime<Utc>,
    /// `None`の場合は永続ブロック
    expires_at: Option<DateTime<Utc>>,
}

impl BlockEntry {
    fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(expires_at) => now >= expires_at,
            None => false,
        }
    }
}

/// IPアドレスのブロックリスト
///
/// エントリはメモリ上のみで保持される（プロセス再起動でリセットされる）。
/// 永続化・複数インスタンス間での共有は今回のスコープ外。
pub struct IpBlocklist {
    entries: Arc<RwLock<HashMap<IpAddr, BlockEntry>>>,
}

impl IpBlocklist {
    /// 新しいブロックリストを作成
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 指定したIPが現在ブロック中か判定する。
    ///
    /// 期限切れの一時ブロックエントリはここで遅延削除される（別途
    /// クリーンアップタスクを持たない、シンプルな設計）。
    pub async fn is_blocked(&self, ip: IpAddr) -> bool {
        let now = Utc::now();
        let mut entries = self.entries.write().await;
        match entries.get(&ip) {
            Some(entry) if entry.is_expired(now) => {
                entries.remove(&ip);
                false
            }
            Some(_) => true,
            None => false,
        }
    }

    /// 指定した期間だけIPをブロックする。
    pub async fn block_temporarily(
        &self,
        ip: IpAddr,
        duration: std::time::Duration,
        reason: String,
    ) {
        let now = Utc::now();
        let expires_at =
            now + Duration::from_std(duration).unwrap_or_else(|_| Duration::seconds(30 * 60));
        info!("Blocking IP {ip} until {expires_at}: {reason}");
        self.entries.write().await.insert(
            ip,
            BlockEntry {
                reason,
                blocked_at: now,
                expires_at: Some(expires_at),
            },
        );
    }

    /// IPを永続的にブロックする。
    pub async fn block_permanently(&self, ip: IpAddr, reason: String) {
        info!("Permanently blocking IP {ip}: {reason}");
        self.entries.write().await.insert(
            ip,
            BlockEntry {
                reason,
                blocked_at: Utc::now(),
                expires_at: None,
            },
        );
    }

    /// ブロックを解除する（誤検知時の運用対応、およびテスト用）。
    pub async fn unblock(&self, ip: IpAddr) {
        self.entries.write().await.remove(&ip);
    }
}

impl Default for IpBlocklist {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1))
    }

    #[tokio::test]
    async fn unblocked_ip_is_not_blocked() {
        let blocklist = IpBlocklist::new();
        assert!(!blocklist.is_blocked(test_ip()).await);
    }

    #[tokio::test]
    async fn temporary_block_is_active_before_expiry() {
        let blocklist = IpBlocklist::new();
        blocklist
            .block_temporarily(
                test_ip(),
                std::time::Duration::from_secs(3600),
                "test".to_string(),
            )
            .await;
        assert!(blocklist.is_blocked(test_ip()).await);
    }

    #[tokio::test]
    async fn temporary_block_expires() {
        let blocklist = IpBlocklist::new();
        // Zero-duration block is already expired the moment it's checked.
        blocklist
            .block_temporarily(
                test_ip(),
                std::time::Duration::from_secs(0),
                "test".to_string(),
            )
            .await;
        assert!(!blocklist.is_blocked(test_ip()).await);
    }

    #[tokio::test]
    async fn permanent_block_never_expires() {
        let blocklist = IpBlocklist::new();
        blocklist
            .block_permanently(test_ip(), "test".to_string())
            .await;
        assert!(blocklist.is_blocked(test_ip()).await);
    }

    #[tokio::test]
    async fn unblock_clears_a_temporary_block() {
        let blocklist = IpBlocklist::new();
        blocklist
            .block_temporarily(
                test_ip(),
                std::time::Duration::from_secs(3600),
                "test".to_string(),
            )
            .await;
        blocklist.unblock(test_ip()).await;
        assert!(!blocklist.is_blocked(test_ip()).await);
    }

    #[tokio::test]
    async fn unblock_clears_a_permanent_block() {
        let blocklist = IpBlocklist::new();
        blocklist
            .block_permanently(test_ip(), "test".to_string())
            .await;
        blocklist.unblock(test_ip()).await;
        assert!(!blocklist.is_blocked(test_ip()).await);
    }
}
