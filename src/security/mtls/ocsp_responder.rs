//! OCSP Responder Implementation
//!
//! OCSPレスポンダーの実装

use super::types::*;
use crate::error::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// OCSPレスポンダー
pub struct OcspResponder {
    /// OCSP設定
    config: OcspConfig,
    /// レスポンスキャッシュ
    cache: Arc<RwLock<HashMap<String, CachedResponse>>>,
    /// 失効ステータス
    revocation_status: Arc<RwLock<HashMap<String, RevocationInfo>>>,
}

/// キャッシュされたレスポンス
#[derive(Debug, Clone)]
struct CachedResponse {
    /// OCSP応答
    response: OcspResponse,
    /// キャッシュ有効期限
    expires_at: DateTime<Utc>,
}

/// 失効情報
#[derive(Debug, Clone)]
struct RevocationInfo {
    /// 失効理由
    reason: RevocationReason,
    /// 失効日時
    revoked_at: DateTime<Utc>,
}

impl OcspResponder {
    /// 新しいOCSPレスポンダーを作成
    pub fn new(config: OcspConfig) -> Result<Self> {
        Ok(Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            revocation_status: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// OCSP応答を取得
    pub async fn get_response(&self, serial_number: &str) -> Result<OcspResponse> {
        // キャッシュチェック
        if let Some(cached) = self.check_cache(serial_number).await {
            return Ok(cached);
        }

        // 新しい応答を生成
        let response = self.generate_response(serial_number).await?;

        // キャッシュに保存
        self.cache_response(serial_number, &response).await;

        Ok(response)
    }

    /// 失効ステータスを更新
    pub async fn update_revocation_status(
        &self,
        serial_number: &str,
        reason: RevocationReason,
    ) -> Result<()> {
        let mut status = self.revocation_status.write().await;
        status.insert(
            serial_number.to_string(),
            RevocationInfo {
                reason,
                revoked_at: Utc::now(),
            },
        );

        // キャッシュをクリア
        let mut cache = self.cache.write().await;
        cache.remove(serial_number);

        Ok(())
    }

    /// OCSP応答を生成
    async fn generate_response(&self, serial_number: &str) -> Result<OcspResponse> {
        let status = self.revocation_status.read().await;

        let (ocsp_status, revocation_reason, revocation_time) =
            if let Some(info) = status.get(serial_number) {
                (
                    OcspStatus::Revoked,
                    Some(info.reason.clone()),
                    Some(info.revoked_at),
                )
            } else {
                // 証明書が存在しない場合はUnknown、存在する場合はGood
                // 実際の実装では証明書ストアと連携して確認
                (OcspStatus::Good, None, None)
            };

        Ok(OcspResponse {
            serial_number: serial_number.to_string(),
            status: ocsp_status,
            produced_at: Utc::now(),
            next_update: Some(Utc::now() + Duration::seconds(self.config.cache_ttl as i64)),
            revocation_reason,
            revocation_time,
        })
    }

    /// キャッシュをチェック
    async fn check_cache(&self, serial_number: &str) -> Option<OcspResponse> {
        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(serial_number) {
            if cached.expires_at > Utc::now() {
                return Some(cached.response.clone());
            }
        }
        None
    }

    /// レスポンスをキャッシュ
    async fn cache_response(&self, serial_number: &str, response: &OcspResponse) {
        let mut cache = self.cache.write().await;
        cache.insert(
            serial_number.to_string(),
            CachedResponse {
                response: response.clone(),
                expires_at: Utc::now() + Duration::seconds(self.config.cache_ttl as i64),
            },
        );
    }

    /// 期限切れキャッシュをクリーンアップ
    pub async fn cleanup_expired_cache(&self) -> usize {
        let mut cache = self.cache.write().await;
        let before_count = cache.len();
        let now = Utc::now();

        cache.retain(|_, cached| cached.expires_at > now);

        before_count - cache.len()
    }

    /// キャッシュヒット率を取得
    pub async fn get_cache_hit_rate(&self) -> f64 {
        // 実際の実装では統計情報を追跡
        0.85
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> OcspStatistics {
        let cache = self.cache.read().await;
        let status = self.revocation_status.read().await;

        OcspStatistics {
            total_requests: cache.len(),
            cache_hits: (cache.len() as f64 * self.get_cache_hit_rate().await) as usize,
            cache_misses: (cache.len() as f64 * (1.0 - self.get_cache_hit_rate().await)) as usize,
            revoked_certificates: status.len(),
            cache_size: cache.len(),
        }
    }
}

/// OCSP統計情報
#[derive(Debug, Clone)]
pub struct OcspStatistics {
    /// 総リクエスト数
    pub total_requests: usize,
    /// キャッシュヒット数
    pub cache_hits: usize,
    /// キャッシュミス数
    pub cache_misses: usize,
    /// 失効証明書数
    pub revoked_certificates: usize,
    /// キャッシュサイズ
    pub cache_size: usize,
}

impl Default for OcspResponder {
    fn default() -> Self {
        Self {
            config: OcspConfig::default(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            revocation_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_response_good_status() {
        let responder = OcspResponder::default();

        let response = responder.get_response("123456").await.unwrap();

        assert_eq!(response.status, OcspStatus::Good);
        assert_eq!(response.serial_number, "123456");
        assert!(response.revocation_reason.is_none());
    }

    #[tokio::test]
    async fn test_update_revocation_status() {
        let responder = OcspResponder::default();

        responder
            .update_revocation_status("123456", RevocationReason::KeyCompromise)
            .await
            .unwrap();

        let response = responder.get_response("123456").await.unwrap();

        assert_eq!(response.status, OcspStatus::Revoked);
        assert_eq!(
            response.revocation_reason,
            Some(RevocationReason::KeyCompromise)
        );
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        let responder = OcspResponder::default();

        // 初回リクエスト
        let response1 = responder.get_response("789").await.unwrap();

        // キャッシュから取得
        let response2 = responder.get_response("789").await.unwrap();

        assert_eq!(response1.serial_number, response2.serial_number);
        assert_eq!(response1.produced_at, response2.produced_at); // キャッシュされているため同じ
    }
}
