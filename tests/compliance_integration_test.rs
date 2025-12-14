//! GDPR/CCPA Compliance Integration Tests

use mcp_rs::compliance::*;

#[tokio::test]
async fn test_compliance_engine_initialization() {
    let engine = ComplianceEngine::new();

    // エンジンが正常に初期化されることを確認
    let report = engine
        .generate_report(
            chrono::Utc::now() - chrono::Duration::days(30),
            chrono::Utc::now(),
        )
        .await;

    assert_eq!(report.total_requests, 0);
}

#[tokio::test]
async fn test_erasure_request() {
    let engine = ComplianceEngine::new();

    let request = DataSubjectRequest::new("user@example.com", RequestType::Erasure);

    let result = engine.process_request(request).await;

    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(result.status, RequestStatus::Completed);
    assert!(result.certificate.is_some());
}

#[tokio::test]
async fn test_access_request() {
    let engine = ComplianceEngine::new();

    let request = DataSubjectRequest::new("user@example.com", RequestType::Access);

    let result = engine.process_request(request).await;

    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(result.status, RequestStatus::Completed);
    assert!(result.data.is_some());
}

#[tokio::test]
async fn test_portability_request() {
    let engine = ComplianceEngine::new();

    let request = DataSubjectRequest::new("user@example.com", RequestType::Portability);

    let result = engine.process_request(request).await;

    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(result.status, RequestStatus::Completed);
    assert!(result.data.is_some());
}

#[tokio::test]
async fn test_consent_management() {
    let manager = consent_manager::ConsentManager::new();

    // 同意を付与
    let consent = ConsentRecord::new(
        "user@example.com",
        "marketing",
        vec!["email".to_string(), "newsletter".to_string()],
        LegalBasis::Consent,
    );

    let consent_id = manager.grant_consent(consent).await;
    assert!(consent_id.is_ok());

    // 同意を確認
    let has_consent = manager.check_consent("user@example.com", "marketing").await;
    assert!(has_consent.is_ok());
    assert!(has_consent.unwrap());

    // 同意を撤回
    let revoke_result = manager
        .revoke_consent("user@example.com", "marketing")
        .await;
    assert!(revoke_result.is_ok());

    // 撤回後の確認
    let has_consent_after = manager.check_consent("user@example.com", "marketing").await;
    assert!(has_consent_after.is_ok());
    assert!(!has_consent_after.unwrap());
}

#[tokio::test]
async fn test_consent_history() {
    let manager = consent_manager::ConsentManager::new();

    // 複数の同意を記録
    for i in 0..3 {
        let consent = ConsentRecord::new(
            "user@example.com",
            "marketing",
            vec![format!("scope_{}", i)],
            LegalBasis::Consent,
        );
        manager.grant_consent(consent).await.unwrap();
    }

    // 履歴を取得
    let history = manager
        .get_consent_history("user@example.com", "marketing")
        .await;
    assert!(history.is_ok());
    assert_eq!(history.unwrap().len(), 3);
}

#[tokio::test]
async fn test_lifecycle_management() {
    let manager = lifecycle_manager::LifecycleManager::new();

    // データを登録
    let entry_id = manager
        .register_data(
            "user@example.com".to_string(),
            DataCategory::ContactInformation,
        )
        .await;

    assert!(entry_id.is_ok());

    // 統計を確認
    let stats = manager.get_statistics().await;
    assert_eq!(stats.total_entries, 1);
    assert_eq!(stats.active_entries, 1);
}

#[tokio::test]
async fn test_retention_policy() {
    let manager = lifecycle_manager::LifecycleManager::new();

    // ポリシーを取得
    let policy = manager
        .get_retention_policy(&DataCategory::PersonalIdentifiable)
        .await;

    assert!(policy.is_ok());
    let policy = policy.unwrap();
    assert_eq!(policy.retention_days, 2557); // 7 years
    assert_eq!(policy.deletion_method, DeletionMethod::HardDelete);
}

#[tokio::test]
async fn test_data_subject_request_handler() {
    let handler = data_subject_requests::DataSubjectRequestHandler::new();

    let request = DataSubjectRequest::new("user@example.com", RequestType::Erasure);

    let request_id = handler.submit_request(request).await;
    assert!(request_id.is_ok());

    let request_id = request_id.unwrap();

    // リクエストを取得
    let retrieved = handler.get_request(&request_id).await;
    assert!(retrieved.is_ok());

    // ステータスを更新
    let update_result = handler
        .update_status(&request_id, RequestStatus::Processing)
        .await;
    assert!(update_result.is_ok());
}

#[tokio::test]
async fn test_request_deadline_extension() {
    let handler = data_subject_requests::DataSubjectRequestHandler::new();

    let request = DataSubjectRequest::new("user@example.com", RequestType::Access);

    let original_deadline = request.deadline;
    let request_id = handler.submit_request(request).await.unwrap();

    // 期限を延長
    let new_deadline = handler.extend_deadline(&request_id, 15).await;

    assert!(new_deadline.is_ok());
    let new_deadline = new_deadline.unwrap();

    assert!(new_deadline > original_deadline);
}

