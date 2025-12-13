//! 継続的認証

use crate::zero_trust::{TrustScore, VerificationResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// 認証イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthEvent {
    /// イベントタイプ
    pub event_type: AuthEventType,
    /// タイムスタンプ
    pub timestamp: SystemTime,
    /// リスクスコア
    pub risk_score: TrustScore,
    /// 詳細情報
    pub details: String,
}

/// 認証イベントタイプ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthEventType {
    /// ログイン
    Login,
    /// 再認証
    Reauthentication,
    /// デバイス変更
    DeviceChange,
    /// 場所変更
    LocationChange,
    /// 異常行動検知
    AnomalousActivity,
    /// タイムアウト
    SessionTimeout,
    /// 手動検証
    ManualVerification,
}

/// セッション情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// セッションID
    pub session_id: String,
    /// ユーザーID
    pub user_id: String,
    /// デバイスID
    pub device_id: String,
    /// 初回認証時刻
    pub initial_auth_time: SystemTime,
    /// 最後の認証時刻
    pub last_auth_time: SystemTime,
    /// 最後の活動時刻
    pub last_activity_time: SystemTime,
    /// 現在のトラストスコア
    pub current_trust_score: TrustScore,
    /// 認証イベント履歴
    pub auth_events: Vec<AuthEvent>,
    /// リスクレベル
    pub risk_level: RiskLevel,
}

impl SessionInfo {
    /// 新しいセッションを作成
    pub fn new(
        session_id: impl Into<String>,
        user_id: impl Into<String>,
        device_id: impl Into<String>,
        initial_trust_score: TrustScore,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            session_id: session_id.into(),
            user_id: user_id.into(),
            device_id: device_id.into(),
            initial_auth_time: now,
            last_auth_time: now,
            last_activity_time: now,
            current_trust_score: initial_trust_score,
            auth_events: vec![AuthEvent {
                event_type: AuthEventType::Login,
                timestamp: now,
                risk_score: 0,
                details: "Initial authentication".to_string(),
            }],
            risk_level: RiskLevel::Low,
        }
    }

    /// セッションの有効期限チェック
    pub fn is_expired(&self, max_idle_duration: Duration) -> bool {
        match self.last_activity_time.elapsed() {
            Ok(elapsed) => elapsed > max_idle_duration,
            Err(_) => true,
        }
    }

    /// アクティビティを記録
    pub fn record_activity(&mut self) {
        self.last_activity_time = SystemTime::now();
    }

    /// 認証イベントを追加
    pub fn add_auth_event(&mut self, event: AuthEvent) {
        self.last_auth_time = event.timestamp;
        self.auth_events.push(event);
    }
}

/// リスクレベル
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// 低リスク
    Low,
    /// 中リスク
    Medium,
    /// 高リスク
    High,
    /// クリティカル
    Critical,
}

/// 継続的認証エンジン
pub struct ContinuousAuth {
    /// アクティブセッション
    sessions: HashMap<String, SessionInfo>,
    /// 最大アイドル時間
    max_idle_duration: Duration,
    /// 再認証間隔
    reauthentication_interval: Duration,
    /// 低リスク閾値
    low_risk_threshold: TrustScore,
    /// 中リスク閾値
    medium_risk_threshold: TrustScore,
    /// 高リスク閾値
    high_risk_threshold: TrustScore,
}

