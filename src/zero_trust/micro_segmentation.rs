//! マイクロセグメンテーション

use crate::zero_trust::{AccessRequest, TrustScore, VerificationResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// アクセスポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPolicy {
    /// ポリシー名
    pub name: String,
    /// 対象リソース（パターンマッチング）
    pub resource_patterns: Vec<String>,
    /// 許可されたアクション
    pub allowed_actions: HashSet<String>,
    /// 必要な最小トラストスコア
    pub min_trust_score: TrustScore,
    /// 許可されたロール
    pub allowed_roles: HashSet<String>,
    /// 時間制限（開始時刻）
    pub time_start: Option<String>,
    /// 時間制限（終了時刻）
    pub time_end: Option<String>,
    /// 有効フラグ
    pub enabled: bool,
}

impl AccessPolicy {
    /// 新しいポリシーを作成
    pub fn new(
        name: impl Into<String>,
        resource_pattern: impl Into<String>,
        min_trust_score: TrustScore,
    ) -> Self {
        Self {
            name: name.into(),
            resource_patterns: vec![resource_pattern.into()],
            allowed_actions: HashSet::new(),
            min_trust_score,
            allowed_roles: HashSet::new(),
            time_start: None,
            time_end: None,
            enabled: true,
        }
    }

    /// アクションを追加
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.allowed_actions.insert(action.into());
        self
    }

    /// ロールを追加
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.allowed_roles.insert(role.into());
        self
    }

    /// リソースパターンにマッチするか判定
    pub fn matches_resource(&self, resource: &str) -> bool {
        self.resource_patterns.iter().any(|pattern| {
            if pattern == "*" {
                true
            } else if pattern.ends_with('*') {
                resource.starts_with(&pattern[..pattern.len() - 1])
            } else {
                resource == pattern
            }
        })
    }

    /// アクセスリクエストを評価
    pub fn evaluate(
        &self,
        request: &AccessRequest,
        user_roles: &HashSet<String>,
        trust_score: TrustScore,
    ) -> bool {
        if !self.enabled {
            return false;
        }

        // リソースマッチ
        if !self.matches_resource(&request.resource) {
            return false;
        }

        // アクションチェック
        if !self.allowed_actions.is_empty() && !self.allowed_actions.contains(&request.action) {
            return false;
        }

        // ロールチェック
        if !self.allowed_roles.is_empty()
            && !self.allowed_roles.iter().any(|r| user_roles.contains(r))
        {
            return false;
        }

        // トラストスコアチェック
        if trust_score < self.min_trust_score {
            return false;
        }

        // 時間制限チェック
        match (&self.time_start, &self.time_end) {
            (Some(start), Some(end)) => {
                let (Some(start), Some(end)) = (parse_hhmm(start), parse_hhmm(end)) else {
                    // 不正な時刻形式は安全側に倒して拒否
                    return false;
                };
                if !is_within_time_window(chrono::Utc::now().time(), start, end) {
                    return false;
                }
            }
            (None, None) => {}
            // 片方だけの設定は誤設定とみなし拒否
            _ => return false,
        }

        true
    }
}

/// `"HH:MM"`（24時間表記）を`chrono::NaiveTime`にパースする。フォーマットが
/// 不正な場合は`None` - 呼び出し側は安全側に倒して拒否すること。
fn parse_hhmm(s: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(s, "%H:%M").ok()
}

/// `now`が`[start, end]`の範囲内かを判定する。`start > end`の場合は日を
/// またぐ範囲（例: 22:00-06:00）として扱う。`now`を引数として受け取る
/// ことで、実際の壁時計時刻に依存せずテストできるようにしている。
fn is_within_time_window(
    now: chrono::NaiveTime,
    start: chrono::NaiveTime,
    end: chrono::NaiveTime,
) -> bool {
    if start <= end {
        now >= start && now <= end
    } else {
        now >= start || now <= end
    }
}

