//! Certificate Store Implementation
//!
//! 証明書ストアの実装

use super::types::*;
use crate::error::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// 証明書ストア
pub struct CertificateStore {
    /// ストア設定
    config: StoreConfig,
    /// 証明書マップ（シリアル番号 -> 証明書）
    certificates: HashMap<String, StoredCertificate>,
}

/// 保存された証明書
#[derive(Debug, Clone)]
struct StoredCertificate {
    /// 証明書
    certificate: Certificate,
    /// 猶予期間終了日
    grace_period_end: Option<DateTime<Utc>>,
    /// 保存日時
    stored_at: DateTime<Utc>,
}

impl CertificateStore {
    /// 新しいストアを作成
    pub fn new(config: StoreConfig) -> Result<Self> {
        Ok(Self {
            config,
            certificates: HashMap::new(),
        })
    }

    /// 証明書を保存
    pub async fn store_certificate(&mut self, cert: &IssuedCertificate) -> Result<()> {
        // 最大数チェック
        if self.certificates.len() >= self.config.max_certificates {
            return Err(Error::InvalidInput("Certificate store is full".to_string()));
        }

        let certificate = Certificate {
            serial_number: cert.serial_number.clone(),
            subject: cert.subject.clone(),
            issuer: cert.issuer.clone(),
            not_before: cert.not_before,
            not_after: cert.not_after,
            validity_days: cert.validity_days,
            subject_alt_names: cert.subject_alt_names.clone(),
            key_usage: cert.key_usage.clone(),
            extended_key_usage: cert.extended_key_usage.clone(),
            certificate_pem: cert.certificate_pem.clone(),
            chain_pem: cert.chain_pem.clone(),
            status: cert.status.clone(),
        };

        self.certificates.insert(
            cert.serial_number.clone(),
            StoredCertificate {
                certificate,
                grace_period_end: None,
                stored_at: Utc::now(),
            },
        );

        Ok(())
    }

    /// 証明書を取得
    pub async fn get_certificate(&self, serial_number: &str) -> Result<Certificate> {
        self.certificates
            .get(serial_number)
            .map(|stored| stored.certificate.clone())
            .ok_or_else(|| Error::NotFound(format!("Certificate not found: {}", serial_number)))
    }

    /// 証明書を失効
    pub async fn revoke_certificate(&mut self, serial_number: &str) -> Result<()> {
        if let Some(stored) = self.certificates.get_mut(serial_number) {
            stored.certificate.status = CertificateStatus::Revoked;
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "Certificate not found: {}",
                serial_number
            )))
        }
    }

    /// 猶予期間を設定
    pub async fn set_grace_period(&mut self, serial_number: &str, days: u32) -> Result<()> {
        if let Some(stored) = self.certificates.get_mut(serial_number) {
            stored.grace_period_end = Some(Utc::now() + Duration::days(days as i64));
            stored.certificate.status = CertificateStatus::GracePeriod;
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "Certificate not found: {}",
                serial_number
            )))
        }
    }

    /// 期限切れ証明書をクリーンアップ
    pub async fn cleanup_expired(&mut self) -> usize {
        let now = Utc::now();
        let before_count = self.certificates.len();

        self.certificates.retain(|_, stored| {
            // 期限切れかつ猶予期間も終了している証明書を削除
            if stored.certificate.not_after < now {
                if let Some(grace_end) = stored.grace_period_end {
                    grace_end > now
                } else {
                    false
                }
            } else {
                true
            }
        });

        before_count - self.certificates.len()
    }

    /// 証明書数を取得
    pub fn count_certificates(&self) -> usize {
        self.certificates.len()
    }

    /// アクティブ証明書数を取得
    pub fn count_active_certificates(&self) -> usize {
        self.certificates
            .values()
            .filter(|stored| stored.certificate.status == CertificateStatus::Active)
            .count()
    }

    /// 失効証明書数を取得
    pub fn count_revoked_certificates(&self) -> usize {
        self.certificates
            .values()
            .filter(|stored| stored.certificate.status == CertificateStatus::Revoked)
            .count()
    }

    /// 期限切れ間近の証明書数を取得
    pub fn count_expiring_soon(&self, days: u32) -> usize {
        let threshold = Utc::now() + Duration::days(days as i64);
        self.certificates
            .values()
            .filter(|stored| {
                stored.certificate.status == CertificateStatus::Active
                    && stored.certificate.not_after <= threshold
            })
            .count()
    }

    /// 期限切れ間近の証明書を取得
    pub async fn get_expiring_certificates(&self, days: u32) -> Vec<Certificate> {
        let threshold = Utc::now() + Duration::days(days as i64);
        self.certificates
            .values()
            .filter(|stored| {
                stored.certificate.status == CertificateStatus::Active
                    && stored.certificate.not_after <= threshold
            })
            .map(|stored| stored.certificate.clone())
            .collect()
    }

    /// Common Nameで証明書を検索
    pub async fn find_by_common_name(&self, common_name: &str) -> Vec<Certificate> {
        self.certificates
            .values()
            .filter(|stored| stored.certificate.subject.common_name == common_name)
            .map(|stored| stored.certificate.clone())
            .collect()
    }

    /// ステータスで証明書を検索
    pub async fn find_by_status(&self, status: CertificateStatus) -> Vec<Certificate> {
        self.certificates
            .values()
            .filter(|stored| stored.certificate.status == status)
            .map(|stored| stored.certificate.clone())
            .collect()
    }
}