impl ContinuousAuth {
    /// 新しいエンジンを作成
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            max_idle_duration: Duration::from_secs(1800), // 30分
            reauthentication_interval: Duration::from_secs(3600), // 1時間
            low_risk_threshold: 70,
            medium_risk_threshold: 50,
            high_risk_threshold: 30,
        }
    }

    /// セッションを開始
    pub fn start_session(
        &mut self,
        session_id: impl Into<String>,
        user_id: impl Into<String>,
        device_id: impl Into<String>,
        initial_trust_score: TrustScore,
    ) {
        let session_id = session_id.into();
        let session = SessionInfo::new(session_id.clone(), user_id, device_id, initial_trust_score);
        self.sessions.insert(session_id, session);
    }

    /// セッション情報を取得
    pub fn get_session(&self, session_id: &str) -> Option<&SessionInfo> {
        self.sessions.get(session_id)
    }

    /// セッションを終了
    pub fn end_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    /// セッションアクティビティを記録
    pub fn record_activity(&mut self, session_id: &str) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.record_activity();
        }
    }

    /// セッションを検証
    pub fn verify_session(&mut self, session_id: &str) -> VerificationResult {
        let session = match self.sessions.get(session_id) {
            Some(s) => s,
            None => {
                return VerificationResult::failure("Session not found");
            }
        };

        // セッションタイムアウトチェック
        if session.is_expired(self.max_idle_duration) {
            self.end_session(session_id);
            return VerificationResult::failure("Session expired");
        }

        // 再認証が必要かチェック
        let needs_reauth = match session.last_auth_time.elapsed() {
            Ok(elapsed) => elapsed > self.reauthentication_interval,
            Err(_) => true,
        };

        if needs_reauth {
            return VerificationResult {
                success: false,
                trust_score: session.current_trust_score,
                reason: "Reauthentication required".to_string(),
                details: HashMap::new(),
            };
        }

        // トラストスコアに基づく検証
        if session.current_trust_score < self.high_risk_threshold {
            return VerificationResult::failure("Trust score too low");
        }

        VerificationResult::success(session.current_trust_score, "Session valid")
    }

    /// トラストスコアを更新
    pub fn update_trust_score(
        &mut self,
        session_id: &str,
        new_score: TrustScore,
        event_type: AuthEventType,
        details: impl Into<String>,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| "Session not found".to_string())?;

        let risk_score =
            (session.current_trust_score as i32 - new_score as i32).unsigned_abs() as TrustScore;

        session.add_auth_event(AuthEvent {
            event_type,
            timestamp: SystemTime::now(),
            risk_score,
            details: details.into(),
        });

        session.current_trust_score = new_score;

        // リスクレベルを更新
        session.risk_level = if new_score >= self.low_risk_threshold {
            RiskLevel::Low
        } else if new_score >= self.medium_risk_threshold {
            RiskLevel::Medium
        } else if new_score >= self.high_risk_threshold {
            RiskLevel::High
        } else {
            RiskLevel::Critical
        };

        Ok(())
    }

    /// 異常行動を検知した場合の処理
    pub fn handle_anomaly(
        &mut self,
        session_id: &str,
        anomaly_details: impl Into<String>,
    ) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| "Session not found".to_string())?;

        // トラストスコアを大幅に下げる
        let new_score = session.current_trust_score.saturating_sub(30);

        session.add_auth_event(AuthEvent {
            event_type: AuthEventType::AnomalousActivity,
            timestamp: SystemTime::now(),
            risk_score: 30,
            details: anomaly_details.into(),
        });

        session.current_trust_score = new_score;
        session.risk_level = RiskLevel::Critical;

        Ok(())
    }

    /// すべての期限切れセッションをクリーンアップ
    pub fn cleanup_expired_sessions(&mut self) -> usize {
        let expired_sessions: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.is_expired(self.max_idle_duration))
            .map(|(id, _)| id.clone())
            .collect();

        let count = expired_sessions.len();
        for session_id in expired_sessions {
            self.end_session(&session_id);
        }

        count
    }
}

impl Default for ContinuousAuth {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let mut auth = ContinuousAuth::new();
        auth.start_session("session1", "user1", "device1", 80);

        let session = auth.get_session("session1").unwrap();
        assert_eq!(session.user_id, "user1");
        assert_eq!(session.current_trust_score, 80);
        assert_eq!(session.risk_level, RiskLevel::Low);
    }

    #[test]
    fn test_session_verification() {
        let mut auth = ContinuousAuth::new();
        auth.start_session("session1", "user1", "device1", 80);

        let result = auth.verify_session("session1");
        assert!(result.success);
    }

    #[test]
    fn test_trust_score_update() {
        let mut auth = ContinuousAuth::new();
        auth.start_session("session1", "user1", "device1", 80);

        auth.update_trust_score(
            "session1",
            60,
            AuthEventType::LocationChange,
            "Location changed",
        )
        .unwrap();

        let session = auth.get_session("session1").unwrap();
        assert_eq!(session.current_trust_score, 60);
        assert_eq!(session.risk_level, RiskLevel::Medium);
    }

    #[test]
    fn test_anomaly_handling() {
        let mut auth = ContinuousAuth::new();
        auth.start_session("session1", "user1", "device1", 80);

        auth.handle_anomaly("session1", "Suspicious activity detected")
            .unwrap();

        let session = auth.get_session("session1").unwrap();
        assert_eq!(session.current_trust_score, 50);
        assert_eq!(session.risk_level, RiskLevel::Critical);
    }

    #[test]
    fn test_session_expiration() {
        let mut auth = ContinuousAuth::new();
        auth.max_idle_duration = Duration::from_secs(0); // 即座に期限切れ

        auth.start_session("session1", "user1", "device1", 80);
        std::thread::sleep(Duration::from_millis(10));

        let result = auth.verify_session("session1");
        assert!(!result.success);
        assert!(result.reason.contains("expired"));
    }

    #[test]
    fn test_session_activity() {
        let mut auth = ContinuousAuth::new();
        auth.start_session("session1", "user1", "device1", 80);

        let session_before = auth.get_session("session1").unwrap().clone();
        std::thread::sleep(Duration::from_millis(10));

        auth.record_activity("session1");

        let session_after = auth.get_session("session1").unwrap();
        assert!(session_after.last_activity_time > session_before.last_activity_time);
    }

    #[test]
    fn test_cleanup_expired_sessions() {
        let mut auth = ContinuousAuth::new();
        auth.max_idle_duration = Duration::from_secs(0);

        auth.start_session("session1", "user1", "device1", 80);
        auth.start_session("session2", "user2", "device2", 70);

        std::thread::sleep(Duration::from_millis(10));

        let cleaned = auth.cleanup_expired_sessions();
        assert_eq!(cleaned, 2);
        assert!(auth.get_session("session1").is_none());
        assert!(auth.get_session("session2").is_none());
    }
}