/// リソースセグメント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSegment {
    /// セグメント名
    pub name: String,
    /// 含まれるリソース
    pub resources: Vec<String>,
    /// セグメント間通信許可
    ///
    /// `allow_communication_from()`で設定できるが、`MicroSegmentation::
    /// evaluate_access()`のどこからも参照されない - セグメント間（例:
    /// プラグイン間）通信の許可判定自体が未実装のため。テーブル単位の
    /// アクセス制御（`DatabaseHandler`への配線）はこのフィールドを必要
    /// としないため、意図的に本フィールドの実装は対象外としている。
    pub allowed_segments: HashSet<String>,
    /// セグメントポリシー
    pub policies: Vec<AccessPolicy>,
}

impl ResourceSegment {
    /// 新しいセグメントを作成
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            resources: Vec::new(),
            allowed_segments: HashSet::new(),
            policies: Vec::new(),
        }
    }

    /// リソースを追加
    pub fn add_resource(&mut self, resource: impl Into<String>) {
        self.resources.push(resource.into());
    }

    /// 通信許可セグメントを追加
    pub fn allow_communication_from(&mut self, segment: impl Into<String>) {
        self.allowed_segments.insert(segment.into());
    }
}

/// マイクロセグメンテーションエンジン
pub struct MicroSegmentation {
    /// セグメント定義
    segments: HashMap<String, ResourceSegment>,
    /// グローバルポリシー
    global_policies: Vec<AccessPolicy>,
}

impl MicroSegmentation {
    /// 新しいエンジンを作成
    pub fn new() -> Self {
        Self {
            segments: HashMap::new(),
            global_policies: Vec::new(),
        }
    }

    /// セグメントを追加
    pub fn add_segment(&mut self, segment: ResourceSegment) {
        self.segments.insert(segment.name.clone(), segment);
    }

    /// グローバルポリシーを追加
    pub fn add_global_policy(&mut self, policy: AccessPolicy) {
        self.global_policies.push(policy);
    }

    /// アクセスリクエストを評価
    pub fn evaluate_access(
        &self,
        request: &AccessRequest,
        user_roles: &HashSet<String>,
        trust_score: TrustScore,
    ) -> VerificationResult {
        // グローバルポリシーチェック
        let global_allowed = self
            .global_policies
            .iter()
            .any(|policy| policy.evaluate(request, user_roles, trust_score));

        if global_allowed {
            return VerificationResult::success(trust_score, "Global policy match");
        }

        // セグメントポリシーチェック
        for segment in self.segments.values() {
            if segment.resources.iter().any(|r| r == &request.resource) {
                for policy in &segment.policies {
                    if policy.evaluate(request, user_roles, trust_score) {
                        return VerificationResult::success(
                            trust_score,
                            format!("Segment {} policy match", segment.name),
                        );
                    }
                }
            }
        }

        VerificationResult::failure("No matching policy found")
    }

    /// デフォルトポリシーを設定
    pub fn setup_default_policies(&mut self) {
        // 読み取り専用アクセス
        self.add_global_policy(
            AccessPolicy::new("/api/public/*", "*", 30)
                .with_action("read")
                .with_role("user"),
        );

        // 管理者フルアクセス
        self.add_global_policy(
            AccessPolicy::new("*", "*", 70)
                .with_action("read")
                .with_action("write")
                .with_action("delete")
                .with_role("admin"),
        );

        // データベースアクセス
        let mut db_segment = ResourceSegment::new("database");
        db_segment.add_resource("/api/database/*");
        db_segment.policies.push(
            AccessPolicy::new("/api/database/*", "database", 80)
                .with_action("read")
                .with_action("write")
                .with_role("db_admin"),
        );
        self.add_segment(db_segment);
    }
}

