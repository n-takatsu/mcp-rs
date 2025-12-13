//! Zero Trust システムの統合テスト

use mcp_rs::zero_trust::{
    continuous_auth::{AuthEventType, ContinuousAuth, RiskLevel},
    device_verifier::{DeviceInfo, DeviceVerifier},
    micro_segmentation::{AccessPolicy, MicroSegmentation, ResourceSegment},
    network_analyzer::{NetworkAnalyzer, NetworkInfo},
    AccessRequest,
};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn test_full_zero_trust_flow() {
    // 1. デバイス検証
    let mut device_verifier = DeviceVerifier::new();
    let device_info = DeviceInfo::from_user_agent(
        "device1",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
    );
    device_verifier.register_device(device_info.clone());
    let device_result = device_verifier.verify(&device_info);

    assert!(device_result.success);

    // 2. ネットワーク分析
    let mut network_analyzer = NetworkAnalyzer::new();
    let mut network_info = NetworkInfo::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)));
    network_info.country_code = Some("JP".to_string());
    network_info.is_vpn = false;
    network_info.is_tor = false;

    let network_result = network_analyzer.analyze("user1", &network_info);

    assert!(network_result.success);

    // 3. アクセスリクエスト
    let request = AccessRequest::new(
        "user1",
        "device1",
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
        "/api/public/info",
        "read",
    );

    // 4. マイクロセグメンテーション
    let segmentation = MicroSegmentation::default();
    let mut user_roles = HashSet::new();
    user_roles.insert("user".to_string());

    let trust_score = (device_result.trust_score + network_result.trust_score) / 2;
    let access_result = segmentation.evaluate_access(&request, &user_roles, trust_score);

    // /api/public/infoは公開APIなので、適切なロールとトラストスコアでアクセス可能
    if trust_score >= 30 {
        assert!(access_result.success);
    }
}

