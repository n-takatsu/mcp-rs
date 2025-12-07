//! Real-time Threat Feed Tests
//!
//! リアルタイム脅威フィードの統合テスト

use mcp_rs::threat_intelligence::feed::*;
use mcp_rs::threat_intelligence::manager::ThreatIntelligenceManager;
use mcp_rs::threat_intelligence::types::*;
use chrono::Utc;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// 基本的なサブスクリプションテスト
#[tokio::test]
async fn test_basic_subscription() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = ThreatFeed::new(manager, config);

    // サブスクリプション作成
    let subscription_id = feed
        .subscribe("test-subscriber".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    // サブスクリプション確認
    let subscription = feed.get_subscription(subscription_id).await;
    assert!(subscription.is_some());
    assert_eq!(subscription.unwrap().subscriber_id, "test-subscriber");

    // サブスクリプション解除
    feed.unsubscribe(subscription_id)
        .await
        .expect("Failed to unsubscribe");

    // 解除確認
    let subscription = feed.get_subscription(subscription_id).await;
    assert!(subscription.is_none());
}

/// フィルター機能テスト
#[tokio::test]
async fn test_subscription_filters() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = ThreatFeed::new(manager, config);

    // カスタムフィルター作成
    let filters = ThreatFeedFilters {
        threat_types: Some(vec![ThreatType::Malware, ThreatType::Phishing]),
        min_severity: Some(SeverityLevel::High),
        indicator_types: Some(vec![IndicatorType::IpAddress, IndicatorType::Domain]),
        providers: Some(vec!["VirusTotal".to_string()]),
        min_confidence: Some(0.8),
        tags: Some(vec!["apt".to_string(), "ransomware".to_string()]),
    };

    // サブスクリプション作成
    let subscription_id = feed
        .subscribe("filtered-subscriber".to_string(), filters.clone())
        .await
        .expect("Failed to subscribe with filters");

    // フィルター確認
    let subscription = feed.get_subscription(subscription_id).await;
    assert!(subscription.is_some());

    let sub = subscription.unwrap();
    assert_eq!(sub.filters.min_severity, Some(SeverityLevel::High));
    assert_eq!(sub.filters.min_confidence, Some(0.8));
}

/// イベント発行テスト
#[tokio::test]
async fn test_event_publishing() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = Arc::new(ThreatFeed::new(manager, config));

    // イベントリスナー作成
    let mut event_receiver = feed.subscribe_to_events();

    // サブスクリプション作成
    let _subscription_id = feed
        .subscribe("test-subscriber".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    // 脅威イベント発行
    let threat = ThreatIntelligence {
        id: "test-threat-001".to_string(),
        threat_type: ThreatType::Malware,
        severity: SeverityLevel::Critical,
        indicators: vec![ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: "192.0.2.100".to_string(),
            pattern: None,
            tags: vec!["malware".to_string()],
            context: Some("Test malware indicator".to_string()),
            first_seen: Utc::now(),
        }],
        source: ThreatSource {
            provider: "VirusTotal".to_string(),
            feed_name: "Test Feed".to_string(),
            reliability: 0.95,
            last_updated: Utc::now(),
        },
        confidence_score: 0.98,
        first_seen: Utc::now(),
        last_seen: Utc::now(),
        expiration: None,
        metadata: ThreatMetadata::default(),
    };

    feed.publish_threat(threat.clone())
        .await
        .expect("Failed to publish threat");

    // イベント受信確認
    let event = tokio::time::timeout(Duration::from_secs(1), event_receiver.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Failed to receive event");

    assert_eq!(event.event_type, ThreatFeedEventType::NewThreat);

    if let ThreatFeedPayload::SingleThreat { threat: received_threat } = event.payload {
        assert_eq!(received_threat.id, "test-threat-001");
        assert_eq!(received_threat.severity, SeverityLevel::Critical);
    } else {
        panic!("Expected SingleThreat payload");
    }

    // 統計確認
    let stats = feed.get_stats().await;
    assert_eq!(stats.total_events, 1);
    assert_eq!(stats.threats_delivered, 1);
}