#[tokio::test]
async fn test_overdue_requests() {
    let handler = data_subject_requests::DataSubjectRequestHandler::new();

    // 過去の期限を持つリクエストを作成
    let mut request = DataSubjectRequest::new("user@example.com", RequestType::Erasure);
    request.deadline = chrono::Utc::now() - chrono::Duration::days(1);

    handler.submit_request(request).await.unwrap();

    // 期限切れリクエストを取得
    let overdue = handler.get_overdue_requests().await;

    assert_eq!(overdue.len(), 1);
}

#[tokio::test]
async fn test_compliance_report() {
    let engine = ComplianceEngine::new();

    // 複数のリクエストを処理
    for i in 0..5 {
        let request = DataSubjectRequest::new(
            format!("user{}@example.com", i),
            if i % 2 == 0 {
                RequestType::Erasure
            } else {
                RequestType::Access
            },
        );
        engine.process_request(request).await.unwrap();
    }

    // レポートを生成
    let report = engine
        .generate_report(
            chrono::Utc::now() - chrono::Duration::days(1),
            chrono::Utc::now() + chrono::Duration::days(1),
        )
        .await;

    assert_eq!(report.total_requests, 5);
    assert!(report.audit_entries > 0);
}

#[tokio::test]
async fn test_consent_statistics() {
    let manager = consent_manager::ConsentManager::new();

    // 複数の同意を記録
    for i in 0..10 {
        let consent = ConsentRecord::new(
            format!("user{}@example.com", i),
            "marketing",
            vec!["email".to_string()],
            LegalBasis::Consent,
        );
        manager.grant_consent(consent).await.unwrap();
    }

    // 統計を取得
    let stats = manager.get_consent_statistics().await;

    assert_eq!(stats.total_subjects, 10);
    assert_eq!(stats.total_consents, 10);
    assert_eq!(stats.active_consents, 10);
}

#[tokio::test]
async fn test_request_statistics() {
    let handler = data_subject_requests::DataSubjectRequestHandler::new();

    // 複数のリクエストを提出
    for i in 0..15 {
        let request_type = match i % 3 {
            0 => RequestType::Erasure,
            1 => RequestType::Access,
            _ => RequestType::Portability,
        };

        let request = DataSubjectRequest::new(format!("user{}@example.com", i), request_type);

        handler.submit_request(request).await.unwrap();
    }

    // 統計を取得
    let stats = handler.get_statistics().await;

    assert_eq!(stats.total_requests, 15);
    assert_eq!(stats.requests_by_type.len(), 3);
}

#[tokio::test]
async fn test_lifecycle_auto_deletion() {
    let manager = lifecycle_manager::LifecycleManager::new();

    // 短い保持期間のポリシーを追加
    let policy = RetentionPolicy {
        id: uuid::Uuid::new_v4().to_string(),
        data_category: DataCategory::Behavioral,
        retention_days: 0, // 即座に削除
        reason: "Test".to_string(),
        legal_basis: LegalBasis::Consent,
        deletion_method: DeletionMethod::HardDelete,
    };

    manager.add_retention_policy(policy).await.unwrap();

    // データを登録
    manager
        .register_data("user@example.com".to_string(), DataCategory::Behavioral)
        .await
        .unwrap();

    // 自動削除ジョブを実行
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let deleted_count = manager.run_auto_deletion_job().await.unwrap();

    assert_eq!(deleted_count, 1);
}

#[tokio::test]
async fn test_data_category_types() {
    // 各データカテゴリが正しく定義されていることを確認
    let categories = [
        DataCategory::PersonalIdentifiable,
        DataCategory::ContactInformation,
        DataCategory::Financial,
        DataCategory::Health,
        DataCategory::Location,
        DataCategory::OnlineIdentifiers,
        DataCategory::Behavioral,
        DataCategory::Sensitive,
    ];

    assert_eq!(categories.len(), 8);
}

#[tokio::test]
async fn test_legal_basis_types() {
    // 各法的根拠が正しく定義されていることを確認
    let bases = [
        LegalBasis::Consent,
        LegalBasis::Contract,
        LegalBasis::LegalObligation,
        LegalBasis::VitalInterests,
        LegalBasis::PublicInterest,
        LegalBasis::LegitimateInterests,
    ];

    assert_eq!(bases.len(), 6);
}

#[tokio::test]
async fn test_request_metadata() {
    let mut request = DataSubjectRequest::new("user@example.com", RequestType::Erasure);

    // メタデータを追加
    request
        .metadata
        .insert("ip_address".to_string(), "192.168.1.1".to_string());
    request
        .metadata
        .insert("user_agent".to_string(), "Mozilla/5.0".to_string());

    assert_eq!(request.metadata.len(), 2);
}
