//! マイクロセグメンテーション

use crate::zero_trust::{AccessRequest, TrustScore, VerificationResult, MAX_TRUST_SCORE};
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
        self.resource_patterns
            .iter()
            .any(|pattern| pattern_matches(pattern, resource))
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

/// リソースのパターン: `*`はすべて、`…*`は前方一致、ほかは完全一致。
/// ポリシーのリソースと、セグメントに含めるリソースで、同じ書き方を使う
fn pattern_matches(pattern: &str, resource: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => resource.starts_with(prefix),
        None => resource == pattern,
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
///
/// 範囲は分の単位で、`end`の分の終わりまでを含む（`23:59`なら23:59:59まで）。
/// `now`の秒以下を切り捨ててから比べる。
fn is_within_time_window(
    now: chrono::NaiveTime,
    start: chrono::NaiveTime,
    end: chrono::NaiveTime,
) -> bool {
    use chrono::Timelike;
    let now = chrono::NaiveTime::from_hms_opt(now.hour(), now.minute(), 0).unwrap_or(now);
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
    /// 含まれるリソース。ポリシーのリソースと同じ書き方（`*`はすべて、
    /// `…*`は前方一致、ほかは完全一致）
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

    /// アクセスリクエストを評価。`trust_score`は0〜100（[`MAX_TRUST_SCORE`]）。
    /// 超える値は、どのポリシーにも当てずに断る（#768。呼ぶ側の計算の
    /// 誤りで、最小スコアの確かめをすり抜けないように）
    pub fn evaluate_access(
        &self,
        request: &AccessRequest,
        user_roles: &HashSet<String>,
        trust_score: TrustScore,
    ) -> VerificationResult {
        if trust_score > MAX_TRUST_SCORE {
            return VerificationResult::failure(format!(
                "Trust score {trust_score} is out of range (0-{MAX_TRUST_SCORE})"
            ));
        }

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
            // セグメントのリソースも、パターンとして比べる（#768。以前は完全
            // 一致で、`/api/database/*`と書いたセグメントに、どのリソースも
            // 入らなかった）
            if segment
                .resources
                .iter()
                .any(|pattern| pattern_matches(pattern, &request.resource))
            {
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
    ///
    /// - `user`（スコア30以上）: `/api/public/*`を読める
    /// - `admin`（スコア70以上）: すべてのリソースを読める・書ける・消せる
    /// - `db_admin`（スコア80以上）: データベースの表（`DatabaseHandler`が
    ///   確かめるリソース`database:<エンジンのID>:<表>`）と`/api/database/*`を
    ///   読める・書ける
    ///
    /// #768より前は、`AccessPolicy::new`の引数の順（名前、リソース、スコア）を
    /// 取り違えていて、`user`のポリシーのリソースが`*`になり（スコア30以上の
    /// `user`が、すべてのリソースを読めた）、`db_admin`のポリシーのリソースが
    /// `database`との完全一致になっていた（どのリソースにも当たらなかった）
    pub fn setup_default_policies(&mut self) {
        // 読み取り専用アクセス
        self.add_global_policy(
            AccessPolicy::new("public-read", "/api/public/*", 30)
                .with_action("read")
                .with_role("user"),
        );

        // 管理者フルアクセス
        self.add_global_policy(
            AccessPolicy::new("admin-full-access", "*", 70)
                .with_action("read")
                .with_action("write")
                .with_action("delete")
                .with_role("admin"),
        );

        // データベースアクセス
        let mut db_segment = ResourceSegment::new("database");
        let mut policy = AccessPolicy::new("database-admin", "database:*", 80)
            .with_action("read")
            .with_action("write")
            .with_role("db_admin");
        policy.resource_patterns.push("/api/database/*".to_string());
        for resource in &policy.resource_patterns {
            db_segment.add_resource(resource.clone());
        }
        db_segment.policies.push(policy);
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

    fn request_for(resource: &str, action: &str) -> AccessRequest {
        AccessRequest::new("user123", "device456", None, resource, action)
    }

    fn roles_of(roles: &[&str]) -> HashSet<String> {
        roles.iter().map(|role| role.to_string()).collect()
    }

    /// 既定の`user`のポリシーは、`/api/public/*`だけを読める（#768。以前は
    /// 引数の順を取り違え、すべてのリソースを読めた）
    #[test]
    fn default_user_policy_only_reads_public_resources() {
        let engine = MicroSegmentation::default();
        let user = roles_of(&["user"]);
        assert!(
            engine
                .evaluate_access(&request_for("/api/public/info", "read"), &user, 30)
                .success
        );
        for (resource, action) in [
            ("/api/admin/settings", "read"),
            ("database:main:orders", "read"),
            ("/api/public/info", "write"),
        ] {
            assert!(
                !engine
                    .evaluate_access(&request_for(resource, action), &user, 100)
                    .success,
                "{resource} {action}"
            );
        }
        // スコアが足りなければ、読めない
        assert!(
            !engine
                .evaluate_access(&request_for("/api/public/info", "read"), &user, 29)
                .success
        );
    }

    /// 既定の`db_admin`のポリシーは、`DatabaseHandler`が確かめるリソース
    /// （`database:<エンジンのID>:<表>`）に当たる（#768。以前は、セグメントの
    /// 所属もポリシーのリソースも完全一致で、どれにも当たらなかった）
    #[test]
    fn default_database_segment_matches_handler_resources() {
        let engine = MicroSegmentation::default();
        let db_admin = roles_of(&["db_admin"]);
        for (resource, action) in [
            ("database:main:orders", "read"),
            ("database:postgresql:public.users", "write"),
            ("/api/database/tables", "read"),
        ] {
            let result = engine.evaluate_access(&request_for(resource, action), &db_admin, 80);
            assert!(result.success, "{resource} {action}: {}", result.reason);
            assert_eq!(result.reason, "Segment database policy match");
        }
        // 消す許可は無い。スコア・ロールが足りなければ断る。ほかのリソースは断る
        assert!(
            !engine
                .evaluate_access(
                    &request_for("database:main:orders", "delete"),
                    &db_admin,
                    100
                )
                .success
        );
        assert!(
            !engine
                .evaluate_access(&request_for("database:main:orders", "read"), &db_admin, 79)
                .success
        );
        assert!(
            !engine
                .evaluate_access(
                    &request_for("database:main:orders", "read"),
                    &roles_of(&["user"]),
                    100
                )
                .success
        );
        assert!(
            !engine
                .evaluate_access(&request_for("/api/admin/settings", "read"), &db_admin, 100)
                .success
        );
    }

    /// セグメントのリソースは、パターンとして比べる（#768）
    #[test]
    fn segment_resources_are_patterns() {
        let mut segment = ResourceSegment::new("reports");
        segment.add_resource("/reports/*");
        segment.add_resource("/summary");
        segment
            .policies
            .push(AccessPolicy::new("any", "*", 0).with_action("read"));
        let mut engine = MicroSegmentation::new();
        engine.add_segment(segment);
        let nobody = HashSet::new();
        for resource in ["/reports/2026", "/reports/", "/summary"] {
            assert!(
                engine
                    .evaluate_access(&request_for(resource, "read"), &nobody, 0)
                    .success,
                "{resource}"
            );
        }
        for resource in ["/report", "/summary/2026", "/other"] {
            assert!(
                !engine
                    .evaluate_access(&request_for(resource, "read"), &nobody, 0)
                    .success,
                "{resource}"
            );
        }
    }

    /// 0〜100の外のスコアは、断る（#768）
    #[test]
    fn trust_score_above_the_maximum_is_refused() {
        let mut engine = MicroSegmentation::new();
        engine.add_global_policy(AccessPolicy::new("any", "*", 0));
        let nobody = HashSet::new();
        let request = request_for("/anything", "read");
        assert!(engine.evaluate_access(&request, &nobody, 100).success);
        let refused = engine.evaluate_access(&request, &nobody, 101);
        assert!(!refused.success);
        assert!(
            refused.reason.contains("out of range"),
            "{}",
            refused.reason
        );
        assert!(!engine.evaluate_access(&request, &nobody, 255).success);
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
    fn is_within_time_window_includes_the_whole_end_minute() {
        let start = chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let end = chrono::NaiveTime::from_hms_opt(23, 59, 0).unwrap();
        let last_second = chrono::NaiveTime::from_hms_milli_opt(23, 59, 59, 999).unwrap();
        assert!(is_within_time_window(last_second, start, end));

        // 日をまたぐ範囲でも、`end`の分の終わりまでを含み、次の分は含まない。
        let start = chrono::NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        let end = chrono::NaiveTime::from_hms_opt(6, 0, 0).unwrap();
        let end_minute = chrono::NaiveTime::from_hms_opt(6, 0, 30).unwrap();
        let next_minute = chrono::NaiveTime::from_hms_opt(6, 1, 0).unwrap();
        assert!(is_within_time_window(end_minute, start, end));
        assert!(!is_within_time_window(next_minute, start, end));
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
