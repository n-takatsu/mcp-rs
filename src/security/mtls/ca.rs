//! Certificate Authority Implementation
//!
//! 証明書認証局の実装

use super::types::*;
use crate::error::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 証明書認証局
pub struct CertificateAuthority {
    /// CA設定
    config: CaConfig,
    /// 発行済みシリアル番号
    issued_serials: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    /// 失効リスト
    revocation_list: Arc<RwLock<HashMap<String, RevocationEntry>>>,
    /// シリアル番号カウンター
    serial_counter: Arc<RwLock<u64>>,
}

/// 失効エントリ
#[derive(Debug, Clone)]
struct RevocationEntry {
    /// 失効理由
    reason: RevocationReason,
    /// 失効日時
    revoked_at: DateTime<Utc>,
}

impl CertificateAuthority {
    /// 新しいCAを作成
    pub fn new(config: CaConfig) -> Result<Self> {
        Ok(Self {
            config,
            issued_serials: Arc::new(RwLock::new(HashMap::new())),
            revocation_list: Arc::new(RwLock::new(HashMap::new())),
            serial_counter: Arc::new(RwLock::new(1)),
        })
    }

    /// 証明書に署名
    pub async fn sign_certificate(&self, request: CertificateRequest) -> Result<IssuedCertificate> {
        // シリアル番号生成
        let serial_number = self.generate_serial_number().await;

        // 有効期限設定
        let not_before = Utc::now();
        let not_after = not_before + Duration::days(request.validity_days as i64);

        // サブジェクト構築
        let subject = Subject {
            common_name: request.common_name.clone(),
            organization: Some("mcp-rs".to_string()),
            organizational_unit: Some("Security".to_string()),
            country: Some("JP".to_string()),
            state: None,
            locality: None,
        };

        // 発行者情報
        let issuer = Subject {
            common_name: "mcp-rs CA".to_string(),
            organization: Some("mcp-rs".to_string()),
            organizational_unit: Some("Certificate Authority".to_string()),
            country: Some("JP".to_string()),
            state: None,
            locality: None,
        };

        // 証明書とキーペアを生成（簡略版）
        let (certificate_pem, private_key_pem) = self.generate_certificate_pair(&request)?;

        // チェーンを構築
        let chain_pem = self.build_certificate_chain().await?;

        // 発行記録
        let mut serials = self.issued_serials.write().await;
        serials.insert(serial_number.clone(), Utc::now());

        Ok(IssuedCertificate {
            serial_number,
            subject,
            issuer,
            not_before,
            not_after,
            validity_days: request.validity_days,
            subject_alt_names: request.subject_alt_names,
            key_usage: request.key_usage,
            extended_key_usage: request.extended_key_usage,
            certificate_pem,
            private_key_pem,
            chain_pem,
            issued_at: Utc::now(),
            status: CertificateStatus::Active,
        })
    }

    /// 証明書チェーンを検証
    pub async fn verify_chain(&self, cert: &Certificate) -> Result<VerificationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // 有効期限チェック
        let now = Utc::now();
        let not_expired = now >= cert.not_before && now <= cert.not_after;
        if !not_expired {
            errors.push("Certificate has expired or is not yet valid".to_string());
        }

        // 失効チェック
        let revocation_list = self.revocation_list.read().await;
        let not_revoked = !revocation_list.contains_key(&cert.serial_number);
        if !not_revoked {
            errors.push("Certificate has been revoked".to_string());
        }

        // チェーン検証（簡略版）
        let chain_valid = self.verify_certificate_chain_internal(cert).await?;
        if !chain_valid {
            errors.push("Certificate chain verification failed".to_string());
        }

        // 鍵使用法チェック
        if cert.key_usage.is_empty() {
            warnings.push("No key usage specified".to_string());
        }

        let valid = errors.is_empty();

