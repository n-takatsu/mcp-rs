//! Consent Manager
//!
//! GDPR/CCPA準拠の同意管理システム

use super::types::*;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 同意管理システム
pub struct ConsentManager {
    /// 同意記録ストレージ（subject_id -> 同意リスト）
    consents: Arc<RwLock<HashMap<String, Vec<ConsentRecord>>>>,
    /// 同意バージョン管理
    versions: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl ConsentManager {
    /// 新しい同意管理システムを作成
    pub fn new() -> Self {
        Self {
            consents: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 同意を記録
    pub async fn grant_consent(&self, consent: ConsentRecord) -> Result<String> {
        let subject_id = consent.subject_id.clone();
        let consent_id = consent.id.clone();

        // 同意を保存
        {
            let mut consents = self.consents.write().await;
            consents
                .entry(subject_id.clone())
                .or_insert_with(Vec::new)
                .push(consent.clone());
        }

        // バージョンを記録
        {
            let mut versions = self.versions.write().await;
            versions
                .entry(consent.purpose.clone())
                .or_insert_with(Vec::new)
                .push(consent.version.clone());
        }

        Ok(consent_id)
    }

    /// 同意を撤回
    pub async fn revoke_consent(&self, subject_id: &str, purpose: &str) -> Result<()> {
        let mut consents = self.consents.write().await;

        if let Some(consent_list) = consents.get_mut(subject_id) {
            for consent in consent_list.iter_mut() {
                if consent.purpose == purpose && consent.is_valid() {
                    consent.revoke();
                }
            }
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "No consents found for subject: {}",
                subject_id
            )))
        }
    }

    /// 同意状態を確認
    pub async fn check_consent(&self, subject_id: &str, purpose: &str) -> Result<bool> {
        let consents = self.consents.read().await;

        if let Some(consent_list) = consents.get(subject_id) {
            Ok(consent_list
                .iter()
                .any(|c| c.purpose == purpose && c.is_valid()))
        } else {
            Ok(false)
        }
    }

    /// 全ての同意を取得
    pub async fn get_consents(&self, subject_id: &str) -> Result<Vec<ConsentRecord>> {
        let consents = self.consents.read().await;

        consents.get(subject_id).cloned().ok_or_else(|| {
            Error::NotFound(format!("No consents found for subject: {}", subject_id))
        })
    }

    /// 同意履歴を取得
    pub async fn get_consent_history(
        &self,
        subject_id: &str,
        purpose: &str,
    ) -> Result<Vec<ConsentRecord>> {
        let consents = self.consents.read().await;

        if let Some(consent_list) = consents.get(subject_id) {
            Ok(consent_list
                .iter()
                .filter(|c| c.purpose == purpose)
                .cloned()
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// 同意統計を取得
    pub async fn get_consent_statistics(&self) -> ConsentStatistics {
        let consents = self.consents.read().await;

        let mut total_subjects = 0;
        let mut total_consents = 0;
        let mut active_consents = 0;
        let mut revoked_consents = 0;
        let mut consents_by_purpose: HashMap<String, usize> = HashMap::new();

        for consent_list in consents.values() {
            total_subjects += 1;
            total_consents += consent_list.len();

            for consent in consent_list {
                if consent.is_valid() {
                    active_consents += 1;
                } else {
                    revoked_consents += 1;
                }

                *consents_by_purpose
                    .entry(consent.purpose.clone())
                    .or_insert(0) += 1;
            }
        }

        ConsentStatistics {
            total_subjects,
            total_consents,
            active_consents,
            revoked_consents,
            consents_by_purpose,
        }
    }
}

/// 同意統計
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsentStatistics {
    /// 総データ主体数
    pub total_subjects: usize,
    /// 総同意数
    pub total_consents: usize,
    /// 有効な同意数
    pub active_consents: usize,
    /// 撤回された同意数
    pub revoked_consents: usize,
    /// 目的別同意数
    pub consents_by_purpose: HashMap<String, usize>,
}

impl Default for ConsentManager {
    fn default() -> Self {
        Self::new()
    }
}
