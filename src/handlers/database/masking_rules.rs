//! マスキングルール定義
//!
//! データマスキングのルール、パターン、ポリシーを定義します。

use chrono::Datelike;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// マスキングタイプ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskingType {
    /// 完全マスク (例: `***`)
    FullMask,
    /// 部分マスク (例: `Jo***`, `****5678`)
    PartialMask {
        /// 先頭から何文字見せるか
        prefix_visible: usize,
        /// 末尾から何文字見せるか
        suffix_visible: usize,
    },
    /// ハッシュ化 (例: `a1b2c3d4`)
    HashMask {
        /// ハッシュアルゴリズム (sha256, sha512)
        algorithm: HashAlgorithm,
        /// 表示する文字数
        display_length: usize,
    },
    /// 形式保持暗号化 (例: `123-45-****`)
    FormatPreserving {
        /// 保持するフォーマットパターン
        format_pattern: String,
        /// マスク文字
        mask_char: char,
    },
    /// トークン化 (例: `TOKEN_12345`)
    TokenMask {
        /// トークンプレフィックス
        prefix: String,
    },
    /// カスタムマスキング (ユーザー定義)
    Custom {
        /// カスタムマスキング名
        name: String,
    },
}

/// カスタムマスキングトレイト
#[async_trait::async_trait]
pub trait CustomMasker: Send + Sync {
    /// カスタムマスキング名を取得
    fn name(&self) -> &str;

    /// 値をマスキング
    async fn mask(&self, value: &str, context: &MaskingContext) -> anyhow::Result<String>;
}

/// ハッシュアルゴリズム
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashAlgorithm {
    Sha256,
    Sha512,
}

/// カラムマッチングパターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnPattern {
    /// カラム名の完全一致
    pub exact_match: Option<Vec<String>>,
    /// カラム名のワイルドカードパターン (例: `*_email`, `credit_*`)
    pub wildcard_patterns: Option<Vec<String>>,
    /// 正規表現パターン
    pub regex_patterns: Option<Vec<String>>,
    /// データタイプマッチング
    pub data_types: Option<Vec<DataType>>,
}

/// データタイプ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    Date,
    DateTime,
    Json,
}

/// マスキングルール
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingRule {
    /// ルール名
    pub name: String,
    /// ルールの説明
    pub description: Option<String>,
    /// マスキングタイプ
    pub masking_type: MaskingType,
    /// カラムパターン
    pub column_pattern: ColumnPattern,
    /// 優先度 (数値が高いほど優先)
    pub priority: i32,
    /// 有効/無効
    pub enabled: bool,
}

/// 権限ベースマスキングポリシー
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingPolicy {
    /// ポリシー名
    pub name: String,
    /// 対象ロール (空の場合はすべてのロール)
    pub roles: Vec<String>,
    /// 対象パーミッション
    pub permissions: Vec<String>,
    /// 時間制約 (営業時間外はマスク強化など)
    pub time_constraints: Option<TimeConstraints>,
    /// IP/地域制約
    pub network_constraints: Option<NetworkConstraints>,
    /// 適用するルール
    pub rules: Vec<MaskingRule>,
}

/// 時間制約
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeConstraints {
    /// 許可される曜日 (0=日曜, 6=土曜)
    pub allowed_weekdays: Vec<u8>,
    /// 許可される時間範囲 (HH:MM形式)
    pub allowed_time_ranges: Vec<TimeRange>,
}

/// 時間範囲
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: String, // "09:00"
    pub end: String,   // "18:00"
}

/// ネットワーク制約
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConstraints {
    /// 許可されるIPアドレス/CIDR
    pub allowed_ips: Vec<String>,
    /// 拒否されるIPアドレス/CIDR
    pub denied_ips: Vec<String>,
    /// 許可される地域コード (ISO 3166-1 alpha-2)
    pub allowed_regions: Vec<String>,
}

/// マスキングコンテキスト
#[derive(Debug, Clone)]
pub struct MaskingContext {
    /// ユーザーロール
    pub roles: Vec<String>,
    /// ユーザーパーミッション
    pub permissions: Vec<String>,
    /// リクエスト元IP
    pub source_ip: Option<String>,
    /// リクエスト時刻
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 用途 (analysis, audit, normal)
    pub purpose: MaskingPurpose,
}

/// マスキング用途
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskingPurpose {
    /// 通常利用
    Normal,
    /// 分析用途
    Analysis,
    /// 監査用途
    Audit,
}