        Ok(VerificationResult {
            valid,
            chain_valid,
            not_revoked,
            not_expired,
            errors,
            warnings,
        })
    }

    /// 証明書を失効
    pub async fn revoke_certificate(
        &self,
        serial_number: &str,
        reason: RevocationReason,
    ) -> Result<()> {
        // 発行済みチェック
        let serials = self.issued_serials.read().await;
        if !serials.contains_key(serial_number) {
            return Err(Error::NotFound(format!(
                "Certificate not found: {}",
                serial_number
            )));
        }
        drop(serials);

        // 失効リストに追加
        let mut revocation_list = self.revocation_list.write().await;
        revocation_list.insert(
            serial_number.to_string(),
            RevocationEntry {
                reason,
                revoked_at: Utc::now(),
            },
        );

        Ok(())
    }

    /// CRLを生成
    pub async fn generate_crl(&self) -> Result<String> {
        let revocation_list = self.revocation_list.read().await;

        // CRL生成（簡略版）
        let mut crl_entries = Vec::new();
        for (serial, entry) in revocation_list.iter() {
            crl_entries.push(format!(
                "Serial: {}, Revoked: {}, Reason: {:?}",
                serial, entry.revoked_at, entry.reason
            ));
        }

        Ok(format!(
            "-----BEGIN X509 CRL-----\n{}\n-----END X509 CRL-----",
            crl_entries.join("\n")
        ))
    }

    /// シリアル番号を生成
    async fn generate_serial_number(&self) -> String {
        let mut counter = self.serial_counter.write().await;
        let serial = *counter;
        *counter += 1;
        format!("{:016x}", serial)
    }

    /// 証明書ペアを生成
    fn generate_certificate_pair(&self, request: &CertificateRequest) -> Result<(String, String)> {
        // 実際の実装ではrcgenやopenssl crateを使用
        // ここでは簡略版のダミー生成

        let certificate_pem = format!(
            "-----BEGIN CERTIFICATE-----\n\
             MIICertificate for {}\n\
             Subject: CN={}\n\
             Validity: {} days\n\
             Key Usage: {:?}\n\
             Extended Key Usage: {:?}\n\
             SANs: {:?}\n\
             -----END CERTIFICATE-----",
            request.common_name,
            request.common_name,
            request.validity_days,
            request.key_usage,
            request.extended_key_usage,
            request.subject_alt_names
        );

        let private_key_pem = format!(
            "-----BEGIN PRIVATE KEY-----\n\
             PrivateKey for {}\n\
             Algorithm: {:?}\n\
             -----END PRIVATE KEY-----",
            request.common_name, self.config.key_algorithm
        );

        Ok((certificate_pem, private_key_pem))
    }

    /// 証明書チェーンを構築
    async fn build_certificate_chain(&self) -> Result<Vec<String>> {
        let mut chain = Vec::new();

        // ルート証明書
        if let Ok(root_cert) = tokio::fs::read_to_string(&self.config.root_cert_path).await {
            chain.push(root_cert);
        }

        // 中間証明書
        if let Some(ref intermediate_path) = self.config.intermediate_cert_path {
            if let Ok(intermediate_cert) = tokio::fs::read_to_string(intermediate_path).await {
                chain.push(intermediate_cert);
            }
        }

        Ok(chain)
    }

    /// 証明書チェーン検証（内部）
    async fn verify_certificate_chain_internal(&self, _cert: &Certificate) -> Result<bool> {
        // 実際の実装ではwebpkiやrustls-webpkiを使用
        // ここでは簡略版
        Ok(true)
    }

    /// 発行済み証明書数を取得
    pub async fn count_issued_certificates(&self) -> usize {
        let serials = self.issued_serials.read().await;
        serials.len()
    }

    /// 失効証明書数を取得
    pub async fn count_revoked_certificates(&self) -> usize {
        let revocation_list = self.revocation_list.read().await;
        revocation_list.len()
    }
}

impl Default for CertificateAuthority {
    fn default() -> Self {
        Self {
            config: CaConfig::default(),
            issued_serials: Arc::new(RwLock::new(HashMap::new())),
            revocation_list: Arc::new(RwLock::new(HashMap::new())),
            serial_counter: Arc::new(RwLock::new(1)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sign_certificate() {
        let ca = CertificateAuthority::default();

        let request = CertificateRequest {
            common_name: "client.example.com".to_string(),
            subject_alt_names: vec!["localhost".to_string()],
            validity_days: 90,
            key_usage: vec![KeyUsage::DigitalSignature, KeyUsage::KeyEncipherment],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
        };

        let cert = ca.sign_certificate(request).await.unwrap();

        assert_eq!(cert.subject.common_name, "client.example.com");
        assert_eq!(cert.validity_days, 90);
        assert_eq!(cert.status, CertificateStatus::Active);
    }

    #[tokio::test]
    async fn test_revoke_certificate() {
        let ca = CertificateAuthority::default();

        let request = CertificateRequest {
            common_name: "test.example.com".to_string(),
            subject_alt_names: vec![],
            validity_days: 30,
            key_usage: vec![KeyUsage::DigitalSignature],
            extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
        };

        let cert = ca.sign_certificate(request).await.unwrap();

        ca.revoke_certificate(&cert.serial_number, RevocationReason::KeyCompromise)
            .await
            .unwrap();

        assert_eq!(ca.count_revoked_certificates().await, 1);
    }
}