/// 複数サブスクリプションテスト
#[tokio::test]
async fn test_multiple_subscriptions() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = ThreatFeed::new(manager, config);

    // 複数のサブスクリプション作成
    let sub1 = feed
        .subscribe("subscriber-1".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    let sub2 = feed
        .subscribe("subscriber-2".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    let sub3 = feed
        .subscribe("subscriber-3".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    // サブスクリプションリスト確認
    let subscriptions = feed.list_subscriptions().await;
    assert_eq!(subscriptions.len(), 3);

    // 統計確認
    let stats = feed.get_stats().await;
    assert_eq!(stats.active_subscriptions, 3);

    // 一部解除
    feed.unsubscribe(sub2).await.expect("Failed to unsubscribe");

    let subscriptions = feed.list_subscriptions().await;
    assert_eq!(subscriptions.len(), 2);

    // 残りの確認
    assert!(feed.get_subscription(sub1).await.is_some());
    assert!(feed.get_subscription(sub2).await.is_none());
    assert!(feed.get_subscription(sub3).await.is_some());
}

/// バッチ更新テスト
#[tokio::test]
async fn test_batch_update() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = Arc::new(ThreatFeed::new(manager, config));

    // イベントリスナー作成
    let mut event_receiver = feed.subscribe_to_events();

    // 複数の脅威作成
    let threats: Vec<ThreatIntelligence> = (0..5)
        .map(|i| ThreatIntelligence {
            id: format!("threat-{}", i),
            threat_type: ThreatType::MaliciousIp,
            severity: SeverityLevel::Medium,
            indicators: vec![],
            source: ThreatSource {
                provider: "AbuseIPDB".to_string(),
                feed_name: "Batch Feed".to_string(),
                reliability: 0.9,
                last_updated: Utc::now(),
            },
            confidence_score: 0.85,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            expiration: None,
            metadata: ThreatMetadata::default(),
        })
        .collect();

    // バッチ発行
    feed.publish_threat_list(threats.clone())
        .await
        .expect("Failed to publish threat list");

    // イベント受信確認
    let event = tokio::time::timeout(Duration::from_secs(1), event_receiver.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Failed to receive event");

    assert_eq!(event.event_type, ThreatFeedEventType::BatchUpdate);

    if let ThreatFeedPayload::ThreatList { threats: received_threats, total_count } = event.payload {
        assert_eq!(total_count, 5);
        assert_eq!(received_threats.len(), 5);
    } else {
        panic!("Expected ThreatList payload");
    }

    // 統計確認
    let stats = feed.get_stats().await;
    assert_eq!(stats.total_events, 1);
    assert_eq!(stats.threats_delivered, 1);
}

/// 最大サブスクリプション数制限テスト
#[tokio::test]
async fn test_max_subscriptions_limit() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig {
        max_subscriptions: 3,
        ..Default::default()
    };
    let feed = ThreatFeed::new(manager, config);

    // 最大数までサブスクリプション作成
    for i in 0..3 {
        feed.subscribe(format!("subscriber-{}", i), ThreatFeedFilters::default())
            .await
            .expect("Failed to subscribe");
    }

    // 最大数を超える試み
    let result = feed
        .subscribe("overflow-subscriber".to_string(), ThreatFeedFilters::default())
        .await;

    assert!(result.is_err());
}

/// サブスクリプションフィルター更新テスト
#[tokio::test]
async fn test_update_subscription_filters() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = ThreatFeed::new(manager, config);

    // 初期サブスクリプション作成
    let subscription_id = feed
        .subscribe("test-subscriber".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    // 新しいフィルター作成
    let new_filters = ThreatFeedFilters {
        min_severity: Some(SeverityLevel::Critical),
        min_confidence: Some(0.95),
        ..Default::default()
    };

    // フィルター更新
    feed.update_filters(subscription_id, new_filters.clone())
        .await
        .expect("Failed to update filters");

    // 更新確認
    let subscription = feed.get_subscription(subscription_id).await.unwrap();
    assert_eq!(subscription.filters.min_severity, Some(SeverityLevel::Critical));
    assert_eq!(subscription.filters.min_confidence, Some(0.95));
}

/// サブスクリプション有効/無効切り替えテスト
#[tokio::test]
async fn test_toggle_subscription() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = ThreatFeed::new(manager, config);

    // サブスクリプション作成
    let subscription_id = feed
        .subscribe("test-subscriber".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    // 初期状態確認（アクティブ）
    let subscription = feed.get_subscription(subscription_id).await.unwrap();
    assert!(subscription.active);

    // 無効化
    feed.toggle_subscription(subscription_id, false)
        .await
        .expect("Failed to toggle subscription");

    let subscription = feed.get_subscription(subscription_id).await.unwrap();
    assert!(!subscription.active);

    // 再有効化
    feed.toggle_subscription(subscription_id, true)
        .await
        .expect("Failed to toggle subscription");

    let subscription = feed.get_subscription(subscription_id).await.unwrap();
    assert!(subscription.active);
}

/// 脅威評価イベント発行テスト
#[tokio::test]
async fn test_publish_assessment() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = Arc::new(ThreatFeed::new(manager, config));

    // イベントリスナー作成
    let mut event_receiver = feed.subscribe_to_events();

    // 評価結果作成
    let assessment = ThreatAssessment {
        indicator: ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: "198.51.100.50".to_string(),
            pattern: None,
            tags: vec![],
            context: None,
            first_seen: Utc::now(),
        },
        is_threat: true,
        threat_level: SeverityLevel::High,
        confidence_score: 0.92,
        matched_threats: vec![],
        assessed_at: Utc::now(),
        assessment_duration_ms: 150,
        context: Default::default(),
    };

    // 評価イベント発行
    feed.publish_assessment(assessment.clone())
        .await
        .expect("Failed to publish assessment");

    // イベント受信確認
    let event = tokio::time::timeout(Duration::from_secs(1), event_receiver.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Failed to receive event");

    if let ThreatFeedPayload::ThreatAssessment { assessment: received_assessment } = event.payload {
        assert_eq!(received_assessment.indicator.value, "198.51.100.50");
        assert_eq!(received_assessment.threat_level, SeverityLevel::High);
        assert!(received_assessment.is_threat);
    } else {
        panic!("Expected ThreatAssessment payload");
    }
}

