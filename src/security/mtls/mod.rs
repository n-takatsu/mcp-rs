//! mTLS Certificate Management System
//!
//! 相互TLS認証のための包括的な証明書管理システム

pub mod ca;
pub mod cert_store;
pub mod ocsp_responder;
pub mod rotation_scheduler;
pub mod types;

pub use ca::CertificateAuthority;
pub use cert_store::CertificateStore;
pub use ocsp_responder::OcspResponder;
pub use rotation_scheduler::RotationScheduler;
pub use types::*;

use crate::error::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

/// mTLS証明書マネージャー
pub struct CertificateManager {
    /// 証明書認証局
    ca: Arc<CertificateAuthority>,
    /// 証明書ストア
    store: Arc<RwLock<CertificateStore>>,
    /// OCSPレスポンダー
    ocsp: Arc<OcspResponder>,
    /// ローテーションスケジューラ
    scheduler: Arc<RwLock<RotationScheduler>>,
}

impl CertificateManager {
    /// 新しい証明書マネージャーを作成
    pub async fn new(config: CertificateManagerConfig) -> Result<Self> {
        let ca = Arc::new(CertificateAuthority::new(config.ca_config)?);
        let store = Arc::new(RwLock::new(CertificateStore::new(config.store_config)?));
        let ocsp = Arc::new(OcspResponder::new(config.ocsp_config)?);
        let scheduler = Arc::new(RwLock::new(RotationScheduler::new(config.rotation_config)));

        Ok(Self {
            ca,
            store,
            ocsp,
            scheduler,
        })
    }

    /// クライアント証明書を発行
    pub async fn issue_client_certificate(
        &self,
        request: CertificateRequest,
    ) -> Result<IssuedCertificate> {
        // CSR検証
        self.validate_csr(&request)?;

        // 証明書生成
        let cert = self.ca.sign_certificate(request).await?;

        // ストアに保存
        let mut store = self.store.write().await;
        store.store_certificate(&cert).await?;

        // ローテーションスケジュール登録
        let scheduler = self.scheduler.write().await;
        scheduler
            .schedule_rotation(&cert.serial_number, cert.not_after)
            .await?;

        Ok(cert)
    }

    /// 証明書チェーンを検証
    pub async fn verify_certificate_chain(&self, cert: &Certificate) -> Result<VerificationResult> {
        self.ca.verify_chain(cert).await
    }

    /// 証明書を失効
    pub async fn revoke_certificate(
        &self,
        serial_number: &str,
        reason: RevocationReason,
    ) -> Result<()> {
        // CA側で失効処理
        self.ca
            .revoke_certificate(serial_number, reason.clone())
            .await?;

        // ストアから削除
        let mut store = self.store.write().await;
        store.revoke_certificate(serial_number).await?;

        // OCSPレスポンダーを更新
        self.ocsp
            .update_revocation_status(serial_number, reason)
            .await?;

        Ok(())
    }

    /// OCSP応答を取得
    pub async fn get_ocsp_response(&self, serial_number: &str) -> Result<OcspResponse> {
        self.ocsp.get_response(serial_number).await
    }

    /// 証明書をローテーション
    pub async fn rotate_certificate(&self, serial_number: &str) -> Result<IssuedCertificate> {
        // 既存証明書を取得
        let store = self.store.read().await;
        let old_cert = store.get_certificate(serial_number).await?;
        drop(store);

        // 新しい証明書を発行
        let new_request = CertificateRequest {
            common_name: old_cert.subject.common_name.clone(),
            subject_alt_names: old_cert.subject_alt_names.clone(),
            validity_days: old_cert.validity_days,
            key_usage: old_cert.key_usage.clone(),
            extended_key_usage: old_cert.extended_key_usage.clone(),
        };

        let new_cert = self.issue_client_certificate(new_request).await?;

        // 古い証明書に猶予期間を設定
        let mut store = self.store.write().await;
        store.set_grace_period(serial_number, 30).await?;

        Ok(new_cert)
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> CertificateStatistics {
        let store = self.store.read().await;
        let scheduler = self.scheduler.read().await;
        let rotation_stats = scheduler.get_statistics().await;

        CertificateStatistics {
            total_certificates: store.count_certificates(),
            active_certificates: store.count_active_certificates(),
            revoked_certificates: store.count_revoked_certificates(),
            expiring_soon: store.count_expiring_soon(30),
            scheduled_rotations: rotation_stats.total_scheduled,
            last_rotation: None, // 履歴から最新を取得する場合は実装
        }
    }

    /// CSRを検証
    fn validate_csr(&self, request: &CertificateRequest) -> Result<()> {
        // Common Name必須チェック
        if request.common_name.is_empty() {
            return Err(crate::error::Error::InvalidInput(
                "Common name is required".to_string(),
            ));
        }

        // 有効期限チェック（推奨90日以内）
        if request.validity_days > 90 {
            return Err(crate::error::Error::InvalidInput(
                "Validity period exceeds 90 days (recommended maximum)".to_string(),
            ));
        }

        Ok(())
    }
}

// Default実装は削除 - 設定が必要なため new() を使用
