//! Certificate Authority Implementation
//!
//! 証明書認証局の実装 (Phase 1: 基本実装)
//!
//! NOTE: この実装はPhase 1（基本機能）です。
//! Phase 2で本格的なrcgen統合とOCSP完全実装を行います。

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
    /// CA証明書PEM（簡略版）
    ca_cert_pem: Arc<RwLock<Option<String>>>,
    /// CA秘密鍵PEM（簡略版）
    ca_key_pem: Arc<RwLock<Option<String>>>,
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
        let ca = Self {
            config,
            ca_cert_pem: Arc::new(RwLock::new(None)),
            ca_key_pem: Arc::new(RwLock::new(None)),
            issued_serials: Arc::new(RwLock::new(HashMap::new())),
            revocation_list: Arc::new(RwLock::new(HashMap::new())),
            serial_counter: Arc::new(RwLock::new(1)),
        };

        Ok(ca)
    }

    /// CA証明書と鍵をロード (Phase 1: 簡略版)
    pub async fn load_ca_certificate(&self) -> Result<()> {
        // CA証明書ファイルを読み込み
        let ca_cert_pem = tokio::fs::read_to_string(&self.config.root_cert_path)
            .await
            .map_err(|e| Error::InvalidInput(format!("Failed to load CA certificate: {}", e)))?;

        let ca_key_pem = tokio::fs::read_to_string(&self.config.root_key_path)
            .await
            .map_err(|e| Error::InvalidInput(format!("Failed to load CA key: {}", e)))?;

        // 保存
        let mut cert_guard = self.ca_cert_pem.write().await;
        *cert_guard = Some(ca_cert_pem);

        let mut key_guard = self.ca_key_pem.write().await;
        *key_guard = Some(ca_key_pem);

        Ok(())
    }

    /// 自己署名CA証明書を生成 (Phase 1: 簡略版)
    /// NOTE: Phase 2でrcgenを使用した本格実装に置き換えます
    pub async fn generate_self_signed_ca(&self) -> Result<()> {
        // Phase 1: 簡略版の証明書生成
        let ca_cert_pem = format!(
            "-----BEGIN CERTIFICATE-----\n\
             MIICertificate: mcp-rs Root CA (Phase 1 - Simplified)\n\
             Subject: CN=mcp-rs Root CA, O=mcp-rs, OU=Certificate Authority, C=JP\n\
             Issuer: CN=mcp-rs Root CA, O=mcp-rs, OU=Certificate Authority, C=JP\n\
             Valid From: {}\n\
             Valid To: {}\n\
             Key Algorithm: {:?}\n\
             Is CA: true\n\
             Key Usage: KeyCertSign, CRLSign, DigitalSignature\n\
             -----END CERTIFICATE-----",
            Utc::now().to_rfc3339(),
            (Utc::now() + Duration::days(3650)).to_rfc3339(),
            self.config.key_algorithm
        );

        let ca_key_pem = format!(
            "-----BEGIN PRIVATE KEY-----\n\
             PrivateKey for mcp-rs Root CA (Phase 1 - Simplified)\n\
             Algorithm: {:?}\n\
             -----END PRIVATE KEY-----",
            self.config.key_algorithm
        );

        // ディレクトリ作成 (Phase 1: ファイル保存前に親ディレクトリを作成)
        if let Some(parent) = std::path::Path::new(&self.config.root_cert_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Error::InvalidInput(format!("Failed to create cert directory: {}", e))
            })?;
        }
        if let Some(parent) = std::path::Path::new(&self.config.root_key_path).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Error::InvalidInput(format!("Failed to create key directory: {}", e))
            })?;
        }

        // ファイルに保存
        tokio::fs::write(&self.config.root_cert_path, &ca_cert_pem)
            .await
            .map_err(|e| Error::InvalidInput(format!("Failed to save CA certificate: {}", e)))?;

        tokio::fs::write(&self.config.root_key_path, &ca_key_pem)
            .await
            .map_err(|e| Error::InvalidInput(format!("Failed to save CA key: {}", e)))?;

        // メモリに保存
        let mut cert_guard = self.ca_cert_pem.write().await;
        *cert_guard = Some(ca_cert_pem);

        let mut key_guard = self.ca_key_pem.write().await;
        *key_guard = Some(ca_key_pem);

        Ok(())
    }

    /// 証明書に署名 (Phase 1: 簡略版)
    /// NOTE: Phase 2でrcgenを使用した本格実装に置き換えます
    pub async fn sign_certificate(&self, request: CertificateRequest) -> Result<IssuedCertificate> {
        // CA証明書が読み込まれているか確認
        let ca_cert_guard = self.ca_cert_pem.read().await;
        if ca_cert_guard.is_none() {
            return Err(Error::InvalidInput("CA certificate not loaded".to_string()));
        }
        drop(ca_cert_guard);

        // シリアル番号生成
        let serial_number = self.generate_serial_number().await;

        // 有効期限設定
        let not_before = Utc::now();
        let not_after = not_before + Duration::days(request.validity_days as i64);

        // Phase 1: 簡略版の証明書生成
        let certificate_pem = format!(
            "-----BEGIN CERTIFICATE-----\n\
             MIICertificate for {} (Phase 1 - Simplified)\n\
             Serial Number: {}\n\
             Subject: CN={}, O=mcp-rs, OU=Security, C=JP\n\
             Issuer: CN=mcp-rs CA, O=mcp-rs, OU=Certificate Authority, C=JP\n\
             Valid From: {}\n\
             Valid To: {}\n\
             Validity Days: {}\n\
             Key Usage: {:?}\n\
             Extended Key Usage: {:?}\n\
             Subject Alternative Names: {:?}\n\
             -----END CERTIFICATE-----",
            request.common_name,
            serial_number,
            request.common_name,
            not_before.to_rfc3339(),
            not_after.to_rfc3339(),
            request.validity_days,
            request.key_usage,
            request.extended_key_usage,
            request.subject_alt_names
        );

        let private_key_pem = format!(
            "-----BEGIN PRIVATE KEY-----\n\
             PrivateKey for {} (Phase 1 - Simplified)\n\
             Algorithm: {:?}\n\
             -----END PRIVATE KEY-----",
            request.common_name, self.config.key_algorithm
        );

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

        // 証明書チェーンを構築
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

    /// 証明書チェーンを検証 (Phase 1: 簡略版)
    /// NOTE: Phase 2でrustls-webpkiを使用した本格検証に置き換えます
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

        // Phase 1: 基本的な検証（PEM形式の確認のみ）
        let chain_valid = cert.certificate_pem.contains("BEGIN CERTIFICATE")
            && cert.certificate_pem.contains("END CERTIFICATE");

        if !chain_valid {
            errors.push("Invalid certificate PEM format".to_string());
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

    /// CRLを生成 (Phase 1: 簡略版)
    /// NOTE: Phase 2でX.509形式の本格CRLに置き換えます
    pub async fn generate_crl(&self) -> Result<String> {
        let revocation_list = self.revocation_list.read().await;

        // CRL Header
        let mut crl = String::from("-----BEGIN X509 CRL-----\n");
        crl.push_str("Version: 2 (Phase 1 - Simplified)\n");
        crl.push_str("Issuer: CN=mcp-rs CA, O=mcp-rs, OU=Certificate Authority, C=JP\n");
        crl.push_str(&format!("This Update: {}\n", Utc::now().to_rfc3339()));
        crl.push_str(&format!(
            "Next Update: {}\n",
            (Utc::now() + Duration::days(7)).to_rfc3339()
        ));
        crl.push_str("\nRevoked Certificates:\n");

        // 各失効証明書のエントリ
        for (serial, entry) in revocation_list.iter() {
            crl.push_str(&format!(
                "  Serial Number: {}\n  Revocation Date: {}\n  Reason: {:?}\n\n",
                serial,
                entry.revoked_at.to_rfc3339(),
                entry.reason
            ));
        }

        crl.push_str("-----END X509 CRL-----\n");

        Ok(crl)
    }

    /// シリアル番号を生成
    async fn generate_serial_number(&self) -> String {
        let mut counter = self.serial_counter.write().await;
        let serial = *counter;
        *counter += 1;
        format!("{:016x}", serial)
    }

    /// 証明書チェーンを構築 (Phase 1: 簡略版)
    async fn build_certificate_chain(&self) -> Result<Vec<String>> {
        let mut chain = Vec::new();

        // CA証明書を取得
        let ca_cert_guard = self.ca_cert_pem.read().await;
        if let Some(ca_cert) = ca_cert_guard.as_ref() {
            chain.push(ca_cert.clone());
        }
        drop(ca_cert_guard);

        // ルート証明書ファイルがあれば追加
        if let Ok(root_cert) = tokio::fs::read_to_string(&self.config.root_cert_path).await {
            if !chain.contains(&root_cert) {
                chain.push(root_cert);
            }
        }

        // 中間証明書
        if let Some(ref intermediate_path) = self.config.intermediate_cert_path {
            if let Ok(intermediate_cert) = tokio::fs::read_to_string(intermediate_path).await {
                chain.push(intermediate_cert);
            }
        }

        Ok(chain)
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
            ca_cert_pem: Arc::new(RwLock::new(None)),
            ca_key_pem: Arc::new(RwLock::new(None)),
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
    async fn test_generate_self_signed_ca() {
        let ca = CertificateAuthority::default();

        // 自己署名CA証明書の生成
        ca.generate_self_signed_ca().await.unwrap();

        // CA証明書が生成されたことを確認
        let ca_cert_guard = ca.ca_cert_pem.read().await;
        assert!(ca_cert_guard.is_some());

        let cert_pem = ca_cert_guard.as_ref().unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert_pem.contains("mcp-rs Root CA"));
    }

    #[tokio::test]
    async fn test_sign_certificate() {
        let ca = CertificateAuthority::default();

        // まずCA証明書を生成
        ca.generate_self_signed_ca().await.unwrap();

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
        assert!(!cert.certificate_pem.is_empty());
        assert!(!cert.private_key_pem.is_empty());
    }

    #[tokio::test]
    async fn test_revoke_certificate() {
        let ca = CertificateAuthority::default();

        // CA証明書を生成
        ca.generate_self_signed_ca().await.unwrap();

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
