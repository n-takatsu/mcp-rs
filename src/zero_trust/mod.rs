//! Zero Trust ネットワークアクセス制御
//!
//! "決して信頼せず、常に検証する" 原則に基づいた
//! 包括的なアクセス制御システム

pub mod continuous_auth;
pub mod device_verifier;
pub mod micro_segmentation;
pub mod network_analyzer;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

/// トラストスコア（0-100）
pub type TrustScore = u8;

/// Zero Trustコンテキスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroTrustContext {
    /// デバイストラストスコア
    pub device_trust: TrustScore,
    /// ネットワークトラストスコア
    pub network_trust: TrustScore,
    /// 行動トラストスコア
    pub behavior_trust: TrustScore,
    /// 全体リスクスコア
    pub overall_risk: TrustScore,
    /// 検証タイムスタンプ
    pub verified_at: u64,
    /// コンテキスト属性
    pub attributes: HashMap<String, String>,
}

impl ZeroTrustContext {
    /// 新しいコンテキストを作成
    pub fn new() -> Self {
        Self {
            device_trust: 0,
            network_trust: 0,
            behavior_trust: 0,
            overall_risk: 100,
            verified_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            attributes: HashMap::new(),
        }
    }

    /// 全体のトラストスコアを計算
    pub fn calculate_trust_score(&mut self) {
        let total =
            (self.device_trust as u16 + self.network_trust as u16 + self.behavior_trust as u16) / 3;
        self.overall_risk = 100 - total.min(100) as u8;
    }

    /// アクセスを許可すべきか判定
    pub fn should_allow(&self, min_trust: TrustScore) -> bool {
        let avg_trust =
            (self.device_trust as u16 + self.network_trust as u16 + self.behavior_trust as u16) / 3;
        avg_trust as u8 >= min_trust
    }

    /// コンテキストが有効期限内か確認
    pub fn is_valid(&self, max_age_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.verified_at < max_age_secs
    }
}

impl Default for ZeroTrustContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 検証結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// 検証成功
    pub success: bool,
    /// トラストスコア
    pub trust_score: TrustScore,
    /// 検証理由
    pub reason: String,
    /// 追加情報
    pub details: HashMap<String, String>,
}

impl VerificationResult {
    /// 成功結果を作成
    pub fn success(trust_score: TrustScore, reason: impl Into<String>) -> Self {
        Self {
            success: true,
            trust_score,
            reason: reason.into(),
            details: HashMap::new(),
        }
    }

    /// 失敗結果を作成
    pub fn failure(reason: impl Into<String>) -> Self {
        Self {
            success: false,
            trust_score: 0,
            reason: reason.into(),
            details: HashMap::new(),
        }
    }

    /// 詳細情報を追加
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }
}

/// アクセスリクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRequest {
    /// ユーザーID
    pub user_id: String,
    /// デバイスID
    pub device_id: String,
    /// 送信元IPアドレス
    pub source_ip: IpAddr,
    /// リクエストされたリソース
    pub resource: String,
    /// リクエストされたアクション
    pub action: String,
    /// タイムスタンプ
    pub timestamp: u64,
    /// 追加属性
    pub attributes: HashMap<String, String>,
}

impl AccessRequest {
    /// 新しいアクセスリクエストを作成
    pub fn new(
        user_id: impl Into<String>,
        device_id: impl Into<String>,
        source_ip: IpAddr,
        resource: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            device_id: device_id.into(),
            source_ip,
            resource: resource.into(),
            action: action.into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            attributes: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_zero_trust_context() {
        let mut context = ZeroTrustContext::new();
        context.device_trust = 80;
        context.network_trust = 70;
        context.behavior_trust = 90;

        context.calculate_trust_score();

        assert_eq!(context.overall_risk, 20);
        assert!(context.should_allow(70));
        assert!(!context.should_allow(90));
    }

    #[test]
    fn test_verification_result() {
        let result = VerificationResult::success(85, "Device verified")
            .with_detail("os", "Linux")
            .with_detail("version", "5.15");

        assert!(result.success);
        assert_eq!(result.trust_score, 85);
        assert_eq!(result.details.get("os"), Some(&"Linux".to_string()));
    }

    #[test]
    fn test_access_request() {
        let request = AccessRequest::new(
            "user123",
            "device456",
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
            "/api/data",
            "read",
        );

        assert_eq!(request.user_id, "user123");
        assert_eq!(request.device_id, "device456");
        assert_eq!(request.resource, "/api/data");
    }

    #[test]
    fn test_context_validity() {
        let context = ZeroTrustContext::new();
        assert!(context.is_valid(300)); // 5分以内
        assert!(context.is_valid(1)); // 1秒以内
    }
}
