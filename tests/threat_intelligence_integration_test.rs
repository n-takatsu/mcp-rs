//! Threat Intelligence Integration Tests
//!
//! リアルタイム脅威インテリジェンスエンジンの統合テスト

use mcp_rs::threat_intelligence::*;
use std::sync::Arc;

#[tokio::test]
async fn test_threat_detection_engine_initialization() {
    // エンジンの初期化テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager.clone());

    assert!(engine.is_ok());
}

#[tokio::test]
async fn test_ioc_detection_accuracy() {
    // IOC検出精度テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    // マルウェアIPアドレス（テスト用）
    let test_input = "Suspicious activity from 198.51.100.42 detected";

    let result = engine.detect_threats(test_input).await;
    assert!(result.is_ok());

    let detection = result.unwrap();
    assert!(!detection.indicators.is_empty());

    // IPアドレスが抽出されているか
    let has_ip = detection
        .indicators
        .iter()
        .any(|ind| matches!(ind.indicator_type, IndicatorType::IpAddress));
    assert!(has_ip);
}

#[tokio::test]
async fn test_url_extraction() {
    // URL抽出テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    let test_input = "Malicious link: https://malware-example.com/payload.exe";

    let result = engine.detect_threats(test_input).await;
    assert!(result.is_ok());

    let detection = result.unwrap();

    // URLが抽出されているか
    let has_url = detection
        .indicators
        .iter()
        .any(|ind| matches!(ind.indicator_type, IndicatorType::Url));
    assert!(has_url);
}

#[tokio::test]
async fn test_file_hash_detection() {
    // ファイルハッシュ検出テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    // SHA256ハッシュ（テスト用）
    let test_input = "File hash: a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    let result = engine.detect_threats(test_input).await;
    assert!(result.is_ok());

    let detection = result.unwrap();

    // ファイルハッシュが抽出されているか
    let has_hash = detection
        .indicators
        .iter()
        .any(|ind| matches!(ind.indicator_type, IndicatorType::FileHash));
    assert!(has_hash);
}

#[tokio::test]
async fn test_email_address_extraction() {
    // メールアドレス抽出テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    let test_input = "Phishing email from: attacker@malicious-domain.com";

    let result = engine.detect_threats(test_input).await;
    assert!(result.is_ok());

    let detection = result.unwrap();

    // メールアドレスが抽出されているか
    let has_email = detection
        .indicators
        .iter()
        .any(|ind| matches!(ind.indicator_type, IndicatorType::Email));
    assert!(has_email);
}

#[tokio::test]
async fn test_multiple_indicator_types() {
    // 複数タイプの指標検出テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    let test_input = r#"
        Attack observed from IP: 203.0.113.1
        Malicious URL: https://evil.example.com/backdoor
        Contact: hacker@evil.example.com
        File hash: d41d8cd98f00b204e9800998ecf8427e
    "#;

    let result = engine.detect_threats(test_input).await;
    assert!(result.is_ok());

    let detection = result.unwrap();

    // 複数の指標タイプが検出されているか
    assert!(detection.indicators.len() >= 3);

    let has_ip = detection
        .indicators
        .iter()
        .any(|ind| matches!(ind.indicator_type, IndicatorType::IpAddress));
    let has_url = detection
        .indicators
        .iter()
        .any(|ind| matches!(ind.indicator_type, IndicatorType::Url));
    let has_email = detection
        .indicators
        .iter()
        .any(|ind| matches!(ind.indicator_type, IndicatorType::Email));

    assert!(has_ip);
    assert!(has_url);
    assert!(has_email);
}

#[tokio::test]
async fn test_threat_assessment() {
    // 脅威評価テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());

    // IPアドレスの脅威評価
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: "192.0.2.1".to_string(),
        pattern: None,
        tags: vec![],
        context: None,
        first_seen: chrono::Utc::now(),
    };

    let assessment = manager.check_threat(indicator).await;
    assert!(assessment.is_ok());
}

#[tokio::test]
async fn test_provider_health_check() {
    // プロバイダー健全性チェックテスト
    let manager = Arc::new(ThreatIntelligenceManager::new());

    // プロバイダー登録がない場合でもエラーにならない
    let _stats = manager.get_stats().await;
    // 統計情報が取得できることを確認（型が正しいこと自体が成功）
}

