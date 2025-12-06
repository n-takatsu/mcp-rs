//! データマスキングエンジン
//!
//! データベースクエリ結果に対してマスキングを適用します。

use crate::handlers::database::masking_formatters::MaskingFormatter;
use crate::handlers::database::masking_rules::{
    MaskingContext, MaskingPolicy, MaskingPurpose, MaskingRule, MaskingType,
};
use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// マスキングエンジン
pub struct DataMaskingEngine {
    /// ポリシー一覧
    policies: Arc<RwLock<Vec<MaskingPolicy>>>,
    /// フォーマッタ
    formatter: Arc<MaskingFormatter>,
    /// ルールキャッシュ (カラム名 -> ルール)
    rule_cache: Arc<RwLock<HashMap<String, Vec<MaskingRule>>>>,
    /// 監査ログ
    audit_log: Arc<RwLock<Vec<AuditEntry>>>,
}

/// 監査ログエントリ
#[derive(Debug, Clone)]
pub struct AuditEntry {
    /// タイムスタンプ
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// カラム名
    pub column_name: String,
    /// マスキングタイプ
    pub masking_type: MaskingType,
    /// 適用されたルール名
    pub rule_name: String,
    /// ユーザーロール
    pub user_roles: Vec<String>,
    /// アンマスク要求フラグ
    pub unmask_requested: bool,
}

impl DataMaskingEngine {
    /// 新しいエンジンを作成
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
            formatter: Arc::new(MaskingFormatter::new()),
            rule_cache: Arc::new(RwLock::new(HashMap::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// ポリシーを追加
    pub async fn add_policy(&self, policy: MaskingPolicy) -> Result<()> {
        let mut policies = self.policies.write().await;
        policies.push(policy);
        
        // キャッシュをクリア
        let mut cache = self.rule_cache.write().await;
        cache.clear();
        
        Ok(())
    }

    /// ポリシーを読み込み
    pub async fn load_policies(&self, policies: Vec<MaskingPolicy>) -> Result<()> {
        let mut policy_store = self.policies.write().await;
        *policy_store = policies;
        
        // キャッシュをクリア
        let mut cache = self.rule_cache.write().await;
        cache.clear();
        
        Ok(())
    }

    /// クエリ結果をマスキング
    pub fn mask_query_result<'a>(
        &'a self,
        data: &'a mut JsonValue,
        context: &'a MaskingContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            match data {
                JsonValue::Object(ref mut map) => {
                    self.mask_object(map, context).await?;
                }
                JsonValue::Array(ref mut arr) => {
                    for item in arr.iter_mut() {
                        self.mask_query_result(item, context).await?;
                    }
                }
                _ => {}
            }
            Ok(())
        })
    }

    /// オブジェクトをマスキング
    fn mask_object<'a>(
        &'a self,
        map: &'a mut serde_json::Map<String, JsonValue>,
        context: &'a MaskingContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let keys: Vec<String> = map.keys().cloned().collect();

            for key in keys {
                // カラムに適用されるルールを取得
                let rules = self.get_rules_for_column(&key, context).await?;

                if let Some(rule) = rules.first() {
                    // 最も優先度の高いルールを適用
                    if let Some(value) = map.get(&key) {
                        if let JsonValue::String(s) = value {
                            let masked = self.formatter.mask(s, &rule.masking_type).await?;
                            map.insert(key.clone(), JsonValue::String(masked));

                            // 監査ログに記録
                            self.log_masking(
                                &key,
                                &rule.masking_type,
                                &rule.name,
                                &context.roles,
                                false,
                            )
                            .await;
                        }
                    }
                }

                // ネストされたオブジェクトを再帰的にマスキング
                if let Some(JsonValue::Object(ref mut nested_map)) = map.get_mut(&key) {
                    self.mask_object(nested_map, context).await?;
                }
            }

            Ok(())
        })
    }

    /// カラムに適用されるルールを取得
    async fn get_rules_for_column(
        &self,
        column_name: &str,
        context: &MaskingContext,
    ) -> Result<Vec<MaskingRule>> {
        // キャッシュをチェック
        {
            let cache = self.rule_cache.read().await;
            if let Some(rules) = cache.get(column_name) {
                return Ok(rules.clone());
            }
        }

        // ポリシーからルールを選択
        let policies = self.policies.read().await;
        let mut applicable_rules = Vec::new();

        for policy in policies.iter() {
            let policy_rules = policy.select_rules(context);
            
            for rule in policy_rules {
                if rule.column_pattern.matches(column_name) {
                    applicable_rules.push(rule.clone());
                }
            }
        }

        // 優先度でソート (高い順)
        applicable_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

        // キャッシュに保存
        let mut cache = self.rule_cache.write().await;
        cache.insert(column_name.to_string(), applicable_rules.clone());

        Ok(applicable_rules)
    }

    /// マスキング適用をログに記録
    async fn log_masking(
        &self,
        column_name: &str,
        masking_type: &MaskingType,
        rule_name: &str,
        user_roles: &[String],
        unmask_requested: bool,
    ) {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now(),
            column_name: column_name.to_string(),
            masking_type: masking_type.clone(),
            rule_name: rule_name.to_string(),
            user_roles: user_roles.to_vec(),
            unmask_requested,
        };

        let mut audit_log = self.audit_log.write().await;
        audit_log.push(entry);
    }

    /// 監査ログを取得
    pub async fn get_audit_log(&self, limit: Option<usize>) -> Vec<AuditEntry> {
        let audit_log = self.audit_log.read().await;
        
        if let Some(limit) = limit {
            audit_log.iter().rev().take(limit).cloned().collect()
        } else {
            audit_log.clone()
        }
    }

    /// 監査ログをクリア
    pub async fn clear_audit_log(&self) {
        let mut audit_log = self.audit_log.write().await;
        audit_log.clear();
    }

    /// ルールキャッシュをクリア
    pub async fn clear_cache(&self) {
        let mut cache = self.rule_cache.write().await;
        cache.clear();
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> MaskingStatistics {
        let audit_log = self.audit_log.read().await;
        let policies = self.policies.read().await;
        let cache = self.rule_cache.read().await;

        let mut masking_type_counts = HashMap::new();
        let mut column_counts = HashMap::new();

        for entry in audit_log.iter() {
            let type_name = format!("{:?}", entry.masking_type);
            *masking_type_counts.entry(type_name).or_insert(0) += 1;
            *column_counts.entry(entry.column_name.clone()).or_insert(0) += 1;
        }

        MaskingStatistics {
            total_maskings: audit_log.len(),
            policy_count: policies.len(),
            cache_size: cache.len(),
            masking_type_counts,
            column_counts,
        }
    }
}