#[allow(clippy::derivable_impls)]
impl Default for CertificateStore {
    fn default() -> Self {
        Self {
            config: StoreConfig::default(),
            certificates: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_certificate(serial: &str, cn: &str, days: i64) -> IssuedCertificate {
        IssuedCertificate {
            serial_number: serial.to_string(),
            subject: Subject {
                common_name: cn.to_string(),
                organization: None,
                organizational_unit: None,
                country: None,
                state: None,
                locality: None,
            },
            issuer: Subject {
                common_name: "Test CA".to_string(),
                organization: None,
                organizational_unit: None,
                country: None,
                state: None,
                locality: None,
            },
            not_before: Utc::now(),
            not_after: Utc::now() + Duration::days(days),
            validity_days: days as u32,
            subject_alt_names: vec![],
            key_usage: vec![],
            extended_key_usage: vec![],
            certificate_pem: String::new(),
            private_key_pem: String::new(),
            chain_pem: vec![],
            issued_at: Utc::now(),
            status: CertificateStatus::Active,
        }
    }

    #[tokio::test]
    async fn test_store_and_retrieve_certificate() {
        let mut store = CertificateStore::default();

        let cert = create_test_certificate("123456", "test.example.com", 90);
        store.store_certificate(&cert).await.unwrap();

        let retrieved = store.get_certificate("123456").await.unwrap();
        assert_eq!(retrieved.serial_number, "123456");
        assert_eq!(retrieved.subject.common_name, "test.example.com");
    }

    #[tokio::test]
    async fn test_count_expiring_soon() {
        let mut store = CertificateStore::default();

        // 7日後に期限切れ
        let cert1 = create_test_certificate("111", "soon1.example.com", 7);
        store.store_certificate(&cert1).await.unwrap();

        // 5日後に期限切れ
        let cert2 = create_test_certificate("222", "soon2.example.com", 5);
        store.store_certificate(&cert2).await.unwrap();

        // 100日後に期限切れ
        let cert3 = create_test_certificate("333", "later.example.com", 100);
        store.store_certificate(&cert3).await.unwrap();

        let expiring = store.count_expiring_soon(10);
        assert_eq!(expiring, 2);
    }

    #[tokio::test]
    async fn test_revoke_certificate() {
        let mut store = CertificateStore::default();

        let cert = create_test_certificate("123", "test.example.com", 90);
        store.store_certificate(&cert).await.unwrap();

        store.revoke_certificate("123").await.unwrap();

        let retrieved = store.get_certificate("123").await.unwrap();
        assert_eq!(retrieved.status, CertificateStatus::Revoked);
    }
}