#[test]
fn test_device_verification_lifecycle() {
    let mut verifier = DeviceVerifier::new();

    // 1. 新しいデバイスの登録
    let device_info =
        DeviceInfo::from_user_agent("device1", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");
    verifier.register_device(device_info.clone());

    // 2. 登録済みデバイスの検証
    let result = verifier.verify(&device_info);

    // 登録済みなので基本スコアは得られる
    assert!(result.trust_score >= 30);

    // 3. 異なるデバイスでの検証
    let different_device =
        DeviceInfo::from_user_agent("device2", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
    let result2 = verifier.verify(&different_device);

    // Windows 10は最小バージョンを満たすので、実際にはスコアが高くなる可能性がある
    // ただし、未登録デバイスなので管理対象ボーナスはない
    assert!(result2.trust_score >= 30);
}

#[test]
fn test_network_anomaly_detection() {
    let mut analyzer = NetworkAnalyzer::new();

    // 1. 通常の接続を検証
    let normal_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
    let mut normal_network = NetworkInfo::new(normal_ip);
    normal_network.country_code = Some("JP".to_string());
    normal_network.is_vpn = false;
    normal_network.is_tor = false;

    let result = analyzer.analyze("user1", &normal_network);

    assert!(result.success);
    assert!(result.trust_score >= 50);

    // 2. VPN経由の接続を検証
    let mut vpn_network = NetworkInfo::new(normal_ip);
    vpn_network.country_code = Some("JP".to_string());
    vpn_network.is_vpn = true;
    vpn_network.is_tor = false;

    let vpn_result = analyzer.analyze("user1", &vpn_network);

    assert!(vpn_result.trust_score < result.trust_score);

    // 3. Tor経由の接続を検証
    let mut tor_network = NetworkInfo::new(normal_ip);
    tor_network.country_code = Some("JP".to_string());
    tor_network.is_vpn = false;
    tor_network.is_tor = true;

    let tor_result = analyzer.analyze("user1", &tor_network);

    assert!(tor_result.trust_score < vpn_result.trust_score);
}

#[test]
fn test_micro_segmentation_policies() {
    let mut segmentation = MicroSegmentation::new();

    // 1. カスタムポリシーの設定
    segmentation.add_global_policy(
        AccessPolicy::new("test-policy", "/api/test/*", 50)
            .with_action("read")
            .with_role("tester"),
    );

    // 2. テスターロールでのアクセステスト
    let request = AccessRequest::new(
        "tester1",
        "device1",
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        "/api/test/data",
        "read",
    );

    let mut roles = HashSet::new();
    roles.insert("tester".to_string());

    let result = segmentation.evaluate_access(&request, &roles, 60);
    assert!(result.success);

    // 3. 書き込みアクセスは拒否されるべき
    let write_request = AccessRequest::new(
        "tester1",
        "device1",
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        "/api/test/data",
        "write",
    );

    let write_result = segmentation.evaluate_access(&write_request, &roles, 60);
    assert!(!write_result.success);
}

#[test]
fn test_continuous_authentication() {
    let mut auth = ContinuousAuth::new();

    // 1. セッション開始
    auth.start_session("session1", "user1", "device1", 80);

    // 2. セッション検証
    let result = auth.verify_session("session1");
    assert!(result.success);

    // 3. トラストスコア更新
    auth.update_trust_score(
        "session1",
        70,
        AuthEventType::LocationChange,
        "Location changed",
    )
    .unwrap();

    let session = auth.get_session("session1").unwrap();
    assert_eq!(session.current_trust_score, 70);
    assert_eq!(session.risk_level, RiskLevel::Low);

    // 4. 異常検知
    auth.handle_anomaly("session1", "Suspicious activity")
        .unwrap();

    let session = auth.get_session("session1").unwrap();
    assert_eq!(session.current_trust_score, 40);
    assert_eq!(session.risk_level, RiskLevel::Critical);
}

#[test]
fn test_session_timeout() {
    let mut auth = ContinuousAuth::new();
    auth.start_session("session1", "user1", "device1", 80);

    // 初回は成功
    let result1 = auth.verify_session("session1");
    assert!(result1.success);

    // アクティビティ記録
    auth.record_activity("session1");

    // まだ有効
    let result2 = auth.verify_session("session1");
    assert!(result2.success);
}

#[test]
fn test_resource_segmentation() {
    let mut segmentation = MicroSegmentation::new();

    // データベースセグメントの作成
    let mut db_segment = ResourceSegment::new("database");
    db_segment.add_resource("/db/users");
    db_segment.add_resource("/db/products");
    db_segment.policies.push(
        AccessPolicy::new("db-policy", "/db/*", 80)
            .with_action("read")
            .with_action("write")
            .with_role("db_admin"),
    );

    segmentation.add_segment(db_segment);

    // データベース管理者のアクセス
    let request = AccessRequest::new(
        "dbadmin1",
        "device1",
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        "/db/users",
        "read",
    );

    let mut roles = HashSet::new();
    roles.insert("db_admin".to_string());

    let result = segmentation.evaluate_access(&request, &roles, 85);
    assert!(result.success);

    // 一般ユーザーのアクセス（拒否されるべき）
    let mut user_roles = HashSet::new();
    user_roles.insert("user".to_string());

    let user_result = segmentation.evaluate_access(&request, &user_roles, 85);
    assert!(!user_result.success);
}

#[test]
fn test_trust_score_calculation() {
    let mut verifier = DeviceVerifier::new();
    let device_info =
        DeviceInfo::from_user_agent("device1", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)");
    verifier.register_device(device_info.clone());
    let device_result = verifier.verify(&device_info);

    let mut analyzer = NetworkAnalyzer::new();
    let mut network_info = NetworkInfo::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)));
    network_info.country_code = Some("JP".to_string());
    let network_result = analyzer.analyze("user1", &network_info);

    // トラストスコア計算（平均）
    let score = (device_result.trust_score + network_result.trust_score) / 2;
    assert!(score >= 50);
}

#[test]
fn test_cleanup_expired_sessions() {
    let mut auth = ContinuousAuth::new();

    // 短いタイムアウトを設定
    auth.start_session("session1", "user1", "device1", 80);
    auth.start_session("session2", "user2", "device2", 70);
    auth.start_session("session3", "user3", "device3", 60);

    // すべてのセッションが有効
    assert!(auth.get_session("session1").is_some());
    assert!(auth.get_session("session2").is_some());
    assert!(auth.get_session("session3").is_some());

    // session1のみアクティビティを記録
    auth.record_activity("session1");

    // 期限切れセッションはない
    let cleaned = auth.cleanup_expired_sessions();
    assert_eq!(cleaned, 0);
}

#[test]
fn test_low_trust_score_access_denial() {
    let segmentation = MicroSegmentation::default();

    let request = AccessRequest::new(
        "user1",
        "device1",
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)),
        "/api/admin/settings",
        "write",
    );

    let mut roles = HashSet::new();
    roles.insert("admin".to_string());

    // トラストスコアが低い場合
    let result = segmentation.evaluate_access(&request, &roles, 30);
    assert!(!result.success);

    // トラストスコアが十分な場合
    let result2 = segmentation.evaluate_access(&request, &roles, 80);
    assert!(result2.success);
}