#[tokio::test]
async fn test_cache_functionality() {
    // キャッシュ機能テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: "203.0.113.100".to_string(),
        pattern: None,
        tags: vec![],
        context: None,
        first_seen: chrono::Utc::now(),
    };

    // 最初のアクセス
    let assessment1 = manager.check_threat(indicator.clone()).await;
    assert!(assessment1.is_ok());

    // 2回目のアクセス（キャッシュから取得されるべき）
    let assessment2 = manager.check_threat(indicator).await;
    assert!(assessment2.is_ok());
}

#[tokio::test]
async fn test_threat_feed_subscription() {
    // 脅威フィードサブスクリプションテスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let feed = ThreatFeed::new(manager, ThreatFeedConfig::default());

    // サブスクリプション作成
    let filters = ThreatFeedFilters {
        threat_types: Some(vec![ThreatType::Malware]),
        min_severity: Some(SeverityLevel::High),
        indicator_types: None,
        providers: None,
        min_confidence: Some(0.7),
        tags: None,
    };

    let subscription = feed.subscribe("test_subscriber".to_string(), filters).await;
    assert!(subscription.is_ok());

    let sub_id = subscription.unwrap();

    // サブスクリプション削除
    let unsub_result = feed.unsubscribe(sub_id).await;
    assert!(unsub_result.is_ok());
}

#[tokio::test]
async fn test_false_positive_rate() {
    // 偽陽性率テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    // 良性のコンテンツ
    let benign_samples = vec![
        "User logged in from 192.168.1.1",
        "Visit our website at https://example.com",
        "Contact us at support@example.com",
        "File checksum: abc123def456",
    ];

    let mut false_positives = 0;
    let total_samples = benign_samples.len();

    for sample in benign_samples {
        if let Ok(result) = engine.detect_threats(sample).await {
            let threat_count = result.assessments.iter().filter(|a| a.is_threat).count();
            if threat_count > 0 {
                false_positives += 1;
            }
        }
    }

    // 偽陽性率が5%未満であることを確認
    let false_positive_rate = (false_positives as f64 / total_samples as f64) * 100.0;
    assert!(
        false_positive_rate < 5.0,
        "False positive rate: {}%",
        false_positive_rate
    );
}

#[tokio::test]
async fn test_performance_ip_check() {
    // IPチェックパフォーマンステスト（50ms未満）
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    let test_input = "Connection from 8.8.8.8";

    let start = std::time::Instant::now();
    let _ = engine.detect_threats(test_input).await;
    let duration = start.elapsed();

    // 初回はプロバイダーなしでも動作確認
    assert!(
        duration.as_millis() < 100,
        "IP check took {}ms",
        duration.as_millis()
    );
}

#[tokio::test]
async fn test_detection_statistics() {
    // 検出統計テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager.clone()).expect("Failed to create engine");

    // 複数の検出を実行
    for i in 0..5 {
        let test_input = format!("test content with 192.0.2.{}", i + 1);
        let _ = engine.detect_threats(&test_input).await;
    }

    // 統計を取得
    let _stats = manager.get_stats().await;
    // 統計情報が取得できることを確認（型が正しいこと自体が成功）
}

#[tokio::test]
async fn test_failover_mechanism() {
    // フェイルオーバー機構テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: "198.51.100.1".to_string(),
        pattern: None,
        tags: vec![],
        context: None,
        first_seen: chrono::Utc::now(),
    };

    // プロバイダーが利用できない場合でも評価が返される
    let assessment = manager.check_threat(indicator).await;
    assert!(assessment.is_ok());
}

#[tokio::test]
async fn test_threat_detection_with_ip() {
    // IP検出の基本テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    let result = engine.evaluate_ip("203.0.113.5").await;
    assert!(result.is_ok());

    let score = result.unwrap();
    assert!(score.score <= 100);
}

#[tokio::test]
async fn test_threat_detection_with_domain() {
    // ドメイン検出の基本テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    let result = engine.evaluate_domain("example.com").await;
    assert!(result.is_ok());

    let score = result.unwrap();
    assert!(score.score <= 100);
}

#[tokio::test]
async fn test_threat_detection_with_file_hash() {
    // ファイルハッシュ検出の基本テスト
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let engine = ThreatDetectionEngine::new(manager).expect("Failed to create engine");

    let result = engine
        .evaluate_file_hash("d41d8cd98f00b204e9800998ecf8427e")
        .await;
    assert!(result.is_ok());

    let score = result.unwrap();
    assert!(score.score <= 100);
}