/// クリーンアップテスト
#[tokio::test]
async fn test_cleanup_inactive_subscriptions() {
    let manager = Arc::new(ThreatIntelligenceManager::new());
    let config = ThreatFeedConfig::default();
    let feed = ThreatFeed::new(manager, config);

    // アクティブなサブスクリプション作成
    let active_sub = feed
        .subscribe("active-subscriber".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    // 非アクティブなサブスクリプション作成
    let inactive_sub = feed
        .subscribe("inactive-subscriber".to_string(), ThreatFeedFilters::default())
        .await
        .expect("Failed to subscribe");

    // 非アクティブ化
    feed.toggle_subscription(inactive_sub, false)
        .await
        .expect("Failed to toggle subscription");

    // 少し待機（last_updatedを古くするため）
    sleep(Duration::from_millis(100)).await;

    // クリーンアップ（0時間以上古いものを削除）
    let removed = feed.cleanup_inactive_subscriptions(0).await.expect("Failed to cleanup");

    // 非アクティブなサブスクリプションが削除されたことを確認
    assert_eq!(removed, 1);

    // アクティブなサブスクリプションは残っていることを確認
    assert!(feed.get_subscription(active_sub).await.is_some());
    assert!(feed.get_subscription(inactive_sub).await.is_none());
}
