//! Zero Trust ネットワークアクセス制御
//!
//! "決して信頼せず、常に検証する" 原則に基づいた
//! 包括的なアクセス制御システム

pub mod micro_segmentation;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

/// トラストスコア（0-100）
pub type TrustScore = u8;

/// トラストスコアの上限。型は`u8`なので、これより大きい値も入れられる。
/// 計算では上限に丸め、[`micro_segmentation::MicroSegmentation::evaluate_access`]は
/// 上限を超えるスコアを断る（#768）
pub const MAX_TRUST_SCORE: TrustScore = 100;

/// 今の時刻（UNIXの秒）。時計がUNIXの始まりより前なら0
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

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
            verified_at: unix_now(),
            attributes: HashMap::new(),
        }
    }

    /// 3つのスコアの平均。それぞれ、上限（[`MAX_TRUST_SCORE`]）に丸めてから
    /// 平均する（#768。以前は丸めず、100を超える値を1つ入れると、ほかが
    /// 低くても平均が上がった）
    fn average_trust(&self) -> TrustScore {
        let capped = |score: TrustScore| u16::from(score.min(MAX_TRUST_SCORE));
        let total =
            capped(self.device_trust) + capped(self.network_trust) + capped(self.behavior_trust);
        // 3つとも100以下なので、平均も100以下
        (total / 3) as TrustScore
    }

    /// 全体のトラストスコアを計算
    pub fn calculate_trust_score(&mut self) {
        self.overall_risk = MAX_TRUST_SCORE - self.average_trust();
    }

    /// アクセスを許可すべきか判定。`min_trust`が上限（[`MAX_TRUST_SCORE`]）を
    /// 超えるなら、どのスコアでも許さない
    pub fn should_allow(&self, min_trust: TrustScore) -> bool {
        self.average_trust() >= min_trust
    }

    /// コンテキストが有効期限内か確認
    pub fn is_valid(&self, max_age_secs: u64) -> bool {
        self.is_valid_at(unix_now(), max_age_secs)
    }

    /// `now`（UNIXの秒）の時点で、有効期限内か。`verified_at`が`now`より
    /// 先（時計が戻った・値を外から入れた）なら、無効とする（#768。以前は
    /// 引き算があふれ、デバッグビルドではpanicした）
    fn is_valid_at(&self, now: u64, max_age_secs: u64) -> bool {
        now.checked_sub(self.verified_at)
            .is_some_and(|age| age < max_age_secs)
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
    /// 送信元IPアドレス（呼び出し元がIPを把握していない場合は`None` -
    /// 例えばDBハンドラ層はトランスポート層のIPを知らない）
    pub source_ip: Option<IpAddr>,
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
        source_ip: Option<IpAddr>,
        resource: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            device_id: device_id.into(),
            source_ip,
            resource: resource.into(),
            action: action.into(),
            timestamp: unix_now(),
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
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
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

        // 時刻を決めて確かめる（#768。以前は`is_valid(1)`を今の時計で確かめ、
        // 秒の境目で失敗した）
        let mut context = ZeroTrustContext::new();
        context.verified_at = 1_000;
        assert!(context.is_valid_at(1_000, 1));
        assert!(!context.is_valid_at(1_001, 1));
        assert!(context.is_valid_at(1_299, 300));
        assert!(!context.is_valid_at(1_300, 300));
        // 期限が0なら、いつでも無効
        assert!(!context.is_valid_at(1_000, 0));
    }

    /// `verified_at`が未来でも、panicせず、無効とする（#768）
    #[test]
    fn test_context_verified_in_the_future_is_not_valid() {
        let mut context = ZeroTrustContext::new();
        context.verified_at = 2_000;
        assert!(!context.is_valid_at(1_000, 300));
        context.verified_at = u64::MAX;
        assert!(!context.is_valid(300));
    }

    /// 上限（100）を超えるスコアは、上限に丸めて平均する（#768）
    #[test]
    fn test_trust_scores_above_the_maximum_are_capped() {
        let mut context = ZeroTrustContext::new();
        context.device_trust = 255;
        context.network_trust = 0;
        context.behavior_trust = 0;
        // 丸めなければ平均85。丸めると33
        assert!(!context.should_allow(50));
        assert!(context.should_allow(33));
        context.calculate_trust_score();
        assert_eq!(context.overall_risk, 67);

        context.network_trust = 255;
        context.behavior_trust = 255;
        context.calculate_trust_score();
        assert_eq!(context.overall_risk, 0);
        assert!(context.should_allow(100));
        // 上限を超える最小スコアは、満たせない
        assert!(!context.should_allow(101));
    }
}
