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

/// Removes every expired entry from the map.
///
/// `is_blocked()` only ever removes the one IP it was asked about, so an IP
/// that gets blocked once and is never checked again (a common shape for
/// one-off scanner/attacker traffic) would otherwise sit in the map forever
/// after its block expires, growing unbounded over the life of the process.
/// Called from `block_temporarily`/`block_permanently`, which already hold
/// the write lock for their own insert, so this piggybacks on that lock
/// rather than taking a second one - new blocks are exactly the events that
/// would otherwise keep growing the map, so sweeping on every one of them
/// keeps the total bounded by blocking activity instead of elapsed time.
fn sweep_expired(entries: &mut HashMap<IpAddr, BlockEntry>, now: DateTime<Utc>) {
    entries.retain(|_, entry| !entry.is_expired(now));
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
    /// クリーンアップタスクを持たない、シンプルな設計）。この呼び出しは
    /// `check_ids()`のホットパスなので、ブロックされていない大多数の
    /// リクエストが書き込みロックを取って全リクエストを直列化させて
    /// しまわないよう、まず読み取りロックだけで判定し、期限切れエントリの
    /// 削除が必要な場合のみ書き込みロックを取り直す。
    pub async fn is_blocked(&self, ip: IpAddr) -> bool {
        let now = Utc::now();
        {
            let entries = self.entries.read().await;
            match entries.get(&ip) {
                Some(entry) if !entry.is_expired(now) => return true,
                Some(_) => {} // 期限切れ - 下で書き込みロックを取って削除する
                None => return false,
            }
        }

        let mut entries = self.entries.write().await;
        match entries.get(&ip) {
            Some(entry) if entry.is_expired(now) => {
                entries.remove(&ip);
                false
            }
            // 読み取りロックを解放してから書き込みロックを取るまでの間に
            // 他のタスクが再ブロックした可能性がある - その場合は最新の
            // 状態を信頼する。
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
        let mut entries = self.entries.write().await;
        entries.insert(
            ip,
            BlockEntry {
                reason,
                blocked_at: now,
                expires_at: Some(expires_at),
            },
        );
        sweep_expired(&mut entries, now);
    }

    /// IPを永続的にブロックする。
    pub async fn block_permanently(&self, ip: IpAddr, reason: String) {
        info!("Permanently blocking IP {ip}: {reason}");
        let mut entries = self.entries.write().await;
        entries.insert(
            ip,
            BlockEntry {
                reason,
                blocked_at: Utc::now(),
                expires_at: None,
            },
        );
        sweep_expired(&mut entries, Utc::now());
    }

    /// ブロックリストに現在保持しているエントリ数（期限切れも含む）。
    /// メモリ使用量の監視・テスト用。
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// `len() == 0`か。
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
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

    /// Regression test: an IP blocked once and never checked again (a
    /// one-off attacker that doesn't come back) must not sit in the map
    /// forever after it expires - the next unrelated block event must
    /// sweep it out, not just query the IP it was actually asked about.
    #[tokio::test]
    async fn expired_one_off_entries_are_swept_by_a_later_unrelated_block() {
        let blocklist = IpBlocklist::new();
        let one_off_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 50));

        blocklist
            .block_temporarily(
                one_off_ip,
                std::time::Duration::from_millis(20),
                "test".to_string(),
            )
            .await;
        assert_eq!(
            blocklist.len().await,
            1,
            "the entry exists right after insertion, before it has expired"
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(
            blocklist.len().await,
            1,
            "expiry alone doesn't remove an entry - nothing has checked or re-blocked it yet"
        );

        // A second, unrelated IP gets blocked - this must sweep the
        // already-expired first entry out, without anyone ever calling
        // is_blocked(one_off_ip) again.
        let other_ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 51));
        blocklist
            .block_temporarily(
                other_ip,
                std::time::Duration::from_secs(3600),
                "test".to_string(),
            )
            .await;

        assert_eq!(
            blocklist.len().await,
            1,
            "the expired one-off entry must be swept, leaving only the still-active block"
        );
        assert!(blocklist.is_blocked(other_ip).await);
    }
}