impl Default for MicroSegmentation {
    fn default() -> Self {
        let mut engine = Self::new();
        engine.setup_default_policies();
        engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_access_policy() {
        let policy = AccessPolicy::new("data-policy", "/api/data/*", 50)
            .with_action("read")
            .with_role("user");

        assert!(policy.matches_resource("/api/data/items"));
        assert!(!policy.matches_resource("/api/other"));
    }

    #[test]
    fn test_policy_evaluation() {
        let policy = AccessPolicy::new("data-policy", "/api/data/*", 50)
            .with_action("read")
            .with_role("user");

        let request = AccessRequest::new(
            "user123",
            "device456",
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            "/api/data/items",
            "read",
        );

        let mut roles = HashSet::new();
        roles.insert("user".to_string());

        assert!(policy.evaluate(&request, &roles, 60));
        assert!(!policy.evaluate(&request, &roles, 40)); // トラストスコア不足
    }

    #[test]
    fn test_micro_segmentation() {
        let engine = MicroSegmentation::default();

        let request = AccessRequest::new(
            "user123",
            "device456",
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            "/api/public/info",
            "read",
        );

        let mut roles = HashSet::new();
        roles.insert("user".to_string());

        let result = engine.evaluate_access(&request, &roles, 50);

        assert!(result.success);
    }

    #[test]
    fn test_admin_access() {
        let engine = MicroSegmentation::default();

        let request = AccessRequest::new(
            "admin123",
            "device456",
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            "/api/admin/settings",
            "write",
        );

        let mut roles = HashSet::new();
        roles.insert("admin".to_string());

        let result = engine.evaluate_access(&request, &roles, 80);

        assert!(result.success);
    }

    #[test]
    fn test_denied_access() {
        let engine = MicroSegmentation::default();

        let request = AccessRequest::new(
            "user123",
            "device456",
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            "/api/admin/settings",
            "write",
        );

        let mut roles = HashSet::new();
        roles.insert("user".to_string());

        let result = engine.evaluate_access(&request, &roles, 50);

        assert!(!result.success);
    }

    #[test]
    fn parse_hhmm_accepts_valid_times_and_rejects_invalid() {
        assert_eq!(
            parse_hhmm("09:30"),
            Some(chrono::NaiveTime::from_hms_opt(9, 30, 0).unwrap())
        );
        assert_eq!(parse_hhmm("not-a-time"), None);
        assert_eq!(parse_hhmm("25:00"), None);
    }

    #[test]
    fn is_within_time_window_same_day_range() {
        let start = chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let end = chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap();

        let inside = chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let before = chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        let after = chrono::NaiveTime::from_hms_opt(18, 0, 0).unwrap();

        assert!(is_within_time_window(inside, start, end));
        assert!(!is_within_time_window(before, start, end));
        assert!(!is_within_time_window(after, start, end));
    }

    #[test]
    fn is_within_time_window_overnight_range() {
        // 22:00-06:00 wraps past midnight.
        let start = chrono::NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        let end = chrono::NaiveTime::from_hms_opt(6, 0, 0).unwrap();

        let late_night = chrono::NaiveTime::from_hms_opt(23, 0, 0).unwrap();
        let early_morning = chrono::NaiveTime::from_hms_opt(3, 0, 0).unwrap();
        let midday = chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        assert!(is_within_time_window(late_night, start, end));
        assert!(is_within_time_window(early_morning, start, end));
        assert!(!is_within_time_window(midday, start, end));
    }

    #[test]
    fn evaluate_rejects_malformed_time_bounds() {
        let policy = AccessPolicy {
            time_start: Some("not-a-time".to_string()),
            time_end: Some("17:00".to_string()),
            ..AccessPolicy::new("time-policy", "*", 0)
        };
        let request = AccessRequest::new(
            "user123",
            "device456",
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            "/anything",
            "read",
        );
        let roles = HashSet::new();

        assert!(!policy.evaluate(&request, &roles, 0));
    }

    #[test]
    fn evaluate_rejects_one_sided_time_bound() {
        let policy = AccessPolicy {
            time_start: Some("09:00".to_string()),
            time_end: None,
            ..AccessPolicy::new("time-policy", "*", 0)
        };
        let request = AccessRequest::new(
            "user123",
            "device456",
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            "/anything",
            "read",
        );
        let roles = HashSet::new();

        assert!(!policy.evaluate(&request, &roles, 0));
    }

    #[test]
    fn evaluate_allows_full_day_window() {
        let policy = AccessPolicy {
            time_start: Some("00:00".to_string()),
            time_end: Some("23:59".to_string()),
            ..AccessPolicy::new("time-policy", "*", 0)
        };
        let request = AccessRequest::new(
            "user123",
            "device456",
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            "/anything",
            "read",
        );
        let roles = HashSet::new();

        assert!(policy.evaluate(&request, &roles, 0));
    }
}