impl Default for DataMaskingEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// マスキング統計情報
#[derive(Debug, Clone)]
pub struct MaskingStatistics {
    /// 総マスキング数
    pub total_maskings: usize,
    /// ポリシー数
    pub policy_count: usize,
    /// キャッシュサイズ
    pub cache_size: usize,
    /// マスキングタイプ別カウント
    pub masking_type_counts: HashMap<String, usize>,
    /// カラム別カウント
    pub column_counts: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::masking_rules::{ColumnPattern, MaskingRule};

    #[tokio::test]
    async fn test_mask_query_result() {
        let engine = DataMaskingEngine::new();

        // ポリシーを追加
        let policy = MaskingPolicy {
            name: "test_policy".to_string(),
            roles: vec![],
            permissions: vec![],
            time_constraints: None,
            network_constraints: None,
            rules: vec![MaskingRule {
                name: "email_rule".to_string(),
                description: None,
                masking_type: MaskingType::PartialMask {
                    prefix_visible: 1,
                    suffix_visible: 0,
                },
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["email".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 10,
                enabled: true,
            }],
        };

        engine.add_policy(policy).await.unwrap();

        // テストデータ
        let mut data = serde_json::json!({
            "id": 1,
            "email": "user@example.com",
            "name": "John Doe"
        });

        let context = MaskingContext {
            roles: vec!["user".to_string()],
            permissions: vec![],
            source_ip: None,
            timestamp: chrono::Utc::now(),
            purpose: MaskingPurpose::Normal,
        };

        engine.mask_query_result(&mut data, &context).await.unwrap();

        // emailがマスクされていることを確認
        if let JsonValue::Object(map) = &data {
            if let Some(JsonValue::String(email)) = map.get("email") {
                assert!(email.starts_with('u'));
                assert!(email.contains('*'));
            }
        }
    }

    #[tokio::test]
    async fn test_audit_log() {
        let engine = DataMaskingEngine::new();

        let policy = MaskingPolicy {
            name: "audit_test".to_string(),
            roles: vec![],
            permissions: vec![],
            time_constraints: None,
            network_constraints: None,
            rules: vec![MaskingRule {
                name: "password_rule".to_string(),
                description: None,
                masking_type: MaskingType::FullMask,
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["password".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 100,
                enabled: true,
            }],
        };

        engine.add_policy(policy).await.unwrap();

        let mut data = serde_json::json!({
            "password": "secret123"
        });

        let context = MaskingContext {
            roles: vec!["admin".to_string()],
            permissions: vec![],
            source_ip: Some("192.168.1.1".to_string()),
            timestamp: chrono::Utc::now(),
            purpose: MaskingPurpose::Normal,
        };

        engine.mask_query_result(&mut data, &context).await.unwrap();

        // 監査ログを確認
        let log = engine.get_audit_log(Some(10)).await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].column_name, "password");
        assert_eq!(log[0].rule_name, "password_rule");
    }

    #[tokio::test]
    async fn test_statistics() {
        let engine = DataMaskingEngine::new();

        let policy = MaskingPolicy {
            name: "stats_test".to_string(),
            roles: vec![],
            permissions: vec![],
            time_constraints: None,
            network_constraints: None,
            rules: vec![MaskingRule {
                name: "test_rule".to_string(),
                description: None,
                masking_type: MaskingType::FullMask,
                column_pattern: ColumnPattern {
                    exact_match: Some(vec!["secret".to_string()]),
                    wildcard_patterns: None,
                    regex_patterns: None,
                    data_types: None,
                },
                priority: 10,
                enabled: true,
            }],
        };

        engine.add_policy(policy).await.unwrap();

        let mut data = serde_json::json!({
            "secret": "value"
        });

        let context = MaskingContext {
            roles: vec![],
            permissions: vec![],
            source_ip: None,
            timestamp: chrono::Utc::now(),
            purpose: MaskingPurpose::Normal,
        };

        engine.mask_query_result(&mut data, &context).await.unwrap();

        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_maskings, 1);
        assert_eq!(stats.policy_count, 1);
    }
}