impl ColumnPattern {
    /// カラム名がパターンにマッチするかチェック
    pub fn matches(&self, column_name: &str) -> bool {
        // 完全一致チェック
        if let Some(ref exact) = self.exact_match {
            if exact.iter().any(|name| name == column_name) {
                return true;
            }
        }

        // ワイルドカードパターンチェック
        if let Some(ref wildcards) = self.wildcard_patterns {
            for pattern in wildcards {
                if Self::wildcard_match(pattern, column_name) {
                    return true;
                }
            }
        }

        // 正規表現パターンチェック
        if let Some(ref regexes) = self.regex_patterns {
            for pattern in regexes {
                if let Ok(re) = Regex::new(pattern) {
                    if re.is_match(column_name) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// ワイルドカードマッチング (*をサポート)
    fn wildcard_match(pattern: &str, text: &str) -> bool {
        let pattern_parts: Vec<&str> = pattern.split('*').collect();

        if pattern_parts.is_empty() {
            return false;
        }

        let mut text_pos = 0;

        for (i, part) in pattern_parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if i == 0 && !pattern.starts_with('*') {
                // 先頭パターン
                if !text.starts_with(part) {
                    return false;
                }
                text_pos = part.len();
            } else if i == pattern_parts.len() - 1 && !pattern.ends_with('*') {
                // 末尾パターン
                if !text.ends_with(part) {
                    return false;
                }
            } else {
                // 中間パターン
                if let Some(pos) = text[text_pos..].find(part) {
                    text_pos += pos + part.len();
                } else {
                    return false;
                }
            }
        }

        true
    }
}

impl MaskingPolicy {
    /// コンテキストに基づいてルールを選択
    pub fn select_rules(&self, context: &MaskingContext) -> Vec<&MaskingRule> {
        // ロールチェック
        if !self.roles.is_empty() {
            let has_role = self.roles.iter().any(|r| context.roles.contains(r));
            if !has_role {
                return vec![];
            }
        }

        // パーミッションチェック
        if !self.permissions.is_empty() {
            let has_permission = self
                .permissions
                .iter()
                .any(|p| context.permissions.contains(p));
            if !has_permission {
                return vec![];
            }
        }

        // 時間制約チェック
        if let Some(ref time_constraints) = self.time_constraints {
            if !Self::check_time_constraints(time_constraints, &context.timestamp) {
                return vec![];
            }
        }

        // ネットワーク制約チェック
        if let Some(ref network_constraints) = self.network_constraints {
            if let Some(ref ip) = context.source_ip {
                if !Self::check_network_constraints(network_constraints, ip) {
                    return vec![];
                }
            }
        }

        // 有効なルールのみ返す
        self.rules.iter().filter(|r| r.enabled).collect()
    }

    fn check_time_constraints(
        constraints: &TimeConstraints,
        timestamp: &chrono::DateTime<chrono::Utc>,
    ) -> bool {
        let weekday = timestamp.weekday().num_days_from_sunday() as u8;

        if !constraints.allowed_weekdays.contains(&weekday) {
            return false;
        }

        let time_str = timestamp.format("%H:%M").to_string();

        for range in &constraints.allowed_time_ranges {
            if time_str >= range.start && time_str <= range.end {
                return true;
            }
        }

        false
    }

    fn check_network_constraints(constraints: &NetworkConstraints, ip: &str) -> bool {
        // 拒否IPチェック
        if constraints
            .denied_ips
            .iter()
            .any(|denied| ip.starts_with(denied))
        {
            return false;
        }

        // 許可IPチェック (空の場合はすべて許可)
        if constraints.allowed_ips.is_empty() {
            return true;
        }

        constraints
            .allowed_ips
            .iter()
            .any(|allowed| ip.starts_with(allowed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_match() {
        assert!(ColumnPattern::wildcard_match("*_email", "user_email"));
        assert!(ColumnPattern::wildcard_match("credit_*", "credit_card"));
        assert!(ColumnPattern::wildcard_match(
            "*password*",
            "user_password_hash"
        ));
        assert!(!ColumnPattern::wildcard_match("*_email", "username"));
    }

    #[test]
    fn test_column_pattern_matches() {
        let pattern = ColumnPattern {
            exact_match: Some(vec!["email".to_string()]),
            wildcard_patterns: Some(vec!["*_email".to_string()]),
            regex_patterns: None,
            data_types: None,
        };

        assert!(pattern.matches("email"));
        assert!(pattern.matches("user_email"));
        assert!(!pattern.matches("username"));
    }

    #[test]
    fn test_masking_policy_select_rules() {
        let context = MaskingContext {
            roles: vec!["user".to_string()],
            permissions: vec!["read".to_string()],
            source_ip: Some("192.168.1.1".to_string()),
            timestamp: chrono::Utc::now(),
            purpose: MaskingPurpose::Normal,
        };

        let policy = MaskingPolicy {
            name: "test_policy".to_string(),
            roles: vec!["user".to_string()],
            permissions: vec!["read".to_string()],
            time_constraints: None,
            network_constraints: None,
            rules: vec![MaskingRule {
                name: "test_rule".to_string(),
                description: None,
                masking_type: MaskingType::FullMask,
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["password".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 10,
                enabled: true,
            }],
        };

        let rules = policy.select_rules(&context);
        assert_eq!(rules.len(), 1);
    }
}
